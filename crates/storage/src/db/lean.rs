use std::sync::Arc;

use alloy_primitives::B256;
use anyhow::anyhow;
use ream_consensus_lean::{
    attestation::Attestation,
    block::{Block, BlockBody},
    state::LeanState,
};
use ream_post_quantum_crypto::leansig::signature::Signature;
use redb::Database;
use ssz_types::{VariableList, typenum::U4096};
use tree_hash::TreeHash;

use crate::tables::{
    lean::{
        latest_finalized::LatestFinalizedField, latest_justified::LatestJustifiedField,
        latest_known_attestation::LatestKnownAttestationTable, lean_block::LeanBlockTable,
        lean_head::LeanHeadField, lean_latest_new_attestations::LeanLatestNewAttestationsTable,
        lean_safe_target::LeanSafeTargetField, lean_state::LeanStateTable,
        lean_time::LeanTimeField, slot_index::LeanSlotIndexTable,
        state_root_index::LeanStateRootIndexTable,
    },
    table::REDBTable,
};

#[derive(Clone, Debug)]
pub struct LeanDB {
    pub db: Arc<Database>,
}

impl LeanDB {
    pub fn block_provider(&self) -> LeanBlockTable {
        LeanBlockTable {
            db: self.db.clone(),
        }
    }
    pub fn state_provider(&self) -> LeanStateTable {
        LeanStateTable {
            db: self.db.clone(),
        }
    }

    pub fn slot_index_provider(&self) -> LeanSlotIndexTable {
        LeanSlotIndexTable {
            db: self.db.clone(),
        }
    }

    pub fn state_root_index_provider(&self) -> LeanStateRootIndexTable {
        LeanStateRootIndexTable {
            db: self.db.clone(),
        }
    }

    pub fn latest_known_attestations_provider(&self) -> LatestKnownAttestationTable {
        LatestKnownAttestationTable {
            db: self.db.clone(),
        }
    }

    pub fn latest_finalized_provider(&self) -> LatestFinalizedField {
        LatestFinalizedField {
            db: self.db.clone(),
        }
    }

    pub fn latest_justified_provider(&self) -> LatestJustifiedField {
        LatestJustifiedField {
            db: self.db.clone(),
        }
    }

    pub fn time_provider(&self) -> LeanTimeField {
        LeanTimeField {
            db: self.db.clone(),
        }
    }

    pub fn head_provider(&self) -> LeanHeadField {
        LeanHeadField {
            db: self.db.clone(),
        }
    }

    pub fn safe_target_provider(&self) -> LeanSafeTargetField {
        LeanSafeTargetField {
            db: self.db.clone(),
        }
    }

    pub fn latest_new_attestations_provider(&self) -> LeanLatestNewAttestationsTable {
        LeanLatestNewAttestationsTable {
            db: self.db.clone(),
        }
    }

    pub fn build_block(
        &self,
        slot: u64,
        proposer_index: u64,
        parent_root: B256,
        attestations: Option<VariableList<Attestation, U4096>>,
    ) -> anyhow::Result<(Block, Vec<Signature>, LeanState)> {
        let (state_provider, latest_known_attestation_provider, block_provider) = {
            (
                self.state_provider(),
                self.latest_known_attestations_provider(),
                self.block_provider(),
            )
        };
        let available_signed_attestations =
            latest_known_attestation_provider.get_all_attestations()?;
        let head_state = state_provider
            .get(parent_root)?
            .ok_or(anyhow!("State not found for head root"))?;

        let mut attestations: VariableList<Attestation, U4096> =
            attestations.unwrap_or_else(VariableList::empty);
        let mut signatures: Vec<Signature> = Vec::new();

        let (mut candidate_block, signatures, post_state) = loop {
            let candidate_block = Block {
                slot,
                proposer_index,
                parent_root,
                state_root: B256::ZERO,
                body: BlockBody {
                    attestations: attestations.clone(),
                },
            };
            let mut advanced_state = head_state.clone();
            advanced_state.process_slots(slot)?;
            advanced_state.process_block(&candidate_block)?;

            let mut new_attestations: VariableList<Attestation, U4096> = VariableList::empty();
            let mut new_signatures: Vec<Signature> = Vec::new();
            for signed_attestation in available_signed_attestations.values() {
                let data = &signed_attestation.message.data;
                if !block_provider.contains_key(data.head.root) {
                    continue;
                }
                if data.source != advanced_state.latest_justified {
                    continue;
                }
                if !attestations.contains(&signed_attestation.message) {
                    new_attestations
                        .push(signed_attestation.message.clone())
                        .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;
                    new_signatures.push(signed_attestation.signature);
                }
            }
            if new_attestations.is_empty() {
                break (candidate_block, signatures, advanced_state);
            }

            for attestation in new_attestations {
                attestations
                    .push(attestation)
                    .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;
            }

            for signature in new_signatures {
                signatures.push(signature);
            }
        };

        candidate_block.state_root = post_state.tree_hash_root();
        Ok((candidate_block, signatures, post_state))
    }
}
