use ream_bls::{BLSSignature, PrivateKey, traits::Signable};
use ream_consensus::{
    constants::DOMAIN_BEACON_PROPOSER,
    electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_state::BeaconState,
        blinded_beacon_block::{BlindedBeaconBlock, SignedBlindedBeaconBlock},
    },
    misc::{compute_domain, compute_epoch_at_slot, compute_signing_root},
};
use ream_network_spec::networks::network_spec;

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
    slot: u64,
    beacon_block: BeaconBlock,
    private_key: &PrivateKey,
) -> anyhow::Result<SignedBeaconBlock> {
    let epoch = compute_epoch_at_slot(slot);
    let domain = compute_domain(
        DOMAIN_BEACON_PROPOSER,
        Some(network_spec().electra_fork_version),
        None,
    );
    let signing_root = compute_signing_root(epoch, domain);
    let signature = private_key.sign(signing_root.as_ref())?;

    Ok(SignedBeaconBlock {
        message: beacon_block,
        signature,
    })
}

pub fn sign_blinded_beacon_block(
    slot: u64,
    blinded_beacon_block: BlindedBeaconBlock,
    private_key: &PrivateKey,
) -> anyhow::Result<SignedBlindedBeaconBlock> {
    let epoch = compute_epoch_at_slot(slot);

    let domain = compute_domain(
        DOMAIN_BEACON_PROPOSER,
        Some(network_spec().electra_fork_version),
        None,
    );
    let signing_root = compute_signing_root(epoch, domain);
    let signature = private_key.sign(signing_root.as_ref())?;

    Ok(SignedBlindedBeaconBlock {
        message: blinded_beacon_block,
        signature,
    })
}
