use std::sync::Arc;

use actix_web::{App, HttpServer, dev::ServerHandle, middleware, web::Data};
use config::LeanRpcServerConfig;
use ream_chain_lean::lean_chain::LeanChain;
use tokio::sync::RwLock;
use tracing::info;

use crate::routes::register_routers;

pub mod config;
pub mod handlers;
pub mod routes;

/// Start the Lean API server.
pub async fn start_lean_server(
    server_config: LeanRpcServerConfig,
    lean_chain: Arc<RwLock<LeanChain>>,
) -> std::io::Result<()> {
    info!(
        "starting HTTP server on {:?}",
        server_config.http_socket_address
    );
    let stop_handle = Data::new(StopHandle::default());

    let server = HttpServer::new(move || {
        let stop_handle = stop_handle.clone();
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(stop_handle)
            .app_data(Data::new(lean_chain.clone()))
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
    pub(crate) fn register(&self, handle: ServerHandle) {
        *self.inner.lock() = Some(handle);
    }

    pub(crate) fn stop(&self, graceful: bool) {
        #[allow(clippy::let_underscore_future)]
        let _ = self.inner.lock().as_ref().unwrap().stop(graceful);
    }
}
