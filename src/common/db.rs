use once_cell::sync::Lazy;

static CUSTOM_MIMES: &[&str] = &[
    "inode/directory",
    "x-scheme-handler/http",
    "x-scheme-handler/https",
    "x-scheme-handler/terminal",
];

/// A list of known mime types
pub static MIME_TYPES: Lazy<Vec<String>> = Lazy::new(|| {
    CUSTOM_MIMES
        .iter()
        .map(|s| s.to_string())
        .chain(
            mime_db::TYPES
                .into_iter()
                .map(|(mime, _, _)| mime.to_string()),
        )
        .collect()
});
