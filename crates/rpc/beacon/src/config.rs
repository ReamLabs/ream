use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone)]
pub struct RpcServerConfig {
    pub http_socket_address: SocketAddr,
    pub http_allow_origin: bool,
}

impl RpcServerConfig {
    /// Creates a new instance from CLI arguments
    pub fn new(http_address: IpAddr, http_port: u16, http_allow_origin: bool) -> Self {
        Self {
            http_socket_address: SocketAddr::new(http_address, http_port),
            http_allow_origin,
        }
    }
}
