use serde::Deserialize;
use ssz_types::{VariableList, typenum};

#[derive(Deserialize, Debug)]
pub struct KZGProof {
    pub bytes: VariableList<u8, typenum::U48>,
}

impl KZGProof {
    pub fn to_fixed_bytes(&self) -> [u8; 48] {
        let mut fixed_array = [0u8; 48];
        fixed_array.copy_from_slice(&self.bytes);
        fixed_array
    }
}
