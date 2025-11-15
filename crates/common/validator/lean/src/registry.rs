use std::{fs, path::Path};

use ream_keystore::lean_keystore::{ValidatorKeystore, ValidatorRegistry};

/// Load validator registry from YAML file for a specific node
///
/// # Arguments
/// * `path` - Path to the validator registry YAML file
/// * `node_id` - Node identifier (e.g., "ream_0", "zeam_0")
pub fn load_validator_registry<P: AsRef<Path> + std::fmt::Debug>(
    path: P,
    node_id: &str,
) -> anyhow::Result<Vec<ValidatorKeystore>> {
    let content = fs::read_to_string(&path).map_err(|err| {
        anyhow::anyhow!("Failed to read validator registry file {path:?}: {err}",)
    })?;

    let mut node_mapping = serde_yaml::from_str::<ValidatorRegistry>(&content)
        .map_err(|err| anyhow::anyhow!("Failed to parse validator registry YAML: {err}"))?;

    node_mapping.nodes.remove(node_id).ok_or_else(|| {
        anyhow::anyhow!(
            "Node ID '{node_id}' not found in registry. Available nodes: {:?}",
            node_mapping.nodes.keys().collect::<Vec<_>>()
        )
    })
}
