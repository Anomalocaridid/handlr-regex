use clap::Parser;
use handlr_regex::{
    apps::SystemApps,
    cli::Cmd,
    common::{self, mime_table},
    config::AppsConfig,
    error::{ErrorKind, Result},
    utils,
};
use std::io::IsTerminal;

fn main() -> Result<()> {
    let mut apps_config = AppsConfig::new().unwrap_or_default();

    let res = || -> Result<()> {
        match Cmd::parse() {
            Cmd::Set { mime, handler } => {
                apps_config.set_handler(&mime, &handler)?
            }
            Cmd::Add { mime, handler } => {
                apps_config.add_handler(&mime, &handler)?
            }
            Cmd::Launch {
                mime,
                args,
                selector,
                enable_selector,
                disable_selector,
            } => {
                apps_config.launch_handler(
                    &mime,
                    args,
                    selector,
                    enable_selector,
                    disable_selector,
                )?;
            }
            Cmd::Get {
                mime,
                json,
                selector,
                enable_selector,
                disable_selector,
            } => {
                apps_config.show_handler(
                    &mime,
                    json,
                    selector,
                    enable_selector,
                    disable_selector,
                )?;
            }
            Cmd::Open {
                paths,
                selector,
                enable_selector,
                disable_selector,
            } => apps_config.open_paths(
                &paths,
                selector,
                enable_selector,
                disable_selector,
            )?,
            Cmd::Mime { paths, json } => {
                mime_table(&paths, json)?;
            }
            Cmd::List { all, json } => {
                apps_config.print(all, json)?;
            }
            Cmd::Unset { mime } => {
                apps_config.unset_handler(&mime)?;
            }
            Cmd::Remove { mime, handler } => {
                apps_config.remove_handler(&mime, &handler)?;
            }
            Cmd::Autocomplete {
                desktop_files,
                mimes,
            } => {
                if desktop_files {
                    SystemApps::list_handlers()?;
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
