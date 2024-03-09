use clap::{CommandFactory, Parser};
use handlr_regex::Cmd;
use std::{
    env,
    error::Error,
    fs::remove_file,
    path::{Path, PathBuf},
};

type DynResult = Result<(), Box<dyn Error>>;

fn main() -> DynResult {
    match Task::parse() {
        Task::Mangen => mangen()?,
    }

    Ok(())
}

/// Action for `cargo xtask mangen`
/// Generate man page for binary and subcommands
fn mangen() -> DynResult {
    eprintln!("Generating man pages");

    let out_dir = assets_dir().join("manual/man1");
    let cmd = Cmd::command().name("handlr");

    clap_mangen::generate_to(cmd, &out_dir)?;

    // Remove hidden subcommand's manpage
    remove_file(out_dir.join("handlr-autocomplete.1"))?;

    Ok(())
}

#[derive(Parser, Clone, Copy, Debug)]
enum Task {
    /// generate man page
    Mangen,
}

// Project root
fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}

/// Output directory for generated assets
fn assets_dir() -> PathBuf {
    project_root().join("assets")
}
