use std::collections::HashSet;
use std::io::Cursor;
use std::ops::Neg;
use std::time::Duration;

use futures_util::StreamExt;
use stubs::common::Coalition;
use tacview::record::{Color, Coords, GlobalProperty, Property, Record, Tag, Update};
use tonic::{transport::Channel, Status};
use ultraviolet::{DRotor3, DVec3};

use crate::client::{HookClient, MissionClient, UnitClient};
use crate::data;
use crate::draw::Datum;
use crate::transform::Transform;
use crate::utils::shutdown::ShutdownHandle;

#[tracing::instrument(skip(ch, shutdown))]
pub async fn record_recovery(
    ch: Channel,
    carrier_name: String,
    plane_name: String,
    shutdown: ShutdownHandle,
) -> Result<(), Status> {
    tracing::debug!("start recording");

    let mut client1 = UnitClient::new(ch.clone());
    let mut client2 = UnitClient::new(ch.clone());
    let mut mission = MissionClient::new(ch.clone());
    let mut hook = HookClient::new(ch.clone());
    let mut interval = crate::utils::interval::interval(Duration::from_millis(150), shutdown);

    let mut acmi = Cursor::new(Vec::new());
    // TODO: compressed
    let mut recording = tacview::Writer::new(&mut acmi)?;
    let mut track: Vec<Datum> = Vec::new();

    let reference_time = mission.get_scenario_start_time().await?;
    recording.write(GlobalProperty::ReferenceTime(reference_time))?;

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

    recording.write(create_initial_update(&mut client1, 1, &carrier_name).await?)?;
    recording.write(create_initial_update(&mut client1, 2, &plane_name).await?)?;

    let mut known_carrier_coords = None;
    let mut known_plane_coords = None;

    while interval.next().await.is_some() {
        let (carrier, plane) = futures_util::future::try_join(
            client1.get_transform(&carrier_name),
            client2.get_transform(&plane_name),
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

        if let Some(datum) = calculate_datum(&carrier, &plane) {
            if let Some(last) = track.last() {
                // ignore data point if position hasn't changed
                let epsilon = 0.01;
                if (last.x - datum.x).abs() < epsilon && (last.y - datum.y).abs() < epsilon {
                    continue;
                }
            }

            track.push(datum);
        } else {
            break;
        }
    }

    let data = acmi.into_inner();
    tokio::fs::write("./test.txt.acmi", &data).await?;
    crate::draw::draw_chart(track);

    Ok(())
}

pub fn calculate_datum(carrier: &Transform, plane: &Transform) -> Option<Datum> {
    // TODO: select carrier and plane according to actual units
    let mut landing_pos_offset = data::NIMITZ.optimal_landing_offset(&data::FA18C);

    let carrier_rot = DRotor3::from_rotation_xz((carrier.heading).neg().to_radians());
    landing_pos_offset.rotate_by(carrier_rot);

    let landing_pos = carrier.position + landing_pos_offset;

    let ray_from_plane_to_carrier = DVec3::new(
        landing_pos.x - plane.position.x,
        0.0, // ignore altitude
        landing_pos.z - plane.position.z,
    );

    let distance = ray_from_plane_to_carrier.mag();
    let fb = DVec3::unit_z().rotated_by(carrier_rot);
    let dot = ray_from_plane_to_carrier.dot(fb);
    if dot < 0.0 && distance > 10.0 {
        tracing::trace!(dot, distance_in_m = distance, "stop tracking");
        return None;
    }

    // construct the x axis, which is aligned to the angled deck
    // TODO: fix origin of angled deck
    let fb_rot = DRotor3::from_rotation_xz(
        (carrier.heading - data::NIMITZ.deck_angle)
            .neg()
            .to_radians(),
    );
    let fb = DVec3::unit_z().rotated_by(fb_rot);

    let x = ray_from_plane_to_carrier.dot(fb);
    let mut y = (distance.powi(2) - x.powi(2)).sqrt();

    // determine whether plane is left or right of the glide slope
    let a = DVec3::unit_x().rotated_by(fb_rot);
    if ray_from_plane_to_carrier.dot(a) > 0.0 {
        y = y.neg();
    }

    // calculate altitude of the hook
    let hook_offset = data::FA18C
        .hook
        .rotated_by(DRotor3::from_rotation_yz(plane.pitch.to_radians().neg()));

    Some(Datum {
        x,
        y,
        aoa: plane.aoa,
        alt: plane.alt - data::NIMITZ.deck_altitude + hook_offset.y,
    })
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
        Property::Group(unit.group_name),
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
        Coalition::Neutral => Color::Grey,
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
