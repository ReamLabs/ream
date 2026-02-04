/// The maximum allowed size of uncompressed payload in gossipsub messages and RPC chunks
pub const MAX_PAYLOAD_SIZE: u64 = 10485760;

/// Maximum number of blocks that can be requested in a single request (BeaconBlocksByRange)
pub const MAX_REQUEST_BLOCKS: u64 = 1024;

/// Maximum number of blocks that can be requested in a single request for Deneb and later
pub const MAX_REQUEST_BLOCKS_DENEB: u64 = 128;

/// Maximum number of blob sidecars that can be requested
pub const MAX_BLOBS_PER_BLOCK: u64 = 9;

/// Maximum number of blob sidecars that can be requested in a single request
pub const MAX_REQUEST_BLOB_SIDECARS: u64 = MAX_REQUEST_BLOCKS_DENEB * MAX_BLOBS_PER_BLOCK;

/// Maximum number of data column sidecars that can be requested per column
pub const MAX_REQUEST_DATA_COLUMN_SIDECARS_PER_COLUMN: u64 = MAX_REQUEST_BLOCKS_DENEB;
