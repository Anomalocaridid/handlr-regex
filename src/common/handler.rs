use crate::{
    common::{DesktopEntry, ExecMode, UserPath},
    config::Config,
    error::{Error, Result},
};
use derive_more::{Deref, Display};
use enum_dispatch::enum_dispatch;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use std::{
    convert::TryFrom,
    ffi::OsString,
    fmt::Display,
    hash::{Hash, Hasher},
    path::PathBuf,
    str::FromStr,
};
use tracing::{debug, info, warn};

/// Represents a program or command that is used to open a file
#[enum_dispatch(Handleable)]
#[derive(Display, Debug, PartialEq, Eq, Hash)]
pub enum Handler {
    DesktopHandler,
    RegexHandler,
}

#[cfg(test)]
impl Handler {
    /// Helper function for testing
    pub fn new(name: &str) -> Self {
        Handler::DesktopHandler(DesktopHandler::assume_valid(name.into()))
    }
}

/// Trait providing common functionality for handlers
#[enum_dispatch]
pub trait Handleable {
    /// Get the desktop entry associated with the handler
    fn get_entry(&self) -> Result<DesktopEntry>;
    /// Open the given paths with the handler
    #[mutants::skip] // Cannot test directly, runs commands
    fn open(&self, config: &Config, args: Vec<String>) -> Result<()> {
        self.get_entry()?.exec(config, ExecMode::Open, args)
    }
}

/// Represents a handler defined in a desktop file
#[derive(
    Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct DesktopHandler(OsString);

impl Display for DesktopHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_string_lossy())
    }
}

impl FromStr for DesktopHandler {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DesktopHandler(s.into()))
    }
}

impl Handleable for DesktopHandler {
    fn get_entry(&self) -> Result<DesktopEntry> {
        DesktopEntry::try_from(Self::get_path(&self.0)?)
    }
}

impl DesktopHandler {
    /// Create a DesktopHandler, skipping validity checks
    pub fn assume_valid(name: OsString) -> Self {
        Self(name)
    }

    /// Get the path of a given desktop entry file
    pub fn get_path(name: &std::ffi::OsStr) -> Result<PathBuf> {
        if cfg!(test) {
            Ok(PathBuf::from(name))
        } else {
            let mut path = PathBuf::from("applications");
            path.push(name);
            Ok(xdg::BaseDirectories::new()?
                .find_data_file(path)
                .ok_or_else(|| {
                    Error::NotFound(name.to_string_lossy().into())
                })?)
        }
    }

    /// Launch a DesktopHandler's desktop entry
    #[mutants::skip] // Cannot test directly, runs command
    pub fn launch(&self, config: &Config, args: Vec<String>) -> Result<()> {
        info!("Launching `{}` with args: {:?}", self, args);
        self.get_entry()?.exec(config, ExecMode::Launch, args)
    }

    /// Issue a warning if the given handler is invalid
    pub fn warn_if_invalid(&self) {
        if let Err(e) = self.get_entry() {
            warn!("The desktop entry `{}` is invalid: {}", self, e);
        }
    }
}

/// Represents a regex handler from the config
#[derive(Display, Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[display(fmt = "\"{}\" (Regex Handler)", exec)]
pub struct RegexHandler {
    exec: String,
    #[serde(default)]
    terminal: bool,
    regexes: Vec<Regex>,
}

impl RegexHandler {
    /// Test if a given path matches the handler's regex
    fn is_match(&self, path: &str) -> bool {
        let matches = self
            .regexes
            .iter()
            .filter(|regex| regex.is_match(path))
            .collect_vec();

        let is_match = !matches.is_empty();

        if is_match {
            debug!(
                "Regex matches found in `{}` for `{}`: {:?}",
                self, path, matches
            );
        } else {
            debug!("No regex matches found in `{}` for `{}`", self, path);
        }

        is_match
    }
}

impl Handleable for RegexHandler {
    fn get_entry(&self) -> Result<DesktopEntry> {
        Ok(DesktopEntry::fake_entry(&self.exec, self.terminal))
    }
}

/// Wrapper struct needed because `Regex` does not implement Eq, Hash, or Deserialize
#[serde_as]
#[derive(Debug, Clone, Deref, Deserialize)]
struct Regex(#[serde_as(as = "DisplayFromStr")] lazy_regex::Regex);

impl PartialEq for Regex {
    #[mutants::skip] // Trivial
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for Regex {}

impl Hash for Regex {
    #[mutants::skip] // Trivial
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

/// A collection of all of the defined RegexHandlers
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RegexApps(Vec<RegexHandler>);

impl RegexApps {
    /// Get a handler matching a given path
    pub fn get_handler(&self, path: &UserPath) -> Result<RegexHandler> {
        Ok(self
            .0
            .iter()
            .find(|app| app.is_match(&path.to_string()))
            .ok_or_else(|| Error::NotFound(path.to_string()))?
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::DesktopEntry;
    use url::Url;

    crate::logs_snapshot_test!(regex_handlers, {
        let exec: &str = "freetube %u";
        let regex_handler = RegexHandler {
            exec: String::from(exec),
            terminal: false,
            regexes: [String::from(
                r"(https://)?(www\.)?youtu(be\.com|\.be)/*",
            )]
            .iter()
            .map(|s| {
                Regex(
                    lazy_regex::Regex::new(s)
                        .expect("Hardcoded regex should be valid"),
                )
            })
            .collect(),
        };

        println!("{:#?}", regex_handler);

        let regex_apps = RegexApps(vec![regex_handler.clone()]);

        assert_eq!(
            regex_apps
                .get_handler(&UserPath::Url(Url::parse(
                    "https://youtu.be/dQw4w9WgXcQ"
                )?))?
                .get_entry()?,
            DesktopEntry {
                exec: exec.to_string(),
                terminal: false,
                ..Default::default()
            }
        );

        assert!(regex_apps
            .get_handler(&UserPath::Url(Url::parse(
                "https://en.wikipedia.org",
            )?))
            .is_err());
    });
}
