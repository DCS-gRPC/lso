use stubs::common::Unit;
use stubs::unit::{self, unit_service_client::UnitServiceClient};
use tonic::{transport::Channel, Status};
use ultraviolet::DVec3;

pub struct UnitClient {
    svc: UnitServiceClient<Channel>,
}

impl UnitClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: UnitServiceClient::new(ch),
        }
    }

    pub async fn get_transform(
        &mut self,
        unit_name: impl Into<String>,
    ) -> Result<Transform, Status> {
        let res = self
            .svc
            .get_transform(unit::GetTransformRequest {
                name: unit_name.into(),
            })
            .await?
            .into_inner();

        let position = res.position.unwrap_or_default();
        let orientation = res.orientation.unwrap_or_default();
        let forward = orientation.forward.unwrap_or_default();
        let velocity = res.velocity.unwrap_or_default();
        Ok(Transform {
            forward: DVec3::new(forward.x, forward.y, forward.z),
            velocity: DVec3::new(velocity.x, velocity.y, velocity.z),
            lat: position.lat,
            lon: position.lon,
            alt: position.alt,
            u: position.u,
            v: position.v,
            yaw: orientation.yaw,
            pitch: orientation.pitch,
            roll: orientation.roll,
            time: res.time,
        })
    }

    pub async fn get_unit(&mut self, unit_name: &str) -> Result<Unit, Status> {
        let unit = self
            .svc
            .get(unit::GetRequest {
                name: unit_name.to_string(),
            })
            .await?
            .into_inner()
            .unit
            .ok_or_else(|| {
                Status::not_found(format!("received empty response for unit `{}`", unit_name))
            })?;
        Ok(unit)
    }

    pub async fn get_descriptor(&mut self, unit_name: &str) -> Result<Vec<String>, Status> {
        let descriptor = self
            .svc
            .get_descriptor(unit::GetDescriptorRequest {
                name: unit_name.to_string(),
            })
            .await?
            .into_inner()
            .attributes;
        Ok(descriptor)
    }
}

#[derive(Debug)]
pub struct Transform {
    pub forward: DVec3,
    pub velocity: DVec3,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub u: f64,
    pub v: f64,
    // Yaw in degrees.
    pub yaw: f64,
    // Pitch in degrees.
    pub pitch: f64,
    // Roll in degrees.
    pub roll: f64,
    /// Time in seconds since the scenario started.
    pub time: f64,
}
