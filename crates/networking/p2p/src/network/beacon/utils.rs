use std::{fs, path::PathBuf};

use anyhow::anyhow;
use ssz::Decode;

use crate::req_resp::beacon::messages::meta_data::GetMetaDataV2;

pub const META_DATA_FILE_NAME: &str = "meta_data.ssz";

pub fn read_meta_data_from_disk(path: PathBuf) -> anyhow::Result<GetMetaDataV2> {
    let meta_data_path = path.join(META_DATA_FILE_NAME);
    if !meta_data_path.exists() {
        return Ok(GetMetaDataV2::default());
    }

    GetMetaDataV2::from_ssz_bytes(&fs::read(meta_data_path)?)
        .map_err(|err| anyhow!("Failed to decode meta data: {err:?}"))
}
