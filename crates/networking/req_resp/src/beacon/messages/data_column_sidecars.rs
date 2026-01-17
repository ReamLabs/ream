use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U128};

pub type NumberOfColumns = U128;
pub type MaxRequestBlocksDeneb = U128;

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DataColumnSidecarsByRangeV1Request {
    pub start_slot: u64,
    pub count: u64,
    pub columns: VariableList<u64, NumberOfColumns>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct DataColumnsByRootIdentifier {
    pub block_root: B256,
    pub columns: VariableList<u64, NumberOfColumns>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct DataColumnSidecarsByRootV1Request {
    pub inner: VariableList<DataColumnsByRootIdentifier, MaxRequestBlocksDeneb>,
}

impl DataColumnSidecarsByRootV1Request {
    pub fn new(identifiers: Vec<DataColumnsByRootIdentifier>) -> Self {
        Self {
            inner: VariableList::new(identifiers)
                .expect("Too many data column identifiers were requested"),
        }
    }
}
