use crate::{
    common::{RegexApps, RegexHandler, UserPath},
    error::Result,
};
use serde::{Deserialize, Serialize};

/// The config file
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether to enable the selector when multiple handlers are set
    pub enable_selector: bool,
    /// The selector command to run
    pub selector: String,
    /// Regex handlers
    // NOTE: Serializing is only necessary for generating a default config file
    #[serde(skip_serializing)]
    pub handlers: RegexApps,
    /// Extra arguments to pass to terminal application
    pub term_exec_args: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enable_selector: false,
            selector: "rofi -dmenu -i -p 'Open With: '".into(),
            handlers: Default::default(),
            // Required for many xterm-compatible terminal emulators
            // Unfortunately, messes up emulators that don't accept it
            term_exec_args: Some("-e".into()),
        }
    }
}

impl Config {
    /// Get the handler associated with a given mime from the config file's regex handlers
    pub fn get_regex_handler(&self, path: &UserPath) -> Result<RegexHandler> {
        self.handlers.get_handler(path)
    }

    /// Load ~/.config/handlr/handlr.toml
    pub fn load() -> Result<Self> {
        Ok(confy::load("handlr")?)
    }

    /// Determine whether or not the selector should be enabled
    pub fn use_selector(
        &self,
        enable_selector: bool,
        disable_selector: bool,
    ) -> bool {
        (self.enable_selector || enable_selector) && !disable_selector
    }
}
