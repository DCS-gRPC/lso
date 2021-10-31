use std::time::Duration;

use futures_util::Stream;
use tokio::time::Instant;

use super::shutdown::ShutdownHandle;

pub fn interval(period: Duration, shutdown: ShutdownHandle) -> impl Stream<Item = Instant> {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let stream = futures_util::stream::poll_fn(move |cx| interval.poll_tick(cx).map(Some));
    shutdown.wrap_stream(stream)
}
