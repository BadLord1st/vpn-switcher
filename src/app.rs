use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use tracing::info;

use crate::{config, http, types::AppState};

pub async fn app_main() -> Result<()> {
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.init();

	let config = config::parse_env()?;
	info!("starting vpn-switcher on {}", config.listen_addr);

	let state = AppState {
		config,
		state_file_lock: Arc::new(Mutex::new(())),
	};

	let router = http::router(state.clone());

	let listener = tokio::net::TcpListener::bind(state.config.listen_addr)
		.await
		.context("failed to bind TCP listener")?;

	axum::serve(listener, router)
		.with_graceful_shutdown(shutdown_signal())
		.await
		.context("HTTP server exited unexpectedly")?;

	Ok(())
}

async fn shutdown_signal() {
	#[cfg(unix)]
	{
		use tokio::signal::unix::{signal, SignalKind};

		let mut term = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
		tokio::select! {
			_ = tokio::signal::ctrl_c() => {}
			_ = term.recv() => {}
		}
	}

	#[cfg(not(unix))]
	{
		tokio::signal::ctrl_c()
			.await
			.expect("failed to install Ctrl+C handler");
	}

	info!("shutdown signal received");
}
