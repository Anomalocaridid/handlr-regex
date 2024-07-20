use crate::{
    config::Config,
    error::{Error, ErrorKind, Result},
};
use aho_corasick::AhoCorasick;
use freedesktop_desktop_entry::{
    get_languages_from_env, DesktopEntry as FreeDesktopEntry,
};
use itertools::Itertools;
use mime::Mime;
use once_cell::sync::Lazy;
use std::{
    convert::TryFrom,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
};

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
        config: &mut Config,
        mode: Mode,
        arguments: Vec<String>,
        selector: &str,
        use_selector: bool,
    ) -> Result<()> {
        let supports_multiple =
            self.exec.contains("%F") || self.exec.contains("%U");
        if arguments.is_empty() {
            self.exec_inner(config, vec![], selector, use_selector)?
        } else if supports_multiple || mode == Mode::Launch {
            self.exec_inner(config, arguments, selector, use_selector)?;
        } else {
            for arg in arguments {
                self.exec_inner(config, vec![arg], selector, use_selector)?;
            }
        };

        Ok(())
    }

    /// Internal helper function for `exec`
    #[mutants::skip] // Cannot test directly, runs command
    fn exec_inner(
        &self,
        config: &mut Config,
        args: Vec<String>,
        selector: &str,
        use_selector: bool,
    ) -> Result<()> {
        let mut cmd = {
            let (cmd, args) =
                self.get_cmd(config, args, selector, use_selector)?;
            let mut cmd = Command::new(cmd);
            cmd.args(args);
            cmd
        };

        if self.terminal && config.terminal_output {
            cmd.spawn()?.wait()?;
        } else {
            cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        }

        Ok(())
    }

    /// Get the `exec` command, formatted with given arguments
    #[mutants::skip] // Cannot test directly, alters system state
    pub fn get_cmd(
        &self,
        config: &mut Config,
        args: Vec<String>,
        selector: &str,
        use_selector: bool,
    ) -> Result<(String, Vec<String>)> {
        let special =
            AhoCorasick::new_auto_configured(&["%f", "%F", "%u", "%U"]);

        let mut exec = shlex::split(&self.exec).ok_or_else(|| {
            Error::from(ErrorKind::BadExec(
                self.exec.clone(),
                self.file_name.to_string_lossy().to_string(),
            ))
        })?;

        // The desktop entry doesn't contain arguments - we make best effort and append them at
        // the end
        if special.is_match(&self.exec) {
            exec = exec
                .into_iter()
                .flat_map(|s| match s.as_str() {
                    "%f" | "%F" | "%u" | "%U" => args.clone(),
                    s if special.is_match(s) => vec![{
                        let mut replaced =
                            String::with_capacity(s.len() + args.len() * 2);
                        special.replace_all_with(
                            s,
                            &mut replaced,
                            |_, _, dst| {
                                dst.push_str(args.clone().join(" ").as_str());
                                false
                            },
                        );
                        replaced
                    }],
                    _ => vec![s],
                })
                .collect()
        } else {
            exec.extend_from_slice(&args);
        }

        // If the entry expects a terminal (emulator), but this process is not running in one, we
        // launch a new one.
        if self.terminal && config.terminal_output {
            let term_cmd = config.terminal(selector, use_selector)?;
            exec = shlex::split(&term_cmd)
                .ok_or_else(|| Error::from(ErrorKind::BadCmd(term_cmd)))?
                .into_iter()
                .chain(exec)
                .collect();
        }

        Ok((exec.remove(0), exec))
    }

    /// Parse a desktop entry file, given a path
    fn parse_file(path: &Path) -> Option<DesktopEntry> {
        // Assume the set locales will not change while handlr is running
        static LOCALES: Lazy<Vec<String>> = Lazy::new(get_languages_from_env);

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
        Self::parse_file(&path).ok_or(Error::from(ErrorKind::BadEntry(path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_exec() -> Result<()> {
        // Note that this entry also has no category key
        let entry =
            DesktopEntry::try_from(PathBuf::from("tests/cmus.desktop"))?;
        assert_eq!(entry.mime_type.len(), 2);
        assert_eq!(entry.mime_type[0].essence_str(), "audio/mp3");
        assert_eq!(entry.mime_type[1].essence_str(), "audio/ogg");

        let mut config = Config::default();
        let args = vec!["test".to_string()];
        assert_eq!(entry.get_cmd(&mut config, args, "", false)?,
            (
                "bash".to_string(),
                [
                    "-c", 
                    "(! pgrep cmus && tilix -e cmus && tilix -a session-add-down -e cava); sleep 0.1 && cmus-remote -q test"
                ].iter().map(|s| s.to_string()).collect()
            )
        );
        assert!(!entry.is_terminal_emulator());

        Ok(())
    }

    #[test]
    fn terminal_emulator() -> Result<()> {
        let entry = DesktopEntry::try_from(PathBuf::from(
            "tests/org.wezfurlong.wezterm.desktop",
        ))?;
        assert!(entry.mime_type.is_empty());

        let mut config = Config::default();
        let args = vec!["test".to_string()];
        assert_eq!(
            entry.get_cmd(&mut config, args, "", false)?,
            (
                "wezterm".to_string(),
                ["start", "--cwd", ".", "test"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            )
        );
        assert!(entry.is_terminal_emulator());

        Ok(())
    }

    #[test]
    fn invalid_desktop_entries() -> Result<()> {
        let empty_name =
            DesktopEntry::try_from(PathBuf::from("tests/empty_name.desktop"));

        assert!(empty_name.is_err());

        let empty_exec =
            DesktopEntry::try_from(PathBuf::from("tests/empty_exec.desktop"));

        assert!(empty_exec.is_err());

        Ok(())
    }
}
