use std::fmt::Display;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PrismError {
    #[error("Configuration error")]
    ConfigurationError(String),
    #[error("Cannot open file")]
    FileOpeningError(String),
    #[error("Parsing error")]
    ParsingError(String),
    #[error("CQL tampered with: {0}")]
    DeserializationError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Invalid Header Value: {0}")]
    InvalidHeaderValue(http::header::InvalidHeaderValue),
}
