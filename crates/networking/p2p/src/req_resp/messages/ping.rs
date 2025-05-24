use ssz_derive::{Decode, Encode};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct Ping {
    pub sequence_number: u64,
}

impl Ping {
    pub fn new(sequence_number: u64) -> Self {
        Self { sequence_number }
    }
}

#[cfg(test)]
mod tests {
    use ssz::{Decode, Encode};

    use super::*;

    #[test]
    fn test_ping_encode_decode() {
        let ping = Ping {
            sequence_number: 42,
        };
        let encoded = ping.as_ssz_bytes();
        let decoded = Ping::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(ping, decoded);
        let sequence_number = ping.sequence_number;
        assert_eq!(sequence_number.as_ssz_bytes(), ping.as_ssz_bytes());
    }
}
