use std::ops::Neg;

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

impl From<(f64, stubs::common::v0::Transform)> for Transform {
    fn from((time, transform): (f64, stubs::common::v0::Transform)) -> Self {
        let position = transform.position.unwrap_or_default();
        let orientation = transform.orientation.unwrap_or_default();
        let forward = orientation.forward.unwrap_or_default();
        let forward = DVec3::new(forward.x, forward.y, forward.z);

        let velocity = transform.velocity.unwrap_or_default();
        let velocity = DVec3::new(velocity.x, velocity.y, velocity.z);
        let aoa = forward.dot(velocity.normalized()).acos().to_degrees();

        Transform {
            forward,
            velocity,
            position: DVec3::new(transform.u, position.alt, transform.v),
            heading: transform.heading,
            lat: position.lat,
            lon: position.lon,
            alt: position.alt,
            yaw: orientation.yaw,
            pitch: orientation.pitch,
            roll: orientation.roll,
            rotation: DRotor3::from_euler_angles(
                orientation.roll.neg().to_radians(),
                orientation.pitch.neg().to_radians(),
                transform.heading.neg().to_radians(),
            ),
            aoa,
            time,
        }
    }
}
