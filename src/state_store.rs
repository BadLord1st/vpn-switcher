use anyhow::{Context, Result};
use tokio::fs;

use crate::types::{Config, PersistedState};

pub async fn load_or_init_state(config: &Config, profiles_len: usize) -> Result<PersistedState> {
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
			let state: PersistedState = serde_json::from_str(&raw).with_context(|| {
				format!("failed parsing state file {}", config.state_path.display())
			})?;
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

pub async fn save_state(config: &Config, state: &PersistedState) -> Result<()> {
	let payload = serde_json::to_string_pretty(state).context("failed to encode state JSON")?;
	fs::write(&config.state_path, payload)
		.await
		.with_context(|| format!("failed writing state file {}", config.state_path.display()))
}
