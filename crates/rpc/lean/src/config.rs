use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone)]
pub struct LeanRpcServerConfig {
    pub http_socket_address: SocketAddr,
    pub http_allow_origin: bool,
}

impl LeanRpcServerConfig {
    pub fn new(http_address: IpAddr, http_port: u16, http_allow_origin: bool) -> Self {
        Self {
            http_socket_address: SocketAddr::new(http_address, http_port),
            http_allow_origin,
        }
    }
}
