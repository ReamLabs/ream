use std::str::FromStr;

use anyhow::anyhow;
use discv5::Enr;
use libp2p::Multiaddr;
use ream_network_spec::networks::Network;

use crate::utils::to_multiaddrs;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Bootnodes {
    #[default]
    Default,
    None,
    Custom(Vec<Enr>),
    Multiaddr(Vec<Multiaddr>),
}

impl FromStr for Bootnodes {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "default" => Ok(Bootnodes::Default),
            "none" => Ok(Bootnodes::None),
            _ => {
                if let Ok(enrs) = s
                    .split(',')
                    .map(Enr::from_str)
                    .collect::<Result<Vec<_>, _>>()
                {
                    return Ok(Bootnodes::Custom(enrs));
                }

                if let Ok(addresses) = s
                    .split(',')
                    .map(Multiaddr::from_str)
                    .collect::<Result<Vec<_>, _>>()
                {
                    return Ok(Bootnodes::Multiaddr(addresses));
                }

                Err(anyhow!("Failed to parse {s} as ENR or Multiaddr"))
            }
        }
    }
}

impl Bootnodes {
    pub fn to_enrs_beacon(self, network: Network) -> Vec<Enr> {
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
            Bootnodes::Multiaddr(_) => vec![],
        }
    }

    pub fn to_multiaddrs_lean(&self) -> Vec<Multiaddr> {
        match self {
            Bootnodes::Default => {
                serde_yaml::from_str(include_str!("../resources/lean_peers.yaml"))
                    .expect("should deserialize static lean peers")
            }
            Bootnodes::None => vec![],
            Bootnodes::Custom(enrs) => to_multiaddrs(enrs),
            Bootnodes::Multiaddr(multiaddrs) => multiaddrs.to_vec(),
        }
    }
}
