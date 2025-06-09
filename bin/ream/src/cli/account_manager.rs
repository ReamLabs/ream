use bip32::Mnemonic;
use clap::Parser;
use rand::rngs::OsRng;
use tracing::warn;

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
}

impl Default for AccountManagerConfig {
    fn default() -> Self {
        Self {
            verbosity: 3,
            lifetime: 28,
            chunk_size: 5,
            seed_phrase: None,
        }
    }
}

impl AccountManagerConfig {
    pub fn new() -> Self {
        Self::parse()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        // Validate chunk size
        if self.chunk_size < 4 {
            anyhow::bail!("Chunk size must be at least 4");
        }

        // Validate lifetime
        if self.lifetime < 18 {
            anyhow::bail!("Lifetime must be at least 18");
        }

        Ok(())
    }

    pub fn get_seed_phrase(&self) -> String {
        if let Some(phrase) = &self.seed_phrase {
            phrase.clone()
        } else {
            let mnemonic = Mnemonic::random(OsRng, Default::default());
            let phrase = mnemonic.phrase().to_string();
            warn!("⚠️  IMPORTANT: Generated new seed phrase: {}", phrase);
            warn!(
                "⚠️  Please save this seed phrase somewhere safe. You will need it to recover your keys."
            );
            phrase
        }
    }
}
