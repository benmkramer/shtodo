use std::{
    io::Write as _,
    path::Path,
    process::{Command, Output, Stdio},
};

use serde_json::{Value, json};

fn run(home: &Path, directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .args(args)
        .env("HOME", home)
        .current_dir(directory)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn success(output: &Output) -> Value {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(output.stdout.last(), Some(&b'\n'));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn failure(output: &Output, code: &str) -> Value {
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["error"]["code"], code);
    assert!(!value["error"]["message"].as_str().unwrap().is_empty());
    value
}

fn task(id: u64, text: &str, state: &str, deleted: bool) -> Value {
    json!({"id": id, "text": text, "state": state, "deleted": deleted})
}

#[test]
fn json_missing_lists_should_return_empty_arrays_without_creating_storage() {
    let home = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir().unwrap();
    for (args, scope) in [
        (vec!["list", "--json"], json!({"kind": "global"})),
        (
            vec!["--json", "--local", "list"],
            json!({"kind": "project", "path": directory.path().canonicalize().unwrap()}),
        ),
    ] {
        assert_eq!(
            success(&run(home.path(), directory.path(), &args)),
            json!({"schema_version": 1, "command": "list", "scope": scope, "tasks": []})
        );
        assert!(!home.path().join(".shtodo").exists());
    }
}

#[test]
fn json_list_should_preserve_order_ids_states_and_literal_text_without_tombstones() {
    let home = tempfile::tempdir().unwrap();
    let text = "Quote \"this\" \\ 東京\t  spaced";
    for text in [text, "deleted", "done"] {
        success(&run(home.path(), home.path(), &["add", "--json", text]));
    }
    success(&run(home.path(), home.path(), &["delete", "2", "--json"]));
    success(&run(home.path(), home.path(), &["done", "--json", "3"]));
    let path = home.path().join(".shtodo/global/tasks.json");
    let mut snapshot: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    snapshot["tasks"].as_array_mut().unwrap().reverse();
    std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let before = std::fs::read(&path).unwrap();

    assert_eq!(
        success(&run(home.path(), home.path(), &["list", "--json"])),
        json!({
            "schema_version": 1, "command": "list", "scope": {"kind": "global"},
            "tasks": [task(3, "done", "done", false), task(1, text, "open", false)]
        })
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn json_mutations_should_report_the_saved_task_and_explicit_change_status() {
    let home = tempfile::tempdir().unwrap();
    for (args, command, expected) in [
        (
            vec!["add", "  original  "],
            "add",
            task(1, "original", "open", false),
        ),
        (
            vec!["done", "1"],
            "done",
            task(1, "original", "done", false),
        ),
        (
            vec!["edit", "1", "  updated  "],
            "edit",
            task(1, "updated", "done", false),
        ),
        (
            vec!["delete", "1"],
            "delete",
            task(1, "updated", "done", true),
        ),
        (
            vec!["restore", "1"],
            "restore",
            task(1, "updated", "done", false),
        ),
        (
            vec!["reopen", "1"],
            "reopen",
            task(1, "updated", "open", false),
        ),
    ] {
        let mut arguments = vec!["--json"];
        arguments.extend(args);
        assert_eq!(
            success(&run(home.path(), home.path(), &arguments)),
            json!({
                "schema_version": 1, "command": command, "scope": {"kind": "global"},
                "task": expected, "changed": true
            })
        );
    }
    let value = success(&run(home.path(), home.path(), &["list", "--json"]));
    assert_eq!(value["tasks"], json!([task(1, "updated", "open", false)]));
}

#[test]
fn json_noop_mutations_should_report_unchanged_without_attempting_to_save() {
    for (setup, args) in [
        (Some("done"), vec!["done", "1"]),
        (None, vec!["reopen", "1"]),
        (None, vec!["edit", "1", "  original  "]),
        (None, vec!["restore", "1"]),
        (Some("delete"), vec!["delete", "1"]),
    ] {
        let home = tempfile::tempdir().unwrap();
        success(&run(
            home.path(),
            home.path(),
            &["--json", "add", "original"],
        ));
        if let Some(command) = setup {
            success(&run(home.path(), home.path(), &["--json", command, "1"]));
        }
        let path = home.path().join(".shtodo/global/tasks.json");
        let before = std::fs::read(&path).unwrap();
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let mut arguments = vec!["--json"];
        arguments.extend(args);

        let value = success(&run(home.path(), home.path(), &arguments));

        assert_eq!(value["changed"], false);
        assert_eq!(value["task"]["id"], 1);
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
}

#[test]
fn json_should_keep_global_parent_and_child_directory_scopes_distinct() {
    let home = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let child = parent.path().join("child");
    std::fs::create_dir(&child).unwrap();
    success(&run(
        home.path(),
        parent.path(),
        &["--json", "add", "global"],
    ));
    for (directory, text) in [(parent.path(), "parent"), (child.as_path(), "child")] {
        let value = success(&run(
            home.path(),
            directory,
            &["--local", "--json", "add", text],
        ));
        assert_eq!(value["task"]["id"], 1);
        assert_eq!(
            value["scope"],
            json!({"kind": "project", "path": directory.canonicalize().unwrap()})
        );
    }
    success(&run(
        home.path(),
        &child,
        &["--local", "done", "1", "--json"],
    ));
    for (directory, args, expected) in [
        (
            parent.path(),
            vec!["--json", "list"],
            task(1, "global", "open", false),
        ),
        (
            parent.path(),
            vec!["--local", "list", "--json"],
            task(1, "parent", "open", false),
        ),
        (
            child.as_path(),
            vec!["--json", "--local", "list"],
            task(1, "child", "done", false),
        ),
    ] {
        assert_eq!(
            success(&run(home.path(), directory, &args))["tasks"],
            json!([expected])
        );
    }
}

#[test]
fn json_add_should_support_single_task_stdin_in_both_scopes() {
    let home = tempfile::tempdir().unwrap();
    for scope in [vec![], vec!["--local"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_shtodo"))
            .args(scope)
            .args(["add", "--json"])
            .env("HOME", home.path())
            .current_dir(home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"  --json  \n")
            .unwrap();
        let value = success(&child.wait_with_output().unwrap());
        assert_eq!(value["task"], task(1, "--json", "open", false));
        assert_eq!(value["changed"], true);
    }
}

#[test]
fn json_usage_errors_should_be_structured_and_create_no_storage() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["--json"],
        vec!["--json", "--local"],
        vec!["--json", "doctor"],
        vec!["doctor", "--json"],
        vec!["--json", "--help"],
        vec!["--help", "--json"],
        vec!["--json", "--version"],
        vec!["--json", "unknown"],
        vec!["unknown", "--json"],
        vec!["--json", "delete"],
        vec!["--json", "done", "0"],
        vec!["edit", "--json", "1"],
        vec!["--json", "list", "--json"],
        vec!["--json", "add", "--print-id", "task"],
        vec!["add", "--print-id", "--json", "task"],
        vec!["add", "--print-id", "task", "--json"],
        vec!["list", "--json", "extra"],
        vec!["delete", "1", "2", "--json"],
    ] {
        failure(&run(home.path(), home.path(), &args), "invalid_arguments");
        assert!(!home.path().join(".shtodo").exists(), "{args:?}");
    }
}

#[test]
fn json_task_errors_should_distinguish_missing_deleted_and_invalid_text() {
    let home = tempfile::tempdir().unwrap();
    let value = failure(
        &run(home.path(), home.path(), &["--json", "done", "42"]),
        "task_not_found",
    );
    assert_eq!(value["error"]["task_id"], 42);
    for args in [vec!["add"], vec!["add", "  "], vec!["add", "first\nsecond"]] {
        let mut arguments = vec!["--json"];
        arguments.extend(args);
        failure(
            &run(home.path(), home.path(), &arguments),
            "invalid_task_text",
        );
    }
    success(&run(
        home.path(),
        home.path(),
        &["--json", "add", "original"],
    ));
    success(&run(home.path(), home.path(), &["--json", "delete", "1"]));
    for args in [
        vec!["done", "1"],
        vec!["reopen", "1"],
        vec!["edit", "1", "updated"],
    ] {
        let mut arguments = vec!["--json"];
        arguments.extend(args);
        let value = failure(&run(home.path(), home.path(), &arguments), "task_deleted");
        assert_eq!(value["error"]["task_id"], 1);
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("restore 1")
        );
    }
}

#[test]
fn json_should_report_failed_saves_without_success_or_snapshot_changes() {
    let home = tempfile::tempdir().unwrap();
    success(&run(
        home.path(),
        home.path(),
        &["--json", "add", "original"],
    ));
    let path = home.path().join(".shtodo/global/tasks.json");
    let before = std::fs::read(&path).unwrap();
    std::fs::create_dir(path.with_extension("json.tmp")).unwrap();
    for args in [
        vec!["--json", "add", "new"],
        vec!["--json", "done", "1"],
        vec!["--json", "edit", "1", "new"],
        vec!["--json", "delete", "1"],
    ] {
        failure(&run(home.path(), home.path(), &args), "command_failed");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn json_should_report_busy_writers_while_listing_the_last_complete_snapshot() {
    let home = tempfile::tempdir().unwrap();
    success(&run(
        home.path(),
        home.path(),
        &["--json", "add", "original"],
    ));
    let path = home.path().join(".shtodo/global/tasks.json");
    let before = std::fs::read(&path).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .unwrap();
    lock.lock().unwrap();

    failure(
        &run(home.path(), home.path(), &["--json", "done", "1"]),
        "busy",
    );
    let value = success(&run(home.path(), home.path(), &["list", "--json"]));
    assert_eq!(value["tasks"], json!([task(1, "original", "open", false)]));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn json_should_reject_invalid_snapshots_and_bypass_interactive_config() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".shtodo")).unwrap();
    std::fs::write(home.path().join(".shtodo/config.toml"), "invalid config").unwrap();
    success(&run(home.path(), home.path(), &["--json", "add", "task"]));
    success(&run(home.path(), home.path(), &["list", "--json"]));
    let path = home.path().join(".shtodo/global/tasks.json");
    std::fs::write(&path, "{invalid}").unwrap();
    for args in [vec!["list", "--json"], vec!["--json", "done", "1"]] {
        failure(&run(home.path(), home.path(), &args), "command_failed");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{invalid}");
    }
}

#[test]
fn json_edit_should_preserve_flag_like_task_text_as_a_positional_argument() {
    let home = tempfile::tempdir().unwrap();
    success(&run(
        home.path(),
        home.path(),
        &["--json", "add", "original"],
    ));
    let value = success(&run(
        home.path(),
        home.path(),
        &["--json", "edit", "1", "--json"],
    ));
    assert_eq!(value["task"]["text"], "--json");
    let output = run(home.path(), home.path(), &["edit", "1", "--json"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"Unchanged 1: --json\n");
}

#[cfg(unix)]
#[test]
fn json_should_report_non_utf8_arguments_without_unstructured_diagnostics() {
    use std::os::unix::ffi::OsStringExt as _;

    let home = tempfile::tempdir().unwrap();
    for (command, code) in [("add", "invalid_task_text"), ("done", "invalid_arguments")] {
        let output = Command::new(env!("CARGO_BIN_EXE_shtodo"))
            .args(["--json", command])
            .arg(std::ffi::OsString::from_vec(vec![0xff]))
            .env("HOME", home.path())
            .output()
            .unwrap();
        failure(&output, code);
        assert!(!home.path().join(".shtodo").exists());
    }
}
