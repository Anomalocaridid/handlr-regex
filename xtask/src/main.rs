use clap::{CommandFactory, Parser};
use handlr_regex::Cmd;
use std::{
    error::Error,
    fs::{create_dir_all, remove_dir_all, write},
    path::{Path, PathBuf},
};

type DynResult = Result<(), Box<dyn Error>>;

fn main() -> DynResult {
    match Task::parse() {
        Task::Dist => dist()?,
    }

    Ok(())
}

/// Action for `cargo xtask dist`
fn dist() -> DynResult {
    if remove_dir_all(dist_dir()).is_ok() {
        eprintln!("Deleted {}", dist_dir().to_str().unwrap());
    };

    dist_manpage()
}

/// Generate man page for binary and subcommands
fn dist_manpage() -> DynResult {
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
        cmd.clone()
    } else {
        cmd.clone().name(format!("handlr-regex-{}", old_name))
    };

    let man = clap_mangen::Man::new(cmd.clone());
    let mut buffer: Vec<u8> = Default::default();

    // Render man page
    man.render(&mut buffer)?;

    // Tweak man page
    let (regex, replace) = if is_main_cmd {
        // For main man page, replace instances of handlr-regex with handlr except the name section
        (r"handlr\\-regex\\", r"handlr\")
    } else {
        // For subcommands, replace dash in synopsis command name with space
        (
            r"handlr\\-regex\\-(?P<name>[[:alpha:]]+)\\",
            r"handlr $name\",
        )
    };

    let re = regex::bytes::Regex::new(regex)?;
    let buffer = re.replace_all(&buffer, replace.as_bytes());

    let out_dir = dist_dir();

    // Write man page to file
    create_dir_all(&out_dir)?;

    let file = if is_main_cmd {
        "handlr.1".to_string()
    } else {
        format!("handlr-{}.1", old_name)
    };

    let file = out_dir.join(file);

    write(&file, buffer)?;

    eprintln!("Created {}", file.to_str().unwrap());

    Ok(())
}

#[derive(Parser, Clone, Copy, Debug)]
enum Task {
    /// Build program and generate man page
    Dist,
}

/// Output directory for `cargo xtast dist`
fn dist_dir() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
        .join("target/dist")
}
