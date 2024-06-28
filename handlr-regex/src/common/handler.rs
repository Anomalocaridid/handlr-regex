use crate::{
    common::{DesktopEntry, ExecMode},
    Error, ErrorKind, RegexHandler, Result,
};
use std::{
    convert::TryFrom, ffi::OsString, fmt::Display, path::PathBuf, str::FromStr,
};

/// Represents a handler defined in a desktop file
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct DesktopHandler(OsString);

impl Display for DesktopHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_string_lossy())
    }
}

impl FromStr for DesktopHandler {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::resolve(s.into())
    }
}

impl DesktopHandler {
    pub fn assume_valid(name: OsString) -> Self {
        Self(name)
    }
    pub fn get_path(name: &std::ffi::OsStr) -> Option<PathBuf> {
        let mut path = PathBuf::from("applications");
        path.push(name);
        xdg::BaseDirectories::new().ok()?.find_data_file(path)
    }
    pub fn resolve(name: OsString) -> Result<Self> {
        let path = Self::get_path(&name).ok_or_else(|| {
            ErrorKind::NotFound(name.to_string_lossy().into())
        })?;
        DesktopEntry::try_from(path)?;
        Ok(Self(name))
    }
    pub fn get_entry(&self) -> Result<DesktopEntry> {
        DesktopEntry::try_from(Self::get_path(&self.0).unwrap())
    }
    pub fn launch(&self, args: Vec<String>) -> Result<()> {
        self.get_entry()?.exec(ExecMode::Launch, args)
    }
    pub fn open(&self, args: Vec<String>) -> Result<()> {
        self.get_entry()?.exec(ExecMode::Open, args)
    }
}

/// Represents a program or command that is used to open a file
#[derive(PartialEq, Eq, Hash)]
pub enum Handler {
    DesktopHandler(DesktopHandler),
    RegexHandler(RegexHandler),
}

impl Handler {
    pub fn open(&self, args: Vec<String>) -> Result<()> {
        match self {
            Handler::DesktopHandler(handler) => handler.open(args),
            Handler::RegexHandler(handler) => handler.open(args),
        }
    }
}
