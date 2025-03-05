use std::{fs, io, path::PathBuf};

use directories::BaseDirs;

/// Creates a `ream` directory in the system's data directory if it doesn't exist.
///
/// # Returns
///
/// Returns the path to the `ream` directory on success, or an `io::Error` if it fails.
pub fn create_ream_dir() -> io::Result<PathBuf> {
    if let Some(base_dirs) = BaseDirs::new() {
        let ream_dir = base_dirs.data_dir().join("ream");
        if !ream_dir.exists() {
            fs::create_dir_all(&ream_dir)?;
        }
        Ok(ream_dir)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Base directories not found",
        ))
    }
}
