mod regex;
mod system;
mod user;

pub use self::regex::{RegexApps, RegexHandler};
pub use system::SystemApps;
pub use user::{MimeApps, Rule as MimeappsRule, APPS};
