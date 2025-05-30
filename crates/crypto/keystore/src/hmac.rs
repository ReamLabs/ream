use alloy_primitives::{B256, B512};
use sha2::{Digest, Sha256, digest::crypto_common::BlockSizeUser};

// Going off of this
// https://en.wikipedia.org/wiki/HMAC#:~:text=In%20cryptography%2C%20an%20HMAC%20(sometimes,and%20a%20secret%20cryptographic%20key.
pub fn hmac_sha_256(key: &[u8], message: &[u8]) -> B256 {
    let block_sized_key = compute_block_sized_key_sha_256(key);

    let outer_padded_key = block_sized_key
        .iter()
        .map(|&b| b ^ 0x5c)
        .collect::<Vec<_>>();
    let inner_padded_key = block_sized_key
        .iter()
        .map(|&b| b ^ 0x36)
        .collect::<Vec<_>>();

    // Compute inner hash
    let mut inner_hasher = Sha256::new();
    inner_hasher.update(&inner_padded_key);
    inner_hasher.update(message);
    let inner_hash = inner_hasher.finalize();

    // Compute outer hash
    let mut outer_hasher = Sha256::new();
    outer_hasher.update(&outer_padded_key);
    outer_hasher.update(inner_hash);

    B256::from_slice(&outer_hasher.finalize())
}

fn compute_block_sized_key_sha_256(key: &[u8]) -> B512 {
    let block_size = Sha256::block_size();
    if key.len() > block_size {
        let mut hasher = Sha256::new();
        hasher.update(key);
        return B512::from_slice(&hasher.finalize());
    }
    let mut padded_key = vec![0u8; block_size];
    padded_key[..key.len()].copy_from_slice(key);
    B512::from_slice(&padded_key)
}

#[cfg(test)]
mod tests {
    use alloy_primitives::hex;

    use crate::hmac::hmac_sha_256;

    #[test]
    fn test_hmac_sha256() {
        let key = b"key";
        let message = b"The quick brown fox jumps over the lazy dog";
        let expected_hmac: [u8; 32] =
            hex::decode("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8")
                .unwrap()
                .try_into()
                .expect("Expected HMAC must be 32 bytes");

        let result = hmac_sha_256(key, message);
        assert_eq!(result.as_slice(), expected_hmac.as_slice());
    }

    #[test]
    fn test_hmac_sha256_long() {
        let key = b"a veryyyyyyyyyyyyyy loooooooooooooong keeeeeeeeeeeeeey";
        let message = b"The quick brown fox jumps over the lazy dog";
        let expected_hmac: [u8; 32] =
            hex::decode("21ceea730aaa96810456eda3ec6ea3dbec121fe232fa103c711fe53db365de88")
                .unwrap()
                .try_into()
                .expect("Expected HMAC must be 32 bytes");

        let result = hmac_sha_256(key, message);
        assert_eq!(result.as_slice(), expected_hmac.as_slice());
    }

    #[test]
    fn test_hmac_sha256_exact() {
        let key = b"quinquagintaquadringentilliardth";
        let message = b"The quick brown fox jumps over the lazy dog";
        let expected_hmac: [u8; 32] =
            hex::decode("f32363682e10c5cd1966701fffb18addaba376aa307c1742019a3d9e01d01608")
                .unwrap()
                .try_into()
                .expect("Expected HMAC must be 32 bytes");

        let result = hmac_sha_256(key, message);
        assert_eq!(result.as_slice(), expected_hmac.as_slice());
    }
}
