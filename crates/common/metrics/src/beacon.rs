use std::time::{SystemTime, UNIX_EPOCH};

use prometheus_exporter::prometheus::{
    Histogram, HistogramOpts, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, default_registry,
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry,
};

use crate::common::set_int_gauge;

// Provisioning each metrics
lazy_static::lazy_static! {
    // Node Info Metrics
    pub static ref BEACON_NODE_INFO: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "beacon_node_info", "Node information (always 1)", &["name", "version"], default_registry()
    ).expect("failed to create BEACON_NODE_INFO int gauge vec");

    pub static ref BEACON_NODE_START_TIME_SECONDS: IntGauge = register_int_gauge_with_registry!(
        "beacon_node_start_time_seconds", "Start timestamp", default_registry()
    ).expect("failed to create BEACON_NODE_START_TIME_SECONDS int gauge");

    // Sync / Head Metrics (beacon-metrics interop spec)
    pub static ref BEACON_HEAD_SLOT: IntGauge = register_int_gauge_with_registry!(
        "beacon_head_slot", "Latest slot of the beacon chain", default_registry()
    ).expect("failed to create BEACON_HEAD_SLOT int gauge");

    pub static ref BEACON_HEAD_EPOCH: IntGauge = register_int_gauge_with_registry!(
        "beacon_head_epoch", "Current head epoch of the beacon chain", default_registry()
    ).expect("failed to create BEACON_HEAD_EPOCH int gauge");

    pub static ref BEACON_FINALIZED_EPOCH: IntGauge = register_int_gauge_with_registry!(
        "beacon_finalized_epoch", "Current finalized epoch", default_registry()
    ).expect("failed to create BEACON_FINALIZED_EPOCH int gauge");

    pub static ref BEACON_CURRENT_JUSTIFIED_EPOCH: IntGauge = register_int_gauge_with_registry!(
        "beacon_current_justified_epoch", "Current justified epoch", default_registry()
    ).expect("failed to create BEACON_CURRENT_JUSTIFIED_EPOCH int gauge");

    pub static ref BEACON_PREVIOUS_JUSTIFIED_EPOCH: IntGauge = register_int_gauge_with_registry!(
        "beacon_previous_justified_epoch", "Current previously justified epoch", default_registry()
    ).expect("failed to create BEACON_PREVIOUS_JUSTIFIED_EPOCH int gauge");

    pub static ref BEACON_CURRENT_ACTIVE_VALIDATORS: IntGauge = register_int_gauge_with_registry!(
        "beacon_current_active_validators", "Current total active validators", default_registry()
    ).expect("failed to create BEACON_CURRENT_ACTIVE_VALIDATORS int gauge");

    pub static ref BEACON_REORGS_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_reorgs_total", "Total number of chain reorganizations", default_registry()
    ).expect("failed to create BEACON_REORGS_TOTAL int counter");

    // Spec types this as a Gauge (not a Counter) despite the `_total` suffix — keep it a
    // gauge to stay conformant even though the naming looks counter-like.
    pub static ref BEACON_PROCESSED_DEPOSITS_TOTAL: IntGauge = register_int_gauge_with_registry!(
        "beacon_processed_deposits_total", "Total number of deposits processed", default_registry()
    ).expect("failed to create BEACON_PROCESSED_DEPOSITS_TOTAL int gauge");

    // Peer / Network Metrics
    // `BEACON_PEER_COUNT` is Ream's own metric name; `LIBP2P_PEERS` is the
    // beacon-metrics interop-spec name (mirrors Lighthouse's "_INTEROP" alias
    // pattern). Always update both via `set_peer_count()` below, never directly,
    // or the two will drift apart.
    pub static ref BEACON_PEER_COUNT: IntGauge = register_int_gauge_with_registry!(
        "beacon_connected_peers", "Number of connected peers", default_registry()
    ).expect("failed to create BEACON_PEER_COUNT int gauge");

    pub static ref LIBP2P_PEERS: IntGauge = register_int_gauge_with_registry!(
        "libp2p_peers", "Tracks the total number of libp2p peers", default_registry()
    ).expect("failed to create LIBP2P_PEERS int gauge");

    // PeerDAS: Data Column Sidecar Gossip Metrics
    pub static ref BEACON_DATA_COLUMN_SIDECAR_PROCESSING_REQUESTS_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_data_column_sidecar_processing_requests_total",
        "Data column sidecars submitted for processing",
        default_registry()
    ).expect("failed to create BEACON_DATA_COLUMN_SIDECAR_PROCESSING_REQUESTS_TOTAL int counter");

    pub static ref BEACON_DATA_COLUMN_SIDECAR_PROCESSING_SUCCESSES_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_data_column_sidecar_processing_successes_total",
        "Data column sidecars verified for gossip",
        default_registry()
    ).expect("failed to create BEACON_DATA_COLUMN_SIDECAR_PROCESSING_SUCCESSES_TOTAL int counter");

    pub static ref BEACON_DATA_COLUMN_SIDECAR_GOSSIP_VERIFICATION_SECONDS: Histogram = {
        // Gossip verification is expected to be fast (sub-100ms typical case).
        // Starting point only — re-tune against real hardware benchmarks.
        let opts = HistogramOpts::new(
            "beacon_data_column_sidecar_gossip_verification_seconds",
            "Time spent verifying data column sidecars for gossip"
        ).buckets(vec![0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]);
        register_histogram_with_registry!(
            opts,
            default_registry()
        ).expect("failed to create BEACON_DATA_COLUMN_SIDECAR_GOSSIP_VERIFICATION_SECONDS")
    };

    // PeerDAS: Data Availability / Reconstruction Metrics
    pub static ref BEACON_DATA_AVAILABILITY_RECONSTRUCTED_COLUMNS_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_data_availability_reconstructed_columns_total",
        "Total count of reconstructed columns",
        default_registry()
    ).expect("failed to create BEACON_DATA_AVAILABILITY_RECONSTRUCTED_COLUMNS_TOTAL int counter");

    pub static ref BEACON_DATA_AVAILABILITY_RECONSTRUCTION_TIME_SECONDS: Histogram = {
        // Reconstruction can run into hundreds of ms to low seconds under load.
        let opts = HistogramOpts::new(
            "beacon_data_availability_reconstruction_time_seconds",
            "Time taken to reconstruct columns"
        ).buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0]);
        register_histogram_with_registry!(
            opts,
            default_registry()
        ).expect("failed to create BEACON_DATA_AVAILABILITY_RECONSTRUCTION_TIME_SECONDS")
    };

    // PeerDAS: Data Column Sidecar Computation Metrics
    pub static ref BEACON_DATA_COLUMN_SIDECAR_COMPUTATION_SECONDS: Histogram = {
        let opts = HistogramOpts::new(
            "beacon_data_column_sidecar_computation_seconds",
            "Time to compute data column sidecar (cells, proofs, inclusion proof)"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0]);
        register_histogram_with_registry!(
            opts,
            default_registry()
        ).expect("failed to create BEACON_DATA_COLUMN_SIDECAR_COMPUTATION_SECONDS")
    };

    // PeerDAS: Inclusion proof verification metrics
    pub static ref BEACON_DATA_COLUMN_SIDECAR_INCLUSION_PROOF_VERIFICATION_SECONDS: Histogram = {
        let opts = HistogramOpts::new(
            "beacon_data_column_sidecar_inclusion_proof_verification_seconds",
            "Time to verify data column sidecar inclusion proof"
        ).buckets(vec![0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1]);
        register_histogram_with_registry!(
            opts,
            default_registry()
        ).expect("failed to create BEACON_DATA_COLUMN_SIDECAR_INCLUSION_PROOF_VERIFICATION_SECONDS")
    };

    // PeerDAS: KZG Verification Metrics
    pub static ref BEACON_KZG_VERIFICATION_DATA_COLUMN_BATCH_SECONDS: Histogram = {
        // Scales with number of columns in the batch — wider range than a single-item check.
        let opts = HistogramOpts::new(
            "beacon_kzg_verification_data_column_batch_seconds",
           "Time spent verifying batched data column KZG proofs"
        ).buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0]);
        register_histogram_with_registry!(
            opts,
            default_registry()
        ).expect("failed to create BEACON_KZG_VERIFICATION_DATA_COLUMN_BATCH_SECONDS")
    };

    // PeerDAS: Custody Metrics
    pub static ref BEACON_CUSTODY_GROUPS: IntGauge = register_int_gauge_with_registry!(
        "beacon_custody_groups", "Total number of custody groups within a node", default_registry()
    ).expect("failed to create BEACON_CUSTODY_GROUPS int gauge");

    pub static ref BEACON_CUSTODY_GROUPS_BACKFILLED: IntGauge = register_int_gauge_with_registry!(
        "beacon_custody_groups_backfilled", "Total number of custody groups backfilled by a node", default_registry()
    ).expect("failed to create BEACON_CUSTODY_GROUPS_BACKFILLED int gauge");

    // PeerDAS: Engine API — engine_getBlobsV2 Metrics
    pub static ref BEACON_ENGINE_GET_BLOBS_V2_REQUESTS_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_engine_getBlobsV2_requests_total", "Total engine_getBlobsV2 requests sent", default_registry()
    ).expect("failed to create BEACON_ENGINE_GET_BLOBS_V2_REQUESTS_TOTAL int counter");

    pub static ref BEACON_ENGINE_GET_BLOBS_V2_RESPONSES_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_engine_getBlobsV2_responses_total", "Total successful engine_getBlobsV2 responses received", default_registry()
    ).expect("failed to create BEACON_ENGINE_GET_BLOBS_V2_RESPONSES_TOTAL int counter");

    pub static ref BEACON_ENGINE_GET_BLOBS_V2_REQUEST_DURATION_SECONDS: Histogram = {
        // Round trip to the EL — expect low ms up to a couple seconds under load.
        let opts = HistogramOpts::new(
            "beacon_engine_getBlobsV2_request_duration_seconds",
            "Duration of engine_getBlobsV2 requests"
        ).buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0]);
        register_histogram_with_registry!(
            opts,
            default_registry()
        ).expect("failed to create BEACON_ENGINE_GET_BLOBS_V2_REQUEST_DURATION_SECONDS")
    };

    // PeerDAS: Engine API — engine_get_blobs_v3 Metrics
    pub static ref BEACON_ENGINE_GET_BLOBS_V3_REQUESTS_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_engine_getBlobsV3_requests_total", "Total engine_getBlobsV3 requests sent", default_registry()
    ).expect("failed to create BEACON_ENGINE_GET_BLOBS_V3_REQUESTS_TOTAL int counter");

    pub static ref BEACON_ENGINE_GET_BLOBS_V3_COMPLETE_RESPONSES_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_engine_getBlobsV3_complete_responses_total", "Total complete engine_getBlobsV3 responses", default_registry()
    ).expect("failed to create BEACON_ENGINE_GET_BLOBS_V3_COMPLETE_RESPONSES_TOTAL int counter");

    pub static ref BEACON_ENGINE_GET_BLOBS_V3_PARTIAL_RESPONSES_TOTAL: IntCounter = register_int_counter_with_registry!(
        "beacon_engine_getBlobsV3_partial_responses_total", "Total partial engine_getBlobsV3 responses", default_registry()
    ).expect("failed to create BEACON_ENGINE_GET_BLOBS_V3_PARTIAL_RESPONSES_TOTAL int counter");

    pub static ref BEACON_ENGINE_GET_BLOBS_V3_REQUEST_DURATION_SECONDS: Histogram = {
        let opts = HistogramOpts::new(
            "beacon_engine_getBlobsV3_request_duration_seconds",
            "Duration of engine_getBlobsV3 requests"
        ).buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0]);
        register_histogram_with_registry!(
            opts,
            default_registry()
        ).expect("failed to create BEACON_ENGINE_GET_BLOBS_V3_REQUEST_DURATION_SECONDS")
    };

    // PeerDAS: Partial Data Column Metrics (Cell-Level Dissemination)
    pub static ref BEACON_PARTIAL_MESSAGE_USEFUL_CELLS_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "beacon_partial_message_useful_cells_total", "Useful cells received via a partial message", &["column_index"], default_registry()
    ).expect("failed to create BEACON_PARTIAL_MESSAGE_USEFUL_CELLS_TOTAL int counter vec");

    pub static ref BEACON_PARTIAL_MESSAGE_CELLS_RECEIVED_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "beacon_partial_message_cells_received_total", "Total cells received via a partial message", &["column_index"], default_registry()
    ).expect("failed to create BEACON_PARTIAL_MESSAGE_CELLS_RECEIVED_TOTAL int counter vec");

    pub static ref BEACON_USEFUL_FULL_COLUMNS_RECEIVED_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "beacon_useful_full_columns_received_total", "Useful full columns received", &["column_index"], default_registry()
    ).expect("failed to create BEACON_USEFUL_FULL_COLUMNS_RECEIVED_TOTAL int counter vec");

    pub static ref BEACON_PARTIAL_MESSAGE_COLUMN_COMPLETIONS_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "beacon_partial_message_column_completions_total", "Times a partial message first completed a column", &["column_index"], default_registry()
    ).expect("failed to create BEACON_PARTIAL_MESSAGE_COLUMN_COMPLETIONS_TOTAL int counter vec");
}

/// Zero-initializes gauges so dashboards show `0` instead of "no data" before the
/// first real observation. Mirrors `lean::init_aggregate_coverage_metrics`.
pub fn init_beacon_metrics() {
    set_int_gauge(&BEACON_CUSTODY_GROUPS, 0);
    set_int_gauge(&BEACON_CUSTODY_GROUPS_BACKFILLED, 0);
    set_peer_count(0);
}

/// Sets `BEACON_PEER_COUNT` and `LIBP2P_PEERS` (the beacon-metrics
/// interop-spec name) together, so the two can't drift apart.
pub fn set_peer_count(count: i64) {
    set_int_gauge(&BEACON_PEER_COUNT, count);
    set_int_gauge(&LIBP2P_PEERS, count);
}

pub fn init_node_metrics() {
    init_beacon_metrics();

    BEACON_NODE_INFO
        .with_label_values(&["ream", env!("CARGO_PKG_VERSION")])
        .set(1);

    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    BEACON_NODE_START_TIME_SECONDS.set(start_time as i64);
}
