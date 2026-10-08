use std::io::{self, Write};

/// A bounded terminal screen that preserves ANSI styles and handles split input.
pub struct Viewport {
	parser: vt100::Parser,
	height: u16,
	width: u16,
	allocated: bool,
}

impl Viewport {
	pub fn new(height: u16, width: u16) -> Self {
		let height = height.max(1);
		let width = width.max(1);
		Self {
			parser: vt100::Parser::new(height, width, 0),
			height,
			width,
			allocated: false,
		}
	}

	/// Interpret output in a virtual terminal and draw only the visible rows.
	pub fn consume(&mut self, bytes: &[u8], output: &mut impl Write) -> io::Result<()> {
		self.parser.process(bytes);
		if !self.allocated {
			output.write_all("\r\n".repeat(usize::from(self.height)).as_bytes())?;
			self.allocated = true;
		}
		write!(output, "\x1b[{}A\x1b[?7l", self.height)?;
		for row in self.parser.screen().rows_formatted(0, self.width) {
			output.write_all(b"\r\x1b[2K\x1b[0m")?;
			output.write_all(&row)?;
			output.write_all(b"\x1b[0m\x1b[1B")?;
		}
		output.write_all(b"\r\x1b[0m\x1b[?7h")?;
		output.flush()
	}

	/// Clear the reserved rows and leave the cursor ready for subsequent output.
	pub fn clear(&mut self, output: &mut impl Write) -> io::Result<()> {
		if !self.allocated {
			return Ok(());
		}
		write!(output, "\x1b[{}A\x1b[?7l", self.height)?;
		for _ in 0..self.height {
			output.write_all(b"\r\x1b[2K\x1b[0m\x1b[1B")?;
		}
		write!(output, "\x1b[{}A\r\x1b[0m\x1b[?7h", self.height)?;
		self.allocated = false;
		output.flush()
	}

	/// Adapt the viewport after a terminal resize, clearing its old allocation.
	pub fn resize(&mut self, height: u16, width: u16, output: &mut impl Write) -> io::Result<()> {
		let allocated = self.allocated;
		self.clear(output)?;
		self.height = height.max(1);
		self.width = width.max(1);
		self.parser = self.resized_parser();
		if allocated {
			self.consume(&[], output)?;
		}
		Ok(())
	}

	fn resized_parser(&self) -> vt100::Parser {
		let screen = self.parser.screen();
		let first_row = screen.size().0.saturating_sub(self.height);
		let mut parser = vt100::Parser::new(self.height, self.width, 0);
		for (index, row) in screen
			.rows_formatted(0, self.width)
			.skip(usize::from(first_row))
			.enumerate()
		{
			if index > 0 {
				parser.process(b"\r\n");
			}
			parser.process(&row);
		}
		let (row, col) = screen.cursor_position();
		let row = row.saturating_sub(first_row).min(self.height - 1) + 1;
		let col = col.min(self.width - 1) + 1;
		parser.process(format!("\x1b[{row};{col}H").as_bytes());
		parser.process(&screen.attributes_formatted());
		parser
	}
}
