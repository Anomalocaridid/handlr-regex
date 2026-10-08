use crate::{
    cli::SelectorArgs,
    common::{RegexApps, RegexHandler, UserPath},
    error::Result,
};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// The config file
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    /// Whether to enable the selector when multiple handlers are set
    pub enable_selector: bool,
    /// The selector command to run
    pub selector: String,
    /// Extra arguments to pass to terminal application
    pub term_exec_args: Option<String>,
    /// Whether to expand wildcards when saving mimeapps.list
    pub expand_wildcards: bool,
    /// Regex handlers
    // NOTE: Serializing is only necessary for generating a default config file
    #[serde(skip_serializing)]
    pub handlers: RegexApps,
}

impl Default for ConfigFile {
    fn default() -> Self {
        ConfigFile {
            enable_selector: false,
            selector: "rofi -dmenu -i -p 'Open With: '".into(),
            // Required for many xterm-compatible terminal emulators
            // Unfortunately, messes up emulators that don't accept it
            term_exec_args: Some("-e".into()),
            expand_wildcards: false,
            handlers: Default::default(),
        }
    }
}

impl ConfigFile {
    /// Get the handler associated with a given mime from the config file's regex handlers
    pub fn get_regex_handler(&self, path: &UserPath) -> Result<RegexHandler> {
        self.handlers.get_handler(path)
    }

    /// Load ~/.config/handlr/handlr.toml
    #[mutants::skip] // Cannot test directly, depends on system state
    pub fn load() -> Result<Self> {
        Ok(confy::load("handlr", "handlr")?)
    }

    /// Override the set selector
    /// Currently assumes the config file will never be saved to
    pub fn override_selector(&mut self, selector_args: SelectorArgs) {
        if let Some(selector) = selector_args.selector {
            debug!("Overriding selector command: {}", selector);
            self.selector = selector;
        }

        self.enable_selector = selector_args
            .enable_selector
            .unwrap_or(self.enable_selector);

        debug!("Selector enabled: {}", self.enable_selector);
    }
}
