use std::{
    fs::{read_dir, read_to_string},
    path::PathBuf,
};

use anyhow::{anyhow, bail};
use ream_keystore::keystore::EncryptedKeystore;
use unicode_normalization::UnicodeNormalization;

pub fn load_password_file(path: &PathBuf) -> anyhow::Result<String> {
    let contents =
        read_to_string(path).map_err(|err| anyhow!("Unable to load password file: {err:?}"))?;
    Ok(contents.trim_end_matches(&['\n', '\r'][..]).to_string())
}

pub fn load_keystore_directory(config: &PathBuf) -> anyhow::Result<Vec<EncryptedKeystore>> {
    Ok(read_dir(config)
        .map_err(|err| anyhow!("Failed to read directory {}: {err:?}", config.display()))?
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

pub fn load_password_from_config(
    password_file: Option<&PathBuf>,
    password: Option<String>,
) -> anyhow::Result<String> {
    if let Some(password_file) = password_file {
        load_password_file(password_file)
    } else if let Some(password_str) = password {
        Ok(password_str)
    } else {
        bail!("Expected either password or password-file to be set")
    }
}

pub fn process_password(password: String) -> String {
    password
        .nfkd()
        .filter(|&character_unprocessed| {
            let character = character_unprocessed as u32;
            !((character == 0x7F) || (character <= 0x1F) || (0x80..=0x9F).contains(&character))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use alloy_primitives::hex;

    use super::*;

    #[test]
    fn test_process_password() {
        let original = "𝔱𝔢𝔰𝔱𝔭𝔞𝔰𝔰𝔴𝔬𝔯𝔡🔑".to_string();
        let processed = process_password(original);

        let expected = hex!("0x7465737470617373776f7264f09f9491");
        assert_eq!(expected, processed.into_bytes().as_slice());
    }
}
