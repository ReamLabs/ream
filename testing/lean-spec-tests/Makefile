EXTRACT_DIR = fixtures
REPO_URL = https://github.com/ReamLabs/lean-spec-tests.git

.PHONY: all clean test

all: test

$(EXTRACT_DIR):
	@if [ -d $(EXTRACT_DIR)/consensus ]; then \
		echo "$(EXTRACT_DIR) already exists. Skipping download."; \
	else \
		echo "Cloning lean-spec-tests from GitHub..."; \
		git clone --depth 1 --quiet $(REPO_URL) $(EXTRACT_DIR) || { echo "Clone failed. Repository may be private or inaccessible."; exit 1; }; \
		echo "Fixtures downloaded successfully."; \
	fi

test: $(EXTRACT_DIR)
	@echo "Running lean-spec tests..."
	@cargo test --release --features lean-spec-tests
	@echo "Tests complete."

clean:
	@echo "Cleaning up downloaded fixtures..."
	@rm -rf $(EXTRACT_DIR)
	@echo "Clean up complete."
