use std::{
    collections::{HashMap, hash_map::Entry},
    mem::take,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
    vec,
};

use alloy_primitives::Address;
use alloy_rpc_types_beacon::events::HeadEvent;
use anyhow::{anyhow, bail};
use futures::{StreamExt, future::try_join_all};
use ream_beacon_api_types::{
    block::{BroadcastValidation, ProduceBlockData},
    duties::{AttesterDuty, ProposerDuty, SyncCommitteeDuty},
    id::{ID, ValidatorID},
    request::SyncCommitteeRequestItem,
};
use ream_bls::{BLSSignature, PublicKey, traits::Signable};
use ream_consensus::{
    attestation_data::AttestationData,
    constants::{DOMAIN_SYNC_COMMITTEE, INTERVALS_PER_SLOT, SLOTS_PER_EPOCH, SYNC_COMMITTEE_SIZE},
    electra::beacon_state::BeaconState,
    misc::{compute_domain, compute_epoch_at_slot, compute_signing_root},
    single_attestation::SingleAttestation,
};
use ream_executor::ReamExecutor;
use ream_keystore::keystore::Keystore;
use ream_network_spec::networks::network_spec;
use reqwest::Url;
use tokio::{
    sync::broadcast,
    time::{Instant, MissedTickBehavior, interval_at},
};
use tracing::{error, info, warn};
use tree_hash::TreeHash;

use crate::{
    aggregate_and_proof::{AggregateAndProof, SignedAggregateAndProof, sign_aggregate_and_proof},
    attestation::{get_selection_proof, sign_attestation_data},
    beacon_api_client::{
        BeaconApiClient,
        event::{BeaconEvent, EventTopic},
    },
    block::{sign_beacon_block, sign_blinded_beacon_block},
    constants::SYNC_COMMITTEE_SUBNET_COUNT,
    contribution_and_proof::{
        ContributionAndProof, SignedContributionAndProof, get_contribution_and_proof_signature,
    },
    randao::sign_randao_reveal,
    sync_committee::{get_sync_committee_selection_proof, is_sync_committee_aggregator},
    voluntary_exit::sign_voluntary_exit,
};

pub fn check_if_validator_active(
    state: &BeaconState,
    validator_index: u64,
) -> anyhow::Result<bool> {
    state
        .validators
        .get(validator_index as usize)
        .map(|validator| validator.is_active_validator(state.get_current_epoch()))
        .ok_or_else(|| anyhow!("Validator index out of bounds"))
}

pub fn is_proposer(state: &BeaconState, validator_index: u64) -> anyhow::Result<bool> {
    Ok(state.get_beacon_proposer_index(None)? == validator_index)
}
pub struct SyncTaskInfo {
    pub validator_index: u64,
    pub committee_index: u64,
    pub selection_proof: BLSSignature,
    pub keystore: Arc<Keystore>,
}

pub struct ValidatorService {
    pub beacon_api_client: Arc<BeaconApiClient>,
    pub validators: Vec<Arc<Keystore>>,
    pub suggested_fee_recipient: Arc<Address>,
    pub executor: ReamExecutor,
    pub active_validator_count: usize,
    pub public_key_to_index: HashMap<PublicKey, u64>,
    pub validator_index_to_keystore: HashMap<u64, Arc<Keystore>>,
    pub proposer_duties: Vec<ProposerDuty>,
    pub attester_duties: Vec<AttesterDuty>,
    pub sync_committee_duties: Vec<SyncCommitteeDuty>,
    pub sync_aggregator_infos: Vec<SyncTaskInfo>,
    pub sync_normal_infos: Vec<SyncTaskInfo>,
    pub highest_slot: u64,
    pub slot_sender: broadcast::Sender<u64>,
}

impl ValidatorService {
    pub fn new(
        keystores: Vec<Keystore>,
        suggested_fee_recipient: Address,
        beacon_api_endpoint: Url,
        request_timeout: Duration,
        executor: ReamExecutor,
    ) -> anyhow::Result<Self> {
        let validators = keystores.into_iter().map(Arc::new).collect::<Vec<_>>();
        let (slot_sender, _) = broadcast::channel(100);

        Ok(Self {
            beacon_api_client: Arc::new(BeaconApiClient::new(
                beacon_api_endpoint,
                request_timeout,
            )?),
            validators,
            suggested_fee_recipient: Arc::new(suggested_fee_recipient),
            executor,
            active_validator_count: 0,
            public_key_to_index: HashMap::new(),
            validator_index_to_keystore: HashMap::new(),
            proposer_duties: Vec::new(),
            attester_duties: Vec::new(),
            sync_committee_duties: Vec::new(),
            sync_aggregator_infos: Vec::new(),
            sync_normal_infos: Vec::new(),
            highest_slot: 0,
            slot_sender,
        })
    }

    pub async fn start(mut self) {
        let seconds_per_slot = network_spec().seconds_per_slot;
        let seconds_per_interval = seconds_per_slot / INTERVALS_PER_SLOT;

        let genesis_instant = UNIX_EPOCH + Duration::from_secs(network_spec().min_genesis_time);
        let elapsed = SystemTime::now()
            .duration_since(genesis_instant)
            .expect("System Time is before the genesis time");

        let mut intervals = elapsed.as_secs() / seconds_per_interval;
        let mut slot = intervals / INTERVALS_PER_SLOT;
        let mut epoch = compute_epoch_at_slot(slot);

        let mut interval = {
            let interval_start =
                Instant::now() - (elapsed - Duration::from_secs(intervals * seconds_per_interval));
            interval_at(interval_start, Duration::from_secs(seconds_per_interval))
        };
        interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

        let mut event_stream = match self
            .beacon_api_client
            .get_events_stream(&[EventTopic::Head], "validator")
        {
            Ok(stream) => stream,
            Err(err) => {
                error!("Failed to create event stream: {err:?}");
                return;
            }
        };

        loop {
            tokio::select! {
                Some(event) = event_stream.next() => {
                    if let BeaconEvent::Head(head_event) = event {
                        if let Err(err) = self.process_head_event(&head_event) {
                            warn!("Failed to process head event: {err:?}");
                        }
                    }
                }
                _ = interval.tick() => {
                    intervals += 1;
                    if intervals % (INTERVALS_PER_SLOT * SLOTS_PER_EPOCH) == 0 {
                        epoch += 1;
                        self.on_epoch(epoch).await;
                    }
                    if intervals % INTERVALS_PER_SLOT == 0 {
                        slot += 1;
                        self.on_slot(slot).await;
                    }
                    if intervals % INTERVALS_PER_SLOT == 2 {
                        self.on_slot_aggregator(slot).await;
                    }
                    if (intervals+1) % (INTERVALS_PER_SLOT * SLOTS_PER_EPOCH) == 0 {
                        self.on_epoch_end(epoch).await;
                    }
                }
            }
        }
    }

    pub fn process_head_event(&mut self, head_event: &HeadEvent) -> anyhow::Result<()> {
        if head_event.slot > self.highest_slot {
            self.highest_slot = head_event.slot;
            info!("Updated highest slot to: {}", self.highest_slot);
            let _ = self.slot_sender.send(head_event.slot);
        }

        Ok(())
    }

    pub async fn process_aggregator_sync_infos(&mut self, slot: u64) -> anyhow::Result<()> {
        let client = self.beacon_api_client.clone();
        let aggregator_infos = take(&mut self.sync_aggregator_infos);

        let block_root = client.get_block_root(ID::Slot(slot)).await?.data.root;

        let contribution_tasks = aggregator_infos
            .into_iter()
            .map(|aggregator_info| {
                let client = client.clone();
                tokio::spawn(async move {
                    let subcommittee_index = aggregator_info.committee_index
                        / (SYNC_COMMITTEE_SIZE / SYNC_COMMITTEE_SUBNET_COUNT);

                    let contribution = client
                        .get_sync_committee_contribution(slot, subcommittee_index, block_root)
                        .await?
                        .data;

                    let contribution_and_proof = ContributionAndProof {
                        aggregator_index: aggregator_info.validator_index,
                        contribution,
                        selection_proof: aggregator_info.selection_proof,
                    };

                    let contribution_and_proof_signature = get_contribution_and_proof_signature(
                        &contribution_and_proof,
                        &aggregator_info.keystore.private_key,
                    )?;

                    Ok::<_, anyhow::Error>(SignedContributionAndProof {
                        message: contribution_and_proof,
                        signature: contribution_and_proof_signature,
                    })
                })
            })
            .collect::<Vec<_>>();

        let signed_proofs = try_join_all(contribution_tasks)
            .await?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        client
            .publish_contribution_and_proofs(signed_proofs)
            .await?;

        Ok(())
    }

    pub async fn process_normal_sync_infos(&mut self, slot: u64) -> anyhow::Result<()> {
        let normal_infos = take(&mut self.sync_normal_infos)
            .into_iter()
            .map(|sync_info| sync_info.validator_index)
            .collect::<Vec<u64>>();
        self.submit_sync_committee(slot, normal_infos.as_ref())
            .await
    }

    // Runs on the start of every epoch prior to the per-slot code.
    // - Fetches validator indicies
    // - Fetches proposer and committee duties for the epoch
    pub async fn on_epoch(&mut self, epoch: u64) {
        info!("Current Epoch: {epoch}");

        self.fetch_validator_indicies().await;
        let validator_indices: Vec<u64> = self.public_key_to_index.values().cloned().collect();

        if validator_indices.is_empty() {
            warn!("No active validators found, skipping duty fetch");
            return;
        }

        if let Some(proposer_duties) = self.fetch_proposer_duties(epoch, &validator_indices).await {
            self.proposer_duties = proposer_duties;
        }
    }

    // Runs on the end of every epoch after the per-slot code(exactly 4 seconds prior to the next
    // epoch).
    // - Fetches the attester duties for the next epoch
    pub async fn on_epoch_end(&mut self, epoch: u64) {
        info!("Current Epoch: {epoch}");
        let validator_indices: Vec<u64> = self.public_key_to_index.values().cloned().collect();

        if validator_indices.is_empty() {
            warn!("No active validators found, skipping duty fetch");
            return;
        }

        // In the future, we can likely increase the lookahead for sync duties
        let (attester_duties, sync_duties) = tokio::join!(
            self.fetch_attester_duties(epoch + 1, &validator_indices),
            self.fetch_sync_committee_duties(epoch + 1, &validator_indices),
        );

        if let Some(attester_duties) = attester_duties {
            self.attester_duties = attester_duties;
        }

        if let Some(sync_duties) = sync_duties {
            self.sync_committee_duties = sync_duties;
        }

        // Fetch proposer duties separately (could also be joined if needed)
        if let Some(proposer_duties) = self
            .fetch_proposer_duties(epoch + 1, &validator_indices)
            .await
        {
            self.proposer_duties = proposer_duties;
        }
    }

    // Runs at the start of every slot
    pub async fn on_slot(&mut self, slot: u64) {
        info!("Current Slot: {slot}");
        if let Err(sync_error) = self.prepare_sync_infos(slot - 1).await {
            warn!("Could not prepare the sync infos: {sync_error:?}");
        } else if let Err(sync_error) = self.process_normal_sync_infos(slot - 1).await {
            warn!("Could not process the normal sync infos: {sync_error:?}");
        }
    }

    // Runs at 2 intervals into every slot: meant for aggregators
    pub async fn on_slot_aggregator(&mut self, slot: u64) {
        info!("Current Slot: {slot}");
        if let Err(sync_error) = self.process_aggregator_sync_infos(slot - 1).await {
            warn!("Could not process the aggregator sync infos: {sync_error:?}");
        }
    }

    pub async fn fetch_validator_indicies(&mut self) {
        if self.active_validator_count < self.validators.len() {
            let validator_states = self
                .beacon_api_client
                .get_state_validator_list(
                    ID::Head,
                    Some(
                        self.validators
                            .iter()
                            .map(|validator_info| {
                                ValidatorID::Address(validator_info.public_key.clone())
                            })
                            .collect::<Vec<_>>(),
                    ),
                    None,
                )
                .await;

            if let Ok(validator_infos) = validator_states {
                validator_infos.data.into_iter().for_each(|validator_data| {
                    if let Entry::Vacant(entry) = self
                        .public_key_to_index
                        .entry(validator_data.validator.public_key.clone())
                    {
                        entry.insert(validator_data.index);

                        if let Some(keystore) = self
                            .validators
                            .iter()
                            .find(|keystore| {
                                keystore.public_key == validator_data.validator.public_key
                            })
                            .cloned()
                        {
                            self.validator_index_to_keystore
                                .insert(validator_data.index, keystore);
                        }

                        self.active_validator_count += 1;
                    }
                });
            }
        }
    }

    pub async fn fetch_proposer_duties(
        &self,
        epoch: u64,
        validator_indices: &[u64],
    ) -> Option<Vec<ProposerDuty>> {
        match self.beacon_api_client.get_proposer_duties(epoch).await {
            Ok(duties_response) => Some(
                duties_response
                    .data
                    .into_iter()
                    .filter(|duty| validator_indices.contains(&duty.validator_index))
                    .collect(),
            ),
            Err(err) => {
                error!("Failed to fetch proposer duties for epoch {epoch}: {err:?}");
                None
            }
        }
    }

    pub async fn fetch_attester_duties(
        &self,
        epoch: u64,
        validator_indices: &[u64],
    ) -> Option<Vec<AttesterDuty>> {
        match self
            .beacon_api_client
            .get_attester_duties(epoch, validator_indices)
            .await
        {
            Ok(duties_response) => Some(duties_response.data),
            Err(err) => {
                error!("Failed to fetch attester duties for epoch {epoch}: {err:?}");
                None
            }
        }
    }

    pub async fn fetch_sync_committee_duties(
        &self,
        epoch: u64,
        validator_indices: &[u64],
    ) -> Option<Vec<SyncCommitteeDuty>> {
        match self
            .beacon_api_client
            .get_sync_committee_duties(epoch, validator_indices)
            .await
        {
            Ok(duties_response) => Some(duties_response.data),
            Err(err) => {
                error!("Failed to fetch sync committee duties for epoch {epoch}: {err:?}");
                None
            }
        }
    }

    pub async fn propose_block(&self, slot: u64, validator_index: u64) -> anyhow::Result<()> {
        let keystore = self
            .validator_index_to_keystore
            .get(&validator_index)
            .cloned()
            .ok_or_else(|| anyhow!("keystore not found for validator: {validator_index}"))?;
        let randao_reveal = sign_randao_reveal(slot, &keystore.private_key)?;
        let block_response = self
            .beacon_api_client
            .produce_block(slot, randao_reveal, None, None, None)
            .await?;

        match block_response.data {
            ProduceBlockData::Full(full_block) => {
                let signed_beacon_block =
                    sign_beacon_block(slot, full_block.block, &keystore.private_key)?;

                self.beacon_api_client
                    .publish_block(BroadcastValidation::Gossip, signed_beacon_block)
                    .await?;
            }
            ProduceBlockData::Blinded(blinded_block) => {
                let signed_blinded_block =
                    sign_blinded_beacon_block(slot, blinded_block, &keystore.private_key)?;

                self.beacon_api_client
                    .publish_blinded_block(BroadcastValidation::Gossip, signed_blinded_block)
                    .await?;
            }
        };

        Ok(())
    }

    pub async fn prepare_sync_infos(&mut self, slot: u64) -> anyhow::Result<()> {
        self.sync_normal_infos.clear();
        self.sync_aggregator_infos.clear();

        for duty in &self.sync_committee_duties {
            let Some(keystore) = self.validator_index_to_keystore.get(&duty.validator_index) else {
                continue;
            };
            for &committee_index in &duty.validator_sync_committee_indices {
                let selection_proof = get_sync_committee_selection_proof(
                    slot,
                    committee_index,
                    &keystore.private_key,
                )
                .map_err(|err| anyhow!("Could not get selection proof: {err:?}"))?;

                let task_info = SyncTaskInfo {
                    validator_index: duty.validator_index,
                    committee_index,
                    selection_proof,
                    keystore: Arc::clone(keystore),
                };

                if is_sync_committee_aggregator(&task_info.selection_proof) {
                    self.sync_aggregator_infos.push(task_info);
                } else {
                    self.sync_normal_infos.push(task_info);
                }
            }
        }

        Ok(())
    }

    pub async fn submit_sync_committee(
        &self,
        slot: u64,
        validator_indices: &[u64],
    ) -> anyhow::Result<()> {
        let domain = compute_domain(
            DOMAIN_SYNC_COMMITTEE,
            Some(network_spec().electra_fork_version),
            None,
        );
        let beacon_block_root = self
            .beacon_api_client
            .get_block_root(ID::Slot(slot))
            .await?
            .data
            .root;
        let signing_root = compute_signing_root(beacon_block_root, domain);

        let payload = validator_indices
            .iter()
            .filter_map(|&validator_index| {
                if let Some(keystore) = self.validator_index_to_keystore.get(&validator_index) {
                    return match keystore.private_key.sign(signing_root.as_ref()) {
                        Ok(signature) => Some(Ok(SyncCommitteeRequestItem {
                            slot,
                            beacon_block_root,
                            validator_index,
                            signature,
                        })),
                        Err(signing_error) => Some(Err(anyhow!(
                            "Signing failed for validator {validator_index:?}: {signing_error:?}"
                        ))),
                    };
                }
                None
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(self
            .beacon_api_client
            .publish_sync_committee_signature(payload)
            .await?)
    }

    pub async fn attestation_wait(&self, slot: u64) {
        if slot <= self.highest_slot {
            return;
        }

        let mut slot_receiver = self.slot_sender.subscribe();

        tokio::select! {
            Ok(received_slot) = slot_receiver.recv() => {
                if received_slot >= slot {
                    info!("Received slot {received_slot}, proceeding with attestation");
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(network_spec().seconds_per_slot / 3)) => {
                info!("One third of slot {slot} has passed, proceeding with attestation");
            }
        }
    }

    pub async fn make_attestation(
        &self,
        slot: u64,
        validator_index: u64,
        committee_index: u64,
    ) -> anyhow::Result<()> {
        self.attestation_wait(slot).await;

        let Some(keystore) = self.validator_index_to_keystore.get(&validator_index) else {
            bail!("Keystore not found for validator: {validator_index}");
        };

        let attestation_data = self
            .beacon_api_client
            .get_attestation_data(slot, committee_index)
            .await?
            .data;
        Ok(self
            .beacon_api_client
            .submit_attestation(vec![SingleAttestation {
                attester_index: validator_index,
                committee_index,
                signature: sign_attestation_data(&attestation_data, &keystore.private_key)?,
                data: attestation_data,
            }])
            .await?)
    }

    pub async fn submit_aggregate_and_proof(
        &self,
        attestation_data: AttestationData,
        slot: u64,
        committee_index: u64,
        aggregator_index: u64,
    ) -> anyhow::Result<()> {
        let keystore = self
            .validator_index_to_keystore
            .get(&aggregator_index)
            .cloned()
            .ok_or_else(|| anyhow!("Keystore not found for validator: {aggregator_index}"))?;

        let aggregate_and_proof = AggregateAndProof {
            aggregator_index,
            aggregate: self
                .beacon_api_client
                .get_aggregated_attestation(
                    attestation_data.tree_hash_root(),
                    slot,
                    committee_index,
                )
                .await?
                .data,
            selection_proof: get_selection_proof(slot, &keystore.private_key)?,
        };

        Ok(self
            .beacon_api_client
            .publish_aggregate_and_proofs(vec![SignedAggregateAndProof {
                signature: sign_aggregate_and_proof(&aggregate_and_proof, &keystore.private_key)?,
                message: aggregate_and_proof,
            }])
            .await?)
    }

    pub async fn submit_voluntary_exit(
        &self,
        validator_index: u64,
        epoch: u64,
    ) -> anyhow::Result<()> {
        self.beacon_api_client
            .submit_signed_voluntary_exit(sign_voluntary_exit(
                epoch,
                validator_index,
                &self
                    .validator_index_to_keystore
                    .get(&validator_index)
                    .ok_or_else(|| anyhow!("Keystore not found for validator: {validator_index}"))?
                    .private_key,
            )?)
            .await?;

        Ok(())
    }
}
