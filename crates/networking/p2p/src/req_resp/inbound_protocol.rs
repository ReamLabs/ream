use std::{
    future::Future,
    io::{Cursor, Read, Write},
    pin::Pin,
    time::Duration,
};

use futures::{
    FutureExt, StreamExt,
    prelude::{AsyncRead, AsyncWrite},
};
use libp2p::{
    InboundUpgrade,
    bytes::{Buf, BufMut},
    core::UpgradeInfo,
};
use snap::{read::FrameDecoder, write::FrameEncoder};
use ssz::{Decode, Encode};
use ssz_types::{VariableList, typenum::U256};
use tokio::time::timeout;
use tokio_io_timeout::TimeoutStream;
use tokio_util::{
    codec::{Decoder, Encoder, Framed},
    compat::{Compat, FuturesAsyncReadCompatExt},
};
use tracing::debug;
use unsigned_varint::codec::Uvi;

use super::{
    error::ReqRespError,
    handler::RespMessage,
    messages::{Message, goodbye::Goodbye, meta_data::GetMetaDataV2, ping::Ping, status::Status},
    protocol_id::{ProtocolId, SupportedProtocol},
};
use crate::utils::max_message_size;

#[derive(Debug, Clone)]
pub struct InboundReqRespProtocol {}

pub type InboundOutput<S> = (Message, InboundFramed<S>);
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
                    Ok((Message::MetaData(GetMetaDataV2::default().into()), socket))
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

impl Encoder<RespMessage> for InboundSSZSnappyCodec {
    type Error = ReqRespError;

    fn encode(
        &mut self,
        item: RespMessage,
        dst: &mut libp2p::bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        dst.clear();
        dst.put_u8(u8::from(
            item.as_response_code().expect("EndOfStream cannot be sent"),
        ));

        let bytes = match item {
            RespMessage::Response(messages) => messages.as_ssz_bytes(),
            RespMessage::Error(req_resp_error) => {
                VariableList::<u8, U256>::from(req_resp_error.to_string().as_bytes().to_vec())
                    .as_ssz_bytes()
            }
            RespMessage::EndOfStream => unreachable!("EndOfStream cannot be sent"),
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

impl Decoder for InboundSSZSnappyCodec {
    type Item = Message;
    type Error = ReqRespError;

    fn decode(
        &mut self,
        src: &mut libp2p::bytes::BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        if self.protocol.protocol == SupportedProtocol::GetMetaDataV2 {
            return Ok(Some(Message::MetaData(GetMetaDataV2::default().into())));
        }

        let length = match Uvi::<usize>::default().decode(src)? {
            Some(length) => length,
            None => return Ok(None),
        };

        let mut decoder = FrameDecoder::new(Cursor::new(&src));
        let mut buf: Vec<u8> = vec![0; length];
        let result = match decoder.read_exact(&mut buf) {
            Ok(_) => {
                src.advance(decoder.get_ref().position() as usize);
                match self.protocol.protocol {
                    SupportedProtocol::GoodbyeV1 => Ok(Some(Message::Goodbye(
                        Goodbye::from_ssz_bytes(&buf).map_err(ReqRespError::from)?,
                    ))),
                    SupportedProtocol::StatusV1 => Ok(Some(Message::Status(
                        Status::from_ssz_bytes(&buf).map_err(ReqRespError::from)?,
                    ))),
                    SupportedProtocol::PingV1 => Ok(Some(Message::Ping(
                        Ping::from_ssz_bytes(&buf).map_err(ReqRespError::from)?,
                    ))),
                    _ => unimplemented!("Decoding of protocol: {:?}", self.protocol.protocol),
                }
            }
            Err(err) => Err(ReqRespError::from(err)),
        };

        debug!(
            "InboundSSZSnappyCodec::decode: Decoding message: {:?} with length: {}",
            result, length
        );
        result
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
