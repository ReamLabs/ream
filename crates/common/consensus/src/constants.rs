use std::sync::OnceLock;

use alloy_primitives::{B256, aliases::B32, fixed_bytes};

pub const BASE_REWARDS_PER_EPOCH: u64 = 4;
pub const BASE_REWARD_FACTOR: u64 = 64;
pub const BEACON_STATE_MERKLE_DEPTH: u64 = 6;
pub const BLOB_KZG_COMMITMENTS_INDEX: u64 = 11;
pub const BLOCK_BODY_MERKLE_DEPTH: u64 = 4;
pub const BYTES_PER_BLOB: usize = BYTES_PER_FIELD_ELEMENT * FIELD_ELEMENTS_PER_BLOB;
pub const BYTES_PER_COMMITMENT: usize = 48;
pub const BYTES_PER_FIELD_ELEMENT: usize = 32;
pub const BYTES_PER_PROOF: usize = 48;
pub const CAPELLA_FORK_VERSION: B32 = fixed_bytes!("0x03000000");
pub const CHURN_LIMIT_QUOTIENT: u64 = 65536;
pub const CURRENT_SYNC_COMMITTEE_INDEX: u64 = 22;
pub const DEPOSIT_CONTRACT_TREE_DEPTH: u64 = 32;
pub const DOMAIN_AGGREGATE_AND_PROOF: B32 = fixed_bytes!("0x06000000");
pub const DOMAIN_BEACON_ATTESTER: B32 = fixed_bytes!("0x01000000");
pub const DOMAIN_BEACON_PROPOSER: B32 = fixed_bytes!("0x00000000");
pub const DOMAIN_BLS_TO_EXECUTION_CHANGE: B32 = fixed_bytes!("0x0A000000");
pub const DOMAIN_DEPOSIT: B32 = fixed_bytes!("0x03000000");
pub const DOMAIN_RANDAO: B32 = fixed_bytes!("0x02000000");
pub const DOMAIN_SYNC_COMMITTEE: B32 = fixed_bytes!("0x07000000");
pub const DOMAIN_VOLUNTARY_EXIT: B32 = fixed_bytes!("0x04000000");
pub const EFFECTIVE_BALANCE_INCREMENT: u64 = 1_000_000_000;
pub const EJECTION_BALANCE: u64 = 16000000000;
pub const EPOCHS_PER_ETH1_VOTING_PERIOD: u64 = 64;
pub const EPOCHS_PER_HISTORICAL_VECTOR: u64 = 65536;
pub const EPOCHS_PER_SLASHINGS_VECTOR: u64 = 8192;
pub const EPOCHS_PER_SYNC_COMMITTEE_PERIOD: u64 = 256;
pub const EXECUTION_PAYLOAD_INDEX: u64 = 9;
pub const FAR_FUTURE_EPOCH: u64 = 18446744073709551615;
pub const FIELD_ELEMENTS_PER_BLOB: usize = 4096;
pub const FINALIZED_CHECKPOINT_INDEX: u64 = 20;
pub const GENESIS_SLOT: u64 = 0;
pub const GENESIS_EPOCH: u64 = 0;
pub const GENESIS_FORK_VERSION: B32 = fixed_bytes!("0x00000000");
pub const HYSTERESIS_DOWNWARD_MULTIPLIER: u64 = 1;
pub const HYSTERESIS_UPWARD_MULTIPLIER: u64 = 5;
pub const HYSTERESIS_QUOTIENT: u64 = 4;
pub const INACTIVITY_PENALTY_QUOTIENT_BELLATRIX: u64 = 16777216;
pub const INTERVALS_PER_SLOT: u64 = 3;
pub const INACTIVITY_SCORE_BIAS: u64 = 4;
pub const INACTIVITY_SCORE_RECOVERY_RATE: u64 = 16;
pub const JUSTIFICATION_BITS_LENGTH: usize = 4;
pub const KZG_COMMITMENTS_MERKLE_DEPTH: u64 = 12;
pub const MAX_COMMITTEES_PER_SLOT: u64 = 64;
pub const MAX_DEPOSITS: u64 = 16;
pub const MAX_SEED_LOOKAHEAD: u64 = 4;
pub const MAX_PER_EPOCH_ACTIVATION_CHURN_LIMIT: u64 = 8;
pub const MAX_RANDOM_VALUE: u64 = 65535;
pub const MAX_VALIDATORS_PER_COMMITTEE: u64 = 2048;
pub const MAX_VALIDATORS_PER_WITHDRAWALS_SWEEP: usize = 16384;
pub const MAX_WITHDRAWALS_PER_PAYLOAD: u64 = 16;
pub const MIN_ATTESTATION_INCLUSION_DELAY: u64 = 1;
pub const MIN_EPOCHS_TO_INACTIVITY_PENALTY: u64 = 4;
pub const MIN_GENESIS_ACTIVE_VALIDATOR_COUNT: u64 = 16384;
pub const MIN_GENESIS_TIME: u64 = 1606824000;
pub const MIN_PER_EPOCH_CHURN_LIMIT: u64 = 4;
pub const MIN_SEED_LOOKAHEAD: u64 = 1;
pub const MIN_VALIDATOR_WITHDRAWABILITY_DELAY: u64 = 256;
pub const NEXT_SYNC_COMMITTEE_INDEX: u64 = 23;
pub const NUM_FLAG_INDICES: usize = 3;
pub const PROPORTIONAL_SLASHING_MULTIPLIER_BELLATRIX: u64 = 3;
pub const PROPOSER_REWARD_QUOTIENT: u64 = 8;
pub const PROPOSER_WEIGHT: u64 = 8;
pub const REORG_PARENT_WEIGHT_THRESHOLD: u64 = 160;
pub const SECONDS_PER_SLOT: u64 = 12;
pub const SHARD_COMMITTEE_PERIOD: u64 = 256;
pub const SHUFFLE_ROUND_COUNT: u8 = 90;
pub const SLOTS_PER_EPOCH: u64 = 32;
pub const SLOTS_PER_HISTORICAL_ROOT: u64 = 8192;
pub const SYNC_COMMITTEE_SIZE: u64 = 512;
pub const SYNC_REWARD_WEIGHT: u64 = 2;
pub const TARGET_COMMITTEE_SIZE: u64 = 128;
pub const TIMELY_HEAD_FLAG_INDEX: u8 = 2;
pub const TIMELY_SOURCE_FLAG_INDEX: u8 = 0;
pub const TIMELY_TARGET_FLAG_INDEX: u8 = 1;
pub const TIMELY_SOURCE_WEIGHT: u64 = 14;
pub const TIMELY_TARGET_WEIGHT: u64 = 26;
pub const TIMELY_HEAD_WEIGHT: u64 = 14;
pub const UINT64_MAX: u64 = u64::MAX;
pub const UINT64_MAX_SQRT: u64 = 4294967295;
pub const WEIGHT_DENOMINATOR: u64 = 64;
pub const WHISTLEBLOWER_REWARD_QUOTIENT: u64 = 512;

// Withdrawal prefixes
pub const BLS_WITHDRAWAL_PREFIX: &[u8] = &[0];
pub const COMPOUNDING_WITHDRAWAL_PREFIX: &[u8] = &[2];
pub const ETH1_ADDRESS_WITHDRAWAL_PREFIX: &[u8] = &[1];

// Execution layer triggered requests
pub const CONSOLIDATION_REQUEST_TYPE: u8 = 2;
pub const DEPOSIT_REQUEST_TYPE: u8 = 0;
pub const WITHDRAWAL_REQUEST_TYPE: u8 = 1;

// Rewards and penalties
pub const MIN_SLASHING_PENALTY_QUOTIENT_ELECTRA: u64 = 4096;
pub const WHISTLEBLOWER_REWARD_QUOTIENT_ELECTRA: u64 = 4096;

// Withdrawals processing
pub const MAX_PENDING_PARTIALS_PER_WITHDRAWALS_SWEEP: u64 = 8;

// Misc
pub const FULL_EXIT_REQUEST_AMOUNT: u64 = 0;
pub const UNSET_DEPOSIT_REQUESTS_START_INDEX: u64 = u64::MAX;

// State list lengths
pub const PENDING_CONSOLIDATIONS_LIMIT: u64 = 262_144;
pub const PENDING_PARTIAL_WITHDRAWALS_LIMIT: u64 = 134_217_728;

// Gwei values
pub const MAX_EFFECTIVE_BALANCE_ELECTRA: u64 = 2_048_000_000_000;
pub const MIN_ACTIVATION_BALANCE: u64 = 32_000_000_000;

// Pending deposits processing
pub const MAX_PENDING_DEPOSITS_PER_EPOCH: u64 = 16;

// Execution
pub const MAX_BLOBS_PER_BLOCK_ELECTRA: u64 = 9;

// Validator cycle
pub const MAX_PER_EPOCH_ACTIVATION_EXIT_CHURN_LIMIT: u64 = 256_000_000_000;
pub const MIN_PER_EPOCH_CHURN_LIMIT_ELECTRA: u64 = 128_000_000_000;

pub const PARTICIPATION_FLAG_WEIGHTS: [u64; NUM_FLAG_INDICES] = [
    TIMELY_SOURCE_WEIGHT,
    TIMELY_TARGET_WEIGHT,
    TIMELY_HEAD_WEIGHT,
];

pub static GENESIS_VALIDATORS_ROOT: OnceLock<B256> = OnceLock::new();

/// MUST be called only once at the start of the application to initialize static
/// [B256].
///
/// The static `B256` can be accessed using [genesis_validators_root].
///
/// # Panics
///
/// Panics if this function is called more than once.
pub fn set_genesis_validator_root(genesis_validators_root: B256) {
    GENESIS_VALIDATORS_ROOT
        .set(genesis_validators_root)
        .expect("GENESIS_VALIDATORS_ROOT should be set only once at the start of the application");
}

/// Returns the static [B256] initialized by [set_genesis_validator_root].
///
/// # Panics
///
/// Panics if [set_genesis_validator_root] wasn't called before this function.
pub fn genesis_validators_root() -> B256 {
    *GENESIS_VALIDATORS_ROOT
        .get()
        .expect("GENESIS_VALIDATORS_ROOT wasn't set")
}
