pub mod timer;

use prometheus_exporter::prometheus::{
    HistogramTimer, HistogramVec, IntCounterVec, IntGaugeVec, default_registry,
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry,
};

use crate::timer::DiscardOnDropHistogramTimer;

// Provisioning each metrics
lazy_static::lazy_static! {
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
    pub static ref FORK_CHOICE_BLOCK_PROCESSING_TIME: HistogramVec = register_histogram_vec_with_registry!(
        "lean_fork_choice_block_processing_time_seconds",
        "Time taken to process block",
        &[],
        default_registry()
    ).expect("failed to create FORK_CHOICE_BLOCK_PROCESSING_TIME histogram vec");

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

    pub static ref ATTESTATION_VALIDATION_TIME: HistogramVec = register_histogram_vec_with_registry!(
        "lean_attestation_validation_time_seconds",
        "Time taken to validate attestation",
        &[],
        default_registry()
    ).expect("failed to create ATTESTATION_VALIDATION_TIME histogram vec");

    // State Transition Metrics
    pub static ref STATE_TRANSITION_TIME: HistogramVec = register_histogram_vec_with_registry!(
        "lean_state_transition_time_seconds",
        "Time taken to process state transition",
        &[],
        default_registry()
    ).expect("failed to create STATE_TRANSITION_TIME histogram vec");

    pub static ref STATE_TRANSITION_BLOCK_PROCESSING_TIME: HistogramVec = register_histogram_vec_with_registry!(
        "lean_state_transition_block_processing_time_seconds",
        "Time taken to process block in state transition",
        &[],
        default_registry()
    ).expect("failed to create STATE_TRANSITION_BLOCK_PROCESSING_TIME histogram vec");

    pub static ref STATE_TRANSITION_SLOTS_PROCESSED_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_state_transition_slots_processed_total",
        "Total number of slots processed in state transition",
        &[],
        default_registry()
    ).expect("failed to create STATE_TRANSITION_SLOTS_PROCESSED_TOTAL int counter vec");

    pub static ref STATE_TRANSITION_SLOTS_PROCESSING_TIME: HistogramVec = register_histogram_vec_with_registry!(
        "lean_state_transition_slots_processing_time_seconds",
        "Time taken to process slots in state transition",
        &[],
        default_registry()
    ).expect("failed to create STATE_TRANSITION_SLOTS_PROCESSING_TIME histogram vec");

    pub static ref STATE_TRANSITION_ATTESTATIONS_PROCESSED_TOTAL: IntCounterVec = register_int_counter_vec_with_registry!(
        "lean_state_transition_attestations_processed_total",
        "Total number of attestations processed in state transition",
        &[],
        default_registry()
    ).expect("failed to create STATE_TRANSITION_ATTESTATIONS_PROCESSED_TOTAL int counter vec");

    pub static ref STATE_TRANSITION_ATTESTATIONS_PROCESSING_TIME: HistogramVec = register_histogram_vec_with_registry!(
        "lean_state_transition_attestations_processing_time_seconds",
        "Time taken to process attestations in state transition",
        &[],
        default_registry()
    ).expect("failed to create STATE_TRANSITION_ATTESTATIONS_PROCESSING_TIME histogram vec");
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
