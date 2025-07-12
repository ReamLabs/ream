use std::{
    str::FromStr,
    sync::{Arc, LazyLock, Once, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy_primitives::{Address, B256, U256, address, aliases::B32, b256, fixed_bytes};
use ream_consensus_misc::{
    constants::GENESIS_VALIDATORS_ROOT, fork::Fork, fork_data::ForkData, misc::checksummed_address,
};
use serde::Deserialize;

use crate::fork_schedule::ForkSchedule;

pub static HAS_NETWORK_SPEC_BEEN_INITIALIZED: Once = Once::new();

pub fn initialize_test_network_spec() {
    let _ = GENESIS_VALIDATORS_ROOT.set(B256::ZERO);
    HAS_NETWORK_SPEC_BEEN_INITIALIZED.call_once(|| {
        set_network_spec(DEV.clone());
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Holesky,
    Sepolia,
    Hoodi,
    Dev,
    Custom(String),
}

impl<'de> Deserialize<'de> for Network {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "mainnet" => Ok(Network::Mainnet),
            "holesky" => Ok(Network::Holesky),
            "sepolia" => Ok(Network::Sepolia),
            "hoodi" => Ok(Network::Hoodi),
            "dev" => Ok(Network::Dev),
            custom => Ok(Network::Custom(custom.to_string())),
        }
    }
}

static NETWORK_SPEC: OnceLock<Arc<NetworkSpec>> = OnceLock::new();

/// MUST be called only once at the start of the application to initialize static [NetworkSpec].
///
/// The static `NetworkSpec` can be accessed using [network_spec].
///
/// # Panics
///
/// Panics if this function is called more than once.
pub fn set_network_spec(network_spec: Arc<NetworkSpec>) {
    NETWORK_SPEC
        .set(network_spec)
        .expect("NetworkSpec should be set only once at the start of the application");
}

/// Returns the static [NetworkSpec] initialized by [set_network_spec].
///
/// # Panics
///
/// Panics if [set_network_spec] wasn't called before this function.
pub fn network_spec() -> Arc<NetworkSpec> {
    NETWORK_SPEC.get().expect("NetworkSpec wasn't set").clone()
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub struct NetworkSpec {
    pub preset_base: String,
    #[serde(rename = "CONFIG_NAME")]
    pub network: Network,

    // Transition
    pub terminal_total_difficulty: U256,
    #[serde(with = "serde_utils::b256_hex")]
    pub terminal_block_hash: B256,
    pub terminal_block_hash_activation_epoch: u64,

    // Genesis
    pub min_genesis_active_validator_count: u64,
    pub min_genesis_time: u64,
    #[serde(with = "crate::b32_hex")]
    pub genesis_fork_version: B32,
    pub genesis_delay: u64,

    // Forking
    #[serde(with = "crate::b32_hex")]
    pub altair_fork_version: B32,
    pub altair_fork_epoch: u64,
    #[serde(with = "crate::b32_hex")]
    pub bellatrix_fork_version: B32,
    pub bellatrix_fork_epoch: u64,
    #[serde(with = "crate::b32_hex")]
    pub capella_fork_version: B32,
    pub capella_fork_epoch: u64,
    #[serde(with = "crate::b32_hex")]
    pub deneb_fork_version: B32,
    pub deneb_fork_epoch: u64,
    #[serde(with = "crate::b32_hex")]
    pub electra_fork_version: B32,
    pub electra_fork_epoch: u64,

    // Time parameters
    pub seconds_per_slot: u64,
    pub seconds_per_eth1_block: u64,
    pub min_validator_withdrawability_delay: u64,
    pub shard_committee_period: u64,
    pub eth1_follow_distance: u64,

    // Validator cycle
    pub inactivity_score_bias: u64,
    pub inactivity_score_recovery_rate: u64,
    pub ejection_balance: u64,
    pub min_per_epoch_churn_limit: u64,
    pub churn_limit_quotient: u64,
    pub max_per_epoch_activation_churn_limit: u64,

    // Fork choice
    pub proposer_score_boost: u64,
    pub reorg_head_weight_threshold: u64,
    pub reorg_parent_weight_threshold: u64,
    pub reorg_max_epochs_since_finalization: u64,

    // Deposit contract
    pub deposit_chain_id: u64,
    pub deposit_network_id: u64,
    #[serde(with = "checksummed_address")]
    pub deposit_contract_address: Address,

    // Networking
    pub max_payload_size: u64,
    pub max_request_blocks: u64,
    pub epochs_per_subnet_subscription: u64,
    pub min_epochs_for_block_requests: u64,
    pub ttfb_timeout: u64,
    pub resp_timeout: u64,
    pub attestation_propagation_slot_range: u64,
    pub maximum_gossip_clock_disparity: u64,
    #[serde(with = "crate::b32_hex")]
    pub message_domain_invalid_snappy: B32,
    #[serde(with = "crate::b32_hex")]
    pub message_domain_valid_snappy: B32,
    pub subnets_per_node: u64,
    pub attestation_subnet_count: u64,
    pub attestation_subnet_extra_bits: u64,
    pub attestation_subnet_prefix_bits: u64,

    // Deneb
    pub max_request_blocks_deneb: u64,
    pub max_request_blob_sidecars: u64,
    pub min_epochs_for_blob_sidecars_requests: u64,
    pub blob_sidecar_subnet_count: u64,

    // Electra
    pub min_per_epoch_churn_limit_electra: u64,
    pub max_per_epoch_activation_exit_churn_limit: u64,
    pub blob_sidecar_subnet_count_electra: u64,
    pub max_blobs_per_block_electra: u64,
    pub max_request_blob_sidecars_electra: u64,
}

impl NetworkSpec {
    pub fn fork_digest(&self, genesis_validators_root: B256) -> B32 {
        ForkData {
            current_version: self.electra_fork_version,
            genesis_validators_root,
        }
        .compute_fork_digest()
    }

    pub fn fork_schedule(&self) -> ForkSchedule {
        ForkSchedule([
            Fork {
                previous_version: self.genesis_fork_version,
                current_version: self.genesis_fork_version,
                epoch: 0,
            },
            Fork {
                previous_version: self.genesis_fork_version,
                current_version: self.altair_fork_version,
                epoch: self.altair_fork_epoch,
            },
            Fork {
                previous_version: self.altair_fork_version,
                current_version: self.bellatrix_fork_version,
                epoch: self.bellatrix_fork_epoch,
            },
            Fork {
                previous_version: self.bellatrix_fork_version,
                current_version: self.capella_fork_version,
                epoch: self.capella_fork_epoch,
            },
            Fork {
                previous_version: self.capella_fork_version,
                current_version: self.deneb_fork_version,
                epoch: self.deneb_fork_epoch,
            },
            Fork {
                previous_version: self.deneb_fork_version,
                current_version: self.electra_fork_version,
                epoch: self.electra_fork_epoch,
            },
        ])
    }

    /// Returns the slot number for `n_days_ago` days ago.
    ///
    /// if n_days_ago is larger then the current slot, it returns 0.
    pub fn slot_n_days_ago(&self, n_days_ago: u64) -> u64 {
        let genesis_instant = UNIX_EPOCH + Duration::from_secs(self.min_genesis_time);
        let elapsed = SystemTime::now()
            .duration_since(genesis_instant)
            .expect("System Time is before the genesis time");
        let current_slot = elapsed.as_secs() / self.seconds_per_slot;
        current_slot.saturating_sub(n_days_ago * 24 * 60 * 60 / self.seconds_per_slot)
    }
}

pub static MAINNET: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        preset_base: "mainnet".to_string(),
        network: Network::Mainnet,
        terminal_total_difficulty: U256::from_str("58750000000000000000000")
            .expect("Could not get U256"),
        terminal_block_hash: b256!(
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        ),
        terminal_block_hash_activation_epoch: 18446744073709551615,
        min_genesis_active_validator_count: 16384,
        min_genesis_time: 1606824000,
        genesis_fork_version: fixed_bytes!("0x00000000"),
        genesis_delay: 604800,
        altair_fork_version: fixed_bytes!("0x01000000"),
        altair_fork_epoch: 74240,
        bellatrix_fork_version: fixed_bytes!("0x02000000"),
        bellatrix_fork_epoch: 144896,
        capella_fork_version: fixed_bytes!("0x03000000"),
        capella_fork_epoch: 194048,
        deneb_fork_version: fixed_bytes!("0x04000000"),
        deneb_fork_epoch: 269568,
        electra_fork_version: fixed_bytes!("0x05000000"),
        electra_fork_epoch: 364032,
        seconds_per_slot: 12,
        seconds_per_eth1_block: 14,
        min_validator_withdrawability_delay: 256,
        shard_committee_period: 256,
        eth1_follow_distance: 2048,
        inactivity_score_bias: 4,
        inactivity_score_recovery_rate: 16,
        ejection_balance: 16000000000,
        min_per_epoch_churn_limit: 4,
        churn_limit_quotient: 65536,
        max_per_epoch_activation_churn_limit: 8,
        proposer_score_boost: 40,
        reorg_head_weight_threshold: 20,
        reorg_parent_weight_threshold: 160,
        reorg_max_epochs_since_finalization: 2,
        deposit_chain_id: 1,
        deposit_network_id: 1,
        deposit_contract_address: address!("0x00000000219ab540356cBB839Cbe05303d7705Fa"),
        max_payload_size: 10485760,
        max_request_blocks: 1024,
        epochs_per_subnet_subscription: 256,
        min_epochs_for_block_requests: 33024,
        ttfb_timeout: 5,
        resp_timeout: 10,
        attestation_propagation_slot_range: 32,
        maximum_gossip_clock_disparity: 500,
        message_domain_invalid_snappy: fixed_bytes!("0x00000000"),
        message_domain_valid_snappy: fixed_bytes!("0x01000000"),
        subnets_per_node: 2,
        attestation_subnet_count: 64,
        attestation_subnet_extra_bits: 0,
        attestation_subnet_prefix_bits: 6,
        max_request_blocks_deneb: 128,
        max_request_blob_sidecars: 768,
        min_epochs_for_blob_sidecars_requests: 4096,
        blob_sidecar_subnet_count: 6,
        min_per_epoch_churn_limit_electra: 128000000000,
        max_per_epoch_activation_exit_churn_limit: 256000000000,
        blob_sidecar_subnet_count_electra: 9,
        max_blobs_per_block_electra: 9,
        max_request_blob_sidecars_electra: 1152,
    }
    .into()
});

pub static HOLESKY: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        preset_base: "mainnet".to_string(),
        network: Network::Holesky,
        terminal_total_difficulty: U256::from_str("58750000000000000000000")
            .expect("Could not get U256"),
        terminal_block_hash: b256!(
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        ),
        terminal_block_hash_activation_epoch: 18446744073709551615,
        min_genesis_active_validator_count: 16384,
        min_genesis_time: 1695902100,
        genesis_fork_version: fixed_bytes!("0x01017000"),
        genesis_delay: 300,
        altair_fork_version: fixed_bytes!("0x02017000"),
        altair_fork_epoch: 0,
        bellatrix_fork_version: fixed_bytes!("0x03017000"),
        bellatrix_fork_epoch: 0,
        capella_fork_version: fixed_bytes!("0x04017000"),
        capella_fork_epoch: 256,
        deneb_fork_version: fixed_bytes!("0x05017000"),
        deneb_fork_epoch: 29696,
        electra_fork_version: fixed_bytes!("0x06017000"),
        electra_fork_epoch: 115968,
        seconds_per_slot: 12,
        seconds_per_eth1_block: 14,
        min_validator_withdrawability_delay: 256,
        shard_committee_period: 256,
        eth1_follow_distance: 2048,
        inactivity_score_bias: 4,
        inactivity_score_recovery_rate: 16,
        ejection_balance: 16000000000,
        min_per_epoch_churn_limit: 4,
        churn_limit_quotient: 65536,
        max_per_epoch_activation_churn_limit: 8,
        proposer_score_boost: 40,
        reorg_head_weight_threshold: 20,
        reorg_parent_weight_threshold: 160,
        reorg_max_epochs_since_finalization: 2,
        deposit_chain_id: 1,
        deposit_network_id: 1,
        deposit_contract_address: address!("0x4242424242424242424242424242424242424242"),
        max_payload_size: 10485760,
        max_request_blocks: 1024,
        epochs_per_subnet_subscription: 256,
        min_epochs_for_block_requests: 33024,
        ttfb_timeout: 5,
        resp_timeout: 10,
        attestation_propagation_slot_range: 32,
        maximum_gossip_clock_disparity: 500,
        message_domain_invalid_snappy: fixed_bytes!("0x00000000"),
        message_domain_valid_snappy: fixed_bytes!("0x01000000"),
        subnets_per_node: 2,
        attestation_subnet_count: 64,
        attestation_subnet_extra_bits: 0,
        attestation_subnet_prefix_bits: 6,
        max_request_blocks_deneb: 128,
        max_request_blob_sidecars: 768,
        min_epochs_for_blob_sidecars_requests: 4096,
        blob_sidecar_subnet_count: 6,
        min_per_epoch_churn_limit_electra: 128000000000,
        max_per_epoch_activation_exit_churn_limit: 256000000000,
        blob_sidecar_subnet_count_electra: 9,
        max_blobs_per_block_electra: 9,
        max_request_blob_sidecars_electra: 1152,
    }
    .into()
});

pub static SEPOLIA: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        preset_base: "mainnet".to_string(),
        network: Network::Sepolia,
        terminal_total_difficulty: U256::from_str("58750000000000000000000")
            .expect("Could not get U256"),
        terminal_block_hash: b256!(
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        ),
        terminal_block_hash_activation_epoch: 18446744073709551615,
        min_genesis_active_validator_count: 1300,
        min_genesis_time: 1655647200,
        genesis_fork_version: fixed_bytes!("0x90000069"),
        genesis_delay: 86400,
        altair_fork_version: fixed_bytes!("0x90000070"),
        altair_fork_epoch: 50,
        bellatrix_fork_version: fixed_bytes!("0x90000071"),
        bellatrix_fork_epoch: 100,
        capella_fork_version: fixed_bytes!("0x90000072"),
        capella_fork_epoch: 56832,
        deneb_fork_version: fixed_bytes!("0x90000073"),
        deneb_fork_epoch: 132608,
        electra_fork_version: fixed_bytes!("0x90000074"),
        electra_fork_epoch: 222464,
        seconds_per_slot: 12,
        seconds_per_eth1_block: 14,
        min_validator_withdrawability_delay: 256,
        shard_committee_period: 256,
        eth1_follow_distance: 2048,
        inactivity_score_bias: 4,
        inactivity_score_recovery_rate: 16,
        ejection_balance: 16000000000,
        min_per_epoch_churn_limit: 4,
        churn_limit_quotient: 65536,
        max_per_epoch_activation_churn_limit: 8,
        proposer_score_boost: 40,
        reorg_head_weight_threshold: 20,
        reorg_parent_weight_threshold: 160,
        reorg_max_epochs_since_finalization: 2,
        deposit_chain_id: 1,
        deposit_network_id: 1,
        deposit_contract_address: address!("0x7f02C3E3c98b133055B8B348B2Ac625669Ed295D"),
        max_payload_size: 10485760,
        max_request_blocks: 1024,
        epochs_per_subnet_subscription: 256,
        min_epochs_for_block_requests: 33024,
        ttfb_timeout: 5,
        resp_timeout: 10,
        attestation_propagation_slot_range: 32,
        maximum_gossip_clock_disparity: 500,
        message_domain_invalid_snappy: fixed_bytes!("0x00000000"),
        message_domain_valid_snappy: fixed_bytes!("0x01000000"),
        subnets_per_node: 2,
        attestation_subnet_count: 64,
        attestation_subnet_extra_bits: 0,
        attestation_subnet_prefix_bits: 6,
        max_request_blocks_deneb: 128,
        max_request_blob_sidecars: 768,
        min_epochs_for_blob_sidecars_requests: 4096,
        blob_sidecar_subnet_count: 6,
        min_per_epoch_churn_limit_electra: 128000000000,
        max_per_epoch_activation_exit_churn_limit: 256000000000,
        blob_sidecar_subnet_count_electra: 9,
        max_blobs_per_block_electra: 9,
        max_request_blob_sidecars_electra: 1152,
    }
    .into()
});

pub static HOODI: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        preset_base: "mainnet".to_string(),
        network: Network::Hoodi,
        terminal_total_difficulty: U256::from_str("58750000000000000000000")
            .expect("Could not get U256"),
        terminal_block_hash: b256!(
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        ),
        terminal_block_hash_activation_epoch: 18446744073709551615,
        min_genesis_active_validator_count: 16384,
        min_genesis_time: 1742212800,
        genesis_fork_version: fixed_bytes!("0x10000910"),
        genesis_delay: 600,
        altair_fork_version: fixed_bytes!("0x20000910"),
        altair_fork_epoch: 0,
        bellatrix_fork_version: fixed_bytes!("0x30000910"),
        bellatrix_fork_epoch: 0,
        capella_fork_version: fixed_bytes!("0x40000910"),
        capella_fork_epoch: 0,
        deneb_fork_version: fixed_bytes!("0x50000910"),
        deneb_fork_epoch: 0,
        electra_fork_version: fixed_bytes!("0x60000910"),
        electra_fork_epoch: 2048,
        seconds_per_slot: 12,
        seconds_per_eth1_block: 14,
        min_validator_withdrawability_delay: 256,
        shard_committee_period: 256,
        eth1_follow_distance: 2048,
        inactivity_score_bias: 4,
        inactivity_score_recovery_rate: 16,
        ejection_balance: 16000000000,
        min_per_epoch_churn_limit: 4,
        churn_limit_quotient: 65536,
        max_per_epoch_activation_churn_limit: 8,
        proposer_score_boost: 40,
        reorg_head_weight_threshold: 20,
        reorg_parent_weight_threshold: 160,
        reorg_max_epochs_since_finalization: 2,
        deposit_chain_id: 1,
        deposit_network_id: 1,
        deposit_contract_address: address!("0x00000000219ab540356cBB839Cbe05303d7705Fa"),
        max_payload_size: 10485760,
        max_request_blocks: 1024,
        epochs_per_subnet_subscription: 256,
        min_epochs_for_block_requests: 33024,
        ttfb_timeout: 5,
        resp_timeout: 10,
        attestation_propagation_slot_range: 32,
        maximum_gossip_clock_disparity: 500,
        message_domain_invalid_snappy: fixed_bytes!("0x00000000"),
        message_domain_valid_snappy: fixed_bytes!("0x01000000"),
        subnets_per_node: 2,
        attestation_subnet_count: 64,
        attestation_subnet_extra_bits: 0,
        attestation_subnet_prefix_bits: 6,
        max_request_blocks_deneb: 128,
        max_request_blob_sidecars: 768,
        min_epochs_for_blob_sidecars_requests: 4096,
        blob_sidecar_subnet_count: 6,
        min_per_epoch_churn_limit_electra: 128000000000,
        max_per_epoch_activation_exit_churn_limit: 256000000000,
        blob_sidecar_subnet_count_electra: 9,
        max_blobs_per_block_electra: 9,
        max_request_blob_sidecars_electra: 1152,
    }
    .into()
});

pub static DEV: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        preset_base: "mainnet".to_string(),
        network: Network::Dev,
        terminal_total_difficulty: U256::from_str("58750000000000000000000")
            .expect("Could not get U256"),
        terminal_block_hash: b256!(
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        ),
        terminal_block_hash_activation_epoch: 18446744073709551615,
        min_genesis_active_validator_count: 16384,
        min_genesis_time: 1606824000,
        genesis_fork_version: fixed_bytes!("0x00000000"),
        genesis_delay: 604800,
        altair_fork_version: fixed_bytes!("0x01000000"),
        altair_fork_epoch: 74240,
        bellatrix_fork_version: fixed_bytes!("0x02000000"),
        bellatrix_fork_epoch: 144896,
        capella_fork_version: fixed_bytes!("0x03000000"),
        capella_fork_epoch: 194048,
        deneb_fork_version: fixed_bytes!("0x04000000"),
        deneb_fork_epoch: 269568,
        electra_fork_version: fixed_bytes!("0x05000000"),
        electra_fork_epoch: 364032,
        seconds_per_slot: 12,
        seconds_per_eth1_block: 14,
        min_validator_withdrawability_delay: 256,
        shard_committee_period: 256,
        eth1_follow_distance: 2048,
        inactivity_score_bias: 4,
        inactivity_score_recovery_rate: 16,
        ejection_balance: 16000000000,
        min_per_epoch_churn_limit: 4,
        churn_limit_quotient: 65536,
        max_per_epoch_activation_churn_limit: 8,
        proposer_score_boost: 40,
        reorg_head_weight_threshold: 20,
        reorg_parent_weight_threshold: 160,
        reorg_max_epochs_since_finalization: 2,
        deposit_chain_id: 1,
        deposit_network_id: 1,
        deposit_contract_address: address!("0x00000000219ab540356cBB839Cbe05303d7705Fa"),
        max_payload_size: 10485760,
        max_request_blocks: 1024,
        epochs_per_subnet_subscription: 256,
        min_epochs_for_block_requests: 33024,
        ttfb_timeout: 5,
        resp_timeout: 10,
        attestation_propagation_slot_range: 32,
        maximum_gossip_clock_disparity: 500,
        message_domain_invalid_snappy: fixed_bytes!("0x00000000"),
        message_domain_valid_snappy: fixed_bytes!("0x01000000"),
        subnets_per_node: 2,
        attestation_subnet_count: 64,
        attestation_subnet_extra_bits: 0,
        attestation_subnet_prefix_bits: 6,
        max_request_blocks_deneb: 128,
        max_request_blob_sidecars: 768,
        min_epochs_for_blob_sidecars_requests: 4096,
        blob_sidecar_subnet_count: 6,
        min_per_epoch_churn_limit_electra: 128000000000,
        max_per_epoch_activation_exit_churn_limit: 256000000000,
        blob_sidecar_subnet_count_electra: 9,
        max_blobs_per_block_electra: 9,
        max_request_blob_sidecars_electra: 1152,
    }
    .into()
});
