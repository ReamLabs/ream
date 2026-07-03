use alloy_primitives::B256;
use ream_consensus_beacon::data_column_sidecar::ColumnIdentifier;
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

    pub fn from_column_identifiers(column_identifiers: Vec<ColumnIdentifier>) -> Self {
        let mut identifiers = Vec::<DataColumnsByRootIdentifier>::new();

        for column_identifier in column_identifiers {
            match identifiers
                .iter_mut()
                .find(|identifier| identifier.block_root == column_identifier.block_root)
            {
                Some(identifier) => identifier
                    .columns
                    .push(column_identifier.index)
                    .expect("Too many data columns were requested for one block"),
                None => identifiers.push(DataColumnsByRootIdentifier {
                    block_root: column_identifier.block_root,
                    columns: VariableList::new(vec![column_identifier.index])
                        .expect("Too many data columns were requested for one block"),
                }),
            }
        }

        Self::new(identifiers)
    }
}
