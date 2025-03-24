use std::{
    future::Future,
    io::{Read, Write},
    pin::Pin,
};

use asynchronous_codec::BytesMut;
use futures::{
    prelude::{AsyncRead, AsyncWrite},
    FutureExt, SinkExt,
};
use libp2p::{core::UpgradeInfo, OutboundUpgrade};
use snap::{read::FrameDecoder, write::FrameEncoder};
use ssz::{Decode, Encode};
use tokio_util::{
    codec::{Decoder, Encoder, Framed},
    compat::{Compat, FuturesAsyncReadCompatExt},
};
use unsigned_varint::codec::Uvi;

use super::{
    error::ReqRespError,
    messages::{meta_data::GetMetaDataV2, Messages},
    protocol_id::{ProtocolId, SupportedProtocol},
    utils::max_message_size,
};

#[derive(Debug, Clone)]
pub struct OutboundReqRespProtocol {
    request: Messages,
}

pub type OutboundFramed<S> = Framed<Compat<S>, OutboundSSZSnappyCodec>;

impl<S> OutboundUpgrade<S> for OutboundReqRespProtocol
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = OutboundFramed<S>;

    type Error = ReqRespError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: S, protocol: ProtocolId) -> Self::Future {
        let mut socket = Framed::new(socket.compat(), OutboundSSZSnappyCodec { protocol });

        async {
            socket.send(self.request).await?;
            socket.close().await?;
            Ok(socket)
        }
        .boxed()
    }
}

impl UpgradeInfo for OutboundReqRespProtocol {
    type Info = ProtocolId;

    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        SupportedProtocol::supported_protocols()
    }
}

#[derive(Debug)]
pub struct OutboundSSZSnappyCodec {
    protocol: ProtocolId,
}

impl Encoder<Messages> for OutboundSSZSnappyCodec {
    type Error = ReqRespError;

    fn encode(
        &mut self,
        item: Messages,
        dst: &mut libp2p::bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        let bytes = match item {
            Messages::MetaData(_) => return Ok(()),
            // todo: remove unreachable patterns allow, as we add new messages which will need it
            #[allow(unreachable_patterns)]
            message => message.as_ssz_bytes(),
        };

        // The length-prefix is within the expected size bounds derived from the payload SSZ type or
        // MAX_PAYLOAD_SIZE, whichever is smaller.
        if bytes.len() > max_message_size() as usize {
            return Err(ReqRespError::Anyhow(anyhow::anyhow!(
                "Message size exceeds maximum: {} > {}",
                bytes.len(),
                max_message_size()
            )));
        }

        Uvi::<usize>::default().encode(bytes.len(), dst)?;

        let mut encoder = FrameEncoder::new(vec![]);
        encoder.write_all(&bytes).map_err(ReqRespError::from)?;
        encoder.flush().map_err(ReqRespError::from)?;
        dst.extend_from_slice(encoder.get_ref());

        Ok(())
    }
}

impl Decoder for OutboundSSZSnappyCodec {
    type Item = Messages;
    type Error = ReqRespError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() <= 1 {
            return Ok(None);
        }

        let length = match Uvi::<usize>::default().decode(src)? {
            Some(length) => length,
            None => return Ok(None),
        };

        let mut decoder = FrameDecoder::new(src.as_ref());
        let mut buf: Vec<u8> = vec![0; length];
        match decoder.read_exact(&mut buf) {
            Ok(_) => match self.protocol.protocol {
                SupportedProtocol::GetMetaDataV2 => Ok(Some(Messages::MetaData(
                    GetMetaDataV2::from_ssz_bytes(&buf).map_err(ReqRespError::from)?,
                ))),
                _ => unimplemented!("Decoding of protocol: {:?}", self.protocol.protocol),
            },
            Err(err) => Err(ReqRespError::from(err)),
        }
    }
}
