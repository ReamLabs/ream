use core::fmt;
use std::{fmt::Display, str::FromStr};

use alloy_primitives::{B256, hex};
use ream_bls::{PubKey, pubkey::pubkey_from_str};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ID {
    Slot(u64),
    Root(B256), // expected to be a 0x-prefixed hex string
}

impl FromStr for ID {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("0x") {
            B256::from_str(s)
                .map(ID::Root)
                .map_err(|e| format!("Invalid hex root: {}", e))
        } else {
            s.parse::<u64>()
                .map(ID::Slot)
                .map_err(|e| format!("Invalid slot: {}", e))
        }
    }
}

impl fmt::Display for ID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ID::Slot(slot) => write!(f, "{slot}"),
            ID::Root(root) => write!(f, "0x{}", hex::encode(root)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValidatorID {
    Index(u64),
    Address(PubKey), // expected to be a 0x-prefixed hex string
}

impl FromStr for ValidatorID {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("0x") {
            pubkey_from_str(s)
                .map(ValidatorID::Address)
                .map_err(|e| format!("Invalid hex root: {}", e))
        } else {
            s.parse::<u64>()
                .map(ValidatorID::Index)
                .map_err(|e| format!("Invalid slot: {}", e))
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
