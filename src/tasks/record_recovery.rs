use std::collections::HashSet;
use std::io::Cursor;
use std::time::Duration;

use futures_util::StreamExt;
use stubs::common::Coalition;
use tacview::record::{Color, Coords, GlobalProperty, Property, Tag, Update};
use tonic::{transport::Channel, Status};

use crate::client::{HookClient, MissionClient, UnitClient};
use crate::utils::shutdown::ShutdownHandle;

#[tracing::instrument(skip(ch, shutdown))]
pub async fn record_recovery(
    ch: Channel,
    carrier_name: String,
    plane_name: String,
    shutdown: ShutdownHandle,
) -> Result<(), Status> {
    let mut client = UnitClient::new(ch.clone());
    let mut mission = MissionClient::new(ch.clone());
    let mut hook = HookClient::new(ch.clone());
    let mut interval = crate::utils::interval::interval(Duration::from_millis(200), shutdown);

    let mut acmi = Cursor::new(Vec::new());
    let mut recording = tacview::Writer::new_compressed(&mut acmi)?;

    let reference_time = mission.get_scenario_start_time().await?;
    recording.write(GlobalProperty::ReferenceTime(reference_time))?;

    let mission_name = hook.get_mission_name().await?;
    recording.write(GlobalProperty::Title(format!(
        "Carrier Recovery during {}",
        mission_name
    )))?;
    recording.write(GlobalProperty::Author(format!(
        "dcs-grpc-lso v{}",
        env!("CARGO_PKG_VERSION")
    )))?;

    recording.write(create_initial_update(&mut client, 1, &carrier_name).await?)?;
    recording.write(create_initial_update(&mut client, 2, &plane_name).await?)?;

    while interval.next().await.is_some() {
        // let carrier = client.get_transform(&carrier_name).await?;
        // let plane = client.get_transform(&plane_name).await?;

        todo!();
    }

    Ok(())
}

async fn create_initial_update(
    client: &mut UnitClient,
    id: u64,
    unit_name: &str,
) -> Result<Update, Status> {
    let unit = client.get_unit(unit_name).await?;

    let pos = unit
        .position
        .as_ref()
        .ok_or_else(|| Status::not_found("unit did not include position"))?;

    let attrs = client.get_descriptor(unit_name).await?;

    let coalition = Coalition::from_i32(unit.coalition).unwrap_or(Coalition::Neutral);
    let mut props = vec![
        Property::T(Coords::default().lat(pos.lat).lon(pos.lon).alt(pos.alt)),
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
