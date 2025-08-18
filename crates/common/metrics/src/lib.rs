use prometheus_exporter::prometheus::{
    HistogramTimer, HistogramVec, IntGaugeVec, default_registry,
    register_histogram_vec_with_registry, register_int_gauge_vec_with_registry,
};

// Provisioning each metrics
lazy_static::lazy_static! {
    pub static ref PROPOSE_BLOCK_TIME: HistogramVec = create_histogram_vec(
        "lean_propose_block_time",
        "Duration of the sections it takes to propose a new block",
        &["section"]
    );

    pub static ref HEAD_SLOT: IntGaugeVec = create_int_gauge_vec(
        "lean_head_slot",
        "The current head slot",
        &[]
    );

    pub static ref JUSTIFIED_SLOT: IntGaugeVec = create_int_gauge_vec(
        "lean_justified_slot",
        "The current justified slot",
        &[]
    );

    pub static ref FINALIZED_SLOT: IntGaugeVec = create_int_gauge_vec(
        "lean_finalized_slot",
        "The current finalized slot",
        &[]
    );
}

/// Create a new gauge metric
pub fn create_int_gauge_vec(name: &str, help: &str, label_names: &[&str]) -> IntGaugeVec {
    let registry = default_registry();
    register_int_gauge_vec_with_registry!(name, help, label_names, registry)
        .expect("failed to create int gauge vec")
}

/// Set the value of a gauge metric
pub fn set_int_gauge_vec(gauge_vec: &IntGaugeVec, value: i64, label_values: &[&str]) {
    gauge_vec.with_label_values(label_values).set(value);
}

/// Create a new histogram metric
pub fn create_histogram_vec(name: &str, help: &str, label_names: &[&str]) -> HistogramVec {
    let registry = default_registry();
    register_histogram_vec_with_registry!(name, help, label_names, registry)
        .expect("failed to create histogram")
}

/// Start a timer for a histogram metric
pub fn start_timer_vec(histogram_vec: &HistogramVec, label_values: &[&str]) -> HistogramTimer {
    histogram_vec.with_label_values(label_values).start_timer()
}

/// Stop a timer for a histogram metric
pub fn stop_timer(timer: HistogramTimer) {
    timer.observe_duration()
}
