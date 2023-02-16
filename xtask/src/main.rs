use clap::{CommandFactory, Parser};
use handlr_regex::Cmd;
use std::{
    env,
    error::Error,
    fs,
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

    let cmd = Cmd::command();
    generate_manpage(&cmd)?;

    for sub in cmd.get_subcommands() {
        generate_manpage(sub)?
    }

    Ok(())
}

/// Generate man page for one command
fn generate_manpage(cmd: &clap::Command) -> DynResult {
    if cmd.is_hide_set() {
        return Ok(());
    }

    let old_name = cmd.get_name();
    let is_main_cmd = old_name == "handlr-regex";

    let cmd = if is_main_cmd {
        cmd.clone().name("handlr")
    } else {
        cmd.clone().name(format!("handlr-{}", old_name))
    };

    let man = clap_mangen::Man::new(cmd.clone());
    let mut buffer: Vec<u8> = Default::default();

    // Render man page
    man.render(&mut buffer)?;

    // Add "-regex" to (sub)command name
    let buffer =
        regex::bytes::Regex::new(r"handlr(?P<name>\\-[[:alpha:]]+)? \\-")?
            .replace(&buffer, r"handlr-regex$name -".as_bytes());

    // Replace dash in subcommands' synopsis command names with a space
    let buffer =
        regex::bytes::Regex::new(r"handlr\\-(?P<name>[[:alpha:]]+)\\")?
            .replace(&buffer, r"handlr $name\".as_bytes());

    let out_dir = assets_dir().join("manual/man1");

    // Write man page to file
    fs::create_dir_all(&out_dir)?;

    let file = if is_main_cmd {
        "handlr.1".to_string()
    } else {
        format!("handlr-{}.1", old_name)
    };

    let file = out_dir.join(file);

    fs::write(&file, buffer)?;

    eprintln!("Created {}", file.to_str().unwrap());

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

/// Output directory for `cargo xtast dist`
fn assets_dir() -> PathBuf {
    project_root().join("assets")
}
