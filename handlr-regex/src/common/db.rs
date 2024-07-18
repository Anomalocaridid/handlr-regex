use crate::error::Result;
use std::io::Write;

static CUSTOM_MIMES: &[&str] = &[
    "inode/directory",
    "x-scheme-handler/http",
    "x-scheme-handler/https",
    "x-scheme-handler/terminal",
];

pub fn autocomplete<W: Write>(writer: &mut W) -> Result<()> {
    mime_db::EXTENSIONS
        .iter()
        .try_for_each(|(ext, _)| writeln!(writer, ".{}", ext))?;

    CUSTOM_MIMES
        .iter()
        .try_for_each(|mime| writeln!(writer, "{}", mime))?;

    mime_db::TYPES
        .iter()
        .try_for_each(|(mime, _, _)| writeln!(writer, "{}", mime))?;

    Ok(())
}
