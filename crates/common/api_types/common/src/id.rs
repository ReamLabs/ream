use std::str::FromStr;

use alloy_primitives::{B256, hex};
use serde::{Deserialize, Serialize};

/// [ID] can be used to identify a specific state (`state_id`) or block (`block_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ID {
    Finalized,
    Genesis,
    Head,
    Justified,
    Slot(u64),
    /// expected to be a 0x-prefixed hex string.
    Root(B256),
}

impl Serialize for ID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        match string.to_lowercase().as_str() {
            "finalized" => Ok(ID::Finalized),
            "genesis" => Ok(ID::Genesis),
            "head" => Ok(ID::Head),
            "justified" => Ok(ID::Justified),
            _ => {
                if string.starts_with("0x") {
                    B256::from_str(&string).map(ID::Root).map_err(|err| {
                        serde::de::Error::custom(format!("Invalid hex root: {string}, error: {err}"))
                    })
                } else if string.chars().all(|c| c.is_ascii_digit()) {
                    string.parse::<u64>().map(ID::Slot).map_err(|err| {
                        serde::de::Error::custom(format!("Invalid slot number: {string}, error: {err}"))
                    })
                } else {
                    Err(serde::de::Error::custom(format!("Invalid state ID: {string}")))
                }
            }
        }
    }
}

impl std::fmt::Display for ID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ID::Finalized => write!(f, "finalized"),
            ID::Genesis => write!(f, "genesis"),
            ID::Head => write!(f, "head"),
            ID::Justified => write!(f, "justified"),
            ID::Slot(slot) => write!(f, "{slot}"),
            ID::Root(root) => write!(f, "0x{}", hex::encode(root)),
        }
    }
}
