use ascii_table::AsciiTable;
use mime::Mime;
use url::Url;

use crate::{common::MimeType, Error, ErrorKind, Result};
use std::{
    convert::TryFrom,
    fmt::{Display, Formatter},
    path::PathBuf,
    str::FromStr,
};

pub enum UserPath {
    Url(Url),
    File(PathBuf),
}

impl UserPath {
    pub fn get_mime(&self) -> Result<Mime> {
        Ok(match self {
            Self::Url(url) => Ok(url.into()),
            Self::File(f) => MimeType::try_from(f.as_path()),
        }?
        .0)
    }
}

impl FromStr for UserPath {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = match url::Url::parse(s) {
            Ok(url) if url.scheme() == "file" => {
                let path = url.to_file_path().map_err(|_| {
                    Error::from(ErrorKind::BadPath(url.path().to_owned()))
                })?;

                Self::File(path)
            }
            Ok(url) => Self::Url(url),
            _ => Self::File(PathBuf::from(s)),
        };

        Ok(normalized)
    }
}

impl Display for UserPath {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::File(f) => fmt.write_str(&f.to_string_lossy()),
            Self::Url(u) => fmt.write_str(u.as_ref()),
        }
    }
}

pub fn mime_table(paths: &[UserPath], output_json: bool) -> Result<(), Error> {
    if output_json {
        let rows = paths
            .iter()
            .map(|path| {
                Ok(json::object! {
                    path: path.to_string(),
                    mime: path.get_mime()?.essence_str().to_owned()
                })
            })
            .collect::<Result<json::Array>>()?;
        println!("{}", json::stringify(rows));
    } else {
        let rows = paths
            .iter()
            .map(|path| {
                Ok(vec![
                    path.to_string(),
                    path.get_mime()?.essence_str().to_owned(),
                ])
            })
            .collect::<Result<Vec<Vec<String>>>>()?;

        let table = AsciiTable::default();
        table.print(rows);
    }

    Ok(())
}
