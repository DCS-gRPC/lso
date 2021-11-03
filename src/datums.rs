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

pub struct Datums {
    previous_distance: f64,
    datums: Vec<Datum>,
}

impl Default for Datums {
    fn default() -> Self {
        Self {
            previous_distance: f64::MAX,
            datums: Default::default(),
        }
    }
}

impl Datums {
    pub fn next(&mut self, carrier: &Transform, plane: &Transform) -> bool {
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

        // Stop tracking once the distance from the plane to the landing position is increasing and has
        // increased more than 20m (since the last time the distance was decreasing).
        let distance = ray_from_plane_to_carrier.mag();
        if distance < self.previous_distance {
            self.previous_distance = distance;
        } else if distance - self.previous_distance > 20.0 {
            tracing::debug!(distance_in_m = distance, "stop tracking");
            return false;
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

        self.datums.push(Datum {
            x,
            y,
            aoa: plane.aoa,
            alt: plane.alt - data::NIMITZ.deck_altitude + hook_offset.y,
        });

        true
    }

    pub fn finish(self) -> Vec<Datum> {
        self.datums
    }
}
