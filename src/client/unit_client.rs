use stubs::common::v0::Unit;
use stubs::unit;
use stubs::unit::v0::unit_service_client::UnitServiceClient;
use tonic::{transport::Channel, Status};

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

    pub async fn get_transform(
        &mut self,
        unit_name: impl Into<String>,
    ) -> Result<Transform, Status> {
        let res = self
            .svc
            .get_transform(unit::v0::GetTransformRequest {
                name: unit_name.into(),
            })
            .await?
            .into_inner();

        Ok((
            res.time,
            res.position.unwrap_or_default(),
            res.orientation.unwrap_or_default(),
            res.velocity.unwrap_or_default(),
        )
            .into())
    }

    pub async fn get_unit(&mut self, unit_name: &str) -> Result<Unit, Status> {
        let unit = self
            .svc
            .get(unit::v0::GetRequest {
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
            .get_descriptor(unit::v0::GetDescriptorRequest {
                name: unit_name.to_string(),
            })
            .await?
            .into_inner()
            .attributes;
        Ok(descriptor)
    }
}
