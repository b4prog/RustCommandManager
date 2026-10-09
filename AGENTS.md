# Rust Command Manager

This repository builds the `rcm` executable. It reads named command sets and
settings from a user TOML file. macOS is the primary platform.

## Commands

Use the Makefile targets for building, testing, formatting, and validation.
The Makefile invokes Cargo directly and supports overriding `CARGO`.

## Rust formatting

- Use one literal tab per indentation level, displayed at a width of two spaces.
- Follow `rustfmt.toml` and `.editorconfig`; run `make fmt` after editing Rust.
- Do not add empty lines inside functions, methods, closures, or test helpers.
- Remove empty lines from function bodies when substantially modifying them.
- Empty lines between top-level declarations are allowed.

## Tests

- Put all tests in dedicated files under `tests/`, separate from implementation.
- Do not add inline `#[cfg(test)]` modules or tests in `src/`.
- Use temporary directories and child-process environment overrides for tests.
- Never open a real editor or modify the user's configuration during tests.

## Validation

Run `make check` before finishing code changes. This runs formatting, Clippy,
and tests.

## Configuration and execution

- Keep the shipped example in `examples/config.toml`; it is also the first-run
  template embedded in the executable.
- Use the standard macOS user configuration directory:
  `~/Library/Application Support/rcm/config.toml`.
- Never overwrite an existing configuration during initialization.
- On first run, create the configuration, open it with the default system app,
  and exit before executing any configured command.
- Keep errors actionable and propagate command failures through the exit status.
- Start every step in the folder from which `rcm` was launched.
- Each command controls `stop_on_error`, defaulting to `true`. A failed step
  makes the command fail even when it continues with later steps.
