use std::str::FromStr;

use alloy_primitives::hex;
use ream_bls::PublicKey;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ValidatorID {
    Index(u64),
    /// expected to be a 0x-prefixed hex string.
    Address(PublicKey),
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
            PublicKey::from_str(&s)
                .map(ValidatorID::Address)
                .map_err(|err| {
                    serde::de::Error::custom(format!("Invalid hex address: {s}, error: {err}"))
                })
        } else if s.chars().all(|c| c.is_ascii_digit()) {
            s.parse::<u64>().map(ValidatorID::Index).map_err(|err| {
                serde::de::Error::custom(format!("Invalid validator index: {s}, error: {err}"))
            })
        } else {
            Err(serde::de::Error::custom(format!(
                "Invalid validator ID: {s}"
            )))
        }
    }
}

impl std::fmt::Display for ValidatorID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidatorID::Index(i) => write!(f, "{i}"),
            ValidatorID::Address(pub_key) => write!(f, "0x{}", hex::encode(pub_key.to_bytes())),
        }
    }
}
