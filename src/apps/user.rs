use crate::{
    common::{DesktopEntry, DesktopHandler, Handleable, UserPath, MIME_TYPES},
    config::{ConfigFile, Languages},
    error::{Error, Result},
};
use derive_more::{Deref, DerefMut};
use itertools::Itertools;
use lazy_regex::regex_replace_all;
use mime::Mime;
use serde::{Deserialize, Serialize};
use serde_with::{
    serde_as, DeserializeFromStr, DisplayFromStr, SerializeDisplay,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, VecDeque},
    fmt::Display,
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
};
use tracing::{debug, info, warn};
use wildmatch::WildMatch;

/// Represents user-configured mimeapps.list file
#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
// IMPORTANT: This ensures missing fields are replaced by a default value rather than making deserialization fail entirely
#[serde(default)]
pub struct MimeApps {
    #[serde(rename = "Added Associations")]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    #[serde_as(as = "BTreeMap<DisplayFromStr, _>")]
    pub added_associations: BTreeMap<Mime, DesktopList>,
    #[serde(rename = "Default Applications")]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    #[serde_as(as = "BTreeMap<DisplayFromStr, _>")]
    pub default_apps: BTreeMap<Mime, DesktopList>,
}

/// Helper struct for a list of `DesktopHandler`s
#[serde_as]
#[derive(
    Debug,
    Default,
    Clone,
    Deref,
    DerefMut,
    SerializeDisplay,
    DeserializeFromStr,
    PartialEq,
)]
pub struct DesktopList(VecDeque<DesktopHandler>);

impl FromStr for DesktopList {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            s.split(';')
                .filter(|s| !s.is_empty()) // Account for ending/duplicated semicolons
                .unique() // Remove duplicate entries
                .map(DesktopHandler::from_str)
                .collect::<Result<_>>()?,
        ))
    }
}

impl Display for DesktopList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{};", self.iter().join(";"))
    }
}

impl MimeApps {
    /// Add a handler to an existing default application association
    pub fn add_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
        expand_wildcards: bool,
    ) -> Result<()> {
        // Warn the user if the given handler does not exist
        handler.warn_if_invalid();

        debug!("Expanding wildcards in mimeapps.list: {}", expand_wildcards);

        if expand_wildcards {
            let wildcard = WildMatch::new(mime.as_ref());
            MIME_TYPES
                .iter()
                .filter(|mime| wildcard.matches(mime))
                .try_for_each(|mime| -> Result<()> {
                    self.default_apps
                        .entry(Mime::from_str(mime)?)
                        .or_default()
                        .push_back(handler.clone());
                    Ok(())
                })?
        } else {
            self.default_apps
                .entry(mime.clone())
                .or_default()
                .push_back(handler.clone());
        }

        self.log_handler_change(mime);
        Ok(())
    }

    /// Set a default application association, overwriting any existing association for the same mimetype
    pub fn set_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
        expand_wildcards: bool,
    ) -> Result<()> {
        // Warn the user if the given handler does not exist
        handler.warn_if_invalid();

        debug!("Expanding wildcards in mimeapps.list: {}", expand_wildcards);

        if expand_wildcards {
            let wildcard = WildMatch::new(mime.as_ref());
            MIME_TYPES
                .iter()
                .filter(|mime| wildcard.matches(mime))
                .try_for_each(|mime| -> Result<()> {
                    self.default_apps.insert(
                        Mime::from_str(mime)?,
                        DesktopList(vec![handler.clone()].into()),
                    );
                    Ok(())
                })?
        } else {
            self.default_apps.insert(
                mime.clone(),
                DesktopList(vec![handler.clone()].into()),
            );
        }

        self.log_handler_change(mime);
        Ok(())
    }

    /// Entirely remove a given mime's default application association
    pub fn unset_handler(&mut self, mime: &Mime) -> Option<()> {
        // If exact match is found, remove it
        self.default_apps.remove(mime).map_or_else(
            || {
                let wildcard = WildMatch::new(mime.as_ref());
                // Otherwise, remove all wildcard matches
                self.default_apps
                    .retain(|m, _| !wildcard.matches(m.as_ref()));
                Some(())
            },
            |_| Some(()),
        )?;

        self.log_handler_change(mime);
        Some(())
    }

    /// Remove a given handler from a given mime's default file associaion
    pub fn remove_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
    ) -> Option<()> {
        let handler_list = self.default_apps.entry(mime.clone()).or_default();

        // If exact match is found, remove handler from it
        handler_list
            .iter()
            .position(|x| *x == *handler)
            .and_then(|pos| handler_list.remove(pos))
            // Otherwise, look for a wildcard match
            .map_or_else(
                || {
                    let wildcard = WildMatch::new(mime.as_ref());
                    self.default_apps
                        .clone()
                        .keys()
                        .filter(|m| wildcard.matches(m.as_ref()))
                        .for_each(|m| {
                            let handler_list =
                                self.default_apps.entry(m.clone()).or_default();
                            handler_list
                                .iter()
                                .position(|x| *x == *handler)
                                .and_then(|pos| handler_list.remove(pos));
                        });
                    Some(())
                },
                |_| Some(()),
            );

        self.log_handler_change(mime);
        Some(())
    }

    /// Helper function to log a change in set handlers
    fn log_handler_change(&self, mime: &Mime) {
        // Fallback value for empty handler list
        const DEFAULT: &str = "<None>";

        debug!(
            "New handlers for `{}`: {}",
            mime,
            self.default_apps.get(mime).map_or(
                DEFAULT.to_string(),
                |handlers| if handlers.is_empty() {
                    DEFAULT.to_string()
                } else {
                    handlers.to_string()
                }
            )
        );
    }

    /// Get a list of handlers associated with a wildcard mime
    fn get_from_wildcard(&self, mime: &Mime) -> Option<&DesktopList> {
        // Get the handlers that wildcard match the given mime
        let associations = self.default_apps.iter().filter(|(m, _)| {
            wildmatch::WildMatch::new(m.as_ref()).matches(mime.as_ref())
        });

        // Get the length of the longest wildcard that matches
        // Assuming the longest match is the best match
        // Inspired by how globs are handled in xdg spec
        let biggest_wildcard_len = associations
            .clone()
            .map(|(ref m, _)| m.as_ref().len())
            .max()?;

        // Keep only the lists of handlers from associations with the longest wildcards
        // And get the first one, assuming it takes precedence
        // Loosely inspired by how globs are handled in xdg spec
        associations
            .filter(|(ref m, _)| m.as_ref().len() == biggest_wildcard_len)
            .map(|(_, handlers)| handlers)
            .collect_vec()
            .first()
            .cloned()
    }

    /// Get the handler associated with a given mime from mimeapps.list's default apps
    #[mutants::skip] // Cannot entirely test, namely cannot test selector or filtering and associated logging
    pub fn get_handler_from_user(
        &self,
        mime: &Mime,
        path: Option<&UserPath>,
        config_file: &ConfigFile,
        languages: &Languages,
    ) -> Result<DesktopHandler> {
        let error = Error::NotFound(mime.to_string());
        // Check for an exact match first and then fall back to wildcard
        match self
            .default_apps
            .get(mime)
            .or_else(|| self.get_from_wildcard(mime))
        {
            Some(handlers) => {
                debug!(
                    "Configured handlers for `{}` in mimeapps.list Default Associations: {}",
                    mime, handlers
                );
                debug!(
                    "Selector enabled: {}, number of set handlers: {}",
                    config_file.selector.enabled,
                    handlers.len()
                );
                if config_file.selector.enabled && handlers.len() > 1 {
                    get_handler_from_selector(
                        handlers,
                        mime,
                        path,
                        config_file,
                        languages,
                    )
                } else {
                    info!("Not running selector, choosing first handler");
                    let handler = handlers
                        .iter()
                        .flat_map(|h| {
                            // Filtering breaks testing, so treat every app as valid
                            if cfg!(test) {
                                Some(h)
                            } else {
                                // get entry to check if valid
                                get_entry(h, languages).ok().map(|_| h)
                            }
                        })
                        .next()
                        .ok_or(error)?;
                    Ok(handler.clone())
                }
            }
            None => {
                info!("No handlers configured for `{}` in mimeapps.list Default associations", mime);
                Err(error)
            }
        }
    }

    /// Get the path to the user's mimeapps.list file
    #[mutants::skip] // Cannot test directly, depends on system state
    fn path() -> Result<PathBuf> {
        let mut config = xdg::BaseDirectories::new()
            .get_config_home()
            .ok_or(Error::NoHome)?;
        config.push("mimeapps.list");
        Ok(config)
    }

    /// Read and parse mimeapps.list
    #[mutants::skip] // Cannot test directly, depends on system state
    pub fn read() -> Result<Self> {
        let exists = std::path::Path::new(&Self::path()?).exists();

        let file = std::fs::OpenOptions::new()
            .write(!exists)
            .create(!exists)
            .read(true)
            .open(Self::path()?)?;

        Self::read_from(file)
    }

    /// Deserialize MimeApps from reader
    /// Makes testing easier
    fn read_from<R: Read>(reader: R) -> Result<Self> {
        let mut mime_apps: MimeApps = serde_ini::de::from_read(reader)?;

        // Remove empty entries
        mime_apps
            .default_apps
            .retain(|_, handlers| !handlers.is_empty());

        Ok(mime_apps)
    }

    /// Save associations to mimeapps.list
    #[mutants::skip] // Cannot test directly, alters system state
    pub fn save(&mut self) -> Result<()> {
        if cfg!(test) {
            Ok(())
        } else {
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .create(true)
                .write(true)
                .truncate(true)
                .open(Self::path()?)?;

            self.save_to(&mut file)
        }
    }

    /// Serialize MimeApps and write to writer
    /// Makes testing easier
    fn save_to<W: Write>(&mut self, writer: &mut W) -> Result<()> {
        // Remove empty entries
        self.default_apps.retain(|_, handlers| !handlers.is_empty());

        // Use Linefeed instead of default carriage return
        let w = serde_ini::write::Writer::new(
            writer,
            serde_ini::write::LineEnding::Linefeed,
        );
        let mut ser = serde_ini::ser::Serializer::new(w);
        self.serialize(&mut ser)?;

        Ok(())
    }
}

/// Returns the entry corresponding to the passed in handler and languages.
///
/// Logs a warning if desktop entry is not valid.
#[mutants::skip] // Only a logger wrapper around `DesktopHandler::get_entry`. Only called in other fns with `#[mutants::skip]`.
fn get_entry(
    handler: &DesktopHandler,
    languages: &Languages,
) -> Result<DesktopEntry> {
    let entry = handler.get_entry(languages);
    if let Err(ref e) = entry {
        warn!("Desktop entry `{}` is invalid: {}", handler, e);
    } else {
        debug!("Desktop entry `{}` is valid", handler);
    }
    entry
}
/// Asks user which handler to use.
/// Calls `config_file.selector`.
#[mutants::skip] // Cannot test directly, runs external command
fn get_handler_from_selector(
    handlers: &DesktopList,
    mime: &Mime,
    path: Option<&UserPath>,
    config_file: &ConfigFile,
    languages: &Languages,
) -> Result<DesktopHandler> {
    info!("Running selector: {}", &config_file.selector.command);

    let entries = handlers
        .iter()
        .flat_map(|h| get_entry(h, languages).map(|e| (h, e)))
        .collect_vec();

    let entry: &DesktopEntry = select(
        &config_file.selector.command,
        &config_file.selector.handler_format,
        config_file
            .selector
            .handler_identifier
            .as_deref()
            .unwrap_or(""),
        &config_file.selector.handler_separator,
        mime,
        path,
        languages,
        entries.iter().map(|(_, e)| e),
    )?;
    info!("Selected: `{}`", entry.name);

    entries
        .iter()
        .find(|(_, expected)| expected.name == entry.name)
        .map(|(handler, _)| (*handler).clone())
        .ok_or(Error::NotFound(mime.to_string()))
}

/// Replaces placeholders in the selector command format ([ConfigFile::selector]).
///
/// Placeholder Format: `{NAME}`
///
/// Available placeholders:
/// * `%Path, %Url, %u, %U, %f, %F`: Path or url to open (
///     (`handlr open https://github.com` -> `https://github.com`)
/// * `%Mime`: Mime type
///
///
/// # Notes
/// * Capitalization must match exactly!
/// * If Unrecognized Name: output name without surrounding braces
/// * To display a `{` use two braces: `{{`: `{{%Url}` outputs `{%Url}`
/// * Leading `%` to align with the format in [format_item] and `%u, %f, %U, %F` in the [XDG Desktop Entry specification](https://specifications.freedesktop.org/desktop-entry/latest/exec-variables.html)
fn format_selector<'s>(
    selector_format: &'s str,
    mime: &Mime,
    path: Option<&UserPath>,
) -> Cow<'s, str> {
    regex_replace_all!(
        r"(\{\{)|(\{(%?[\w-]+)})",
        selector_format,
        |_, escaped: &str, inner: &str, placeholder: &str| {
            if escaped.is_empty() {
                match placeholder {
                    "%Path" | "%Url" | "%u" | "%f" | "%U" | "%F" => {
                        path.map(|p| p.to_string()).unwrap_or_default()
                    }
                    "%Mime" => mime.to_string(),
                    _ => inner.to_string(),
                }
            } else {
                "{".to_string()
            }
        }
    )
}
/// Replaces placeholders in the selector handler format ([ConfigFile::selector_handler_format]).
///
/// Placeholder Format: `{NAME}`
///
/// Available placeholders:
/// * Name without leading `%`: key inside the corresponding `.desktop` file inside the `[Desktop Entry]` section.
///     For available keys see [XDG Desktop Entry specification](https://specifications.freedesktop.org/desktop-entry/latest/recognized-keys.html).
///     * Examples: `Name`, `Icon`, `GenericName`
///     * Note: All keys are localized with `languages`
///     * Note: If key is not present in the desktop file: output empty string.
/// * Name with leading `%`: Any of the following placeholders:
///     * `%FileName`: Name of the .desktop file
///     * `%Url, %Path, %u, %U, %f, %F`: Url or path to open
///     * `%Index, %Index0, %i`: 0-based index of this handler in the list of all handlers for this mime type.  
///        For use with `rofi -format i` wich returns the selected 0-based index.
///     * `%Index1, %d`: 1-based index of this handler in the list of all handler for this mime type.  
///        For use with `rofi -format d` wich returns the selected 1-based index.
///     * Note: If name is not one of the recognized ones: output name without surrounding braces.
fn format_item<'s>(
    item_format: &'s str,
    mime: &Mime,
    path: Option<&UserPath>,
    entry: &DesktopEntry,
    index: usize,
    languages: &Languages,
) -> Cow<'s, str> {
    regex_replace_all!(
        r"(\{\{)|(\{(%?[\w-]+)})",
        item_format,
        |_, escaped: &str, inner: &str, placeholder: &str| {
            if escaped.is_empty() {
                // without leading `%`: pass on to Desktop Entry
                // with    leading `%`: not in Desktop Entry, but data from outside (like index or passed path)
                match placeholder {
                    // Shortcut for name: probably the most uses placeholder (and default!)
                    "Name" => entry.name.clone(),
                    "%FileName" => {
                        entry.file_name.to_str().unwrap_or_default().to_string()
                    }
                    "%Url" | "%Path" | "%u" | "%f" | "%U" | "%F" => {
                        path.map(|p| p.to_string()).unwrap_or_default()
                    }
                    "%Mime" => mime.to_string(),
                    // 0-based index
                    "%Index" | "%Index0" | "%i" => index.to_string(),
                    // 1-based index
                    "%Index1" | "%d" => (index + 1).to_string(),
                    _ if placeholder.starts_with('%') => inner.to_string(),
                    _ => entry
                        .get_first_with_language(placeholder, languages)
                        .unwrap_or_default()
                        .to_string(),
                }
            } else {
                "{".to_string()
            }
        }
    )
}

/// Try to pair `output` from selector with the correct handler/Desktop Entry.
///
/// Issue is: `output` might not be same as the input.
/// Example in rofi: `Helix\0icon\x1fhelix` -> `Helix`: Input includes icon, which is NOT returned by rofi.
///
/// If `config.selector_handler_identifier` is specified that can be used to match input with output (done elsewhere).
/// Otherwise this function tries some simple rules to detect the correct handler.
///
/// # Parameters
/// * `items`: for each handler: DesktopEntry and corresponding line passed to selector
/// * `output`: output of selector
fn guess_matching_entry<'e>(
    items: &[(&'e DesktopEntry, Cow<'_, str>)],
    output: &str,
) -> Result<&'e DesktopEntry> {
    // match output with input
    if let Some((entry, _)) = items.iter().find(|(_, input)| *input == output) {
        return Ok(entry);
    }

    // match name
    if let Some((entry, _)) =
        items.iter().find(|(entry, _)| entry.name == output)
    {
        return Ok(entry);
    }

    // match before 1st control character
    //   example: `Helix\0icon\x1fhelix` -> match `Helix`
    if let Some((entry, _)) = items.iter().find(|(_, input)| {
        input.split(|c: char| c.is_control()).next() == Some(output)
    }) {
        return Ok(entry);
    }

    Err(Error::BadSelection(output.to_string()))
}

/// Run given selector command
#[mutants::skip] // Cannot test directly, runs external command
fn select<'e>(
    selector: &str,
    item_format: &str,
    item_identifier: &str,
    item_separator: &str,
    mime: &Mime,
    path: Option<&UserPath>,
    languages: &Languages,
    entries: impl Iterator<Item = &'e DesktopEntry>,
) -> Result<&'e DesktopEntry> {
    use std::{io::prelude::*, process::Stdio};

    let selector = format_selector(selector, mime, path);
    let process = {
        execute::command(&selector)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?
    };

    let items: Vec<_> = entries
        .enumerate()
        .map(|(i, entry)| {
            let item =
                format_item(item_format, mime, path, &entry, i, languages);
            (entry, item)
        })
        .collect();

    let output = {
        let es = items.iter().map(|(_, t)| t).join(item_separator);

        /// Pretty print and escape control chars.
        /// For debugging purposes.
        fn format_cmd(text: &str) -> String {
            text.chars()
                .map(|c| {
                    if c.is_control() {
                        // Note: `c.escape_default()` formats unicode as `\u{...}`, but we need `\u....` or `\x..` or `\U.....`
                        //       but good enough for debugging -> only do `\xHH` for ascii, and keep rust format otherwise
                        match c {
                            '\n' => "\\n".to_string(),
                            '\t' => "\\t".to_string(),
                            '\0' => "\\0".to_string(),
                            _ if c.is_ascii_control() => {
                                format!("\\x{:x?}", c as u32)
                            }
                            _ => c.escape_default().to_string(),
                        }
                    } else {
                        c.to_string()
                    }
                })
                .collect()
        }
        // Line to copy & paste into shell. For debugging purposes.
        info!("echo -en '{}' | {}", format_cmd(&es), format_cmd(&selector));

        process
            .stdin
            .ok_or_else(|| Error::Selector(selector.to_string()))?
            .write_all(es.as_bytes())?;

        let output = {
            let mut output = String::with_capacity(24);
            process
                .stdout
                .ok_or_else(|| Error::Selector(selector.to_string()))?
                .read_to_string(&mut output)?;
            output
        };

        let output = output.trim_end().to_owned();
        info!("Selector output: {}", output);
        output
    };

    if output.is_empty() {
        Err(Error::Cancelled)
    } else if item_identifier.is_empty() {
        // guess identifier
        guess_matching_entry(items.as_slice(), &output)
    } else {
        // match according to `item_identifier`
        items
            .into_iter()
            .enumerate()
            .find_map(|(i, (entry, _))| {
                let expected = format_item(
                    item_identifier,
                    mime,
                    path,
                    entry,
                    i,
                    languages,
                );
                if expected == output {
                    Some(entry)
                } else {
                    None
                }
            })
            .ok_or(Error::BadSelection(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use similar_asserts::assert_eq;
    use std::{fs::File, str::FromStr};

    // Helper function to test serializing and deserializing mimeapps.list files
    fn mimeapps_round_trip(
        input_path: &str,
        expected_path: &str,
        mutation: fn(&mut MimeApps) -> Result<()>,
    ) -> Result<()> {
        let file = File::open(input_path)?;
        let mut mime_apps = MimeApps::read_from(file)?;

        mutation(&mut mime_apps)?;

        let mut buffer = Vec::new();
        mime_apps.save_to(&mut buffer)?;

        assert_eq!(
            String::from_utf8(buffer)?,
            std::fs::read_to_string(expected_path)?
        );

        Ok(())
    }

    // Helper function that does nothing
    fn noop(_: &mut MimeApps) -> Result<()> {
        Ok(())
    }

    // Helper function to reduce duplicate code for the most common case
    fn mimeapps_round_trip_simple(path: &str) -> Result<()> {
        mimeapps_round_trip(path, path, noop)
    }

    #[test]
    fn mimeapps_no_added_round_trip() -> Result<()> {
        mimeapps_round_trip_simple("./tests/assets/mimeapps_no_added.list")
    }

    #[test]
    fn mimeapps_no_default_round_trip() -> Result<()> {
        mimeapps_round_trip_simple("./tests/assets/mimeapps_no_default.list")
    }

    #[test]
    fn mimeapps_sorted_round_trip() -> Result<()> {
        mimeapps_round_trip_simple("./tests/assets/mimeapps_sorted.list")
    }

    #[test]
    fn mimeapps_anomalous_semicolons_round_trip() -> Result<()> {
        mimeapps_round_trip(
            "./tests/assets/mimeapps_anomalous_semicolons.list",
            "./tests/assets/mimeapps_sorted.list",
            noop,
        )
    }

    #[test]
    fn mimeapps_empty_entry_round_trip() -> Result<()> {
        mimeapps_round_trip(
            "./tests/assets/mimeapps_empty_entry.list",
            "./tests/assets/mimeapps_no_added.list",
            noop,
        )
    }

    #[test]
    fn mimeapps_empty_entry_fallback() -> Result<()> {
        let file = File::open("./tests/assets/mimeapps_empty_entry.list")?;
        let mime_apps = MimeApps::read_from(file)?;
        let config_file = ConfigFile::default();

        assert_eq!(
            mime_apps
                .get_handler_from_user(
                    &mime::TEXT_PLAIN,
                    None,
                    &config_file,
                    &Vec::new()
                )?
                .to_string(),
            "nvim.desktop"
        );

        Ok(())
    }

    #[test]
    // This is mainly to check that "empty" entries don't get mixed in and complicate things
    fn mimeapps_round_trip_with_deletion_and_re_addition() -> Result<()> {
        let remove_and_re_add = |mime_apps: &mut MimeApps| {
            mime_apps.remove_handler(
                &mime::TEXT_HTML,
                &DesktopHandler::from_str("nvim.desktop")?,
            );
            mime_apps.add_handler(
                &mime::TEXT_HTML,
                &DesktopHandler::from_str("nvim.desktop")?,
                false,
            )?;
            Ok(())
        };

        let path = "./tests/assets/mimeapps_sorted.list";

        mimeapps_round_trip(path, path, remove_and_re_add)
    }

    #[test]
    fn mimeapps_duplicate_round_trip() -> Result<()> {
        mimeapps_round_trip(
            "./tests/assets/mimeapps_duplicate.list",
            "./tests/assets/mimeapps_no_added.list",
            noop,
        )
    }

    #[test]
    fn set_handlers_expand_wildcards() -> Result<()> {
        let mut mime_apps = MimeApps::default();

        mime_apps.set_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
            true,
        )?;

        mime_apps.set_handler(
            &Mime::from_str("application/vnd.oasis.opendocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
            true,
        )?;

        // This should only add video/mp4
        mime_apps.set_handler(
            &Mime::from_str("video/mp4")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
            true,
        )?;

        let mut buffer = Vec::new();
        mime_apps.save_to(&mut buffer)?;

        insta::assert_snapshot!(String::from_utf8(buffer)?);

        Ok(())
    }

    #[test]
    fn add_handlers_expand_wildcards() -> Result<()> {
        let mut mime_apps = MimeApps::default();

        mime_apps.add_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
            true,
        )?;

        mime_apps.add_handler(
            &Mime::from_str("application/vnd.oasis.opendocument.*")?,
            &DesktopHandler::assume_valid("startcenter.desktop".into()),
            true,
        )?;

        mime_apps.add_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("nvim.desktop".into()),
            true,
        )?;

        // This should only add video/mp4
        mime_apps.add_handler(
            &Mime::from_str("video/mp4")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
            true,
        )?;

        let mut buffer = Vec::new();
        mime_apps.save_to(&mut buffer)?;

        insta::assert_snapshot!(String::from_utf8(buffer)?);

        Ok(())
    }

    #[test]
    fn unset_handlers_expand_wildcards() -> Result<()> {
        let mut mime_apps = MimeApps::default();

        // Just add text/*
        mime_apps.set_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
            false,
        )?;

        // Add all the non-wildcard text mimes
        mime_apps.set_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
            true,
        )?;

        // text/* should still be present
        assert!(mime_apps
            .default_apps
            .contains_key(&Mime::from_str("text/*")?));

        mime_apps.unset_handler(&Mime::from_str("text/*")?);

        let mut buffer = Vec::new();
        mime_apps.save_to(&mut buffer)?;

        // Only text/* should be removed first
        insta::assert_snapshot!(String::from_utf8(buffer)?);

        mime_apps.unset_handler(&Mime::from_str("text/*")?);

        // Now that text/* isn't literally present, remove the rest of the text mimes
        assert!(mime_apps.default_apps.is_empty());

        Ok(())
    }

    #[test]
    fn remove_handlers_expand_wildcards() -> Result<()> {
        let mut mime_apps = MimeApps::default();
        // Just add text/*
        mime_apps.add_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
            false,
        )?;

        mime_apps.add_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("nvim.desktop".into()),
            false,
        )?;

        // Add all the non-wildcard text mimes
        mime_apps.add_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
            true,
        )?;

        mime_apps.add_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("nvim.desktop".into()),
            true,
        )?;

        // Only remove from text/*
        mime_apps.remove_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
        );

        assert_eq!(
            mime_apps.default_apps.get(&Mime::from_str("text/*")?),
            Some(&DesktopList(
                vec![DesktopHandler::assume_valid("nvim.desktop".into())]
                    .into()
            ))
        );

        // Remove from the rest of the text mimes
        mime_apps.remove_handler(
            &Mime::from_str("text/*")?,
            &DesktopHandler::assume_valid("Helix.desktop".into()),
        );

        let mut buffer = Vec::new();
        mime_apps.save_to(&mut buffer)?;
        insta::assert_snapshot!(String::from_utf8(buffer)?);

        Ok(())
    }
}

#[cfg(test)]
mod select_tests {
    use super::*;
    use similar_asserts::assert_eq;
    use std::str::FromStr;

    #[test]
    fn formatting_selector() {
        let mime = Mime::from_str("x-scheme-handler/https").unwrap();
        let url: UserPath =
            "https://specifications.freedesktop.org/desktop-entry/latest/"
                .parse()
                .unwrap();

        let actual = format_selector("rofi -dmenu -show-icons -i -p 'Open With: ' -mesg 'Open {%Path}\n\t({%Mime})'", &mime, Some(&url));
        let expected = format!("rofi -dmenu -show-icons -i -p 'Open With: ' -mesg 'Open {url}\n\t({mime})'");
        assert_eq!(actual, expected);

        let actual = format_selector("rofi -dmenu -show-icons -i -p 'Open With: ' -mesg 'Open {%Path}\n\t({%Mime})'", &mime, None);
        let expected = format!("rofi -dmenu -show-icons -i -p 'Open With: ' -mesg 'Open {url}\n\t({mime})'", url="");
        assert_eq!(actual, expected);

        let actual = format_selector(
            "UserPath: {%u} {%f} {%U} {%F} {%Path} {%Url}; Mime: {%Mime}",
            &mime,
            Some(&url),
        );
        let expected = format!(
            "UserPath: {url} {url} {url} {url} {url} {url}; Mime: {mime}"
        );
        assert_eq!(actual, expected);

        let actual = format_selector(
            "{%Path} {{%Path} {Path} {%Path}",
            &mime,
            Some(&url),
        );
        let expected = format!("{url} {{%Path}} {{Path}} {url}");
        assert_eq!(actual, expected);

        let actual = format_selector("{Foo}", &mime, Some(&url));
        let expected = "{Foo}";
        assert_eq!(actual, expected);
    }

    #[test]
    fn formatting_item() {
        let mime = Mime::from_str("x-scheme-handler/https").unwrap();
        let path: UserPath =
            "https://specifications.freedesktop.org/desktop-entry/latest/"
                .parse()
                .unwrap();
        let entry = DesktopEntry::parse_file(
            &PathBuf::from("./tests/assets/https/org.mozilla.firefox.desktop"),
            &vec![],
        )
        .unwrap();
        let index = 2;
        let languages = vec![];

        let actual = format_item(
            "{Name}",
            &mime,
            Some(&path),
            &entry,
            index,
            &languages,
        );
        let expected = format!("{name}", name = entry.name);
        assert_eq!(actual, expected);

        // {Name}\0icon\x1f{Icon}
        //   but in TOML: cannot use `\0` -> must use one of the Unicode formats (`\x00, \u0000, \U00000000`)
        let actual = format_item(
            r"{Name}\x00icon\x1f{Icon}",
            &mime,
            Some(&path),
            &entry,
            index,
            &languages,
        );
        let expected = format!(
            r"{name}\x00icon\x1f{icon}",
            name = entry.name,
            icon = entry.get_first("Icon").unwrap()
        );
        assert_eq!(actual, expected);

        let actual = format_item(
            "{%Index}",
            &mime,
            Some(&path),
            &entry,
            index,
            &languages,
        );
        let expected = format!("{index}");
        assert_eq!(actual, expected);

        let actual = format_item(
            "{%Index1}",
            &mime,
            Some(&path),
            &entry,
            index,
            &languages,
        );
        let expected = format!("{index}", index = index + 1);
        assert_eq!(actual, expected);

        let actual = format_item(
            "{Name} {GenericName} {Comment} {Icon} {X-GNOME-FullName}",
            &mime,
            Some(&path),
            &entry,
            index,
            &languages,
        );
        let expected = [
            entry.get_first("Name").unwrap(),
            entry.get_first("GenericName").unwrap(),
            entry.get_first("Comment").unwrap(),
            entry.get_first("Icon").unwrap(),
            entry.get_first("X-GNOME-FullName").unwrap(),
        ]
        .join(" ");
        assert_eq!(actual, expected);

        // Index in rofi: `-format i`: 0-based, `-format d`: 1-based
        let actual = format_item(
            "{%FileName} {%Url} {%Path} {%Index} {%Index0} {%Index1}",
            &mime,
            Some(&path),
            &entry,
            index,
            &languages,
        );
        // let actual = format_item("%name %genericName %comment %icon %path %url %index0 %index1 %index %i %d", Some(&path), &entry, index, &languages);
        let expected = format!(
            "{file_name} {path} {path} {index0} {index0} {index1}",
            file_name = entry.file_name.to_str().unwrap(),
            index0 = index,
            index1 = (index + 1)
        );
        assert_eq!(actual, expected);

        let actual = format_item(
            "{Name} {{Name} {{Name}} {{foo {Name} bar {{{{Name}}}}",
            &mime,
            Some(&path),
            &entry,
            index,
            &languages,
        );
        let expected = [
            &entry.name,
            "{Name}",
            "{Name}}",
            "{foo",
            &entry.name,
            "bar",
            "{{Name}}}}",
        ]
        .join(" ");
        assert_eq!(actual, expected);

        // different languages
        let actual = format_item(
            "{Comment}",
            &mime,
            Some(&path),
            &entry,
            index,
            &vec!["de".to_string()],
        );
        assert_eq!(actual, "Schneller und privater Browser");

        let actual = format_item(
            "{Comment}",
            &mime,
            Some(&path),
            &entry,
            index,
            // fall back to unlocalized
            &vec!["foo".to_string()],
        );
        assert_eq!(actual, "Fast and private browser");

        let actual = format_item(
            "{Comment}",
            &mime,
            Some(&path),
            &entry,
            index,
            // both exist -> use first
            &vec!["de".to_string(), "fr".to_string()],
        );
        assert_eq!(actual, "Schneller und privater Browser");

        let actual = format_item(
            "{Comment}",
            &mime,
            Some(&path),
            &entry,
            index,
            // 2nd exist -> use 2nd
            &vec!["foo".to_string(), "fr".to_string()],
        );
        assert_eq!(actual, "Navigateur rapide et privé");
    }

    #[test]
    fn match_entry_to_output() {
        let langs = vec![];
        let entries = [
            "./tests/assets/https/org.mozilla.firefox.desktop",
            "./tests/assets/https/org.mozilla.firefox.private.desktop",
            "./tests/assets/https/com.vivaldi.Vivaldi.desktop",
            "./tests/assets/https/com.vivaldi.Vivaldi.private.desktop",
            "./tests/assets/https/copy-to-clipboard.desktop",
        ]
        .map(|path| {
            DesktopEntry::parse_file(&PathBuf::from(path), &langs).unwrap()
        });
        let mime = Mime::from_str("x-scheme-handler/https").unwrap();
        let path: UserPath = "https://github.com/Anomalocaridid/handlr-regex"
            .parse()
            .unwrap();

        let assert_entry =
            |item_format: &str,
             output: &str,
             expected: Option<&DesktopEntry>| {
                let items = entries
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| {
                        let item = format_item(
                            item_format,
                            &mime,
                            Some(&path),
                            entry,
                            i,
                            &langs,
                        );
                        (entry, item)
                    })
                    .collect_vec();

                let actual = guess_matching_entry(items.as_slice(), output);
                assert_eq!(
                    actual.ok(),
                    expected,
                    "item_format='{}', output='{}', items={:?}",
                    item_format,
                    output,
                    items.iter().map(|(_, item)| item).collect_vec()
                );
            };

        let name = "Vivaldi (private)";
        let entry = entries.iter().find(|e| e.name == name).unwrap();
        let name_generic = format!(
            "{} {}",
            name,
            entry
                .get_first_with_language("GenericName", &langs)
                .unwrap()
        );

        assert_entry("{Name}", name, Some(entry));
        assert_entry("{Name}", &name_generic, None);
        assert_entry("{Name} {GenericName}", &name_generic, Some(entry));
        assert_entry("{Name}\0icon\x1f{Icon}", name, Some(entry));
        assert_entry("{Name}\x00icon\x1f{Icon}", name, Some(entry));
        assert_entry("{Name}\0icon\x1f{Icon}", &name_generic, None);
        assert_entry("{Name}\x00icon\x1f{Icon}", &name_generic, None);
        assert_entry("{Name} {GenericName}\0icon\x1f{Icon}", name, Some(entry));
        assert_entry(
            "{Name} {GenericName}\x00icon\x1f{Icon}",
            name,
            Some(entry),
        );
        assert_entry(
            "{Name} {GenericName}\0icon\x1f%icon",
            &name_generic,
            Some(entry),
        );

        let idx = entries.iter().position(|e| e.name == name).unwrap();
        assert_entry(r"{%Index0}", &format!("{}", idx), Some(entry));
        assert_entry(r"{%Index1}", &format!("{}", idx + 1), Some(entry));

        assert_entry("{Name}", "", None);
        assert_entry("{Name}", "FooBar", None);
    }
}
