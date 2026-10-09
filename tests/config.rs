use std::fs;
use std::path::PathBuf;

use rcm::config::{Config, DEFAULT_CONFIG, config_path, initialize_config};
use tempfile::tempdir;

#[test]
fn shipped_template_has_described_brew_update_steps() {
	let config = Config::parse(DEFAULT_CONFIG).unwrap();
	assert!(!config.settings.shell.trim().is_empty());
	assert!(config.settings.stop_on_error);
	assert_eq!(config.settings.scroll_lines, 5);
	assert_eq!(config.commands.len(), 1);
	assert_eq!(
		config.commands["brew-update"]
			.steps
			.iter()
			.map(|step| step.command.as_str())
			.collect::<Vec<_>>(),
		[
			"brew update",
			"brew upgrade --greedy --no-ask",
			"brew cleanup -s"
		]
	);
	assert!(
		config
			.commands
			.values()
			.flat_map(|command| &command.steps)
			.all(|step| !step.description.trim().is_empty())
	);
	assert!(
		config.commands["brew-update"].steps[0]
			.description
			.contains("The following upgrade")
	);
	assert!(
		config.commands["brew-update"].steps[1]
			.description
			.contains("--no-ask skips confirmation")
	);
	assert!(
		config.commands["brew-update"].steps[2]
			.description
			.contains("-s also removes downloads")
	);
}

#[test]
fn omitted_settings_use_defaults() {
	let config = Config::parse("[commands.sample]\nsteps = ['echo sample']").unwrap();
	assert!(!config.settings.shell.trim().is_empty());
	assert!(config.settings.stop_on_error);
	assert!(config.commands["sample"].description.is_empty());
	assert_eq!(config.commands["sample"].steps[0].command, "echo sample");
	assert!(config.commands["sample"].steps[0].description.is_empty());
}

#[test]
fn described_steps_support_multiline_descriptions_and_array_tables() {
	let config = Config::parse(
		r#"[commands.sample]
[[commands.sample.steps]]
command = 'printf first'
description = """
Print the first message.
Then move on to the next step.
"""
[[commands.sample.steps]]
command = 'printf second'
description = 'Print the second message.'"#,
	)
	.unwrap();
	let steps = &config.commands["sample"].steps;
	assert_eq!(steps.len(), 2);
	assert_eq!(steps[0].command, "printf first");
	assert_eq!(
		steps[0].description,
		"Print the first message.\nThen move on to the next step.\n"
	);
	assert_eq!(steps[1].command, "printf second");
	assert_eq!(steps[1].description, "Print the second message.");
}

#[test]
fn described_inline_steps_can_be_mixed_with_existing_string_steps() {
	let config = Config::parse(
		"[commands.sample]\nsteps = ['printf legacy', { command = 'printf described', description = 'Explain this step.' }]",
	)
	.unwrap();
	let steps = &config.commands["sample"].steps;
	assert_eq!(steps[0].command, "printf legacy");
	assert!(steps[0].description.is_empty());
	assert_eq!(steps[1].command, "printf described");
	assert_eq!(steps[1].description, "Explain this step.");
}

#[test]
fn settings_and_command_details_are_loaded() {
	let config = Config::parse(
		"[settings]\nshell = '/bin/zsh'\nstop_on_error = false\n\
		[commands.sample]\ndescription = 'Sample'\nsteps = ['pwd']",
	)
	.unwrap();
	assert_eq!(config.settings.shell, "/bin/zsh");
	assert!(!config.settings.stop_on_error);
	assert_eq!(config.commands["sample"].description, "Sample");
	assert_eq!(config.commands["sample"].steps[0].command, "pwd");
}

#[test]
fn invalid_configuration_is_rejected_before_execution() {
	let cases = [
		("[broken", "invalid TOML"),
		("unknown = true", "unknown field"),
		("[settings]\nstop_on_eror = false", "unknown field"),
		(
			"[settings]\nscroll_lines = 0",
			"settings.scroll_lines must be greater than zero",
		),
		(
			"[settings]\nshell = '  '",
			"settings.shell must not be empty",
		),
		(
			"[commands.' ']\nsteps = ['true']",
			"names must not be empty",
		),
		("[commands.empty]\nsteps = []", "at least one step"),
		("[commands.empty]\nsteps = ['  ']", "empty step"),
		(
			"[commands.empty]\nsteps = [{ command = '  ', description = 'Empty command.' }]",
			"empty step",
		),
		(
			"[commands.empty]\nsteps = [{ command = 'true', description = '  ' }]",
			"step description must not be empty",
		),
		(
			"[commands.empty]\nsteps = [{ command = 'true' }]",
			"invalid TOML",
		),
		(
			"[commands.empty]\nsteps = [{ description = 'Missing command.' }]",
			"invalid TOML",
		),
		(
			"[commands.typo]\nsteps = [{ command = 'true', description = 'Sample', extra = true }]",
			"invalid TOML",
		),
		(
			"[commands.empty]\ndescription = 'Missing steps'",
			"missing field",
		),
		(
			"[commands.typo]\nsteps = ['true']\nstep = 'false'",
			"unknown field",
		),
	];
	for (source, expected_error) in cases {
		let error = Config::parse(source).unwrap_err();
		assert!(
			format!("{error:#}").contains(expected_error),
			"{source}: {error:#}"
		);
	}
}

#[test]
fn initialization_creates_nested_directories_and_the_template() {
	let directory = tempdir().unwrap();
	let path = directory.path().join("Application Support/rcm/config.toml");
	assert!(initialize_config(&path).unwrap());
	assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
	let config = Config::load(&path).unwrap();
	assert_eq!(
		config.commands["brew-update"]
			.steps
			.iter()
			.map(|step| step.command.as_str())
			.collect::<Vec<_>>(),
		[
			"brew update",
			"brew upgrade --greedy --no-ask",
			"brew cleanup -s"
		]
	);
	assert!(
		config.commands["brew-update"]
			.steps
			.iter()
			.all(|step| !step.description.is_empty())
	);
}

#[test]
fn initialization_preserves_an_existing_file_even_when_invalid() {
	let directory = tempdir().unwrap();
	let path = directory.path().join("config.toml");
	fs::write(&path, "keep this user content").unwrap();
	assert!(!initialize_config(&path).unwrap());
	assert_eq!(fs::read_to_string(&path).unwrap(), "keep this user content");
}

#[test]
fn initialization_reports_an_unusable_parent_directory() {
	let directory = tempdir().unwrap();
	let parent = directory.path().join("a-file");
	fs::write(&parent, "file").unwrap();
	let error = initialize_config(&parent.join("config.toml")).unwrap_err();
	assert!(
		error
			.to_string()
			.contains("could not create configuration directory")
	);
	assert_eq!(fs::read_to_string(&parent).unwrap(), "file");
}

#[test]
fn loading_errors_include_the_configuration_path() {
	let directory = tempdir().unwrap();
	let path = directory.path().join("config.toml");
	let missing = Config::load(&path).unwrap_err();
	assert!(missing.to_string().contains(&path.display().to_string()));
	fs::write(&path, "[broken").unwrap();
	let invalid = Config::load(&path).unwrap_err();
	assert!(invalid.to_string().contains(&path.display().to_string()));
	assert!(format!("{invalid:#}").contains("invalid TOML"));
}

#[test]
fn configuration_overrides_resolve_to_absolute_paths() {
	let relative = PathBuf::from("custom.toml");
	assert_eq!(
		config_path(Some(relative.clone())).unwrap(),
		std::path::absolute(relative).unwrap()
	);
	assert!(config_path(Some(PathBuf::new())).is_err());
}

#[test]
fn default_configuration_uses_the_standard_platform_directory() {
	let expected = dirs::config_dir().unwrap().join("rcm/config.toml");
	assert_eq!(config_path(None).unwrap(), expected);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_configuration_is_in_application_support() {
	let expected = dirs::home_dir()
		.unwrap()
		.join("Library/Application Support/rcm/config.toml");
	assert_eq!(config_path(None).unwrap(), expected);
}
