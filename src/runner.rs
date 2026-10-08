use std::path::Path;
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result};

use crate::config::Config;

/// Run a named command set, returning zero or its first failing step's exit code.
pub fn run_set(config: &Config, name: &str) -> Result<i32> {
	let command = config.commands.get(name).with_context(|| {
		format!("unknown command set {name:?}; use `rcm list` to see available sets")
	})?;
	let working_dir = command
		.working_dir
		.as_deref()
		.or(config.settings.working_dir.as_deref());
	let mut first_failure = None;
	for (index, step) in command.steps.iter().enumerate() {
		eprintln!("[{name} {}/{}] {step}", index + 1, command.steps.len());
		let status = execute_step(&config.settings.shell, step, working_dir)
			.with_context(|| format!("could not run step {} of {name:?}", index + 1))?;
		if status.success() {
			continue;
		}
		first_failure.get_or_insert(status.code().unwrap_or(1));
		if config.settings.stop_on_error {
			break;
		}
	}
	Ok(first_failure.unwrap_or(0))
}

fn execute_step(shell: &str, step: &str, working_dir: Option<&Path>) -> Result<ExitStatus> {
	let mut command = Command::new(shell);
	command.arg("-c").arg(step);
	if let Some(directory) = working_dir {
		command.current_dir(directory);
	}
	command
		.status()
		.with_context(|| format!("could not start shell {shell:?}"))
}
