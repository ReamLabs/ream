use discv5::Enr;

use crate::util::ListenAddress;

pub struct NetworkConfig {
    #[serde(skip)]
    pub discv5_config: discv5::Config,

    pub boot_nodes_enr: Vec<Enr>,

    pub disable_discovery: bool,

    pub(crate) listen_addresses: ListenAddress,

    pub total_peers: usize,
}
