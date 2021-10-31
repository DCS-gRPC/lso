use std::time::Duration;

use futures_util::StreamExt;
use tonic::{transport::Channel, Status};
use ultraviolet::DVec3;

use crate::client::UnitClient;
use crate::utils::{m_to_ft, m_to_nm, shutdown::ShutdownHandle};

#[tracing::instrument(skip(ch, shutdown))]
pub async fn detect_recovery(
    ch: Channel,
    carrier_name: String,
    plane_name: String,
    shutdown: ShutdownHandle,
) -> Result<(), Status> {
    let mut client = UnitClient::new(ch);
    let mut interval = crate::utils::interval::interval(Duration::from_secs(2), shutdown);

    while interval.next().await.is_some() {
        let carrier = client.get_transform(&carrier_name).await?;
        let plane = client.get_transform(&plane_name).await?;

        // dbg!(&carrier);
        // dbg!(&plane);

        // ignore planes above 500ft
        if m_to_ft(plane.alt) > 500.0 {
            tracing::trace!(alt_in_ft = m_to_ft(plane.alt), "ignore planes above 500ft");
            continue;
        }

        let mut ray_from_plane_to_carrier = DVec3::new(
            carrier.u - plane.u,
            carrier.alt - plane.alt,
            carrier.v - plane.v,
        );
        let distance = ray_from_plane_to_carrier.mag();

        // ignore planes farther away than 1.5nm
        if m_to_nm(distance) > 1.5 {
            tracing::trace!(
                distance_in_nm = m_to_nm(distance),
                "ignore planes farther away than 1.5nm"
            );
            continue;
        }

        // ignore takeoffs
        if distance < 70.0 {
            tracing::trace!(distance_in_m = distance, "ignore takeoffs");
            continue;
        }

        ray_from_plane_to_carrier.normalize();

        // Does the nose of the plane roughly point towards the carrier?
        let dot = plane.velocity.normalized().dot(ray_from_plane_to_carrier);
        if dot < 0.65 {
            tracing::trace!(dot, "ignore not roughly pointing towards the carrier");
            continue;
        }

        tracing::debug!(
            at = plane.time,
            dot,
            distance_in_nm = m_to_nm(distance),
            "found recovery attempt",
        );

        break;
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum Error {}
