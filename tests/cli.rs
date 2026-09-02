use std::{
    io::Write,
    process::{Command, Stdio},
};

use serde_json::Value;

#[test]
fn help_should_exit_successfully_without_starting_tui() {
    let output = Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("A fast, fully local terminal todo list"));
    assert!(stdout.contains("shtodo add <TASK>"));
    assert!(stdout.contains("  add <TASK>  Add one task without opening the terminal UI.\n              When TASK is omitted, read it from standard input."));
    assert!(stdout.contains("printf 'Fix the bug\\n' | shtodo --local add"));
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

#[test]
fn add_argument_should_persist_trimmed_task_in_global_list() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .args(["add", "  hello world  "])
        .env("HOME", home.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Added: hello world\n"
    );
    assert_eq!(stored_task_text(home.path(), "global"), "hello world");
}

#[test]
fn local_add_should_persist_to_current_project_list() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .args(["--local", "add", "project task"])
        .env("HOME", home.path())
        .current_dir(project.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let projects = home.path().join(".shtodo/projects");
    let directory = std::fs::read_dir(projects)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        stored_task_text_at(&directory.join("tasks.json")),
        "project task"
    );
    assert!(!home.path().join(".shtodo/global/tasks.json").exists());
}

#[test]
fn add_without_argument_should_read_single_task_from_stdin() {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .arg("add")
        .env("HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"piped task\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Added: piped task\n"
    );
    assert_eq!(stored_task_text(home.path(), "global"), "piped task");
}

#[test]
fn add_without_any_input_should_fail_with_actionable_help() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .arg("add")
        .env("HOME", home.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("task text is required"));
    assert!(stderr.contains("shtodo add <TASK>"));
    assert!(stderr.contains("printf 'Fix the bug\\n' | shtodo --local add"));
    assert!(!home.path().join(".shtodo/global/tasks.json").exists());
}

#[test]
fn add_should_reject_multiline_stdin_without_persisting() {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .arg("add")
        .env("HOME", home.path())
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"first\nsecond\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("non-empty single line"));
    assert!(!home.path().join(".shtodo/global/tasks.json").exists());
}

fn stored_task_text(home: &std::path::Path, scope: &str) -> String {
    stored_task_text_at(&home.join(".shtodo").join(scope).join("tasks.json"))
}

fn stored_task_text_at(path: &std::path::Path) -> String {
    let snapshot: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    snapshot["tasks"][0]["text"].as_str().unwrap().to_owned()
}
