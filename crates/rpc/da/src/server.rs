use std::{io::Result, sync::Arc};

use ream_da::store::DaReadStore;
use ream_da_node::ingest::DaIngestHandle;
use ream_rpc_common::{config::RpcServerConfig, server::RpcServerBuilder};

use crate::routes::register_routers;

/// Start the DA API server.
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
