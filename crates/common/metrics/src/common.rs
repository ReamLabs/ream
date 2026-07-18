use prometheus_exporter::prometheus::{
    Histogram, HistogramTimer, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec,
};

use crate::timer::DiscardOnDropHistogramTimer;

/// Set the value of a gauge metric (no labels)
pub fn set_int_gauge(gauge: &IntGauge, value: i64) {
    gauge.set(value);
}

/// Set the value of a gauge metric
pub fn set_int_gauge_vec(gauge_vec: &IntGaugeVec, value: i64, label_values: &[&str]) {
    gauge_vec.with_label_values(label_values).set(value);
}

/// Start a timer for a histogram metric (no labels)
pub fn start_timer_plain(histogram: &Histogram) -> HistogramTimer {
    histogram.start_timer()
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

/// Increment a counter metric (no labels)
pub fn inc_int_counter(counter: &IntCounter) {
    counter.inc();
}

/// Increment a counter metric by a given amount (no labels)
pub fn inc_int_counter_by(counter: &IntCounter, amount: u64) {
    counter.inc_by(amount);
}

/// Increment a counter metric
pub fn inc_int_counter_vec(counter_vec: &IntCounterVec, label_values: &[&str]) {
    counter_vec.with_label_values(label_values).inc();
}

/// Increment a counter metric by a given amount
pub fn inc_int_counter_vec_by(counter_vec: &IntCounterVec, amount: u64, label_values: &[&str]) {
    counter_vec.with_label_values(label_values).inc_by(amount);
}

/// Observe a value on a histogram metric (no labels)
pub fn observe_histogram(histogram: &Histogram, value: f64) {
    histogram.observe(value);
}

/// Observe a value on a histogram metric
pub fn observe_histogram_vec(histogram_vec: &HistogramVec, value: f64, label_values: &[&str]) {
    histogram_vec.with_label_values(label_values).observe(value);
}
