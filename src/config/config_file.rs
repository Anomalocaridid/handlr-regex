use crate::{
    cli::SelectorArgs,
    common::{RegexApps, RegexHandler, UserPath},
    error::Result,
};
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectorConfig {
    /// Whether to enable the selector when multiple handlers are set
    pub enabled: bool,
    /// The selector command to run
    pub command: String,
    /// The format for each handler passed to `command`.
    ///
    /// Defaults to the handler name (`{Name}`).
    ///
    /// Note: `\0` is not valid inside a TOML document. Instead use its Unicode representation (like `\u0000`).
    pub handler_format: String,
    /// Value to match the result from `command` with a handler.
    /// Should be used if the returned value from command is different from the input (`handler_format`]).
    ///
    /// # Example
    /// `selector` calls rofi with `rofi -dmenu -show-icons -i -p 'Open With:'`
    /// and `handler_format` is `{Name}\x00icon\x1f{Icon}`.
    /// Rofi doesn't return the full input, but only the text part and doesn't include the icon.
    ///
    /// So for a handler for Helix the input is `Helix\x00icon\x1fhelix`, but when selected rofi outputs just `Helix`.
    /// `handler_identifier = "{Name}"` matches the return value directly.
    ///
    /// The above actually works without specifying `handler_identifier`:
    /// Matching selector output with an handler without a `handler_identifier` doesn't just trivially match input with output,
    /// but additional tries to match the input before any control chars. This rule would correctly identify the handler.
    ///
    /// `rofi -demnu -i -p 'Open With:' -format 'i'` returns the selected index instead of the selected text.
    /// In this case `handler_identifier = {%Index0}` is required!
    pub handler_identifier: Option<String>,
    /// Separator between handlers when passed to `command`.
    ///
    /// Defaults to `\n`
    pub handler_separator: String,
}
impl Default for SelectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "rofi -dmenu -i -p 'Open With:'".to_string(),
            handler_format: "{Name}".to_string(),
            handler_identifier: None,
            handler_separator: "\n".to_string(),
        }
    }
}

/// The config file
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    /// Configuration for Selector command
    pub selector: SelectorConfig,

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
            selector: Default::default(),
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
        if let Some(enabled) = selector_args.selector_enabled {
            debug!("Overriding selector enabled: {}", enabled);
            self.selector.enabled = enabled;
        }

        if let Some(command) = selector_args.selector_command {
            debug!("Overriding selector command: {}", command);
            self.selector.command = command;
        }

        if let Some(handler_format) = selector_args.selector_handler_format {
            debug!("Overriding selector handler format: {}", handler_format);
            self.selector.handler_format = handler_format;
        }

        if let Some(handler_identifier) =
            selector_args.selector_handler_identifier
        {
            debug!(
                "Overriding selector handler identifier: {}",
                handler_identifier
            );
            self.selector.handler_identifier = Some(handler_identifier);
        }

        if let Some(handler_separator) =
            selector_args.selector_handler_separator
        {
            debug!(
                "Overriding selector handler separator: {}",
                handler_separator
            );
            self.selector.handler_separator = handler_separator;
        }

        debug!("Selector enabled: {}", self.selector.enabled);
    }
}
