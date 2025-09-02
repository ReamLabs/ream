use anyhow::ensure;
use bip39::Mnemonic;
use clap::Parser;
use tracing::info;

const MIN_CHUNK_SIZE: u64 = 4;
const MIN_LIFETIME: u64 = 18;
const DEFAULT_ACTIVATION_EPOCH: usize = 0;
const DEFAULT_NUM_ACTIVE_EPOCHS: usize = 1 << 18;

#[derive(Debug, Parser)]
pub struct AccountManagerConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    /// Account lifetime in 2 ** lifetime slots
    #[arg(short, long, default_value_t = 18)]
    pub lifetime: u64,

    /// Chunk size for messages
    #[arg(short, long, default_value_t = 5)]
    pub chunk_size: u64,

    /// Seed phrase for key generation
    #[arg(short, long)]
    pub seed_phrase: Option<String>,

    /// Optional BIP39 passphrase used with the seed phrase
    #[arg(long)]
    pub passphrase: Option<String>,

    /// Activation epoch for the validator
    #[arg(long, default_value_t = DEFAULT_ACTIVATION_EPOCH)]
    pub activation_epoch: usize,

    /// Number of active epochs
    #[arg(long, default_value_t = DEFAULT_NUM_ACTIVE_EPOCHS)]
    pub num_active_epochs: usize,

    /// Path for keystore file
    #[arg(long, default_value = "./.keystore/")]
    pub path: Option<String>,

    /// Import existing keystore
    #[arg(long)]
    pub import_keystore: Option<String>,
}

impl Default for AccountManagerConfig {
    fn default() -> Self {
        Self {
            verbosity: 3,
            lifetime: 18,
            chunk_size: 5,
            seed_phrase: None,
            passphrase: None,
            activation_epoch: DEFAULT_ACTIVATION_EPOCH,
            num_active_epochs: DEFAULT_NUM_ACTIVE_EPOCHS,
            path: Some("./.keystore/".to_string()),
            import_keystore: None,
        }
    }
}

impl AccountManagerConfig {
    pub fn new() -> Self {
        Self::parse()
    }

    pub fn validate(&mut self) -> anyhow::Result<()> {
        ensure!(
            self.chunk_size >= MIN_CHUNK_SIZE,
            "Chunk size must be at least {MIN_CHUNK_SIZE}"
        );
        ensure!(
            self.lifetime >= MIN_LIFETIME,
            "Lifetime must be at least {MIN_LIFETIME}"
        );

        // Ensure that if import-keystore is provided, seed-phrase is not provided
        ensure!(
            !(self.import_keystore.is_some() && self.seed_phrase.is_some()),
            "Cannot provide both --seed-phrase and --import-keystore. Use one or the other."
        );

        Ok(())
    }

    pub fn get_seed_phrase(&self) -> String {
        if let Some(phrase) = &self.seed_phrase {
            phrase.clone()
        } else {
            // Generate a new BIP39 mnemonic with 24 words (256 bits of entropy)
            let entropy: [u8; 32] = rand::random();
            let mnemonic = Mnemonic::from_entropy_in(bip39::Language::English, &entropy).unwrap();
            let phrase = mnemonic.words().collect::<Vec<_>>().join(" ");
            info!(
                "========================================================================================="
            );
            info!("Generated new seed phrase (KEEP SAFE): {}", phrase);
            info!(
                "========================================================================================="
            );
            phrase
        }
    }
}
