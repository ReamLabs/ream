
#[derive(Debug, Clone)]
pub enum ApiError {
    NotFound(String),
    InvalidInput(String),
    Http(StatusCode, String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ApiError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            ApiError::Http(code, msg) => write!(f, "HTTP error {}: {}", code, msg),
        }
    }
}

impl Error for ApiError {}