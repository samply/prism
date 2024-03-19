use thiserror::Error;

#[derive(Error, Debug)]
pub enum PrismError {
    #[error("Parsing error: {0}")]
    ParsingError(String),
    #[error("Beam error: {0}")]
    BeamError(String),
    #[error("Deserialization error: {0}")]
    DeserializationError(serde_json::Error),
    #[error("Decode error: {0}")]
    DecodeError(base64::DecodeError),
    #[error("Unexpected WorkStatus: {0:?}")]
    UnexpectedWorkStatus(beam_lib::WorkStatus),
}
