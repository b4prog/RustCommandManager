.DEFAULT_GOAL := help

CARGO ?= cargo
ARGS ?=

.PHONY: help build release run test fmt fmt-check lint check install clean

help:
	@printf '%s\n' \
	  'build       Build target/debug/rcm' \
	  'release     Build target/release/rcm' \
	  'run         Run rcm (pass arguments with ARGS="...")' \
	  'test        Run the dedicated integration test files' \
	  'fmt         Format Rust sources with tabs at width 2' \
	  'fmt-check   Check Rust formatting' \
	  'lint        Run Clippy with warnings treated as errors' \
	  'check       Run formatting, lint, and tests' \
	  'install     Install rcm into the Cargo binary directory' \
	  'clean       Remove build artifacts'

build:
	$(CARGO) build --locked

release:
	$(CARGO) build --release --locked

run:
	$(CARGO) run --locked -- $(ARGS)

test:
	$(CARGO) test --all-targets --locked

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

lint:
	$(CARGO) clippy --all-targets --locked -- -D warnings

check: fmt-check lint test

install:
	$(CARGO) install --path . --locked

clean:
	$(CARGO) clean
