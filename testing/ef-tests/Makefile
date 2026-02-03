TARGET = mainnet.tar.gz
EXTRACT_DIR = mainnet
LATEST_RELEASE_URL = https://api.github.com/repos/ethereum/consensus-specs/releases/latest

PEERDAS_REPO = https://github.com/paulmillr/eth-vectors.git
PEERDAS_DIR = eth-vectors

.PHONY: all clean peerdas_data

all: test

$(EXTRACT_DIR): $(TARGET)
	@if [ -d $(EXTRACT_DIR) ]; then \
		echo "$(EXTRACT_DIR) already exists. Skipping extraction."; \
	else \
		echo "Extracting $(TARGET) into $(EXTRACT_DIR)..."; \
		mkdir -p $(EXTRACT_DIR); \
		tar -xzf $(TARGET) -C $(EXTRACT_DIR); \
		rm -f $(TARGET); \
		echo "Extraction complete."; \
	fi

$(TARGET):
	@if [ -d $(EXTRACT_DIR) ]; then \
		echo "$(EXTRACT_DIR) already downloaded. Skipping download."; \
	else \
		echo "Fetching the latest release (including pre-releases) for $(TARGET)..."; \
		curl -s $(LATEST_RELEASE_URL) \
		| grep "browser_download_url.*$(TARGET)" \
		| head -n 1 \
		| cut -d : -f 2,3 \
		| tr -d \" \
		| wget -qi -; \
		echo "$(TARGET) downloaded successfully."; \
	fi

peerdas_data: $(EXTRACT_DIR)
	@if [ -d $(EXTRACT_DIR)/tests/mainnet/fulu/kzg ]; then \
		echo "PeerDAS vectors already synced. Skipping."; \
	else \
		echo "Cloning PeerDAS vectors..."; \
		git clone --depth 1 $(PEERDAS_REPO) $(PEERDAS_DIR); \
		mkdir -p $(EXTRACT_DIR)/tests/mainnet/fulu/kzg; \
		for dir in $(PEERDAS_DIR)/peerdas/kzg/*; do \
			if [ -d "$$dir" ]; then \
				handler=$$(basename $$dir); \
				mkdir -p $(EXTRACT_DIR)/tests/mainnet/fulu/kzg/$$handler; \
				cp -r $$dir/* $(EXTRACT_DIR)/tests/mainnet/fulu/kzg/$$handler/; \
			fi \
		done; \
		rm -rf $(PEERDAS_DIR); \
		echo "PeerDAS sync complete."; \
	fi

test: $(EXTRACT_DIR) peerdas_data
	@echo "Running tests..."
	@cargo test --release --features ef-tests
	@echo "Tests complete."

clean:
	@echo "Cleaning up downloaded and extracted files..."
	@rm -f $(TARGET)
	@rm -rf $(EXTRACT_DIR)
	@rm -rf $(PEERDAS_DIR)
	@echo "Clean up complete."
	