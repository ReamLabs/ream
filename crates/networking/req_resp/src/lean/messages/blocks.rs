use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U1024};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct BlocksByRootV1Request {
    pub inner: VariableList<B256, U1024>,
}

/// Will panic if over 1024 roots are requested
impl BlocksByRootV1Request {
    pub fn new(roots: Vec<B256>) -> Self {
        Self {
            inner: VariableList::new(roots).expect("Too many roots were requested"),
        }
    }
}
