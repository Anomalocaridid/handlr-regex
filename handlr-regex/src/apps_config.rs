use crate::{
    apps::{MimeApps, SystemApps},
    config::Config,
    error::Result,
};

/// A single struct that holds all apps and config.
/// Used to streamline explicitly passing state.
#[derive(Default)]
pub struct AppsConfig {
    pub mime_apps: MimeApps,
    pub system_apps: SystemApps,
    pub config: Config,
}

impl AppsConfig {
    pub fn new() -> Result<Self> {
        Ok(Self {
            mime_apps: MimeApps::read()?,
            system_apps: SystemApps::populate()?,
            config: Config::load()?,
        })
    }
}
