use thiserror::Error;

#[derive(Error, Debug)]
pub enum CraftError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML serialization error: {0}")]
    Toml(#[from] toml::ser::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Server '{0}' not found")]
    ServerNotFound(String),

    #[error("Server at '{0}' is already registered")]
    ServerAlreadyExists(String),

    #[error("Path '{0}' does not exist or is not a valid directory")]
    InvalidPath(String),

    #[error("Directory '{0}' is not empty")]
    DirectoryNotEmpty(String),

    #[error("Software '{0}' is unsupported")]
    UnknownSoftware(String),

    #[error("Version '{version}' is not available for software '{software}'")]
    UnknownVersion { software: String, version: String },

    #[error("Java detection error: {0}")]
    Java(String),

    #[error("Process error: {0}")]
    Process(String),

    #[error("IPC communication error: {0}")]
    Ipc(String),

    #[error("Download error: {0}")]
    Download(String),

    #[error("Checksum mismatch for '{file}': expected {expected}, got {actual}")]
    ChecksumMismatch {
        file: String,
        expected: String,
        actual: String,
    },

    #[error("Interactive prompt error: {0}")]
    Prompt(#[from] dialoguer::Error),

    #[error("Operation cancelled by user")]
    Cancelled,

    #[error("TUI error: {0}")]
    Tui(#[from] modalx::TuiError),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CraftError>;
