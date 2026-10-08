//! Configuration, command execution, and CLI support for Rust Command Manager.

pub mod app;
pub mod cli;
pub mod config;
pub mod editor;
pub mod runner;
#[cfg(unix)]
mod terminal;
pub mod viewport;
