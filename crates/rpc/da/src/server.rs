use std::{io::Result, sync::Arc};

use ream_da::store::DaReadStore;
use ream_da_node::ingest::DaIngestHandle;
use ream_rpc_common::{config::RpcServerConfig, server::RpcServerBuilder};

use crate::routes::register_routers;

/// Start the DA API server.
///
/// The handlers are given two pieces of shared state, registered with
/// [`RpcServerBuilder::with_data`] so every request can extract them by type:
/// - `ingest_handle` (`Data<DaIngestHandle>`): the producer side of the verification queue, so
///   `/ingest` `/retention` can feed work into the pipeline.
/// - `store` (`Data<Arc<dyn DaReadStore>>`): a read-only store handle, for serving columns and
///   availability.
pub async fn start(
    server_config: RpcServerConfig,
    ingest_handle: DaIngestHandle,
    store: Arc<dyn DaReadStore>,
) -> Result<()> {
    RpcServerBuilder::new(server_config.http_socket_address)
        .allow_origin(server_config.http_allow_origin)
        .with_data(ingest_handle)
        .with_data(store)
        .configure(register_routers)
        .start()
        .await
}
