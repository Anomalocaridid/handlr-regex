use crate::{
    config::Config,
    error::{Error, Result},
};
use freedesktop_desktop_entry::{
    get_languages_from_env, DesktopEntry as FreeDesktopEntry,
};
use itertools::Itertools;
use mime::Mime;
use std::{
    convert::TryFrom,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
    sync::LazyLock,
};
use tracing::debug;

/// Represents a desktop entry file for an application
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    /// Name of the application
    pub name: String,
    /// Command to execute
    pub exec: String,
    /// Name of the desktop entry file
    pub file_name: OsString,
    /// Whether the program runs in a terminal window
    pub terminal: bool,
    /// The MIME type(s) supported by this application
    pub mime_type: Vec<Mime>,
    /// Categories in which the entry should be shown in a menu
    pub categories: Vec<String>,
}

/// Modes for running a DesktopFile's `exec` command
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Mode {
    /// Launch the command directly, passing arguments given to `handlr`
    Launch,
    /// Open files/urls passed to `handler` with the command
    Open,
}

impl DesktopEntry {
    /// Execute the command in `exec` in the given mode and with the given arguments
    #[mutants::skip] // Cannot test directly, runs external command
    pub fn exec(
        &self,
        config: &Config,
        mode: Mode,
        arguments: Vec<String>,
    ) -> Result<()> {
        let supports_multiple =
            self.exec.contains("%F") || self.exec.contains("%U");
        if arguments.is_empty() {
            self.exec_inner(config, vec![])?
        } else if supports_multiple || mode == Mode::Launch {
            self.exec_inner(config, arguments)?;
        } else {
            for arg in arguments {
                self.exec_inner(config, vec![arg])?;
            }
        };

        Ok(())
    }

    /// Internal helper function for `exec`
    #[mutants::skip] // Cannot test directly, runs command
    fn exec_inner(&self, config: &Config, args: Vec<String>) -> Result<()> {
        let cmd = self.get_cmd(config, args)?;
        debug!("Executing command: \"{}\"", cmd);

        let mut cmd = execute::command(cmd);

        if self.terminal && config.terminal_output {
            cmd.spawn()?.wait()?;
        } else {
            cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        }

        Ok(())
    }

    /// Get the `exec` command, formatted with given arguments
    pub fn get_cmd(
        &self,
        config: &Config,
        args: Vec<String>,
    ) -> Result<String> {
        let special = lazy_regex::regex!("%(f|u)"i);

        let mut exec = self.exec.clone();
        let args = args.join(" ");

        if special.is_match(&exec) {
            exec = special.replace_all(&exec, args).to_string();
        } else {
            // The desktop entry doesn't contain arguments - we make best effort and append them at the end
            exec.push(' ');
            exec.push_str(&args);
        }

        // If the entry expects a terminal (emulator), but this process is not running in one, we launch a new one.
        if self.terminal && !config.terminal_output {
            let mut term_cmd = config.terminal()?;
            term_cmd.push(' ');
            term_cmd.push_str(&exec);
            exec = term_cmd;
        }

        Ok(exec.trim().to_string())
    }

    /// Parse a desktop entry file, given a path
    fn parse_file(path: &Path) -> Option<DesktopEntry> {
        // Assume the set locales will not change while handlr is running
        static LOCALES: LazyLock<Vec<String>> =
            LazyLock::new(get_languages_from_env);

        let fd_entry =
            FreeDesktopEntry::from_path(path.to_path_buf(), &LOCALES).ok()?;

        let entry = DesktopEntry {
            name: fd_entry.name(&LOCALES)?.into_owned(),
            exec: fd_entry.exec()?.to_owned(),
            file_name: path.file_name()?.to_owned(),
            terminal: fd_entry.terminal(),
            mime_type: fd_entry
                .mime_type()
                .unwrap_or_default()
                .iter()
                .filter_map(|m| Mime::from_str(m).ok())
                .collect_vec(),
            categories: fd_entry
                .categories()
                .unwrap_or_default()
                .iter()
                .map(|&c| c.to_owned())
                .collect_vec(),
        };

        if !entry.name.is_empty() && !entry.exec.is_empty() {
            Some(entry)
        } else {
            None
        }
    }

    /// Make a fake DesktopEntry given only a value for exec and terminal.
    /// All other keys will have default values.
    pub fn fake_entry(exec: &str, terminal: bool) -> DesktopEntry {
        DesktopEntry {
            exec: exec.to_owned(),
            terminal,
            ..Default::default()
        }
    }

    /// Check if the given desktop entry represents a terminal emulator
    pub fn is_terminal_emulator(&self) -> bool {
        self.categories.contains(&"TerminalEmulator".to_string())
    }
}

impl TryFrom<PathBuf> for DesktopEntry {
    type Error = Error;
    fn try_from(path: PathBuf) -> Result<Self> {
        Self::parse_file(&path).ok_or(Error::BadEntry(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::DesktopHandler;
    use similar_asserts::assert_eq;

    // Helper function to test getting the command from the Exec field
    fn test_get_cmd(
        entry: &DesktopEntry,
        config: &Config,
        expected_command: &str,
    ) -> Result<()> {
        assert_eq!(
            entry.get_cmd(config, vec!["test".to_string()])?,
            expected_command
        );
        Ok(())
    }

    #[test]
    fn complex_exec() -> Result<()> {
        // Note that this entry also has no category key
        let entry =
            DesktopEntry::try_from(PathBuf::from("tests/assets/cmus.desktop"))?;
        assert_eq!(entry.mime_type.len(), 2);
        assert_eq!(entry.mime_type[0].essence_str(), "audio/mp3");
        assert_eq!(entry.mime_type[1].essence_str(), "audio/ogg");
        assert!(!entry.is_terminal_emulator());

        test_get_cmd(
            &entry,
            &Config::default(),
            "bash -c \"(! pgrep cmus && tilix -e cmus && tilix -a session-add-down -e cava); sleep 0.1 && cmus-remote -q test\""
        )
    }

    #[test]
    fn terminal_emulator() -> Result<()> {
        let entry = DesktopEntry::try_from(PathBuf::from(
            "tests/assets/org.wezfurlong.wezterm.desktop",
        ))?;
        assert!(entry.mime_type.is_empty());
        assert!(entry.is_terminal_emulator());

        test_get_cmd(&entry, &Config::default(), "wezterm start --cwd . test")
    }

    #[test]
    fn invalid_desktop_entries() -> Result<()> {
        let empty_name = DesktopEntry::try_from(PathBuf::from(
            "tests/assets/empty_name.desktop",
        ));

        assert!(empty_name.is_err());

        let empty_exec = DesktopEntry::try_from(PathBuf::from(
            "tests/assets/empty_exec.desktop",
        ));

        assert!(empty_exec.is_err());

        Ok(())
    }

    #[test]
    fn terminal_application_command() -> Result<()> {
        let mut config = Config::default();

        config.terminal_output = false;

        config.add_handler(
            &Mime::from_str("x-scheme-handler/terminal")?,
            &DesktopHandler::assume_valid(
                "tests/assets/org.wezfurlong.wezterm.desktop".into(),
            ),
        )?;

        let entry = DesktopEntry::try_from(PathBuf::from(
            "tests/assets/Helix.desktop",
        ))?;

        test_get_cmd(&entry, &config, "wezterm start --cwd . -e hx test")
    }
}
