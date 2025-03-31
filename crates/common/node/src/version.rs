pub const APP_NAME: &str = "ream";

/// The latest git commit hash of the build.
pub const REAM_FULL_COMMIT: &str = env!("VERGEN_GIT_SHA");
pub const REAM_SHORT_COMMIT: &str = env!("VERGEN_GIT_SHA_SHORT");

/// Ream's version is the same as the git tag.
pub const REAM_VERSION: &str = env!("REAM_VERSION");

/// The operating system of the build, linux, macos, windows etc.
pub const BUILD_OPERATING_SYSTEM: &str = env!("REAM_BUILD_OPERATING_SYSTEM");

/// The architecture of the build, x86_64, aarch64, etc.
pub const BUILD_ARCHITECTURE: &str = env!("REAM_BUILD_ARCHITECTURE");

// /// The version of the programming language used to build the binary.
pub const PROGRAMMING_LANGUAGE_VERSION: &str = env!("VERGEN_RUSTC_SEMVER");

pub const FULL_VERSION: &str = env!("REAM_FULL_VERSION");

/// Returns the ream version and git revision.
pub const fn get_ream_version_short_commit() -> &'static str {
    REAM_SHORT_COMMIT
}

/// Information about the client.
/// example: ream/v0.0.1-892ad575/linux-x86_64/rustc1.85.0
pub fn ream_node_version() -> String {
    format!(
        "{}/{}-{}/{}-{}/rustc{}",
        APP_NAME,
        REAM_VERSION,
        REAM_SHORT_COMMIT,
        BUILD_OPERATING_SYSTEM,
        BUILD_ARCHITECTURE,
        PROGRAMMING_LANGUAGE_VERSION
    )
}
