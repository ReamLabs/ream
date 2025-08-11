use std::str::FromStr;

use anyhow::anyhow;
use discv5::{Enr, multiaddr::Protocol};
use libp2p::Multiaddr;
use ream_network_spec::networks::Network;

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

                if let Ok(addrs) = s
                    .split(',')
                    .map(Multiaddr::from_str)
                    .collect::<Result<Vec<_>, _>>()
                {
                    return Ok(Bootnodes::Multiaddr(addrs));
                }

                Err(anyhow!("Failed to parse bootnodes as ENR or Multiaddr"))
            }
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
            Bootnodes::Multiaddr(_) => vec![],
        }
    }

    pub fn get_static_lean_peers(&self) -> Vec<Multiaddr> {
        match self {
            Bootnodes::Default => {
                serde_yaml::from_str(include_str!("../resources/lean_peers.yaml"))
                    .expect("should deserialize static lean peers")
            }
            Bootnodes::None => vec![],
            Bootnodes::Custom(enrs) => {
                let mut static_peers: Vec<Enr> =
                    serde_yaml::from_str(include_str!("../resources/lean_peers.yaml"))
                        .expect("should deserialize static lean peers");
                static_peers.extend(enrs.clone());
                Self::to_multiaddrs(static_peers)
            }
            Bootnodes::Multiaddr(multiaddrs) => {
                let mut static_peers: Vec<Multiaddr> =
                    serde_yaml::from_str(include_str!("../resources/lean_peers.yaml"))
                        .expect("should deserialize static lean peers");
                static_peers.extend(multiaddrs.clone());
                static_peers
            }
        }
    }

    pub fn to_multiaddrs(enrs: Vec<Enr>) -> Vec<Multiaddr> {
        let mut multiaddrs: Vec<Multiaddr> = Vec::new();
        for enr in enrs {
            if let Some(peer_id) = crate::network::beacon::Network::peer_id_from_enr(&enr) {
                if let Some(ip) = enr.ip4()
                    && let Some(tcp) = enr.tcp4()
                {
                    let mut multiaddr: Multiaddr = ip.into();
                    multiaddr.push(Protocol::Tcp(tcp));
                    multiaddr.push(Protocol::P2p(peer_id));
                    multiaddrs.push(multiaddr);
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
}
