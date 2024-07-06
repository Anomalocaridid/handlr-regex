use crate::{
    apps::SystemApps, common::DesktopHandler, Error, ErrorKind, Handleable,
    MimeApps, RegexApps, RegexHandler, Result, UserPath,
};
use mime::Mime;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

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
    term_exec_args: Option<String>,
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

    /// Get the command for the x-scheme-handler/terminal handler if one is set.
    /// Otherwise, finds a terminal emulator program, sets it as the handler, and makes a notification.
    pub fn terminal(
        &self,
        mime_apps: &mut MimeApps,
        system_apps: &SystemApps,
        selector: &str,
        enable_selector: bool,
    ) -> Result<String> {
        let terminal_entry = mime_apps
            .get_handler(
                system_apps,
                &Mime::from_str("x-scheme-handler/terminal")?,
                selector,
                enable_selector,
            )
            .ok()
            .and_then(|h| h.get_entry().ok());

        terminal_entry
            .or_else(|| {
                let entry = SystemApps::get_entries()
                    .ok()?
                    .find(|(_handler, entry)| {
                        entry.is_terminal_emulator()
                    })?;

                crate::utils::notify(
                    "handlr",
                    &format!(
                        "Guessed terminal emulator: {}.\n\nIf this is wrong, use `handlr set x-scheme-handler/terminal` to update it.",
                        entry.0.to_string_lossy()
                    )
                ).ok()?;

                mime_apps.set_handler(
                    &Mime::from_str("x-scheme-handler/terminal").ok()?,
                    &DesktopHandler::assume_valid(entry.0),
                );
                mime_apps.save().ok()?;

                Some(entry.1)
            })
            .map(|e| {
                let mut exec = e.exec.to_owned();

                if let Some(opts) = &self.term_exec_args {
                    exec.push(' ');
                    exec.push_str(opts)
                }

                exec
            })
            .ok_or(Error::from(ErrorKind::NoTerminal))
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
