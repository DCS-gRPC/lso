use std::collections::HashSet;
use std::fs::File;
use std::time::Duration;

use backoff::ExponentialBackoff;
use futures_util::future::{select, FutureExt};
use futures_util::TryFutureExt;
use lso::{tasks, Shutdown};
use stubs::coalition::coalition_service_client::CoalitionServiceClient;
use stubs::common::{Coalition, GroupCategory};
use stubs::group::group_service_client::GroupServiceClient;
use stubs::hook::hook_service_client::HookServiceClient;
use stubs::mission::mission_service_client::MissionServiceClient;
use stubs::unit::unit_service_client::UnitServiceClient;
use stubs::{coalition, group, hook, mission, unit};
use tacview::record::{Color, Coords, GlobalProperty, Property, Tag, Update};
use tonic::transport::{Channel, Endpoint};
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() {
    let filter =
        std::env::var("RUST_LOG").unwrap_or_else(|_| "recorder=trace,lso=trace".to_owned());
    let registry = tracing_subscriber::registry().with(
        tracing_subscriber::filter::EnvFilter::new(filter)
            .and_then(tracing_subscriber::fmt::layer()),
    );
    registry.init();

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
        // stop on CTRL+C
        Box::pin(tokio::signal::ctrl_c().map(|_| ())),
    )
    .await;
}

struct Services {
    coalition: CoalitionServiceClient<Channel>,
    group: GroupServiceClient<Channel>,
    hook: HookServiceClient<Channel>,
    mission: MissionServiceClient<Channel>,
    unit: UnitServiceClient<Channel>,
}

async fn run() -> Result<(), Error> {
    let addr = "http://127.0.0.1:50051"; // TODO: move to config
    tracing::debug!(endpoint = addr, "Connecting to gRPC server");
    let channel = Endpoint::from_static(addr)
        .keep_alive_while_idle(true)
        .connect()
        .await?;
    let mut svc = Services {
        coalition: CoalitionServiceClient::new(channel.clone()),
        group: GroupServiceClient::new(channel.clone()),
        hook: HookServiceClient::new(channel.clone()),
        mission: MissionServiceClient::new(channel.clone()),
        unit: UnitServiceClient::new(channel.clone()),
    };

    detect_recoveries(&mut svc, channel).await?;
    // record_carrier_recovery(&mut svc, "Mother", "F18").await?;

    Ok(())
}

async fn record_carrier_recovery(
    svc: &mut Services,
    carrier_name: &str,
    aircraft_name: &str,
) -> Result<(), Error> {
    let file = File::create("./test.txt.acmi")?;
    let mut recording = tacview::Writer::new(file)?;

    let reference_time = svc
        .mission
        .get_scenario_start_time(mission::GetScenarioStartTimeRequest {})
        .await?
        .into_inner();
    recording.write(GlobalProperty::ReferenceTime(reference_time.datetime))?;

    let mission_name = svc
        .hook
        .get_mission_name(hook::GetMissionNameRequest {})
        .await?
        .into_inner();
    recording.write(GlobalProperty::Title(format!(
        "Carrier Recovery during {}",
        mission_name.name
    )))?;
    recording.write(GlobalProperty::Author(format!(
        "dcs-grpc-lso v{}",
        env!("CARGO_PKG_VERSION")
    )))?;

    recording.write(create_initial_update(svc, 1, carrier_name).await?)?;
    recording.write(create_initial_update(svc, 2, aircraft_name).await?)?;

    Ok(())
}

async fn create_initial_update(
    svc: &mut Services,
    id: u64,
    unit_name: &str,
) -> Result<Update, Error> {
    let unit = svc
        .unit
        .get(unit::GetRequest {
            name: unit_name.to_string(),
        })
        .await?
        .into_inner()
        .unit
        .ok_or_else(|| Error::MissingUnit(unit_name.to_string()))?;

    let pos = unit
        .position
        .as_ref()
        .ok_or(Error::MissingProperty("position"))?;

    let attrs = svc
        .unit
        .get_descriptor(unit::GetDescriptorRequest {
            name: unit_name.to_string(),
        })
        .await?
        .into_inner()
        .attributes;

    let coalition = Coalition::from_i32(unit.coalition).unwrap_or(Coalition::Neutral);
    let mut props = vec![
        Property::T(Coords::default().position(pos.lat, pos.lon, pos.alt)),
        Property::Type(tags(&attrs)),
        Property::Name(unit.r#type),
        Property::Group(unit.group_name),
        Property::Color(color(coalition)),
    ];
    if let Some(player_name) = &unit.player_name {
        props.push(Property::Pilot(player_name.to_string()))
    }

    Ok(Update { id, props })
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

    dbg!(&planes);
    dbg!(&carriers);

    let shutdown = Shutdown::new();

    for carrier_name in carriers {
        for plane_name in &planes {
            // TODO: retry, error handling, spawn
            tasks::detect_recovery(
                ch.clone(),
                carrier_name.clone(),
                plane_name.clone(),
                shutdown.handle(),
            )
            .await?;
        }
    }

    Ok(())
}

fn tags<I: AsRef<str>>(attrs: impl IntoIterator<Item = I>) -> HashSet<Tag> {
    let mut tags = HashSet::with_capacity(2);
    for attr in attrs.into_iter() {
        match attr.as_ref() {
            "Ships" => {
                tags.insert(Tag::Sea);
                tags.insert(Tag::Watercraft);
            }
            "AircraftCarrier" => {
                tags.insert(Tag::AircraftCarrier);
            }
            "Air" => {
                tags.insert(Tag::Air);
            }
            "Planes" => {
                tags.insert(Tag::FixedWing);
            }
            _ => {}
        }
    }
    tags
}

fn color(coalition: Coalition) -> Color {
    match coalition {
        Coalition::Neutral => Color::Grey,
        Coalition::Red => Color::Red,
        Coalition::Blue => Color::Blue,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Grpc(#[from] tonic::Status),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error("event stream ended")]
    End,
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    #[error("failed to write ACMI file")]
    Write(#[from] std::io::Error),
    #[error("expected property `{0}` was missing")]
    MissingProperty(&'static str),
    #[error("unit `{0}` does not exist")]
    MissingUnit(String),
}
