use std::collections::HashSet;
use std::fs::File;
use std::ops::Neg;
use std::path::PathBuf;
use std::time::Instant;

use crate::draw::DrawError;
use crate::tasks::detect_recovery::is_recovery_attempt;
use crate::track::Track;
use crate::transform::Transform;
use tacview::record::{Property, Record, Tag, Update};
use ultraviolet::{DRotor3, DVec3};

#[derive(clap::Parser)]
pub struct Opts {
    input: PathBuf,
}

pub fn execute(opts: Opts) -> Result<(), crate::error::Error> {
    let start = Instant::now();

    let file = File::open(opts.input)?;
    let parser = tacview::Parser::new(file)?;

    let mut carriers: HashSet<u64> = HashSet::new();
    let mut planes: HashSet<u64> = HashSet::new();
    let mut tracks: Vec<CarrierPlanePair> = Vec::new();

    let mut time = 0.0;
    for record in parser {
        match record? {
            Record::Frame(secs) => {
                for track in &mut tracks {
                    track.process_frame()?;
                }

                time = secs;
            }

            Record::Update(update) => {
                if !carriers.contains(&update.id) && !planes.contains(&update.id) {
                    for p in &update.props {
                        // TODO: filter players
                        if let Property::Type(tags) = p {
                            if tags.contains(&Tag::AircraftCarrier) {
                                for plane_id in &planes {
                                    tracks.push(CarrierPlanePair::new(update.id, *plane_id));
                                }

                                carriers.insert(update.id);
                            } else if tags.contains(&Tag::FixedWing) {
                                for carrier_id in &carriers {
                                    tracks.push(CarrierPlanePair::new(*carrier_id, update.id));
                                }

                                planes.insert(update.id);
                            }
                        }
                    }
                }

                for track in &mut tracks {
                    track.update(time, &update);
                }
            }

            _ => {}
        }
    }

    for track in &mut tracks {
        track.draw()?;
    }

    println!("Took: {:.4}s", start.elapsed().as_secs_f64());

    Ok(())
}

struct CarrierPlanePair {
    carrier_id: u64,
    carrier: Transform,
    plane_id: u64,
    plane: Transform,
    is_recovery_attempt: bool,
    is_dirty: bool,
    datums: Track,
}

impl CarrierPlanePair {
    fn new(carrier_id: u64, plane_id: u64) -> Self {
        Self {
            carrier_id,
            carrier: Default::default(),
            plane_id,
            plane: Default::default(),
            is_recovery_attempt: false,
            is_dirty: false,
            datums: Track::default(),
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

                    transform.velocity = (new_pos - transform.position) / (time - transform.time);
                    transform.position = new_pos;
                    transform.time = time;

                    if is_plane {
                        self.is_dirty = true;
                    }
                }
                // Property::Name(name) => {
                //     self.name = Some(name);
                //     self.dirty = true;
                // }
                // Property::Pilot(pilot) => {
                //     self.pilot = Some(pilot);
                //     self.dirty = true;
                // }
                Property::AOA(aoa) => {
                    transform.aoa = *aoa;
                }
                _ => {}
            }
        }
    }

    fn process_frame(&mut self) -> Result<(), DrawError> {
        if !self.is_dirty {
            return Ok(());
        }

        self.is_dirty = false;

        if self.carrier.time == 0.0 || self.plane.time == 0.0 {
            return Ok(());
        }

        if self.is_recovery_attempt {
            if !self.datums.next(&self.carrier, &self.plane) {
                self.draw()?;
            }
        } else if is_recovery_attempt(&self.carrier, &self.plane) {
            self.is_recovery_attempt = true;
        }

        Ok(())
    }

    fn draw(&mut self) -> Result<(), DrawError> {
        if self.is_recovery_attempt {
            crate::draw::draw_chart(std::mem::take(&mut self.datums).finish())?;
            self.is_recovery_attempt = false;
        }

        Ok(())
    }
}
