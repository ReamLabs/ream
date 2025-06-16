use ream_bls::traits::Verifiable;
use ream_consensus::{
    constants::DOMAIN_BEACON_PROPOSER, electra::beacon_state::BeaconState,
    misc::compute_signing_root,
};

use crate::blinded_beacon_block::SignedBlindedBeaconBlock;

pub fn verify_blinded_block_signature(
    state: BeaconState,
    signed_block: SignedBlindedBeaconBlock,
) -> anyhow::Result<bool> {
    let proposer_index = state.get_beacon_proposer_index(Some(state.slot))?;

    let proposer = state
        .validators
        .get(proposer_index as usize)
        .ok_or(anyhow::anyhow!("Invalid proposer index"))?;

    let signing_root = compute_signing_root(
        signed_block.message,
        state.get_domain(DOMAIN_BEACON_PROPOSER, Some(state.get_current_epoch())),
    );

    Ok(signed_block
        .signature
        .verify(&proposer.pubkey, signing_root.as_ref())?)
}
