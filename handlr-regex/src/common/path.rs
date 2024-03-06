use mime::Mime;
use serde::Serialize;
use tabled::{
    settings::{Alignment, Padding, Style},
    Table, Tabled,
};
use url::Url;

use crate::{common::MimeType, Error, ErrorKind, Result};
use std::{
    convert::TryFrom,
    fmt::{Display, Formatter},
    io::IsTerminal,
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

/// Internal helper struct for turning a UserPath into tabular data
#[derive(Tabled, Serialize)]
struct UserPathTable {
    path: String,
    mime: String,
}

impl UserPathTable {
    fn new(path: &UserPath) -> Result<Self> {
        Ok(Self {
            path: path.to_string(),
            mime: path.get_mime()?.essence_str().to_owned(),
        })
    }
}

pub fn mime_table(paths: &[UserPath], output_json: bool) -> Result<()> {
    let rows = paths
        .iter()
        .map(UserPathTable::new)
        .collect::<Result<Vec<UserPathTable>>>()?;

    let table = if output_json {
        serde_json::to_string(&rows)?
    } else if std::io::stdout().is_terminal() {
        // If output is going to a terminal, print as a table
        Table::new(&rows).with(Style::sharp()).to_string()
    } else {
        // If output is being piped, print as tab-delimited text
        let mut table = Table::new(&rows);
        table
            .with(Style::empty().vertical('\t'))
            .with(Alignment::left())
            .with(Padding::zero());
        table.to_string()
    };

    println!("{table}");

    Ok(())
}
