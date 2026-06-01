use std::sync::OnceLock;

static BLOB_SCHEDULE: OnceLock<Vec<(u64, u64)>> = OnceLock::new();

pub fn set_blob_schedule(schedule: Vec<(u64, u64)>) {
    BLOB_SCHEDULE
        .set(schedule)
        .expect("BLOB_SCHEDULE should only be set once");
}
