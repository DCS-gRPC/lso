use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::ops::Neg;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::draw::DrawError;
use crate::tasks::detect_recovery_attempt::is_recovery_attempt;
use crate::tasks::record_recovery::FILENAME_DATETIME_FORMAT;
use crate::track::{Track, TrackResult};
use crate::transform::Transform;
use tacview::record::{Event, EventKind, GlobalProperty, Property, Record, Tag, Update};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime, UtcOffset};
use ultraviolet::{DRotor3, DVec3};

#[derive(clap::Parser)]
pub struct Opts {
    input: PathBuf,
}

pub fn execute(opts: Opts) -> Result<(), crate::error::Error> {
    let start = Instant::now();

    let mut file = File::open(opts.input)?;
    let mut tracks = extract_tracks(&mut file)?;
    for track in &mut tracks {
        track.draw()?;
    }

    println!("Took: {:.4}s", start.elapsed().as_secs_f64());

    Ok(())
}

#[allow(unused)] // used in integration tests
pub fn extract_recoveries(rd: &mut impl Read) -> Result<Vec<TrackResult>, crate::error::Error> {
    let mut tracks = extract_tracks(rd)?;
    Ok(tracks
        .into_iter()
        .filter(|t| t.is_recovery_attempt)
        .map(|t| t.datums.finish())
        .collect())
}

fn extract_tracks(rd: &mut impl Read) -> Result<Vec<CarrierPlanePair>, crate::error::Error> {
    let parser = tacview::Parser::new_compressed(rd)?;

    let mut recording_time =
        OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let mut carriers: HashMap<u64, &'static CarrierInfo> = HashMap::new();
    let mut planes: HashMap<u64, (String, &'static AirplaneInfo)> = HashMap::new();
    let mut tracks: Vec<CarrierPlanePair> = Vec::new();

    let mut time = 0.0;
    for record in parser {
        match record? {
            Record::GlobalProperty(GlobalProperty::RecordingTime(time)) => {
                if let Ok(time) = OffsetDateTime::parse(&time, &Rfc3339) {
                    recording_time = if let Ok(offset) = UtcOffset::current_local_offset() {
                        time.to_offset(offset)
                    } else {
                        time
                    };
                }
            }

            Record::Frame(secs) => {
                for track in &mut tracks {
                    track.process_frame()?;
                }

                time = secs;
            }

            Record::Update(update) => {
                if !carriers.contains_key(&update.id) && !planes.contains_key(&update.id) {
                    let pilot_name = update
                        .props
                        .iter()
                        .find_map(|p| {
                            if let Property::Pilot(pilot_name) = p {
                                Some(pilot_name.as_str())
                            } else {
                                None
                            }
                        })
                        .unwrap_or("KI");
                    let name = update.props.iter().find_map(|p| {
                        if let Property::Name(name) = p {
                            Some(name)
                        } else {
                            None
                        }
                    });
                    let tags = update.props.iter().find_map(|p| {
                        if let Property::Type(tags) = p {
                            Some(tags)
                        } else {
                            None
                        }
                    });

                    if let Some((name, tags)) = name.zip(tags) {
                        if tags.contains(&Tag::AircraftCarrier) {
                            match CarrierInfo::by_type(name) {
                                Some(carrier_info) => {
                                    for (plane_id, (pilot_name, plane_info)) in &planes {
                                        tracks.push(CarrierPlanePair::new(
                                            recording_time + Duration::seconds_f64(time),
                                            update.id,
                                            carrier_info,
                                            *plane_id,
                                            pilot_name,
                                            plane_info,
                                        ));
                                    }

                                    carriers.insert(update.id, carrier_info);
                                }
                                None => tracing::trace!(name, "unsupported aircraft carrier"),
                            }
                        } else if tags.contains(&Tag::FixedWing) {
                            // TODO: filter players
                            match AirplaneInfo::by_type(name) {
                                Some(plane_info) => {
                                    for (carrier_id, carrier_info) in &carriers {
                                        tracks.push(CarrierPlanePair::new(
                                            recording_time + Duration::seconds_f64(time),
                                            *carrier_id,
                                            carrier_info,
                                            update.id,
                                            pilot_name,
                                            plane_info,
                                        ));
                                    }

                                    planes.insert(update.id, (pilot_name.to_string(), plane_info));
                                }
                                None => tracing::trace!(name, "unsupported fixed wing aircraft"),
                            }
                        }
                    }
                }

                for track in &mut tracks {
                    track.update(time, &update);
                }
            }

            Record::Event(Event {
                kind: EventKind::Landed,
                mut params,
                ..
            }) => {
                tracing::trace!(?params, "landed event");
                if let Some((carrier_id, plane_id)) = params
                    .pop()
                    .and_then(|id| u64::from_str(&id).ok())
                    .zip(params.pop().and_then(|id| u64::from_str(&id).ok()))
                {
                    tracing::trace!(carrier_id, plane_id, "landed event");
                    for track in &mut tracks {
                        track.landed(carrier_id, plane_id);
                    }
                }
            }

            Record::Event(Event {
                kind: EventKind::Message,
                mut params,
                text: Some(dcs_grading),
            }) => {
                if let Some((carrier_id, plane_id)) = params
                    .pop()
                    .and_then(|id| u64::from_str(&id).ok())
                    .zip(params.pop().and_then(|id| u64::from_str(&id).ok()))
                {
                    tracing::trace!(carrier_id, plane_id, dcs_grading, "dcs lso grading");
                    for track in &mut tracks {
                        track.dcs_grading(carrier_id, plane_id, &dcs_grading);
                    }
                }
            }

            _ => {}
        }
    }

    for track in &mut tracks {
        track.process_frame()?;
    }

    Ok(tracks)
}

struct CarrierPlanePair {
    recording_time: OffsetDateTime,
    pilot_name: String,
    carrier_id: u64,
    carrier: Transform,
    carrier_info: &'static CarrierInfo,
    plane_id: u64,
    plane: Transform,
    plane_info: &'static AirplaneInfo,
    is_recovery_attempt: bool,
    is_dirty: bool,
    is_done: bool,
    datums: Track,
    landed: bool,
}

impl CarrierPlanePair {
    fn new(
        recording_time: OffsetDateTime,
        carrier_id: u64,
        carrier_info: &'static CarrierInfo,
        plane_id: u64,
        pilot_name: &str,
        plane_info: &'static AirplaneInfo,
    ) -> Self {
        Self {
            recording_time,
            pilot_name: pilot_name.to_string(),
            carrier_id,
            carrier: Default::default(),
            carrier_info,
            plane_id,
            plane: Default::default(),
            plane_info,
            is_recovery_attempt: false,
            is_dirty: false,
            is_done: false,
            datums: Track::new(pilot_name, carrier_info, plane_info),
            landed: false,
        }
    }

    fn update(&mut self, time: f64, update: &Update) {
        let (mut transform, is_plane) = if update.id == self.carrier_id {
            (&mut self.carrier, false)
        } else if update.id == self.plane_id {
            (&mut self.plane, true)
        } else {
            return;
        };

        for p in &update.props {
            match p {
                Property::T(coords) => {
                    let mut orientation_changed = false;

                    if let Some(roll) = coords.roll {
                        transform.roll = roll;
                        orientation_changed = true;
                    }
                    if let Some(pitch) = coords.pitch {
                        transform.pitch = pitch;
                        orientation_changed = true;
                    }
                    if let Some(yaw) = coords.yaw {
                        transform.yaw = yaw;
                        orientation_changed = true;
                    }
                    if let Some(heading) = coords.heading {
                        transform.heading = heading;
                        orientation_changed = true;
                    }

                    if orientation_changed {
                        transform.forward = DVec3::new(
                            transform.yaw.to_radians().sin() * transform.pitch.to_radians().cos(),
                            transform.pitch.to_radians().sin(),
                            transform.yaw.to_radians().cos() * transform.pitch.to_radians().cos(),
                        );
                        transform.rotation = DRotor3::from_euler_angles(
                            transform.roll.neg().to_radians(),
                            transform.pitch.neg().to_radians(),
                            transform.heading.neg().to_radians(),
                        );
                    }

                    let mut new_pos = transform.position;

                    if let Some(altitude) = coords.altitude {
                        new_pos.y = altitude;
                        transform.alt = altitude;
                    }
                    if let Some(u) = coords.u {
                        new_pos.x = u;
                    }
                    if let Some(v) = coords.v {
                        new_pos.z = v;
                    }

                    transform.position = new_pos;
                    transform.time = time;

                    if is_plane {
                        self.is_dirty = true;
                    }
                }
                Property::Pilot(pilot_name) => {
                    self.pilot_name = pilot_name.to_string();
                }
                Property::AOA(aoa) => {
                    transform.aoa = *aoa;
                }
                _ => {}
            }
        }
    }

    fn landed(&mut self, carrier_id: u64, plane_id: u64) {
        if self.carrier_id == carrier_id && self.plane_id == plane_id && !self.landed {
            self.landed = true;
            self.is_dirty = true;
        }
    }

    fn dcs_grading(&mut self, carrier_id: u64, plane_id: u64, dcs_grading: &str) {
        if self.carrier_id == carrier_id && self.plane_id == plane_id {
            self.datums.set_dcs_grading(dcs_grading.to_string());
        }
    }

    fn process_frame(&mut self) -> Result<(), DrawError> {
        if !self.is_dirty || self.is_done {
            return Ok(());
        }

        self.is_dirty = false;

        if self.carrier.time == 0.0 || self.plane.time == 0.0 {
            return Ok(());
        }

        if self.is_recovery_attempt {
            let mut should_continue = self.datums.next(&self.carrier, &self.plane);
            if self.landed {
                self.datums.landed(&self.carrier, &self.plane);
                should_continue = false;
            }
            if !should_continue {
                self.is_done = true;
            }
        } else if is_recovery_attempt(&self.carrier, &self.plane) {
            self.is_recovery_attempt = true;
        }

        Ok(())
    }

    fn draw(&mut self) -> Result<(), DrawError> {
        if self.is_recovery_attempt {
            let out_dir = PathBuf::from(".");
            let filename = format!(
                "LSO-{}-{}",
                self.recording_time
                    .format(&FILENAME_DATETIME_FORMAT)
                    .unwrap_or_default(),
                self.pilot_name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
            );
            let track = std::mem::replace(
                &mut self.datums,
                Track::new(&self.pilot_name, self.carrier_info, self.plane_info),
            )
            .finish();
            crate::draw::draw_chart(&out_dir, &filename, &track)?;
            self.is_recovery_attempt = false;
            self.landed = false;
        }

        Ok(())
    }
}
