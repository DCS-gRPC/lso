use std::time::Duration;

use crate::utils::shutdown::{Shutdown, ShutdownHandle};
use backoff::ExponentialBackoff;
use futures_util::future::select;
use futures_util::TryFutureExt;
use stubs::coalition::coalition_service_client::CoalitionServiceClient;
use stubs::common::{Coalition, GroupCategory};
use stubs::group::group_service_client::GroupServiceClient;
use stubs::unit::unit_service_client::UnitServiceClient;
use stubs::{coalition, group, unit};
use tonic::transport::{Channel, Endpoint};

/// A subcommand for controlling testing
#[derive(clap::Parser)]
pub struct Opts {}

pub async fn execute(_opts: Opts, shutdown_handle: ShutdownHandle) {
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
            || async { run().await.map_err(backoff::Error::Transient) },
            // error hook:
            |err, backoff: Duration| {
                tracing::error!(
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

struct Services {
    coalition: CoalitionServiceClient<Channel>,
    group: GroupServiceClient<Channel>,
    unit: UnitServiceClient<Channel>,
}

async fn run() -> Result<(), Error> {
    let addr = "http://127.0.0.1:50051"; // TODO: move to config
    tracing::info!(endpoint = addr, "Connecting to gRPC server");
    let channel = Endpoint::from_static(addr)
        .keep_alive_while_idle(true)
        .connect()
        .await?;
    let mut svc = Services {
        coalition: CoalitionServiceClient::new(channel.clone()),
        group: GroupServiceClient::new(channel.clone()),
        unit: UnitServiceClient::new(channel.clone()),
    };

    detect_recoveries(&mut svc, channel).await?;

    Ok(())
}

async fn detect_recoveries(svc: &mut Services, ch: Channel) -> Result<(), Error> {
    // initial full-sync of all current units inside of the mission
    let groups = futures_util::future::try_join_all(
        [Coalition::Blue, Coalition::Red, Coalition::Neutral].map(|coalition| {
            let mut coalition_svc = svc.coalition.clone();
            async move {
                coalition_svc
                    .get_groups(coalition::GetGroupsRequest {
                        coalition: coalition.into(),
                        category: None,
                    })
                    .map_ok(|res| res.into_inner().groups)
                    .await
            }
        }),
    )
    .await?
    .into_iter()
    .flatten();

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
                let mut group_svc = svc.group.clone();
                async move {
                    let category = group.category;
                    group_svc
                        .get_units(group::GetUnitsRequest {
                            group_name: group.name,
                            active: Some(true),
                        })
                        .map_ok(|res| (category, res.into_inner().units))
                        .await
                }
            }),
    )
    .await?;

    let mut planes = Vec::new();
    let mut carriers = Vec::new();

    for (category, units) in group_units {
        for unit in units {
            match GroupCategory::from_i32(category) {
                // TODO: only players
                // Some(UnitCategory::UnitAirplane) if unit.player_name.is_some() => {
                Some(GroupCategory::Airplane) => planes.push(unit.name),
                Some(GroupCategory::Ship) => {
                    let attrs = svc
                        .unit
                        .get_descriptor(unit::GetDescriptorRequest {
                            name: unit.name.clone(),
                        })
                        .await?
                        .into_inner()
                        .attributes;

                    if attrs
                        .iter()
                        .any(|a| a.as_str() == "AircraftCarrier With Arresting Gear")
                    {
                        carriers.push(unit.name)
                    }
                }
                _ => {}
            }
        }
    }

    let shutdown = Shutdown::new();

    for carrier_name in carriers {
        for plane_name in &planes {
            // TODO: retry, error handling, spawn
            crate::tasks::detect_recovery::detect_recovery(
                ch.clone(),
                carrier_name.clone(),
                plane_name.clone(),
                shutdown.handle(),
            )
            .await?;
        }
    }

    // TODO: listen on events for new carrier/plane pairs to observe

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Grpc(#[from] tonic::Status),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    #[error("failed to write ACMI file")]
    Write(#[from] std::io::Error),
}
