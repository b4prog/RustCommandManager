use std::io::{self, IsTerminal};
use std::process::Command;

use anyhow::{Context, Result};

use crate::config::{CommandStep, Config};
use crate::status::CommandStatus;

/// Run a named command set, returning zero or its first failing step's exit code.
pub fn run_set(config: &Config, name: &str) -> Result<i32> {
	run_set_with_options(config, name, false)
}

/// Run a set with compact terminal output or an explicitly verbose transcript.
pub fn run_set_with_options(config: &Config, name: &str, verbose: bool) -> Result<i32> {
	let command = config.commands.get(name).with_context(|| {
		format!("unknown command set {name:?}; use `rcm list` to see available sets")
	})?;
	let mut first_failure = None;
	for (index, step) in command.steps.iter().enumerate() {
		let code = execute_step(
			&config.settings.shell,
			step,
			verbose,
			config.settings.scroll_lines,
		)
		.with_context(|| format!("could not run step {} of {name:?}", index + 1))?;
		if code == 0 {
			continue;
		}
		first_failure.get_or_insert(code);
		if command.stop_on_error {
			break;
		}
	}
	Ok(first_failure.unwrap_or(0))
}

fn execute_step(shell: &str, step: &CommandStep, verbose: bool, scroll_lines: u16) -> Result<i32> {
	#[cfg(unix)]
	if io::stdout().is_terminal() {
		return crate::terminal::execute(shell, step, verbose, scroll_lines);
	}
	#[cfg(not(unix))]
	let _ = scroll_lines;
	run_inherited(shell, step, verbose)
}

fn run_inherited(shell: &str, step: &CommandStep, verbose: bool) -> Result<i32> {
	let mut progress = CommandStatus::start(&step.command, &step.description, false, !verbose)?;
	let mut command = Command::new(shell);
	command.arg("-c").arg(&step.command);
	let result = command
		.status()
		.with_context(|| format!("could not start shell {shell:?}"));
	progress.finish(result.as_ref().is_ok_and(|status| status.success()))?;
	let status = result?;
	Ok(status.code().unwrap_or(1))
}
