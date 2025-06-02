use std::{collections::HashMap, sync::Arc, time::Duration};

use actix_web::{App, HttpServer, dev::ServerHandle, middleware, web::Data};
use config::RpcServerConfig;
use ream_discv5::subnet::SyncCommitteeSubnets;
use ream_p2p::network_state::NetworkState;
use ream_storage::db::ReamDB;
use tokio::sync::RwLock;
use tracing::info;

use crate::{handlers::validator::SyncCommitteeSubscriptionMap, routes::register_routers};

pub mod config;
pub mod handlers;
pub mod routes;

/// Spawns a background task to expire sync committee subscriptions.
pub fn spawn_sync_committee_expiry_task(
    sync_committee_subscriptions: SyncCommitteeSubscriptionMap,
    sync_committee_subnets: Arc<RwLock<SyncCommitteeSubnets>>,
    db: ReamDB,
) {
    tokio::spawn(async move {
        loop {
            // Fetch the current epoch from the latest state in the DB
            let current_epoch = match get_current_epoch_from_db(&db).await {
                Ok(epoch) => epoch,
                Err(_) => {
                    // If we can't get the epoch, skip this round
                    tokio::time::sleep(Duration::from_secs(12 * 32)).await;
                    continue;
                }
            };
            let mut map = sync_committee_subscriptions.write().await;
            let expired: Vec<u8> = map
                .iter()
                .filter_map(|(&subnet_id, &until_epoch)| {
                    if until_epoch <= current_epoch {
                        Some(subnet_id)
                    } else {
                        None
                    }
                })
                .collect();
            if !expired.is_empty() {
                let mut subnets = sync_committee_subnets.write().await;
                for subnet_id in &expired {
                    if let Err(e) = subnets.disable_sync_committee_subnet(*subnet_id) {
                        tracing::error!(
                            "Failed to disable sync committee subnet {}: {}",
                            subnet_id,
                            e
                        );
                    }
                    map.remove(subnet_id);
                }

                if !expired.is_empty() {
                    tracing::info!(
                        "Marked that ENR needs to be updated after sync committee subnet expiry"
                    );
                }
            }
            tokio::time::sleep(Duration::from_secs(12 * 32)).await; // One epoch (customize as needed)
        }
    });
}

async fn get_current_epoch_from_db(db: &ReamDB) -> Result<u64, ()> {
    use ream_beacon_api_types::id::ID;

    use crate::handlers::state::get_state_from_id;
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|_| ())?
        .ok_or(())?;
    let state = get_state_from_id(ID::Slot(highest_slot), db)
        .await
        .map_err(|_| ())?;
    Ok(state.get_current_epoch())
}

/// Start the Beacon API server.
pub async fn start_server(
    server_config: RpcServerConfig,
    db: ReamDB,
    network_state: Arc<NetworkState>,
) -> std::io::Result<()> {
    info!(
        "starting HTTP server on {:?}",
        server_config.http_socket_address
    );
    // create the stop handle container
    let stop_handle = Data::new(StopHandle::default());

    let sync_committee_subscriptions = Arc::new(RwLock::new(HashMap::new()));
    let sync_committee_subnets = Arc::new(RwLock::new(SyncCommitteeSubnets::new()));
    spawn_sync_committee_expiry_task(
        sync_committee_subscriptions.clone(),
        sync_committee_subnets.clone(),
        db.clone(),
    );

    let server = HttpServer::new(move || {
        let stop_handle = stop_handle.clone();
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(stop_handle)
            .app_data(Data::new(db.clone()))
            .app_data(Data::new(network_state.clone()))
            .app_data(Data::new(sync_committee_subscriptions.clone()))
            .app_data(Data::new(sync_committee_subnets.clone()))
            .configure(register_routers)
    })
    .bind(server_config.http_socket_address)?
    .run();

    server.await
}

#[derive(Default)]
struct StopHandle {
    inner: parking_lot::Mutex<Option<ServerHandle>>,
}

#[allow(dead_code)]
impl StopHandle {
    /// Sets the server handle to stop.
    pub(crate) fn register(&self, handle: ServerHandle) {
        *self.inner.lock() = Some(handle);
    }

    /// Sends stop signal through contained server handle.
    pub(crate) fn stop(&self, graceful: bool) {
        #[allow(clippy::let_underscore_future)]
        let _ = self.inner.lock().as_ref().unwrap().stop(graceful);
    }
}
