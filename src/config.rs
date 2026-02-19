use std::{env, path::PathBuf};

use anyhow::{Context, Result};

use crate::types::Config;

pub fn parse_env() -> Result<Config> {
	let listen_addr: std::net::SocketAddr = env::var("LISTEN_ADDR")
		.unwrap_or_else(|_| "0.0.0.0:8080".to_string())
		.parse()
		.context("failed to parse LISTEN_ADDR")?;

	let state_path = PathBuf::from(
		env::var("STATE_PATH").unwrap_or_else(|_| "./state/vpn-switcher-state.json".to_string()),
	);

	let command_bin = env::var("OUTLINE_COMMAND_BIN").unwrap_or_else(|_| "vpn".to_string());

	let list_args = env::var("OUTLINE_LIST_ARGS")
		.unwrap_or_else(|_| "list -f %name%".to_string())
		.split_whitespace()
		.map(ToString::to_string)
		.collect::<Vec<_>>();

	let command_args_prefix = env::var("OUTLINE_COMMAND_PREFIX")
		.unwrap_or_else(|_| "connect".to_string())
		.split_whitespace()
		.map(ToString::to_string)
		.collect::<Vec<_>>();

	Ok(Config {
		listen_addr,
		state_path,
		command_bin,
		list_args,
		command_args_prefix,
	})
}

pub fn runtime_workers() -> usize {
	env::var("TOKIO_WORKER_THREADS")
		.ok()
		.and_then(|v| v.parse::<usize>().ok())
		.map(|v| v.clamp(1, 2))
		.unwrap_or(2)
}
