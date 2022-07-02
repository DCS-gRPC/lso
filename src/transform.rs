use std::ops::Neg;

use stubs::common::v0::{Orientation, Position, Vector, Velocity};
use ultraviolet::{DRotor3, DVec3};

#[derive(Debug, Default)]
pub struct Transform {
    pub forward: DVec3,
    pub velocity: DVec3,
    pub position: DVec3,
    pub heading: f64,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    // Yaw in degrees.
    pub yaw: f64,
    // Pitch in degrees.
    pub pitch: f64,
    // Roll in degrees.
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
        let velocity = fix_vector(velocity.velocity.unwrap_or_default());
        let forward = fix_vector(orientation.forward.unwrap_or_default());
        let aoa = forward.dot(velocity.normalized()).acos().to_degrees();

        Transform {
            forward,
            velocity,
            position: DVec3::new(position.u, position.alt, position.v),
            heading: orientation.heading,
            lat: position.lat,
            lon: position.lon,
            alt: position.alt,
            yaw: orientation.yaw,
            pitch: orientation.pitch,
            roll: orientation.roll,
            rotation: DRotor3::from_euler_angles(
                orientation.roll.neg().to_radians(),
                orientation.pitch.neg().to_radians(),
                orientation.heading.neg().to_radians(),
            ),
            aoa,
            time,
        }
    }
}

/// Convert DCS' unusual right-hand coordinate system where +x points north to a more common
/// left-hand coordinate system where +z points north (and +x points east).
fn fix_vector(v: Vector) -> DVec3 {
    DVec3::new(v.z, v.y, v.x)
}
