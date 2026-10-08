use std::path::Path;

use anyhow::Result;

use crate::cli::{Cli, Commands};
use crate::config::{Config, config_path, initialize_config};
use crate::editor::open_config;
use crate::runner::run_set;

/// Initialize the configuration if needed, then perform the requested action.
pub fn run(cli: Cli) -> Result<i32> {
	let path = config_path(cli.config)?;
	if matches!(cli.command, Some(Commands::Config { path: true })) {
		println!("{}", path.display());
		return Ok(0);
	}
	if prepare_config(&path)? {
		return Ok(0);
	}
	dispatch(cli.command, &path)
}

fn prepare_config(path: &Path) -> Result<bool> {
	if !initialize_config(path)? {
		return Ok(false);
	}
	println!("Created configuration at {}", path.display());
	open_config(path)?;
	println!("Save your configuration, then run `rcm list` or `rcm run <name>`.");
	Ok(true)
}

fn dispatch(command: Option<Commands>, path: &Path) -> Result<i32> {
	match command {
		Some(Commands::Config { .. }) => {
			open_config(path)?;
			Ok(0)
		}
		Some(Commands::Run { name }) => run_set(&Config::load(path)?, &name),
		Some(Commands::List) | None => {
			list_commands(&Config::load(path)?);
			Ok(0)
		}
	}
}

fn list_commands(config: &Config) {
	if config.commands.is_empty() {
		println!("No command sets configured. Add one with `rcm config`.");
		return;
	}
	for (name, command) in &config.commands {
		println!("{name}\t{}", command.description);
	}
}
