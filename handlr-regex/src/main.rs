use clap::Parser;
use handlr_regex::{
    apps,
    cli::Cmd,
    common::{self, mime_table},
    error::{ErrorKind, Result},
    utils, Config, MimeApps, SystemApps,
};
use std::io::IsTerminal;

fn main() -> Result<()> {
    let config = Config::load();
    let mut mime_apps = MimeApps::read().unwrap_or_default();
    let system_apps = SystemApps::populate().unwrap_or_default();

    let res = || -> Result<()> {
        match Cmd::parse() {
            Cmd::Set { mime, handler } => {
                mime_apps.set_handler(mime.0, handler);
                mime_apps.save()?;
            }
            Cmd::Add { mime, handler } => {
                mime_apps.add_handler(mime.0, handler);
                mime_apps.save()?;
            }
            Cmd::Launch { mime, args } => {
                mime_apps.launch_handler(
                    &config,
                    &system_apps,
                    &mime.0,
                    args,
                )?;
            }
            Cmd::Get { mime, json } => {
                mime_apps.show_handler(&config, &system_apps, &mime.0, json)?;
            }
            Cmd::Open { paths } => {
                mime_apps.open_paths(&config, &system_apps, &paths)?
            }
            Cmd::Mime { paths, json } => {
                mime_table(&paths, json)?;
            }
            Cmd::List { all, json } => {
                mime_apps.print(&system_apps, all, json)?;
            }
            Cmd::Unset { mime } => {
                mime_apps.unset_handler(&mime.0)?;
            }
            Cmd::Remove { mime, handler } => {
                mime_apps.remove_handler(mime.0, handler)?;
            }
            Cmd::Autocomplete {
                desktop_files,
                mimes,
            } => {
                if desktop_files {
                    apps::SystemApps::list_handlers()?;
                } else if mimes {
                    common::db_autocomplete()?;
                }
            }
        }
        Ok(())
    }();

    match (res, std::io::stdout().is_terminal()) {
        (Err(e), _) if matches!(*e.kind, ErrorKind::Cancelled) => {
            std::process::exit(1);
        }
        (Err(e), true) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
        (Err(e), false) => {
            utils::notify("handlr error", &e.to_string())?;
            std::process::exit(1);
        }
        _ => Ok(()),
    }
}
