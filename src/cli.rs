use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Run named command sets from a user TOML configuration.
#[derive(Debug, Parser)]
#[command(name = "rcm", version, about)]
pub struct Cli {
	/// Use this configuration file instead of the standard user location.
	#[arg(short, long, global = true, env = "RCM_CONFIG", value_name = "PATH")]
	pub config: Option<PathBuf>,
	/// Show each step's description and all output without the scrolling window.
	#[arg(short, long, global = true)]
	pub verbose: bool,
	#[command(subcommand)]
	pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
	/// List the available command sets (also the default action).
	List,
	/// Run all steps in a named command set.
	Run {
		/// Name of the command set defined in the TOML file.
		name: String,
	},
	/// Open the configuration with the default system application.
	Config {
		/// Print the configuration path without creating or opening the file.
		#[arg(long)]
		path: bool,
	},
}
