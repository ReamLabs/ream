use std::{future::Future, pin::Pin};

use libp2p::{core::UpgradeInfo, InboundUpgrade};

use super::{
    error::ReqRespError,
    protocol_id::{ProtocolId, SupportedProtocol},
};

#[derive(Debug, Clone)]
pub struct ReqRespInboundProtocol {}

impl<C> InboundUpgrade<C> for ReqRespInboundProtocol {
    type Output = ();

    type Error = ReqRespError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, _socket: C, _info: Self::Info) -> Self::Future {
        todo!()
    }
}

impl UpgradeInfo for ReqRespInboundProtocol {
    type Info = ProtocolId;

    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        SupportedProtocol::supported_protocols()
    }
}
