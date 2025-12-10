use serde::{Deserialize, Serialize};

/// Payload attributes event.
///
/// The node has computed new payload attributes for execution payload building.
///
/// This event gives block builders and relays sufficient information to construct or verify
/// a block at `proposal_slot`. The meanings of the fields are:
///
/// - `version`: the identifier of the beacon hard fork at `proposal_slot`, e.g. "bellatrix",
///   "capella".
/// - `proposal_slot`: the slot at which a block using these payload attributes may be built.
/// - `parent_block_root`: the beacon block root of the parent block to be built upon.
/// - `parent_block_number`: the execution block number of the parent block.
/// - `parent_block_hash`: the execution block hash of the parent block.
/// - `proposer_index`: the validator index of the proposer at `proposal_slot` on the chain
///   identified by `parent_block_root`.
/// - `payload_attributes`: beacon API encoding of PayloadAttributesV<N> as defined by the
///   execution-apis specification.
///
/// The frequency at which this event is sent may depend on beacon node configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAttributesEvent {
    pub version: String,
    pub data: serde_json::Value, // TODO: Properly type this
}
