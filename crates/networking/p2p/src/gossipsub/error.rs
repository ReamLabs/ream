#[derive(thiserror::Error, Debug)]
pub enum GossipsubError {
    #[error("Invalid data {0}")]
    InvalidData(String),
    #[error("Invalid topic {0}")]
    InvalidTopic(String),
}

impl From<ssz::DecodeError> for GossipsubError {
    fn from(err: ssz::DecodeError) -> Self {
        GossipsubError::InvalidData(format!("Failed to decode ssz: {err:?}"))
    }
}
