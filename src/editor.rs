use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};

/// Open the file with the operating system's default associated application.
pub fn open_config(path: &Path) -> Result<()> {
	let opener = if cfg!(target_os = "macos") {
		"open"
	} else {
		"xdg-open"
	};
	let status = Command::new(opener).arg(path).status().with_context(|| {
		format!(
			"could not launch {opener}; edit {} manually",
			path.display()
		)
	})?;
	ensure!(
		status.success(),
		"{opener} could not open {}; associate .toml files with an editor or edit the file manually",
		path.display()
	);
	Ok(())
}
