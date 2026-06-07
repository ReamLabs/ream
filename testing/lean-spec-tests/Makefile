EXTRACT_DIR = fixtures
NETWORK ?= devnet4
DEVNET4_REPO_URL = https://github.com/ReamLabs/lean-spec-tests.git
DEVNET5_RELEASE_URL = https://github.com/leanEthereum/leanSpec/releases/download/latest/fixtures-prod-scheme.tar.gz
DEVNET5_RELEASE_API_URL = https://api.github.com/repos/leanEthereum/leanSpec/releases/tags/latest
DEVNET5_ARCHIVE = fixtures-prod-scheme.tar.gz
DEVNET5_TMP_DIR = .leanSpec-fixtures
CURL = curl --fail --location --retry 3 --retry-all-errors --retry-delay 5 --connect-timeout 30 --speed-time 60 --speed-limit 1024
KEYS_DIR = fixtures/keys/prod_scheme
KEYS_URL = https://github.com/leanEthereum/leansig-test-keys/releases/download/leanSpec-bbbbf62/prod_scheme.tar.gz

FIXTURES := state_transition ssz fork_choice justifiability slot_clock sync verify_signatures
SELECTED := $(filter $(FIXTURES),$(MAKECMDGOALS))
CARGO_FEATURES = lean-spec-tests $(NETWORK)
SOURCE_MARKER = $(EXTRACT_DIR)/.ream-source-$(NETWORK)

.PHONY: all clean test test-devnet4 test-devnet5 prepare-fixtures $(FIXTURES)

all: test

test-devnet4:
	@$(MAKE) test NETWORK=devnet4

test-devnet5:
	@$(MAKE) test NETWORK=devnet5

prepare-fixtures:
	@if [ "$(NETWORK)" != "devnet4" ] && [ "$(NETWORK)" != "devnet5" ]; then \
		echo "Unsupported NETWORK=$(NETWORK). Use devnet4 or devnet5."; \
		exit 1; \
	fi
	@if [ -f $(SOURCE_MARKER) ] && [ -d $(EXTRACT_DIR)/consensus ]; then \
		echo "$(EXTRACT_DIR) already contains $(NETWORK) fixtures. Skipping download."; \
	elif [ "$(NETWORK)" = "devnet4" ]; then \
		echo "Cloning devnet4 fixtures from $(DEVNET4_REPO_URL)..."; \
		rm -rf $(EXTRACT_DIR); \
		git clone --depth 1 --filter=blob:none --single-branch --quiet $(DEVNET4_REPO_URL) $(EXTRACT_DIR) || { echo "Clone failed. Repository may be private or inaccessible."; exit 1; }; \
		touch $(SOURCE_MARKER); \
		echo "devnet4 fixtures downloaded successfully."; \
	else \
		echo "Downloading devnet5 fixtures from $(DEVNET5_RELEASE_URL)..."; \
		rm -rf $(EXTRACT_DIR); \
		mkdir -p $(DEVNET5_TMP_DIR); \
		if [ ! -f $(DEVNET5_TMP_DIR)/$(DEVNET5_ARCHIVE) ]; then \
			$(CURL) --output $(DEVNET5_TMP_DIR)/$(DEVNET5_ARCHIVE) $(DEVNET5_RELEASE_URL) || { echo "Fixture download failed."; exit 1; }; \
		else \
			echo "Using existing $(DEVNET5_TMP_DIR)/$(DEVNET5_ARCHIVE)."; \
		fi; \
		fixture_sha=$$($(CURL) --silent $(DEVNET5_RELEASE_API_URL) | awk '/"name": "$(DEVNET5_ARCHIVE)"/ { in_asset = 1 } in_asset && /"digest": "sha256:/ { gsub(/.*sha256:|",/, ""); print; exit }'); \
		if [ -z "$$fixture_sha" ]; then echo "Could not find devnet5 fixture checksum in release metadata."; exit 1; fi; \
		printf '%s  %s\n' "$$fixture_sha" "$(DEVNET5_ARCHIVE)" > $(DEVNET5_TMP_DIR)/$(DEVNET5_ARCHIVE).sha256; \
		(cd $(DEVNET5_TMP_DIR) && shasum -a 256 -c $(DEVNET5_ARCHIVE).sha256) || { echo "Fixture checksum verification failed."; exit 1; }; \
		tar xzf $(DEVNET5_TMP_DIR)/$(DEVNET5_ARCHIVE) -C $(DEVNET5_TMP_DIR) || { echo "Fixture extraction failed."; exit 1; }; \
		if [ -d $(DEVNET5_TMP_DIR)/fixtures/consensus ]; then \
			mv $(DEVNET5_TMP_DIR)/fixtures $(EXTRACT_DIR); \
		elif [ -d $(DEVNET5_TMP_DIR)/consensus ]; then \
			mkdir -p $(EXTRACT_DIR); \
			mv $(DEVNET5_TMP_DIR)/consensus $(EXTRACT_DIR)/consensus; \
			if [ -d $(DEVNET5_TMP_DIR)/keys ]; then mv $(DEVNET5_TMP_DIR)/keys $(EXTRACT_DIR)/keys; fi; \
		else \
			echo "Downloaded devnet5 archive does not contain consensus fixtures."; \
			exit 1; \
		fi; \
		touch $(SOURCE_MARKER); \
		echo "devnet5 fixtures downloaded successfully."; \
	fi

$(KEYS_DIR):
	@if [ -d $(KEYS_DIR) ]; then \
		echo "$(KEYS_DIR) already exists. Skipping download."; \
	else \
		echo "Downloading test keys..."; \
		mkdir -p fixtures/keys; \
		$(CURL) --silent --output - $(KEYS_URL) | tar xz -C fixtures/keys || { echo "Key download failed."; exit 1; }; \
		echo "Test keys downloaded successfully."; \
	fi

test: prepare-fixtures $(KEYS_DIR)
	@echo "Running lean-spec tests for $(NETWORK)..."
ifeq ($(SELECTED),)
	@cargo test --release --no-default-features --features "$(CARGO_FEATURES)"
else
	@cargo test --release --no-default-features --features "$(CARGO_FEATURES)" --test tests -- \
		--exact $(addprefix test_all_,$(addsuffix _fixtures,$(SELECTED))) --nocapture
endif
	@echo "Tests complete."

$(FIXTURES):
	@:

clean:
	@echo "Cleaning up downloaded fixtures and keys..."
	@rm -rf $(EXTRACT_DIR)
	@rm -rf $(DEVNET5_TMP_DIR)
	@echo "Clean up complete."
