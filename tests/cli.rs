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
    assert!(stdout.contains("shtodo list"));
    assert!(stdout.contains("shtodo --local list"));
    assert!(stdout.contains("shtodo delete <ID>"));
    assert!(stdout.contains("shtodo --local delete <ID>"));
    assert!(stdout.contains("shtodo doctor"));
    assert!(stdout.contains("Validate ~/.shtodo/config.toml"));
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

#[test]
fn doctor_should_accept_missing_config_without_creating_storage() {
    let home = tempfile::tempdir().unwrap();

    let output = run_with_home(home.path(), &["doctor"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Config:"));
    assert!(stdout.contains("OK: no config file; using defaults"));
    assert!(!home.path().join(".shtodo").exists());
}

#[test]
fn doctor_should_summarize_valid_effective_keymap_without_task_storage() {
    let home = tempfile::tempdir().unwrap();
    write_config(home.path(), "[keybindings.normal]\nmove_down = [\"x\"]\n");

    let output = run_with_home(home.path(), &["doctor"]);

    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("OK: 24 configurable actions, 32 active bindings")
    );
    assert!(!home.path().join(".shtodo/global").exists());
    assert!(!home.path().join(".shtodo/projects").exists());
}

#[test]
fn doctor_should_report_all_available_invalid_config_issues() {
    let home = tempfile::tempdir().unwrap();
    write_config(
        home.path(),
        "[keybindings.normal]\nmove_down = [\"dn\"]\nadd_task = [\"d\"]\n",
    );

    let output = run_with_home(home.path(), &["doctor"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Invalid configuration:"));
    assert!(stderr.contains("keybindings.normal.move_down"));
    assert!(stderr.contains("unknown key \"dn\""));
    assert!(stderr.contains("keybindings.normal.add_task"));
    assert!(stderr.contains("conflicts with delete_task"));
    assert!(!home.path().join(".shtodo/global").exists());
}

#[test]
fn interactive_startup_should_reject_invalid_config_before_creating_task_storage() {
    let home = tempfile::tempdir().unwrap();
    write_config(home.path(), "[keybindings.normal]\nmove_down = [\"dn\"]\n");

    let output = run_with_home(home.path(), &[]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown key \"dn\""));
    assert!(stderr.contains("Run `shtodo doctor`"));
    assert!(!home.path().join(".shtodo/global").exists());
}

#[test]
fn add_should_remain_available_while_interactive_config_is_invalid() {
    let home = tempfile::tempdir().unwrap();
    write_config(home.path(), "[keybindings.normal]\nmove_down = [\"dn\"]\n");

    let output = run_with_home(home.path(), &["add", "repair config later"]);

    assert!(output.status.success());
    assert_eq!(
        stored_task_text(home.path(), "global"),
        "repair config later"
    );
}

#[test]
fn list_missing_global_scope_should_be_empty_and_side_effect_free() {
    let home = tempfile::tempdir().unwrap();

    let output = run_with_home(home.path(), &["list"]);

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(!home.path().join(".shtodo").exists());
}

#[test]
fn list_should_print_exact_ids_states_and_text_in_canonical_order() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        run_with_home(home.path(), &["add", "Fix the bug"])
            .status
            .success()
    );
    assert!(
        run_with_home(home.path(), &["add", "Run the tests"])
            .status
            .success()
    );
    let path = snapshot_path(home.path(), "global");
    let mut snapshot = read_snapshot(&path);
    snapshot["tasks"][1]["completed"] = Value::Bool(true);
    write_snapshot(&path, &snapshot);

    let output = run_with_home(home.path(), &["list"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1  open  Fix the bug\n2  done  Run the tests\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn list_should_omit_tombstones_without_renumbering_ids() {
    let home = tempfile::tempdir().unwrap();
    for task in ["first", "deleted", "third"] {
        assert!(run_with_home(home.path(), &["add", task]).status.success());
    }
    let path = snapshot_path(home.path(), "global");
    let mut snapshot = read_snapshot(&path);
    snapshot["tasks"][1]["deletion_sequence"] = Value::from(1);
    snapshot["next_deletion_sequence"] = Value::from(2);
    write_snapshot(&path, &snapshot);

    let output = run_with_home(home.path(), &["list"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1  open  first\n3  open  third\n"
    );
}

#[test]
fn list_should_keep_global_and_exact_local_scopes_isolated() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    assert!(
        run_with_home(home.path(), &["add", "global task"])
            .status
            .success()
    );
    assert!(
        run_with_home_at(
            home.path(),
            project.path(),
            &["--local", "add", "local task"]
        )
        .status
        .success()
    );

    let global = run_with_home_at(home.path(), project.path(), &["list"]);
    let local = run_with_home_at(home.path(), project.path(), &["--local", "list"]);

    assert!(global.status.success());
    assert!(local.status.success());
    assert_eq!(
        String::from_utf8_lossy(&global.stdout),
        "1  open  global task\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&local.stdout),
        "1  open  local task\n"
    );
}

#[test]
fn list_should_reject_invalid_snapshots_without_modifying_them() {
    let documents = [
        b"{".as_slice(),
        br#"{"schema_version":1,"scope":{"kind":"project","path":"/tmp/wrong"},"next_task_id":1,"next_deletion_sequence":1,"tasks":[]}"#.as_slice(),
    ];

    for document in documents {
        let home = tempfile::tempdir().unwrap();
        let path = snapshot_path(home.path(), "global");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, document).unwrap();
        let before = std::fs::read(&path).unwrap();

        let output = run_with_home(home.path(), &["list"]);

        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn list_should_ignore_invalid_interactive_config() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        run_with_home(home.path(), &["add", "repair config later"])
            .status
            .success()
    );
    write_config(home.path(), "[keybindings.normal]\nmove_down = [\"dn\"]\n");

    let output = run_with_home(home.path(), &["list"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1  open  repair config later\n"
    );
}

#[test]
fn delete_should_tombstone_one_global_task_after_persisting() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        run_with_home(home.path(), &["add", "Fix the bug"])
            .status
            .success()
    );
    assert!(
        run_with_home(home.path(), &["add", "Keep this task"])
            .status
            .success()
    );

    let output = run_with_home(home.path(), &["delete", "1"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Deleted 1: Fix the bug\n"
    );
    assert!(output.stderr.is_empty());
    let snapshot = read_snapshot(&snapshot_path(home.path(), "global"));
    assert_eq!(snapshot["tasks"][0]["deletion_sequence"], Value::from(1));
    assert_eq!(snapshot["next_deletion_sequence"], Value::from(2));
    assert_eq!(snapshot["tasks"][1]["text"], "Keep this task");
    assert_eq!(snapshot["tasks"][1]["deletion_sequence"], Value::Null);
}

#[test]
fn repeated_delete_should_be_an_idempotent_no_write_success() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        run_with_home(home.path(), &["add", "Fix the bug"])
            .status
            .success()
    );
    assert!(
        run_with_home(home.path(), &["delete", "1"])
            .status
            .success()
    );
    let path = snapshot_path(home.path(), "global");
    let before = std::fs::read(&path).unwrap();

    let output = run_with_home(home.path(), &["delete", "1"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Already deleted 1: Fix the bug\n"
    );
    assert!(output.stderr.is_empty());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn delete_unknown_id_should_fail_without_writing() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        run_with_home(home.path(), &["add", "existing"])
            .status
            .success()
    );
    let path = snapshot_path(home.path(), "global");
    let before = std::fs::read(&path).unwrap();

    let output = run_with_home(home.path(), &["delete", "3"]);

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("task 3 was not found"));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn invalid_delete_ids_should_fail_before_creating_storage() {
    for arguments in [
        &["delete"] as &[&str],
        &["delete", "0"],
        &["delete", "-1"],
        &["delete", "three"],
        &["delete", "1", "extra"],
    ] {
        let home = tempfile::tempdir().unwrap();

        let output = run_with_home(home.path(), arguments);

        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Usage:"));
        assert!(!home.path().join(".shtodo").exists());
    }
}

#[test]
fn local_delete_should_change_only_the_exact_directory_scope() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    assert!(
        run_with_home(home.path(), &["add", "global task"])
            .status
            .success()
    );
    assert!(
        run_with_home_at(
            home.path(),
            project.path(),
            &["--local", "add", "local task"]
        )
        .status
        .success()
    );
    let global_path = snapshot_path(home.path(), "global");
    let global_before = std::fs::read(&global_path).unwrap();

    let output = run_with_home_at(home.path(), project.path(), &["--local", "delete", "1"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Deleted 1: local task\n"
    );
    assert_eq!(std::fs::read(global_path).unwrap(), global_before);
    assert_eq!(
        String::from_utf8_lossy(&run_with_home_at(home.path(), project.path(), &["list"]).stdout),
        "1  open  global task\n"
    );
    assert!(
        run_with_home_at(home.path(), project.path(), &["--local", "list"])
            .stdout
            .is_empty()
    );
}

#[test]
fn delete_should_ignore_invalid_interactive_config() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        run_with_home(home.path(), &["add", "repair config later"])
            .status
            .success()
    );
    write_config(home.path(), "[keybindings.normal]\nmove_down = [\"dn\"]\n");

    let output = run_with_home(home.path(), &["delete", "1"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Deleted 1: repair config later\n"
    );
}

fn write_config(home: &std::path::Path, source: &str) {
    let directory = home.join(".shtodo");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("config.toml"), source).unwrap();
}

fn run_with_home(home: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .args(arguments)
        .env("HOME", home)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn run_with_home_at(
    home: &std::path::Path,
    current_dir: &std::path::Path,
    arguments: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .args(arguments)
        .env("HOME", home)
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn snapshot_path(home: &std::path::Path, scope: &str) -> std::path::PathBuf {
    home.join(".shtodo").join(scope).join("tasks.json")
}

fn read_snapshot(path: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn write_snapshot(path: &std::path::Path, snapshot: &Value) {
    let mut bytes = serde_json::to_vec_pretty(snapshot).unwrap();
    bytes.push(b'\n');
    std::fs::write(path, bytes).unwrap();
}

fn stored_task_text(home: &std::path::Path, scope: &str) -> String {
    stored_task_text_at(&home.join(".shtodo").join(scope).join("tasks.json"))
}

fn stored_task_text_at(path: &std::path::Path) -> String {
    let snapshot: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    snapshot["tasks"][0]["text"].as_str().unwrap().to_owned()
}
