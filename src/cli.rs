use crate::common::{DesktopHandler, MimeOrExtension, UserPath};
use clap::Parser;

/// A better xdg-utils
///
/// Resource opener with support for wildcards, multiple handlers, and regular expressions.
///
/// Based on handlr at <https://github.com/chmln/handlr>
///
/// Regular expression handlers inspired by mimeo at <https://xyne.dev/projects/mimeo/>
#[deny(missing_docs)]
#[derive(Parser)]
#[clap(disable_help_subcommand = true)]
#[clap(version, about)]
pub enum Cmd {
    #[clap(verbatim_doc_comment)]
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
    List {
        #[clap(long)]
        /// Output handler info as json
        json: bool,
        #[clap(long, short)]
        /// Expand wildcards in mimetypes and show global defaults
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
        #[clap(required = true)]
        /// Paths/URLs to open
        paths: Vec<UserPath>,
        #[clap(long, short)]
        /// Override the configured selector command
        selector: Option<String>,
        #[clap(long, short)]
        /// Enable selector, overrides `enable_selector`
        enable_selector: bool,
        #[clap(long, short)]
        #[clap(overrides_with = "enable_selector")]
        /// Disable selector, overrides `enable_selector`
        disable_selector: bool,
    },

    /// Set the default handler for mime/extension
    ///
    /// Overwrites currently set handler(s) for the given mime/extension.
    ///
    /// Asterisks can be used as wildcards to set multiple mimetypes.
    ///
    /// File extensions are converted into their respective mimetypes in mimeapps.list.
    ///
    /// Currently does not support regex handlers.
    Set {
        /// Mimetype or file extension to operate on.
        mime: MimeOrExtension,
        /// Desktop file of handler program
        handler: DesktopHandler,
    },

    /// Unset the default handler for mime/extension
    ///
    /// Wildcards cannot be used unless removing handlers that already have wildcards.
    ///
    /// If multiple default handlers are set, both will be removed.
    ///
    /// Currently does not support regex handlers.
    Unset {
        /// Mimetype or file extension to unset the default handler of
        mime: MimeOrExtension,
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
        mime: MimeOrExtension,
        /// Arguments to pass to handler program
        args: Vec<UserPath>,
        #[clap(long, short)]
        /// Override the configured selector command
        selector: Option<String>,
        #[clap(long, short)]
        /// Enable selector, overrides `enable_selector`
        enable_selector: bool,
        #[clap(long, short)]
        #[clap(overrides_with = "enable_selector")]
        /// Disable selector, overrides `enable_selector`
        disable_selector: bool,
    },

    #[clap(verbatim_doc_comment)]
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
    Get {
        #[clap(long)]
        /// Output handler info as json
        json: bool,
        /// Mimetype to get the handler of
        mime: MimeOrExtension,
        #[clap(long, short)]
        /// Override the configured selector command
        selector: Option<String>,
        #[clap(long, short)]
        /// Enable selector, overrides `enable_selector`
        enable_selector: bool,
        #[clap(long, short)]
        #[clap(overrides_with = "enable_selector")]
        /// Disable selector, overrides `enable_selector`
        disable_selector: bool,
    },

    /// Add a handler for given mime/extension
    ///
    /// Note that the first handler is the default.
    ///
    /// This subcommand adds secondary handlers that coexist with the default
    /// and does not overwrite existing handlers.
    Add {
        /// Mimetype to add handler to
        mime: MimeOrExtension,
        /// Desktop file of handler program
        handler: DesktopHandler,
    },

    /// Remove a given handler from a given mime/extension
    ///
    /// Note that if a handler is not supplied,
    ///
    /// Wildcards cannot be used unless removing handlers from mimetypes
    /// that already have wildcards.
    Remove {
        /// Mimetype to remove handler from
        mime: MimeOrExtension,
        /// Desktop file of handler program to remove
        handler: DesktopHandler,
    },

    #[clap(verbatim_doc_comment)]
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
    Mime {
        #[clap(required = true)]
        /// File paths/URLs to get the mimetype of
        paths: Vec<UserPath>,
        #[clap(long)]
        /// Output mimetype info as json
        json: bool,
    },

    #[clap(hide = true)]
    /// Helper subcommand for autocompletion scripts; should be hidden
    ///
    /// This should not be visible in `handlr --help` output, autocompletion, or man pages.
    /// If you see this there, please open an issue on GitHub.
    ///
    /// However it is fine if this shows up in the output of `handlr autocomplete --help`.
    Autocomplete {
        #[clap(short)]
        /// Autocomplete for desktop files present on system
        desktop_files: bool,
        #[clap(short)]
        /// Autocomplete for mimetypes/file extensions
        mimes: bool,
    },
}
