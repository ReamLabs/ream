use std::net::SocketAddr;

use actix_web::{App, HttpServer, dev::Server, middleware};
use tracing::info;

/// Starts a new RPC server with the given configuration.
pub fn start_rpc_server<F>(socket_addr: SocketAddr, configure_app: F) -> std::io::Result<Server>
where
    F: Fn(&mut actix_web::web::ServiceConfig) + Send + Clone + 'static,
{
    info!("starting HTTP server on {:?}", socket_addr);

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .configure(configure_app.clone())
    })
    .bind(socket_addr)?
    .run();

    Ok(server)
}
