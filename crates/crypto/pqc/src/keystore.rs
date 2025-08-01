use std::{fs, path::Path};

use anyhow::{Result, anyhow};
use hmac::{Hmac, Mac};
use rand::{RngCore, thread_rng};
use serde::{Deserialize, Serialize};
use serde_json;
use sha2::Sha256;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keystore {
    pub public_key: Vec<u8>,
    pub private_key: Vec<u8>,
}

impl Keystore {
    /// using HMAC to verify the integrity of the keystore file
    /// to be replaced with a new EIP for PQ keystore format.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let encrypted_data = fs::read(path)?;

        // Extract HMAC key (first 32 bytes)
        if encrypted_data.len() < 64 {
            // 32 bytes key + 32 bytes HMAC + at least some data
            return Err(anyhow!("Invalid keystore file format"));
        }
        let hmac_key = &encrypted_data[0..32];
        let hmac_result = &encrypted_data[32..64];
        let json_data = &encrypted_data[64..];

        // Verify HMAC
        let mut mac = Hmac::<Sha256>::new_from_slice(hmac_key)
            .map_err(|e| anyhow!("HMAC key error: {}", e))?;
        mac.update(json_data);
        mac.verify_slice(hmac_result)
            .map_err(|_| anyhow!("HMAC verification failed - file may be corrupted"))?;

        // Parse JSON
        let keystore: Keystore = serde_json::from_slice(json_data)?;
        Ok(keystore)
    }

    /// using HMAC to verify the integrity of the keystore file
    /// to be replaced with a new EIP for PQ keystore format.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;

        // Generate a random HMAC key (32 bytes)
        let mut hmac_key = [0u8; 32];
        thread_rng().fill_bytes(&mut hmac_key);

        // Create HMAC and compute the MAC
        let mut mac = Hmac::<Sha256>::new_from_slice(&hmac_key)
            .map_err(|e| anyhow!("HMAC key error: {}", e))?;
        mac.update(json.as_bytes());
        let hmac_result = mac.finalize().into_bytes();

        // Combine HMAC key, HMAC result, and encrypted data
        let encrypted_data =
            [hmac_key.as_slice(), hmac_result.as_slice(), json.as_bytes()].concat();

        fs::write(path, encrypted_data)?;
        Ok(())
    }
}
