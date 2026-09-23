use thiserror::Error as ThisError;

/// An error raised while reading a `cty.dat` file.
#[derive(Debug, Clone, PartialEq, ThisError)]
pub enum CtyError {
    #[error("malformed cty.dat record header: {0}")]
    MalformedHeader(String),

    #[error("invalid {field} in record header: {header}")]
    InvalidHeaderField { header: String, field: &'static str },

    #[error("unknown continent abbreviation: {0}")]
    UnknownContinent(String),

    #[error("malformed override in alias: {0}")]
    MalformedOverride(String),

    #[error("invalid {field} override in alias: {alias}")]
    InvalidOverride { alias: String, field: &'static str },

    #[error("UTC offset out of range: {0}")]
    UtcOffsetOutOfRange(f64),
}
