use std::{future::Future, pin::Pin};

use libp2p::{core::UpgradeInfo, InboundUpgrade, OutboundUpgrade};
use void::Void;

#[derive(Clone)]
pub struct ProtocolId {
    pub protocol_id: String,
}

impl AsRef<str> for ProtocolId {
    fn as_ref(&self) -> &str {
        &self.protocol_id
    }
}

pub struct GossipsubProtocol {
    pub protocol_ids: Vec<ProtocolId>,
}

impl UpgradeInfo for GossipsubProtocol {
    type Info = ProtocolId;
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.protocol_ids.clone()
    }
}

impl<T> InboundUpgrade<T> for GossipsubProtocol {
    type Output = ();
    type Error = Void;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, _socket: T, _info: Self::Info) -> Self::Future {
        todo!()
    }
}

impl<T> OutboundUpgrade<T> for GossipsubProtocol {
    type Output = ();
    type Error = Void;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, _socket: T, _info: Self::Info) -> Self::Future {
        todo!()
    }
}
