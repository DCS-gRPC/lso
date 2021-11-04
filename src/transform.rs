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

impl Transform {
    fn interpolate_position(&self, time: f64) -> DVec3 {
        if time <= self.time || self.velocity.mag() == 0.0 {
            return self.position;
        }

        let elapsed = time - self.time;
        let delta = self.velocity * (self.velocity.mag() * elapsed);

        self.position + delta
    }
}
