use ream_bls::{BLSSignature, PrivateKey, traits::Signable};
use ream_consensus_beacon::electra::{
    beacon_block::{BeaconBlock, SignedBeaconBlock},
    beacon_state::BeaconState,
    blinded_beacon_block::{BlindedBeaconBlock, SignedBlindedBeaconBlock},
};
use ream_consensus_misc::{
    constants::beacon::{DOMAIN_BEACON_PROPOSER, genesis_validators_root},
    misc::{compute_domain, compute_epoch_at_slot, compute_signing_root},
};
use ream_network_spec::networks::beacon_network_spec;

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

pub fn sign_beacon_block(
    _slot: u64,
    beacon_block: BeaconBlock,
    private_key: &PrivateKey,
) -> anyhow::Result<SignedBeaconBlock> {
    let domain = compute_domain(
        DOMAIN_BEACON_PROPOSER,
        Some(beacon_network_spec().electra_fork_version),
        Some(genesis_validators_root()),
    );
    let signing_root = compute_signing_root(&beacon_block, domain);
    let signature = private_key.sign(signing_root.as_ref())?;

    Ok(SignedBeaconBlock {
        message: beacon_block,
        signature,
    })
}

pub fn sign_blinded_beacon_block(
    _slot: u64,
    blinded_beacon_block: BlindedBeaconBlock,
    private_key: &PrivateKey,
) -> anyhow::Result<SignedBlindedBeaconBlock> {
    let domain = compute_domain(
        DOMAIN_BEACON_PROPOSER,
        Some(beacon_network_spec().electra_fork_version),
        Some(genesis_validators_root()),
    );
    let signing_root = compute_signing_root(&blinded_beacon_block, domain);
    let signature = private_key.sign(signing_root.as_ref())?;

    Ok(SignedBlindedBeaconBlock {
        message: blinded_beacon_block,
        signature,
    })
}
