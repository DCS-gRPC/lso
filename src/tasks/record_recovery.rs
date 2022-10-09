use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Cursor;
use std::time::{Duration, Instant};

use futures_util::future::Either;
use futures_util::stream::select;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use serenity::http::Http;
use serenity::model::channel::Embed;
use serenity::model::id::UserId;
use serenity::model::mention::Mention;
use stubs::common::v0::{initiator, Airbase, Coalition, Initiator};
use stubs::mission::v0::stream_events_response::{Event, LandEvent, LandingQualityMarkEvent};
use tacview::record::{self, Color, Coords, GlobalProperty, Property, Record, Tag, Update};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tonic::Status;

use crate::client::{HookClient, MissionClient, UnitClient};
use crate::track::{Grading, Track};
use crate::transform::Transform;

use super::TaskParams;

pub static FILENAME_DATETIME_FORMAT: Lazy<Vec<time::format_description::FormatItem<'_>>> =
    Lazy::new(|| {
        time::format_description::parse("[year][month][day]-[hour][minute][second]").unwrap()
    });

#[tracing::instrument(
        skip_all,
        fields(carrier_name = params.carrier_name, plane_name = params.plane_name)
    )]
pub async fn record_recovery(params: TaskParams<'_>) -> Result<(), crate::error::Error> {
    tracing::debug!("started recording");

    // Tacview-20211111-143727-DCS-grpc-lso.zip
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let filename = format!(
        "LSO-{}-{}",
        now.format(&FILENAME_DATETIME_FORMAT).unwrap_or_default(),
        params
            .pilot_name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
    );

    let mut client1 = UnitClient::new(params.ch.clone());
    let mut client2 = UnitClient::new(params.ch.clone());
    let mut mission = MissionClient::new(params.ch.clone());
    let mut hook = HookClient::new(params.ch.clone());
    let interval = crate::utils::interval::interval(Duration::from_millis(100), params.shutdown);

    let mut acmi = Cursor::new(Vec::new());
    let mut recording = tacview::Writer::new_compressed(&mut acmi)?;
    let mut datums = Track::new(params.pilot_name, params.carrier_info, params.plane_info);

    let reference_time = mission.get_scenario_start_time().await?;
    recording.write(GlobalProperty::ReferenceTime(reference_time))?;
    recording.write(GlobalProperty::RecordingTime(
        OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
    ))?;

    let mission_name = hook.get_mission_name().await?;
    recording.write(GlobalProperty::Title(format!(
        "Carrier Recovery during {}",
        mission_name
    )))?;
    recording.write(GlobalProperty::Author(format!(
        "dcs-grpc-lso v{}",
        env!("CARGO_PKG_VERSION")
    )))?;
    let mut ref_written = false;
    let mut lat_ref = 0.0;
    let mut lon_ref = 0.0;

    recording.write(create_initial_update(&mut client1, 1, params.carrier_name).await?)?;
    recording.write(create_initial_update(&mut client1, 2, params.plane_name).await?)?;

    let events = mission.stream_events().await?;

    let mut known_carrier_coords = None;
    let mut known_plane_coords = None;
    let mut track_stopped: Option<Instant> = None;

    let mut stream = select(interval.map(Either::Left), events.map(Either::Right));

    while let Some(next) = stream.next().await {
        match next {
            // next interval
            Either::Left(_) => {
                let (carrier, plane) = futures_util::future::try_join(
                    client1.get_transform(params.carrier_name),
                    client2.get_transform(params.plane_name),
                )
                .await?;

                if !ref_written {
                    lat_ref = carrier.lat;
                    lon_ref = carrier.lon;
                    recording.write(GlobalProperty::ReferenceLatitude(lat_ref))?;
                    recording.write(GlobalProperty::ReferenceLongitude(lon_ref))?;
                    ref_written = true;
                }

                let carrier_update = Update {
                    id: 1,
                    props: vec![Property::T(remove_unchanged(
                        Coords::default()
                            .position(carrier.lat - lat_ref, carrier.lon - lon_ref, carrier.alt)
                            .uv(carrier.position.x, carrier.position.z)
                            .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                            .heading(carrier.heading),
                        &mut known_carrier_coords,
                    ))],
                };
                let plane_update = Update {
                    id: 2,
                    props: vec![
                        Property::T(remove_unchanged(
                            Coords::default()
                                .position(plane.lat - lat_ref, plane.lon - lon_ref, plane.alt)
                                .uv(plane.position.x, plane.position.z)
                                .orientation(plane.yaw, plane.pitch, plane.roll)
                                .heading(plane.heading),
                            &mut known_plane_coords,
                        )),
                        Property::AOA(plane.aoa),
                    ],
                };

                if (carrier.time - plane.time).abs() < 0.01 {
                    recording.write(Record::Frame(carrier.time))?;
                    recording.write(carrier_update)?;
                    recording.write(plane_update)?;
                } else if carrier.time < plane.time {
                    recording.write(Record::Frame(carrier.time))?;
                    recording.write(carrier_update)?;
                    recording.write(Record::Frame(plane.time))?;
                    recording.write(plane_update)?;
                } else {
                    recording.write(Record::Frame(plane.time))?;
                    recording.write(plane_update)?;
                    recording.write(Record::Frame(carrier.time))?;
                    recording.write(carrier_update)?;
                }

                if !datums.next(&carrier, &plane) {
                    break;
                }

                if let Some(track_stopped) = track_stopped {
                    if track_stopped.elapsed() > Duration::from_secs(10) {
                        break;
                    }
                }
            }

            // next event
            Either::Right(event) => match event? {
                (
                    time,
                    Event::LandingQualityMark(LandingQualityMarkEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(plane)),
                            }),
                        place:
                            Some(Airbase {
                                unit: Some(carrier),
                                ..
                            }),
                        comment,
                    }),
                ) if plane.name == params.plane_name && carrier.name == params.carrier_name => {
                    tracing::info!(%comment, "landing quality mark event");
                    datums.set_dcs_grading(comment.clone());
                    recording.write(Record::Frame(time))?;

                    let carrier = Transform::from((
                        time,
                        carrier.position.unwrap_or_default(),
                        carrier.orientation.unwrap_or_default(),
                        carrier.velocity.unwrap_or_default(),
                    ));
                    recording.write(Update {
                        id: 1,
                        props: vec![Property::T(remove_unchanged(
                            Coords::default()
                                .position(carrier.lat - lat_ref, carrier.lon - lon_ref, carrier.alt)
                                .uv(carrier.position.x, carrier.position.z)
                                .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                                .heading(carrier.heading),
                            &mut known_carrier_coords,
                        ))],
                    })?;

                    let plane = Transform::from((
                        time,
                        plane.position.unwrap_or_default(),
                        plane.orientation.unwrap_or_default(),
                        plane.velocity.unwrap_or_default(),
                    ));
                    recording.write(Update {
                        id: 2,
                        props: vec![
                            Property::T(remove_unchanged(
                                Coords::default()
                                    .position(plane.lat - lat_ref, plane.lon - lon_ref, plane.alt)
                                    .uv(plane.position.x, plane.position.z)
                                    .orientation(plane.yaw, plane.pitch, plane.roll)
                                    .heading(plane.heading),
                                &mut known_plane_coords,
                            )),
                            Property::AOA(plane.aoa),
                        ],
                    })?;

                    recording.write(record::Event {
                        kind: record::EventKind::Message,
                        params: vec!["2".to_string(), "1".to_string()],
                        text: Some(comment),
                    })?;
                }

                (
                    time,
                    Event::Land(LandEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(plane)),
                            }),
                        place:
                            Some(Airbase {
                                unit: Some(carrier),
                                ..
                            }),
                    }),
                ) if plane.name == params.plane_name && carrier.name == params.carrier_name => {
                    tracing::info!("land event");
                    recording.write(Record::Frame(time))?;

                    let carrier = Transform::from((
                        time,
                        carrier.position.unwrap_or_default(),
                        carrier.orientation.unwrap_or_default(),
                        carrier.velocity.unwrap_or_default(),
                    ));
                    recording.write(Update {
                        id: 1,
                        props: vec![Property::T(remove_unchanged(
                            Coords::default()
                                .position(carrier.lat - lat_ref, carrier.lon - lon_ref, carrier.alt)
                                .uv(carrier.position.x, carrier.position.z)
                                .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                                .heading(carrier.heading),
                            &mut known_carrier_coords,
                        ))],
                    })?;

                    let plane = Transform::from((
                        time,
                        plane.position.unwrap_or_default(),
                        plane.orientation.unwrap_or_default(),
                        plane.velocity.unwrap_or_default(),
                    ));
                    recording.write(Update {
                        id: 2,
                        props: vec![
                            Property::T(remove_unchanged(
                                Coords::default()
                                    .position(plane.lat - lat_ref, plane.lon - lon_ref, plane.alt)
                                    .uv(plane.position.x, plane.position.z)
                                    .orientation(plane.yaw, plane.pitch, plane.roll)
                                    .heading(plane.heading),
                                &mut known_plane_coords,
                            )),
                            Property::AOA(plane.aoa),
                        ],
                    })?;

                    recording.write(record::Event {
                        kind: record::EventKind::Landed,
                        params: vec!["2".to_string(), "1".to_string()],
                        text: None,
                    })?;

                    datums.next(&carrier, &plane);
                    datums.landed(&carrier, &plane);

                    // don't stop right away, track a couple of more seconds
                    track_stopped = Some(Instant::now());
                }

                _ => {}
            },
        }
    }

    recording.into_inner();
    let data = acmi.into_inner();
    let acmi_path = params.out_dir.join(&filename).with_extension("zip.acmi");
    tokio::fs::write(&acmi_path, &data).await?;
    let track = datums.finish();
    let chart_path = crate::draw::draw_chart(params.out_dir, &filename, &track)?;

    if let Some(discord_webhook) = params.discord_webhook.as_deref() {
        let http = Http::new("token");
        let webhook = http.get_webhook_from_url(discord_webhook).await?;

        let embed = Embed::fake(|e| {
            let e = e
                .field(
                    "Pilot",
                    params
                        .users
                        .get(params.pilot_name)
                        .map(|id| Cow::Owned(Mention::from(UserId(*id)).to_string()))
                        .unwrap_or(Cow::Borrowed(params.pilot_name)),
                    true,
                )
                .field(
                    "Grading",
                    match track.grading {
                        Grading::Unknown => Cow::Borrowed("unknown"),
                        Grading::Bolter => Cow::Borrowed("Bolter"),
                        Grading::Recovered { cable, .. } => cable
                            .map(|c| Cow::Owned(format!("#{}", c)))
                            .unwrap_or(Cow::Borrowed("-")),
                    },
                    true,
                );
            if let Some(dcs_grading) = track.dcs_grading {
                e.field("DCS LSO", dcs_grading, true)
            } else {
                e
            }
        });

        webhook
            .execute(&http, false, |w| {
                w.embeds(vec![embed])
                    .add_file(&chart_path)
                    .add_file(&acmi_path)
            })
            .await?;
    }

    Ok(())
}

async fn create_initial_update(
    client: &mut UnitClient,
    id: u64,
    unit_name: &str,
) -> Result<Update, Status> {
    let unit = client.get_unit(unit_name).await?;
    let attrs = client.get_descriptor(unit_name).await?;

    let coalition = Coalition::from_i32(unit.coalition).unwrap_or(Coalition::Neutral);
    let mut props = vec![
        Property::Type(tags(&attrs)),
        Property::Name(unit.r#type),
        Property::Group(unit.group.unwrap_or_default().name),
        Property::Color(color(coalition)),
    ];
    if let Some(player_name) = &unit.player_name {
        props.push(Property::Pilot(player_name.to_string()))
    }

    Ok(Update { id, props })
}

fn tags<I: AsRef<str>>(attrs: impl IntoIterator<Item = I>) -> HashSet<Tag> {
    let mut tags = HashSet::with_capacity(2);
    for attr in attrs.into_iter() {
        match attr.as_ref() {
            "Ships" => {
                tags.insert(Tag::Sea);
                tags.insert(Tag::Watercraft);
            }
            "AircraftCarrier" => {
                tags.insert(Tag::AircraftCarrier);
            }
            "Air" => {
                tags.insert(Tag::Air);
            }
            "Planes" => {
                tags.insert(Tag::FixedWing);
            }
            _ => {}
        }
    }
    tags
}

fn color(coalition: Coalition) -> Color {
    match coalition {
        Coalition::All | Coalition::Neutral => Color::Grey,
        Coalition::Red => Color::Red,
        Coalition::Blue => Color::Blue,
    }
}

fn remove_unchanged(mut coords: Coords, known: &mut Option<Coords>) -> Coords {
    if let Some(known) = known {
        if changed_precision(coords.longitude, known.longitude, 0.0000001) {
            known.longitude = coords.longitude;
        } else {
            coords.longitude = None;
        }

        if changed_precision(coords.latitude, known.latitude, 0.0000001) {
            known.latitude = coords.latitude;
        } else {
            coords.latitude = None;
        }

        if changed_precision(coords.altitude, known.altitude, 0.01) {
            known.altitude = coords.altitude;
        } else {
            coords.altitude = None;
        }

        if changed_precision(coords.u, known.u, 0.01) {
            known.u = coords.u;
        } else {
            coords.u = None;
        }

        if changed_precision(coords.v, known.v, 0.01) {
            known.v = coords.v;
        } else {
            coords.v = None;
        }

        if changed_precision(coords.roll, known.roll, 0.1) {
            known.roll = coords.roll;
        } else {
            coords.roll = None;
        }

        if changed_precision(coords.pitch, known.pitch, 0.1) {
            known.pitch = coords.pitch;
        } else {
            coords.pitch = None;
        }

        if changed_precision(coords.yaw, known.yaw, 0.1) {
            known.yaw = coords.yaw;
        } else {
            coords.yaw = None;
        }

        if changed_precision(coords.heading, known.heading, 0.1) {
            known.heading = coords.heading;
        } else {
            coords.heading = None;
        }
    } else {
        *known = Some(coords.clone());
    }

    coords
}

fn changed_precision(a: Option<f64>, b: Option<f64>, theta: f64) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() >= theta,
        (None, None) => false,
        _ => true,
    }
}
