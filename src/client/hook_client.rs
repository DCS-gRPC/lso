use stubs::hook;
use stubs::hook::v0::hook_service_client::HookServiceClient;
use tonic::{transport::Channel, Status};

pub struct HookClient {
    svc: HookServiceClient<Channel>,
}

impl HookClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: HookServiceClient::new(ch),
        }
    }

    pub async fn get_mission_name(&mut self) -> Result<String, Status> {
        let res = self
            .svc
            .get_mission_name(hook::v0::GetMissionNameRequest {})
            .await?
            .into_inner();
        Ok(res.name)
    }
}
