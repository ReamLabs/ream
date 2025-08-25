#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeanSupportedProtocol {
    BlocksByRootV1,
    StatusV1,
}

impl LeanSupportedProtocol {
    pub fn message_name(&self) -> &str {
        match self {
            LeanSupportedProtocol::BlocksByRootV1 => "lean_blocks_by_root",
            LeanSupportedProtocol::StatusV1 => "status",
        }
    }

    pub fn schema_version(&self) -> &str {
        match self {
            LeanSupportedProtocol::BlocksByRootV1 => "1",
            LeanSupportedProtocol::StatusV1 => "1",
        }
    }

    pub fn has_context_bytes(&self) -> bool {
        match self {
            LeanSupportedProtocol::BlocksByRootV1 => false,
            LeanSupportedProtocol::StatusV1 => false,
        }
    }
}
