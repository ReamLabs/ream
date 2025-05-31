use ream_bls::{BLSSignature, PrivateKey, traits::Signable};
use ream_consensus::{
    constants::DOMAIN_BEACON_PROPOSER,
    electra::{beacon_block::BeaconBlock, beacon_state::BeaconState},
    misc::{compute_epoch_at_slot, compute_signing_root},
};
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
