use kzg::eth::c_bindings::Bytes48;
use serde::Deserialize;
use ssz_types::{VariableList, typenum};

#[derive(Deserialize, Debug)]
pub struct KZGProof {
    pub bytes: VariableList<u8, typenum::U48>,
}

impl From<&KZGProof> for Bytes48 {
    fn from(value: &KZGProof) -> Self {
        let mut fixed_array = [0u8; 48];
        fixed_array.copy_from_slice(&value.bytes);
        Bytes48 { bytes: fixed_array }
    }
}
