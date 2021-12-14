use std::path::PathBuf;
use std::time::Duration;

use crate::utils::shutdown::ShutdownHandle;
use backoff::ExponentialBackoff;
use futures_util::future::select;
use futures_util::{StreamExt, TryFutureExt};
use stubs::coalition::v0::coalition_service_client::CoalitionServiceClient;
use stubs::common::v0::{Coalition, GroupCategory};
use stubs::group::v0::group_service_client::GroupServiceClient;
use stubs::mission::v0::mission_service_client::MissionServiceClient;
use stubs::mission::v0::stream_events_response::Event;
use stubs::unit::v0::unit_service_client::UnitServiceClient;
use stubs::{coalition, common, group, mission, unit};
use tokio::sync::mpsc;
use tonic::transport::{Channel, Endpoint};
use tonic::Status;

#[derive(clap::Parser)]
pub struct Opts {
    #[clap(short = 'o', long, default_value = ".")]
    out_dir: PathBuf,
    #[clap(long, env)]
    discord_webhook: Option<String>,
}

pub async fn execute(opts: Opts, shutdown_handle: ShutdownHandle) {
    if opts.discord_webhook.is_some() {
        tracing::info!("Discord integration enabled.");
    }

    let addr = "http://127.0.0.1:50051"; // TODO: move to config
    tracing::info!(endpoint = addr, "Connecting to gRPC server");

    let backoff = ExponentialBackoff {
        // never wait longer than 30s for a retry
        max_interval: Duration::from_secs(30),
        // never stop trying
        max_elapsed_time: None,
        ..Default::default()
    };

    select(
        Box::pin(backoff::future::retry_notify(
            backoff,
            // on each try, run the program and consider every error as transient (ie. worth
            // retrying)
            || async {
                run(&opts, addr, shutdown_handle.clone())
                    .await
                    .map_err(backoff::Error::Transient)
            },
            // error hook:
            |err, backoff: Duration| {
                tracing::debug!(
                    %err,
                    backoff = %format!("{:.2}s", backoff.as_secs_f64()),
                    "retrying after error"
                );
            },
        )),
        shutdown_handle.signal(),
    )
    .await;
}

async fn run<'a>(
    opts: &'a Opts,
    addr: &'static str,
    shutdown_handle: ShutdownHandle,
) -> Result<(), crate::error::Error> {
    let out_dir = opts.out_dir.clone();
    let channel = Endpoint::from_static(addr)
        .keep_alive_while_idle(true)
        .connect()
        .await?;
    tracing::info!("Connected");
    let mut coalition_svc = CoalitionServiceClient::new(channel.clone());
    let group_svc = GroupServiceClient::new(channel.clone());
    let mut unit_svc = UnitServiceClient::new(channel.clone());
    let mut mission_svc = MissionServiceClient::new(channel.clone());

    // initial full-sync of all current units inside of the mission
    let groups = coalition_svc
        .get_groups(coalition::v0::GetGroupsRequest {
            coalition: Coalition::All.into(),
            category: None,
        })
        .map_ok(|res| res.into_inner().groups)
        .await?;

    let group_units = futures_util::future::try_join_all(
        groups
            .into_iter()
            .filter(|group| {
                if let Some(category) = GroupCategory::from_i32(group.category) {
                    matches!(category, GroupCategory::Airplane | GroupCategory::Ship)
                } else {
                    false
                }
            })
            .map(|group| {
                let mut group_svc = group_svc.clone();
                async move {
                    group_svc
                        .get_units(group::v0::GetUnitsRequest {
                            group_name: group.name,
                            active: Some(true),
                        })
                        .map_ok(|res| res.into_inner().units)
                        .await
                }
            }),
    )
    .await?;

    let mut planes = Vec::new();
    let mut carriers = Vec::new();

    for units in group_units {
        for unit in units {
            match check_candidate(&mut unit_svc, &unit).await? {
                Some(Candidate::Plane) => planes.push((
                    unit.name,
                    unit.player_name.unwrap_or_else(|| String::from("KI")),
                )),
                Some(Candidate::Carrier) => carriers.push(unit.name),
                None => {}
            }
        }
    }

    let (tx, mut rx) = mpsc::channel(1);

    let discord_webhook = opts.discord_webhook.clone();
    let tx2 = tx.clone();
    let spawn_detect_recovery =
        move |carrier_name: String, plane_name: String, pilot_name: String| {
            let out_dir = out_dir.clone();
            let discord_webhook = discord_webhook.clone();
            let channel = channel.clone();
            let tx = tx2.clone();
            let shutdown_handle = shutdown_handle.clone();
            tokio::spawn(async move {
                if let Err(err) = crate::tasks::detect_recovery::detect_recovery(
                    &out_dir,
                    discord_webhook,
                    channel,
                    &carrier_name,
                    &plane_name,
                    &pilot_name,
                    shutdown_handle,
                )
                .await
                {
                    tx.send(err).await.ok();
                }
            });
        };

    for carrier_name in &carriers {
        for (plane_name, pilot_name) in &planes {
            spawn_detect_recovery(carrier_name.clone(), plane_name.clone(), pilot_name.clone());
        }
    }

    // listen for birth events to track carriers and planes spawned at a later point in time
    let mut events = mission_svc
        .stream_events(mission::v0::StreamEventsRequest {})
        .await?
        .into_inner();
    let tx = tx.clone();
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            let event = match event {
                Ok(stubs::mission::v0::StreamEventsResponse {
                    event: Some(event), ..
                }) => event,
                Ok(_) => continue,
                Err(err) => {
                    tx.send(err.into()).await.ok();
                    return;
                }
            };

            if let Event::Birth(mission::v0::stream_events_response::BirthEvent {
                initiator:
                    Some(common::v0::Initiator {
                        initiator: Some(common::v0::initiator::Initiator::Unit(unit)),
                    }),
                ..
            }) = event
            {
                match check_candidate(&mut unit_svc, &unit).await {
                    Ok(Some(Candidate::Plane)) => {
                        for carrier_name in &carriers {
                            spawn_detect_recovery(
                                carrier_name.clone(),
                                unit.name.clone(),
                                unit.player_name
                                    .clone()
                                    .unwrap_or_else(|| String::from("KI")),
                            );
                        }
                    }
                    Ok(Some(Candidate::Carrier)) => {
                        for (plane_name, pilot_name) in &planes {
                            spawn_detect_recovery(
                                unit.name.clone(),
                                plane_name.clone(),
                                pilot_name.clone(),
                            );
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::error!(
                            unit_name = %unit.name,
                            %err,
                            "ignoring unit due to an error while checking its eligibility",
                        );
                    }
                }
            }
        }
    });

    match rx.recv().await {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

enum Candidate {
    Carrier,
    Plane,
}

async fn check_candidate(
    svc: &mut UnitServiceClient<Channel>,
    unit: &common::v0::Unit,
) -> Result<Option<Candidate>, Status> {
    match GroupCategory::from_i32(unit.category) {
        // TODO: only players
        // Some(UnitCategory::UnitAirplane) if unit.player_name.is_some() => {
        Some(GroupCategory::Airplane) => return Ok(Some(Candidate::Plane)),
        Some(GroupCategory::Ship) => {
            let attrs = svc
                .get_descriptor(unit::v0::GetDescriptorRequest {
                    name: unit.name.clone(),
                })
                .await?
                .into_inner()
                .attributes;

            if attrs
                .iter()
                .any(|a| a.as_str() == "AircraftCarrier With Arresting Gear")
            {
                return Ok(Some(Candidate::Carrier));
            }
        }
        _ => {}
    }

    Ok(None)
}
