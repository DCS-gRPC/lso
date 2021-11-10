use stubs::mission;
use stubs::mission::v0::mission_service_client::MissionServiceClient;
use tonic::{transport::Channel, Status};

pub struct MissionClient {
    svc: MissionServiceClient<Channel>,
}

impl MissionClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: MissionServiceClient::new(ch),
        }
    }

    pub async fn get_scenario_start_time(&mut self) -> Result<String, Status> {
        let res = self
            .svc
            .get_scenario_start_time(mission::v0::GetScenarioStartTimeRequest {})
            .await?
            .into_inner();
        Ok(res.datetime)
    }
}
