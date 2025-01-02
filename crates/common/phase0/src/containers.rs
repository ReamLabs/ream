use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use crate::primitives::{Epoch, Version};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Deserialize, Serialize)]
pub struct Fork {
    pub previous_version: Version,
    pub current_version: Version,
    pub epoch: Epoch,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Deserialize, Serialize)]
pub struct ForkData {
    pub curent_version: Version,
    pub genesis_validators: B256,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Deserialize, Serialize)]
pub struct Checkpoint {
    pub epoch: Epoch,
    pub root: B256,
}
