use serde::Deserialize;
use ssz_types::{serde_utils::list_of_hex_var_list, typenum, VariableList};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BlobsAndProofV1 {
    #[serde(with = "list_of_hex_var_list")]
    pub blob: VariableList<VariableList<u8, typenum::U1073741824>, typenum::U1048576>,
    #[serde(with = "list_of_hex_var_list")]
    pub proofs: VariableList<VariableList<u8, typenum::U96>, typenum::U1024>,
}
