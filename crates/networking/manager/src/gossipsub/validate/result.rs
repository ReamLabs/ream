#[derive(Debug)]
pub enum ValidationResult {
    Accept,
    Ignore(String),
    Reject(String),
}
