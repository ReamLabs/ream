pub mod helpers;

use helpers::{create_histogram_vec, create_int_gauge_vec};
use prometheus_exporter::prometheus::{HistogramVec, IntGaugeVec};

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
