use std::{path::PathBuf, process::ExitCode};

use tracing::{error, info};

/// Custom error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Xdg(#[from] xdg::BaseDirectoriesError),
    #[error(transparent)]
    Config(#[from] confy::ConfyError),
    #[error("No handlers found for '{0}'")]
    NotFound(String),
    #[error(
        "Could not find a mimetype associated with the file extension: '{0}'"
    )]
    AmbiguousExtension(String),
    #[error(transparent)]
    BadMimeType(#[from] mime::FromStrError),
    #[error("Bad mime: {0}")]
    InvalidMime(mime::Mime),
    #[error("The desktop entry at '{0}' lacks a valid '{1}' field")]
    BadEntry(PathBuf, String),
    #[error("Error spawning selector process '{0}'")]
    Selector(String),
    #[error("Selection cancelled")]
    Cancelled,
    #[error("Please specify the default terminal with handlr set x-scheme-handler/terminal")]
    NoTerminal,
    #[error("Bad path: {0}")]
    BadPath(String),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    SerdeIniDe(#[from] serde_ini::de::Error),
    #[error(transparent)]
    SerdeIniSer(#[from] serde_ini::ser::Error),
    #[error(transparent)]
    TracingGlobalDefault(#[from] tracing::dispatcher::SetGlobalDefaultError),
    #[error("Could not find file at path: '{0}'")]
    NonexistentFile(String),
    #[cfg(test)]
    #[error(transparent)]
    BadUrl(#[from] url::ParseError),
    #[cfg(test)]
    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[mutants::skip] // Cannot completely test, relies on user input
pub fn handle(result: Result<()>) -> ExitCode {
    if let Err(error) = result {
        match error {
            // Cancelling the selector is an acceptable outcome
            Error::Cancelled => {
                info!("{}", error);
                ExitCode::SUCCESS
            }
            _ => {
                error!("{}", error);
                ExitCode::FAILURE
            }
        }
    } else {
        ExitCode::SUCCESS
    }
}
