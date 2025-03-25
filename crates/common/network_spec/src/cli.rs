use std::sync::Arc;

use crate::networks::{NetworkSpec, DEV, HOLESKY, HOODI, MAINNET, SEPOLIA};

pub fn network_parser(network_string: &str) -> Result<Arc<NetworkSpec>, String> {
    match network_string {
        "mainnet" => Ok(MAINNET.clone()),
        "holesky" => Ok(HOLESKY.clone()),
        "sepolia" => Ok(SEPOLIA.clone()),
        "hoodi" => Ok(HOODI.clone()),
        "dev" => Ok(DEV.clone()),
        _ => Err(format!(
            "Not a valid network: {network_string}, try mainnet, holesky, sepolia, hoodi, or dev"
        )),
    }
}
