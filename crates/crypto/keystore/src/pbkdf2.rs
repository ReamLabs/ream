use alloy_primitives::B256;

use crate::hmac::hmac_sha_256;

// Based on https://www.ietf.org/rfc/rfc2898.txt
fn pbkdf2_helper(password: &[u8], salt: &[u8], c: u32, i: u32) -> B256 {
    let mut u = hmac_sha_256(password, &[salt, &i.to_be_bytes()].concat());
    let mut block = u;

    for _ in 1..c {
        u = hmac_sha_256(password, u.as_ref());
        block
            .iter_mut()
            .zip(u.iter())
            .for_each(|(b, u_b)| *b ^= u_b);
    }
    block
}

pub fn pbkdf2(password: &[u8], salt: &[u8], c: u32, dk_len: u32) -> Vec<u8> {
    let l = (dk_len as f64 / 32.0).ceil() as u32;
    let r = dk_len - (l - 1) * 32;

    let mut derived_key = Vec::with_capacity(dk_len as usize);

    for i in 1..=l {
        let block = pbkdf2_helper(password, salt, c, i);
        if i == l {
            derived_key.extend_from_slice(&block[..r as usize]);
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
