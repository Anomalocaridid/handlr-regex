use crate::Result;

static CUSTOM_MIMES: &[&str] = &[
    "inode/directory",
    "x-scheme-handler/http",
    "x-scheme-handler/https",
    "x-scheme-handler/terminal",
];

pub fn autocomplete() -> Result<()> {
    use std::io::Write;

    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    mime_db::EXTENSIONS
        .iter()
        .try_for_each(|(ext, _)| writeln!(stdout, ".{}", ext))?;

    CUSTOM_MIMES
        .iter()
        .try_for_each(|mime| writeln!(stdout, "{}", mime))?;

    mime_db::TYPES
        .iter()
        .try_for_each(|(mime, _, _)| writeln!(stdout, "{}", mime))?;

    Ok(())
}
