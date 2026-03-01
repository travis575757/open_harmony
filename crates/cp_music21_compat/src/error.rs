use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompatError {
    #[error("invalid MusicXML: {0}")]
    InvalidMusicXml(String),
    #[error("missing required MusicXML field: {0}")]
    MissingField(&'static str),
    #[error("invalid MusicXML value in {field}: {value}")]
    InvalidValue { field: &'static str, value: String },
    #[error("serialization failed: {0}")]
    Serialization(String),
}
