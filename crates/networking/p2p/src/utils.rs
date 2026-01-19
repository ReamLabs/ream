use discv5::Enr;

use crate::constants::QUIC_ENR_KEY;

/// The QUIC port of ENR record if it is defined.
pub fn quic_from_enr(enr: &Enr) -> Option<u16> {
    enr.get_decodable(QUIC_ENR_KEY).and_then(Result::ok)
}
