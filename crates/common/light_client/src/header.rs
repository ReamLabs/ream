use ream_consensus::beacon_block_header::BeaconBlockHeader;
use serde::Serialize;

#[derive(Serialize)]
pub struct LightClientHeader {
    pub beacon: BeaconBlockHeader,
}
