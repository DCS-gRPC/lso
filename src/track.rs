use std::ops::Neg;

use ultraviolet::{DRotor3, DVec3};

use crate::data;
use crate::transform::Transform;

#[derive(Debug)]
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
}

#[derive(Debug, Default)]
pub struct Grading {
    pub cable: Option<u8>,
}

pub struct TrackResult {
    pub pilot_name: String,
    pub grading: Grading,
    pub dcs_grading: Option<String>,
    pub datums: Vec<Datum>,
}

impl Track {
    pub fn new(pilot_name: impl Into<String>) -> Self {
        Self {
            pilot_name: pilot_name.into(),
            previous_distance: f64::MAX,
            datums: Default::default(),
            grading: None,
            dcs_grading: None,
        }
    }

    pub fn next(&mut self, carrier: &Transform, plane: &Transform) -> bool {
        // TODO: select carrier and plane according to actual units
        let landing_pos_offset = data::NIMITZ
            .optimal_landing_offset(&data::FA18C)
            .rotated_by(carrier.rotation);
        let landing_pos = carrier.position + landing_pos_offset;

        let ray_from_plane_to_carrier = DVec3::new(
            landing_pos.x - plane.position.x,
            0.0, // ignore altitude
            landing_pos.z - plane.position.z,
        );

        // Stop tracking once the distance from the plane to the landing position is increasing and
        // has increased more than 20m (since the last time the distance was decreasing).
        let distance = ray_from_plane_to_carrier.mag();
        if distance < self.previous_distance {
            self.previous_distance = distance;
        } else if distance - self.previous_distance > 20.0 {
            tracing::debug!(distance_in_m = distance, "stop tracking");
            return false;
        }

        // Construct the x axis, which is aligned to the angled deck.
        let fb_rot = DRotor3::from_rotation_xz(
            (carrier.heading - data::NIMITZ.deck_angle)
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

        let hook_offset = data::FA18C.hook.rotated_by(plane.rotation);
        let alt = plane.alt - data::NIMITZ.deck_altitude + hook_offset.y;
        self.datums.push(Datum {
            x,
            y,
            aoa: plane.aoa,
            alt: alt.max(0.0),
        });

        // Detect touchdown based on whether the hook's end touches the deck.
        if self.grading.is_none() && alt <= 0.09 {
            self.grading = Some(Grading {
                cable: get_cable(carrier, plane),
            });
            tracing::debug!(distance_in_m = distance, alt, "stop tracking");
            return false;
        }

        true
    }

    pub fn finish(self) -> TrackResult {
        TrackResult {
            pilot_name: self.pilot_name,
            grading: self.grading.unwrap_or_default(),
            dcs_grading: self.dcs_grading,
            datums: self.datums,
        }
    }

    /// Set the track's dcs grading.
    pub fn set_dcs_grading(&mut self, dcs_grading: String) {
        self.dcs_grading = Some(dcs_grading);
    }
}

fn get_cable(carrier: &Transform, plane: &Transform) -> Option<u8> {
    let hook_offset = data::FA18C.hook.rotated_by(plane.rotation);
    let touchdown = plane.position + hook_offset;

    let cables = [
        (1, &data::NIMITZ.cable1),
        (2, &data::NIMITZ.cable2),
        (3, &data::NIMITZ.cable3),
        (4, &data::NIMITZ.cable4),
    ];
    for (nr, pendants) in cables {
        // Calculate the mid position between both cable pendants:
        // o-----------o
        //       ^
        //       |
        let mid_cable = (pendants.0 - pendants.1) / 2.0;
        let mid_cable = pendants.0 - mid_cable;

        // compensate for cable hitbox
        let mid_cable = mid_cable + (carrier.forward * 2.0);

        let mid_cable = carrier.position + mid_cable.rotated_by(carrier.rotation);

        // If the cable is in front of the touchdown position, consider it the one the plane
        // catches.
        let ray_to_cable = mid_cable - touchdown;
        tracing::trace!(
            cable = nr,
            distance = ray_to_cable.mag(),
            dot = ray_to_cable.dot(plane.forward),
            "cable candidate"
        );
        if ray_to_cable.dot(plane.forward) > 0.0 {
            return Some(nr);
        }
    }

    None
}
