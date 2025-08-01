use tracing::info;

pub struct NetworkService {
    pub time: u64,
}

impl NetworkService {
    pub async fn new() -> Self {
        NetworkService { time: 0 }
    }

    pub async fn start(self) {
        info!("NetworkService started");

        loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }
}
