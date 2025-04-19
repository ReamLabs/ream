use std::path::Path;

use anyhow::anyhow;
use snap::raw::Decoder;

pub fn read_ssz_snappy<T: ssz::Decode>(path: &Path) -> anyhow::Result<T> {
    let ssz_snappy = std::fs::read(path)?;
    let mut decoder = Decoder::new();
    let ssz = decoder.decompress_vec(&ssz_snappy)?;
    T::from_ssz_bytes(&ssz).map_err(|err| anyhow!("Failed to decode SSZ: {:?}", err))
}
