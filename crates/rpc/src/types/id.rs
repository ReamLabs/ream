use core::fmt;
use std::{fmt::Display, str::FromStr};

use alloy_primitives::{B256, hex};
use ream_bls::PubKey;
use serde::{Deserialize, Serialize};

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
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "finalized" => Ok(ID::Finalized),
            "genesis" => Ok(ID::Genesis),
            "head" => Ok(ID::Head),
            "justified" => Ok(ID::Justified),
            _ => {
                if s.starts_with("0x") {
                    B256::from_str(&s)
                        .map(ID::Root)
                        .map_err(|_| serde::de::Error::custom(format!("Invalid hex root: {s}")))
                } else if s.chars().all(|c| c.is_ascii_digit()) {
                    s.parse::<u64>()
                        .map(ID::Slot)
                        .map_err(|_| serde::de::Error::custom(format!("Invalid slot number: {s}")))
                } else {
                    Err(serde::de::Error::custom(format!("Invalid state ID: {s}")))
                }
            }
        }
    }
}

impl fmt::Display for ID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

#[derive(Debug, Clone)]
pub enum ValidatorID {
    Index(u64),
    /// expected to be a 0x-prefixed hex string.
    Address(PubKey),
}

impl Serialize for ValidatorID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ValidatorID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.starts_with("0x") {
            PubKey::from_str(&s)
                .map(ValidatorID::Address)
                .map_err(|_| serde::de::Error::custom(format!("Invalid hex address: {s}")))
        } else if s.chars().all(|c| c.is_ascii_digit()) {
            s.parse::<u64>()
                .map(ValidatorID::Index)
                .map_err(|_| serde::de::Error::custom(format!("Invalid validator index: {s}")))
        } else {
            Err(serde::de::Error::custom(format!(
                "Invalid validator ID: {s}"
            )))
        }
    }
}

impl Display for ValidatorID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidatorID::Index(i) => write!(f, "{i}"),
            ValidatorID::Address(pub_key) => write!(f, "0x{:?}", pub_key.to_bytes()),
        }
    }
}
