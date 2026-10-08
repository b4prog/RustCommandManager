# Rust Command Manager

`rcm` is a Rust CLI that runs named sets of shell commands from a user TOML file.
macOS is the primary platform. Command sets run sequentially and share configurable
defaults for the shell, working directory, and failure handling.

## Build and install

Install the stable Rust toolchain, then run:

```sh
make build
make install
```

The debug executable is `target/debug/rcm`; `make release` produces
`target/release/rcm`. Installation places `rcm` in Cargo's binary directory
(normally `~/.cargo/bin`), which must be on your `PATH`.

## First run

```sh
rcm
```

On macOS, the default configuration is:

```text
~/Library/Application Support/rcm/config.toml
```

This uses the standard macOS user configuration location documented by
[dirs](https://docs.rs/dirs/6.0.0/dirs/fn.config_dir.html) and Apple's
[file system guide](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html).

If the file is missing, `rcm` creates its parent directory and writes the sample
configuration, then calls macOS `open` to open it with the default application
associated with `.toml` files. It exits after setup, including when the requested
action was `run`, so save your edits and invoke the desired command again.
Existing configuration files are preserved.

If macOS has no default app for TOML files, choose an editor with Finder's
**Get Info → Open with → Change All**, or edit the printed path manually. An
opening failure leaves the new configuration available and returns a nonzero
exit status. Linux uses its standard configuration directory and `xdg-open`.

## Usage

```sh
rcm                         # List command sets, or perform first-run setup
rcm list                    # List command sets and descriptions
rcm run hello               # Run a command set
rcm config                  # Open the configuration for editing
rcm config --path           # Print its path without creating or opening it
rcm --config ./rcm.toml list # Use a different configuration file
rcm --help
rcm --version
```

`RCM_CONFIG` also overrides the default path; `--config` takes precedence.
Help, version, and `config --path` do not create or open configuration files.
`rcm config` can open a file containing invalid TOML so you can repair it.

## Configuration

The complete first-run template is [examples/config.toml](examples/config.toml).

```toml
[settings]
shell = "/bin/sh"
stop_on_error = true
# working_dir = "/absolute/path/to/project"

[commands.hello]
description = "Say hello"
steps = ['printf "Hello from rcm!\n"']

[commands.inspect]
description = "Inspect a project"
working_dir = "/absolute/path/to/project"
steps = ['pwd', 'ls -la']
```

| Field | Behavior |
| --- | --- |
| `settings.shell` | Shell executable called with `-c`; defaults to `/bin/sh`. Use an executable path or name, without flags. |
| `settings.stop_on_error` | Defaults to `true`. Set to `false` to attempt later steps after a step fails. |
| `settings.working_dir` | Optional default working directory; otherwise the invocation directory is inherited. |
| `commands.NAME.description` | Optional description displayed by `rcm list`. |
| `commands.NAME.steps` | Required, nonempty array of shell command strings, executed in order. |
| `commands.NAME.working_dir` | Optional override of the global working directory. |

Relative working directories are resolved from the directory where you invoke
`rcm`. Working directory paths are used literally; use an absolute path instead
of `~` or environment variables in these fields.

Every step starts a separate shell with inherited environment, input, and output.
Pipelines, redirections, and shell expansion work within each step. A `cd` or
`export` in one step does not affect the next; combine related operations in a
single step, for example `cd /some/project && ./build.sh`.

Successful sets return exit code `0`. A failed set returns the first failed
step's exit code, even when `stop_on_error = false`; signal termination and
application errors return `1`. Failure to start a shell aborts the set.
Invalid TOML, unknown fields, empty names or steps, and unknown command sets
produce an error with a nonzero exit status.

## Development

```sh
make                    # Show available targets
make run ARGS="--help"
make fmt
make check
```

`make check` runs formatting, Clippy, and all tests. Individual checks are
available through `make fmt-check`, `make lint`, and `make test`.
Cargo commands use `--locked` where applicable; commit `Cargo.lock` for
reproducible executable builds.

Rust uses literal tabs, one per indentation level, displayed at width **2**.
`rustfmt.toml`, `.editorconfig`, and the VS Code settings enforce this convention.
There are no empty lines inside function bodies. All tests live in dedicated
files under `tests/`; implementation belongs under `src/`.

```text
src/                     CLI, configuration, editor opening, and execution
tests/                   Configuration, command execution, and CLI tests
examples/config.toml     Embedded first-run template
AGENTS.md                Repository instructions for coding agents
Makefile                 Development, validation, and installation targets
```

## License

[MIT](LICENSE).
