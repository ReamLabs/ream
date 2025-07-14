use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    VariableList,
    typenum::{U2, U16, U8192},
};
use tree_hash_derive::TreeHash;

use crate::{
    consolidation_request::ConsolidationRequest, deposit_request::DepositRequest,
    withdrawal_request::WithdrawalRequest,
};

#[derive(
    Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Default,
)]
pub struct ExecutionRequests {
    pub deposits: VariableList<DepositRequest, U8192>,
    pub withdrawals: VariableList<WithdrawalRequest, U16>,
    pub consolidations: VariableList<ConsolidationRequest, U2>,
}
