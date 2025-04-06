use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct EpochQuery {
    pub epoch: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SlotQuery {
    pub slot: Option<u64>,
}
#[derive(Debug, Deserialize)]
pub struct IndexQuery {
    pub index: Option<u64>,
}
