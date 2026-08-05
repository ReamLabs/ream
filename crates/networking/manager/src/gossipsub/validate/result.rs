use alloy_primitives::B256;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ValidationResult {
    Accept,
    Ignore(String),
    Reject(String),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DependencyValidationResult<T> {
    Accept,
    Ignore(String),
    Reject(String),
    ParentPendingAvailability { parent_root: B256, validated: T },
}
