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

    
    pub fn get_blinded_block_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blinded_blocks/{}", self.base_url, block_id)
    }

    pub fn get_block_attestations_v2_url(&self, block_id: &str) -> String {
        format!("{}/eth/v2/beacon/blocks/{}/attestations", self.base_url, block_id)
    }

    pub fn get_pool_attestations_url(&self) -> String {
        format!("{}/eth/v1/beacon/pool/attestations", self.base_url)
    }

    pub fn get_pool_attestations_v2_url(&self) -> String {
        format!("{}/eth/v2/beacon/pool/attestations", self.base_url)
    }

    pub fn get_pool_attester_slashings_url(&self) -> String {
        format!("{}/eth/v1/beacon/pool/attester_slashings", self.base_url)
    }

    pub fn get_pool_attester_slashings_v2_url(&self) -> String {
        format!("{}/eth/v2/beacon/pool/attester_slashings", self.base_url)
    }

    pub fn get_pool_proposer_slashings_url(&self) -> String {
        format!("{}/eth/v1/beacon/pool/proposer_slashings", self.base_url)
    }

    pub fn get_pool_voluntary_exits_url(&self) -> String {
        format!("{}/eth/v1/beacon/pool/voluntary_exits", self.base_url)
    }

    pub fn get_pool_bls_to_execution_changes_url(&self) -> String {
        format!("{}/eth/v1/beacon/pool/bls_to_execution_changes", self.base_url)
    }

    pub fn get_state_fork_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/fork", self.base_url, state_id)
    }

    pub fn get_state_randao_url(&self, state_id: &str, epoch: u64) -> String {
        format!("{}/eth/v1/beacon/states/{}/randao?epoch={}", self.base_url, state_id, epoch)
    }

    pub fn get_state_finality_checkpoints_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/finality_checkpoints", self.base_url, state_id)
    }

    pub fn get_state_validators_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/validators", self.base_url, state_id)
    }

    pub fn get_state_validator_url(&self, state_id: &str, validator_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/validators/{}", self.base_url, state_id, validator_id)
    }

    pub fn get_state_validator_balances_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/validator_balances", self.base_url, state_id)
    }

    pub fn get_epoch_committees_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/committees", self.base_url, state_id)
    }

    pub fn get_epoch_sync_committees_url(&self, state_id: &str, epoch: u64) -> String {
        format!("{}/eth/v1/beacon/states/{}/sync_committees?epoch={}", self.base_url, state_id, epoch)
    }

    pub fn get_pending_deposits_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/pending_deposits", self.base_url, state_id)
    }

    pub fn get_pending_partial_withdrawals_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/pending_partial_withdrawals", self.base_url, state_id)
    }

    pub fn get_pending_consolidations_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/pending_consolidations", self.base_url, state_id)
    }

    pub fn get_proposer_lookahead_url(&self, state_id: &str) -> String {
        format!("{}/eth/v1/beacon/states/{}/proposer_lookahead", self.base_url, state_id)
    }

    pub fn get_block_rewards_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/rewards", self.base_url, block_id)
    }

    pub fn get_attestations_rewards_url(&self, epoch: u64) -> String {
        format!("{}/eth/v1/beacon/rewards/attestations?epoch={}", self.base_url, epoch)
    }

    pub fn get_sync_committee_rewards_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blocks/{}/rewards/sync_committee", self.base_url, block_id)
    }

    pub fn get_blob_sidecars_url(&self, block_id: &str) -> String {
        format!("{}/eth/v1/beacon/blob_sidecars/{}", self.base_url, block_id)
    }
} 