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

.PHONY: install
install: # Build and install the Ream binary under `~/.cargo/bin`.
	cargo install --path bin/ream --force --locked \
		--features "$(FEATURES)" \
		--profile "$(PROFILE)" \
		$(CARGO_INSTALL_EXTRA_FLAGS)

##@ Others
.PHONY: clean
clean: # Run `cargo clean`.
	cargo clean

.PHONY: lint
lint: # Run `clippy` and `rustfmt`.
	cargo +nightly fmt --all
	cargo clippy --all --all-targets --features "$(FEATURES)" --no-deps -- --deny warnings

	# clippy for bls with supranational feature
	cargo clippy --package ream-bls --all-targets --features "supranational" --no-deps -- --deny warnings

	# cargo sort
	cargo sort --grouped --workspace

.PHONY: build-debug
build-debug: ## Build the ream binary into `target/debug` directory.
	cargo build --bin ream --features "$(FEATURES)"

.PHONY: update-book-cli
update-book-cli: build-debug ## Update book cli documentation.
	@echo "Updating book cli doc..."
	@./book/cli/update.sh $(CARGO_TARGET_DIR)/debug/ream

.PHONY: test
test: # Run all tests.
	cargo test --workspace -- --nocapture

clean-deps:
	cargo +nightly udeps --workspace --tests --all-targets --release --exclude ef-tests

pr:
	make lint && \
	make update-book-cli && \
	make test
