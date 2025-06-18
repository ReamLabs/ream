use alloy_primitives::map::HashMap;
use anyhow::{Ok, anyhow, ensure};
use ream_bls::{PubKey, traits::Verifiable};
use ream_consensus::{
    electra::beacon_state::BeaconState,
    misc::{compute_domain, compute_signing_root},
    validator::Validator,
};

use crate::{
    DOMAIN_APPLICATION_BUILDER, MAX_REGISTRATION_LOOKAHEAD,
    validator_registration::{SignedValidatorRegistrationV1, ValidatorRegistrationV1},
};

/// Check if ``validator`` is pending in ``epoch``.
pub fn is_pending_validator(validator: &Validator, epoch: u64) -> bool {
    validator.activation_epoch > epoch
}

/// Check if ``validator`` is active or pending.
pub fn is_eligible_for_registration(state: &BeaconState, validator: Validator) -> bool {
    let epoch = state.get_current_epoch();

    is_pending_validator(&validator, epoch) || validator.is_active_validator(epoch)
}

pub fn verify_registration_signature(
    _state: BeaconState,
    signed_registration: SignedValidatorRegistrationV1,
) -> bool {
    let domain = compute_domain(DOMAIN_APPLICATION_BUILDER, None, None);

    let signing_root = compute_signing_root(signed_registration.message.clone(), domain);

    signed_registration
        .signature
        .verify(
            &signed_registration.message.public_key,
            signing_root.as_ref(),
        )
        .is_ok()
}

pub fn process_registration(
    state: BeaconState,
    registration: SignedValidatorRegistrationV1,
    registrations: HashMap<PubKey, ValidatorRegistrationV1>,
    current_timestamp: u64,
) -> anyhow::Result<()> {
    let validator_pubkeys = state
        .validators
        .iter()
        .map(|validator| validator.pubkey.clone())
        .collect::<Vec<_>>();

    ensure!(
        validator_pubkeys.contains(&registration.message.public_key),
        "Validator not found"
    );

    let validator = state
        .validators
        .iter()
        .find(|v| v.pubkey == registration.message.public_key)
        .ok_or(anyhow!("Validator not found"))?
        .clone();

    ensure!(
        is_eligible_for_registration(&state, validator),
        "Validator not eligible for registration"
    );
    ensure!(
        registration.message.timestamp <= current_timestamp + MAX_REGISTRATION_LOOKAHEAD,
        "Registration timestamp is too far in the future"
    );

    if let Some(prev_registration) = registrations.get(&registration.message.public_key) {
        ensure!(
            registration.message.timestamp >= prev_registration.timestamp,
            "Registration timestamp must be greater than or equal to previous registration timestamp"
        );
    }

    ensure!(
        verify_registration_signature(state, registration),
        "Registration signature is invalid"
    );

    Ok(())
}
