use std::ops::Neg;
use std::str::FromStr;

use ultraviolet::{DRotor3, DVec3};

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::transform::Transform;

#[derive(Debug, PartialEq)]
pub struct Datum {
    pub x: f64,
    pub y: f64,
    pub aoa: f64,
    pub alt: f64,
}

pub struct Track {
    pilot_name: String,
    previous_distance: f64,
    datums: Vec<Datum>,
    grading: Option<Grading>,
    dcs_grading: Option<String>,
    carrier_info: &'static CarrierInfo,
    plane_info: &'static AirplaneInfo,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Grading {
    Unknown,
    Bolter,
    Recovered {
        cable: Option<u8>,
        cable_estimated: Option<u8>,
    },
}

#[derive(Debug, PartialEq)]
pub struct TrackResult {
    pub pilot_name: String,
    pub glide_slope: f64,
    pub grading: Grading,
    pub dcs_grading: Option<String>,
    pub datums: Vec<Datum>,
}

impl Track {
    pub fn new(
        pilot_name: impl Into<String>,
        carrier_info: &'static CarrierInfo,
        plane_info: &'static AirplaneInfo,
    ) -> Self {
        Self {
            pilot_name: pilot_name.into(),
            previous_distance: f64::MAX,
            datums: Default::default(),
            grading: None,
            dcs_grading: None,
            carrier_info,
            plane_info,
        }
    }

    pub fn next(&mut self, carrier: &Transform, plane: &Transform) -> bool {
        let landing_pos_offset = self
            .carrier_info
            .optimal_landing_offset(self.plane_info)
            .rotated_by(carrier.rotation);
        let landing_pos = carrier.position + landing_pos_offset;

        let ray_from_plane_to_carrier = DVec3::new(
            landing_pos.x - plane.position.x,
            0.0, // ignore altitude
            landing_pos.z - plane.position.z,
        );

        // Stop tracking once the distance from the plane to the landing position is increasing and
        // has increased more than 100m (since the last time the distance was decreasing).
        let distance = ray_from_plane_to_carrier.mag();
        if distance < self.previous_distance {
            self.previous_distance = distance;
        } else if distance - self.previous_distance > 100.0 {
            if self.grading.is_some() {
                tracing::debug!(distance_in_m = distance, "bolter detected");
                self.grading = Some(Grading::Bolter);
            }

            tracing::debug!(distance_in_m = distance, "stop tracking");

            return false;
        }

        // Already landed, no need to actually record any more datums, but keep going to detect
        // bolters.
        if self.grading.is_some() {
            return true;
        }

        // Construct the x axis, which is aligned to the angled deck.
        let fb_rot = DRotor3::from_rotation_xz(
            (carrier.heading - self.carrier_info.deck_angle)
                .neg()
                .to_radians(),
        );
        let fb = DVec3::unit_z().rotated_by(fb_rot);

        let x = ray_from_plane_to_carrier.dot(fb);
        let mut y = (distance.powi(2) - x.powi(2)).sqrt();

        // Determine whether plane is left or right of the glide slope.
        let a = DVec3::unit_x().rotated_by(fb_rot);
        if ray_from_plane_to_carrier.dot(a) > 0.0 {
            y = y.neg();
        }

        let hook_offset = self.plane_info.hook.rotated_by(plane.rotation);
        let alt = plane.alt - self.carrier_info.deck_altitude + hook_offset.y;
        self.datums.push(Datum {
            x,
            y,
            aoa: plane.aoa,
            alt: alt.max(0.0),
        });

        true
    }

    pub fn landed(&mut self, carrier: &Transform, plane: &Transform) {
        let cable = self.estimate_cable(carrier, plane);
        self.grading = Some(Grading::Recovered {
            cable,
            cable_estimated: cable,
        });
        tracing::debug!(?cable, "landed, stop tracking");
    }

    pub fn finish(self) -> TrackResult {
        // If DCS grading is set, use its reported wire instead of the estimated one.
        let grading = if let Some(dcs_wire) = self.dcs_grading.as_ref().and_then(|s| {
            s.split_once("WIRE# ")
                .and_then(|(_, w)| u8::from_str(w).ok())
        }) {
            match self.grading {
                Some(Grading::Recovered {
                    cable_estimated, ..
                }) => Grading::Recovered {
                    cable: Some(dcs_wire),
                    cable_estimated,
                },
                _ => Grading::Recovered {
                    cable: Some(dcs_wire),
                    cable_estimated: None,
                },
            }
        } else {
            self.grading.unwrap_or_default()
        };

        TrackResult {
            pilot_name: self.pilot_name,
            glide_slope: self.plane_info.glide_slope,
            grading,
            dcs_grading: self.dcs_grading,
            datums: self.datums,
        }
    }

    /// Set the track's dcs grading.
    pub fn set_dcs_grading(&mut self, dcs_grading: String) {
        self.dcs_grading = Some(dcs_grading);
    }

    fn estimate_cable(&self, carrier: &Transform, plane: &Transform) -> Option<u8> {
        let hook_offset = self.plane_info.hook.rotated_by(plane.rotation);
        let touchdown = plane.position + hook_offset;
        let forward = carrier
            .forward
            .rotated_by(DRotor3::from_rotation_xz(-self.carrier_info.deck_angle));

        // The land event is fired shortly after the aircraft caught the wire, so already when the hook
        // is past the wire it caught. To compensate for that, move the touchdown position 3.0m back.
        let touchdown = touchdown + (forward * 3.0);

        // For some visual debugging, uncomment the println! lines here and in the `.map()` below and
        // plot them (e.g. in excel in a scatter graph; plotting the top-down view, so only x/y is
        // usually enough).
        // println!("name;x;y;z");
        // println!(
        //     "plane_position;{};{};{}",
        //     plane.position.x, plane.position.z, plane.position.y
        // );
        // println!(
        //     "hook_touchdown;{};{};{}",
        //     touchdown.x, touchdown.z, touchdown.y
        // );

        let cables = [
            (1, &self.carrier_info.cable1),
            (2, &self.carrier_info.cable2),
            (3, &self.carrier_info.cable3),
            (4, &self.carrier_info.cable4),
        ]
        .into_iter()
        .map(|(nr, pendants)| {
            // Calculate the mid position between both cable pendants:
            // o-----------o
            //       ^
            //       |
            let mid_cable = (pendants.0 - pendants.1) / 2.0;
            let mid_cable = pendants.0 - mid_cable;
            let mid_cable = carrier.position + mid_cable.rotated_by(carrier.rotation);

            // println!(
            //     "cable_{};{};{};{}",
            //     nr, mid_cable.x, mid_cable.z, mid_cable.y
            // );
            // let p0 = carrier.position + pendants.0.rotated_by(carrier.rotation);
            // let p1 = carrier.position + pendants.1.rotated_by(carrier.rotation);
            // println!("p0_{};{};{};{}", nr, p0.x, p0.z, p0.y);
            // println!("p1_{};{};{};{}", nr, p1.x, p1.z, p1.y);

            (nr, mid_cable)
        })
        .collect::<Vec<_>>();

        for (nr, mid_cable) in cables {
            // If the cable is in front of the touchdown position, consider it the one the plane
            // catches.
            let ray_to_cable = touchdown - mid_cable;
            tracing::trace!(
                cable = nr,
                distance = ray_to_cable.mag(),
                dot = ray_to_cable.dot(forward),
                "cable candidate"
            );
            if ray_to_cable.dot(forward) > 0.0 {
                return Some(nr);
            }
        }

        None
    }
}

impl Default for Grading {
    fn default() -> Self {
        Self::Unknown
    }
}
