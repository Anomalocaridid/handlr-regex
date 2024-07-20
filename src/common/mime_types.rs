use crate::error::{Error, ErrorKind, Result};
use derive_more::Deref;
use mime::Mime;
use std::{convert::TryFrom, path::Path, str::FromStr};
use url::Url;

/// A mime derived from a path or URL
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct MimeType(pub Mime);

impl MimeType {
    fn from_ext(ext: &str) -> Result<Mime> {
        match &*xdg_mime::SharedMimeInfo::new()
            .get_mime_types_from_file_name(ext)
        {
            [m] if m == &mime::APPLICATION_OCTET_STREAM => {
                Err(Error::from(ErrorKind::Ambiguous(ext.into())))
            }
            [guess, ..] => Ok(guess.clone()),
            [] => unreachable!(),
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
        let db = xdg_mime::SharedMimeInfo::new();

        let mut guess = db.guess_mime_type();
        guess.file_name(&path.to_string_lossy());

        let mime = if let Some(mime) =
            mime_to_option(&db, guess.guess().mime_type().clone())
        {
            mime
        } else {
            mime_to_option(&db, guess.path(path).guess().mime_type().clone())
                .ok_or_else(|| ErrorKind::Ambiguous(path.to_owned()))?
        };

        Ok(Self(mime))
    }
}

/// Tests if a given mime is "acceptable" and returns None otherwise
fn mime_to_option(db: &xdg_mime::SharedMimeInfo, mime: Mime) -> Option<Mime> {
    let application_zerosize: Mime = "application/x-zerosize".parse().ok()?;

    if mime == mime::APPLICATION_OCTET_STREAM
        || db.mime_type_equal(&mime, &application_zerosize)
    {
        None
    } else {
        Some(mime)
    }
}

/// Mime derived from user input: extension(.pdf) or type like image/jpg
#[derive(Debug, Clone, Deref)]
pub struct MimeOrExtension(pub Mime);

impl FromStr for MimeOrExtension {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let mime = if s.starts_with('.') {
            MimeType::from_ext(s)?
        } else {
            match Mime::from_str(s)? {
                m if m.subtype() == "" => {
                    return Err(Error::from(ErrorKind::InvalidMime(m)))
                }
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
        assert_eq!(MimeOrExtension::from_str(".pdf")?.0, mime::APPLICATION_PDF);
        assert_eq!(
            MimeOrExtension::from_str("image/jpeg")?.0,
            mime::IMAGE_JPEG
        );

        assert!("image//jpg".parse::<MimeOrExtension>().is_err());
        assert!("image".parse::<MimeOrExtension>().is_err());

        Ok(())
    }

    #[test]
    fn from_path() -> Result<()> {
        assert_eq!(
            MimeType::try_from(Path::new("."))?.0.essence_str(),
            "inode/directory"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/rust.vim"))?.0,
            "text/plain"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/cat"))?.0,
            "application/x-shellscript"
        );
        assert_eq!(
            MimeType::try_from(Path::new(
                "./tests/SettingsWidgetFdoSecrets.ui"
            ))?
            .0,
            "application/x-designer"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/empty.txt"))?.0,
            "text/plain"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/p.html"))?.0,
            "text/html"
        );
        assert_eq!(
            MimeType::try_from(Path::new("./tests/no_html_tags.html"))?.0,
            "text/html"
        );

        Ok(())
    }

    #[test]
    fn from_ext() -> Result<()> {
        assert_eq!(".mp3".parse::<MimeOrExtension>()?.0, "audio/mpeg");
        assert_eq!("audio/mpeg".parse::<MimeOrExtension>()?.0, "audio/mpeg");
        assert!(".".parse::<MimeOrExtension>().is_err());
        assert!("audio/".parse::<MimeOrExtension>().is_err());

        Ok(())
    }
}
