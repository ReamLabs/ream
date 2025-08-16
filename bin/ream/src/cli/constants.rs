use std::net::{IpAddr, Ipv4Addr};

pub const DEFAULT_BEACON_API_ENDPOINT: &str = "http://localhost:5052";
pub const DEFAULT_DISABLE_DISCOVERY: bool = false;
pub const DEFAULT_DISCOVERY_PORT: u16 = 9000;
pub const DEFAULT_HTTP_ADDRESS: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
pub const DEFAULT_HTTP_ALLOW_ORIGIN: bool = false;
pub const DEFAULT_HTTP_PORT: u16 = 5052;
pub const DEFAULT_KEY_MANAGER_HTTP_PORT: u16 = 8008;
pub const DEFAULT_METRICS_ENABLED: bool = false;
pub const DEFAULT_METRICS_ADDRESS: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
pub const DEFAULT_METRICS_PORT: u16 = 8080;
pub const DEFAULT_NETWORK: &str = "mainnet";
pub const DEFAULT_REQUEST_TIMEOUT: &str = "60";
pub const DEFAULT_SOCKET_ADDRESS: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
pub const DEFAULT_SOCKET_PORT: u16 = 9000;
