use crate::error::Result;

/// Issue a notification
#[mutants::skip] // Cannot test directly, runs command
pub fn notify(title: &str, msg: &str) -> Result<()> {
    std::process::Command::new("notify-send")
        .args(["-t", "10000", title, msg])
        .spawn()?;
    Ok(())
}
