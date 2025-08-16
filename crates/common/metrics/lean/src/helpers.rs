use prometheus_exporter::prometheus::{
    HistogramTimer, HistogramVec, IntGaugeVec, default_registry,
    register_histogram_vec_with_registry, register_int_gauge_vec_with_registry,
};

pub fn create_int_gauge_vec(name: &str, help: &str, label_names: &[&str]) -> IntGaugeVec {
    let registry = default_registry();
    register_int_gauge_vec_with_registry!(name, help, label_names, registry)
        .expect("failed to create int gauge vec")
}

pub fn set_int_gauge_vec(gauge_vec: &IntGaugeVec, value: i64, label_values: &[&str]) {
    gauge_vec.with_label_values(label_values).set(value);
}

pub fn create_histogram_vec(name: &str, help: &str, label_names: &[&str]) -> HistogramVec {
    let registry = default_registry();
    register_histogram_vec_with_registry!(name, help, label_names, registry)
        .expect("failed to create histogram")
}

pub fn start_timer_vec(histogram_vec: &HistogramVec, label_values: &[&str]) -> HistogramTimer {
    histogram_vec.with_label_values(label_values).start_timer()
}

pub fn stop_timer(timer: HistogramTimer) {
    timer.observe_duration()
}
