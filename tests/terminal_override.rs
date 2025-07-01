#[path = "../src/testing.rs"]
mod testing;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::process::Command;

/// Helper function to test terminal output detection
fn test_terminal_output(terminal_output: bool) -> Command {
    let mut cmd = Command::new(get_cargo_bin("handlr"));
    cmd.arg(format!("--force-terminal-output={}", terminal_output))
        .arg("-vvv") // Maximum verbosity
        .arg("--disable-notifications") // Not much point showing these in tests
        .arg("mime")
        .arg("./tests/assets");
    cmd
}

#[test]
fn terminal_output_tests_force_true() {
    insta::with_settings!(
        {
            filters => testing::timestamp_filter()
        },
        { assert_cmd_snapshot!(test_terminal_output(true)) }
    )
}

#[test]
fn terminal_output_tests_force_false() {
    insta::with_settings!(
        {
            filters => testing::timestamp_filter()
        },
        { assert_cmd_snapshot!(test_terminal_output(false)) }
    )
}
