use clap::Parser;
use handlr_regex::{
    apps::{self, APPS},
    cli::Cmd,
    common::{self, mime_table},
    config::CONFIG,
    error::{ErrorKind, Result},
    utils,
};
use once_cell::sync::Lazy;
use std::io::IsTerminal;

fn main() -> Result<()> {
    // create config if it doesn't exist
    Lazy::force(&CONFIG);

    let mut apps = (*APPS).clone();

    let res = || -> Result<()> {
        match Cmd::parse() {
            Cmd::Set { mime, handler } => {
                apps.set_handler(mime.0, handler);
                apps.save()?;
            }
            Cmd::Add { mime, handler } => {
                apps.add_handler(mime.0, handler);
                apps.save()?;
            }
            Cmd::Launch { mime, args } => {
                apps.get_handler(&mime.0)?.launch(
                    args.into_iter().map(|a| a.to_string()).collect(),
                )?;
            }
            Cmd::Get { mime, json } => {
                apps.show_handler(&mime.0, json)?;
            }
            Cmd::Open { paths } => apps.open_paths(&paths)?,
            Cmd::Mime { paths, json } => {
                mime_table(&paths, json)?;
            }
            Cmd::List { all, json } => {
                apps.print(all, json)?;
            }
            Cmd::Unset { mime } => {
                apps.unset_handler(&mime.0)?;
            }
            Cmd::Remove { mime, handler } => {
                apps.remove_handler(mime.0, handler)?;
            }
            Cmd::Autocomplete {
                desktop_files,
                mimes,
            } => {
                if desktop_files {
                    apps::MimeApps::list_handlers()?;
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
