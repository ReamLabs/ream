use std::{io::Result, net::SocketAddr, sync::Arc};

use actix_web::{
    App, HttpServer,
    middleware::Logger,
    web::{Data, ServiceConfig},
};
use tracing::info;

/// A type alias for a function that configures the actix-web ServiceConfig.
type Configurator = dyn Fn(&mut ServiceConfig) + Send + Sync;

/// A builder for configuring and starting an RPC server.
pub struct RpcServerBuilder {
    http_socket_address: SocketAddr,
    http_allow_origin: bool,
    configurators: Vec<Arc<Configurator>>,
}

impl RpcServerBuilder {
    /// Create a new RpcServerBuilder with the given configuration.
    pub fn new(http_socket_address: SocketAddr) -> Self {
        Self {
            http_socket_address,
            http_allow_origin: false,
            configurators: Vec::new(),
        }
    }

    /// Set whether to allow CORS for all origins.
    pub fn allow_origin(mut self, allow: bool) -> Self {
        self.http_allow_origin = allow;
        self
    }

    /// Configure actix-web App by providing a closure that takes a mutable ref ServiceConfig.
    pub fn configure<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut ServiceConfig) + Send + Sync + 'static,
    {
        self.configurators.push(Arc::new(f));
        self
    }

    /// Add app data to the ServiceConfig.
    pub fn with_data<T>(mut self, value: T) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        self.configurators
            .push(Arc::new(move |config: &mut ServiceConfig| {
                config.app_data(Data::new(value.clone()));
            }));
        self
    }

    /// Start the RPC server by applying all configurations.
    pub async fn start(self) -> Result<()> {
        let configurators = self.configurators.clone();
        let configure_all = move |config: &mut ServiceConfig| {
            for configurator in &configurators {
                configurator(config);
            }
        };

        info!("starting HTTP server on {:?}", self.http_socket_address);

        let server = HttpServer::new(move || {
            App::new()
                .wrap(Logger::default())
                .configure(configure_all.clone())
        })
        .bind(self.http_socket_address)?
        .run();

        server.await
    }
}
