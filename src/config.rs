use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

/// The configuration written on first run, shared with the documented example.
pub const DEFAULT_CONFIG: &str = include_str!("../examples/config.toml");

/// User settings and named command sets loaded from TOML.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
	#[serde(default)]
	pub settings: Settings,
	#[serde(default)]
	pub commands: BTreeMap<String, CommandSet>,
}

/// Defaults applied to every command set.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
	pub shell: String,
	pub stop_on_error: bool,
	pub scroll_lines: u16,
	pub working_dir: Option<PathBuf>,
}

/// Shell commands executed sequentially under one name.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSet {
	#[serde(default)]
	pub description: String,
	pub steps: Vec<CommandStep>,
	pub working_dir: Option<PathBuf>,
}

/// One shell command and its explanation, displayed when running verbosely.
#[derive(Debug, Deserialize)]
#[serde(try_from = "StepDefinition")]
pub struct CommandStep {
	pub command: String,
	pub description: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StepDefinition {
	Legacy(String),
	Described(StepDetails),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepDetails {
	command: String,
	description: String,
}

impl TryFrom<StepDefinition> for CommandStep {
	type Error = anyhow::Error;
	fn try_from(definition: StepDefinition) -> Result<Self> {
		match definition {
			StepDefinition::Legacy(command) => Ok(Self {
				command,
				description: String::new(),
			}),
			StepDefinition::Described(step) => {
				ensure!(
					!step.description.trim().is_empty(),
					"step description must not be empty"
				);
				Ok(Self {
					command: step.command,
					description: step.description,
				})
			}
		}
	}
}

impl Default for Settings {
	fn default() -> Self {
		Self {
			shell: default_shell(),
			stop_on_error: true,
			scroll_lines: 5,
			working_dir: None,
		}
	}
}

fn default_shell() -> String {
	std::env::var("SHELL")
		.ok()
		.filter(|shell| !shell.trim().is_empty())
		.unwrap_or_else(|| "/bin/sh".to_owned())
}

impl Config {
	/// Parse TOML and validate every command set before it can be executed.
	pub fn parse(source: &str) -> Result<Self> {
		let config: Self = toml::from_str(source).context("invalid TOML configuration")?;
		ensure!(
			!config.settings.shell.trim().is_empty(),
			"settings.shell must not be empty"
		);
		validate_working_dir(config.settings.working_dir.as_deref())?;
		ensure!(
			config.settings.scroll_lines > 0,
			"settings.scroll_lines must be greater than zero"
		);
		for (name, command) in &config.commands {
			validate_command(name, command)?;
		}
		Ok(config)
	}

	/// Read and validate a configuration file without changing it.
	pub fn load(path: &Path) -> Result<Self> {
		let source = fs::read_to_string(path)
			.with_context(|| format!("could not read configuration {}", path.display()))?;
		Self::parse(&source).with_context(|| format!("could not load configuration {}", path.display()))
	}
}

fn validate_command(name: &str, command: &CommandSet) -> Result<()> {
	ensure!(
		!name.trim().is_empty(),
		"command set names must not be empty"
	);
	ensure!(
		!command.steps.is_empty(),
		"command set {name:?} must contain at least one step"
	);
	ensure!(
		command
			.steps
			.iter()
			.all(|step| !step.command.trim().is_empty()),
		"command set {name:?} contains an empty step"
	);
	validate_working_dir(command.working_dir.as_deref())
		.with_context(|| format!("invalid command set {name:?}"))
}

fn validate_working_dir(path: Option<&Path>) -> Result<()> {
	ensure!(
		path.is_none_or(|path| !path.as_os_str().is_empty()),
		"working_dir must not be empty"
	);
	Ok(())
}

/// Resolve an override or the platform's standard user configuration path.
pub fn config_path(override_path: Option<PathBuf>) -> Result<PathBuf> {
	let path = match override_path {
		Some(path) => path,
		None => dirs::config_dir()
			.context("could not locate the user's configuration directory; use --config PATH")?
			.join("rcm")
			.join("config.toml"),
	};
	ensure!(
		!path.as_os_str().is_empty(),
		"configuration path must not be empty"
	);
	std::path::absolute(path).context("could not resolve the configuration path")
}

/// Create a missing configuration and return whether it was created.
///
/// Exclusive creation preserves existing files, including concurrent creations.
pub fn initialize_config(path: &Path) -> Result<bool> {
	if let Some(parent) = path
		.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
	{
		fs::create_dir_all(parent).with_context(|| {
			format!(
				"could not create configuration directory {}",
				parent.display()
			)
		})?;
	}
	let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
		Ok(file) => file,
		Err(error) if error.kind() == ErrorKind::AlreadyExists => return Ok(false),
		Err(error) => {
			return Err(error)
				.with_context(|| format!("could not create configuration {}", path.display()));
		}
	};
	file
		.write_all(DEFAULT_CONFIG.as_bytes())
		.with_context(|| format!("could not write configuration {}", path.display()))?;
	Ok(true)
}
