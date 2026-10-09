#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tempfile::{TempDir, tempdir};

struct TerminalSession {
	_directory: TempDir,
	master: Box<dyn MasterPty + Send>,
	child: Box<dyn Child + Send + Sync>,
	input: Box<dyn Write + Send>,
	output: Receiver<Vec<u8>>,
	captured: Vec<u8>,
	parser: vt100::Parser,
	initial_attributes: String,
}

impl TerminalSession {
	fn start(script: &str, verbose: bool) -> Result<Self> {
		Self::start_with_step(script, "./command.sh", verbose)
	}

	fn start_with_step(script: &str, step: &str, verbose: bool) -> Result<Self> {
		Self::start_with_steps(script, &[step], verbose)
	}

	fn start_with_steps(script: &str, steps: &[&str], verbose: bool) -> Result<Self> {
		let directory = terminal_fixture(script, steps)?;
		let pair = native_pty_system().openpty(PtySize {
			rows: 40,
			cols: 120,
			pixel_width: 0,
			pixel_height: 0,
		})?;
		let initial_attributes = format!("{:?}", pair.master.get_termios());
		let mut reader = pair.master.try_clone_reader()?;
		let input = pair.master.take_writer()?;
		let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_rcm"));
		command.args([
			"--config",
			directory.path().join("config.toml").to_str().unwrap(),
			"run",
			"sample",
		]);
		command.env_remove("RCM_CONFIG");
		command.env("SHELL", "/bin/sh");
		command.env("TERM", "xterm-256color");
		if verbose {
			command.arg("--verbose");
		}
		let child = pair.slave.spawn_command(command)?;
		drop(pair.slave);
		let (sender, output) = mpsc::channel();
		thread::spawn(move || {
			let mut buffer = [0; 16384];
			while let Ok(count) = reader.read(&mut buffer) {
				if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
					break;
				}
			}
		});
		Ok(Self {
			_directory: directory,
			master: pair.master,
			child,
			input,
			output,
			captured: Vec::new(),
			parser: vt100::Parser::new(40, 120, 1000),
			initial_attributes,
		})
	}

	fn read_until(&mut self, text: &str) -> Result<()> {
		let deadline = Instant::now() + Duration::from_secs(10);
		while !self.parser.screen().contents().contains(text) {
			ensure!(Instant::now() < deadline, "prompt timeout: {}", self.text());
			ensure!(
				self.read_tick()?,
				"command ended before prompt: {}",
				self.text()
			);
		}
		Ok(())
	}

	fn read_tick(&mut self) -> Result<bool> {
		match self.output.recv_timeout(Duration::from_millis(20)) {
			Ok(chunk) => {
				self.parser.process(&chunk);
				self.captured.extend_from_slice(&chunk);
				Ok(true)
			}
			Err(RecvTimeoutError::Timeout) => Ok(true),
			Err(RecvTimeoutError::Disconnected) => Ok(false),
		}
	}

	fn send(&mut self, bytes: &[u8]) -> Result<()> {
		self.input.write_all(bytes)?;
		self.input.flush()?;
		Ok(())
	}

	fn wait(&mut self) -> Result<u32> {
		let deadline = Instant::now() + Duration::from_secs(10);
		while self.read_tick()? {
			ensure!(Instant::now() < deadline, "exit timeout: {}", self.text());
		}
		Ok(self.child.wait()?.exit_code())
	}

	fn text(&self) -> String {
		String::from_utf8_lossy(&self.captured).into_owned()
	}

	fn verify_restored_input(&self) {
		assert_eq!(
			format!("{:?}", self.master.get_termios()),
			self.initial_attributes
		);
	}

	fn verify_status_colors(&self, success: bool) -> Result<()> {
		let symbol = if success { "✓" } else { "✗" };
		let row = self
			.parser
			.screen()
			.rows(0, 120)
			.position(|row| row.contains(&format!("❯ ./command.sh {symbol}")))
			.context("missing completed command status")?;
		let row = u16::try_from(row)?;
		assert_eq!(
			self.parser.screen().cell(row, 0).unwrap().fgcolor(),
			vt100::Color::Idx(245)
		);
		assert_eq!(
			self.parser.screen().cell(row, 2).unwrap().fgcolor(),
			vt100::Color::Idx(129)
		);
		assert_eq!(
			self.parser.screen().cell(row, 15).unwrap().fgcolor(),
			vt100::Color::Idx(if success { 34 } else { 196 })
		);
		Ok(())
	}

	fn verify_no_completion_marks(&self) {
		let output = self.text();
		assert!(!output.contains('✓'));
		assert!(!output.contains('✗'));
	}
}

impl Drop for TerminalSession {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
	}
}

fn terminal_fixture(script: &str, steps: &[&str]) -> Result<TempDir> {
	let directory = tempdir()?;
	let executable = directory.path().join("command.sh");
	fs::write(&executable, format!("#!/bin/sh\n{script}\n"))?;
	fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
	let working_dir = toml::Value::String(directory.path().display().to_string());
	let steps = toml::Value::Array(
		steps
			.iter()
			.map(|step| toml::Value::String((*step).to_owned()))
			.collect(),
	);
	fs::write(
		directory.path().join("config.toml"),
		format!(
			"[settings]\nscroll_lines = 3\n[commands.sample]\ndescription = 'Runner test description'\nworking_dir = {working_dir}\nsteps = {steps}\n"
		),
	)?;
	Ok(directory)
}

#[test]
fn successful_output_scrolls_in_a_few_lines_and_clears_at_exit() -> Result<()> {
	let mut session = TerminalSession::start(
		"printf '\x1b[31mold line\\nline two\\nline three\\nline four\\nline five\\nline six\x1b[0m\\nReady? '\nread answer",
		false,
	)?;
	session.read_until("Ready?")?;
	let visible = session.parser.screen().contents();
	assert!(visible.contains("❯ ./command.sh"));
	assert!(!visible.contains("✓"));
	assert!(!visible.contains("✗"));
	assert!(visible.contains("line six"));
	assert!(!visible.contains("old line"));
	assert!(!visible.contains("line two"));
	let row = session
		.parser
		.screen()
		.rows(0, 120)
		.position(|row| row.contains("line six"))
		.context("missing colored row")?;
	assert_eq!(
		session
			.parser
			.screen()
			.cell(u16::try_from(row)?, 0)
			.unwrap()
			.fgcolor(),
		vt100::Color::Idx(1)
	);
	session.send(b"yes\n")?;
	assert_eq!(session.wait()?, 0);
	let final_output = session.parser.screen().contents();
	assert!(!final_output.contains("line six"));
	assert!(!final_output.contains("Ready?"));
	assert!(final_output.contains("❯ ./command.sh ✓"));
	assert!(!final_output.contains("[sample"));
	session.verify_status_colors(true)?;
	session.verify_restored_input();
	Ok(())
}

#[test]
fn failed_commands_replay_the_complete_colored_stdout_and_stderr() -> Result<()> {
	let mut session = TerminalSession::start(
		"printf '\x1b[32mfirst stdout\x1b[0m\\n'\nfor n in 1 2 3 4 5 6 7 8; do printf 'middle %s\\n' \"$n\"; done\nprintf '\x1b[31mlast stderr\x1b[0m\\n' >&2\nexit 23",
		false,
	)?;
	assert_eq!(session.wait()?, 23);
	let visible = session.parser.screen().contents();
	assert!(visible.contains("first stdout"));
	assert!(visible.contains("last stderr"));
	for number in 1..=8 {
		assert!(visible.contains(&format!("middle {number}")));
	}
	assert!(session.text().contains("\x1b[32mfirst stdout\x1b[0m"));
	assert!(session.text().contains("\x1b[31mlast stderr\x1b[0m"));
	session.verify_status_colors(false)?;
	session.verify_restored_input();
	Ok(())
}

#[test]
fn verbose_mode_prints_the_description_and_full_output_without_a_viewport() -> Result<()> {
	let mut session = TerminalSession::start(
		"for n in 1 2 3 4 5 6 7 8; do printf 'output %s\\n' \"$n\"; done\nprintf 'Ready? '\nread answer",
		true,
	)?;
	session.read_until("Ready?")?;
	let running = session.parser.screen().contents();
	for number in 1..=8 {
		assert!(running.contains(&format!("output {number}")));
	}
	session.send(b"yes\n")?;
	assert_eq!(session.wait()?, 0);
	let output = session.text();
	let visible = session.parser.screen().contents();
	assert!(output.contains("Runner test description"));
	assert_eq!(output.matches("./command.sh").count(), 1);
	assert!(!output.contains("\x1b[2K"));
	assert!(!output.contains("\x1b[?7l"));
	for number in 1..=8 {
		assert!(visible.contains(&format!("output {number}")));
	}
	session.verify_no_completion_marks();
	session.verify_restored_input();
	Ok(())
}

#[test]
fn verbose_failures_keep_colored_stdout_and_stderr_without_replaying_them() -> Result<()> {
	let mut session = TerminalSession::start(
		"printf '\x1b[32mfirst stdout\x1b[0m\\n'\nprintf '\x1b[31mlast stderr\x1b[0m\\n' >&2\nexit 23",
		true,
	)?;
	assert_eq!(session.wait()?, 23);
	let output = session.text();
	let visible = session.parser.screen().contents();
	assert_eq!(output.matches("./command.sh").count(), 1);
	assert_eq!(output.matches("\x1b[32mfirst stdout\x1b[0m").count(), 1);
	assert_eq!(output.matches("\x1b[31mlast stderr\x1b[0m").count(), 1);
	assert!(visible.contains("first stdout"));
	assert!(visible.contains("last stderr"));
	assert!(!output.contains("\x1b[2K"));
	assert!(!output.contains("\x1b[?7l"));
	session.verify_no_completion_marks();
	session.verify_restored_input();
	Ok(())
}

#[test]
fn verbose_mode_retains_output_from_every_step_and_prints_each_command_once() -> Result<()> {
	let mut session = TerminalSession::start_with_steps(
		"printf 'Output from %s\\n' \"$1\"",
		&["./command.sh first", "./command.sh second"],
		true,
	)?;
	assert_eq!(session.wait()?, 0);
	let output = session.text();
	let visible = session.parser.screen().contents();
	assert_eq!(output.matches("./command.sh first").count(), 1);
	assert_eq!(output.matches("./command.sh second").count(), 1);
	assert!(visible.contains("Output from first\n❯ ./command.sh second\nOutput from second"));
	assert!(!output.contains("\x1b[2K"));
	session.verify_no_completion_marks();
	session.verify_restored_input();
	Ok(())
}

#[test]
fn verbose_silent_or_unterminated_output_has_no_completion_marks() -> Result<()> {
	for (script, expected) in [("exit 0", "❯ ./command.sh"), ("printf tail", "tail")] {
		let mut session = TerminalSession::start(script, true)?;
		assert_eq!(session.wait()?, 0);
		assert!(session.parser.screen().contents().ends_with(expected));
		assert_eq!(session.text().matches("./command.sh").count(), 1);
		session.verify_no_completion_marks();
		session.verify_restored_input();
	}
	Ok(())
}

fn verify_password_input(verbose: bool, exit_code: u8) -> Result<()> {
	let script = format!(
		"stty -echo </dev/tty\nprintf 'Password: ' >/dev/tty\nIFS= read -r password </dev/tty\nstty echo </dev/tty\n[ \"$password\" = 'fixture-password' ] || exit 19\nprintf '\\nPassword accepted\\n'\nexit {exit_code}"
	);
	let mut session = TerminalSession::start(&script, verbose)?;
	session.read_until("Password:")?;
	session.send(b"fixture-password\n")?;
	assert_eq!(session.wait()?, u32::from(exit_code));
	assert!(!session.text().contains("fixture-password"));
	if verbose || exit_code != 0 {
		assert!(
			session
				.parser
				.screen()
				.contents()
				.contains("Password accepted")
		);
	}
	session.verify_restored_input();
	Ok(())
}

#[test]
fn compact_mode_accepts_a_hidden_password_from_dev_tty() -> Result<()> {
	verify_password_input(false, 0)
}

#[test]
fn verbose_mode_accepts_a_hidden_password_from_dev_tty() -> Result<()> {
	verify_password_input(true, 0)
}

#[test]
fn failure_replay_does_not_expose_hidden_password_input() -> Result<()> {
	verify_password_input(false, 7)
}

#[test]
fn control_c_interrupts_the_child_and_restores_input() -> Result<()> {
	let mut session = TerminalSession::start("printf 'Waiting for interrupt\\n'\nsleep 30", false)?;
	session.read_until("Waiting for interrupt")?;
	session.send(b"\x03")?;
	assert_ne!(session.wait()?, 0);
	session.verify_restored_input();
	Ok(())
}

#[test]
fn control_d_forwards_end_of_input_to_the_child() -> Result<()> {
	let mut session = TerminalSession::start(
		"printf 'Waiting for EOF: '\nif read answer; then exit 9; fi\nprintf 'EOF received\\n'",
		true,
	)?;
	session.read_until("Waiting for EOF:")?;
	session.send(b"\x04")?;
	assert_eq!(session.wait()?, 0);
	assert!(session.text().contains("EOF received"));
	session.verify_restored_input();
	Ok(())
}

#[test]
fn a_command_can_finish_while_a_background_child_still_has_the_terminal_open() -> Result<()> {
	let started = Instant::now();
	let mut session = TerminalSession::start("sleep 30 &\nprintf 'Command complete\\n'", true)?;
	assert_eq!(session.wait()?, 0);
	assert!(started.elapsed() < Duration::from_secs(2));
	assert!(session.text().contains("Command complete"));
	session.verify_restored_input();
	Ok(())
}

#[test]
fn pasted_input_is_forwarded_without_stalling_on_buffered_bytes() -> Result<()> {
	let mut session = TerminalSession::start(
		"stty -echo -icanon </dev/tty\nprintf 'Paste data: '\nhead -c 8192 </dev/tty | wc -c\nstty echo icanon </dev/tty",
		true,
	)?;
	session.read_until("Paste data:")?;
	session.send(&vec![b'z'; 8192])?;
	assert_eq!(session.wait()?, 0);
	assert!(session.text().contains("8192"));
	session.verify_restored_input();
	Ok(())
}

#[test]
fn a_silent_command_finishes_with_a_green_check_on_its_header() -> Result<()> {
	let mut session = TerminalSession::start("exit 0", false)?;
	assert_eq!(session.wait()?, 0);
	assert_eq!(session.parser.screen().contents(), "❯ ./command.sh ✓");
	session.verify_status_colors(true)?;
	session.verify_restored_input();
	Ok(())
}

#[test]
fn wrapped_command_headers_are_updated_without_duplicate_lines() -> Result<()> {
	let step = format!("./command.sh {}", "long-argument".repeat(24));
	let mut session = TerminalSession::start_with_step("printf 'Temporary output\\n'", &step, false)?;
	assert_eq!(session.wait()?, 0);
	let visible = session.parser.screen().contents().replace('\n', "");
	assert_eq!(visible, format!("❯ {step} ✓"));
	assert!(
		!session
			.parser
			.screen()
			.contents()
			.contains("Temporary output")
	);
	session.verify_restored_input();
	Ok(())
}

#[test]
fn command_newlines_are_displayed_without_breaking_the_status_header() -> Result<()> {
	let mut session = TerminalSession::start_with_step("exit 0", "./command.sh\ntrue", false)?;
	assert_eq!(session.wait()?, 0);
	assert_eq!(
		session.parser.screen().contents(),
		"❯ ./command.sh\\ntrue ✓"
	);
	session.verify_restored_input();
	Ok(())
}

#[test]
fn terminal_resize_is_forwarded_to_the_command() -> Result<()> {
	let mut session =
		TerminalSession::start("printf 'Ready to resize: '\nread answer\nstty size", true)?;
	session.read_until("Ready to resize:")?;
	session.master.resize(PtySize {
		rows: 24,
		cols: 60,
		pixel_width: 0,
		pixel_height: 0,
	})?;
	thread::sleep(Duration::from_millis(100));
	session.send(b"yes\n")?;
	assert_eq!(session.wait()?, 0);
	assert!(session.text().contains("24 60"));
	session.verify_restored_input();
	Ok(())
}
