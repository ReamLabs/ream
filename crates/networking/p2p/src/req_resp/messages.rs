use std::sync::Arc;

use ssz_derive::{Decode, Encode};

use super::{
    beacon::messages::{BeaconRequestMessage, BeaconResponseMessage},
    lean::messages::{LeanRequestMessage, LeanResponseMessage},
    protocol_id::ProtocolId,
};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum RequestMessage {
    Beacon(BeaconRequestMessage),
    Lean(LeanRequestMessage),
}

impl RequestMessage {
    pub fn supported_protocols(&self) -> Vec<ProtocolId> {
        match self {
            RequestMessage::Beacon(request_message) => request_message.supported_protocols(),
            RequestMessage::Lean(request_message) => request_message.supported_protocols(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum ResponseMessage {
    Beacon(Arc<BeaconResponseMessage>),
    Lean(LeanResponseMessage),
}
