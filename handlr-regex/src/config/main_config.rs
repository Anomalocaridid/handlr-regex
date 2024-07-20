use mime::Mime;
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    io::{IsTerminal, Write},
    str::FromStr,
};
use tabled::Tabled;

use crate::{
    apps::{DesktopList, MimeApps, SystemApps},
    common::{render_table, DesktopHandler, Handleable, Handler, UserPath},
    config::config_file::ConfigFile,
    error::{Error, ErrorKind, Result},
};

/// A single struct that holds all apps and config.
/// Used to streamline explicitly passing state.
#[derive(Default)]
pub struct Config {
    /// User-configured associations
    mime_apps: MimeApps,
    /// Available applications on the system
    system_apps: SystemApps,
    /// Handlr-specific config file
    config: ConfigFile,
    /// Whether or not stdout is a terminal
    pub terminal_output: bool,
}

impl Config {
    /// Create a new instance of AppsConfig
    pub fn new() -> Result<Self> {
        Ok(Self {
            mime_apps: MimeApps::read()?,
            system_apps: SystemApps::populate()?,
            config: ConfigFile::load()?,
            terminal_output: std::io::stdout().is_terminal(),
        })
    }

    /// Get the handler associated with a given mime
    pub fn get_handler(
        &self,
        mime: &Mime,
        selector: &str,
        use_selector: bool,
    ) -> Result<DesktopHandler> {
        match self
            .mime_apps
            .get_handler_from_user(mime, selector, use_selector)
        {
            Err(e) if matches!(*e.kind, ErrorKind::Cancelled) => Err(e),
            h => h.or_else(|_| self.get_handler_from_added_associations(mime)),
        }
    }

    /// Get the handler associated with a given mime from mimeapps.list's added associations
    /// If there is none, default to the system apps
    fn get_handler_from_added_associations(
        &self,
        mime: &Mime,
    ) -> Result<DesktopHandler> {
        self.mime_apps
            .added_associations
            .get(mime)
            .map_or_else(
                || self.system_apps.get_handler(mime),
                |h| h.front().cloned(),
            )
            .ok_or_else(|| Error::from(ErrorKind::NotFound(mime.to_string())))
    }

    /// Given a mime and arguments, launch the associated handler with the arguments
    /// If a terminal emulator is needed, but not set, one will be chosen and set
    #[mutants::skip] // Cannot test directly, runs external command
    pub fn launch_handler(
        &mut self,
        mime: &Mime,
        args: Vec<UserPath>,
        selector: Option<String>,
        enable_selector: bool,
        disable_selector: bool,
    ) -> Result<()> {
        let selector = selector.unwrap_or(self.config.selector.clone());
        let use_selector =
            self.config.use_selector(enable_selector, disable_selector);

        self.get_handler(mime, &selector, use_selector)?.launch(
            self,
            args.into_iter().map(|a| a.to_string()).collect(),
            &selector,
            use_selector,
        )
    }

    /// Get the handler associated with a given mime
    /// If a terminal emulator is needed, but not set, one will be chosen and set
    #[mutants::skip] // Cannot test directly, depends on system state
    pub fn show_handler<W: Write>(
        &mut self,
        writer: &mut W,
        mime: &Mime,
        output_json: bool,
        selector: Option<String>,
        enable_selector: bool,
        disable_selector: bool,
    ) -> Result<()> {
        let selector = selector.unwrap_or(self.config.selector.clone());
        let use_selector =
            self.config.use_selector(enable_selector, disable_selector);

        let handler = self.get_handler(mime, &selector, use_selector)?;

        let output = if output_json {
            let entry = handler.get_entry()?;
            let cmd = entry.get_cmd(self, vec![], &selector, use_selector)?;

            (serde_json::json!( {
                "handler": handler.to_string(),
                "name": entry.name,
                "cmd": cmd.0 + " " + &cmd.1.join(" "),
            }))
            .to_string()
        } else {
            handler.to_string()
        };
        writeln!(writer, "{output}")?;
        Ok(())
    }

    /// Set a default application association, overwriting any existing association for the same mimetype
    /// and writes it to mimeapps.list
    #[mutants::skip] // Cannot test directly, alters system state
    pub fn set_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
    ) -> Result<()> {
        self.mime_apps.set_handler(mime, handler);
        self.mime_apps.save()
    }

    /// Add a handler to an existing default application association
    /// and writes it to mimeapps.list
    #[mutants::skip] // Cannot test directly, alters system state
    pub fn add_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
    ) -> Result<()> {
        self.mime_apps.add_handler(mime, handler);
        self.mime_apps.save()
    }

    /// Open the given paths with their respective handlers
    /// If a terminal emulator is needed, but not set, one will be chosen and set
    #[mutants::skip] // Cannot test directly, alters system state
    pub fn open_paths(
        &mut self,
        paths: &[UserPath],
        selector: Option<String>,
        enable_selector: bool,
        disable_selector: bool,
    ) -> Result<()> {
        let selector = selector.unwrap_or(self.config.selector.clone());
        let use_selector =
            self.config.use_selector(enable_selector, disable_selector);

        let mut handlers: HashMap<Handler, Vec<String>> = HashMap::new();

        for path in paths.iter() {
            handlers
                .entry(self.get_handler_from_path(
                    path,
                    &selector,
                    use_selector,
                )?)
                .or_default()
                .push(path.to_string())
        }

        for (handler, paths) in handlers.into_iter() {
            handler.open(self, paths, &selector, use_selector)?;
        }

        Ok(())
    }

    /// Get the handler associated with a given path
    fn get_handler_from_path(
        &self,
        path: &UserPath,
        selector: &str,
        use_selector: bool,
    ) -> Result<Handler> {
        Ok(if let Ok(handler) = self.config.get_regex_handler(path) {
            handler.into()
        } else {
            self.get_handler(&path.get_mime()?, selector, use_selector)?
                .into()
        })
    }

    /// Get the command for the x-scheme-handler/terminal handler if one is set.
    /// Otherwise, finds a terminal emulator program, sets it as the handler, and makes a notification.
    #[mutants::skip] // Cannot test directly, alters system state
    pub fn terminal(
        &mut self,
        selector: &str,
        use_selector: bool,
    ) -> Result<String> {
        let terminal_entry = self
            .get_handler(
                &Mime::from_str("x-scheme-handler/terminal")?,
                selector,
                use_selector,
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

                self.mime_apps.set_handler(
                    &Mime::from_str("x-scheme-handler/terminal").ok()?,
                    &DesktopHandler::assume_valid(entry.0),
                );
                self.mime_apps.save().ok()?;

                Some(entry.1)
            })
            .map(|e| {
                let mut exec = e.exec.to_owned();

                if let Some(opts) = &self.config.term_exec_args {
                    exec.push(' ');
                    exec.push_str(opts)
                }

                exec
            })
            .ok_or(Error::from(ErrorKind::NoTerminal))
    }

    /// Print the set associations and system-level associations in a table
    pub fn print<W: Write>(
        &self,
        writer: &mut W,
        detailed: bool,
        output_json: bool,
    ) -> Result<()> {
        let mimeapps_table = MimeAppsTable::new(
            &self.mime_apps,
            &self.system_apps,
            self.terminal_output,
        );

        if detailed {
            if output_json {
                writeln!(writer, "{}", serde_json::to_string(&mimeapps_table)?)?
            } else {
                writeln!(writer, "Default Apps")?;
                writeln!(
                    writer,
                    "{}",
                    render_table(
                        &mimeapps_table.default_apps,
                        self.terminal_output
                    )
                )?;
                if !self.mime_apps.added_associations.is_empty() {
                    writeln!(writer, "Added associations")?;
                    writeln!(
                        writer,
                        "{}",
                        render_table(
                            &mimeapps_table.added_associations,
                            self.terminal_output
                        )
                    )?;
                }
                writeln!(writer, "System Apps")?;
                writeln!(
                    writer,
                    "{}",
                    render_table(
                        &mimeapps_table.system_apps,
                        self.terminal_output
                    )
                )?
            }
        } else if output_json {
            writeln!(
                writer,
                "{}",
                serde_json::to_string(&mimeapps_table.default_apps)?
            )?
        } else {
            writeln!(
                writer,
                "{}",
                render_table(
                    &mimeapps_table.default_apps,
                    self.terminal_output
                )
            )?
        }

        Ok(())
    }

    /// Entirely remove a given mime's default application association
    #[mutants::skip] // Cannot test directly, alters system state
    pub fn unset_handler(&mut self, mime: &Mime) -> Result<()> {
        if self.mime_apps.unset_handler(mime).is_some() {
            self.mime_apps.save()?
        }

        Ok(())
    }

    /// Remove a given handler from a given mime's default file associaion
    #[mutants::skip] // Cannot test directly, alters system state
    pub fn remove_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
    ) -> Result<()> {
        if self.mime_apps.remove_handler(mime, handler).is_some() {
            self.mime_apps.save()?
        }

        Ok(())
    }
}

/// Internal helper struct for turning MimeApps into tabular data
#[derive(PartialEq, Eq, PartialOrd, Ord, Tabled, Serialize)]
struct MimeAppsEntry {
    mime: String,
    #[tabled(display_with("Self::display_handlers", self))]
    handlers: Vec<String>,
    #[tabled(skip)]
    #[serde(skip_serializing)]
    // This field should not appear in any output
    // It is only used for determining how to render output
    separator: String,
}

impl MimeAppsEntry {
    /// Create a new `MimeAppsEntry`
    fn new(
        mime: &Mime,
        handlers: &VecDeque<DesktopHandler>,
        separator: &str,
    ) -> Self {
        Self {
            mime: mime.to_string(),
            handlers: handlers
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>(),
            separator: separator.to_string(),
        }
    }

    /// Display list of handlers as a string
    fn display_handlers(&self) -> String {
        self.handlers.join(&self.separator)
    }
}

/// Internal helper struct for turning MimeApps into tabular data
#[derive(Serialize)]
struct MimeAppsTable {
    added_associations: Vec<MimeAppsEntry>,
    default_apps: Vec<MimeAppsEntry>,
    system_apps: Vec<MimeAppsEntry>,
}

impl MimeAppsTable {
    /// Create a new `MimeAppsTable`
    fn new(
        mimeapps: &MimeApps,
        system_apps: &SystemApps,
        terminal_output: bool,
    ) -> Self {
        // If output is a terminal, optimize for readability
        // Otherwise, if piped, optimize for parseability
        let separator = if terminal_output { ",\n" } else { ", " };

        let to_entries =
            |map: &HashMap<Mime, DesktopList>| -> Vec<MimeAppsEntry> {
                let mut rows = map
                    .iter()
                    .map(|(mime, handlers)| {
                        MimeAppsEntry::new(mime, handlers, separator)
                    })
                    .collect::<Vec<_>>();
                rows.sort_unstable();
                rows
            };
        Self {
            added_associations: to_entries(&mimeapps.added_associations),
            default_apps: to_entries(&mimeapps.default_apps),
            system_apps: to_entries(system_apps),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_mimes() -> Result<()> {
        let mut config = Config::default();
        config.mime_apps.add_handler(
            &Mime::from_str("video/*")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
        );
        config.mime_apps.add_handler(
            &Mime::from_str("video/webm")?,
            &DesktopHandler::assume_valid("brave.desktop".into()),
        );

        assert_eq!(
            config
                .get_handler(&Mime::from_str("video/mp4")?, "", false)?
                .to_string(),
            "mpv.desktop"
        );
        assert_eq!(
            config
                .get_handler(&Mime::from_str("video/asdf")?, "", false)?
                .to_string(),
            "mpv.desktop"
        );
        assert_eq!(
            config
                .get_handler(&Mime::from_str("video/webm")?, "", false)?
                .to_string(),
            "brave.desktop"
        );

        Ok(())
    }

    #[test]
    fn complex_wildcard_mimes() -> Result<()> {
        let mut config = Config::default();
        config.mime_apps.add_handler(
            &Mime::from_str("application/vnd.oasis.opendocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
        );
        config.mime_apps.add_handler(
            &Mime::from_str("application/vnd.openxmlformats-officedocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
        );

        assert_eq!(
            config
                .get_handler(
                    &Mime::from_str("application/vnd.oasis.opendocument.text")?,
                    "",
                    false
                )?
                .to_string(),
            "startcenter.desktop"
        );
        assert_eq!(
            config
                .get_handler(
                    &Mime::from_str("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")?,
                    "",
                    false
                )?
                .to_string(),
            "startcenter.desktop"
        );

        Ok(())
    }

    // Helper command to test the tables of handlers
    // Renders a table with a bunch of arbitrary handlers to a writer
    // TODO: test printing with non-empty system apps too
    fn print_handlers_test<W: Write>(
        buffer: &mut W,
        detailed: bool,
        output_json: bool,
        terminal_output: bool,
    ) -> Result<()> {
        let mut config = Config::default();

        // Add arbitrary video handlers
        config.mime_apps.add_handler(
            &Mime::from_str("video/mp4")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
        );
        config.mime_apps.add_handler(
            &Mime::from_str("video/asdf")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
        );
        config.mime_apps.add_handler(
            &Mime::from_str("video/webm")?,
            &DesktopHandler::assume_valid("brave.desktop".into()),
        );

        // Add arbitrary text handlers
        config.mime_apps.add_handler(
            &Mime::from_str("text/plain")?,
            &DesktopHandler::assume_valid("helix.desktop".into()),
        );
        config.mime_apps.add_handler(
            &Mime::from_str("text/plain")?,
            &DesktopHandler::assume_valid("nvim.desktop".into()),
        );
        config.mime_apps.add_handler(
            &Mime::from_str("text/plain")?,
            &DesktopHandler::assume_valid("kakoune.desktop".into()),
        );

        // Add arbitrary document handlers
        config.mime_apps.add_handler(
            &Mime::from_str("application/vnd.oasis.opendocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
        );
        config.mime_apps.add_handler(
            &Mime::from_str("application/vnd.openxmlformats-officedocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
        );

        // Add arbirtary terminal emulator as an added association
        config
            .mime_apps
            .added_associations
            .entry(Mime::from_str("x-scheme-handler/terminal")?)
            .or_default()
            .push_back(DesktopHandler::assume_valid(
                "org.wezfurlong.wezterm.desktop".into(),
            ));

        // Set terminal output
        config.terminal_output = terminal_output;

        config.print(buffer, detailed, output_json)?;

        Ok(())
    }

    #[test]
    fn print_handlers_default() -> Result<()> {
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, false, false, true)?;
        goldie::assert!(String::from_utf8(buffer)?);
        Ok(())
    }

    #[test]
    fn print_handlers_piped() -> Result<()> {
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, false, false, false)?;
        goldie::assert!(String::from_utf8(buffer)?);
        Ok(())
    }

    #[test]
    fn print_handlers_detailed() -> Result<()> {
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, true, false, true)?;
        goldie::assert!(String::from_utf8(buffer)?);
        Ok(())
    }

    #[test]
    fn print_handlers_detailed_piped() -> Result<()> {
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, true, false, false)?;
        goldie::assert!(String::from_utf8(buffer)?);
        Ok(())
    }

    #[test]
    fn print_handlers_json() -> Result<()> {
        // NOTE: both calls should have the same result
        // JSON output and terminal output
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, false, true, true)?;
        goldie::assert!(String::from_utf8(buffer)?);

        // JSON output and piped
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, false, true, false)?;
        goldie::assert!(String::from_utf8(buffer)?);

        Ok(())
    }

    #[test]
    fn print_handlers_detailed_json() -> Result<()> {
        // NOTE: both calls should have the same result
        // JSON output and terminal output
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, true, true, false)?;
        goldie::assert!(String::from_utf8(buffer)?);

        // JSON output and piped
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, true, true, false)?;
        goldie::assert!(String::from_utf8(buffer)?);

        Ok(())
    }
}
