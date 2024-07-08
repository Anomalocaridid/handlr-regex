use clap::Parser;
use handlr_regex::{
    apps::SystemApps,
    apps_config::AppsConfig,
    cli::Cmd,
    common::{self, mime_table},
    error::{ErrorKind, Result},
    utils,
};
use std::io::IsTerminal;

fn main() -> Result<()> {
    let mut apps_config = AppsConfig::new().unwrap_or_default();

    let res = || -> Result<()> {
        match Cmd::parse() {
            Cmd::Set { mime, handler } => {
                apps_config.mime_apps.set_handler(&mime, &handler);
                apps_config.mime_apps.save()?;
            }
            Cmd::Add { mime, handler } => {
                apps_config.mime_apps.add_handler(&mime, &handler);
                apps_config.mime_apps.save()?;
            }
            Cmd::Launch {
                mime,
                args,
                selector,
                enable_selector,
                disable_selector,
            } => {
                apps_config.mime_apps.launch_handler(
                    &apps_config.config,
                    &apps_config.system_apps,
                    &mime,
                    args,
                    &selector.unwrap_or(apps_config.config.selector.clone()),
                    apps_config
                        .config
                        .use_selector(enable_selector, disable_selector),
                )?;
            }
            Cmd::Get {
                mime,
                json,
                selector,
                enable_selector,
                disable_selector,
            } => {
                apps_config.mime_apps.show_handler(
                    &apps_config.config,
                    &apps_config.system_apps,
                    &mime,
                    json,
                    &selector.unwrap_or(apps_config.config.selector.clone()),
                    apps_config
                        .config
                        .use_selector(enable_selector, disable_selector),
                )?;
            }
            Cmd::Open {
                paths,
                selector,
                enable_selector,
                disable_selector,
            } => apps_config.mime_apps.open_paths(
                &apps_config.config,
                &apps_config.system_apps,
                &paths,
                &selector.unwrap_or(apps_config.config.selector.clone()),
                apps_config
                    .config
                    .use_selector(enable_selector, disable_selector),
            )?,
            Cmd::Mime { paths, json } => {
                mime_table(&paths, json)?;
            }
            Cmd::List { all, json } => {
                apps_config.mime_apps.print(
                    &apps_config.system_apps,
                    all,
                    json,
                )?;
            }
            Cmd::Unset { mime } => {
                apps_config.mime_apps.unset_handler(&mime)?;
            }
            Cmd::Remove { mime, handler } => {
                apps_config.mime_apps.remove_handler(&mime, &handler)?;
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
