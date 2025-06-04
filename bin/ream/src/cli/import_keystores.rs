use std::{
    fs::{read_dir, read_to_string},
    path::PathBuf,
};

use anyhow::anyhow;
use ream_keystore::keystore::EncryptedKeystore;

pub fn load_password_file(path: &PathBuf) -> anyhow::Result<String> {
    let contents = read_to_string(path)
        .map_err(|err| anyhow!(format!("Unable to load password file: {err:?}")))?;
    Ok(contents.trim_end_matches(&['\n', '\r'][..]).to_string())
}

pub fn load_keystore_directory(config: &PathBuf) -> anyhow::Result<Vec<EncryptedKeystore>> {
    Ok(read_dir(config)
        .map_err(|err| {
            anyhow!(format!(
                "Failed to read directory {}: {err:?}",
                config.display()
            ))
        })?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("json")
            {
                Some(EncryptedKeystore::load_from_file(path).ok()?)
            } else {
                None
            }
        })
        .collect::<Vec<_>>())
}
