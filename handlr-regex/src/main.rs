use clap::Parser;
use handlr_regex::{
    apps::SystemApps,
    cli::Cmd,
    common::{self, mime_table},
    config::Config,
    error::{ErrorKind, Result},
    utils,
};
use std::io::IsTerminal;

fn main() -> Result<()> {
    let mut config = Config::new().unwrap_or_default();
    let terminal_output = std::io::stdout().is_terminal();

    let res = || -> Result<()> {
        match Cmd::parse() {
            Cmd::Set { mime, handler } => {
                config.set_handler(&mime, &handler)?
            }
            Cmd::Add { mime, handler } => {
                config.add_handler(&mime, &handler)?
            }
            Cmd::Launch {
                mime,
                args,
                selector,
                enable_selector,
                disable_selector,
            } => {
                config.launch_handler(
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
                config.show_handler(
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
            } => config.open_paths(
                &paths,
                selector,
                enable_selector,
                disable_selector,
            )?,
            Cmd::Mime { paths, json } => {
                mime_table(&paths, json, terminal_output)?;
            }
            Cmd::List { all, json } => {
                config.print(all, json, terminal_output)?;
            }
            Cmd::Unset { mime } => {
                config.unset_handler(&mime)?;
            }
            Cmd::Remove { mime, handler } => {
                config.remove_handler(&mime, &handler)?;
            }
            Cmd::Autocomplete {
                desktop_files,
                mimes,
            } => {
                if desktop_files {
                    SystemApps::list_handlers()?;
                } else if mimes {
                    common::db_autocomplete(&mut std::io::stdout().lock())?;
                }
            }
        }
        Ok(())
    }();

    match (res, terminal_output) {
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
