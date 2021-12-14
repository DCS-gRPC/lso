use std::future::ready;

use futures_util::{Stream, StreamExt};
use stubs::mission;
use stubs::mission::v0::mission_service_client::MissionServiceClient;
use stubs::mission::v0::stream_events_response::Event;
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

    pub async fn stream_events(
        &mut self,
    ) -> Result<impl Stream<Item = Result<Event, Status>>, Status> {
        let events = self
            .svc
            .stream_events(mission::v0::StreamEventsRequest {})
            .await?
            .into_inner()
            .filter_map(|event| {
                ready(match event {
                    Ok(stubs::mission::v0::StreamEventsResponse {
                        event: Some(event), ..
                    }) => Some(Ok(event)),
                    Err(err) => Some(Err(err)),
                    Ok(_) => None,
                })
            });
        Ok(events)
    }
}
