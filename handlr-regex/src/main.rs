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
    let config = Config::load().unwrap_or_default();
    let mut mime_apps = MimeApps::read().unwrap_or_default();
    let system_apps = SystemApps::populate().unwrap_or_default();

    let res = || -> Result<()> {
        match Cmd::parse() {
            Cmd::Set { mime, handler } => {
                mime_apps.set_handler(&mime, &handler);
                mime_apps.save()?;
            }
            Cmd::Add { mime, handler } => {
                mime_apps.add_handler(&mime, &handler);
                mime_apps.save()?;
            }
            Cmd::Launch {
                mime,
                args,
                selector,
                enable_selector,
                disable_selector,
            } => {
                mime_apps.launch_handler(
                    &config,
                    &system_apps,
                    &mime,
                    args,
                    &selector.unwrap_or(config.selector.clone()),
                    config.use_selector(enable_selector, disable_selector),
                )?;
            }
            Cmd::Get {
                mime,
                json,
                selector,
                enable_selector,
                disable_selector,
            } => {
                mime_apps.show_handler(
                    &config,
                    &system_apps,
                    &mime,
                    json,
                    &selector.unwrap_or(config.selector.clone()),
                    config.use_selector(enable_selector, disable_selector),
                )?;
            }
            Cmd::Open {
                paths,
                selector,
                enable_selector,
                disable_selector,
            } => mime_apps.open_paths(
                &config,
                &system_apps,
                &paths,
                &selector.unwrap_or(config.selector.clone()),
                config.use_selector(enable_selector, disable_selector),
            )?,
            Cmd::Mime { paths, json } => {
                mime_table(&paths, json)?;
            }
            Cmd::List { all, json } => {
                mime_apps.print(&system_apps, all, json)?;
            }
            Cmd::Unset { mime } => {
                mime_apps.unset_handler(&mime)?;
            }
            Cmd::Remove { mime, handler } => {
                mime_apps.remove_handler(&mime, &handler)?;
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
