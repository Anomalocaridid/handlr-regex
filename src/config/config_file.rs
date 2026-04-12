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
    /// The format for each handler passed to `selector`.
    /// Defaults to the handler name (`{Name}`).
    ///
    /// Note: `\0` is not valid inside a TOML document. Instead use its Unicode representation (like `\u0000`).
    pub selector_handler_format: String,
    /// Separator between handlers when passed to `selector`.
    /// Defaults to `\n`
    pub selector_handler_separator: String,
    /// Value to match the result from `selector` with a handler.
    /// Should be used if the returned value from `selector` is different from the input (`handler_format`).
    ///
    /// # Example
    /// `selector` calls rofi with `rofi -dmenu -show-icons -i -p 'Open With:'`
    /// and `handler_format` is `{Name}\u0000icon\x1f{Icon}`.
    /// Rofi doesn't return the full input, but only the text part and doesn't include the icon.
    ///
    /// So for a handler for Helix the input is `Helix\u000icon\x1ffhelix`, but when selected rofi outputs just `Helix`.
    /// `handler_identifier = "{Name}"` matches the return value directly.
    ///
    /// The above actually works without specifying `handler_identifier`:
    /// Matching selector output with an handler without a `handler_identifier` doesn't just trivially match input with output,
    /// but additional tries to match the input before any control chars. This rule would correctly identify the handler.
    ///
    /// `rofi -demnu -i -p 'Open With:' -format 'i'` returns the selected index instead of the selected text.
    /// In this case `handler_identifier = {%Index0}` is required!
    pub selector_handler_identifier: Option<String>,

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
            selector_handler_format: "{Name}".to_string(),
            selector_handler_separator: "\n".to_string(),
            selector_handler_identifier: None,
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
        Ok(confy::load("handlr", None)?)
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
