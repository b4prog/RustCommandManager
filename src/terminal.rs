use std::fs::File;
use std::io::{self, IsTerminal, Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use filedescriptor::{FileDescriptor, POLLIN, poll, pollfd};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::status::CommandStatus;
use crate::viewport::Viewport;

pub(crate) fn execute(
	shell: &str,
	step: &str,
	directory: Option<&Path>,
	verbose: bool,
	lines: u16,
) -> Result<i32> {
	let mut progress = CommandStatus::start(step, true, !verbose)?;
	let mut session = Session::spawn(shell, step, directory)?;
	let _input_mode = InputMode::enter()?;
	let mut output = CommandOutput::new(!verbose, session.size, lines)?;
	let result = session.stream(&mut output).and_then(|()| {
		session
			.child
			.wait()
			.context("could not wait for the command")
	});
	output.finish(
		result.as_ref().map_or(true, |status| !status.success()),
		&mut progress,
	)?;
	let status = result?;
	Ok(i32::try_from(status.exit_code()).unwrap_or(1))
}

struct Session {
	master: Box<dyn MasterPty + Send>,
	child: Box<dyn Child + Send + Sync>,
	input: Box<dyn Write + Send>,
	output: Box<dyn Read + Send>,
	output_fd: RawFd,
	stdin: InputForwarder,
	size: PtySize,
}

impl Session {
	fn spawn(shell: &str, step: &str, directory: Option<&Path>) -> Result<Self> {
		let size = terminal_size();
		let pair = native_pty_system()
			.openpty(size)
			.context("could not create a pseudo-terminal")?;
		let reader = pair
			.master
			.try_clone_reader()
			.context("could not read command output")?;
		let input = pair
			.master
			.take_writer()
			.context("could not forward command input")?;
		let output_fd = pair
			.master
			.as_raw_fd()
			.context("could not poll command output")?;
		let stdin = InputForwarder::new()?;
		let mut command = CommandBuilder::new(shell);
		command.args(["-c", step]);
		if let Some(directory) = directory {
			command.cwd(directory);
		}
		if std::env::var_os("TERM").is_none() {
			command.env("TERM", "xterm-256color");
		}
		let child = pair
			.slave
			.spawn_command(command)
			.with_context(|| format!("could not start shell {shell:?}"))?;
		drop(pair.slave);
		Ok(Self {
			master: pair.master,
			child,
			input,
			output: reader,
			output_fd,
			stdin,
			size,
		})
	}

	fn stream(&mut self, output: &mut CommandOutput) -> Result<()> {
		let mut finished_at = None;
		loop {
			self.resize(output)?;
			let mut descriptors = self.descriptors();
			let ready = poll_ready(&mut descriptors)?;
			if !self.forward_ready(&descriptors, output)? {
				return Ok(());
			}
			if self.finished(&mut finished_at, ready)? {
				return Ok(());
			}
		}
	}

	fn forward_ready(
		&mut self,
		descriptors: &[pollfd; 2],
		output: &mut CommandOutput,
	) -> Result<bool> {
		if descriptors[0].revents != 0 && !self.forward_output(output)? {
			return Ok(false);
		}
		if descriptors[1].revents != 0 && self.child.try_wait()?.is_none() {
			self.stdin.forward(&mut self.input)?;
		}
		Ok(true)
	}

	fn descriptors(&self) -> [pollfd; 2] {
		[
			pollfd {
				fd: self.output_fd,
				events: POLLIN,
				revents: 0,
			},
			pollfd {
				fd: if self.stdin.closed {
					-1
				} else {
					self.stdin.descriptor.as_raw_fd()
				},
				events: POLLIN,
				revents: 0,
			},
		]
	}

	fn forward_output(&mut self, output: &mut CommandOutput) -> Result<bool> {
		let mut buffer = [0; 16 * 1024];
		let count = match self.output.read(&mut buffer) {
			Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(true),
			result => result.context("could not read command output")?,
		};
		if count == 0 {
			return Ok(false);
		}
		output.consume(&buffer[..count])?;
		Ok(true)
	}

	fn finished(&mut self, finished_at: &mut Option<Instant>, ready: usize) -> Result<bool> {
		if self
			.child
			.try_wait()
			.context("could not wait for the command")?
			.is_none()
		{
			return Ok(false);
		}
		let finished = finished_at.get_or_insert_with(Instant::now);
		Ok(ready == 0 || finished.elapsed() >= Duration::from_millis(100))
	}

	fn resize(&mut self, output: &mut CommandOutput) -> Result<()> {
		let new_size = terminal_size();
		if new_size != self.size {
			self
				.master
				.resize(new_size)
				.context("could not resize the command terminal")?;
			output.resize(new_size)?;
			self.size = new_size;
		}
		Ok(())
	}
}

impl Drop for Session {
	fn drop(&mut self) {
		if self.child.try_wait().ok().flatten().is_none() {
			let _ = self.child.kill();
		}
		let _ = self.child.wait();
	}
}

fn terminal_size() -> PtySize {
	let (cols, rows) = size().unwrap_or((120, 24));
	PtySize {
		rows: rows.max(2),
		cols: cols.max(1),
		pixel_width: 0,
		pixel_height: 0,
	}
}

fn poll_ready(descriptors: &mut [pollfd]) -> Result<usize> {
	match poll(descriptors, Some(Duration::from_millis(20))) {
		Err(filedescriptor::Error::Poll(error)) if error.kind() == io::ErrorKind::Interrupted => Ok(0),
		result => result.context("could not poll the command terminal"),
	}
}

struct InputMode(bool);

impl InputMode {
	fn enter() -> Result<Self> {
		let enabled = io::stdin().is_terminal();
		if enabled {
			enable_raw_mode().context("could not enable interactive terminal input")?;
		}
		Ok(Self(enabled))
	}
}

impl Drop for InputMode {
	fn drop(&mut self) {
		if self.0 {
			let _ = disable_raw_mode();
		}
	}
}

struct InputForwarder {
	descriptor: FileDescriptor,
	closed: bool,
}

impl InputForwarder {
	fn new() -> Result<Self> {
		Ok(Self {
			descriptor: FileDescriptor::dup(&io::stdin()).context("could not read command input")?,
			closed: false,
		})
	}

	fn forward(&mut self, destination: &mut impl Write) -> Result<()> {
		let mut buffer = [0; 4096];
		let count = match self.descriptor.read(&mut buffer) {
			Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(()),
			result => result.context("could not read command input")?,
		};
		if count == 0 {
			self.closed = true;
			destination
				.write_all(b"\x04")
				.context("could not finish command input")?;
			return Ok(());
		}
		destination
			.write_all(&buffer[..count])
			.context("could not forward command input")?;
		destination.flush().context("could not flush command input")
	}
}

struct CommandOutput {
	viewport: Option<Viewport>,
	transcript: Option<File>,
	last_byte: Option<u8>,
	lines: u16,
}

impl CommandOutput {
	fn new(compact: bool, size: PtySize, lines: u16) -> Result<Self> {
		let (viewport, transcript) = if compact {
			(
				Some(Viewport::new(viewport_height(lines, size), size.cols)),
				Some(tempfile::tempfile().context("could not capture command output")?),
			)
		} else {
			(None, None)
		};
		Ok(Self {
			viewport,
			transcript,
			last_byte: None,
			lines,
		})
	}

	fn consume(&mut self, chunk: &[u8]) -> Result<()> {
		let mut stdout = io::stdout().lock();
		if let Some(transcript) = &mut self.transcript {
			transcript
				.write_all(chunk)
				.context("could not capture command output")?;
		}
		self.last_byte = chunk.last().copied();
		match &mut self.viewport {
			Some(viewport) => viewport.consume(chunk, &mut stdout)?,
			None => {
				stdout.write_all(chunk)?;
				stdout.flush()?;
			}
		}
		Ok(())
	}

	fn resize(&mut self, size: PtySize) -> Result<()> {
		if let Some(viewport) = &mut self.viewport {
			viewport.resize(
				viewport_height(self.lines, size),
				size.cols,
				&mut io::stdout().lock(),
			)?;
		}
		Ok(())
	}

	fn finish(&mut self, failed: bool, progress: &mut CommandStatus) -> Result<()> {
		if let Some(viewport) = &mut self.viewport {
			viewport.clear(&mut io::stdout().lock())?;
		}
		progress.finish(!failed)?;
		let mut stdout = io::stdout().lock();
		if failed {
			self.replay(&mut stdout)?;
		}
		stdout.write_all(b"\x1b[0m")?;
		stdout.flush()?;
		Ok(())
	}

	fn replay(&mut self, output: &mut impl Write) -> io::Result<()> {
		if let Some(transcript) = &mut self.transcript {
			transcript.seek(SeekFrom::Start(0))?;
			io::copy(transcript, output)?;
			if self.last_byte.is_some_and(|byte| byte != b'\n') {
				output.write_all(b"\r\n")?;
			}
		}
		Ok(())
	}
}

impl Drop for CommandOutput {
	fn drop(&mut self) {
		if let Some(viewport) = &mut self.viewport {
			let _ = viewport.clear(&mut io::stdout().lock());
		}
	}
}

fn viewport_height(lines: u16, size: PtySize) -> u16 {
	lines.max(1).min(size.rows.saturating_sub(1).max(1))
}
