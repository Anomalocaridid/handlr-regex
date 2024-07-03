use crate::{
    apps::SystemApps, common::DesktopHandler, render_table, Config, Error,
    ErrorKind, Handleable, Handler, Result, UserPath,
};
use mime::Mime;
use pest::Parser;
use serde::Serialize;
use tabled::Tabled;

use std::{
    collections::{HashMap, VecDeque},
    io::{IsTerminal, Read},
    path::PathBuf,
    str::FromStr,
};

/// Represents user-configured mimeapps.list file
#[derive(Debug, Default, Clone, pest_derive::Parser)]
#[grammar = "common/ini.pest"]
pub struct MimeApps {
    added_associations: HashMap<Mime, VecDeque<DesktopHandler>>,
    default_apps: HashMap<Mime, VecDeque<DesktopHandler>>,
}

impl MimeApps {
    pub fn add_handler(&mut self, mime: Mime, handler: DesktopHandler) {
        self.default_apps
            .entry(mime)
            .or_default()
            .push_back(handler);
    }

    pub fn set_handler(&mut self, mime: Mime, handler: DesktopHandler) {
        self.default_apps.insert(mime, vec![handler].into());
    }

    pub fn unset_handler(&mut self, mime: &Mime) -> Result<()> {
        if let Some(_unset) = self.default_apps.remove(mime) {
            self.save()?;
        }

        Ok(())
    }

    pub fn remove_handler(
        &mut self,
        mime: Mime,
        handler: DesktopHandler,
    ) -> Result<()> {
        let handler_list = self.default_apps.entry(mime).or_default();

        if let Some(pos) = handler_list.iter().position(|x| *x == handler) {
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
                        Mime::from_str(&format!("{}/*", mime.type_())).unwrap();
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
        match self.default_apps.get(mime) {
            Some(handlers) if config.enable_selector && handlers.len() > 1 => {
                let handlers = handlers
                    .iter()
                    .map(|h| (h, h.get_entry().unwrap().name))
                    .collect::<Vec<_>>();

                let handler = {
                    let name =
                        config.select(handlers.iter().map(|h| h.1.clone()))?;

                    handlers
                        .into_iter()
                        .find(|h| h.1 == name)
                        .unwrap()
                        .0
                        .clone()
                };

                Ok(handler)
            }
            Some(handlers) => Ok(handlers.front().unwrap().clone()),
            None => Err(Error::from(ErrorKind::NotFound(mime.to_string()))),
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
    pub fn path() -> Result<PathBuf> {
        let mut config = xdg::BaseDirectories::new()?.get_config_home();
        config.push("mimeapps.list");
        Ok(config)
    }
    pub fn read() -> Result<Self> {
        let raw_conf = {
            let mut buf = String::new();
            let exists = std::path::Path::new(&Self::path()?).exists();
            std::fs::OpenOptions::new()
                .write(!exists)
                .create(!exists)
                .read(true)
                .open(Self::path()?)?
                .read_to_string(&mut buf)?;
            buf
        };
        let file = Self::parse(Rule::file, &raw_conf)?.next().unwrap();

        let mut current_section_name = "".to_string();
        let mut conf = Self {
            added_associations: HashMap::default(),
            default_apps: HashMap::default(),
        };

        file.into_inner().for_each(|line| {
            match line.as_rule() {
                Rule::section => {
                    current_section_name = line.into_inner().concat();
                }
                Rule::property => {
                    let mut inner_rules = line.into_inner(); // { name ~ "=" ~ value }

                    let name = inner_rules.next().unwrap().as_str();
                    let handlers = {
                        use itertools::Itertools;

                        inner_rules
                            .next()
                            .unwrap()
                            .as_str()
                            .split(';')
                            .filter(|s| !s.is_empty())
                            .unique()
                            .filter_map(|s| DesktopHandler::from_str(s).ok())
                            .collect::<VecDeque<_>>()
                    };

                    if !handlers.is_empty() {
                        match (
                            Mime::from_str(name),
                            current_section_name.as_str(),
                        ) {
                            (Ok(mime), "Added Associations") => {
                                conf.added_associations.insert(mime, handlers)
                            }

                            (Ok(mime), "Default Applications") => {
                                conf.default_apps.insert(mime, handlers)
                            }
                            _ => None,
                        };
                    }
                }
                _ => {}
            }
        });

        Ok(conf)
    }
    pub fn save(&self) -> Result<()> {
        use itertools::Itertools;
        use std::io::{prelude::*, BufWriter};

        let f = std::fs::OpenOptions::new()
            .read(true)
            .create(true)
            .write(true)
            .truncate(true)
            .open(Self::path()?)?;
        let mut writer = BufWriter::new(f);

        writer.write_all(b"[Added Associations]\n")?;
        for (k, v) in self.added_associations.iter().sorted() {
            writer.write_all(k.essence_str().as_ref())?;
            writer.write_all(b"=")?;
            writer.write_all(v.iter().join(";").as_ref())?;
            writer.write_all(b";\n")?;
        }

        writer.write_all(b"\n[Default Applications]\n")?;
        for (k, v) in self.default_apps.iter().sorted() {
            writer.write_all(k.essence_str().as_ref())?;
            writer.write_all(b"=")?;
            writer.write_all(v.iter().join(";").as_ref())?;
            writer.write_all(b";\n")?;
        }

        writer.flush()?;
        Ok(())
    }
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
    pub fn list_handlers() -> Result<()> {
        use std::{io::Write, os::unix::ffi::OsStrExt};

        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();

        SystemApps::get_entries()?.for_each(|(_, e)| {
            stdout.write_all(e.file_name.as_bytes()).unwrap();
            stdout.write_all(b"\t").unwrap();
            stdout.write_all(e.name.as_bytes()).unwrap();
            stdout.write_all(b"\n").unwrap();
        });

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
}

/// Internal helper struct for turning MimeApps into tabular data
#[derive(PartialEq, Eq, PartialOrd, Ord, Tabled, Serialize)]
struct MimeAppsEntry {
    mime: String,
    #[tabled(display_with("Self::display_handlers", self))]
    handlers: Vec<String>,
}

impl MimeAppsEntry {
    fn new(mime: &Mime, handlers: &VecDeque<DesktopHandler>) -> Self {
        Self {
            mime: mime.to_string(),
            handlers: handlers
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>(),
        }
    }

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
    fn new(mimeapps: &MimeApps, system_apps: &SystemApps) -> Self {
        fn to_entries(
            map: &HashMap<Mime, VecDeque<DesktopHandler>>,
        ) -> Vec<MimeAppsEntry> {
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
            Mime::from_str("video/*").unwrap(),
            DesktopHandler::assume_valid("mpv.desktop".into()),
        );
        user_apps.add_handler(
            Mime::from_str("video/webm").unwrap(),
            DesktopHandler::assume_valid("brave.desktop".into()),
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
