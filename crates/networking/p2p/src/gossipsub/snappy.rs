use libp2p::gossipsub::{DataTransform, Message, RawMessage, TopicHash};
use snap::raw::{Decoder, Encoder, decompress_len};

pub struct SnappyTransform {
    max_size_per_message: usize,
}

impl SnappyTransform {
    pub fn new(max_size_per_message: usize) -> Self {
        SnappyTransform {
            max_size_per_message,
        }
    }
}

impl DataTransform for SnappyTransform {
    fn inbound_transform(&self, raw_message: RawMessage) -> Result<Message, std::io::Error> {
        let len = decompress_len(&raw_message.data)?;

        if len > self.max_size_per_message {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Message size ({}) exceeds max gossip size per message ({})",
                    len, self.max_size_per_message
                ),
            ));
        }

        let mut decoder = Decoder::new();
        let data = decoder.decompress_vec(&raw_message.data)?;

        Ok(Message {
            source: raw_message.source,
            data,
            sequence_number: raw_message.sequence_number,
            topic: raw_message.topic,
        })
    }

    fn outbound_transform(
        &self,
        _topic: &TopicHash,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, std::io::Error> {
        if data.len() > self.max_size_per_message {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Message size ({}) exceeds max gossip size per message ({})",
                    data.len(),
                    self.max_size_per_message
                ),
            ));
        }

        let mut encoder = Encoder::new();
        let raw_message = encoder.compress_vec(&data)?;

        Ok(raw_message)
    }
}
