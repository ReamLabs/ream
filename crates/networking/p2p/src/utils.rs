use std::cmp::max;

use crate::constants::MAX_PAYLOAD_SIZE;

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
