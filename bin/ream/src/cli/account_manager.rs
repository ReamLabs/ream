use anyhow::ensure;
use clap::Parser;

const MIN_CHUNK_SIZE: u64 = 4;
const MIN_LIFETIME: u64 = 18;
const DEFAULT_ACTIVATION_EPOCH: usize = 0;
const DEFAULT_NUM_ACTIVE_EPOCHS: usize = 1 << 28;

#[derive(Debug, Parser)]
pub struct AccountManagerConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    /// Account lifetime in 2 ** lifetime slots
    #[arg(short, long, default_value_t = 28)]
    pub lifetime: u64,

    /// Chunk size for messages
    #[arg(short, long, default_value_t = 5)]
    pub chunk_size: u64,

    /// Seed phrase for key generation
    #[arg(short, long)]
    pub seed_phrase: Option<String>,

    /// Activation epoch for the validator
    #[arg(long, default_value_t = DEFAULT_ACTIVATION_EPOCH)]
    pub activation_epoch: usize,

    /// Number of active epochs
    #[arg(long, default_value_t = DEFAULT_NUM_ACTIVE_EPOCHS)]
    pub num_active_epochs: usize,
}

impl Default for AccountManagerConfig {
    fn default() -> Self {
        Self {
            verbosity: 3,
            lifetime: 28,
            chunk_size: 5,
            seed_phrase: None,
            activation_epoch: DEFAULT_ACTIVATION_EPOCH,
            num_active_epochs: DEFAULT_NUM_ACTIVE_EPOCHS,
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
        Ok(())
    }

    pub fn get_seed_phrase(&self) -> String {
        if let Some(phrase) = &self.seed_phrase {
            phrase.clone()
        } else {
            "default_seed_phrase".to_string()
        }
    }
}
