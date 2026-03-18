EXTRACT_DIR = fixtures
REPO_URL = https://github.com/ReamLabs/lean-spec-tests.git
KEYS_DIR = fixtures/keys/prod_scheme
KEYS_URL = https://github.com/leanEthereum/leansig-test-keys/releases/download/leanSpec-bbbbf62/prod_scheme.tar.gz

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

$(KEYS_DIR):
	@if [ -d $(KEYS_DIR) ]; then \
		echo "$(KEYS_DIR) already exists. Skipping download."; \
	else \
		echo "Downloading test keys..."; \
		mkdir -p fixtures/keys; \
		curl -sL $(KEYS_URL) | tar xz -C fixtures/keys || { echo "Key download failed."; exit 1; }; \
		echo "Test keys downloaded successfully."; \
	fi

test: $(EXTRACT_DIR) $(KEYS_DIR)
	@echo "Running lean-spec tests..."
	@cargo test --release --features lean-spec-tests
	@echo "Tests complete."

clean:
	@echo "Cleaning up downloaded fixtures and keys..."
	@rm -rf $(EXTRACT_DIR)
	@echo "Clean up complete."
