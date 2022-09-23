use std::time::Duration;

use futures_util::StreamExt;
use tonic::Code;

use crate::client::UnitClient;
use crate::transform::Transform;
use crate::utils::{m_to_ft, m_to_nm};

use super::TaskParams;

#[tracing::instrument(
    skip_all,
    fields(carrier_name = params.carrier_name, plane_name = params.plane_name)
)]
pub async fn detect_recovery_attempt(params: TaskParams<'_>) -> Result<(), crate::error::Error> {
    tracing::debug!("started observing for possible recovery attempts");

    let mut client1 = UnitClient::new(params.ch.clone());
    let mut client2 = UnitClient::new(params.ch.clone());
    let mut interval =
        crate::utils::interval::interval(Duration::from_secs(2), params.shutdown.clone());

    while interval.next().await.is_some() {
        let result = futures_util::future::try_join(
            client1.get_transform(params.carrier_name),
            client2.get_transform(params.plane_name),
        )
        .await;

        match result {
            Ok((carrier, plane)) => {
                if is_recovery_attempt(&carrier, &plane) {
                    super::record_recovery::record_recovery(params.clone()).await?;
                }
            }
            Err(status) if status.code() == Code::NotFound => {
                tracing::debug!("stop tracking as either carrier or plane doesn't exist anymore");
                return Ok(());
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    Ok(())
}

pub fn is_recovery_attempt(carrier: &Transform, plane: &Transform) -> bool {
    // ignore planes above 500ft
    if m_to_ft(plane.alt) > 500.0 {
        tracing::trace!(alt_in_ft = m_to_ft(plane.alt), "ignore planes above 500ft");
        return false;
    }

    let ray_from_plane_to_carrier = carrier.position - plane.position;
    let distance = ray_from_plane_to_carrier.mag();

    // ignore planes farther away than 1.5nm
    if m_to_nm(distance) > 1.5 {
        tracing::trace!(
            distance_in_nm = m_to_nm(distance),
            "ignore planes farther away than 1.5nm"
        );
        return false;
    }

    // ignore takeoffs
    if distance < 200.0 {
        tracing::trace!(distance_in_m = distance, "ignore takeoffs");
        return false;
    }

    // is the plane behind the carrier
    let dot = carrier
        .forward
        .normalized()
        .dot(ray_from_plane_to_carrier.normalized());
    if dot < 0.0 {
        tracing::trace!(dot, "ignore not behind the carrier");
        return false;
    }

    // Does the nose of the plane roughly point towards the carrier?
    let dot = plane
        .velocity
        .normalized()
        .dot(ray_from_plane_to_carrier.normalized());
    if dot < 0.65 {
        tracing::trace!(dot, "ignore not roughly pointing towards the carrier");
        return false;
    }

    tracing::debug!(
        at = plane.time,
        dot,
        distance_in_m = distance,
        distance_in_nm = m_to_nm(distance),
        "found recovery attempt",
    );

    true
}
