pub mod event;
pub mod http_client;

use std::{pin::Pin, str::FromStr, time::Duration};

use alloy_primitives::{B256, hex};
use anyhow::anyhow;
use event::{BeaconEvent, EventTopic};
use eventsource_client::{Client, ClientBuilder, SSE};
use futures::{Stream, StreamExt};
use http_client::{ClientWithBaseUrl, ContentType};
use ream_beacon_api_types::{
    block::{BroadcastValidation, FullBlockData, ProduceBlockData, ProduceBlockResponse},
    committee::BeaconCommitteeSubscription,
    duties::{AttesterDuty, ProposerDuty, SyncCommitteeDuty},
    error::ValidatorError,
    id::{ID, ValidatorID},
    request::{SyncCommitteeRequestItem, ValidatorsPostRequest},
    responses::{
        BeaconResponse, DataResponse, DataVersionedResponse, DutiesResponse,
        ETH_CONSENSUS_VERSION_HEADER, RootResponse, SyncCommitteeDutiesResponse, VERSION,
    },
    sync::SyncStatus,
    validator::{ValidatorData, ValidatorStatus},
};
use ream_bls::BLSSignature;
use ream_consensus_beacon::{
    attestation::Attestation,
    electra::{
        beacon_block::SignedBeaconBlock,
        blinded_beacon_block::{BlindedBeaconBlock, SignedBlindedBeaconBlock},
    },
    genesis::Genesis,
    single_attestation::SingleAttestation,
    voluntary_exit::SignedVoluntaryExit,
};
use ream_consensus_misc::{attestation_data::AttestationData, fork::Fork};
use ream_network_spec::networks::NetworkSpec;
use reqwest::{Url, header::HeaderMap};
use serde_json::json;
use ssz::{Decode, Encode};
use tracing::{error, info};

use crate::{
    aggregate_and_proof::SignedAggregateAndProof,
    contribution_and_proof::{SignedContributionAndProof, SyncCommitteeContribution},
};

#[derive(Clone)]
pub struct BeaconApiClient {
    http_client: ClientWithBaseUrl,
}

impl BeaconApiClient {
    pub fn new(beacon_api_endpoint: Url, request_timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            http_client: ClientWithBaseUrl::new(
                beacon_api_endpoint,
                request_timeout,
                ContentType::Ssz,
            )?,
        })
    }

    pub fn get_events_stream(
        &self,
        topics: &[EventTopic],
        stream_tag: &'static str,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = BeaconEvent> + Send>>> {
        let endpoint = self.http_client.base_url().join(&format!(
            "/eth/v1/events?topics={}",
            topics
                .iter()
                .map(|topic| topic.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ))?;

        Ok(ClientBuilder::for_url(endpoint.as_str())?
            .build()
            .stream()
            .filter_map(move |event| async move {
                let event = match event {
                    Ok(SSE::Event(event)) => event,
                    Ok(SSE::Connected(connection_details)) => {
                        info!("{stream_tag}: Connected to SSE stream: {connection_details:?}");
                        return None;
                    }
                    Ok(SSE::Comment(comment)) => {
                        info!("{stream_tag}: Received comment: {comment:?}");
                        return None;
                    }
                    Err(err) => {
                        error!("{stream_tag}: Error receiving event: {err:?}");
                        return None;
                    }
                };
                match BeaconEvent::try_from(event) {
                    Ok(event) => Some(event),
                    Err(err) => {
                        error!("{stream_tag}: Failed to decode event: {err:?}");
                        None
                    }
                }
            })
            .boxed())
    }

    pub async fn get_genesis(&self) -> anyhow::Result<DataResponse<Genesis>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v1/beacon/genesis".to_string())?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_block_root(
        &self,
        state_id: ID,
    ) -> anyhow::Result<BeaconResponse<RootResponse>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get(format!("/eth/v1/beacon/states/{state_id}/root"))?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_config_spec(
        &self,
    ) -> anyhow::Result<DataResponse<NetworkSpec>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v1/config/spec".to_string())?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_state_fork(
        &self,
        state_id: ID,
    ) -> anyhow::Result<BeaconResponse<Fork>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get(format!("/eth/v1/beacon/states/{state_id}/fork"))?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn publish_sync_committee_signature(
        &self,
        sync_committee_request: Vec<SyncCommitteeRequestItem>,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        "/eth/v1/beacon/pool/sync_committees".to_string(),
                        ContentType::Json,
                    )?
                    .json(&sync_committee_request)
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }

    pub async fn get_state_validator_list(
        &self,
        state_id: ID,
        validator_ids: Option<Vec<ValidatorID>>,
        validator_statuses: Option<Vec<ValidatorStatus>>,
    ) -> anyhow::Result<BeaconResponse<Vec<ValidatorData>>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        format!("/eth/v1/beacon/states/{state_id}/validators"),
                        ContentType::Json,
                    )?
                    .json(&ValidatorsPostRequest {
                        ids: validator_ids,
                        statuses: validator_statuses,
                    })
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_state_validator(
        &self,
        state_id: ID,
        validator_id: ValidatorID,
    ) -> anyhow::Result<BeaconResponse<ValidatorData>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get(format!(
                        "/eth/v1/beacon/states/{state_id}/validators/{validator_id}"
                    ))?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_node_syncing_status(
        &self,
    ) -> anyhow::Result<DataResponse<SyncStatus>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v1/node/syncing".to_string())?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_proposer_duties(
        &self,
        epoch: u64,
    ) -> anyhow::Result<DutiesResponse<ProposerDuty>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get(format!("eth/v1/validator/duties/proposer/{epoch}"))?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_attester_duties(
        &self,
        epoch: u64,
        validator_indices: &[u64],
    ) -> Result<DutiesResponse<AttesterDuty>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        format!("/eth/v1/validator/duties/attester/{epoch}"),
                        ContentType::Json,
                    )?
                    .json(&json!(
                        validator_indices
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                    ))
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_sync_committee_duties(
        &self,
        epoch: u64,
        validator_indices: &[u64],
    ) -> Result<SyncCommitteeDutiesResponse<SyncCommitteeDuty>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        format!("/eth/v1/validator/duties/sync/{epoch}"),
                        ContentType::Json,
                    )?
                    .json(&json!(
                        validator_indices
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                    ))
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn prepare_committee_subnet(
        &self,
        subscriptions: Vec<BeaconCommitteeSubscription>,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        "/eth/v1/validator/beacon_committee_subscriptions".to_string(),
                        ContentType::Json,
                    )?
                    .json(&subscriptions)
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }

    pub async fn get_sync_committee_contribution(
        &self,
        slot: u64,
        subcommittee_index: u64,
        beacon_block_root: B256,
    ) -> Result<DataResponse<SyncCommitteeContribution>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v1/validator/sync_committee_contribution".to_string())?
                    .query(&[
                        ("slot", slot.to_string()),
                        ("subcommittee_index", subcommittee_index.to_string()),
                        ("beacon_block_root", beacon_block_root.to_string()),
                    ])
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn publish_contribution_and_proofs(
        &self,
        signed_contribution_and_proofs: Vec<SignedContributionAndProof>,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        "/eth/v1/validator/contribution_and_proofs".to_string(),
                        ContentType::Json,
                    )?
                    .json(&signed_contribution_and_proofs)
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }

    pub async fn get_attestation_data(
        &self,
        slot: u64,
        committee_index: u64,
    ) -> Result<DataResponse<AttestationData>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v1/validator/attestation_data".to_string())?
                    .query(&[
                        ("slot", slot.to_string()),
                        ("committee_index", committee_index.to_string()),
                    ])
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn submit_attestation(
        &self,
        single_attestation: Vec<SingleAttestation>,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        "/eth/v2/beacon/pool/attestations".to_string(),
                        ContentType::Json,
                    )?
                    .header(ETH_CONSENSUS_VERSION_HEADER, VERSION)
                    .json(&single_attestation)
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }

    pub async fn publish_aggregate_and_proofs(
        &self,
        signed_aggregate_and_proofs: Vec<SignedAggregateAndProof>,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        "/eth/v2/validator/aggregate_and_proofs".to_string(),
                        ContentType::Json,
                    )?
                    .header(ETH_CONSENSUS_VERSION_HEADER, VERSION)
                    .json(&signed_aggregate_and_proofs)
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }

    pub async fn get_aggregated_attestation(
        &self,
        attestation_data_root: B256,
        slot: u64,
        committee_index: u64,
    ) -> anyhow::Result<DataVersionedResponse<Attestation>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v2/validator/aggregate_attestation".to_string())?
                    .query(&[
                        ("attestation_data_root", attestation_data_root.to_string()),
                        ("slot", slot.to_string()),
                        ("committee_index", committee_index.to_string()),
                    ])
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn produce_block(
        &self,
        slot: u64,
        randao_reveal: BLSSignature,
        graffiti: Option<B256>,
        skip_randao_verification: Option<bool>,
        builder_boost_factor: Option<u64>,
    ) -> anyhow::Result<ProduceBlockResponse, ValidatorError> {
        let mut request_builder = self
            .http_client
            .get(format!("/eth/v3/validator/blocks/{slot}"))?
            .query(&[("randao_reveal", hex::encode(randao_reveal.to_slice()))]);

        if let Some(graffiti_value) = graffiti {
            request_builder = request_builder.query(&[("graffiti", graffiti_value.to_string())]);
        }

        if let Some(skip_randao) = skip_randao_verification {
            request_builder =
                request_builder.query(&[("skip_randao_verification", skip_randao.to_string())]);
        }

        if let Some(boost_factor) = builder_boost_factor {
            request_builder =
                request_builder.query(&[("builder_boost_factor", boost_factor.to_string())]);
        }

        let response = self.http_client.execute(request_builder.build()?).await?;

        let headers = response.headers();

        let content_type = get_header_str(headers, "content-type")?;

        let version = get_header_str(headers, "Eth-Consensus-Version")?.to_string();
        let execution_payload_blinded =
            parse_header::<bool>(headers, "Eth-Execution-Payload-Blinded")?;
        let execution_payload_value = parse_header::<u64>(headers, "Eth-Execution-Payload-Value")?;
        let consensus_block_value = parse_header::<u64>(headers, "Eth-Consensus-Block-Value")?;

        if content_type.contains("application/octet-stream") {
            Ok(ProduceBlockResponse {
                version,
                execution_payload_blinded,
                execution_payload_value,
                consensus_block_value,
                data: if execution_payload_blinded {
                    ProduceBlockData::Blinded(
                        BlindedBeaconBlock::from_ssz_bytes(&response.bytes().await?).map_err(
                            |err| anyhow!("Failed to decode SSZ bytes for blinded block: {err:?}"),
                        )?,
                    )
                } else {
                    ProduceBlockData::Full(
                        FullBlockData::from_ssz_bytes(&response.bytes().await?).map_err(|err| {
                            anyhow!("Failed to decode SSZ bytes for full block: {err:?}")
                        })?,
                    )
                },
            })
        } else {
            Ok(ProduceBlockResponse {
                version,
                execution_payload_blinded,
                execution_payload_value,
                consensus_block_value,
                data: if execution_payload_blinded {
                    ProduceBlockData::Blinded(response.json().await?)
                } else {
                    ProduceBlockData::Full(response.json().await?)
                },
            })
        }
    }

    pub async fn publish_block(
        &self,
        broadcast_validation: BroadcastValidation,
        signed_beacon_block: SignedBeaconBlock,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post("/eth/v2/beacon/blocks".to_string(), ContentType::Ssz)?
                    .query(&[(
                        "broadcast_validation",
                        serde_json::to_string(&broadcast_validation)?,
                    )])
                    .header(ETH_CONSENSUS_VERSION_HEADER, VERSION)
                    .body(signed_beacon_block.as_ssz_bytes())
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }

    pub async fn publish_blinded_block(
        &self,
        broadcast_validation: BroadcastValidation,
        signed_blinded_beacon_block: SignedBlindedBeaconBlock,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        "/eth/v2/beacon/blinded_blocks".to_string(),
                        ContentType::Ssz,
                    )?
                    .query(&[(
                        "broadcast_validation",
                        serde_json::to_string(&broadcast_validation)?,
                    )])
                    .header(ETH_CONSENSUS_VERSION_HEADER, VERSION)
                    .body(signed_blinded_beacon_block.as_ssz_bytes())
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }

    pub async fn submit_signed_voluntary_exit(
        &self,
        signed_voluntary_exit: SignedVoluntaryExit,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(
                        "/eth/v1/beacon/pool/voluntary_exits".to_string(),
                        ContentType::Json,
                    )?
                    .json(&signed_voluntary_exit)
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }
}

pub fn get_header_str<'a>(headers: &'a HeaderMap, key: &'a str) -> anyhow::Result<&'a str> {
    headers
        .get(key)
        .ok_or_else(|| anyhow!("Header '{key}' not found"))?
        .to_str()
        .map_err(|err| anyhow!("Failed to convert header '{key}' to string: {err}"))
}

pub fn parse_header<T: std::str::FromStr>(headers: &HeaderMap, key: &str) -> anyhow::Result<T>
where
    <T as FromStr>::Err: std::error::Error,
{
    get_header_str(headers, key)?
        .parse::<T>()
        .map_err(|err| anyhow!("Failed to parse header '{key}': {err}"))
}
