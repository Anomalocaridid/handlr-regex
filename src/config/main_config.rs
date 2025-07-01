// FIXME
#![allow(clippy::mutable_key_type)]

use itertools::Itertools;
use mime::Mime;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt::Display,
    io::Write,
    str::FromStr,
};
use tabled::Tabled;
use tracing::{debug, info};

use crate::{
    apps::{DesktopList, MimeApps, SystemApps},
    cli::SelectorArgs,
    common::{render_table, DesktopHandler, Handleable, Handler, UserPath},
    config::config_file::ConfigFile,
    error::{Error, Result},
};

/// A single struct that holds all apps and config.
/// Used to streamline explicitly passing state.
#[derive(Default, Debug)]
pub struct Config {
    /// User-configured associations
    mime_apps: MimeApps,
    /// Available applications on the system
    system_apps: SystemApps,
    /// Handlr-specific config file
    pub config: ConfigFile,
    /// Whether or not stdout is a terminal
    pub terminal_output: bool,
}

impl Config {
    /// Create a new instance of AppsConfig
    pub fn new(terminal_output: bool) -> Result<Self> {
        let config = ConfigFile::load();

        Ok(Self {
            // Ensure fields individually default rather than making the whole thing fail if one is missing
            mime_apps: MimeApps::read()?,
            system_apps: SystemApps::populate()?,
            config: config?,
            terminal_output,
        })
    }

    /// Get the handler associated with a given mime
    #[mutants::skip] // Cannot test match guard because it relies on user interactivity
    pub fn get_handler(&self, mime: &Mime) -> Result<DesktopHandler> {
        match self.mime_apps.get_handler_from_user(mime, &self.config) {
            Err(e) if matches!(e, Error::Cancelled) => Err(e),
            h => h
                .inspect(|_| {
                    info!("Match found for `{}` in mimeapps.list Default Associations", mime);
                })
                .or_else(|_|{
                    info!("No match for `{}` in mimeapps.list Default Associations", mime);
                    self.get_handler_from_added_associations(mime)}),
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
            .inspect(|_|
                info!("Found matching entry for `{}` in mimeapps.list Added Associations", mime)
            )
            .map_or_else(
                || {
                    info!("No matching entries for `{}` in mimeapps.list Added Associations", mime);
                    self.system_apps.get_handler(mime)
                },
                |h| h.front().cloned(),
            )
            .ok_or_else(|| {
                info!("No matching installed handlers found for `{}`", mime);
                Error::NotFound(mime.to_string())
            })
    }

    /// Given a mime and arguments, launch the associated handler with the arguments
    #[mutants::skip] // Cannot test directly, runs external command
    pub fn launch_handler(&self, mime: &Mime, args: Vec<String>) -> Result<()> {
        info!(
            "Launching handler for `{}` with arguments: {:?}",
            mime, args
        );
        self.get_handler(mime)?
            .launch(self, args.into_iter().map(|a| a.to_string()).collect())?;
        info!("Finished launching handler");
        Ok(())
    }

    /// Get the handler associated with a given mime
    pub fn show_handler<W: Write>(
        &self,
        writer: &mut W,
        mime: &Mime,
        output_json: bool,
    ) -> Result<()> {
        info!("Showing handler for `{}`", mime);
        debug!("JSON output: {}", output_json);

        let handler = self.get_handler(mime)?;

        let output = if output_json {
            let entry = handler.get_entry()?;
            let cmd = entry.get_cmd(self, vec![])?;

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

        info!("Finished showing handler");
        Ok(())
    }

    /// Set a default application association, overwriting any existing association for the same mimetype
    /// and writes it to mimeapps.list
    pub fn set_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
    ) -> Result<()> {
        info!("Setting `{}` as handler for `{}`", handler, mime);

        self.mime_apps.set_handler(
            mime,
            handler,
            self.config.expand_wildcards,
        )?;
        self.mime_apps.save()?;

        info!("Finished setting handler");
        Ok(())
    }

    /// Add a handler to an existing default application association
    /// and writes it to mimeapps.list
    pub fn add_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
    ) -> Result<()> {
        info!("Adding {} to list of handlers for `{}`", handler, mime);

        self.mime_apps.add_handler(
            mime,
            handler,
            self.config.expand_wildcards,
        )?;
        self.mime_apps.save()?;

        info!("Finished adding handler");
        Ok(())
    }

    /// Open the given paths with their respective handlers
    #[mutants::skip] // Cannot test directly, runs external commands
    pub fn open_paths(&self, paths: &[UserPath]) -> Result<()> {
        fn format_paths<T: Display>(paths: &[T]) -> String {
            format!(
                "[{}]",
                paths
                    .iter()
                    .format_with(", ", |str, f| f(&format!("\"{}\"", str)))
            )
        }

        info!("Started opening paths: {}", format_paths(paths));

        for (handler, paths) in
            self.assign_files_to_handlers(paths)?.into_iter()
        {
            debug!("Opening {} using `{}`", format_paths(&paths), handler);
            handler.open(self, paths)?;
        }

        info!("Finished opening paths");

        Ok(())
    }

    /// Helper function to assign files to their respective handlers
    fn assign_files_to_handlers(
        &self,
        paths: &[UserPath],
    ) -> Result<HashMap<Handler, Vec<String>>> {
        let mut handlers: HashMap<Handler, Vec<String>> = HashMap::new();

        for path in paths.iter() {
            handlers
                .entry(self.get_handler_from_path(path)?)
                .or_default()
                .push(path.to_string())
        }

        Ok(handlers)
    }

    /// Get the handler associated with a given path
    fn get_handler_from_path(&self, path: &UserPath) -> Result<Handler> {
        Ok(if let Ok(handler) = self.config.get_regex_handler(path) {
            info!("Using regex handler for `{}`", path);
            handler.into()
        } else {
            info!("No matching regex handlers found for `{}`", path);
            self.get_handler(&path.get_mime()?)?.into()
        })
    }

    /// Get the command for the x-scheme-handler/terminal handler if one is set.
    /// Otherwise, finds a terminal emulator program and uses it.
    // TODO: test falling back to system
    pub fn terminal(&self) -> Result<String> {
        // Get the terminal handler if there is one set
        self.get_handler(&Mime::from_str("x-scheme-handler/terminal")?)
            .ok()
            .and_then(|h| h.get_entry().ok())
            // Otherwise, get a terminal emulator program
            .or_else(|| self.system_apps.terminal_emulator())
            .map(|e| {
                let mut exec = e.exec.to_owned();

                if let Some(opts) = &self.config.term_exec_args {
                    exec.push(' ');
                    exec.push_str(opts)
                }

                exec
            })
            .ok_or(Error::NoTerminal)
    }

    /// Print the set associations and system-level associations in a table
    pub fn print<W: Write>(
        &self,
        writer: &mut W,
        detailed: bool,
        output_json: bool,
    ) -> Result<()> {
        info!("Printing associations");
        debug!("JSON output: {}", output_json);

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

        info!("Finished printing associations");
        Ok(())
    }

    /// Entirely remove a given mime's default application association
    pub fn unset_handler(&mut self, mime: &Mime) -> Result<()> {
        info!("Unsetting handler for `{}`", mime);

        if self.mime_apps.unset_handler(mime).is_some() {
            self.mime_apps.save()?
        }

        info!("Finished unsetting handler");
        Ok(())
    }

    /// Remove a given handler from a given mime's default file associaion
    pub fn remove_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
    ) -> Result<()> {
        info!(
            "Removing `{}` from list of handlers for `{}`",
            handler, mime
        );

        if self.mime_apps.remove_handler(mime, handler).is_some() {
            self.mime_apps.save()?
        }

        info!("Finished removing handler");
        Ok(())
    }

    /// Override the set selector
    /// Currently assumes the config file will never be saved to other than to create an existing one
    pub fn override_selector(&mut self, selector_args: SelectorArgs) {
        self.config.override_selector(selector_args);
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
            |map: &BTreeMap<Mime, DesktopList>| -> Vec<MimeAppsEntry> {
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
            system_apps: to_entries(&system_apps.associations),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use crate::testing;

    use super::*;
    use similar_asserts::assert_eq;

    crate::logs_snapshot_test!(wildcard_mimes, {
        let mut config = Config::default();
        config.add_handler(
            &Mime::from_str("video/*")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
        )?;
        config.add_handler(
            &Mime::from_str("video/webm")?,
            &DesktopHandler::assume_valid("brave.desktop".into()),
        )?;

        assert_eq!(
            config
                .get_handler(&Mime::from_str("video/mp4")?)?
                .to_string(),
            "mpv.desktop"
        );
        assert_eq!(
            config
                .get_handler(&Mime::from_str("video/asdf")?)?
                .to_string(),
            "mpv.desktop"
        );
        assert_eq!(
            config
                .get_handler(&Mime::from_str("video/webm")?)?
                .to_string(),
            "brave.desktop"
        );
    });

    #[test]
    fn complex_wildcard_mimes() -> Result<()> {
        let mut config = Config::default();
        config.add_handler(
            &Mime::from_str("application/vnd.oasis.opendocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
        )?;
        config.add_handler(
            &Mime::from_str("application/vnd.openxmlformats-officedocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
        )?;

        assert_eq!(
            config
                .get_handler(&Mime::from_str(
                    "application/vnd.oasis.opendocument.text"
                )?,)?
                .to_string(),
            "startcenter.desktop"
        );
        assert_eq!(
            config
                .get_handler(
                    &Mime::from_str("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")?,
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
        config.add_handler(
            &Mime::from_str("video/mp4")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
        )?;
        config.add_handler(
            &Mime::from_str("video/asdf")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
        )?;
        config.add_handler(
            &Mime::from_str("video/webm")?,
            &DesktopHandler::assume_valid("brave.desktop".into()),
        )?;

        // Add arbitrary text handlers
        config.add_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("helix.desktop".into()),
        )?;
        config.add_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("nvim.desktop".into()),
        )?;
        config.add_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("kakoune.desktop".into()),
        )?;

        // Add arbitrary document handlers
        config.add_handler(
            &Mime::from_str("application/vnd.oasis.opendocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
        )?;
        config.add_handler(
            &Mime::from_str("application/vnd.openxmlformats-officedocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
        )?;

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

    crate::logs_snapshot_test!(print_handlers_default, {
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, false, false, true)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    crate::logs_snapshot_test!(print_handlers_piped, {
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, false, false, false)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    crate::logs_snapshot_test!(print_handlers_detailed, {
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, true, false, true)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    crate::logs_snapshot_test!(print_handlers_detailed_piped, {
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, true, false, false)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    crate::logs_snapshot_test!(print_handlers_json, {
        // NOTE: both calls should have the same result
        // JSON output and terminal output
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, false, true, true)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);

        // JSON output and piped
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, false, true, false)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    crate::logs_snapshot_test!(print_handlers_detailed_json, {
        // NOTE: both calls should have the same result
        // JSON output and terminal output
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, true, true, false)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);

        // JSON output and piped
        let mut buffer = Vec::new();
        print_handlers_test(&mut buffer, true, true, false)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    crate::logs_snapshot_test!(terminal_command_set, {
        let mut config = Config::default();

        config.add_handler(
            &Mime::from_str("x-scheme-handler/terminal")?,
            &DesktopHandler::from_str(
                "tests/assets/org.wezfurlong.wezterm.desktop",
            )?,
        )?;

        assert_eq!(config.terminal()?, "wezterm start --cwd . -e");
    });

    crate::logs_snapshot_test!(terminal_command_fallback, {
        let mut config = Config::default();

        config
            .system_apps
            .add_unassociated(DesktopHandler::from_str(
                "tests/assets/org.wezfurlong.wezterm.desktop",
            )?);

        assert_eq!(config.terminal()?, "wezterm start --cwd . -e");
    });

    fn test_show_handler<W: Write>(
        writer: &mut W,
        output_json: bool,
        terminal_output: bool,
    ) -> Result<()> {
        let mut config = Config {
            terminal_output,
            ..Default::default()
        };

        // Use actual desktop file because command may be needed
        config.add_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::from_str("tests/assets/Helix.desktop")?,
        )?;

        // May be needed if terminal command is needed
        config.add_handler(
            &Mime::from_str("x-scheme-handler/terminal")?,
            &DesktopHandler::from_str(
                "tests/assets/org.wezfurlong.wezterm.desktop",
            )?,
        )?;

        config.show_handler(writer, &mime::TEXT_PLAIN, output_json)?;

        Ok(())
    }

    // NOTE: result will begin with tests/assets/, which is normal ONLY for tests
    crate::logs_snapshot_test!(show_handler, {
        let mut buffer = Vec::new();
        test_show_handler(&mut buffer, false, false)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    crate::logs_snapshot_test!(show_handler_json, {
        let mut buffer = Vec::new();
        test_show_handler(&mut buffer, true, false)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    // NOTE: result will begin with tests/, which is normal ONLY for tests
    crate::logs_snapshot_test!(show_handler_terminal, {
        let mut buffer = Vec::new();
        test_show_handler(&mut buffer, false, true)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    crate::logs_snapshot_test!(show_handler_json_terminal, {
        let mut buffer = Vec::new();
        test_show_handler(&mut buffer, true, true)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);
    });

    fn test_add_handlers(config: &mut Config) -> Result<()> {
        config.add_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
        )?;

        // Should return first added handler
        assert_eq!(
            config.get_handler(&mime::TEXT_PLAIN)?.to_string(),
            "Helix.desktop"
        );

        config.add_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("nvim.desktop".into()),
        )?;

        // Should still return first added handler
        assert_eq!(
            config.get_handler(&mime::TEXT_PLAIN)?.to_string(),
            "Helix.desktop"
        );

        Ok(())
    }

    fn test_remove_handlers(config: &mut Config) -> Result<()> {
        config.remove_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
        )?;

        // With first added handler removed, second handler replaces it
        assert_eq!(
            config.get_handler(&mime::TEXT_PLAIN)?.to_string(),
            "nvim.desktop"
        );

        config.remove_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("nvim.desktop".into()),
        )?;

        // Both handlers removed, should not be any left
        assert!(config.get_handler(&mime::TEXT_PLAIN).is_err());

        Ok(())
    }

    fn test_set_handlers(config: &mut Config) -> Result<()> {
        config.set_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
        )?;

        assert_eq!(
            config.get_handler(&mime::TEXT_PLAIN)?.to_string(),
            "Helix.desktop"
        );

        config.set_handler(
            &mime::TEXT_PLAIN,
            &DesktopHandler::assume_valid("nvim.desktop".into()),
        )?;

        // Should return second set handler because it should replace the first one
        assert_eq!(
            config.get_handler(&mime::TEXT_PLAIN)?.to_string(),
            "nvim.desktop"
        );

        Ok(())
    }

    fn test_unset_handlers(config: &mut Config) -> Result<()> {
        config.unset_handler(&mime::TEXT_PLAIN)?;

        // Handler completely unset, should not be any left
        assert!(config.get_handler(&mime::TEXT_PLAIN).is_err());

        Ok(())
    }

    crate::logs_snapshot_test!(add_and_remove_handlers, {
        let mut config = Config::default();
        test_add_handlers(&mut config)?;
        test_remove_handlers(&mut config)?;
    });

    crate::logs_snapshot_test!(set_and_unset_handlers, {
        let mut config = Config::default();
        test_set_handlers(&mut config)?;
        test_unset_handlers(&mut config)?;
    });

    crate::logs_snapshot_test!(add_and_unset_handlers, {
        let mut config = Config::default();
        test_add_handlers(&mut config)?;
        test_unset_handlers(&mut config)?;
    });

    crate::logs_snapshot_test!(set_and_remove_handlers, {
        let mut config = Config::default();
        test_set_handlers(&mut config)?;
        test_remove_handlers(&mut config)?;
    });

    crate::logs_snapshot_test!(override_selector, {
        let mut config = Config::default();

        // Ensure defaults are as expected just in case
        assert_eq!(config.config.selector, "rofi -dmenu -i -p 'Open With: '");
        assert_eq!(config.config.enable_selector, false);

        config.override_selector(SelectorArgs {
            selector: Some("fzf".to_string()),
            enable_selector: Some(true),
        });

        assert_eq!(config.config.selector, "fzf");
        assert_eq!(config.config.enable_selector, true);

        config.override_selector(SelectorArgs {
            selector: Some("fuzzel --dmenu --prompt='Open With: '".to_string()),
            enable_selector: Some(false),
        });

        assert_eq!(
            config.config.selector,
            "fuzzel --dmenu --prompt='Open With: '"
        );
        assert_eq!(config.config.enable_selector, false);
    });

    crate::logs_snapshot_test!(dont_override_selector, {
        let mut config = Config::default();

        // Ensure defaults are as expected just in case
        assert_eq!(config.config.selector, "rofi -dmenu -i -p 'Open With: '");
        assert_eq!(config.config.enable_selector, false);

        config.override_selector(SelectorArgs {
            selector: None,
            enable_selector: None,
        });

        assert_eq!(config.config.selector, "rofi -dmenu -i -p 'Open With: '");
        assert_eq!(config.config.enable_selector, false);

        config.override_selector(SelectorArgs {
            selector: None,
            enable_selector: Some(false),
        });

        assert_eq!(config.config.selector, "rofi -dmenu -i -p 'Open With: '");
        assert_eq!(config.config.enable_selector, false);

        // Now repeat with `enable_selector` set to true
        config.config.enable_selector = true;

        config.override_selector(SelectorArgs {
            selector: None,
            enable_selector: Some(true),
        });

        assert_eq!(config.config.selector, "rofi -dmenu -i -p 'Open With: '");
        assert_eq!(config.config.enable_selector, true);

        config.override_selector(SelectorArgs {
            selector: None,
            enable_selector: None,
        });

        assert_eq!(config.config.selector, "rofi -dmenu -i -p 'Open With: '");
        assert_eq!(config.config.enable_selector, true);
    });

    crate::logs_snapshot_test!(properly_assign_files_to_handlers, {
        let mut config = Config::default();
        config.add_handler(
            &Mime::from_str("image/png")?,
            &DesktopHandler::assume_valid("swayimg.desktop".into()),
        )?;
        config.add_handler(
            &Mime::from_str("application/pdf")?,
            &DesktopHandler::assume_valid("mupdf.desktop".into()),
        )?;

        let mut expected_handlers = HashMap::new();
        expected_handlers.insert(
            Handler::new("swayimg.desktop"),
            vec!["tests/assets/a.png".to_owned()],
        );
        expected_handlers.insert(
            Handler::new("mupdf.desktop"),
            vec!["tests/assets/a.pdf".to_owned()],
        );

        assert_eq!(
            config.assign_files_to_handlers(&[
                UserPath::from_str("tests/assets/a.png")?,
                UserPath::from_str("tests/assets/a.pdf")?
            ])?,
            expected_handlers
        );

        assert_eq!(
            config.assign_files_to_handlers(&[
                UserPath::from_str("tests/assets/a.pdf")?,
                UserPath::from_str("tests/assets/a.png")?
            ])?,
            expected_handlers
        );

        let mut expected_handlers = HashMap::new();
        expected_handlers.insert(
            Handler::new("swayimg.desktop"),
            vec![
                "tests/assets/a.png".to_owned(),
                "tests/assets/b.png".to_owned(),
            ],
        );
        expected_handlers.insert(
            Handler::new("mupdf.desktop"),
            vec!["tests/assets/a.pdf".to_owned()],
        );

        assert_eq!(
            config.assign_files_to_handlers(&[
                UserPath::from_str("tests/assets/a.png")?,
                UserPath::from_str("tests/assets/b.png")?,
                UserPath::from_str("tests/assets/a.pdf")?
            ])?,
            expected_handlers
        );

        assert_eq!(
            config.assign_files_to_handlers(&[
                UserPath::from_str("tests/assets/a.pdf")?,
                UserPath::from_str("tests/assets/a.png")?,
                UserPath::from_str("tests/assets/b.png")?
            ])?,
            expected_handlers
        );
    });
}
