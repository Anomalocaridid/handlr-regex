use crate::{
    apps::SystemApps, common::DesktopHandler, render_table, Config, Error,
    ErrorKind, Handleable, Handler, Result, UserPath,
};
use derive_more::{Deref, DerefMut};
use itertools::Itertools;
use mime::Mime;
use serde::{Deserialize, Serialize};
use serde_with::{
    serde_as, DeserializeFromStr, DisplayFromStr, SerializeDisplay,
};
use std::{
    collections::{HashMap, VecDeque},
    fmt::Display,
    io::IsTerminal,
    path::PathBuf,
    str::FromStr,
};
use tabled::Tabled;

/// Helper struct for a list of `DesktopHandler`s
#[serde_as]
#[derive(
    Debug, Default, Clone, Deref, DerefMut, SerializeDisplay, DeserializeFromStr,
)]
pub struct DesktopList(VecDeque<DesktopHandler>);

impl FromStr for DesktopList {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            s.split(';')
                .filter(|s| !s.is_empty()) // Account for ending semicolon
                .unique()
                .filter_map(|s| DesktopHandler::from_str(s).ok())
                .collect::<VecDeque<DesktopHandler>>(),
        ))
    }
}

/// Represents user-configured mimeapps.list file
#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MimeApps {
    #[serde(rename = "Added Associations")]
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    added_associations: HashMap<Mime, DesktopList>,
    #[serde(rename = "Default Applications")]
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    default_apps: HashMap<Mime, DesktopList>,
}

impl Display for DesktopList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Ensure final semicolon is added
        write!(f, "{};", self.iter().join(";"))
    }
}

impl MimeApps {
    /// Add a handler to an existing default application association
    pub fn add_handler(&mut self, mime: &Mime, handler: &DesktopHandler) {
        self.default_apps
            .entry(mime.clone())
            .or_default()
            .push_back(handler.clone());
    }

    /// Set a default application association, overwriting any existing association for the same mimetype
    pub fn set_handler(&mut self, mime: &Mime, handler: &DesktopHandler) {
        self.default_apps
            .insert(mime.clone(), DesktopList(vec![handler.clone()].into()));
    }

    /// Entirely remove a given mime's default application association
    pub fn unset_handler(&mut self, mime: &Mime) -> Result<()> {
        if let Some(_unset) = self.default_apps.remove(mime) {
            self.save()?;
        }

        Ok(())
    }

    /// Remove a given handler from a given mime's default file associaion
    pub fn remove_handler(
        &mut self,
        mime: &Mime,
        handler: &DesktopHandler,
    ) -> Result<()> {
        let handler_list = self.default_apps.entry(mime.clone()).or_default();

        if let Some(pos) = handler_list.iter().position(|x| *x == *handler) {
            if let Some(_removed) = handler_list.remove(pos) {
                self.save()?
            }
        }

        Ok(())
    }

    /// Get the handler associated with a given mime
    pub fn get_handler(
        &self,
        config: &Config,
        system_apps: &SystemApps,
        mime: &Mime,
    ) -> Result<DesktopHandler> {
        match self.get_handler_from_user(config, mime) {
            Err(e) if matches!(*e.kind, ErrorKind::Cancelled) => Err(e),
            h => h
                .or_else(|_| {
                    let wildcard =
                        Mime::from_str(&format!("{}/*", mime.type_()))?;
                    self.get_handler_from_user(config, &wildcard)
                })
                .or_else(|_| {
                    self.get_handler_from_added_associations(system_apps, mime)
                }),
        }
    }

    /// Get the handler associated with a given path
    fn get_handler_from_path(
        &self,
        config: &Config,
        system_apps: &SystemApps,
        path: &UserPath,
    ) -> Result<Handler> {
        Ok(if let Ok(handler) = config.get_regex_handler(path) {
            handler.into()
        } else {
            self.get_handler(config, system_apps, &path.get_mime()?)?
                .into()
        })
    }

    /// Get the handler associated with a given mime from mimeapps.list's default apps
    fn get_handler_from_user(
        &self,
        config: &Config,
        mime: &Mime,
    ) -> Result<DesktopHandler> {
        let error = Error::from(ErrorKind::NotFound(mime.to_string()));
        match self.default_apps.get(mime) {
            Some(handlers) if config.enable_selector && handlers.len() > 1 => {
                let handlers = handlers
                    .iter()
                    .map(|h| Ok((h, h.get_entry()?.name)))
                    .collect::<Result<Vec<_>>>()?;

                let handler = {
                    let name =
                        config.select(handlers.iter().map(|h| h.1.clone()))?;

                    handlers
                        .into_iter()
                        .find(|h| h.1 == name)
                        .ok_or(error)?
                        .0
                        .clone()
                };

                Ok(handler)
            }
            Some(handlers) => Ok(handlers.front().ok_or(error)?.clone()),
            None => Err(error),
        }
    }

    /// Get the handler associated with a given mime from mimeapps.list's added associations
    fn get_handler_from_added_associations(
        &self,
        system_apps: &SystemApps,
        mime: &Mime,
    ) -> Result<DesktopHandler> {
        self.added_associations
            .get(mime)
            .map_or_else(
                || system_apps.get_handler(mime),
                |h| h.front().cloned(),
            )
            .ok_or_else(|| Error::from(ErrorKind::NotFound(mime.to_string())))
    }

    /// Get the handler associated with a given mime
    pub fn show_handler(
        &mut self,
        config: &Config,
        system_apps: &SystemApps,
        mime: &Mime,
        output_json: bool,
    ) -> Result<()> {
        let handler = self.get_handler(config, system_apps, mime)?;
        let output = if output_json {
            let entry = handler.get_entry()?;
            let cmd = entry.get_cmd(config, self, system_apps, vec![])?;

            (serde_json::json!( {
                "handler": handler.to_string(),
                "name": entry.name,
                "cmd": cmd.0 + " " + &cmd.1.join(" "),
            }))
            .to_string()
        } else {
            handler.to_string()
        };
        println!("{}", output);
        Ok(())
    }

    /// Get the path to the user's mimeapps.list file
    pub fn path() -> Result<PathBuf> {
        let mut config = xdg::BaseDirectories::new()?.get_config_home();
        config.push("mimeapps.list");
        Ok(config)
    }

    /// Read and parse mimeapps.list
    pub fn read() -> Result<Self> {
        let exists = std::path::Path::new(&Self::path()?).exists();

        let file = std::fs::OpenOptions::new()
            .write(!exists)
            .create(!exists)
            .read(true)
            .open(Self::path()?)?;

        let mut mimeapps: Self = serde_ini::de::from_read(file)?;

        // Remove empty default associations
        // Can happen if all handlers set are invalid (e.g. do not exist)
        mimeapps.default_apps.retain(|_, h| !h.is_empty());

        Ok(mimeapps)
    }

    /// Save associations to mimeapps.list
    pub fn save(&self) -> Result<()> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .create(true)
            .write(true)
            .truncate(true)
            .open(Self::path()?)?;

        serde_ini::ser::to_writer(file, self)?;

        Ok(())
    }

    /// Print the set associations and system-level associations in a table
    pub fn print(
        &self,
        system_apps: &SystemApps,
        detailed: bool,
        output_json: bool,
    ) -> Result<()> {
        let mimeapps_table = MimeAppsTable::new(self, system_apps);

        if detailed {
            if output_json {
                println!("{}", serde_json::to_string(&mimeapps_table)?)
            } else {
                println!("Default Apps");
                println!("{}", render_table(&mimeapps_table.default_apps));
                if !self.added_associations.is_empty() {
                    println!("Added associations");
                    println!(
                        "{}",
                        render_table(&mimeapps_table.added_associations)
                    );
                }
                println!("System Apps");
                println!("{}", render_table(&mimeapps_table.system_apps))
            }
        } else if output_json {
            println!("{}", serde_json::to_string(&mimeapps_table.default_apps)?)
        } else {
            println!("{}", render_table(&mimeapps_table.default_apps))
        }

        Ok(())
    }

    /// Open the given paths with their respective handlers
    pub fn open_paths(
        &mut self,
        config: &Config,
        system_apps: &SystemApps,
        paths: &[UserPath],
    ) -> Result<()> {
        let mut handlers: HashMap<Handler, Vec<String>> = HashMap::new();

        for path in paths.iter() {
            handlers
                .entry(self.get_handler_from_path(config, system_apps, path)?)
                .or_default()
                .push(path.to_string())
        }

        for (handler, paths) in handlers.into_iter() {
            handler.open(config, self, system_apps, paths)?;
        }

        Ok(())
    }

    /// Given a mime and arguments, launch the associated handler with the arguments
    pub fn launch_handler(
        &mut self,
        config: &Config,
        system_apps: &SystemApps,
        mime: &Mime,
        args: Vec<UserPath>,
    ) -> Result<()> {
        self.get_handler(config, system_apps, mime)?.launch(
            config,
            self,
            system_apps,
            args.into_iter().map(|a| a.to_string()).collect(),
        )
    }
}

/// Internal helper struct for turning MimeApps into tabular data
#[derive(PartialEq, Eq, PartialOrd, Ord, Tabled, Serialize)]
struct MimeAppsEntry {
    mime: String,
    #[tabled(display_with("Self::display_handlers", self))]
    handlers: Vec<String>,
}

impl MimeAppsEntry {
    /// Create a new `MimeAppsEntry`
    fn new(mime: &Mime, handlers: &VecDeque<DesktopHandler>) -> Self {
        Self {
            mime: mime.to_string(),
            handlers: handlers
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>(),
        }
    }

    /// Display list of handlers as a string
    fn display_handlers(&self) -> String {
        // If output is a terminal, optimize for readability
        // Otherwise, if piped, optimize for parseability
        let separator = if std::io::stdout().is_terminal() {
            ",\n"
        } else {
            ", "
        };

        self.handlers.join(separator)
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
    fn new(mimeapps: &MimeApps, system_apps: &SystemApps) -> Self {
        fn to_entries(map: &HashMap<Mime, DesktopList>) -> Vec<MimeAppsEntry> {
            let mut rows = map
                .iter()
                .map(|(mime, handlers)| MimeAppsEntry::new(mime, handlers))
                .collect::<Vec<_>>();
            rows.sort_unstable();
            rows
        }
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
        let mut user_apps = MimeApps::default();
        user_apps.add_handler(
            &Mime::from_str("video/*")?,
            &DesktopHandler::assume_valid("mpv.desktop".into()),
        );
        user_apps.add_handler(
            &Mime::from_str("video/webm")?,
            &DesktopHandler::assume_valid("brave.desktop".into()),
        );

        let config = Config::default();
        let system_apps = SystemApps::default();

        assert_eq!(
            user_apps
                .get_handler(
                    &config,
                    &system_apps,
                    &Mime::from_str("video/mp4")?
                )?
                .to_string(),
            "mpv.desktop"
        );
        assert_eq!(
            user_apps
                .get_handler(
                    &config,
                    &system_apps,
                    &Mime::from_str("video/asdf")?
                )?
                .to_string(),
            "mpv.desktop"
        );

        assert_eq!(
            user_apps
                .get_handler(
                    &config,
                    &system_apps,
                    &Mime::from_str("video/webm")?
                )?
                .to_string(),
            "brave.desktop"
        );

        Ok(())
    }
}
