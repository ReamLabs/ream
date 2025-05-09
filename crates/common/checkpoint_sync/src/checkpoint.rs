use ream_network_spec::networks::{Network, network_spec};
use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub network: String,
    pub checkpoint_urls: Vec<String>,
}

pub fn get_checkpoint_sync_sources(checkpoint_sync_url: Option<Url>) -> Vec<Url> {
    if let Some(checkpoint_sync_url) = checkpoint_sync_url {
        return vec![checkpoint_sync_url];
    }
    let raw_urls: Vec<String> = match network_spec().network {
        Network::Mainnet => serde_yaml::from_str(include_str!(
            "../resources/checkpoint_sync_sources/mainnet.yaml"
        ))
        .expect("should deserialize checkpoint sync sources"),
        Network::Holesky => serde_yaml::from_str(include_str!(
            "../resources/checkpoint_sync_sources/holesky.yaml"
        ))
        .expect("should deserialize checkpoint sync sources"),
        Network::Sepolia => serde_yaml::from_str(include_str!(
            "../resources/checkpoint_sync_sources/sepolia.yaml"
        ))
        .expect("should deserialize checkpoint sync sources"),
        Network::Hoodi => serde_yaml::from_str(include_str!(
            "../resources/checkpoint_sync_sources/hoodi.yaml"
        ))
        .expect("should deserialize checkpoint sync sources"),
        Network::Dev => vec![],
    };

    raw_urls
        .into_iter()
        .map(|s| Url::parse(&s).expect("invalid URL in checkpoint sync YAML"))
        .collect()
}
