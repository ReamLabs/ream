use std::{net::IpAddr, path::PathBuf};

use ream_p2p::bootnodes::Bootnodes;
use url::Url;

pub struct ManagerConfig {
    pub http_address: IpAddr,
    pub http_port: u16,
    pub http_allow_origin: bool,
    pub socket_address: IpAddr,
    pub socket_port: u16,
    pub discovery_port: u16,
    pub disable_discovery: bool,
    pub data_dir: Option<PathBuf>,
    pub ephemeral: bool,
    pub bootnodes: Bootnodes,
    pub checkpoint_sync_url: Option<Url>,
    pub purge_db: bool,
    pub execution_endpoint: Option<Url>,
    pub execution_jwt_secret: Option<PathBuf>,
}
