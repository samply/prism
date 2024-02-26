use thiserror::Error;

#[derive(Error, Debug)]
pub enum PrismError {
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("Cannot open file: {0}")]
    FileOpeningError(String),
    #[error("Parsing error: {0}")]
    ParsingError(String),
    #[error("Beam error: {0}")]
    BeamError(String),
    #[error("CQL tampered with: {0}")]
    DeserializationError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Invalid Header Value: {0}")]
    InvalidHeaderValue(http::header::InvalidHeaderValue),
    #[error("Decode error: {0}")]
    DecodeError(base64::DecodeError),
}
