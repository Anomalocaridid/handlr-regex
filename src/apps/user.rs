use crate::{
    common::{mime_types, DesktopHandler, Handleable},
    config::ConfigFile,
    error::{Error, Result},
};
use derive_more::{Deref, DerefMut};
use itertools::Itertools;
use mime::Mime;
use serde::{Deserialize, Serialize};
use serde_with::{
    serde_as, DeserializeFromStr, DisplayFromStr, SerializeDisplay,
};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Display,
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
};
use tracing::{debug, info};
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
            mime_types()
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
            mime_types()
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
        config_file: &ConfigFile,
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
                // Prepares for selector and filters out apps that do not exist
                let handlers = handlers
                    .iter()
                    .flat_map(|h| -> Result<(&DesktopHandler, String)> {
                        // Filtering breaks testing, so treat every app as valid
                        // TODO: test logging

                        if cfg!(test) {
                            Ok((h, h.to_string()))
                        } else {
                            let entry = h.get_entry();
                            if let Err(ref e) = entry {
                                debug!(
                                    "Desktop entry `{}` is invalid: {}",
                                    h, e
                                );
                            } else {
                                debug!("Desktop entry `{}` is valid", h);
                            }

                            Ok((h, entry?.name))
                        }
                    })
                    .collect_vec();

                debug!(
                    "Selector enabled: {}, number of set handlers: {}",
                    config_file.enable_selector,
                    handlers.len()
                );
                if config_file.enable_selector && handlers.len() > 1 {
                    info!("Running selector: {}", &config_file.selector);
                    let handler = {
                        let name = select(
                            &config_file.selector,
                            handlers.iter().map(|h| h.1.clone()),
                        )?;

                        handlers
                            .into_iter()
                            .find(|h| h.1 == name)
                            .ok_or(error)?
                            .0
                            .clone()
                    };

                    Ok(handler)
                } else {
                    info!("Not running selector, choosing first handler");
                    Ok(handlers.first().ok_or(error)?.0.clone())
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
        let mut config = xdg::BaseDirectories::new()?.get_config_home();
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

/// Run given selector command
#[mutants::skip] // Cannot test directly, runs external command
fn select<O: Iterator<Item = String>>(
    selector: &str,
    mut opts: O,
) -> Result<String> {
    use std::{io::prelude::*, process::Stdio};

    let process = {
        execute::command(selector)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?
    };

    let output = {
        process
            .stdin
            .ok_or_else(|| Error::Selector(selector.to_string()))?
            .write_all(opts.join("\n").as_bytes())?;

        let mut output = String::with_capacity(24);

        process
            .stdout
            .ok_or_else(|| Error::Selector(selector.to_string()))?
            .read_to_string(&mut output)?;

        output.trim_end().to_owned()
    };

    if output.is_empty() {
        Err(Error::Cancelled)
    } else {
        Ok(output)
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
                .get_handler_from_user(&mime::TEXT_PLAIN, &config_file)?
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
