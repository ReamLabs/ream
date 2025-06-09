#[derive(thiserror::Error, Debug)]
pub enum GossipsubError {
    #[error("Gossipsub invalid data {0}")]
    InvalidData(String),
    #[error("Gossipsub invalid topic {0}")]
    InvalidTopic(String),
}

impl From<ssz::DecodeError> for GossipsubError {
    fn from(err: ssz::DecodeError) -> Self {
        GossipsubError::InvalidData(format!("Failed to decode ssz: {err:?}"))
    }
}
