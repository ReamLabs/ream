use libp2p::gossipsub::DataTransform;

pub struct SnappyTransform {
    _max_size_per_message: usize,
}

impl SnappyTransform {
    pub fn new(max_size_per_message: usize) -> Self {
        SnappyTransform {
            _max_size_per_message: max_size_per_message,
        }
    }
}

impl DataTransform for SnappyTransform {
    fn inbound_transform(
        &self,
        _raw_message: libp2p::gossipsub::RawMessage,
    ) -> Result<libp2p::gossipsub::Message, std::io::Error> {
        todo!()
    }

    fn outbound_transform(
        &self,
        _topic: &libp2p::gossipsub::TopicHash,
        _data: Vec<u8>,
    ) -> Result<Vec<u8>, std::io::Error> {
        todo!()
    }
}
