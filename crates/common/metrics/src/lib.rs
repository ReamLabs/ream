pub mod timer;

use prometheus_exporter::prometheus::{
    HistogramTimer, HistogramVec, IntGaugeVec, default_registry,
    register_histogram_vec_with_registry, register_int_gauge_vec_with_registry,
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
