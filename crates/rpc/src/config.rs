
/// Server Config.
#[derive(Debug)]
pub struct ServerConfig {
    pub http_port: usize,
    pub http_address: String,
    pub http_allow_origin: bool,
}

impl ServerConfig {
    /// Creates a new instance from CLI arguments
    pub fn from_args(http_port: usize, http_address: String, http_allow_origin: bool) -> Self {
        Self {
            http_port: http_port,
            http_address: http_address,
            http_allow_origin,
        }
    }

    /// Returns the server's full address as a string
    pub fn full_address(&self) -> String {
        format!("{}:{}", self.http_address, self.http_port)
    }

    /// Checks if CORS is enabled
    pub fn is_cors_enabled(&self) -> bool {
        self.http_allow_origin
    }
}
