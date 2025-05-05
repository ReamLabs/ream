use ssz::{Decode, Encode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Goodbye {
    ClientShutdown,
    IrrelevantNetwork,
    FaultOrError,
    UnableToVerifyNetwork,
    TooManyPeers,
    BadScore,
    Banned,
    BannedIP,
    UnspecifiedGoodbye(u64),
}

impl From<Goodbye> for u64 {
    fn from(reason: Goodbye) -> u64 {
        match reason {
            Goodbye::ClientShutdown => 1,
            Goodbye::IrrelevantNetwork => 2,
            Goodbye::FaultOrError => 3,
            Goodbye::UnableToVerifyNetwork => 128,
            Goodbye::TooManyPeers => 129,
            Goodbye::BadScore => 250,
            Goodbye::Banned => 251,
            Goodbye::BannedIP => 252,
            Goodbye::UnspecifiedGoodbye(reason) => reason,
        }
    }
}

impl From<u64> for Goodbye {
    fn from(reason: u64) -> Goodbye {
        match reason {
            1 => Goodbye::ClientShutdown,
            2 => Goodbye::IrrelevantNetwork,
            3 => Goodbye::FaultOrError,
            128 => Goodbye::UnableToVerifyNetwork,
            129 => Goodbye::TooManyPeers,
            250 => Goodbye::BadScore,
            251 => Goodbye::Banned,
            252 => Goodbye::BannedIP,
            reason => Goodbye::UnspecifiedGoodbye(reason),
        }
    }
}

impl Encode for Goodbye {
    fn is_ssz_fixed_len() -> bool {
        true
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        u64::from(*self).ssz_append(buf);
    }

    fn ssz_bytes_len(&self) -> usize {
        8
    }

    fn ssz_fixed_len() -> usize {
        8
    }
}

impl Decode for Goodbye {
    fn is_ssz_fixed_len() -> bool {
        true
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, ssz::DecodeError> {
        let value = u64::from_ssz_bytes(bytes)?;
        Ok(Goodbye::from(value))
    }

    fn ssz_fixed_len() -> usize {
        8
    }
}
