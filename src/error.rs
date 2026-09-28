//! The crate's one error type.

/// What went wrong. Every malformed input comes back as one of these; the
/// decoder never panics on bytes it is given.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The bitstream breaks the syntax or the semantics of the standard: a
    /// field out of range, a codeword that matches nothing, data that runs
    /// past the end of the access unit.
    #[error("invalid AAC data: {0}")]
    Invalid(String),
    /// Valid AAC this crate does not implement, named: an audio object type
    /// other than AAC-LC, a coupling channel element, 960-sample frames.
    #[error("unsupported AAC feature: {0}")]
    Unsupported(String),
    /// A configuration the caller asked for that cannot be coded: a channel
    /// count, sample rate or bit rate outside what the encoder supports.
    #[error("invalid AAC configuration: {0}")]
    Config(String),
}

pub(crate) fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

pub(crate) fn unsupported(msg: impl Into<String>) -> Error {
    Error::Unsupported(msg.into())
}

pub type Result<T> = std::result::Result<T, Error>;
