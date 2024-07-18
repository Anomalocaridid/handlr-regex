use crate::{
    common::{render_table, MimeType},
    error::{Error, ErrorKind, Result},
};
use mime::Mime;
use serde::Serialize;
use std::{
    convert::{TryFrom, TryInto},
    fmt::{Display, Formatter},
    io::Write,
    path::PathBuf,
    str::FromStr,
};
use tabled::Tabled;
use url::Url;

#[derive(Debug, Clone)]
pub enum UserPath {
    Url(Url),
    File(PathBuf),
}

impl UserPath {
    pub fn get_mime(&self) -> Result<Mime> {
        Ok(match self {
            Self::Url(url) => Ok(url.try_into()?),
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

/// Render a table of mime types from a list of paths
/// and write it to the given writer
pub fn mime_table<W: Write>(
    writer: &mut W,
    paths: &[UserPath],
    output_json: bool,
    terminal_output: bool,
) -> Result<()> {
    let rows = paths
        .iter()
        .map(UserPathTable::new)
        .collect::<Result<Vec<UserPathTable>>>()?;

    let table = if output_json {
        serde_json::to_string(&rows)?
    } else {
        render_table(&rows, terminal_output)
    };

    writeln!(writer, "{table}")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a vector of UserPaths for testing `mime_table`
    fn paths() -> Result<Vec<UserPath>> {
        [
            "tests",
            "tests/cat",
            "tests/cmus.desktop",
            "tests/empty.txt",
            "tests/no_html_tags.html",
            "tests/org.wezfurlong.wezterm.desktop",
            "tests/p.html",
            "tests/rust.vim",
            "tests/SettingsWidgetFdoSecrets.ui",
            "https://duckduckgo.com",
            ".",
            "../README.md",
        ]
        .iter()
        .map(|p| UserPath::from_str(p))
        .collect()
    }

    #[test]
    fn mime_table_terminal() -> Result<()> {
        let mut buffer = Vec::new();
        mime_table(&mut buffer, &paths()?, false, true)?;
        goldie::assert!(String::from_utf8(buffer)?);
        Ok(())
    }

    #[test]
    fn test_mime_table_piped() -> Result<()> {
        let mut buffer = Vec::new();
        mime_table(&mut buffer, &paths()?, false, false)?;
        goldie::assert!(String::from_utf8(buffer)?);
        Ok(())
    }

    #[test]
    fn test_mime_table_json() -> Result<()> {
        //NOTE: both calls should have the same result
        // JSON output and terminal output
        let mut buffer = Vec::new();
        mime_table(&mut buffer, &paths()?, true, true)?;
        goldie::assert!(String::from_utf8(buffer)?);

        // JSON output and no terminal output
        let mut buffer = Vec::new();
        mime_table(&mut buffer, &paths()?, true, false)?;
        goldie::assert!(String::from_utf8(buffer)?);

        Ok(())
    }
}
