use crate::{
    common::{DesktopEntry, ExecMode, UserPath},
    error::{ErrorKind, Result},
    CONFIG,
};
use regex::RegexSet;
use serde::Deserialize;
use std::{
    collections::HashMap,
    ffi::OsString,
    hash::{Hash, Hasher},
    ops::Deref,
};

/// Represents a regex handler from the config
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct RegexHandler {
    exec: String,
    #[serde(default)]
    terminal: bool,
    regexes: HandlerRegexSet,
}

impl RegexHandler {
    /// Get the desktop entry associated with the handler
    fn get_entry(&self) -> DesktopEntry {
        // Make a fake DesktopEntry
        DesktopEntry {
            name: String::from(""),
            exec: self.exec.clone(),
            file_name: OsString::from(""),
            terminal: self.terminal,
            mimes: Vec::new(),
            categories: HashMap::new(),
        }
    }

    /// Open the given paths with the handler
    pub fn open(&self, args: Vec<String>) -> Result<()> {
        self.get_entry().exec(ExecMode::Open, args)
    }

    /// Test if a given path matches the handler's regex
    fn is_match(&self, path: &str) -> bool {
        self.regexes.is_match(path)
    }
}

// Wrapping RegexSet in a struct and implementing Eq and Hash for it
// saves us from having to implement them for RegexHandler as a whole.
#[derive(Debug, Clone, Deserialize)]
struct HandlerRegexSet(#[serde(with = "serde_regex")] RegexSet);

impl PartialEq for HandlerRegexSet {
    fn eq(&self, other: &Self) -> bool {
        self.patterns() == other.patterns()
    }
}

impl Eq for HandlerRegexSet {}

impl Hash for HandlerRegexSet {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.patterns().hash(state);
    }
}

// Makes it more convenient to call the underlying RegexSet's methods
impl Deref for HandlerRegexSet {
    type Target = RegexSet;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct RegexApps(Vec<RegexHandler>);

impl RegexApps {
    /// Create a RegexApps from the config's regex handlers
    pub fn populate() -> Self {
        RegexApps(CONFIG.handlers.clone())
    }

    /// Get a handler matching a given path
    pub fn get_handler(&self, path: &UserPath) -> Result<RegexHandler> {
        Ok(self
            .0
            .iter()
            .find(|app| app.is_match(&path.to_string()))
            .ok_or_else(|| ErrorKind::NotFound(path.to_string()))?
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[test]
    fn regex_handlers() -> Result<()> {
        let exec: &str = "freetube %u";
        let regexes: &[String] =
            &[String::from(r"(https://)?(www\.)?youtu(be\.com|\.be)/*")];

        let regex_handler = RegexHandler {
            exec: String::from(exec),
            terminal: false,
            regexes: HandlerRegexSet(
                RegexSet::new(regexes).expect("Test regex is invalid"),
            ),
        };

        let regex_apps = RegexApps(vec![regex_handler.clone()]);

        assert_eq!(
            regex_apps
                .get_handler(&UserPath::Url(
                    Url::parse("https://youtu.be/dQw4w9WgXcQ").unwrap()
                ))
                .expect("RegexApps::get_handler() returned Err"),
            regex_handler
        );

        assert!(regex_apps
            .get_handler(&UserPath::Url(
                Url::parse("https://en.wikipedia.org").unwrap()
            ))
            .is_err());

        Ok(())
    }
}
