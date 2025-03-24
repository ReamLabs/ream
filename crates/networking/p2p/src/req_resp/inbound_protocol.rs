use std::{
    future::Future,
    io::{Read, Write},
    pin::Pin,
    time::Duration,
};

use futures::{
    prelude::{AsyncRead, AsyncWrite},
    FutureExt, StreamExt,
};
use libp2p::{bytes::BufMut, core::UpgradeInfo, InboundUpgrade};
use snap::{read::FrameDecoder, write::FrameEncoder};
use ssz::{Decode, Encode};
use tokio::time::timeout;
use tokio_io_timeout::TimeoutStream;
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
pub struct InboundReqRespProtocol {}

pub type InboundOutput<S> = (Messages, InboundFramed<S>);
pub type InboundFramed<S> =
    Framed<std::pin::Pin<Box<TimeoutStream<Compat<S>>>>, InboundSSZSnappyCodec>;

impl<S> InboundUpgrade<S> for InboundReqRespProtocol
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = InboundOutput<S>;

    type Error = ReqRespError;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: S, info: ProtocolId) -> Self::Future {
        async move {
            let mut timed_socket = TimeoutStream::new(socket.compat());
            // Set a timeout for the request for some reasonable time
            timed_socket.set_read_timeout(Some(Duration::from_secs(5)));

            let socket = Framed::new(
                Box::pin(timed_socket),
                InboundSSZSnappyCodec {
                    protocol: info.clone(),
                },
            );

            match info.protocol {
                SupportedProtocol::GetMetaDataV2 => {
                    Ok((Messages::MetaData(GetMetaDataV2::default()), socket))
                }
                _ => match timeout(Duration::from_secs(15), socket.into_future()).await {
                    Ok((Some(Ok(message)), stream)) => Ok((message, stream)),
                    Ok((Some(Err(err)), _)) => Err(err),
                    Ok((None, _)) => Err(ReqRespError::IncompleteStream),
                    Err(err) => Err(ReqRespError::from(err)),
                },
            }
        }
        .boxed()
    }
}

impl UpgradeInfo for InboundReqRespProtocol {
    type Info = ProtocolId;

    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        SupportedProtocol::supported_protocols()
    }
}

#[derive(Debug)]
pub struct InboundSSZSnappyCodec {
    protocol: ProtocolId,
}

impl Encoder<Messages> for InboundSSZSnappyCodec {
    type Error = ReqRespError;

    fn encode(
        &mut self,
        item: Messages,
        dst: &mut libp2p::bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        dst.clear();
        dst.put_u8(u8::from(ResponseCode::Success));

        let bytes = item.as_ssz_bytes();

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

impl Decoder for InboundSSZSnappyCodec {
    type Item = Messages;
    type Error = ReqRespError;

    fn decode(
        &mut self,
        src: &mut libp2p::bytes::BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        if self.protocol.protocol == SupportedProtocol::GetMetaDataV2 {
            return Ok(Some(Messages::MetaData(GetMetaDataV2::default())));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCode {
    Success,
    InvalidRequest,
    ServerError,
    ResourceUnavailable,
    ReservedCode(u8),
    ErroneousCode(u8),
}

impl From<u8> for ResponseCode {
    fn from(byte: u8) -> Self {
        match byte {
            0 => ResponseCode::Success,
            1 => ResponseCode::InvalidRequest,
            2 => ResponseCode::ServerError,
            3 => ResponseCode::ResourceUnavailable,
            4..=127 => ResponseCode::ReservedCode(byte),
            _ => ResponseCode::ErroneousCode(byte),
        }
    }
}

impl From<ResponseCode> for u8 {
    fn from(code: ResponseCode) -> u8 {
        match code {
            ResponseCode::Success => 0,
            ResponseCode::InvalidRequest => 1,
            ResponseCode::ServerError => 2,
            ResponseCode::ResourceUnavailable => 3,
            ResponseCode::ReservedCode(byte) => byte,
            ResponseCode::ErroneousCode(byte) => byte,
        }
    }
}
