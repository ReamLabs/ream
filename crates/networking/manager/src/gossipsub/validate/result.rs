#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ValidationResult {
    Accept,
    Ignore(String),
    Reject(String),
}
