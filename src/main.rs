use std::{
    env,
    net::SocketAddr,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
};

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    process::Command,
    sync::Mutex,
};
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    config: Config,
    state_file_lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug)]
struct Config {
    listen_addr: SocketAddr,
    state_path: PathBuf,
    command_bin: String,
    list_args: Vec<String>,
    command_args_prefix: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedState {
    last_client: String,
    last_profile_index: usize,
}

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

fn parse_env() -> Result<Config> {
    let listen_addr: SocketAddr = env::var("LISTEN_ADDR")
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

async fn discover_outline_profiles(config: &Config) -> Result<Vec<String>> {
    let output = Command::new(&config.command_bin)
        .args(&config.list_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| {
            format!(
                "failed to execute profile discovery command: {} {:?}",
                config.command_bin, config.list_args
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        anyhow::bail!(
            "profile discovery command failed: status={} stderr={} stdout={}",
            output.status,
            stderr,
            stdout
        );
    }

    let profiles = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if profiles.is_empty() {
        anyhow::bail!(
            "no outline profiles discovered; ensure access keys exist and OUTLINE_LIST_ARGS is correct"
        );
    }

    Ok(profiles)
}

async fn load_or_init_state(config: &Config, profiles_len: usize) -> Result<PersistedState> {
    if profiles_len == 0 {
        anyhow::bail!("profiles_len must be > 0");
    }

    if let Some(parent) = config.state_path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed creating state dir: {}", parent.display()))?;
    }

    match fs::read_to_string(&config.state_path).await {
        Ok(raw) => {
            let state: PersistedState = serde_json::from_str(&raw)
                .with_context(|| format!("failed parsing state file {}", config.state_path.display()))?;
            Ok(state)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let initial = PersistedState {
                last_client: "outline".to_string(),
                last_profile_index: profiles_len.saturating_sub(1),
            };
            save_state(config, &initial).await?;
            Ok(initial)
        }
        Err(e) => Err(e).with_context(|| format!("failed reading {}", config.state_path.display())),
    }
}

async fn save_state(config: &Config, state: &PersistedState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("failed to encode state JSON")?;
    fs::write(&config.state_path, payload)
        .await
        .with_context(|| format!("failed writing state file {}", config.state_path.display()))
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn get_state(State(app): State<AppState>) -> impl IntoResponse {
    let _lock = app.state_file_lock.lock().await;
    let profiles = match discover_outline_profiles(&app.config).await {
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

    match load_or_init_state(&app.config, profiles.len()).await {
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
    let profiles = match discover_outline_profiles(&app.config).await {
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

    let mut state = match load_or_init_state(&app.config, profiles.len()).await {
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

    let mut args = app.config.command_args_prefix.clone();
    args.push(next_profile.clone());

    info!(
        "switching outline profile: index={} profile={} command={} {:?}",
        next_index, next_profile, app.config.command_bin, args
    );

    let output = match Command::new(&app.config.command_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
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

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        error!(
            "command failed status={} stdout={} stderr={}",
            output.status, stdout, stderr
        );
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": "vpn command failed",
                "status": output.status.to_string(),
                "stdout": stdout,
                "stderr": stderr
            })),
        )
            .into_response();
    }

    state.last_client = "outline".to_string();
    state.last_profile_index = next_index;

    if let Err(e) = save_state(&app.config, &state).await {
        error!("failed to save state: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("VPN switched but failed to persist state: {e:#}"),
        )
            .into_response();
    }

    let mut full_cmd = vec![app.config.command_bin.clone()];
    full_cmd.extend(args);

    (
        StatusCode::OK,
        Json(SwitchResponse {
            client: "outline".to_string(),
            profile: next_profile,
            profile_index: next_index,
            command: full_cmd,
            stdout,
            stderr,
        }),
    )
        .into_response()
}

async fn app_main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = parse_env()?;
    info!("starting vpn-switcher on {}", config.listen_addr);

    let state = AppState {
        config,
        state_file_lock: Arc::new(Mutex::new(())),
    };

    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/state", get(get_state))
        .route("/switch", post(switch_vpn))
        .with_state(state.clone());

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

fn main() {
    let workers = env::var("TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|v| v.clamp(1, 2))
        .unwrap_or(2);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(workers)
        .max_blocking_threads(2)
        .build()
        .expect("failed to build Tokio runtime");

    if let Err(err) = runtime.block_on(app_main()) {
        eprintln!("fatal error: {err:#}");
        std::process::exit(1);
    }
}
