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

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error(transparent)]
    ParseApps(#[from] pest::error::Error<crate::apps::MimeappsRule>),
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
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
