use std::net::{IpAddr, SocketAddr};

pub struct ServerConfig {
    pub http_socket_address: SocketAddr,
    pub http_allow_origin: bool,
}

impl ServerConfig {
    /// Creates a new instance from CLI arguments
    pub fn new(http_address: IpAddr, http_port: u16, http_allow_origin: bool) -> Self {
        Self {
            http_socket_address: SocketAddr::new(http_address, http_port),
            http_allow_origin,
        }
    }
}
