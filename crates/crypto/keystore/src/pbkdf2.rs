use alloy_primitives::B256;
use anyhow::ensure;

use crate::hmac::hmac_sha_256;

// Based on https://www.ietf.org/rfc/rfc2898.txt
fn pbkdf2_helper(password: &[u8], salt: &[u8], iterations: u64, index: u32) -> B256 {
    let mut mac = hmac_sha_256(password, &[salt, &index.to_be_bytes()].concat());
    let mut block = mac;

    for _ in 1..iterations {
        mac = hmac_sha_256(password, mac.as_ref());
        block
            .iter_mut()
            .zip(mac.iter())
            .for_each(|(block_byte, mac_byte)| *block_byte ^= mac_byte);
    }
    block
}

pub fn pbkdf2(
    password: &[u8],
    salt: &[u8],
    iterations: u64,
    derived_key_length: u64,
) -> anyhow::Result<Vec<u8>> {
    ensure!(iterations > 0, "iteration count must be a positive integer");
    ensure!(
        derived_key_length > 0 && derived_key_length <= (u32::MAX as u64) * 32,
        "derived key length must be a positive integer and at most (2^32 - 1) * hLen"
    );
    let block_total = (derived_key_length as f64 / 32.0).ceil() as u64;
    let last_block_size = derived_key_length - (block_total - 1) * 32;

    let mut derived_key = Vec::with_capacity(derived_key_length as usize);

    for block_index in 1..=block_total {
        let block = pbkdf2_helper(password, salt, iterations, block_index as u32);
        if block_index == block_total {
            derived_key.extend_from_slice(&block[..last_block_size as usize]);
        } else {
            derived_key.extend_from_slice(block.as_ref());
        }
    }

    Ok(derived_key)
}

#[cfg(test)]
mod tests {
    use alloy_primitives::hex;

    use super::*;

    #[test]
    fn test_pbkdf2() {
        let password = b"passwordPASSWORDpassword";
        let salt = b"saltSALTsaltSALTsaltSALTsaltSALTsalt";
        let c = 4096;
        let derived_key_length = 32;

        let derived_key = pbkdf2(password, salt, c, derived_key_length).unwrap();
        let expected_key = hex!("348c89dbcbd32b2f32d814b8116e84cf2b17347ebc1800181c4e2a1fb8dd53e1");

        assert_eq!(derived_key, expected_key);
    }
}
