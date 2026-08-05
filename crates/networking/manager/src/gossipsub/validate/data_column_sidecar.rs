use anyhow::anyhow;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    data_column_sidecar::DataColumnSidecar, electra::beacon_state::BeaconState,
};
use ream_consensus_misc::{constants::beacon::GENESIS_SLOT, misc::compute_start_slot_at_epoch};
use ream_network_spec::networks::beacon_network_spec;
use ream_polynomial_commitments::handlers::verify_data_column_sidecar_kzg_proofs;
use ream_storage::{
    cache::BeaconCacheDB,
    tables::{field::REDBField, table::REDBTable},
};
use tree_hash::TreeHash;

use super::result::DependencyValidationResult;

type ValidationResult = DependencyValidationResult<GossipValidatedDataColumn>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GossipValidatedDataColumn {
    sidecar: Box<DataColumnSidecar>,
}

impl GossipValidatedDataColumn {
    fn new(sidecar: DataColumnSidecar) -> Self {
        Self {
            sidecar: Box::new(sidecar),
        }
    }

    pub(crate) fn sidecar(&self) -> &DataColumnSidecar {
        &self.sidecar
    }

    pub(crate) fn into_inner(self) -> DataColumnSidecar {
        *self.sidecar
    }
}

struct ParentContext {
    block_slot: u64,
    state: BeaconState,
    pending_availability: bool,
}

pub async fn validate_data_column_sidecar_full(
    data_column_sidecar: &DataColumnSidecar,
    beacon_chain: &BeaconChain,
    current_time_ms: u64,
    subnet_id: u64,
    cached_db: &BeaconCacheDB,
) -> anyhow::Result<ValidationResult> {
    let header = &data_column_sidecar.signed_block_header.message;
    let tuple = (
        header.slot,
        header.proposer_index,
        data_column_sidecar.index,
    );
    if cached_db
        .seen_data_column_sidecars
        .read()
        .await
        .contains(&tuple)
    {
        return Ok(ValidationResult::Ignore(
            "Already seen sidecar from this proposer for this slot and index".to_string(),
        ));
    }

    if !data_column_sidecar.verify() {
        return Ok(ValidationResult::Reject(
            "Data column sidecar failed basic verification".to_string(),
        ));
    }

    if subnet_id != data_column_sidecar.compute_subnet() {
        return Ok(ValidationResult::Reject(
            "Column sidecar not for correct subnet".to_string(),
        ));
    }

    let store = beacon_chain.store.lock().await;
    let head_root = store.get_head()?;
    let head_state: BeaconState = store
        .db
        .state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    if !is_not_from_future_slot(&head_state, header.slot, current_time_ms) {
        return Ok(ValidationResult::Ignore(
            "The sidecar is from a future slot".to_string(),
        ));
    }

    let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
    if header.slot <= compute_start_slot_at_epoch(finalized_checkpoint.epoch) {
        return Ok(ValidationResult::Ignore(
            "The sidecar is from a slot less than or equal to the latest finalized slot"
                .to_string(),
        ));
    }

    let parent = if let Some(parent_block) = store.db.block_provider().get(header.parent_root)? {
        let Some(parent_state) = store.db.state_provider().get(header.parent_root)? else {
            return Ok(ValidationResult::Reject(
                "Sidecar's parent failed validation".to_string(),
            ));
        };
        Some(ParentContext {
            block_slot: parent_block.message.slot,
            state: parent_state,
            pending_availability: false,
        })
    } else if let Some(pending) = store
        .data_availability_checker
        .pending_block(&header.parent_root)
    {
        if pending.signed_block.message.tree_hash_root() != header.parent_root {
            return Err(anyhow!(
                "pending availability block root does not match lookup key"
            ));
        }
        Some(ParentContext {
            block_slot: pending.signed_block.message.slot,
            state: pending.post_state.clone(),
            pending_availability: true,
        })
    } else {
        None
    };

    // Looking up the parent above does not classify the message. Validation still checks the
    // signature before returning unknown-parent, as required by the gossip specification.
    let signature_state = parent.as_ref().map_or(&head_state, |parent| &parent.state);
    if usize::try_from(header.proposer_index)
        .ok()
        .and_then(|index| signature_state.validators.get(index))
        .is_none()
    {
        return Ok(ValidationResult::Reject(
            "Sidecar proposer index out of range".to_string(),
        ));
    }
    if !matches!(
        signature_state.verify_block_header_signature(&data_column_sidecar.signed_block_header),
        Ok(true)
    ) {
        return Ok(ValidationResult::Reject(
            "Invalid proposer signature on data column sidecar's block header".to_string(),
        ));
    }

    let Some(ParentContext {
        block_slot,
        mut state,
        pending_availability,
    }) = parent
    else {
        return Ok(ValidationResult::Ignore(
            "Parent block not seen".to_string(),
        ));
    };

    if header.slot <= block_slot {
        return Ok(ValidationResult::Reject(
            "Sidecar slot not higher than parent block's slot".to_string(),
        ));
    }

    #[cfg(not(feature = "disable_ancestor_validation"))]
    if !pending_availability
        && store.get_checkpoint_block(header.parent_root, finalized_checkpoint.epoch)?
            != finalized_checkpoint.root
    {
        return Ok(ValidationResult::Reject(
            "Finalized checkpoint is not an ancestor of the sidecar's block".to_string(),
        ));
    }

    // A pending parent is absent from the block DB, so ancestry cannot be walked at arrival.
    // Release repeats the normal walk after that parent imports and finality may have advanced.

    // KZG verification and state advancement do not require exclusive access to the store.
    drop(store);

    if !data_column_sidecar.verify_inclusion_proof() {
        return Ok(ValidationResult::Reject(
            "Invalid data column sidecar inclusion proof".to_string(),
        ));
    }

    if !matches!(
        verify_data_column_sidecar_kzg_proofs(data_column_sidecar),
        Ok(true)
    ) {
        return Ok(ValidationResult::Reject(
            "Invalid KZG proofs for data column sidecar".to_string(),
        ));
    }

    if let Err(err) = state.process_slots(header.slot) {
        return Ok(ValidationResult::Ignore(format!(
            "Could not advance parent state to sidecar slot: {err:?}"
        )));
    }

    match state.get_beacon_proposer_index(None) {
        Ok(expected_index) if expected_index == header.proposer_index => {}
        Ok(expected_index) => {
            return Ok(ValidationResult::Reject(format!(
                "Wrong proposer index: slot {}: expected {expected_index}, got {}",
                header.slot, header.proposer_index
            )));
        }
        Err(err) => {
            return Ok(ValidationResult::Ignore(format!(
                "Could not get proposer index: {err:?}"
            )));
        }
    }

    // Re-check under the write lock in case another validation inserted the tuple.
    let mut seen = cached_db.seen_data_column_sidecars.write().await;
    if seen.contains(&tuple) {
        return Ok(ValidationResult::Ignore(
            "Duplicate data column sidecar for (slot, proposer_index, index)".to_string(),
        ));
    }
    seen.put(tuple, ());

    if pending_availability {
        Ok(ValidationResult::ParentPendingAvailability {
            parent_root: header.parent_root,
            validated: GossipValidatedDataColumn::new(data_column_sidecar.clone()),
        })
    } else {
        Ok(ValidationResult::Accept)
    }
}

fn is_not_from_future_slot(state: &BeaconState, slot: u64, current_time_ms: u64) -> bool {
    let network_spec = beacon_network_spec();
    let slots_since_genesis = slot.saturating_sub(GENESIS_SLOT);
    let slot_time_ms = state
        .genesis_time
        .saturating_mul(1000)
        .saturating_add(slots_since_genesis.saturating_mul(network_spec.slot_duration_ms));

    current_time_ms.saturating_add(network_spec.maximum_gossip_clock_disparity) >= slot_time_ms
}
