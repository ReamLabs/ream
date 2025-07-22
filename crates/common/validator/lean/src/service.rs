use std::{thread::sleep, time::Duration};

use tracing::info;

pub struct ValidatorService {}

impl ValidatorService {
    pub async fn new() -> Self {
        ValidatorService {}
    }

    pub async fn start(self) {
        info!("Validator Service started");

        loop {
            sleep(Duration::from_secs(10));
        }
    }
}
