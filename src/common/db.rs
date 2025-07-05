use std::sync::LazyLock;

use itertools::Itertools;

/// Comprehensive list of mimetypes
pub static MIME_TYPES: LazyLock<Vec<String>> = LazyLock::new(|| {
    // Mimes that are not in mime_db::TYPES
    [
        "inode/directory",
        "x-scheme-handler/http",
        "x-scheme-handler/https",
        "x-scheme-handler/terminal",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(
        mime_db::TYPES
            .into_iter()
            .map(|(mime, _, _)| mime.to_string()),
    )
    .collect_vec()
});
