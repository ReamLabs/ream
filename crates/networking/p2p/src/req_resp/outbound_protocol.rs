use std::{
    future::Future,
    io::{Cursor, Read, Write},
    pin::Pin,
};

use asynchronous_codec::BytesMut;
use futures::{
    FutureExt, SinkExt,
    prelude::{AsyncRead, AsyncWrite},
};
use libp2p::{OutboundUpgrade, bytes::Buf, core::UpgradeInfo};
use snap::{read::FrameDecoder, write::FrameEncoder};
use ssz::{Decode, Encode};
use ssz_types::{VariableList, typenum::U256};
use tokio_util::{
    codec::{Decoder, Encoder, Framed},
    compat::{Compat, FuturesAsyncReadCompatExt},
};
use tracing::debug;
use unsigned_varint::codec::Uvi;

use super::{
    error::ReqRespError,
    handler::RespMessage,
    inbound_protocol::ResponseCode,
    messages::{Message, meta_data::GetMetaDataV2, ping::Ping, status::Status},
    protocol_id::{ProtocolId, SupportedProtocol},
};
use crate::utils::max_message_size;

#[derive(Debug, Clone)]
pub struct OutboundReqRespProtocol {
    pub request: Message,
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
        let mut socket = Framed::new(
            socket.compat(),
            OutboundSSZSnappyCodec {
                protocol,
                current_response_code: None,
            },
        );

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
    current_response_code: Option<ResponseCode>,
}

impl Encoder<Message> for OutboundSSZSnappyCodec {
    type Error = ReqRespError;

    fn encode(
        &mut self,
        item: Message,
        dst: &mut libp2p::bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        let bytes = match item {
            Message::MetaData(_) => return Ok(()),
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
    type Item = RespMessage;
    type Error = ReqRespError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() <= 1 {
            return Ok(None);
        }

        let response_code = *self
            .current_response_code
            .get_or_insert_with(|| ResponseCode::from(src.split_to(1)[0]));

        let length = match Uvi::<usize>::default().decode(src)? {
            Some(length) => length,
            None => return Ok(None),
        };

        // The length-prefix is within the expected size bounds derived from the payload SSZ
        // type or MAX_PAYLOAD_SIZE, whichever is smaller.
        if length > max_message_size() as usize {
            return Err(ReqRespError::Anyhow(anyhow::anyhow!(
                "Message size exceeds maximum: {} > {}",
                length,
                max_message_size()
            )));
        }

        let mut decoder = FrameDecoder::new(Cursor::new(&src));
        let mut buf: Vec<u8> = vec![0; length];
        let result = match decoder.read_exact(&mut buf) {
            Ok(_) => {
                src.advance(decoder.get_ref().position() as usize);
                if ResponseCode::Success == response_code {
                    match self.protocol.protocol {
                        SupportedProtocol::GoodbyeV1 => Ok(Some(RespMessage::Error(
                            ReqRespError::InvalidData("Goodbye has no response".to_string()),
                        ))),
                        SupportedProtocol::GetMetaDataV2 => {
                            Ok(Some(RespMessage::Response(Message::MetaData(
                                GetMetaDataV2::from_ssz_bytes(&buf)
                                    .map_err(ReqRespError::from)?
                                    .into(),
                            ))))
                        }
                        SupportedProtocol::StatusV1 => {
                            Ok(Some(RespMessage::Response(Message::Status(
                                Status::from_ssz_bytes(&buf).map_err(ReqRespError::from)?,
                            ))))
                        }
                        SupportedProtocol::PingV1 => Ok(Some(RespMessage::Response(
                            Message::Ping(Ping::from_ssz_bytes(&buf).map_err(ReqRespError::from)?),
                        ))),
                        _ => Err(ReqRespError::InvalidData(format!(
                            "Unsupported protocol: {:?}",
                            self.protocol.protocol
                        ))),
                    }
                } else {
                    Ok(Some(RespMessage::Error(
                        VariableList::<u8, U256>::from_ssz_bytes(&buf)
                            .map(ReqRespError::from)
                            .unwrap_or_else(|err| {
                                ReqRespError::InvalidData(format!(
                                    "Failed to decode variable list: {err:?}"
                                ))
                            }),
                    )))
                }
            }
            Err(_) => Err(ReqRespError::InvalidData(
                "Failed to snappy message".to_string(),
            )),
        };
        debug!(
            "OutboundSSZSnappyCodec::decode: protocol: {:?}, response_code: {:?}, result: {:?}",
            self.protocol.protocol, response_code, result
        );

        if let Ok(Some(_)) = result {
            self.current_response_code = None;
        }

        result
    }
}
