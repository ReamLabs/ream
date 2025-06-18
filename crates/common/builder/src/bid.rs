use alloy_primitives::{B256, U256, map::HashMap};
use ream_bls::{BLSSignature, PrivateKey, PubKey, traits::Signable};
use ream_consensus::{
    electra::{beacon_state::BeaconState, execution_payload::ExecutionPayload},
    misc::{compute_domain, compute_signing_root},
};

use crate::{
    BuilderConfig, DOMAIN_APPLICATION_BUILDER, builder_bid::BuilderBid,
    builder_client::BuilderClient, validator_registration::ValidatorRegistrationV1,
};

pub fn is_eligible_for_bid(
    state: BeaconState,
    registrations: HashMap<PubKey, ValidatorRegistrationV1>,
    slot: u64,
    parent_hash: B256,
    public_key: PubKey,
) -> bool {
    if state.slot != slot {
        return false;
    }

    if !registrations.contains_key(&public_key) {
        return false;
    }

    let proposer_index = match state.get_beacon_proposer_index(Some(slot)) {
        Ok(index) => index,
        Err(_) => return false,
    };

    let validator = match state.validators.get(proposer_index as usize) {
        Some(validator) => validator,
        None => return false,
    };

    if public_key != validator.pubkey {
        return false;
    }

    parent_hash == state.latest_execution_payload_header.block_hash
}

pub async fn get_bid(
    config: BuilderConfig,
    execution_payload: ExecutionPayload,
    value: U256,
    public_key: PubKey,
    slot: u64,
) -> anyhow::Result<BuilderBid> {
    let builder_client = BuilderClient::new(config)?;
    let header = execution_payload.to_execution_payload_header();
    let parent_hash = execution_payload.parent_hash;

    let signed_blinded_bid = builder_client
        .get_builder_header(parent_hash, &public_key, slot)
        .await?;

    Ok(BuilderBid {
        header,
        blob_kzg_commitments: signed_blinded_bid.message.blob_kzg_commitments,
        execution_requests: signed_blinded_bid.message.execution_requests,
        value,
        public_key,
    })
}

pub fn get_bid_signature(bid: BuilderBid, private_key: PrivateKey) -> anyhow::Result<BLSSignature> {
    let domain = compute_domain(DOMAIN_APPLICATION_BUILDER, None, None);
    let signing_root = compute_signing_root(bid, domain);
    Ok(private_key.sign(signing_root.as_ref())?)
}
