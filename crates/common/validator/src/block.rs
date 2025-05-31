use ream_bls::{BLSSignature, PrivateKey, traits::Signable};
use ream_consensus::{
    beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader},
    constants::DOMAIN_BEACON_PROPOSER,
    electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_state::BeaconState,
    },
    misc::{compute_epoch_at_slot, compute_signing_root},
};
use tree_hash::TreeHash;

pub fn get_block_signature(
    state: &BeaconState,
    block: &BeaconBlock,
    private_key: PrivateKey,
) -> anyhow::Result<BLSSignature> {
    let domain = state.get_domain(
        DOMAIN_BEACON_PROPOSER,
        Some(compute_epoch_at_slot(block.slot)),
    );
    let signing_root = compute_signing_root(block, domain);
    Ok(private_key.sign(signing_root.as_ref())?)
}

pub fn compute_signed_block_header(signed_block: &SignedBeaconBlock) -> SignedBeaconBlockHeader {
    let block = &signed_block.message;
    let block_header = BeaconBlockHeader {
        slot: block.slot,
        proposer_index: block.proposer_index,
        parent_root: block.parent_root,
        state_root: block.state_root,
        body_root: block.body.tree_hash_root(),
    };
    SignedBeaconBlockHeader {
        message: block_header,
        signature: signed_block.signature.clone(),
    }
}
