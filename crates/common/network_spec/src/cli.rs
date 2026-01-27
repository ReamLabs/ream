use std::{fs, sync::Arc};

use serde::de::DeserializeOwned;

use crate::networks::{BeaconNetworkSpec, DEV, HOODI, LeanNetworkSpec, MAINNET, SEPOLIA};

pub fn beacon_network_parser(network_string: &str) -> Result<Arc<BeaconNetworkSpec>, String> {
    match network_string {
        "mainnet" => Ok(MAINNET.clone()),
        "sepolia" => Ok(SEPOLIA.clone()),
        "hoodi" => Ok(HOODI.clone()),
        "dev" => Ok(DEV.clone()),
        path => read_network_spec(path).map(Arc::new),
    }
}

pub fn lean_network_parser(network_string: &str) -> Result<LeanNetworkSpec, String> {
    match network_string {
        "ephemery" => Ok(LeanNetworkSpec::ephemery()),
        path => read_network_spec(path),
    }
}

fn read_network_spec<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let contents = fs::read_to_string(path).map_err(|err| format!("Failed to read file: {err}"))?;
    serde_yaml::from_str(&contents).map_err(|err| format!("Failed to parse YAML from: {err}"))
}
