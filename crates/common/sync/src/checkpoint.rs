use std::{error::Error, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub network: String,
    pub checkpoint_urls: Vec<String>,
}

pub fn fetch_default_checkpoint_url() -> Result<Vec<NetworkConfig>, Box<dyn Error>> {
    let yaml_file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let yaml_file_path = format!(
        "{}{}",
        yaml_file_path.display(),
        "/src/assets/checkpoint.yaml"
    );
    let yaml_content = fs::read_to_string(yaml_file_path)?;

    let config: Vec<NetworkConfig> = serde_yaml::from_str(&yaml_content)?;

    Ok(config)
}
