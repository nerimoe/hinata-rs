use std::num::ParseIntError;
use std::string::FromUtf8Error;
use thiserror::Error;
use crate::hinata::pn532::Pn532Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Parse Error: {0}")]
    Parse(String),

    #[error("PN532 Error: {0}")]
    Pn532(#[from] Pn532Error),

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Timeout Error: {0}")]
    Timeout(String),

    #[error("Not Found Error: {0}")]
    NotFound(String),

    #[error("Disconnect Error: {0}")]
    Disconnected(String),

    #[error("NotSupport Error: {0}")]
    NotSupport(String),

    #[error("Protocol Error: {0}")]
    Protocol(String),

    #[error("Other Error: {0}")]
    Other(String),
}

impl From<FromUtf8Error> for Error {
    fn from(e: FromUtf8Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<ParseIntError> for Error {
    fn from(e: ParseIntError) -> Self {
        Error::Parse(e.to_string())
    }
}
