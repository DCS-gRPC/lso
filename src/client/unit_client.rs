use stubs::common::Unit;
use stubs::unit::{self, unit_service_client::UnitServiceClient};
use tonic::{transport::Channel, Status};
use ultraviolet::DVec3;

use crate::transform::Transform;

pub struct UnitClient {
    svc: UnitServiceClient<Channel>,
}

impl UnitClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: UnitServiceClient::new(ch),
        }
    }

    pub async fn export(
        &mut self,
        unit_name: impl Into<String>,
    ) -> Result<Transform, Status> {
        let res = self
            .svc
            .export(unit::ExportRequest {
                name: unit_name.into(),
            })
            .await?
            .into_inner();

        let position = res.position.unwrap_or_default();
        let orientation = res.orientation.unwrap_or_default();
        let forward = orientation.forward.unwrap_or_default();
        let forward = DVec3::new(forward.x, forward.y, forward.z);

        let velocity = res.velocity.unwrap_or_default();
        let velocity = DVec3::new(velocity.x, velocity.y, velocity.z);
        let aoa = forward.dot(velocity.normalized()).acos().to_degrees();

        Ok(Transform {
            forward,
            velocity,
            position: DVec3::new(res.u, position.alt, res.v),
            heading: res.heading,
            lat: position.lat,
            lon: position.lon,
            alt: position.alt,
            yaw: orientation.yaw,
            pitch: orientation.pitch,
            roll: orientation.roll,
            aoa,
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
