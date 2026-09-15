use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PsdError {
    #[error("this is not a Photoshop file (no 8BPS signature)")]
    NotPsd,
    #[error("the file ends early while reading {0}")]
    Truncated(&'static str),
    #[error("the file is damaged: {0}")]
    Corrupt(String),
    #[error("{0} is not supported")]
    Unsupported(String),
    #[error("the file needs more memory than allowed ({needed} bytes, limit {limit})")]
    TooLarge { needed: u64, limit: u64 },
    #[error("cannot write this document: {0}")]
    Write(String),
}

pub type Result<T> = std::result::Result<T, PsdError>;

pub(crate) fn corrupt(msg: impl Into<String>) -> PsdError {
    PsdError::Corrupt(msg.into())
}
