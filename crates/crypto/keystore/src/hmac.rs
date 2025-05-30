use sha2::{Digest, digest::crypto_common::BlockSizeUser};
use ssz_types::FixedVector;

// Going off of this
// https://en.wikipedia.org/wiki/HMAC#:~:text=In%20cryptography%2C%20an%20HMAC%20(sometimes,and%20a%20secret%20cryptographic%20key.
pub fn hmac<T: Digest + BlockSizeUser>(
    key: &[u8],
    message: &[u8],
) -> FixedVector<u8, T::OutputSize> {
    let block_sized_key = compute_block_sized_key::<T>(key);

    let o_key_pad = block_sized_key
        .iter()
        .map(|&b| b ^ 0x5c)
        .collect::<Vec<_>>();
    let i_key_pad = block_sized_key
        .iter()
        .map(|&b| b ^ 0x36)
        .collect::<Vec<_>>();

    // Compute inner hash
    let mut inner_hasher = T::new();
    inner_hasher.update(&i_key_pad);
    inner_hasher.update(message);
    let inner_hash = inner_hasher.finalize();

    // Compute outer hash
    let mut outer_hasher = T::new();
    outer_hasher.update(&o_key_pad);
    outer_hasher.update(&inner_hash);

    FixedVector::<u8, T::OutputSize>::from(outer_hasher.finalize().to_vec())
}

pub fn compute_block_sized_key<T: Digest + BlockSizeUser>(
    key: &[u8],
) -> FixedVector<u8, T::BlockSize> {
    let block_size = T::block_size();
    if key.len() > block_size {
        let mut hasher = T::new();
        hasher.update(key);
        return FixedVector::<u8, T::BlockSize>::from(hasher.finalize().to_vec());
    }
    FixedVector::<u8, T::BlockSize>::from(key.to_vec())
}

#[cfg(test)]
mod tests {
    use alloy_primitives::hex;
    use sha2::{Sha256, Sha512, Sha512_224, Sha512_256};

    use crate::hmac::hmac;

    #[test]
    fn test_hmac_sha256() {
        let key = b"key";
        let message = b"The quick brown fox jumps over the lazy dog";
        let expected_hmac =
            hex::decode("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8")
                .unwrap();

        let result = hmac::<Sha256>(key, message);
        assert_eq!(result, expected_hmac.into());
    }

    #[test]
    fn test_hmac_sha512() {
        let key = b"key";
        let message = b"The quick brown fox jumps over the lazy dog";
        let expected_hmac = hex::decode("b42af09057bac1e2d41708e48a902e09b5ff7f12ab428a4fe86653c73dd248fb82f948a549f7b791a5b41915ee4d1ec3935357e4e2317250d0372afa2ebeeb3a").unwrap();

        let result = hmac::<Sha512>(key, message);
        assert_eq!(result, expected_hmac.into());
    }

    #[test]
    fn test_hmac_sha512_224() {
        let key = b"key";
        let message = b"The quick brown fox jumps over the lazy dog";
        let expected_hmac =
            hex::decode("a1afb4f708cb63570639195121785ada3dc615989cc3c73f38e306a3").unwrap();

        let result = hmac::<Sha512_224>(key, message);
        assert_eq!(result, expected_hmac.into());
    }

    #[test]
    fn test_hmac_sha512_256() {
        let key = b"a very loooooooooooooooooooooooooooooong keyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy";
        let message = b"The quick brown fox jumps over the lazy dog";
        let expected_hmac =
            hex::decode("52ebe97c6bc6cf28493d0ac9304af47482eda1345f9c80be311949ccc1726b6b")
                .unwrap();

        let result = hmac::<Sha512_256>(key, message);
        assert_eq!(result, expected_hmac.into());
    }
}
