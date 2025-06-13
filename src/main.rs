mod apps;
mod cli;
mod common;
mod config;
mod error;
mod logging;
mod testing;

use std::process::ExitCode;

use cli::{Cli, Cmd};
use common::mime_table;
use config::Config;
use error::Result;
use logging::init_tracing;

use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;
use tracing::debug;

#[mutants::skip] // Cannot test directly at the moment
fn main() -> ExitCode {
    // Shell completions
    CompleteEnv::with_factory(|| Cli::command().name("handlr"))
        .completer("handlr")
        .complete();

    let cli = Cli::parse();

    let _guard = init_tracing(&cli)
        .expect("handlr error: Could not initialize global tracing subscriber");

    error::handle(run(cli))
}

/// Run main program logic
#[mutants::skip] // Cannot test directly at the moment
fn run(cli: Cli) -> Result<()> {
    let mut config = Config::new(cli.terminal_output())?;

    let mut stdout = std::io::stdout().lock();

    debug!("Interactive terminal detected: {}", config.terminal_output);

    match cli.command {
        Cmd::Set { mime, handler } => config.set_handler(&mime, &handler),
        Cmd::Add { mime, handler } => config.add_handler(&mime, &handler),
        Cmd::Launch {
            mime,
            args,
            selector_args,
        } => {
            config.override_selector(selector_args);
            config.launch_handler(&mime, args)
        }
        Cmd::Get {
            mime,
            json,
            selector_args,
        } => {
            config.override_selector(selector_args);
            config.show_handler(&mut stdout, &mime, json)
        }
        Cmd::Open {
            paths,
            selector_args,
        } => {
            config.override_selector(selector_args);
            config.open_paths(&paths)
        }
        Cmd::Mime { paths, json } => {
            mime_table(&mut stdout, &paths, json, config.terminal_output)
        }
        Cmd::List { all, json } => config.print(&mut stdout, all, json),
        Cmd::Unset { mime } => config.unset_handler(&mime),
        Cmd::Remove { mime, handler } => config.remove_handler(&mime, &handler),
    }
}
