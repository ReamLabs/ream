use std::sync::Arc;

use crate::networks::{NetworkSpec, HOLESKY, MAINNET, SEPOLIA};

pub fn network_parser(network_string: &str) -> Result<Arc<NetworkSpec>, String> {
    match network_string {
        "mainnet" => Ok(MAINNET.clone()),
        "holesky" => Ok(HOLESKY.clone()),
        "sepolia" => Ok(SEPOLIA.clone()),
        _ => Err(format!(
            "Not a valid network: {network_string}, try mainnet, holesky, or sepolia"
        )),
    }
}
