/// Custom error type
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error {
    pub kind: Box<ErrorKind>,
}

impl<E> From<E> for Error
where
    ErrorKind: From<E>,
{
    fn from(err: E) -> Self {
        Error {
            kind: Box::new(ErrorKind::from(err)),
        }
    }
}

/// Custom error messages
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Xdg(#[from] xdg::BaseDirectoriesError),
    #[error(transparent)]
    Config(#[from] confy::ConfyError),
    #[error("no handlers found for '{0}'")]
    NotFound(String),
    #[error("could not figure out the mime type of '{0}'")]
    Ambiguous(std::path::PathBuf),
    #[error(transparent)]
    BadMimeType(#[from] mime::FromStrError),
    #[error("bad mime: {0}")]
    InvalidMime(mime::Mime),
    #[error("malformed desktop entry at {0}")]
    BadEntry(std::path::PathBuf),
    #[error(transparent)]
    BadRegex(#[from] regex::Error),
    #[error("error spawning selector process '{0}'")]
    Selector(String),
    #[error("selection cancelled")]
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
    #[error("Could not split exec command '{0}' in desktop file '{1}' into shell words")]
    BadExec(String, String),
    #[error("Could not split command '{0}' into shell words")]
    BadCmd(String),
    #[cfg(test)]
    #[error(transparent)]
    BadUrl(#[from] url::ParseError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
