use crate::{
    common::{RegexApps, RegexHandler, UserPath},
    error::Result,
};
use serde::{Deserialize, Serialize};

/// The config file
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
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

impl Default for ConfigFile {
    fn default() -> Self {
        ConfigFile {
            enable_selector: false,
            selector: "rofi -dmenu -i -p 'Open With: '".into(),
            handlers: Default::default(),
            // Required for many xterm-compatible terminal emulators
            // Unfortunately, messes up emulators that don't accept it
            term_exec_args: Some("-e".into()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_use_selector() -> Result<()> {
        let mut config_file = ConfigFile {
            enable_selector: true,
            ..Default::default()
        };

        assert!(config_file.use_selector(true, false));
        assert!(config_file.use_selector(false, false));
        assert!(!config_file.use_selector(false, true));
        assert!(!config_file.use_selector(true, true));

        config_file.enable_selector = false;

        assert!(config_file.use_selector(true, false));
        assert!(!config_file.use_selector(false, false));
        assert!(!config_file.use_selector(false, true));
        assert!(!config_file.use_selector(true, true));

        Ok(())
    }
}
