#![cfg(unix)]

use std::fs;
use std::path::Path;

use rcm::config::{Config, DEFAULT_CONFIG};
use rcm::runner::run_set;
use tempfile::tempdir;

fn config_for(directory: &Path, steps: &[&str]) -> Config {
	let mut config = Config::parse(DEFAULT_CONFIG).unwrap();
	config.settings.shell = "/bin/sh".to_owned();
	config.settings.working_dir = Some(directory.to_path_buf());
	config.commands.get_mut("hello").unwrap().steps =
		steps.iter().map(|step| (*step).to_owned()).collect();
	config
}

#[test]
fn steps_run_in_order_with_shell_features_and_a_directory_containing_spaces() {
	let temporary = tempdir().unwrap();
	let directory = temporary.path().join("project with spaces");
	fs::create_dir(&directory).unwrap();
	let config = config_for(
		&directory,
		&["printf first > result", "printf second | cat >> result"],
	);
	assert_eq!(run_set(&config, "hello").unwrap(), 0);
	assert_eq!(
		fs::read_to_string(directory.join("result")).unwrap(),
		"firstsecond"
	);
}

#[test]
fn execution_stops_at_the_first_failure_by_default() {
	let directory = tempdir().unwrap();
	let config = config_for(directory.path(), &["exit 7", "touch should-not-exist"]);
	assert_eq!(run_set(&config, "hello").unwrap(), 7);
	assert!(!directory.path().join("should-not-exist").exists());
}

#[test]
fn continuing_after_failure_preserves_the_first_failure_code() {
	let directory = tempdir().unwrap();
	let mut config = config_for(directory.path(), &["exit 7", "touch continued", "exit 9"]);
	config.settings.stop_on_error = false;
	assert_eq!(run_set(&config, "hello").unwrap(), 7);
	assert!(directory.path().join("continued").exists());
}

#[test]
fn command_directory_overrides_the_global_directory() {
	let global = tempdir().unwrap();
	let local = tempdir().unwrap();
	let mut config = config_for(global.path(), &["touch result"]);
	config.commands.get_mut("hello").unwrap().working_dir = Some(local.path().to_path_buf());
	assert_eq!(run_set(&config, "hello").unwrap(), 0);
	assert!(local.path().join("result").exists());
	assert!(!global.path().join("result").exists());
}

#[test]
fn each_step_uses_a_separate_shell() {
	let directory = tempdir().unwrap();
	let inherited_value = std::env::var("RCM_TEST_LOCAL").unwrap_or_else(|_| "unset".to_owned());
	let config = config_for(
		directory.path(),
		&[
			"export RCM_TEST_LOCAL=changed",
			"printf '%s' \"${RCM_TEST_LOCAL-unset}\" > result",
		],
	);
	assert_eq!(run_set(&config, "hello").unwrap(), 0);
	assert_eq!(
		fs::read_to_string(directory.path().join("result")).unwrap(),
		inherited_value
	);
}

#[test]
fn unknown_command_sets_have_an_actionable_error() {
	let config = Config::parse(DEFAULT_CONFIG).unwrap();
	let error = run_set(&config, "missing").unwrap_err();
	assert!(error.to_string().contains("unknown command set"));
	assert!(error.to_string().contains("rcm list"));
}

#[test]
fn shell_startup_errors_identify_the_command_and_step() {
	let directory = tempdir().unwrap();
	let mut config = config_for(directory.path(), &["true"]);
	config.settings.shell = directory.path().join("missing-shell").display().to_string();
	let error = run_set(&config, "hello").unwrap_err();
	assert!(error.to_string().contains("step 1"));
	assert!(format!("{error:#}").contains("could not start shell"));
}

#[test]
fn a_missing_working_directory_fails_without_running_the_step() {
	let directory = tempdir().unwrap();
	let config = config_for(&directory.path().join("missing"), &["true"]);
	assert!(run_set(&config, "hello").is_err());
}
