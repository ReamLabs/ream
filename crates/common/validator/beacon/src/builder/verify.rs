use ream_bls::traits::Verifiable;
use ream_consensus_misc::misc::{compute_domain, compute_signing_root};

use super::{DOMAIN_APPLICATION_BUILDER, builder_bid::SignedBuilderBid};

pub fn verify_bid_signature(signed_bid: &SignedBuilderBid) -> anyhow::Result<bool> {
    let domain = compute_domain(DOMAIN_APPLICATION_BUILDER, None, None);
    let signing_root = compute_signing_root(signed_bid.message.clone(), domain);

    Ok(signed_bid
        .signature
        .verify(&signed_bid.message.public_key, signing_root.as_ref())?)
}
