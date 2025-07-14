use alloy_primitives::B256;
use ream_bls::{BLSSignature, PrivateKey, traits::Signable};
use ream_consensus_beacon::{
    electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_state::BeaconState,
    },
    execution_engine::engine_trait::ExecutionApi,
};
use ream_consensus_misc::{
    constants::DOMAIN_RANDAO,
    misc::{compute_epoch_at_slot, compute_signing_root},
};
use tree_hash::TreeHash;

pub fn get_epoch_signature(
    state: &BeaconState,
    block: &BeaconBlock,
    private_key: PrivateKey,
) -> anyhow::Result<BLSSignature> {
    let domain = state.get_domain(DOMAIN_RANDAO, Some(compute_epoch_at_slot(block.slot)));
    let signing_root = compute_signing_root(compute_epoch_at_slot(block.slot), domain);
    Ok(private_key.sign(signing_root.as_ref())?)
}

pub async fn compute_new_state_root<T: ExecutionApi>(
    state: &BeaconState,
    block: &BeaconBlock,
    execution_engine: &Option<T>,
) -> anyhow::Result<B256> {
    let mut temp_state = state.clone();
    temp_state
        .state_transition(
            &SignedBeaconBlock {
                message: block.clone(),
                signature: BLSSignature::infinity(),
            },
            false,
            execution_engine,
        )
        .await?;
    Ok(temp_state.tree_hash_root())
}
