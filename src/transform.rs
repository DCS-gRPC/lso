use std::ops::Neg;

use stubs::common::v0::{Orientation, Position, Vector, Velocity};
use ultraviolet::{DRotor3, DVec3};

use crate::utils::precision::Precision;

#[derive(Debug, Default)]
pub struct Transform {
    pub forward: DVec3,
    pub position: DVec3,
    pub heading: f64,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    /// Yaw in degrees.
    pub yaw: f64,
    /// Pitch in degrees.
    pub pitch: f64,
    /// Roll in degrees.
    pub roll: f64,
    pub rotation: DRotor3,
    pub aoa: f64,
    /// Time in seconds since the scenario started.
    pub time: f64,
}

impl From<(f64, Position, Orientation, Velocity)> for Transform {
    fn from(
        (time, position, orientation, velocity): (f64, Position, Orientation, Velocity),
    ) -> Self {
        // Since AOA is directly written to the TacView file, it can be calculated on the unrounded
        // data.
        let velocity = fix_vector(velocity.velocity.unwrap_or_default());
        let forward = fix_vector(orientation.forward.unwrap_or_default());
        let aoa = forward.dot(velocity.normalized()).acos().to_degrees();

        // The result from a DCS recording and TacView replay should match exactly, which is why the
        // values the calculations are based on must be rounded to the same precision
        // (see https://github.com/rkusa/tacview/blob/main/src/record/property.rs#L982-L1031).
        let yaw = orientation.yaw.max_precision(1);
        let pitch = orientation.pitch.max_precision(1);
        let roll = orientation.roll.max_precision(1);

        Transform {
            // Calculate forward instead of taking it from the gRPC response to match the behavior
            // when generating a report from a TacView recording
            forward: DVec3::new(
                yaw.to_radians().sin() * pitch.to_radians().cos(),
                pitch.to_radians().sin(),
                yaw.to_radians().cos() * pitch.to_radians().cos(),
            ),
            position: DVec3::new(
                position.u.max_precision(2),
                position.alt.max_precision(2),
                position.v.max_precision(2),
            ),
            heading: orientation.heading.max_precision(1),
            lat: position.lat.max_precision(7),
            lon: position.lon.max_precision(7),
            alt: position.alt.max_precision(2),
            yaw,
            pitch,
            roll,
            rotation: DRotor3::from_euler_angles(
                roll.neg().to_radians(),
                pitch.neg().to_radians(),
                orientation.heading.max_precision(1).neg().to_radians(),
            ),
            aoa: aoa.max_precision(2),
            time: time.max_precision(2),
        }
    }
}

/// Convert DCS' unusual right-hand coordinate system where +x points north to a more common
/// left-hand coordinate system where +z points north (and +x points east).
fn fix_vector(v: Vector) -> DVec3 {
    DVec3::new(v.z, v.y, v.x)
}
