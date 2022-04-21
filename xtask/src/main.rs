use clap::{CommandFactory, Parser};
use handlr_regex::Cmd;
use std::{
    io::Result,
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    match Task::parse() {
        Task::Dist => dist()?,
    }

    Ok(())
}

fn dist() -> Result<()> {
    dist_manpage()
}

fn dist_manpage() -> Result<()> {
    let out_dir = dist_dir();
    let man = clap_mangen::Man::new(Cmd::command()).title("handlr");
    let mut buffer: Vec<u8> = Default::default();

    // Render man page
    man.render(&mut buffer)?;

    // Tweak man page and replace instances of handlr-regex with handlr except the title section
    let re = regex::bytes::Regex::new(r"handlr\\-regex\\")
        .expect("man page replacement regex invalid");

    let buffer = re.replace_all(&buffer, r"handlr\".as_bytes());

    // Write man page to file
    std::fs::create_dir_all(&out_dir)?;
    std::fs::write(&out_dir.join("handlr.1"), buffer)?;

    Ok(())
}

#[derive(Parser, Clone, Copy, Debug)]
enum Task {
    /// Build program and generate man page
    Dist,
}

fn dist_dir() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
        .join("target/dist")
}
