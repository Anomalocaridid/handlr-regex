use crate::{
    apps::SystemApps, common::DesktopHandler, Error, ErrorKind, Handleable,
    RegexHandler, Result,
};
use mime::Mime;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub static CONFIG: Lazy<Config> = Lazy::new(Config::load);

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
    pub handlers: Vec<RegexHandler>,
    /// Extra arguments to pass to terminal application
    term_exec_args: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enable_selector: false,
            selector: "rofi -dmenu -i -p 'Open With: '".into(),
            handlers: Vec::new(),
            // Required for many xterm-compatible terminal emulators
            // Unfortunately, messes up emulators that don't accept it
            term_exec_args: Some("-e".into()),
        }
    }
}

impl Config {
    pub fn terminal() -> Result<String> {
        let terminal_entry = crate::apps::APPS
            .get_handler(&Mime::from_str("x-scheme-handler/terminal").unwrap())
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

                let mut apps = (*crate::apps::APPS).clone();
                apps.set_handler(
                    Mime::from_str("x-scheme-handler/terminal").unwrap(),
                    DesktopHandler::assume_valid(entry.0),
                );
                apps.save().ok()?;

                Some(entry.1)
            })
            .map(|e| {
                let mut exec = e.exec.to_owned();

                if let Some(opts) = &CONFIG.term_exec_args {
                    exec.push(' ');
                    exec.push_str(opts)
                }

                exec
            })
            .ok_or(Error::from(ErrorKind::NoTerminal))
    }
    pub fn load() -> Self {
        confy::load("handlr").unwrap()
    }

    pub fn select<O: Iterator<Item = String>>(
        &self,
        mut opts: O,
    ) -> Result<String> {
        use itertools::Itertools;
        use std::{
            io::prelude::*,
            process::{Command, Stdio},
        };

        let process = {
            let mut split = shlex::split(&self.selector).unwrap();
            let (cmd, args) = (split.remove(0), split);
            Command::new(cmd)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?
        };

        let output = {
            process
                .stdin
                .ok_or_else(|| ErrorKind::Selector(self.selector.clone()))?
                .write_all(opts.join("\n").as_bytes())?;

            let mut output = String::with_capacity(24);

            process
                .stdout
                .ok_or_else(|| ErrorKind::Selector(self.selector.clone()))?
                .read_to_string(&mut output)?;

            output.trim_end().to_owned()
        };

        if output.is_empty() {
            Err(Error::from(ErrorKind::Cancelled))
        } else {
            Ok(output)
        }
    }
}
