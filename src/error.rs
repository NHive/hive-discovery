use thiserror::Error;

/// Error types that can be produced by the service discovery component.
#[derive(Error, Debug)]
pub enum HiveDiscoError {
    /// Network communication related errors.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// mDNS service errors.
    #[error("mDNS error: {0}")]
    MdnsError(#[from] mdns_sd::Error),

    /// Standard I/O errors.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Errors encountered during service discovery shutdown.
    #[error("Discovery shutdown error: {0}")]
    ShutdownError(String),
    
    /// Configuration errors.
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    /// State errors, indicating an invalid operation for the current state.
    #[error("State error: {0}")]
    StateError(String),
}

/// Result type for the service discovery component.
pub type Result<T> = std::result::Result<T, HiveDiscoError>;
