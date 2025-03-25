use discv5::Enr;
use ream_network_spec::networks::Network;

pub struct Bootnodes {
    pub bootnodes: Vec<Enr>,
}

impl Bootnodes {
    pub fn new(network: Network) -> Self {
        let bootnodes: Vec<Enr> = match network {
            Network::Mainnet => {
                serde_yaml::from_str(include_str!("../resources/bootnodes_mainnet.yaml"))
                    .expect("should deserialize bootnodes")
            }
            Network::Holesky => {
                serde_yaml::from_str(include_str!("../resources/bootnodes_holesky.yaml"))
                    .expect("should deserialize bootnodes")
            }
            Network::Sepolia => {
                serde_yaml::from_str(include_str!("../resources/bootnodes_sepolia.yaml"))
                    .expect("should deserialize bootnodes")
            }
            Network::Hoodi => {
                serde_yaml::from_str(include_str!("../resources/bootnodes_hoodi.yaml"))
                    .expect("should deserialize bootnodes")
            }
            Network::Dev => vec![],
        };
        Self { bootnodes }
    }
}
