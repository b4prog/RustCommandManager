#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use rcm::config::{CommandStep, Config, DEFAULT_CONFIG};
use rcm::runner::run_set;
use tempfile::tempdir;

fn config_for(steps: &[&str]) -> Config {
	let mut config = Config::parse("[commands.sample]\nsteps = ['true']").unwrap();
	config.settings.shell = "/bin/sh".to_owned();
	config.commands.get_mut("sample").unwrap().steps = steps
		.iter()
		.map(|step| CommandStep {
			command: (*step).to_owned(),
			description: String::new(),
		})
		.collect();
	config
}

fn run_steps(directory: &Path, steps: &[&str], stop_on_error: bool) -> Output {
	let config_directory = tempdir().unwrap();
	let path = config_directory.path().join("config.toml");
	let steps = toml::Value::Array(
		steps
			.iter()
			.map(|step| toml::Value::String((*step).to_owned()))
			.collect(),
	);
	fs::write(&path, format!("[settings]\nshell = '/bin/sh'\nstop_on_error = {stop_on_error}\n[commands.sample]\nsteps = {steps}\n")).unwrap();
	Command::new(env!("CARGO_BIN_EXE_rcm"))
		.env_remove("RCM_CONFIG")
		.current_dir(directory)
		.arg("--config")
		.arg(path)
		.args(["run", "sample"])
		.output()
		.unwrap()
}

#[test]
fn steps_run_in_order_in_the_invocation_folder_with_spaces() {
	let temporary = tempdir().unwrap();
	let directory = temporary.path().join("project with spaces");
	fs::create_dir(&directory).unwrap();
	let output = run_steps(
		&directory,
		&["printf first > result", "printf second | cat >> result"],
		true,
	);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(
		fs::read_to_string(directory.join("result")).unwrap(),
		"firstsecond"
	);
}

#[test]
fn execution_stops_at_the_first_failure_by_default() {
	let directory = tempdir().unwrap();
	let output = run_steps(
		directory.path(),
		&["exit 7", "touch should-not-exist"],
		true,
	);
	assert_eq!(output.status.code(), Some(7));
	assert!(!directory.path().join("should-not-exist").exists());
}

#[test]
fn continuing_after_failure_preserves_the_first_failure_code() {
	let directory = tempdir().unwrap();
	let output = run_steps(
		directory.path(),
		&["exit 7", "touch continued", "exit 9"],
		false,
	);
	assert_eq!(output.status.code(), Some(7));
	assert!(directory.path().join("continued").exists());
}

#[test]
fn every_step_starts_in_the_invocation_folder_even_after_a_previous_step_changes_folder() {
	let directory = tempdir().unwrap();
	let output = run_steps(
		directory.path(),
		&["pwd > first", "mkdir nested && cd nested", "pwd > last"],
		true,
	);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	for file in ["first", "last"] {
		let recorded = fs::read_to_string(directory.path().join(file)).unwrap();
		assert_eq!(
			fs::canonicalize(recorded.trim()).unwrap(),
			directory.path().canonicalize().unwrap()
		);
	}
}

#[test]
fn each_step_uses_a_separate_shell() {
	let directory = tempdir().unwrap();
	let inherited_value = std::env::var("RCM_TEST_LOCAL").unwrap_or_else(|_| "unset".to_owned());
	let output = run_steps(
		directory.path(),
		&[
			"export RCM_TEST_LOCAL=changed",
			"printf '%s' \"${RCM_TEST_LOCAL-unset}\" > result",
		],
		true,
	);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
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
	let mut config = config_for(&["true"]);
	config.settings.shell = directory.path().join("missing-shell").display().to_string();
	let error = run_set(&config, "sample").unwrap_err();
	assert!(error.to_string().contains("step 1"));
	assert!(format!("{error:#}").contains("could not start shell"));
}
