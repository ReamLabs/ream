use std::str::FromStr;

use anyhow::anyhow;
use discv5::{Enr, multiaddr::Protocol};
use libp2p::Multiaddr;
use ream_network_spec::networks::Network;

use crate::{network::misc::peer_id_from_enr, utils::quic_from_enr};

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
                // Check if it's a file path ending with .yaml or .yml
                if (s.ends_with(".yaml") || s.ends_with(".yml")) && std::path::Path::new(s).exists()
                {
                    let yaml_content = std::fs::read_to_string(s)?;
                    return Ok(Bootnodes::Custom(
                        serde_yaml::from_str(&yaml_content)
                            .map_err(|err| anyhow!("Failed to deserialize ENRs from {s}: {err}"))?,
                    ));
                }

                // Try parsing as comma-separated ENRs
                if let Ok(enrs) = s
                    .split(',')
                    .map(Enr::from_str)
                    .collect::<Result<Vec<_>, _>>()
                {
                    return Ok(Bootnodes::Custom(enrs));
                }

                // Try parsing as comma-separated multiaddrs
                if let Ok(addresses) = s
                    .split(',')
                    .map(Multiaddr::from_str)
                    .collect::<Result<Vec<_>, _>>()
                {
                    return Ok(Bootnodes::Multiaddr(addresses));
                }

                Err(anyhow!(
                    "Failed to parse {s} as ENR, Multiaddr, or YAML file path"
                ))
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

pub fn to_multiaddrs(enrs: &[Enr]) -> Vec<Multiaddr> {
    let mut multiaddrs: Vec<Multiaddr> = Vec::new();
    for enr in enrs {
        if let Some(peer_id) = peer_id_from_enr(enr) {
            if let Some(ip) = enr.ip4() {
                if let Some(quic) = quic_from_enr(enr) {
                    let mut multiaddr: Multiaddr = ip.into();
                    multiaddr.push(Protocol::Udp(quic));
                    multiaddr.push(Protocol::QuicV1);
                    multiaddr.push(Protocol::P2p(peer_id));
                    multiaddrs.push(multiaddr);
                }

                if let Some(tcp) = enr.tcp4() {
                    let mut multiaddr: Multiaddr = ip.into();
                    multiaddr.push(Protocol::Tcp(tcp));
                    multiaddr.push(Protocol::P2p(peer_id));
                    multiaddrs.push(multiaddr);
                }
            }
            if let Some(ip6) = enr.ip6()
                && let Some(tcp6) = enr.tcp6()
            {
                let mut multiaddr: Multiaddr = ip6.into();
                multiaddr.push(Protocol::Tcp(tcp6));
                multiaddr.push(Protocol::P2p(peer_id));
                multiaddrs.push(multiaddr);
            }
        }
    }
    multiaddrs
}
