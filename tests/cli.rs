#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use rcm::config::DEFAULT_CONFIG;
use tempfile::{TempDir, tempdir};

fn cli(path: &Path) -> Command {
	let mut command = Command::new(env!("CARGO_BIN_EXE_rcm"));
	command
		.env_remove("RCM_CONFIG")
		.env("SHELL", "/bin/sh")
		.arg("--config")
		.arg(path);
	command
}

fn fixture(source: &str) -> (TempDir, PathBuf) {
	let directory = tempdir().unwrap();
	let path = directory.path().join("config.toml");
	fs::write(&path, source).unwrap();
	(directory, path)
}

fn fake_opener(directory: &Path, exit_code: u8) -> PathBuf {
	let bin = directory.join("bin");
	fs::create_dir(&bin).unwrap();
	let name = if cfg!(target_os = "macos") {
		"open"
	} else {
		"xdg-open"
	};
	let executable = bin.join(name);
	fs::write(
		&executable,
		format!("#!/bin/sh\nprintf '%s' \"$1\" >> \"$RCM_TEST_OPEN_LOG\"\nexit {exit_code}\n"),
	)
	.unwrap();
	fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
	bin
}

fn with_fake_opener(command: &mut Command, directory: &Path, exit_code: u8) {
	let bin = fake_opener(directory, exit_code);
	command
		.env("PATH", bin)
		.env("RCM_TEST_OPEN_LOG", directory.join("opened"));
}

#[test]
fn plain_rcm_creates_and_opens_the_missing_default_configuration() {
	let directory = tempdir().unwrap();
	let home = directory.path().join("home");
	let path = if cfg!(target_os = "macos") {
		home.join("Library/Application Support/rcm/config.toml")
	} else {
		home.join(".config/rcm/config.toml")
	};
	let mut command = Command::new(env!("CARGO_BIN_EXE_rcm"));
	command
		.env_remove("RCM_CONFIG")
		.env("HOME", &home)
		.env("XDG_CONFIG_HOME", home.join(".config"));
	with_fake_opener(&mut command, directory.path(), 0);
	assert!(!path.parent().unwrap().exists());
	let output = command.output().unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(path.parent().unwrap().is_dir());
	assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
	assert_eq!(
		fs::read_to_string(directory.path().join("opened")).unwrap(),
		path.display().to_string()
	);
	assert!(
		String::from_utf8(output.stdout)
			.unwrap()
			.contains("Created configuration")
	);
	assert!(output.stderr.is_empty());
	let subsequent = command.output().unwrap();
	assert!(subsequent.status.success());
	assert!(
		String::from_utf8(subsequent.stdout)
			.unwrap()
			.contains("hello\tRun the sample command set")
	);
	assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
	assert_eq!(
		fs::read_to_string(directory.path().join("opened")).unwrap(),
		path.display().to_string()
	);
}

#[test]
fn first_run_creates_and_opens_the_configuration_without_running_commands() {
	let directory = tempdir().unwrap();
	let path = directory.path().join("Application Support/rcm/config.toml");
	let mut command = cli(&path);
	with_fake_opener(&mut command, directory.path(), 0);
	let output = command.args(["run", "hello"]).output().unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
	assert_eq!(
		fs::read_to_string(directory.path().join("opened")).unwrap(),
		path.display().to_string()
	);
	let stdout = String::from_utf8(output.stdout).unwrap();
	assert!(stdout.contains("Created configuration"));
	assert!(!stdout.contains("Hello from rcm!"));
	assert!(output.stderr.is_empty());
}

#[test]
fn subsequent_runs_list_commands_without_reopening_or_rewriting_the_file() {
	let (directory, path) = fixture(DEFAULT_CONFIG);
	let mut command = cli(&path);
	with_fake_opener(&mut command, directory.path(), 0);
	let output = command.arg("list").output().unwrap();
	assert!(output.status.success());
	assert!(
		String::from_utf8(output.stdout)
			.unwrap()
			.contains("hello\tRun the sample command set")
	);
	assert!(!directory.path().join("opened").exists());
	assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
}

#[test]
fn default_action_lists_commands_in_name_order() {
	let (_directory, path) =
		fixture("[commands.zebra]\nsteps = ['true']\n[commands.alpha]\nsteps = ['true']");
	let output = cli(&path).output().unwrap();
	assert!(output.status.success());
	assert_eq!(
		String::from_utf8(output.stdout).unwrap(),
		"alpha\t\nzebra\t\n"
	);
}

#[test]
fn run_executes_a_named_set_and_inherits_output() {
	let (_directory, path) = fixture("[commands.sample]\nsteps = ['printf first', 'printf second']");
	let output = cli(&path).args(["run", "sample"]).output().unwrap();
	assert!(output.status.success());
	assert_eq!(String::from_utf8(output.stdout).unwrap(), "firstsecond");
}

#[test]
fn omitted_shell_uses_the_inherited_shell_executable() {
	let (directory, path) = fixture("[commands.sample]\nsteps = ['printf executed']");
	let shell = directory.path().join("custom shell");
	fs::write(
		&shell,
		"#!/bin/sh\nprintf 'inherited-shell\\n'\nexec /bin/sh \"$@\"\n",
	)
	.unwrap();
	fs::set_permissions(&shell, fs::Permissions::from_mode(0o755)).unwrap();
	let output = cli(&path)
		.env("SHELL", &shell)
		.args(["run", "sample"])
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(output.stdout, b"inherited-shell\nexecuted");
}

#[test]
fn missing_or_empty_shell_environment_falls_back_to_bin_sh() {
	let (_directory, path) = fixture("[commands.sample]\nsteps = ['printf \"%s\" \"$0\"']");
	for shell in [None, Some(""), Some(" \t ")] {
		let mut command = cli(&path);
		command.env_remove("SHELL");
		if let Some(shell) = shell {
			command.env("SHELL", shell);
		}
		let output = command.args(["run", "sample"]).output().unwrap();
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		assert_eq!(output.stdout, b"/bin/sh");
	}
}

#[test]
fn explicit_shell_configuration_takes_precedence_over_the_environment() {
	let (_directory, path) =
		fixture("[settings]\nshell = '/bin/sh'\n[commands.sample]\nsteps = ['printf \"%s\" \"$0\"']");
	let output = cli(&path)
		.env("SHELL", "/missing/inherited-shell")
		.args(["run", "sample"])
		.output()
		.unwrap();
	assert!(output.status.success());
	assert_eq!(output.stdout, b"/bin/sh");
}

fn fake_brew(directory: &Path) {
	let executable = directory.join("brew");
	fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$RCM_TEST_BREW_LOG\"\n[ \"${RCM_TEST_BREW_FAIL-}\" != \"$1\" ] || exit 23\n").unwrap();
	fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
}

fn brew_cli(directory: &Path, path: &Path) -> Command {
	fake_brew(directory);
	let mut command = cli(path);
	command
		.env("PATH", format!("{}:/usr/bin:/bin", directory.display()))
		.env("RCM_TEST_BREW_LOG", directory.join("brew-calls"))
		.env_remove("RCM_TEST_BREW_FAIL")
		.args(["run", "brew-update"]);
	command
}

#[test]
fn shipped_brew_update_runs_refresh_upgrade_and_cleanup_in_order() {
	let (directory, path) = fixture(DEFAULT_CONFIG);
	let output = brew_cli(directory.path(), &path).output().unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert_eq!(
		fs::read_to_string(directory.path().join("brew-calls")).unwrap(),
		"update\nupgrade --greedy --no-ask\ncleanup -s\n"
	);
}

#[test]
fn shipped_brew_update_stops_before_cleanup_when_upgrade_fails() {
	let (directory, path) = fixture(DEFAULT_CONFIG);
	let output = brew_cli(directory.path(), &path)
		.env("RCM_TEST_BREW_FAIL", "upgrade")
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(23));
	assert_eq!(
		fs::read_to_string(directory.path().join("brew-calls")).unwrap(),
		"update\nupgrade --greedy --no-ask\n"
	);
}

#[test]
fn verbose_mode_prints_the_description_and_preserves_colors_when_redirected() {
	let (_directory, path) = fixture(
		r#"[commands.sample]
description = 'Describe the sample command'
steps = ['printf "\033[32mcolored\033[0m"']"#,
	);
	let output = cli(&path)
		.args(["run", "sample", "--verbose"])
		.output()
		.unwrap();
	assert!(output.status.success());
	assert_eq!(output.stdout, b"\x1b[32mcolored\x1b[0m");
	assert!(
		String::from_utf8(output.stderr)
			.unwrap()
			.contains("Describe the sample command")
	);
}

#[test]
fn run_propagates_the_failing_command_exit_code() {
	let (_directory, path) = fixture("[commands.fail]\nsteps = ['exit 23', 'printf should-not-run']");
	let output = cli(&path).args(["run", "fail"]).output().unwrap();
	assert_eq!(output.status.code(), Some(23));
	assert!(output.stdout.is_empty());
}

#[test]
fn unknown_command_sets_fail_with_a_useful_message() {
	let (_directory, path) = fixture(DEFAULT_CONFIG);
	let output = cli(&path).args(["run", "unknown"]).output().unwrap();
	assert!(!output.status.success());
	assert!(
		String::from_utf8(output.stderr)
			.unwrap()
			.contains("unknown command set")
	);
}

#[test]
fn config_opens_existing_files_even_when_their_toml_is_invalid() {
	let (directory, path) = fixture("[broken");
	let mut command = cli(&path);
	with_fake_opener(&mut command, directory.path(), 0);
	let output = command.arg("config").output().unwrap();
	assert!(output.status.success());
	assert_eq!(
		fs::read_to_string(directory.path().join("opened")).unwrap(),
		path.display().to_string()
	);
	assert_eq!(fs::read_to_string(&path).unwrap(), "[broken");
}

#[test]
fn opener_failure_preserves_the_created_configuration_and_explains_recovery() {
	let directory = tempdir().unwrap();
	let path = directory.path().join("config.toml");
	let mut command = cli(&path);
	with_fake_opener(&mut command, directory.path(), 1);
	let output = command.output().unwrap();
	assert!(!output.status.success());
	assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
	assert!(
		String::from_utf8(output.stderr)
			.unwrap()
			.contains("edit the file manually")
	);
}

#[test]
fn invalid_configuration_is_reported_without_overwriting_it() {
	let (_directory, path) = fixture("[broken");
	let output = cli(&path).arg("list").output().unwrap();
	assert!(!output.status.success());
	assert!(
		String::from_utf8(output.stderr)
			.unwrap()
			.contains("invalid TOML configuration")
	);
	assert_eq!(fs::read_to_string(&path).unwrap(), "[broken");
}

#[test]
fn path_help_and_version_do_not_create_configuration_files() {
	let directory = tempdir().unwrap();
	let path = directory.path().join("config.toml");
	let output = cli(&path).args(["config", "--path"]).output().unwrap();
	assert!(output.status.success());
	assert_eq!(
		String::from_utf8(output.stdout).unwrap().trim(),
		path.display().to_string()
	);
	assert!(cli(&path).arg("--help").output().unwrap().status.success());
	assert!(
		cli(&path)
			.arg("--version")
			.output()
			.unwrap()
			.status
			.success()
	);
	assert!(!path.exists());
}

#[test]
fn environment_override_is_supported_and_cli_override_takes_precedence() {
	let (directory, path) = fixture("[commands.from_env]\nsteps = ['true']");
	let other = directory.path().join("other.toml");
	fs::write(&other, "[commands.from_cli]\nsteps = ['true']").unwrap();
	let output = Command::new(env!("CARGO_BIN_EXE_rcm"))
		.env("RCM_CONFIG", &path)
		.arg("list")
		.output()
		.unwrap();
	assert!(output.status.success());
	assert!(
		String::from_utf8(output.stdout)
			.unwrap()
			.contains("from_env")
	);
	let output = cli(&other)
		.env("RCM_CONFIG", &path)
		.arg("list")
		.output()
		.unwrap();
	assert!(output.status.success());
	assert!(
		String::from_utf8(output.stdout)
			.unwrap()
			.contains("from_cli")
	);
}

#[test]
fn global_config_option_is_accepted_after_a_subcommand() {
	let (_directory, path) = fixture(DEFAULT_CONFIG);
	let output = Command::new(env!("CARGO_BIN_EXE_rcm"))
		.env_remove("RCM_CONFIG")
		.arg("list")
		.arg("--config")
		.arg(&path)
		.output()
		.unwrap();
	assert!(output.status.success());
	assert!(String::from_utf8(output.stdout).unwrap().contains("hello"));
}
