use std::process::Command;

#[test]
fn help_should_exit_successfully_without_starting_tui() {
    let output = Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("shtodo --local"));
}

#[test]
fn unknown_argument_should_fail_with_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .arg("--wat")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage:"));
}
