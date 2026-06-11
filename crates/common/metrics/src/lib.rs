pub mod timer;

use prometheus_exporter::prometheus::{
    HistogramOpts, HistogramTimer, HistogramVec, IntCounterVec, IntGaugeVec, default_registry,
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry,
};

use crate::timer::DiscardOnDropHistogramTimer;

pub const ATTESTATION_AGGREGATE_COVERAGE_SECTIONS: &[&str] = &[
    "timely",
    "late",
    "block",
    "combined",
    "agg_start_new",
    "proposal_payloads",
    "proposal_gossip",
    "proposal_combined",
];

pub const AGGREGATOR_SKIP_REASONS: &[&str] = &[
    "not_aggregator",
    "not_synced",
    "missing_state",
    "spawn_failed",
    "other",
];

pub const ATTESTATION_AGGREGATE_COVERAGE_DIFFERENT_DIRECTIONS: &[&str] =
    &["block_only", "timely_only"];

// Provisioning each metrics
lazy_static::lazy_static! {
    // Node Info Metrics
    pub static ref NODE_INFO: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_node_info",
        "Node information (always 1)",
        &["name", "version"],
        default_registry()
    ).expect("failed to create NODE_INFO int gauge vec");

    pub static ref NODE_START_TIME_SECONDS: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_node_start_time_seconds",
        "Start timestamp",
        &[],
        default_registry()
    ).expect("failed to create NODE_START_TIME_SECONDS int gauge vec");

    pub static ref PROPOSE_BLOCK_TIME: HistogramVec = register_histogram_vec_with_registry!(
        "lean_propose_block_time",
        "Duration of the sections it takes to propose a new block",
        &["section"],
        default_registry()
    ).expect("failed to create PROPOSE_BLOCK_TIME histogram vec");

    pub static ref HEAD_SLOT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_head_slot",
        "The current head slot",
        &[],
        default_registry()
    ).expect("failed to create HEAD_SLOT int gauge vec");

    // Sync Metrics
    pub static ref CURRENT_SLOT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_current_slot",
        "Current slot of the lean chain",
        &[],
        default_registry()
    ).expect("failed to create CURRENT_SLOT int gauge vec");

    pub static ref SAFE_TARGET_SLOT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_safe_target_slot",
        "Safe target slot",
        &[],
        default_registry()
    ).expect("failed to create SAFE_TARGET_SLOT int gauge vec");

    pub static ref JUSTIFIED_SLOT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_justified_slot",
        "The current justified slot",
        &[],
        default_registry()
    ).expect("failed to create JUSTIFIED_SLOT int gauge vec");

    pub static ref FINALIZED_SLOT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_finalized_slot",
        "The current finalized slot",
        &[],
        default_registry()
    ).expect("failed to create FINALIZED_SLOT int gauge vec");

    pub static ref LATEST_JUSTIFIED_SLOT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_latest_justified_slot",
        "The latest justified slot",
        &[],
        default_registry()
    ).expect("failed to create LATEST_JUSTIFIED_SLOT int gauge vec");

    pub static ref LATEST_FINALIZED_SLOT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_latest_finalized_slot",
        "The latest finalized slot",
        &[],
        default_registry()
    ).expect("failed to create LATEST_FINALIZED_SLOT int gauge vec");

    pub static ref VALIDATORS_COUNT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_validators_count",
        "The total number of validators",
        &[],
        default_registry()
    ).expect("failed to create VALIDATORS_COUNT int gauge vec");

    // Fork-Choice Metrics
    pub static ref FORK_CHOICE_BLOCK_PROCESSING_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_fork_choice_block_processing_time_seconds",
            "Time taken to process block"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 1.0, 1.25, 1.5, 2.0, 4.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create FORK_CHOICE_BLOCK_PROCESSING_TIME histogram vec")
    };

    pub static ref ATTESTATIONS_VALID_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_attestations_valid_total",
        "Total number of valid attestations",
        &[],
        default_registry()
    ).expect("failed to create ATTESTATIONS_VALID_TOTAL int counter vec");

    pub static ref ATTESTATIONS_INVALID_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_attestations_invalid_total",
        "Total number of invalid attestations",
        &[],
        default_registry()
    ).expect("failed to create ATTESTATIONS_INVALID_TOTAL int counter vec");

    pub static ref ATTESTATION_VALIDATION_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_attestation_validation_time_seconds",
            "Time taken to validate attestation"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 1.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create ATTESTATION_VALIDATION_TIME histogram vec")
    };

    pub static ref FORK_CHOICE_REORGS_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_fork_choice_reorgs_total",
        "Total number of fork choice reorgs",
        &[],
        default_registry()
    ).expect("failed to create FORK_CHOICE_REORGS_TOTAL int counter vec");

    pub static ref FORK_CHOICE_REORG_DEPTH: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_fork_choice_reorg_depth",
            "Depth of fork choice reorgs (in blocks)"
        ).buckets(vec![1.0, 2.0, 3.0, 5.0, 7.0, 10.0, 20.0, 30.0, 50.0, 100.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create FORK_CHOICE_REORG_DEPTH histogram vec")
    };

    // State Transition Metrics
    pub static ref STATE_TRANSITION_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_state_transition_time_seconds",
            "Time taken to process state transition"
        ).buckets(vec![0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create STATE_TRANSITION_TIME histogram vec")
    };

    pub static ref STATE_TRANSITION_BLOCK_PROCESSING_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_state_transition_block_processing_time_seconds",
            "Time taken to process block in state transition"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 1.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create STATE_TRANSITION_BLOCK_PROCESSING_TIME histogram vec")
    };

    pub static ref STATE_TRANSITION_SLOTS_PROCESSED_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_state_transition_slots_processed_total",
        "Total number of slots processed in state transition",
        &[],
        default_registry()
    ).expect("failed to create STATE_TRANSITION_SLOTS_PROCESSED_TOTAL int counter vec");

    pub static ref STATE_TRANSITION_SLOTS_PROCESSING_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_state_transition_slots_processing_time_seconds",
            "Time taken to process slots in state transition"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 1.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create STATE_TRANSITION_SLOTS_PROCESSING_TIME histogram vec")
    };

    pub static ref STATE_TRANSITION_ATTESTATIONS_PROCESSED_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_state_transition_attestations_processed_total",
        "Total number of attestations processed in state transition",
        &[],
        default_registry()
    ).expect("failed to create STATE_TRANSITION_ATTESTATIONS_PROCESSED_TOTAL int counter vec");

    pub static ref STATE_TRANSITION_ATTESTATIONS_PROCESSING_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_state_transition_attestations_processing_time_seconds",
            "Time taken to process attestations in state transition"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 1.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create STATE_TRANSITION_ATTESTATIONS_PROCESSING_TIME histogram vec")
    };

    // Finalization Metrics
    pub static ref FINALIZATIONS_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_finalizations_total",
        "Total number of finalization attempts",
        &["result"],
        default_registry()
    ).expect("failed to create FINALIZATIONS_TOTAL int counter vec");

    // PQ Signature Metrics
    pub static ref PQ_SIG_ATTESTATION_SIGNING_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_pq_sig_attestation_signing_time_seconds",
            "Time taken to sign an attestation"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 1.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create PQ_SIG_ATTESTATION_SIGNING_TIME histogram vec")
    };

    pub static ref PQ_SIG_ATTESTATION_VERIFICATION_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_pq_sig_attestation_verification_time_seconds",
            "Time taken to verify an attestation signature"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 1.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create PQ_SIG_ATTESTATION_VERIFICATION_TIME histogram vec")
    };

    pub static ref PQ_SIG_ATTESTATION_SIGNATURES_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_pq_sig_attestation_signatures_total",
        "Total number of individual attestation signatures",
        &[],
        default_registry()
    ).expect("failed to create PQ_SIG_ATTESTATION_SIGNATURES_TOTAL int counter vec");

    pub static ref PQ_SIG_ATTESTATION_SIGNATURES_VALID_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_pq_sig_attestation_signatures_valid_total",
        "Total number of valid individual attestation signatures",
        &[],
        default_registry()
    ).expect("failed to create PQ_SIG_ATTESTATION_SIGNATURES_VALID_TOTAL int counter vec");

    pub static ref PQ_SIG_ATTESTATION_SIGNATURES_INVALID_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_pq_sig_attestation_signatures_invalid_total",
        "Total number of invalid individual attestation signatures",
        &[],
        default_registry()
    ).expect("failed to create PQ_SIG_ATTESTATION_SIGNATURES_INVALID_TOTAL int counter vec");

    pub static ref PQ_SIG_AGGREGATED_SIGNATURES_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_pq_sig_aggregated_signatures_total",
        "Total number of aggregated signatures",
        &[],
        default_registry()
    ).expect("failed to create PQ_SIG_AGGREGATED_SIGNATURES_TOTAL int counter vec");

    pub static ref PQ_SIG_ATTESTATIONS_IN_AGGREGATED_SIGNATURES_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_pq_sig_attestations_in_aggregated_signatures_total",
        "Total number of attestations included into aggregated signatures",
        &[],
        default_registry()
    ).expect("failed to create PQ_SIG_ATTESTATIONS_IN_AGGREGATED_SIGNATURES_TOTAL int counter vec");

    pub static ref PQ_SIG_AGGREGATED_SIGNATURES_VALID_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_pq_sig_aggregated_signatures_valid_total",
        "Total number of valid aggregated signatures",
        &[],
        default_registry()
    ).expect("failed to create PQ_SIG_AGGREGATED_SIGNATURES_VALID_TOTAL int counter vec");

    pub static ref PQ_SIG_AGGREGATED_SIGNATURES_BUILDING_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_pq_sig_aggregated_signatures_building_time_seconds",
            "Time taken to build an aggregated attestation signature"
        ).buckets(vec![0.1, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 4.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create PQ_SIG_AGGREGATED_SIGNATURES_BUILDING_TIME histogram vec")
    };

    pub static ref PQ_SIG_AGGREGATED_SIGNATURES_INVALID_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_pq_sig_aggregated_signatures_invalid_total",
        "Total number of invalid aggregated signatures",
        &[],
        default_registry()
    ).expect("failed to create PQ_SIG_AGGREGATED_SIGNATURES_INVALID_TOTAL int counter vec");

    pub static ref PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_pq_sig_aggregated_signatures_verification_time_seconds",
            "Time taken to verify an aggregated attestation signature"
        ).buckets(vec![0.1, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 4.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME histogram vec")
    };

    // Network Metrics
    pub static ref LEAN_PEER_COUNT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_connected_peers",
        "Number of connected peers",
        &[],
        default_registry()
    ).expect("failed to create LEAN_PEER_COUNT int gauge vec");

    pub static ref LEAN_CONNECTION_EVENT_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_peer_connection_events_total",
        "Number of connection events",
        &[],
        default_registry()
    ).expect("failed to create LEAN_CONNECTION_EVENT_TOTAL int counter vec");

    pub static ref LEAN_DISCONNECTION_EVENT_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_peer_disconnection_events_total",
        "Number of disconnection events",
        &[],
        default_registry()
    ).expect("failed to create LEAN_DISCONNECTION_EVENT_TOTAL int counter vec");

    pub static ref ATTESTATION_COMMITTEE_SUBNET: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_attestation_committee_subnet",
        "Node's attestation committee subnet",
        &[],
        default_registry()
    ).expect("failed to create ATTESTATION_COMMITTEE_SUBNET int gauge vec");

    pub static ref ATTESTATION_COMMITTEE_COUNT: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_attestation_committee_count",
        "Number of attestation committees",
        &[],
        default_registry()
    ).expect("failed to create ATTESTATION_COMMITTEE_COUNT int gauge vec");

    // Fork-Choice Additional Metrics
    pub static ref GOSSIP_SIGNATURES: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_gossip_signatures",
        "Number of gossip signatures in fork-choice store",
        &[],
        default_registry()
    ).expect("failed to create GOSSIP_SIGNATURES int gauge vec");

    pub static ref LATEST_NEW_AGGREGATED_PAYLOADS: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_latest_new_aggregated_payloads",
        "Number of new aggregated payload items",
        &[],
        default_registry()
    ).expect("failed to create LATEST_NEW_AGGREGATED_PAYLOADS int gauge vec");

    pub static ref LATEST_KNOWN_AGGREGATED_PAYLOADS: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_latest_known_aggregated_payloads",
        "Number of known aggregated payload items",
        &[],
        default_registry()
    ).expect("failed to create LATEST_KNOWN_AGGREGATED_PAYLOADS int gauge vec");

    pub static ref COMMITTEE_SIGNATURES_AGGREGATION_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_committee_signatures_aggregation_time_seconds",
            "Time taken to aggregate committee signatures"
        ).buckets(vec![0.05, 0.1, 0.25, 0.5, 0.75, 1.0, 2.0, 3.0, 4.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create COMMITTEE_SIGNATURES_AGGREGATION_TIME histogram vec")
    };

    // Block Production Metrics
    pub static ref BLOCK_AGGREGATED_PAYLOADS: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_block_aggregated_payloads",
            "Number of aggregated_payloads in a block"
        ).buckets(vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create BLOCK_AGGREGATED_PAYLOADS histogram vec")
    };

    pub static ref BLOCK_BUILDING_PAYLOAD_AGGREGATION_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_block_building_payload_aggregation_time_seconds",
            "Time taken to build aggregated_payloads during block building"
        ).buckets(vec![0.1, 0.25, 0.5, 0.75, 1.0, 2.0, 3.0, 4.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create BLOCK_BUILDING_PAYLOAD_AGGREGATION_TIME histogram vec")
    };

    pub static ref BLOCK_BUILDING_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_block_building_time_seconds",
            "Time taken to build a block"
        ).buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 0.75, 1.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create BLOCK_BUILDING_TIME histogram vec")
    };

    pub static ref BLOCK_BUILDING_SUCCESS_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_block_building_success_total",
        "Successful block builds",
        &[],
        default_registry()
    ).expect("failed to create BLOCK_BUILDING_SUCCESS_TOTAL int counter vec");

    pub static ref BLOCK_BUILDING_FAILURES_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_block_building_failures_total",
        "Failed block builds",
        &[],
        default_registry()
    ).expect("failed to create BLOCK_BUILDING_FAILURES_TOTAL int counter vec");

    // Gossip Message Size Metrics
    pub static ref GOSSIP_BLOCK_SIZE_BYTES: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_gossip_block_size_bytes",
            "Bytes size of a gossip block message"
        ).buckets(vec![10000.0, 50000.0, 100000.0, 250000.0, 500000.0, 1000000.0, 2000000.0, 5000000.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create GOSSIP_BLOCK_SIZE_BYTES histogram vec")
    };

    pub static ref GOSSIP_ATTESTATION_SIZE_BYTES: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_gossip_attestation_size_bytes",
            "Bytes size of a gossip attestation message"
        ).buckets(vec![512.0, 1024.0, 2048.0, 4096.0, 8192.0, 16384.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create GOSSIP_ATTESTATION_SIZE_BYTES histogram vec")
    };

    pub static ref GOSSIP_AGGREGATION_SIZE_BYTES: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_gossip_aggregation_size_bytes",
            "Bytes size of a gossip aggregated attestation message"
        ).buckets(vec![1024.0, 4096.0, 16384.0, 65536.0, 131072.0, 262144.0, 524288.0, 1048576.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create GOSSIP_AGGREGATION_SIZE_BYTES histogram vec")
    };

    // Validator Attestation Production Metrics
    pub static ref ATTESTATIONS_PRODUCTION_TIME: HistogramVec = {
        let opts = HistogramOpts::new(
            "lean_attestations_production_time_seconds",
            "Time taken to produce attestation"
        ).buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 0.75, 1.0]);
        register_histogram_vec_with_registry!(
            opts,
            &[],
            default_registry()
        ).expect("failed to create ATTESTATIONS_PRODUCTION_TIME histogram vec")
    };

    // State Transition Additional Metrics
    pub static ref IS_AGGREGATOR: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_is_aggregator",
        "Validator's is_aggregator status. True=1, False=0",
        &[],
        default_registry()
    ).expect("failed to create IS_AGGREGATOR int gauge vec");

    // Gossip Mesh Peers Metrics
    pub static ref LEAN_GOSSIP_MESH_PEERS: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_gossip_mesh_peers",
        "Number of peers in the gossipsub mesh",
        &["client"],
        default_registry()
    ).expect("failed to create LEAN_GOSSIP_MESH_PEERS int gauge vec");

    // Tick Interval Duration Metrics
    pub static ref LEAN_TICK_INTERVAL_DURATION_SECONDS: HistogramVec = {
        let histogram_opts = HistogramOpts::new(
            "lean_tick_interval_duration_seconds",
            "Tracks elapsed time between clock ticks in seconds"
        ).buckets(vec![0.4, 0.6, 0.75, 0.8, 0.805, 0.81, 0.815, 0.82, 0.825, 0.85, 0.9, 1.0, 1.2, 1.6]);
        register_histogram_vec_with_registry!(
            histogram_opts,
            &[],
            default_registry()
        ).expect("failed to create LEAN_TICK_INTERVAL_DURATION_SECONDS histogram vec")
    };

    pub static ref LEAN_ATTESTATION_AGGREGATE_VALIDATORS: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_attestation_aggregate_coverage_validators",
        "Validator's is_aggregator status. True=1, False=0",
        &["section", "subnet"],
        default_registry()
    ).expect("failed to create IS_AGGREGATOR int gauge vec");

    pub static ref LEAN_ATTESTATION_AGGREGATE_SUBNETS: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_attestation_aggregate_coverage_subnets",
        "Validator's is_aggregator status. True=1, False=0",
        &["section"],
        default_registry()
    ).expect("failed to create IS_AGGREGATOR int gauge vec");

    pub static ref LEAN_ATTESTATION_AGGREGATE_DIFFERENT_VALIDATORS: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "lean_attestation_aggregate_coverage_different_validators",
        "Validator's is_aggregator status. True=1, False=0",
        &["direction"],
        default_registry()
    ).expect("failed to create IS_AGGREGATOR int gauge vec");

    pub static ref LEAN_AGGREGATOR_SKIPPED_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_aggregator_skipped_total",
        "Total number of aggregation submissions skipped, labeled by reason",
        &["reason"],
        default_registry()
    ).expect("failed to create AGGREGATOR_SKIPPED_TOTAL int counter vec");
}

pub fn init_aggregate_coverage_metrics() {
    for &section in ATTESTATION_AGGREGATE_COVERAGE_SECTIONS {
        set_int_gauge_vec(&LEAN_ATTESTATION_AGGREGATE_SUBNETS, 0, &[section]);
        set_int_gauge_vec(
            &LEAN_ATTESTATION_AGGREGATE_VALIDATORS,
            0,
            &[section, "combined"],
        );
    }

    for &direction in ATTESTATION_AGGREGATE_COVERAGE_DIFFERENT_DIRECTIONS {
        set_int_gauge_vec(
            &LEAN_ATTESTATION_AGGREGATE_DIFFERENT_VALIDATORS,
            0,
            &[direction],
        );
    }
}

/// Set the value of a gauge metric
pub fn set_int_gauge_vec(gauge_vec: &IntGaugeVec, value: i64, label_values: &[&str]) {
    gauge_vec.with_label_values(label_values).set(value);
}

/// Start a timer for a histogram metric
pub fn start_timer(histogram_vec: &HistogramVec, label_values: &[&str]) -> HistogramTimer {
    histogram_vec.with_label_values(label_values).start_timer()
}

pub fn stop_timer(timer: HistogramTimer) {
    timer.observe_duration()
}

/// Start a timer for a histogram metric that discards the result on drop if
/// stop_timer_discard_on_drop is not called
pub fn start_timer_discard_on_drop(
    histogram_vec: &HistogramVec,
    label_values: &[&str],
) -> DiscardOnDropHistogramTimer {
    DiscardOnDropHistogramTimer::new(histogram_vec.with_label_values(label_values).clone())
}

pub fn stop_timer_discard_on_drop(timer: DiscardOnDropHistogramTimer) {
    timer.observe_duration()
}

/// Increment a counter metric
pub fn inc_int_counter_vec(counter_vec: &IntCounterVec, label_values: &[&str]) {
    counter_vec.with_label_values(label_values).inc();
}

/// Increment a counter metric by a given amount
pub fn inc_int_counter_vec_by(counter_vec: &IntCounterVec, amount: u64, label_values: &[&str]) {
    counter_vec.with_label_values(label_values).inc_by(amount);
}

/// Observe a value on a histogram metric
pub fn observe_histogram_vec(histogram_vec: &HistogramVec, value: f64, label_values: &[&str]) {
    histogram_vec.with_label_values(label_values).observe(value);
}
