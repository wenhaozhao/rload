use std::fmt;

#[derive(Debug)]
pub enum RunError {
    InvalidUrl(String),
    InvalidConfig(String),
    InvalidAccessLog(String),
    InvalidRequestFile(String),
    Io(std::io::Error),
    Tls(String),
    InvalidResponse(String),
    WorkerPanic,
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(message) => write!(f, "invalid URL: {message}"),
            Self::InvalidConfig(message) => write!(f, "invalid configuration: {message}"),
            Self::InvalidAccessLog(message) => write!(f, "invalid access log: {message}"),
            Self::InvalidRequestFile(message) => write!(f, "invalid request file: {message}"),
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Tls(message) => write!(f, "TLS error: {message}"),
            Self::InvalidResponse(message) => write!(f, "invalid HTTP response: {message}"),
            Self::WorkerPanic => write!(f, "load worker panicked"),
        }
    }
}

impl std::error::Error for RunError {}

impl From<std::io::Error> for RunError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
