use serde::{Deserialize, Serialize};

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct GenesisData{
    pub genesis_time:String,
    pub genesis_validator_root:String,
    pub genesis_fork_version:String,
}