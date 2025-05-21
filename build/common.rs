// This file exists solely to trick build script into working
// These types are used by cli.rs, which cannot be transitively imported
// because they rely on their own dependencies and so on

use once_cell::sync::Lazy;

pub type DesktopHandler = String;
pub type MimeType = String;
pub type UserPath = String;

pub static MIME_TYPES: Lazy<Vec<String>> = Lazy::new(|| vec!["".to_string()]);
