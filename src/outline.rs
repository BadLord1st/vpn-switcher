use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;

use crate::types::Config;

pub struct SwitchCommandOutput {
	pub status_success: bool,
	pub status_text: String,
	pub command: Vec<String>,
	pub stdout: String,
	pub stderr: String,
}

pub async fn discover_profiles(config: &Config) -> Result<Vec<String>> {
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

pub async fn switch_profile(config: &Config, profile: &str) -> Result<SwitchCommandOutput> {
	let mut args = config.command_args_prefix.clone();
	args.push(profile.to_string());

	let output = Command::new(&config.command_bin)
		.args(&args)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.output()
		.await
		.context("failed to execute switch command")?;

	let stdout = String::from_utf8_lossy(&output.stdout).to_string();
	let stderr = String::from_utf8_lossy(&output.stderr).to_string();

	let mut command = vec![config.command_bin.clone()];
	command.extend(args);

	Ok(SwitchCommandOutput {
		status_success: output.status.success(),
		status_text: output.status.to_string(),
		command,
		stdout,
		stderr,
	})
}
