use alloy_primitives::Address;
use anyhow::ensure;
use ream_consensus::electra::beacon_state::BeaconState;

use super::builder_bid::SignedBuilderBid;
use crate::builder::verify::verify_bid_signature;

pub fn process_bid(
    state: &BeaconState,
    bid: &SignedBuilderBid,
    fee_recipient: &Address,
) -> anyhow::Result<()> {
    ensure!(
        bid.message.header.parent_hash == state.latest_execution_payload_header.block_hash,
        "parent hash must be equal to state.latest_execution_payload_header.block_hash"
    );
    ensure!(
        bid.message.header.fee_recipient == *fee_recipient,
        "fee recipient must be equal to fee_recipient"
    );
    ensure!(verify_bid_signature(bid)?, "bid signature must be valid");
    Ok(())
}
