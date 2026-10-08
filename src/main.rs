use std::process::ExitCode;

use clap::Parser;
use rcm::{app, cli::Cli};

fn main() -> ExitCode {
	match app::run(Cli::parse()) {
		Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
		Err(error) => {
			eprintln!("rcm: {error:#}");
			ExitCode::FAILURE
		}
	}
}
