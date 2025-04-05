use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RandaoQuery {
    pub epoch: Option<u64>,
}
