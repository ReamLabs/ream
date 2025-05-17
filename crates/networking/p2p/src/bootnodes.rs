use std::str::FromStr;

use anyhow::anyhow;
use discv5::Enr;
use ream_network_spec::networks::Network;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Bootnodes {
    #[default]
    Default,
    None,
    Custom(Vec<Enr>),
}

impl FromStr for Bootnodes {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "default" => Ok(Bootnodes::Default),
            "none" => Ok(Bootnodes::None),
            _ => s
                .split(',')
                .map(Enr::from_str)
                .collect::<Result<Vec<_>, String>>()
                .map(Bootnodes::Custom)
                .map_err(|err| anyhow!("Failed to parse bootnodes: {err:?}")),
        }
    }
}

impl Bootnodes {
    pub fn to_enrs(self, network: Network) -> Vec<Enr> {
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
            Network::Dev | Network::Custom(_) => vec![],
        };

        match self {
            Bootnodes::Default => bootnodes,
            Bootnodes::None => vec![],
            Bootnodes::Custom(bootnodes) => bootnodes,
        }
    }
}
