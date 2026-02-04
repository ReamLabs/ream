EXTRACT_DIR = fixtures
REPO_URL = https://github.com/ReamLabs/lean-spec-tests.git

.PHONY: all clean test test-devnet2 test-devnet3

all: test

$(EXTRACT_DIR):
	@if [ -d $(EXTRACT_DIR)/consensus ]; then \
		echo "$(EXTRACT_DIR) already exists. Skipping download."; \
	else \
		echo "Cloning lean-spec-tests from GitHub..."; \
		git clone --depth 1 --quiet $(REPO_URL) $(EXTRACT_DIR) || { echo "Clone failed. Repository may be private or inaccessible."; exit 1; }; \
		echo "Fixtures downloaded successfully."; \
	fi

test: test-devnet2 test-devnet3

test-devnet2: $(EXTRACT_DIR)
	@echo "Running lean-spec tests (devnet2)..."
	@cargo test --release --features lean-spec-tests,devnet2 --no-default-features
	@echo "Devnet2 tests complete."

test-devnet3: $(EXTRACT_DIR)
	@echo "Running lean-spec tests (devnet3)..."
	@cargo test --release --features lean-spec-tests,devnet3 --no-default-features
	@echo "Devnet3 tests complete."

clean:
	@echo "Cleaning up downloaded fixtures..."
	@rm -rf $(EXTRACT_DIR)
	@echo "Clean up complete."
