mod apps;
mod cli;
mod common;
mod config;
mod error;
mod utils;

use apps::SystemApps;
use cli::Cmd;
use common::mime_table;
use config::Config;
use error::{ErrorKind, Result};

use clap::Parser;
use std::io::IsTerminal;

#[mutants::skip] // Cannot test directly at the moment
fn main() -> Result<()> {
    let mut config = Config::new().unwrap_or_default();
    let terminal_output = std::io::stdout().is_terminal();
    let mut stdout = std::io::stdout().lock();

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
                    &mut stdout,
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
                mime_table(&mut stdout, &paths, json, terminal_output)?;
            }
            Cmd::List { all, json } => {
                config.print(&mut stdout, all, json)?;
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
                    SystemApps::list_handlers(&mut stdout)?;
                } else if mimes {
                    common::db_autocomplete(&mut stdout)?;
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
