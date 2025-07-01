use crate::error::{Error, Result};
use derive_more::Deref;
use mime::Mime;
use std::{convert::TryFrom, path::Path, str::FromStr};
use tracing_unwrap::{OptionExt, ResultExt};
use url::Url;

/// A mime derived from a path or URL
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Deref)]
pub struct MimeType(pub Mime);

impl MimeType {
    /// Gets the `Mime` from a given file extension
    fn from_ext(ext: &str) -> Result<Mime> {
        match xdg_mime::SharedMimeInfo::new()
            .get_mime_types_from_file_name(ext)
            .into_iter()
            .nth(0)
            .expect_or_log("The function xdg_mime::get_mime_types_from_file_type should always return a non-empty Vec<Mime>")
        {
            // If the file extension is ambiguous, then error
            // Otherwise, the user may not expect this mimetype being assigned
            mime if mime == mime::APPLICATION_OCTET_STREAM => {
                Err(Error::AmbiguousExtension(ext.into()))
            }
            mime => Ok(mime)
        }
    }
}

impl TryFrom<&Url> for MimeType {
    type Error = Error;
    fn try_from(url: &Url) -> Result<Self> {
        Ok(Self(
            format!("x-scheme-handler/{}", url.scheme()).parse::<Mime>()?,
        ))
    }
}

impl TryFrom<&Path> for MimeType {
    type Error = Error;
    fn try_from(path: &Path) -> Result<Self> {
        if !path.try_exists()? {
            return Err(Error::NonexistentFile(
                path.to_string_lossy().to_string(),
            ));
        }

        let db = xdg_mime::SharedMimeInfo::new();

        let mut guess = db.guess_mime_type();
        guess.file_name(&path.to_string_lossy());

        let mut mime = guess.guess().mime_type().clone();
        // TODO: remove this check once xdg-mime crate makes a new release (currently v0.4.0)
        if mime
            == "application/x-zerosize"
                .parse::<Mime>()
                .expect_or_log("Hardcoded mime should be valid")
        {
            mime = guess.path(path).guess().mime_type().clone();
        }

        Ok(Self(mime.clone()))
    }
}

impl FromStr for MimeType {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let mime = if s.starts_with('.') {
            Self::from_ext(s)?
        } else {
            match Mime::from_str(s)? {
                m if m.subtype() == "" => return Err(Error::InvalidMime(m)),
                proper_mime => proper_mime,
            }
        };

        Ok(Self(mime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_input() -> Result<()> {
        assert_eq!(MimeType::from_str(".pdf")?.0, mime::APPLICATION_PDF);
        assert_eq!(MimeType::from_str("image/jpeg")?.0, mime::IMAGE_JPEG);

        assert!("image//jpg".parse::<MimeType>().is_err());
        assert!("image".parse::<MimeType>().is_err());

        Ok(())
    }

    #[test]
    fn from_path() -> Result<()> {
        assert_eq!(
            MimeType::try_from(Path::new("."))?.0.essence_str(),
            "inode/directory"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/assets/rust.vim"))?.0,
            "text/plain"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/assets/cat"))?.0,
            "application/x-shellscript"
        );
        assert_eq!(
            MimeType::try_from(Path::new(
                "./tests/assets/SettingsWidgetFdoSecrets.ui"
            ))?
            .0,
            "application/x-designer"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/assets/empty.txt"))?.0,
            "text/plain"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/assets/p.html"))?.0,
            "text/html"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/assets/no_html_tags.html"))?
                .0,
            "text/html"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/assets/empty"))?.0,
            "application/x-zerosize"
        );
        assert_eq!(
            MimeType::try_from(Path::new(
                "./tests/assets/nonsense_binary_data"
            ))?
            .0,
            "application/octet-stream"
        );

        Ok(())
    }

    #[test]
    fn from_str() -> Result<()> {
        assert_eq!(".mp3".parse::<MimeType>()?.0, "audio/mpeg");
        assert_eq!("audio/mpeg".parse::<MimeType>()?.0, "audio/mpeg");
        assert!(".".parse::<MimeType>().is_err());
        assert!("audio/".parse::<MimeType>().is_err());
        assert_eq!(
            "application/octet-stream".parse::<MimeType>()?.0,
            "application/octet-stream"
        );

        Ok(())
    }

    #[test]
    fn nonexistent_file() {
        assert!(MimeType::try_from(Path::new("nonexistent_file")).is_err());
    }
}
