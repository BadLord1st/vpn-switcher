use std::{path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Config {
	pub listen_addr: std::net::SocketAddr,
	pub state_path: PathBuf,
	pub command_bin: String,
	pub list_args: Vec<String>,
	pub command_args_prefix: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedState {
	pub last_client: String,
	pub last_profile_index: usize,
}

#[derive(Clone)]
pub struct AppState {
	pub config: Config,
	pub state_file_lock: Arc<Mutex<()>>,
}
