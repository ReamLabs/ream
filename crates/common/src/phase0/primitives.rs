use alloy_primitives::{U256, U64,aliases::B32};

pub type CommiteeIndex = u64;   //commitee index at a slot
pub type Domain = B32;          //signature Domain
pub type DomainType = B32;      // Domain type
pub type Epoch = u64;           //epoch number
pub type ForkDigest = B32;      //digest of current fork data
pub type Gwei = u64;            //amount in gwei
pub type NodeID = U256;         //Node Idnetifier
pub type Slot = u64;            //slot number
pub type SubnetID = U64;        //Subnet Identifier
pub type ValidatorIndex = u64;  //validator registry index
pub type Version = B32;         //fork version number

