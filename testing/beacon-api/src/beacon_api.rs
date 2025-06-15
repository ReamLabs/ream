pub struct BeaconApi {
    pub base_url: String,
}

impl BeaconApi {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
        }
    }

    pub fn get_genesis_url(&self) -> String {
        format!("{}/eth/v1/beacon/genesis", self.base_url)
    }

    pub fn get_state_root_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/root", self.base_url, state_id)
    }

    pub fn get_block_url(&self, block_id: &str) -> String {
        format!("{}/eth/v2/beacon/blocks/{}", self.base_url, block_id)
    }

    pub fn get_block_attestations_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/attestations", self.base_url, block_id)
    }

    pub fn get_block_header_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/headers/{}", self.base_url, block_id)
    }

    pub fn get_block_headers_url(&self) -> String {
        format!("{}/eth/v1/beacon/headers", self.base_url)
    }

    pub fn get_block_root_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/root", self.base_url, block_id)
    }

    pub fn get_block_voluntary_exits_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/voluntary_exits", self.base_url, block_id)
    }

    pub fn get_block_proposer_slashings_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/proposer_slashings", self.base_url, block_id)
    }

    pub fn get_block_attester_slashings_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/attester_slashings", self.base_url, block_id)
    }

    pub fn get_block_deposits_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/deposits", self.base_url, block_id)
    }

    pub fn get_block_bls_to_execution_changes_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/bls_to_execution_changes", self.base_url, block_id)
    }

    pub fn get_block_eth1_data_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/eth1_data", self.base_url, block_id)
    }

    pub fn get_block_sync_aggregate_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/sync_aggregate", self.base_url, block_id)
    }

    pub fn get_block_execution_payload_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/execution_payload", self.base_url, block_id)
    }

    pub fn get_block_withdrawals_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/withdrawals", self.base_url, block_id)
    }

    pub fn get_block_blob_kzg_commitments_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/blob_kzg_commitments", self.base_url, block_id)
    }
} 