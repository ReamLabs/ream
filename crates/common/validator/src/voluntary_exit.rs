use anyhow::anyhow;
use ream_bls::{PrivateKey, traits::Signable};
use ream_consensus_beacon::voluntary_exit::{SignedVoluntaryExit, VoluntaryExit};
use ream_consensus_misc::{
    constants::DOMAIN_VOLUNTARY_EXIT,
    misc::{compute_domain, compute_signing_root},
};
use ream_network_spec::networks::network_spec;

pub fn sign_voluntary_exit(
    epoch: u64,
    validator_index: u64,
    private_key: &PrivateKey,
) -> anyhow::Result<SignedVoluntaryExit> {
    let voluntary_exit = VoluntaryExit {
        epoch,
        validator_index,
    };

    Ok(SignedVoluntaryExit {
        signature: private_key
            .sign(
                compute_signing_root(
                    &voluntary_exit,
                    compute_domain(
                        DOMAIN_VOLUNTARY_EXIT,
                        Some(network_spec().electra_fork_version),
                        None,
                    ),
                )
                .as_ref(),
            )
            .map_err(|err| anyhow!("Failed to sign voluntary exit: {err}"))?,
        message: voluntary_exit,
    })
}
