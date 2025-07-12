use ream_bls::{BLSSignature, PrivateKey, traits::Signable};
use ream_consensus_misc::{
    constants::DOMAIN_RANDAO,
    misc::{compute_domain, compute_epoch_at_slot, compute_signing_root},
};
use ream_network_spec::networks::network_spec;

pub fn sign_randao_reveal(slot: u64, private_key: &PrivateKey) -> anyhow::Result<BLSSignature> {
    let epoch = compute_epoch_at_slot(slot);

    let domain = compute_domain(
        DOMAIN_RANDAO,
        Some(network_spec().electra_fork_version),
        None,
    );
    let signing_root = compute_signing_root(epoch, domain);
    Ok(private_key.sign(signing_root.as_ref())?)
}
