use alloy_primitives::{B256, U256, map::HashMap};
use ream_bls::{BLSSignature, PrivateKey, PubKey, traits::Signable};
use ream_consensus::{
    constants::DOMAIN_APPLICATION_BUILDER,
    electra::{beacon_state::BeaconState, execution_payload::ExecutionPayload},
    misc::{compute_domain, compute_signing_root},
};

use crate::{builder_bid::BuilderBid, validator_registration::ValidatorRegistrationV1};

pub fn is_eligible_for_bid(
    state: BeaconState,
    registrations: HashMap<PubKey, ValidatorRegistrationV1>,
    slot: u64,
    parent_hash: B256,
    pubkey: PubKey,
) -> bool {
    if state.slot != slot {
        return false;
    }

    if !registrations.contains_key(&pubkey) {
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

    if pubkey != validator.pubkey {
        return false;
    }

    parent_hash == state.latest_execution_payload_header.block_hash
}

pub fn get_bid(execution_payload: ExecutionPayload, value: U256, pubkey: PubKey) -> BuilderBid {
    let header = execution_payload.to_execution_payload_header();

    // TODO: Call `getHeader` Builder API to fetch `SignedBuilderBid` to get the
    // `blob_kzg_commitments` and `execution_requests`

    BuilderBid {
        header,
        blob_kzg_commitments: todo!(),
        execution_requests: todo!(),
        value,
        pubkey,
    }
}

pub fn get_bid_signature(
    _state: BeaconState,
    bid: BuilderBid,
    private_key: PrivateKey,
) -> anyhow::Result<BLSSignature> {
    let domain = compute_domain(DOMAIN_APPLICATION_BUILDER, None, None);
    let signing_root = compute_signing_root(bid, domain);
    Ok(private_key.sign(signing_root.as_ref())?)
}
