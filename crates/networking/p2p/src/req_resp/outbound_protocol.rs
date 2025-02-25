use std::{future::Future, pin::Pin};

use libp2p::{core::UpgradeInfo, OutboundUpgrade};

use super::{
    error::ReqRespError,
    protocol_id::{ProtocolId, SupportedProtocol},
};

#[derive(Debug, Clone)]
pub struct ReqRespOutboundProtocol {}

impl<C> OutboundUpgrade<C> for ReqRespOutboundProtocol {
    type Output = ();

    type Error = ReqRespError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, _socket: C, _info: Self::Info) -> Self::Future {
        todo!()
    }
}

impl UpgradeInfo for ReqRespOutboundProtocol {
    type Info = ProtocolId;

    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        SupportedProtocol::supported_protocols()
    }
}
