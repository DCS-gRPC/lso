use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tonic::transport::Channel;

use crate::utils::shutdown::ShutdownHandle;

pub mod detect_recovery;
pub mod record_recovery;

#[derive(Clone)]
pub struct TaskParams<'a> {
    pub out_dir: &'a Path,
    pub discord_webhook: Option<String>,
    pub users: Arc<HashMap<String, u64>>,
    pub ch: Channel,
    pub carrier_name: &'a str,
    pub plane_name: &'a str,
    pub pilot_name: &'a str,
    pub shutdown: ShutdownHandle,
}
