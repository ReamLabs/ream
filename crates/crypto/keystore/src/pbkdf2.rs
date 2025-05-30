use alloy_primitives::B256;

use crate::hmac::hmac_sha_256;

// Based on https://www.ietf.org/rfc/rfc2898.txt
fn pbkdf2_helper(password: &[u8], salt: &[u8], iterations: u32, index: u32) -> B256 {
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

pub fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32, dk_len: u32) -> Vec<u8> {
    let num_blocks = (dk_len as f64 / 32.0).ceil() as u32;
    let last_block_size = dk_len - (num_blocks - 1) * 32;

    let mut derived_key = Vec::with_capacity(dk_len as usize);

    for block_index in 1..=num_blocks {
        let block = pbkdf2_helper(password, salt, iterations, block_index);
        if block_index == num_blocks {
            derived_key.extend_from_slice(&block[..last_block_size as usize]);
        } else {
            derived_key.extend_from_slice(block.as_ref());
        }
    }

    derived_key
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
        let dk_len = 32;

        let derived_key = pbkdf2(password, salt, c, dk_len);
        let expected_key = hex!("348c89dbcbd32b2f32d814b8116e84cf2b17347ebc1800181c4e2a1fb8dd53e1");

        assert_eq!(derived_key, expected_key);
    }
}
