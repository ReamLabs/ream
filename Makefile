# Heavily inspired by Reth: https://github.com/paradigmxyz/reth/blob/4c39b98b621c53524c6533a9c7b52fc42c25abd6/Makefile
.DEFAULT_GOAL := help

# Cargo features for builds.
FEATURES ?=

# Cargo profile for builds.
PROFILE ?= release

# Extra flags for Cargo.
CARGO_INSTALL_EXTRA_FLAGS ?=

CARGO_TARGET_DIR ?= target

##@ Help

.PHONY: help
help: # Display this help.
	@awk 'BEGIN {FS = ":.*#"; printf "Usage:\n  make \033[34m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?#/ { printf "  \033[34m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)

##@ Build

.PHONY: build
build: # Build the Ream binary into `target` directory.
	cargo build --bin ream --features "$(FEATURES)" --profile "$(PROFILE)"

.PHONY: build-debug
build-debug: # Build the Ream binary into `target/debug` directory
	cargo build --bin ream --features "$(FEATURES)"

.PHONY: install
install: # Build and install the Ream binary under `~/.cargo/bin`.
	cargo install --path bin/ream --force --locked \
		--features "$(FEATURES)" \
		--profile "$(PROFILE)" \
		$(CARGO_INSTALL_EXTRA_FLAGS)

##@ Testing and Linting

.PHONY: test
test: # Run all tests.
	cargo test --workspace -- --nocapture

.PHONY: fmt
fmt: # Run `rustfmt` on the entire workspace.
	cargo +nightly fmt --all

.PHONY: clippy
clippy: # Run `clippy` on the entire workspace.
	cargo clippy --all --all-targets --features "$(FEATURES)" --no-deps -- --deny warnings
	cargo clippy --package ream-bls --all-targets --features "supranational" --no-deps -- --deny warnings

.PHONY: sort
sort: # Run `cargo sort` on the entire workspace.
	cargo sort --grouped --workspace

.PHONY: lint
lint: fmt clippy sort # Run all linters.

##@ Others

.PHONY: check
check: # Run `cargo check`.
	cargo check --workspace --features "$(FEATURES)"

.PHONY: clean
clean: # Run `cargo clean`.
	cargo clean

.PHONY: update-book-cli
update-book-cli: build-debug # Update book cli documentation.
	@echo "Updating book cli doc..."
	@./book/cli/update.sh $(CARGO_TARGET_DIR)/debug/ream

.PHONY: clean-deps
clean-deps: # Run `cargo udeps` except `ef-tests` directory.
	cargo +nightly udeps --workspace --tests --all-targets --release --exclude ef-tests

.PHONY: pr
pr: lint update-book-cli clean-deps test # Run all checks for a PR.
