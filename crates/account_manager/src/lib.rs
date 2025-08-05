use anyhow::Result;
use rand::thread_rng;
use tracing::info;

// Custom signature scheme with lifetime 2^8
// THIS SHOULD BE USED ONLY FOR TESTING PURPOSES
pub mod custom_lifetime_8 {
    use hashsig::{
        inc_encoding::basic_winternitz::WinternitzEncoding,
        signature::generalized_xmss::GeneralizedXMSSSignatureScheme,
        symmetric::{
            message_hash::poseidon::PoseidonMessageHash, prf::shake_to_field::ShakePRFtoF,
            tweak_hash::poseidon::PoseidonTweakHash,
        },
    };

    // Configuration constants for lifetime 2^8
    const LOG_LIFETIME: usize = 8;
    const PARAMETER_LEN: usize = 5;
    const MSG_HASH_LEN_FE: usize = 5;
    const HASH_LEN_FE: usize = 7;
    const MSG_LEN_FE: usize = 9;
    const TWEAK_LEN_FE: usize = 2;
    const RAND_LEN: usize = 5;
    const CAPACITY: usize = 9;

    // Winternitz encoding parameters for chunk size 8
    const CHUNK_SIZE_W8: usize = 8;
    const BASE_W8: usize = 256;
    const NUM_CHUNKS_W8: usize = 20;
    const NUM_CHUNKS_CHECKSUM_W8: usize = 2;

    // Type definitions
    type MHw8 = PoseidonMessageHash<
        PARAMETER_LEN,
        RAND_LEN,
        MSG_HASH_LEN_FE,
        NUM_CHUNKS_W8,
        BASE_W8,
        TWEAK_LEN_FE,
        MSG_LEN_FE,
    >;
    type THw8 =
        PoseidonTweakHash<PARAMETER_LEN, HASH_LEN_FE, TWEAK_LEN_FE, CAPACITY, NUM_CHUNKS_W8>;
    type PRFw8 = ShakePRFtoF<HASH_LEN_FE>;
    type IEw8 = WinternitzEncoding<MHw8, CHUNK_SIZE_W8, NUM_CHUNKS_CHECKSUM_W8>;

    /// Custom instantiation with Lifetime 2^8, Winternitz encoding, chunk size w = 8
    pub type CustomSIGWinternitzLifetime8W8 =
        GeneralizedXMSSSignatureScheme<PRFw8, IEw8, THw8, LOG_LIFETIME>;
}

/// Generates public and secret keys with default configuration and returns them in serialized
/// format
pub fn generate_keys_with_default_config() -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>>
{
    use custom_lifetime_8::CustomSIGWinternitzLifetime8W8;
    use hashsig::signature::SignatureScheme;
    let mut rng = thread_rng();

    // Generate keys with default configuration:
    // - Poseidon winternitz signature type
    // - Chunk size of 8
    // - Lifetime of 2^8
    let (public_key, secret_key) =
        <CustomSIGWinternitzLifetime8W8 as SignatureScheme>::key_gen(&mut rng, 0, 256);

    // Serialize keys using serde_json instead of bincode for now
    let public_key_serialized = serde_json::to_vec(&public_key)?;
    let secret_key_serialized = serde_json::to_vec(&secret_key)?;

    Ok((public_key_serialized, secret_key_serialized))
}

pub fn generate_keys(seed_phrase: &str) {
    // TODO: Implement this
    info!("Generating keys with seed phrase: {}", seed_phrase);
}
