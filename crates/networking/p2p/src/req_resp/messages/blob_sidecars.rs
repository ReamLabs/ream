use ream_consensus::blob_sidecar::BlobIdentifier;
use ssz_derive::{Decode, Encode};
use ssz_types::{
    VariableList,
    typenum::{B0, B1, UInt, UTerm},
};

pub type MaxRequestBlobSidecarsElectra = UInt<
    UInt<
        UInt<
            UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UTerm, B1>, B0>, B0>, B1>, B0>, B0>, B0>, B0>,
            B0,
        >,
        B0,
    >,
    B0,
>;

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BlobSidecarsByRangeV1Request {
    pub start_slot: u64,
    pub count: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct BlobSidecarsByRootV1Request {
    pub inner: VariableList<BlobIdentifier, MaxRequestBlobSidecarsElectra>,
}

impl BlobSidecarsByRootV1Request {
    pub fn new(blob_identifiers: Vec<BlobIdentifier>) -> Self {
        Self {
            inner: VariableList::new(blob_identifiers)
                .expect("Too many blob identifiers were requested"),
        }
    }
}
