//! Error type shared by all Kova Native crates.

use std::fmt;

/// The error type returned by fallible Kova Native operations.
#[derive(Debug)]
pub enum KovaError {
    /// The windowing system / event loop failed.
    Platform(String),
    /// No suitable GPU adapter or device could be created, or the surface failed.
    Gpu(String),
    /// An asset could not be loaded or decoded.
    Asset(String),
    /// Font loading failed.
    Font(String),
    Io(std::io::Error),
    Other(String),
}

impl fmt::Display for KovaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KovaError::Platform(m) => write!(f, "platform error: {m}"),
            KovaError::Gpu(m) => write!(f, "gpu error: {m}"),
            KovaError::Asset(m) => write!(f, "asset error: {m}"),
            KovaError::Font(m) => write!(f, "font error: {m}"),
            KovaError::Io(e) => write!(f, "io error: {e}"),
            KovaError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for KovaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            KovaError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for KovaError {
    fn from(e: std::io::Error) -> Self {
        KovaError::Io(e)
    }
}

/// `Result` alias used across Kova Native.
pub type KovaResult<T = ()> = Result<T, KovaError>;
