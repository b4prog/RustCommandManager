use std::io::{self, Write};

use unicode_width::UnicodeWidthStr;

const GRAY: &str = "\x1b[38;5;245m";
const PURPLE: &str = "\x1b[38;5;129m";
const GREEN: &str = "\x1b[38;5;34m";
const RED: &str = "\x1b[38;5;196m";
const RESET: &str = "\x1b[0m";

pub(crate) struct CommandStatus {
	command: String,
	terminal: bool,
	in_place: bool,
	finished: bool,
}

impl CommandStatus {
	pub(crate) fn start(command: &str, terminal: bool, in_place: bool) -> io::Result<Self> {
		let status = Self {
			command: sanitize(command),
			terminal,
			in_place,
			finished: false,
		};
		status.write(&status.line(None))?;
		Ok(status)
	}

	pub(crate) fn finish(&mut self, success: bool) -> io::Result<()> {
		if self.finished {
			return Ok(());
		}
		let mut output = if self.in_place {
			self.clear_header()
		} else if self.terminal {
			"\r\n".to_owned()
		} else {
			String::new()
		};
		output.push_str(&self.line(Some(success)));
		self.write(&output)?;
		self.finished = true;
		Ok(())
	}

	fn line(&self, success: Option<bool>) -> String {
		let mut line = if self.terminal {
			format!("{GRAY}❯ {PURPLE}{}{RESET}", self.command)
		} else {
			format!("❯ {}", self.command)
		};
		if let Some(success) = success {
			let (symbol, color) = if success {
				("✓", GREEN)
			} else {
				("✗", RED)
			};
			let suffix = if self.terminal {
				format!(" {color}{symbol}{RESET}")
			} else {
				format!(" {symbol}")
			};
			line.push_str(&suffix);
		}
		line.push_str(if self.terminal { "\r\n" } else { "\n" });
		line
	}

	fn clear_header(&self) -> String {
		let (columns, rows) = crossterm::terminal::size().unwrap_or((120, 24));
		let width = UnicodeWidthStr::width(self.command.as_str()) + 2;
		let height = width
			.div_ceil(usize::from(columns.max(1)))
			.max(1)
			.min(usize::from(rows.max(1)));
		format!(
			"\x1b[{height}A{}\x1b[{height}A",
			"\r\x1b[2K\x1b[1B".repeat(height)
		)
	}

	fn write(&self, text: &str) -> io::Result<()> {
		if self.terminal {
			let mut output = io::stdout().lock();
			output.write_all(text.as_bytes())?;
			output.flush()
		} else {
			let mut output = io::stderr().lock();
			output.write_all(text.as_bytes())?;
			output.flush()
		}
	}
}

impl Drop for CommandStatus {
	fn drop(&mut self) {
		let _ = self.finish(false);
	}
}

fn sanitize(command: &str) -> String {
	command
		.chars()
		.map(|character| {
			if character.is_control() {
				character.escape_default().to_string()
			} else {
				character.to_string()
			}
		})
		.collect()
}
