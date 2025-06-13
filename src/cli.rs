// NOTE: `cfg(execute)` is defined in build.rs and represents code to be included in the executable, but not build.rs
// Only use it when necessary.

use clap::{ArgAction, Args, Parser, Subcommand};
use clap_complete::{engine::ArgValueCompleter, PathCompleter};
use clap_verbosity_flag::{Verbosity, WarnLevel};

// Dependencies not needed for the build script
#[cfg(executable)]
use crate::{
    apps::SystemApps,
    common::mime_types,
    common::{DesktopHandler, MimeType, UserPath},
};
#[cfg(executable)]
use clap::builder::StyledStr;
#[cfg(executable)]
use clap_complete::engine::CompletionCandidate;
#[cfg(executable)]
use std::{ffi::OsStr, fmt::Write, io::IsTerminal};

// Needed to trick cli.rs to cooperate when `include`d in build.rs
#[cfg(not(executable))]
pub type DesktopHandler = String;
#[cfg(not(executable))]
pub type MimeType = String;
#[cfg(not(executable))]
pub type UserPath = String;

/// A better xdg-utils
///
/// Resource opener with support for wildcards, multiple handlers, and regular expressions.
///
/// Based on handlr at <https://github.com/chmln/handlr>
///
/// Regular expression handlers inspired by mimeo at <https://xyne.dev/projects/mimeo/>
///
/// Mime associations file location: $XDG_CONFIG_HOME/mimeapps.list
///
/// Config file location: $XDG_CONFIG_HOME/handlr/handlr.toml
///
/// Log file location: $XDG_CACHE_HOME/handlr/handlr.log
#[deny(missing_docs)]
#[derive(Parser)]
#[clap(disable_help_subcommand = true)]
#[clap(version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,

    /// Disable notifications on error
    #[clap(global = true, long = "disable-notifications", short = 'n', action = ArgAction::SetFalse)]
    enable_notifications: Option<bool>,

    /// Overrides whether or not to behave as if the output is an interactive terminal
    #[clap(global = true, long = "force-terminal-output", short = 't')]
    terminal_output: Option<bool>,

    #[command(flatten)]
    pub verbosity: Verbosity<WarnLevel>,
}

#[cfg(executable)]
impl Cli {
    pub fn terminal_output(&self) -> bool {
        self.terminal_output
            .unwrap_or(std::io::stdout().is_terminal())
    }

    pub fn show_notifications(&self) -> bool {
        !self.terminal_output() && self.enable_notifications.unwrap_or(true)
    }
}

#[deny(missing_docs)]
#[derive(Clone, Subcommand)]
pub enum Cmd {
    /// List default apps and the associated handlers
    ///
    /// Output is formatted as a table with two columns.
    /// The left column shows mimetypes and the right column shows the handlers
    ///
    /// Currently does not support regex handlers.
    ///
    /// When using `--json`, output will be in the form:
    ///
    /// [
    ///   {
    ///     "mime": "text/*",
    ///     "handlers": [
    ///       "Helix.desktop"
    ///      ]
    ///   },
    ///   {
    ///     "mime": "x-scheme-handler/https",
    ///     "handlers": [
    ///       "firefox.desktop",
    ///       "nyxt.desktop"
    ///     ]
    ///   },
    ///   ...
    /// ]
    ///
    /// When using `--json` with `--all`, output will be in the form
    ///
    /// {
    ///   "added_associations": [ ... ],   
    ///   "default_apps": [ ... ],
    ///   "system_apps": [ ... ]
    /// }
    ///
    /// Where each top-level key has an array with the same scheme as the normal `--json` output
    #[clap(verbatim_doc_comment)]
    List {
        /// Output handler info as json
        #[clap(long)]
        json: bool,
        /// Expand wildcards in mimetypes and show global defaults
        #[clap(long, short)]
        all: bool,
    },

    /// Open a path/URL with its default handler
    ///
    /// Unlike xdg-open and similar resource openers, multiple paths/URLs may be supplied.
    ///
    /// If multiple handlers are set and `enable_selector` is set to true,
    /// you will be prompted to select one using `selector` from ~/.config/handlr/handlr.toml.
    /// Otherwise, the default handler will be opened.
    Open {
        /// Paths/URLs to open
        #[clap(required = true)]
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(PathCompleter::any())))]
        paths: Vec<UserPath>,
        #[command(flatten)]
        selector_args: SelectorArgs,
    },

    /// Set the default handler for mime/extension
    ///
    /// Overwrites currently set handler(s) for the given mime/extension.
    ///
    /// Asterisks can be used as wildcards to set multiple mimetypes.
    /// When `expand_wildcards` is true in `~/.config/handlr/handlr.toml`,
    /// wildcards will be expanded into matching mimes rather than added verbatim
    ///
    /// File extensions are converted into their respective mimetypes in mimeapps.list.
    ///
    /// Currently does not support regex handlers.
    Set {
        /// Mimetype or file extension to operate on.
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_mimes)))]
        mime: MimeType,
        /// Desktop file of handler program
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_desktop_files)))]
        handler: DesktopHandler,
    },

    /// Unset the default handler for mime/extension
    ///
    /// Literal wildcards (e.g. `text/*`) will be favored over matching mimetypes if present.
    /// Otherwise, mimes matching wildcards (e.g. `text/plain`, etc.) will be removed.
    ///
    /// If multiple default handlers are set, both will be removed.
    ///
    /// Currently does not support regex handlers.
    Unset {
        /// Mimetype or file extension to unset the default handler of
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_mimes)))]
        mime: MimeType,
    },

    /// Launch the handler for specified extension/mime with optional arguments
    ///
    /// Only supports wildcards for mimetypes for handlers that have been set or added with wildcards.
    ///
    /// If multiple handlers are set and `enable_selector` is set to true,
    /// you will be prompted to select one using `selector` from ~/.config/handlr/handlr.toml.
    /// Otherwise, the default handler will be opened.
    Launch {
        /// Mimetype or file extension to launch the handler of
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_mimes)))]
        mime: MimeType,
        /// Arguments to pass to handler program
        // Not necessarily a path, but completing as a path tends to be the expected "default" behavior
        #[clap(add=ArgValueCompleter::new(PathCompleter::any()))]
        args: Vec<String>,
        #[command(flatten)]
        selector_args: SelectorArgs,
    },

    /// Get handler for this mime/extension
    ///
    /// If multiple handlers are set and `enable_selector` is set to true,
    /// you will be prompted to select one using `selector` from ~/.config/handlr/handlr.toml.
    /// Otherwise, only the default handler will be printed.
    ///
    /// Note that regex handlers are not supported by this subcommand currently.
    ///
    /// When using `--json`, output is in the form:
    ///
    /// {
    ///   "cmd": "helix",
    ///   "handler": "helix.desktop",
    ///   "name": "Helix"
    /// }
    ///
    /// Note that when handlr is not being directly output to a terminal, and the handler is a terminal program,
    /// the "cmd" key in the json output will include the command of the `x-scheme-handler/terminal` handler.
    #[clap(verbatim_doc_comment)]
    Get {
        /// Output handler info as json
        #[clap(long)]
        json: bool,
        /// Mimetype to get the handler of
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_mimes)))]
        mime: MimeType,
        #[command(flatten)]
        selector_args: SelectorArgs,
    },

    /// Add a handler for given mime/extension
    ///
    /// Note that the first handler is the default.
    ///
    /// When `expand_wildcards` is true in `~/.config/handlr/handlr.toml`,
    /// wildcards will be expanded into matching mimes rather than matched verbatim.
    ///
    /// This subcommand adds secondary handlers that coexist with the default
    /// and does not overwrite existing handlers.
    Add {
        /// Mimetype to add handler to
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_mimes)))]
        mime: MimeType,
        /// Desktop file of handler program
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_desktop_files)))]
        handler: DesktopHandler,
    },

    /// Remove a given handler from a given mime/extension
    ///
    /// Note that if a handler is not supplied,
    ///
    /// Literal wildcards (e.g. `text/*`) will be favored over matching mimetypes if present.
    /// Otherwise, mimes matching wildcards (e.g. `text/plain`, etc.) will have their handlers removed.
    Remove {
        /// Mimetype to remove handler from
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_mimes)))]
        mime: MimeType,
        /// Desktop file of handler program to remove
        #[cfg_attr(executable, clap(add = ArgValueCompleter::new(autocomplete_desktop_files)))]
        handler: DesktopHandler,
    },

    /// Get the mimetype of a given file/URL
    ///
    /// By default, output is in the form of a table that matches file paths/URLs to their mimetypes.
    ///
    /// When using `--json`, output will be in the form:
    ///
    /// [
    ///   {
    ///     "path": "README.md"
    ///     "mime": "text/markdown"
    ///   },
    ///   {
    ///     "path": "https://duckduckgo.com/"
    ///     "mime": "x-scheme-handler/https"
    ///   },
    /// ...
    /// ]
    #[clap(verbatim_doc_comment)]
    Mime {
        /// File paths/URLs to get the mimetype of
        #[clap(required = true, add=ArgValueCompleter::new(PathCompleter::any()))]
        paths: Vec<UserPath>,
        /// Output mimetype info as json
        #[clap(long)]
        json: bool,
    },
}

#[derive(Clone, Args)]
pub struct SelectorArgs {
    /// Override the configured selector command
    #[clap(long, short)]
    pub selector: Option<String>,
    /// Override whether or not the selector is enabled
    #[clap(long, short)]
    pub enable_selector: Option<bool>,
}

/// Generate candidates for mimes and file extensions to use
#[cfg(executable)]
fn autocomplete_mimes(current: &OsStr) -> Vec<CompletionCandidate> {
    let mut mimes = mime_db::EXTENSIONS
        .iter()
        .map(|(ext, _)| format!(".{ext}"))
        .chain(mime_types())
        .filter(|x| x.starts_with(current.to_string_lossy().as_ref()))
        .map(CompletionCandidate::new)
        .collect::<Vec<_>>();
    mimes.sort();
    mimes
}

/// Generate candidates for desktop files
#[mutants::skip] // Cannot test directly, relies on system state
#[cfg(executable)]
fn autocomplete_desktop_files(current: &OsStr) -> Vec<CompletionCandidate> {
    SystemApps::get_entries()
        .expect("handlr error: Could not get system desktop entries")
        .filter(|(path, _)| {
            path.to_string_lossy()
                .starts_with(current.to_string_lossy().as_ref())
        })
        .map(|(path, entry)| {
            let mut name = StyledStr::new();
            write!(name, "{}", entry.name)
                .expect("handlr error: Could not write desktop entry name");
            CompletionCandidate::new(path).help(Some(name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::error::Result;

    use super::*;

    #[test]
    fn test_autocomplete_mimes() {
        insta::assert_compact_debug_snapshot!(autocomplete_mimes(OsStr::new(
            ""
        )));
    }

    #[test]
    fn test_show_notifications() -> Result<()> {
        let mut cli = Cli {
            command: Cmd::Unset {
                mime: MimeType::from_str("fake/mime")?,
            },
            enable_notifications: Some(true),
            terminal_output: Some(false),
            verbosity: Verbosity::default(),
        };

        assert!(cli.show_notifications());

        cli.terminal_output = Some(true);
        assert!(!cli.show_notifications());

        cli.enable_notifications = Some(false);
        assert!(!cli.show_notifications());

        cli.terminal_output = Some(true);
        assert!(!cli.show_notifications());

        Ok(())
    }
}
