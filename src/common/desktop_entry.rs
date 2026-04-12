use crate::{
    config::{Config, Languages},
    error::{Error, Result},
};
use freedesktop_entry_parser::{Entry, Section};
use itertools::Itertools;
use mime::Mime;
use std::{ffi::OsString, path::Path, process::Stdio, str::FromStr};
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

    /// Backing Desktop Entry loaded from `file_name`.
    /// Other fields in this struct are extracted from this `entry` for frequent use.
    ///
    /// Empty if no real Desktop Entry file, for example for tests or regex match.
    pub(crate) entry: Option<Entry>,
}

/// `[Desktop Entry]` in `.desktop` file
const MAIN_SECTION: &str = "Desktop Entry";

impl DesktopEntry {
    /// Main Section `[Desktop Entry]` in the `.desktop` file.
    /// Contains keys like `Name` or `Exec`.
    fn main_section(&self) -> Option<&Section> {
        self.entry.as_ref()?.section(MAIN_SECTION)
    }

    /// Note: A section can contain a `key` multiple times. This here returns only the first occurrence!
    fn get_first_in_section<'s>(section: &'s Section, key: &str) -> Option<&'s str> {
        section
            .attr(key)
            .first()
            .map(String::as_str)
    }
    /// Returns `None` if no such key in the `MAIN_SECTION`.
    ///
    /// Note: If key exist, but is empty, it returns `Some("")`.
    /// 
    /// Note: A `key` can occur multiple times. This here returns only the first one!
    pub(crate) fn get_first(&self, key: &str) -> Option<&str> {
        DesktopEntry::get_first_in_section(self.main_section()?, key)
    }
    fn get_first_with_language_in_section<'s>(section: &'s Section, key: &str, languages: &Languages) -> Option<&'s str> {
        languages
            .iter()
            .find_map(|lang| section.attr_with_param(key, lang).first())
            .or_else(|| section.attr(key).first())
            .map(String::as_str)
    }
    /// While `get_first` returns the first match without language, this tries to return a localized value in order of `languages`.
    /// Falls back to unlocalized key (-> same as `get_first`)
    /// 
    /// # Example
    /// If `entry` contains the following names: 
    /// ```
    /// Name=VLC media player
    /// Name[de]=VLC Media Player
    /// Name[fr]=Lecteur multimédia VLC
    /// ```
    /// `get_first_with_language` for `[it,de,fr]` returns the german name `VLC Media Player`,
    ///     while `[it,pl]` returns the default `VLC media player`
    pub(crate) fn get_first_with_language(
        &self,
        key: &str,
        languages: &Languages,
    ) -> Option<&str> {
        DesktopEntry::get_first_with_language_in_section(self.main_section()?, key, languages)
    }
    /// While `get_first` returns only the first key occurrence, this returns all.
    pub(crate) fn get_all(&self, key: &str) -> Option<&[String]> {
        self.main_section().map(|s| s.attr(key))
    }
    /// Besides getting all key occurrences (like `get_all`), it further splits the values at the passed separator.
    /// 
    /// # Example
    /// `Categories=AudioVideo;Player;` is split into `["AudioVideo", "Player"]`
    pub(crate) fn get_all_separated<'e>(
        &'e self,
        entry_name: &str,
        separator: &'e str,
    ) -> Option<impl Iterator<Item = &'e str>> {
        let values = self.get_all(entry_name)?;
        Some(values.iter().flat_map(move |v| {
            v.split(separator)
                .filter(|s| !s.is_empty())  // Account for ending/duplicated semicolons
                .unique()   // Remove duplicate entries
        }))
    }
}

impl DesktopEntry {
    /// The MIME type(s) supported by this application
    pub fn mime_type(&self) -> Option<impl Iterator<Item = Mime> + use<'_>> {
        let ms = self.get_all_separated("MimeType", ";")?
            .flat_map(|m| Mime::from_str(m).ok());
        Some(ms)
    }
    /// Categories in which the entry should be shown in a menu
    pub fn categories(&self) -> Option<impl Iterator<Item = &str>> {
        self.get_all_separated("Categories", ";")
    }
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

    pub fn new( path: &Path, entry: Entry, languages: &Languages) -> Result<DesktopEntry> {
        let entry_error = |field_name: &str| -> Error {
            Error::BadEntry(path.to_path_buf(), field_name.to_string())
        };

        let section = entry.section(MAIN_SECTION).ok_or_else(|| entry_error(MAIN_SECTION))?;

        let entry = DesktopEntry {
            name: Self::get_first_with_language_in_section(section, "Name", languages).ok_or_else(|| entry_error("Name"))?.to_string(),
            exec: Self::get_first_in_section(section, "Exec").ok_or_else(|| entry_error("Exec"))?.to_string(),
            file_name: path.file_name().unwrap_or_default().to_owned(),
            terminal: Self::get_first_in_section(section, "Terminal").and_then(|t| t.parse().ok()).unwrap_or(false),
            entry: Some(entry),
        };

        if entry.name.is_empty() {
            Err(entry_error("Name"))
        } else if entry.exec.is_empty() {
            Err(entry_error("Exec"))
        } else {
            Ok(entry)
        }
    }

    /// Parse a desktop entry file, given a path
    pub fn parse_file(
        path: &Path,
        languages: &Languages,
    ) -> Result<DesktopEntry> {
        Self::new(path, Entry::parse_file(path)?, languages)
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
        let Some(mut categories) = self.categories() else { return false };
        categories.contains(&"TerminalEmulator")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
        let entry = DesktopEntry::parse_file(
            &PathBuf::from("tests/assets/cmus.desktop"),
            &Vec::new(),
        )?;
        let mime_type = entry.mime_type().unwrap().collect_vec();
        assert_eq!(mime_type.len(), 2);
        assert_eq!(mime_type[0].essence_str(), "audio/mp3");
        assert_eq!(mime_type[1].essence_str(), "audio/ogg");
        assert!(!entry.is_terminal_emulator());

        test_get_cmd(
            &entry,
            &Config::default(),
            "bash -c \"(! pgrep cmus && tilix -e cmus && tilix -a session-add-down -e cava); sleep 0.1 && cmus-remote -q test\""
        )
    }

    #[test]
    fn terminal_emulator() -> Result<()> {
        let entry = DesktopEntry::parse_file(
            &PathBuf::from("tests/assets/org.wezfurlong.wezterm.desktop"),
            &Vec::new(),
        )?;
        assert!(entry.mime_type().is_none_or(|mt| mt.collect_vec().is_empty()));
        assert!(entry.is_terminal_emulator());

        test_get_cmd(&entry, &Config::default(), "wezterm start --cwd . test")
    }

    #[test]
    fn invalid_desktop_entries() -> Result<()> {
        let languages = Vec::new();

        let empty_name = DesktopEntry::parse_file(
            &PathBuf::from("tests/assets/empty_name.desktop"),
            &languages,
        );

        assert!(empty_name.is_err());

        let empty_exec = DesktopEntry::parse_file(
            &PathBuf::from("tests/assets/empty_exec.desktop"),
            &languages,
        );

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

        let entry = DesktopEntry::parse_file(
            &PathBuf::from("tests/assets/Helix.desktop"),
            &Vec::new(),
        )?;

        test_get_cmd(&entry, &config, "wezterm start --cwd . -e hx test")
    }

    /// Helper function for testing language support
    fn lang_test(languages: &[&str], expected_name: &str) -> Result<()> {
        let entry = DesktopEntry::parse_file(
            &PathBuf::from("tests/assets/vlc.desktop"),
            &languages.iter().map(|s| s.to_string()).collect_vec(),
        )?;

        assert_eq!(entry.name, expected_name);

        Ok(())
    }

    #[test]
    fn language_support() -> Result<()> {
        // No languages
        lang_test(&[], "VLC media player")?;

        // Just one language
        lang_test(&["es"], "Reproductor multimedia VLC")?;

        // Multiple languages
        lang_test(&["ja", "fr", "nl"], "VLCメディアプレイヤー")?;

        // No valid languages
        lang_test(&["qwert", "yuiop"], "VLC media player")?;

        // Some invalid languages
        lang_test(&["asdfg", "hjkl?;", "bn", "hu"], "VLC মিডিয়া প্লেয়ার")?;
        lang_test(&["zxcv", "pa", "it", "ru"], "VLC ਮੀਡਿਆ ਪਲੇਅਰ")?;

        Ok(())
    }
}
