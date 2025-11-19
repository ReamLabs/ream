use std::{
    fmt::{self, Display},
    sync::{Arc, Once, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use tracing::warn;

static HAS_NETWORK_SPEC_BEEN_INITIALIZED: Once = Once::new();

/// Static specification of the Lean Chain network.
pub static LEAN_NETWORK_SPEC: OnceLock<Arc<LeanNetworkSpec>> = OnceLock::new();

/// SHOULD be called only once at the start of the application to initialize static
/// [LeanNetworkSpec].
///
/// The static `LeanNetworkSpec` can be accessed using [lean_network_spec].
pub fn set_lean_network_spec(network_spec: Arc<LeanNetworkSpec>) {
    HAS_NETWORK_SPEC_BEEN_INITIALIZED.call_once(|| {
        LEAN_NETWORK_SPEC
            .set(network_spec)
            .expect("LeanNetworkSpec should be set only once at the start of the application");
    });

    if HAS_NETWORK_SPEC_BEEN_INITIALIZED.is_completed() {
        warn!(
            "LeanNetworkSpec has already been initialized. Subsequent calls to set_lean_network_spec will be ignored. If this is production code, this is likely a bug."
        );
    }
}

/// Returns the static [LeanNetworkSpec] initialized by [set_lean_network_spec].
///
/// # Panics
///
/// Panics if [set_lean_network_spec] wasn't called before this function.
pub fn lean_network_spec() -> Arc<LeanNetworkSpec> {
    LEAN_NETWORK_SPEC
        .get()
        .expect("LeanNetworkSpec wasn't set")
        .clone()
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum Devnet {
    #[default]
    One,
    Two,
}

impl Display for Devnet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Devnet::One => write!(f, "One"),
            Devnet::Two => write!(f, "Two"),
        }
    }
}

/// Use 3 as the default justification lookback slots if not specified.
fn default_justification_lookback_slots() -> u64 {
    3
}

/// Use 4 seconds as the default seconds per slot if not specified.
fn default_seconds_per_slot() -> u64 {
    4
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub struct LeanNetworkSpec {
    pub genesis_time: u64,
    #[serde(default = "default_justification_lookback_slots")]
    pub justification_lookback_slots: u64,
    #[serde(default = "default_seconds_per_slot")]
    pub seconds_per_slot: u64,
    #[serde(alias = "VALIDATOR_COUNT")]
    pub num_validators: u64,

    /// Skipped in YAML, defaults to Devnet::One
    #[serde(skip)]
    pub devnet: Devnet,
}

impl LeanNetworkSpec {
    /// Creates a new instance of `LeanNetworkSpec` for the Ephemery network
    /// that starts 3 seconds after the current system time,
    pub fn ephemery() -> Self {
        let current_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .as_secs();

        Self {
            genesis_time: current_timestamp + 10,
            justification_lookback_slots: 3,
            seconds_per_slot: 4,
            num_validators: 4,
            devnet: Devnet::One,
        }
    }

    pub fn is_devnet_enabled(&self, target: Devnet) -> bool {
        match self.devnet {
            Devnet::Two => true,
            Devnet::One => matches!(target, Devnet::One),
        }
    }
}
