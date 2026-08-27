//use nostr::prelude::secp256k1;
use std::array::TryFromSliceError;
use thiserror::Error;

/// Websocket failure details retained across the transport boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebSocketError {
    message: String,
    raw_os_error: Option<i32>,
}

impl WebSocketError {
    /// Build websocket error data without a structured OS error code.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            raw_os_error: None,
        }
    }

    /// Return the original websocket error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Return the OS error code reported by the native websocket backend.
    pub fn raw_os_error(&self) -> Option<i32> {
        self.raw_os_error
    }

    pub(crate) fn with_context(mut self, context: impl std::fmt::Display) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
    }
}

impl std::fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for WebSocketError {}

impl From<std::io::Error> for WebSocketError {
    fn from(error: std::io::Error) -> Self {
        Self {
            message: error.to_string(),
            raw_os_error: error.raw_os_error(),
        }
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for WebSocketError {
    fn from(error: tokio_tungstenite::tungstenite::Error) -> Self {
        match error {
            tokio_tungstenite::tungstenite::Error::Io(error) => error.into(),
            error => Self::new(error.to_string()),
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("message is empty")]
    Empty,

    #[error("decoding failed: {0}")]
    DecodeFailed(String),

    #[error("hex decoding failed")]
    HexDecodeFailed,

    #[error("invalid bech32")]
    InvalidBech32,

    #[error("invalid byte size")]
    InvalidByteSize,

    #[error("invalid signature")]
    InvalidSignature,

    #[error("invalid public key")]
    InvalidPublicKey,

    #[error("invalid relay url")]
    InvalidRelayUrl,

    // Secp(secp256k1::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("nostrdb error: {0}")]
    Nostrdb(#[from] nostrdb::Error),

    #[error("websocket error: {0}")]
    WebSocket(WebSocketError),

    #[error("{0}")]
    Generic(String),
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Generic(s)
    }
}

impl From<TryFromSliceError> for Error {
    fn from(_e: TryFromSliceError) -> Self {
        Error::InvalidByteSize
    }
}

impl From<hex::FromHexError> for Error {
    fn from(_e: hex::FromHexError) -> Self {
        Error::HexDecodeFailed
    }
}

// nostrdb-net convergence: enostr re-exports nostrdb_net's primitives, whose
// fallible methods (e.g. `Pubkey::from_hex`) yield `nostrdb_net::Error`. Bridge
// it into enostr's error so `?` still works at enostr call sites during the
// migration. Variants map 1:1 where they exist; the rest stringify.
impl From<nostrdb_net::Error> for Error {
    fn from(e: nostrdb_net::Error) -> Self {
        use nostrdb_net::Error as NnErr;
        match e {
            NnErr::Empty => Error::Empty,
            NnErr::HexDecodeFailed => Error::HexDecodeFailed,
            NnErr::InvalidBech32 => Error::InvalidBech32,
            NnErr::InvalidByteSize => Error::InvalidByteSize,
            NnErr::InvalidSignature => Error::InvalidSignature,
            NnErr::InvalidPublicKey => Error::InvalidPublicKey,
            NnErr::Nostrdb(err) => Error::Nostrdb(err),
            other => Error::Generic(other.to_string()),
        }
    }
}
