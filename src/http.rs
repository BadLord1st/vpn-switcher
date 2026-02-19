use axum::{
	extract::State,
	http::StatusCode,
	response::IntoResponse,
	routing::{get, post},
	Json, Router,
};
use serde::Serialize;
use tracing::{error, info};

use crate::{
	outline, state_store,
	types::{AppState, PersistedState},
};

#[derive(Debug, Serialize)]
struct SwitchResponse {
	client: String,
	profile: String,
	profile_index: usize,
	command: Vec<String>,
	stdout: String,
	stderr: String,
}

#[derive(Debug, Serialize)]
struct StateResponse {
	state: PersistedState,
	discovered_profiles: Vec<String>,
}

pub fn router(state: AppState) -> Router {
	Router::new()
		.route("/healthz", get(healthz))
		.route("/state", get(get_state))
		.route("/switch", post(switch_vpn))
		.with_state(state)
}

async fn healthz() -> impl IntoResponse {
	(StatusCode::OK, "ok")
}

async fn get_state(State(app): State<AppState>) -> impl IntoResponse {
	let _lock = app.state_file_lock.lock().await;

	let profiles = match outline::discover_profiles(&app.config).await {
		Ok(p) => p,
		Err(e) => {
			error!("profile discovery error: {e:#}");
			return (
				StatusCode::BAD_GATEWAY,
				format!("failed to discover outline profiles: {e:#}"),
			)
				.into_response();
		}
	};

	match state_store::load_or_init_state(&app.config, profiles.len()).await {
		Ok(state) => {
			let body = StateResponse {
				state,
				discovered_profiles: profiles,
			};
			(StatusCode::OK, Json(body)).into_response()
		}
		Err(e) => {
			error!("state read error: {e:#}");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				format!("failed to read state: {e:#}"),
			)
				.into_response()
		}
	}
}

async fn switch_vpn(State(app): State<AppState>) -> impl IntoResponse {
	let _lock = app.state_file_lock.lock().await;

	let profiles = match outline::discover_profiles(&app.config).await {
		Ok(p) => p,
		Err(e) => {
			error!("profile discovery error: {e:#}");
			return (
				StatusCode::BAD_GATEWAY,
				format!("failed to discover outline profiles: {e:#}"),
			)
				.into_response();
		}
	};

	let mut state = match state_store::load_or_init_state(&app.config, profiles.len()).await {
		Ok(s) => s,
		Err(e) => {
			error!("state load error: {e:#}");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				format!("failed to load state: {e:#}"),
			)
				.into_response();
		}
	};

	let current_index = state.last_profile_index % profiles.len();
	let next_index = (current_index + 1) % profiles.len();
	let next_profile = profiles[next_index].clone();

	info!(
		"switching outline profile: index={} profile={}",
		next_index, next_profile
	);

	let cmd_output = match outline::switch_profile(&app.config, &next_profile).await {
		Ok(o) => o,
		Err(e) => {
			error!("command execution error: {e:#}");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				format!("failed to execute command: {e:#}"),
			)
				.into_response();
		}
	};

	if !cmd_output.status_success {
		error!(
			"command failed status={} stdout={} stderr={}",
			cmd_output.status_text, cmd_output.stdout, cmd_output.stderr
		);
		return (
			StatusCode::BAD_GATEWAY,
			Json(serde_json::json!({
				"error": "vpn command failed",
				"status": cmd_output.status_text,
				"stdout": cmd_output.stdout,
				"stderr": cmd_output.stderr
			})),
		)
			.into_response();
	}

	state.last_client = "outline".to_string();
	state.last_profile_index = next_index;

	if let Err(e) = state_store::save_state(&app.config, &state).await {
		error!("failed to save state: {e:#}");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			format!("VPN switched but failed to persist state: {e:#}"),
		)
			.into_response();
	}

	(
		StatusCode::OK,
		Json(SwitchResponse {
			client: "outline".to_string(),
			profile: next_profile,
			profile_index: next_index,
			command: cmd_output.command,
			stdout: cmd_output.stdout,
			stderr: cmd_output.stderr,
		}),
	)
		.into_response()
}
