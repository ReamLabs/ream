use std::{
    sync::{Arc, LazyLock, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use alloy_primitives::FixedBytes;
#[cfg(feature = "devnet4")]
use serde::Serialize;
use serde::{Deserialize, Deserializer};
use tracing::warn;

/// Static specification of the Lean Chain network.
/// Uses RwLock to allow resetting in tests while maintaining thread-safety.
static LEAN_NETWORK_SPEC: RwLock<Option<Arc<LeanNetworkSpec>>> = RwLock::new(None);

/// SHOULD be called only once at the start of the application to initialize static
/// [LeanNetworkSpec].
///
/// The static `LeanNetworkSpec` can be accessed using [lean_network_spec].
pub fn set_lean_network_spec(network_spec: Arc<LeanNetworkSpec>) {
    let mut spec = LEAN_NETWORK_SPEC
        .write()
        .expect("LEAN_NETWORK_SPEC RwLock poisoned");
    if spec.is_some() {
        warn!("LeanNetworkSpec has already been initialized. Overwriting with new spec.");
    }
    *spec = Some(network_spec);
}

pub fn initialize_lean_test_network_spec() {
    set_lean_network_spec(TEST.clone());
}

/// Returns the static [LeanNetworkSpec] initialized by [set_lean_network_spec].
///
/// # Panics
///
/// Panics if [set_lean_network_spec] wasn't called before this function.
pub fn lean_network_spec() -> Arc<LeanNetworkSpec> {
    LEAN_NETWORK_SPEC
        .read()
        .expect("LEAN_NETWORK_SPEC RwLock poisoned")
        .as_ref()
        .expect("LeanNetworkSpec wasn't set")
        .clone()
}

/// Use 3 as the default justification lookback slots if not specified.
fn default_justification_lookback_slots() -> u64 {
    3
}

/// Use 4 seconds as the default seconds per slot if not specified.
fn default_seconds_per_slot() -> u64 {
    4
}

/// A single validator's public keys in the genesis configuration (devnet4).
#[cfg(feature = "devnet4")]
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize, Default)]
pub struct GenesisValidatorEntry {
    pub attestation_public_key: FixedBytes<52>,
    pub proposal_public_key: FixedBytes<52>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub struct LeanNetworkSpec {
    pub genesis_time: u64,
    #[serde(alias = "VALIDATOR_COUNT")]
    pub num_validators: u64,

    #[cfg(feature = "devnet3")]
    #[serde(alias = "GENESIS_VALIDATORS")]
    pub validator_public_keys: Vec<FixedBytes<52>>,

    #[cfg(feature = "devnet4")]
    #[serde(alias = "GENESIS_VALIDATORS")]
    pub genesis_validators: Vec<GenesisValidatorEntry>,

    #[serde(default = "default_justification_lookback_slots")]
    pub justification_lookback_slots: u64,
    #[serde(default = "default_seconds_per_slot")]
    pub seconds_per_slot: u64,

    /// Capture any extra fields we aren't interested in
    #[serde(flatten)]
    discarded_values: DiscardUnknown,
}

impl LeanNetworkSpec {
    /// Creates a new instance of `LeanNetworkSpec` for the Ephemery network
    /// that starts 3 seconds after the current system time,
    pub fn ephemery() -> Self {
        let current_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .as_secs();

        #[cfg(feature = "devnet3")]
        let config: &str = include_str!("../../../../../bin/ream/assets/lean/config.yaml");
        #[cfg(feature = "devnet4")]
        let config: &str = include_str!("../../../../../bin/ream/assets/lean/config-devnet4.yaml");
        let config = serde_yaml::from_str::<LeanNetworkSpec>(config)
            .expect("Our sample config should always be correct");

        Self {
            genesis_time: current_timestamp + 10,
            justification_lookback_slots: 3,
            seconds_per_slot: 4,
            num_validators: config.num_validators,
            #[cfg(feature = "devnet3")]
            validator_public_keys: config.validator_public_keys,
            #[cfg(feature = "devnet4")]
            genesis_validators: config.genesis_validators,
            discarded_values: DiscardUnknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
struct DiscardUnknown;

impl<'de> Deserialize<'de> for DiscardUnknown {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // just consume it without keeping anything
        let _ = serde_yaml::Value::deserialize(deserializer)?;
        Ok(DiscardUnknown)
    }
}

pub static TEST: LazyLock<Arc<LeanNetworkSpec>> =
    LazyLock::new(|| LeanNetworkSpec::ephemery().into());
