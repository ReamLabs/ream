use tracing::info;

pub struct NetworkService {}

impl NetworkService {
    pub async fn new() -> Self {
        NetworkService {}
    }

    pub async fn start(self) {
        info!("NetworkService started");

        loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }
}
