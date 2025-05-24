use std::{cmp::max, fs, path::PathBuf};

use anyhow::anyhow;
use ssz::Decode;

use crate::{constants::MAX_PAYLOAD_SIZE, req_resp::messages::meta_data::GetMetaDataV2};

pub const META_DATA_FILE_NAME: &str = "meta_data.ssz";

/// Worst-case compressed length for a given payload of size n when using snappy:
/// https://github.com/google/snappy/blob/32ded457c0b1fe78ceb8397632c416568d6714a0/snappy.cc#L218C1-L218C47
pub fn max_compressed_len(n: u64) -> u64 {
    32 + n + n / 6
}

/// Allow 1024 bytes for framing and encoding overhead but at least 1MiB in case MAX_PAYLOAD_SIZE is
/// small.
pub fn max_message_size() -> u64 {
    max(max_compressed_len(MAX_PAYLOAD_SIZE) + 1024, 1024 * 1024)
}

pub fn read_meta_data_from_disk(path: PathBuf) -> anyhow::Result<GetMetaDataV2> {
    let meta_data_path = path.join(META_DATA_FILE_NAME);
    if !meta_data_path.exists() {
        return Ok(GetMetaDataV2::default());
    }

    GetMetaDataV2::from_ssz_bytes(&fs::read(meta_data_path)?)
        .map_err(|err| anyhow!("Failed to decode meta data: {err:?}"))
}
