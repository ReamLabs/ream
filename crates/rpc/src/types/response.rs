use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

pub const VERSION: &str = "electra";
pub const ETH_CONSENSUS_VERSION_HEADER: &str = "Eth-Consensus-Version";
const EXECUTION_OPTIMISTIC: bool = false;
const FINALIZED: bool = false;

/// A DataResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!(T)
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct DataResponse<T> {
    pub data: T,
}

impl<T: Serialize> DataResponse<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

#[derive(Serialize, Deserialize)]
pub struct RootResponse {
    pub root: B256,
}

impl RootResponse {
    pub fn new(root: B256) -> Self {
        Self { root }
    }
}

/// A BeaconResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!({
///     "execution_optimistic" : bool,
///     "finalized" : bool,
///     "data" : T
/// })
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct BeaconResponse<T> {
    pub execution_optimistic: bool,
    pub finalized: bool,
    pub data: T,
}

impl<T: Serialize> BeaconResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            execution_optimistic: EXECUTION_OPTIMISTIC,
            finalized: FINALIZED,
        }
    }
}

/// A BeaconVersionedResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!({
///     "version": "electra"
///     "execution_optimistic" : bool,
///     "finalized" : bool,
///     "data" : T
/// })
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct BeaconVersionedResponse<T> {
    pub version: String,
    pub execution_optimistic: bool,
    pub finalized: bool,
    pub data: T,
}

impl<T: Serialize> BeaconVersionedResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            version: VERSION.into(),
            data,
            execution_optimistic: EXECUTION_OPTIMISTIC,
            finalized: FINALIZED,
        }
    }
}

/// A DataVersionedResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///     "version": "electra",
///     "data": T
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct DataVersionedResponse<T> {
    pub version: String,
    pub data: T,
}

impl<T: Serialize> DataVersionedResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            version: VERSION.into(),
            data,
        }
    }
}

/// A OptionalBeaconVersionedResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!({
///     "version": Some("electra")
///     "execution_optimistic" : Some("false"),
///     "finalized" : None,
///     "data" : T
/// })
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct OptionalBeaconVersionedResponse<T> {
    pub version: Option<String>,
    #[serde(default, deserialize_with = "option_bool_from_str_or_bool")]
    pub execution_optimistic: Option<bool>,
    #[serde(default, deserialize_with = "option_bool_from_str_or_bool")]
    pub finalized: Option<bool>,
    pub data: T,
}

impl<T: Serialize> OptionalBeaconVersionedResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            version: Some(VERSION.into()),
            data,
            execution_optimistic: Some(EXECUTION_OPTIMISTIC),
            finalized: Some(FINALIZED),
        }
    }
}

fn bool_from_str_or_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BoolVisitor;

    impl serde::de::Visitor<'_> for BoolVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean or a string representing a boolean")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
            Ok(v)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse::<bool>().map_err(E::custom)
        }
    }

    deserializer.deserialize_any(BoolVisitor)
}

fn option_bool_from_str_or_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(bool_from_str_or_bool(deserializer)?))
}
