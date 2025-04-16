use std::sync::{Arc, mpsc};

use actix_web::{
    App, HttpServer,
    dev::{Server, ServerHandle},
    middleware, web,
};
use config::ServerConfig;
use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use routes::get_routes;
use tracing::info;
use utils::error::handle_rejection;
use warp::{Filter, serve};

use crate::routes::register_routers;

pub mod config;
pub mod handlers;
pub mod routes;
pub mod types;
pub mod utils;

/// Start the Beacon API server.
pub async fn start_server(network_spec: Arc<NetworkSpec>, server_config: ServerConfig, db: ReamDB) {
    let routes = get_routes(network_spec, db).recover(handle_rejection);

    info!("Starting server on {:?}", server_config.http_socket_address);
    serve(routes).run(server_config.http_socket_address).await;
}

pub async fn run_app(
    network_spec: Arc<NetworkSpec>,
    server_config: ServerConfig,
    db: ReamDB,
) -> std::io::Result<()> {
    info!(
        "starting HTTP server at {:?}",
        server_config.http_socket_address
    );
    // create the stop handle container
    let stop_handle = web::Data::new(StopHandle::default());

    // srv is server controller type, `dev::Server`
    let server = HttpServer::new(move || {
        let stop_handle = stop_handle.clone();
        App::new()
            // enable logger
            .app_data(stop_handle)
            .app_data(web::Data::new(db.clone()))
            .wrap(middleware::Logger::default())
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
