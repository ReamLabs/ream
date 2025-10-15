use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

/// YAML structure for node-based validator mapping
/// Example:
/// ```yaml
/// zeam_0:
///     - 2
///     - 5
///     - 8
/// ream_0:
///     - 0
///     - 3
///     - 6
/// ```
#[derive(Debug, Deserialize, Serialize)]
pub struct NodeValidatorMapping {
    #[serde(flatten)]
    pub nodes: HashMap<String, Vec<u64>>,
}

// TODO: We need to replace this after PQC integration.
// For now, we only need ID for keystore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeanKeystore {
    pub validator_id: u64,
}

/// Load validator registry from YAML file for a specific node
///
/// # Arguments
/// * `path` - Path to the validator registry YAML file
/// * `node_id` - Node identifier (e.g., "ream_0", "zeam_0")
pub fn load_validator_registry<P: AsRef<Path> + std::fmt::Debug>(
    path: P,
    node_id: &str,
) -> anyhow::Result<Vec<LeanKeystore>> {
    let content = fs::read_to_string(&path).map_err(|err| {
        anyhow::anyhow!("Failed to read validator registry file {path:?}: {err}",)
    })?;

    let node_mapping = serde_yaml::from_str::<NodeValidatorMapping>(&content)
        .map_err(|err| anyhow::anyhow!("Failed to parse validator registry YAML: {err}"))?;

    Ok(node_mapping
        .nodes
        .get(node_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Node ID '{node_id}' not found in registry. Available nodes: {:?}",
                node_mapping.nodes.keys().collect::<Vec<_>>()
            )
        })?
        .iter()
        .map(|&id| LeanKeystore { validator_id: id })
        .collect())
}
