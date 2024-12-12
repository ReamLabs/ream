.PHONY: lint
lint: # Run clippy and rustfmt
	cargo fmt --all
	cargo clippy --all --all-targets --all-features --no-deps -- --deny warnings
