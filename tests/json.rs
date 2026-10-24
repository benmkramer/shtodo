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
    tagged(id, text, state, deleted, None, &[])
}

fn tagged(
    id: u64,
    text: &str,
    state: &str,
    deleted: bool,
    priority: Option<&str>,
    tags: &[&str],
) -> Value {
    dated(id, text, state, deleted, priority, tags, None)
}

fn dated(
    id: u64,
    text: &str,
    state: &str,
    deleted: bool,
    priority: Option<&str>,
    tags: &[&str],
    due: Option<&str>,
) -> Value {
    json!({
        "id": id, "text": text, "state": state, "deleted": deleted,
        "priority": priority, "tags": tags, "due": due, "parent_id": null
    })
}

fn ids(value: &Value) -> Vec<u64> {
    value["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_u64().unwrap())
        .collect()
}

/// Seven tasks: 3 is done, 6 is deleted, and 7 has a lowercase (non-)priority.
fn seed_tagged_tasks(home: &Path) {
    for text in [
        "(B) Write docs +shtodo",
        "Plain errand @home",
        "(A) Fix the bug +shtodo @laptop.",
        "(A) Ship release +shtodo",
        "(C) Call bob @home",
        "(A) deleted +shtodo",
        "(b) lowercase +shtodo",
    ] {
        success(&run(home, home, &["--json", "add", text]));
    }
    success(&run(home, home, &["--json", "done", "3"]));
    success(&run(home, home, &["--json", "delete", "6"]));
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

#[test]
fn json_list_filters_should_return_the_text_mode_rows_with_derived_fields() {
    let home = tempfile::tempdir().unwrap();
    seed_tagged_tasks(home.path());

    assert_eq!(
        success(&run(
            home.path(),
            home.path(),
            &["list", "--json", "--tag", "+shtodo", "--open"]
        )),
        json!({
            "schema_version": 1, "command": "list", "scope": {"kind": "global"},
            "tasks": [
                tagged(1, "(B) Write docs +shtodo", "open", false, Some("B"), &["+shtodo"]),
                tagged(4, "(A) Ship release +shtodo", "open", false, Some("A"), &["+shtodo"]),
                tagged(7, "(b) lowercase +shtodo", "open", false, None, &["+shtodo"]),
            ]
        })
    );
    let all = success(&run(home.path(), home.path(), &["--json", "list"]));
    assert_eq!(
        all["tasks"][2],
        tagged(
            3,
            "(A) Fix the bug +shtodo @laptop.",
            "done",
            false,
            Some("A"),
            &["+shtodo", "@laptop"]
        )
    );
    for filters in [
        vec!["--done"],
        vec!["--tag", "@laptop."],
        vec!["--priority", "a-b", "--priority", "none"],
        vec!["--search", "SHIP"],
        vec![
            "--priority",
            "C",
            "--tag",
            "@home",
            "--open",
            "--search",
            "bob",
        ],
        vec!["--tag", "+missing"],
    ] {
        let mut text_args = vec!["list"];
        text_args.extend(&filters);
        let text = run(home.path(), home.path(), &text_args);
        let text_ids: Vec<u64> = String::from_utf8_lossy(&text.stdout)
            .lines()
            .map(|line| line.split("  ").next().unwrap().parse().unwrap())
            .collect();
        let mut json_args = vec!["--json"];
        json_args.extend(&text_args);
        let value = success(&run(home.path(), home.path(), &json_args));

        assert_eq!(ids(&value), text_ids, "{filters:?}");
        for task in value["tasks"].as_array().unwrap() {
            assert_eq!(
                all["tasks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["id"] == task["id"]),
                Some(task)
            );
        }
    }
}

#[test]
fn json_list_sort_priority_should_reorder_the_same_task_objects() {
    let home = tempfile::tempdir().unwrap();
    seed_tagged_tasks(home.path());

    for (filters, expected) in [
        (vec![], vec![3, 4, 1, 5, 2, 7]),
        (vec!["--open", "--tag", "+shtodo"], vec![4, 1, 7]),
        (vec!["--priority", "none", "--priority", "C"], vec![5, 2, 7]),
        (vec!["--search", "CA"], vec![5, 7]),
    ] {
        let mut unsorted_args = vec!["--json", "list"];
        unsorted_args.extend(&filters);
        let unsorted = success(&run(home.path(), home.path(), &unsorted_args));
        let mut sorted_args = unsorted_args.clone();
        sorted_args.extend(["--sort", "priority"]);
        let sorted = success(&run(home.path(), home.path(), &sorted_args));
        let text_args = sorted_args[1..].to_vec();
        let text = run(home.path(), home.path(), &text_args);
        let text_ids: Vec<u64> = String::from_utf8_lossy(&text.stdout)
            .lines()
            .map(|line| line.split("  ").next().unwrap().parse().unwrap())
            .collect();

        assert_eq!(ids(&sorted), expected, "{filters:?}");
        assert_eq!(text_ids, expected, "{filters:?}");
        let mut reordered: Vec<Value> = unsorted["tasks"].as_array().unwrap().clone();
        reordered.sort_by_key(|task| expected.iter().position(|id| task["id"] == *id).unwrap());
        assert_eq!(sorted["tasks"], Value::Array(reordered), "{filters:?}");
        assert_eq!(sorted["command"], "list");
    }
}

#[test]
fn json_list_filter_errors_should_be_invalid_arguments_without_storage() {
    let home = tempfile::tempdir().unwrap();
    for (args, message) in [
        (
            vec!["--json", "list", "extra"],
            "unsupported shtodo arguments",
        ),
        (
            vec!["list", "--wat", "--json"],
            "unsupported shtodo arguments",
        ),
        (
            vec!["--json", "list", "--tag=+x"],
            "unsupported shtodo arguments",
        ),
        (vec!["--json", "list", "--tag"], "--tag requires a value"),
        (
            vec!["--json", "list", "--priority"],
            "--priority requires a value",
        ),
        (
            vec!["--json", "list", "--search"],
            "--search requires a value",
        ),
        (
            vec!["--json", "list", "--tag", "work"],
            "expected +project or @context",
        ),
        (
            vec!["list", "--json", "--tag", "+."],
            "expected +project or @context",
        ),
        (
            vec!["--json", "list", "--priority", "C-A"],
            "expected a letter A-Z",
        ),
        (
            vec!["--json", "list", "--search", ""],
            "--search requires non-empty single-line text",
        ),
        (
            vec!["--json", "list", "--open", "--done"],
            "--open and --done cannot be combined",
        ),
        (
            vec!["--json", "list", "--open", "--open"],
            "--open may only be specified once",
        ),
        (
            vec!["--json", "list", "--search", "a", "--search", "a"],
            "--search may only be specified once",
        ),
        (
            vec!["--json", "list", "--open", "--json"],
            "--json may only be specified once",
        ),
        (
            vec!["--json", "list", "--sort"],
            "--sort requires a value; expected priority or due",
        ),
        (
            vec!["--json", "list", "--sort", "manual"],
            r#"invalid --sort \"manual\"; expected priority or due"#,
        ),
        (
            vec!["--json", "list", "--sort", "x"],
            "expected priority or due",
        ),
        (
            vec!["--json", "list", "--sort", ""],
            "expected priority or due",
        ),
        (
            vec!["--json", "list", "--sort", "--json"],
            r#"invalid --sort \"--json\"; expected priority or due"#,
        ),
        (
            vec!["list", "--json", "--sort", "priority", "--sort", "priority"],
            "--sort may only be specified once",
        ),
        (
            vec!["--json", "list", "--done", "--done"],
            "--done may only be specified once",
        ),
        (
            vec!["--json", "list", "--tag", "+"],
            r#"invalid --tag \"+\"; expected +project or @context"#,
        ),
        (
            vec!["--json", "list", "--tag", "+a b"],
            r#"invalid --tag \"+a b\"; expected +project or @context"#,
        ),
        (
            vec!["--json", "list", "--priority", "AA"],
            r#"invalid --priority \"AA\"; expected a letter A-Z, a range such as A-C, or none"#,
        ),
        (
            vec!["--json", "list", "--priority", ""],
            r#"invalid --priority \"\"; expected a letter A-Z"#,
        ),
        (
            vec!["--json", "list", "--search", " \t "],
            "--search requires non-empty single-line text",
        ),
        (
            vec!["--json", "list", "--search", "a\nb"],
            "--search requires non-empty single-line text",
        ),
        (
            vec!["--json", "list", "--search", "a\rb"],
            "--search requires non-empty single-line text",
        ),
        (vec!["--json", "list", "--due"], "--due requires a value"),
        (
            vec!["--json", "list", "--due", "soon"],
            r#"invalid --due \"soon\"; expected today, tomorrow, overdue, none, any, YYYY-MM-DD, or START..END"#,
        ),
        (
            vec!["--json", "list", "--due", "week"],
            r#"invalid --due \"week\""#,
        ),
        (vec!["--json", "list", "--due", ""], r#"invalid --due \"\""#),
        (
            vec!["--json", "list", "--due", ".."],
            r#"invalid --due \"..\""#,
        ),
        (
            vec!["--json", "list", "--due", "2026-02-30"],
            r#"invalid --due \"2026-02-30\""#,
        ),
        (
            vec!["--json", "list", "--due", "2026-1-5"],
            r#"invalid --due \"2026-1-5\""#,
        ),
        (
            vec!["--json", "list", "--due", "due:today"],
            r#"invalid --due \"due:today\""#,
        ),
        (
            vec!["list", "--json", "--due", "--json"],
            r#"invalid --due \"--json\""#,
        ),
        (
            vec!["--json", "list", "--due", "2026-10-22..2026-10-21"],
            r#"invalid --due \"2026-10-22..2026-10-21\""#,
        ),
        (
            vec!["--json", "list", "--due", "tomorrow..today"],
            r#"invalid --due \"tomorrow..today\""#,
        ),
        (
            vec!["--json", "list", "--sort", "Due"],
            r#"invalid --sort \"Due\"; expected priority or due"#,
        ),
        (
            vec!["--json", "list", "--sort", "priority", "--sort", "due"],
            "--sort may only be specified once",
        ),
    ] {
        let output = run(home.path(), home.path(), &args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        let value = failure(&output, "invalid_arguments");
        let error = serde_json::to_string(&value["error"]["message"]).unwrap();
        assert!(error.contains(message), "{args:?}: {error}");
        assert!(!home.path().join(".shtodo").exists(), "{args:?}");
    }
}

#[cfg(unix)]
#[test]
fn json_list_should_reject_non_utf8_filter_values_without_storage() {
    use std::os::unix::ffi::OsStringExt as _;

    let home = tempfile::tempdir().unwrap();
    for flag in ["--tag", "--priority", "--search", "--sort", "--due"] {
        let output = Command::new(env!("CARGO_BIN_EXE_shtodo"))
            .args(["--json", "list", flag])
            .arg(std::ffi::OsString::from_vec(vec![b'+', 0xff]))
            .env("HOME", home.path())
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{flag}");
        let value = failure(&output, "invalid_arguments");
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .starts_with(&format!("invalid {flag} value; expected UTF-8")),
            "{value}"
        );
        assert!(!home.path().join(".shtodo").exists(), "{flag}");
    }
}

#[test]
fn json_filtered_lists_should_be_empty_for_missing_scopes_and_readable_under_a_writer_lock() {
    let home = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["--json", "list", "--tag", "+x", "--sort", "priority"],
        vec!["--json", "--local", "list", "--open", "--priority", "A"],
    ] {
        let value = success(&run(home.path(), directory.path(), &args));
        assert_eq!(value["tasks"], json!([]), "{args:?}");
        assert!(!home.path().join(".shtodo").exists(), "{args:?}");
    }

    seed_tagged_tasks(home.path());
    let path = home.path().join(".shtodo/global/tasks.json");
    let before = std::fs::read(&path).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .unwrap();
    lock.lock().unwrap();

    let value = success(&run(
        home.path(),
        home.path(),
        &["--json", "list", "--priority", "A", "--sort", "priority"],
    ));
    assert_eq!(ids(&value), [3, 4]);
    let value = success(&run(
        home.path(),
        home.path(),
        &["--json", "list", "--due", "none", "--sort", "due"],
    ));
    assert_eq!(ids(&value), [3, 4, 1, 5, 2, 7]);
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn json_mutation_replies_should_carry_derived_priority_and_tags() {
    let home = tempfile::tempdir().unwrap();
    let text = "(A) Fix +shtodo @laptop +shtodo!";
    let tags = ["+shtodo", "@laptop"];
    for (args, expected) in [
        (
            vec!["add", text],
            tagged(1, text, "open", false, Some("A"), &tags),
        ),
        (
            vec!["done", "1"],
            tagged(1, text, "done", false, Some("A"), &tags),
        ),
        (
            vec!["reopen", "1"],
            tagged(1, text, "open", false, Some("A"), &tags),
        ),
        (
            vec!["delete", "1"],
            tagged(1, text, "open", true, Some("A"), &tags),
        ),
        (
            vec!["restore", "1"],
            tagged(1, text, "open", false, Some("A"), &tags),
        ),
        (
            vec!["edit", "1", "(B) moved @desk"],
            tagged(1, "(B) moved @desk", "open", false, Some("B"), &["@desk"]),
        ),
        (vec!["edit", "1", "plain"], task(1, "plain", "open", false)),
    ] {
        let mut arguments = vec!["--json"];
        arguments.extend(&args);
        let value = success(&run(home.path(), home.path(), &arguments));
        assert_eq!(value["task"], expected, "{args:?}");
    }
}

#[test]
fn json_replies_should_carry_the_derived_due_date() {
    let home = tempfile::tempdir().unwrap();
    let text = "(A) Pay rent due:2026-11-01. +home";
    let with_due = |state, deleted| {
        dated(
            1,
            text,
            state,
            deleted,
            Some("A"),
            &["+home"],
            Some("2026-11-01"),
        )
    };
    for (args, expected) in [
        (vec!["add", text], with_due("open", false)),
        (vec!["done", "1"], with_due("done", false)),
        (vec!["reopen", "1"], with_due("open", false)),
        (vec!["delete", "1"], with_due("open", true)),
        (vec!["restore", "1"], with_due("open", false)),
        (
            vec!["edit", "1", "x due:2026-02-30 due:2026-12-01"],
            dated(
                1,
                "x due:2026-02-30 due:2026-12-01",
                "open",
                false,
                None,
                &[],
                Some("2026-12-01"),
            ),
        ),
        (
            vec!["edit", "1", "x Due:2026-11-01 overdue:2026-11-01"],
            task(1, "x Due:2026-11-01 overdue:2026-11-01", "open", false),
        ),
    ] {
        let mut arguments = vec!["--json"];
        arguments.extend(&args);
        let value = success(&run(home.path(), home.path(), &arguments));
        assert_eq!(value["task"], expected, "{args:?}");
    }
}

/// Five tasks with absolute dates, so no command here reads the clock: 2 is done, 4 is deleted.
fn seed_dated_tasks(home: &Path) {
    for text in [
        "(B) Pay rent due:2026-10-20 +home",
        "(A) File taxes due:2026-10-01",
        "Plain errand",
        "deleted due:2026-10-21",
        "(A) Call bank due:2026-10-20",
    ] {
        success(&run(home, home, &["--json", "add", text]));
    }
    success(&run(home, home, &["--json", "done", "2"]));
    success(&run(home, home, &["--json", "delete", "4"]));
}

#[test]
fn json_list_due_filters_and_sort_should_return_the_text_mode_rows_and_same_objects() {
    let home = tempfile::tempdir().unwrap();
    seed_dated_tasks(home.path());
    let all = success(&run(home.path(), home.path(), &["--json", "list"]));
    assert_eq!(
        all["tasks"],
        json!([
            dated(
                1,
                "(B) Pay rent due:2026-10-20 +home",
                "open",
                false,
                Some("B"),
                &["+home"],
                Some("2026-10-20")
            ),
            dated(
                2,
                "(A) File taxes due:2026-10-01",
                "done",
                false,
                Some("A"),
                &[],
                Some("2026-10-01")
            ),
            task(3, "Plain errand", "open", false),
            dated(
                5,
                "(A) Call bank due:2026-10-20",
                "open",
                false,
                Some("A"),
                &[],
                Some("2026-10-20")
            ),
        ])
    );
    for (filters, expected) in [
        (vec!["--due", "2026-10-20"], vec![1, 5]),
        (vec!["--due", "any"], vec![1, 2, 5]),
        (vec!["--due", "none"], vec![3]),
        (vec!["--due", "..2026-10-19", "--done"], vec![2]),
        (vec!["--due", "2026-10-02..", "--tag", "+home"], vec![1]),
        (vec!["--sort", "due"], vec![2, 5, 1, 3]),
        (vec!["--sort", "due", "--open"], vec![5, 1, 3]),
        (
            vec!["--due", "any", "--sort", "due", "--priority", "A"],
            vec![2, 5],
        ),
    ] {
        let mut text_args = vec!["list"];
        text_args.extend(&filters);
        let text = run(home.path(), home.path(), &text_args);
        let text_ids: Vec<u64> = String::from_utf8_lossy(&text.stdout)
            .lines()
            .map(|line| line.split("  ").next().unwrap().parse().unwrap())
            .collect();
        let mut json_args = vec!["--json"];
        json_args.extend(&text_args);
        let value = success(&run(home.path(), home.path(), &json_args));

        assert_eq!(ids(&value), expected, "{filters:?}");
        assert_eq!(text_ids, expected, "{filters:?}");
        for task in value["tasks"].as_array().unwrap() {
            assert_eq!(
                all["tasks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["id"] == task["id"]),
                Some(task),
                "{filters:?}"
            );
        }
    }
}

#[test]
fn json_add_and_edit_should_show_the_rewritten_text_and_unchanged_status() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".shtodo/global/tasks.json");
    for _ in 0..2 {
        let day = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date();
        let utc = |args: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_shtodo"))
                .args(args)
                .env("HOME", home.path())
                .env("TZ", "UTC0")
                .stdin(Stdio::null())
                .output()
                .unwrap()
        };
        let added = success(&utc(&["--json", "add", "x due:today"]));
        let before = std::fs::read(&path).unwrap();
        let unchanged = success(&utc(&["--json", "edit", "1", "x due:Today"]));
        let after = std::fs::read(&path).unwrap();
        if jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date()
            != day
        {
            std::fs::remove_dir_all(home.path().join(".shtodo")).unwrap();
            continue;
        }
        let text = format!("x due:{day}");
        let expected = dated(1, &text, "open", false, None, &[], Some(&day.to_string()));
        assert_eq!(added["task"], expected);
        assert_eq!(added["changed"], true);
        assert_eq!(unchanged["task"], expected);
        assert_eq!(unchanged["changed"], false);
        assert_eq!(after, before);
        return;
    }
    panic!("the UTC date changed twice while the test ran");
}

fn subtask(id: u64, text: &str, state: &str, deleted: bool, parent: u64) -> Value {
    let mut value = task(id, text, state, deleted);
    value["parent_id"] = json!(parent);
    value
}

/// 1 [3 done, 4], 2, and 5 [6], with priorities for sorting.
fn seed_subtask_tree(home: &Path) {
    for args in [
        vec!["add", "(B) one"],
        vec!["add", "(A) two"],
        vec!["add", "--parent", "1", "(C) three"],
        vec!["add", "--parent", "1", "(A) four"],
        vec!["add", "(C) five"],
        vec!["add", "--parent", "5", "six"],
        vec!["done", "3"],
    ] {
        success(&run(home, home, &[&["--json"], args.as_slice()].concat()));
    }
}

fn text_ids(home: &Path, args: &[&str]) -> Vec<u64> {
    let output = run(home, home, args);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.split_whitespace().next().unwrap().parse().unwrap())
        .collect()
}

#[test]
fn json_should_carry_parent_id_on_lists_and_every_mutation_reply() {
    let home = tempfile::tempdir().unwrap();
    let reply = |args: &[&str]| success(&run(home.path(), home.path(), args));
    let added = reply(&["--json", "add", "parent"]);
    assert_eq!(added["task"], task(1, "parent", "open", false));
    let child = reply(&["add", "--parent", "1", "child", "--json"]);
    assert_eq!(child["command"], "add");
    assert_eq!(child["task"], subtask(2, "child", "open", false, 1));
    assert_eq!(
        reply(&["--json", "list"])["tasks"],
        json!([
            task(1, "parent", "open", false),
            subtask(2, "child", "open", false, 1)
        ])
    );
    for (args, command, expected, changed) in [
        (
            vec!["done", "2"],
            "done",
            subtask(2, "child", "done", false, 1),
            true,
        ),
        (
            vec!["reopen", "2"],
            "reopen",
            subtask(2, "child", "open", false, 1),
            true,
        ),
        (
            vec!["edit", "2", "kid"],
            "edit",
            subtask(2, "kid", "open", false, 1),
            true,
        ),
        (
            vec!["parent", "2", "none"],
            "parent",
            task(2, "kid", "open", false),
            true,
        ),
        (
            vec!["parent", "2", "none"],
            "parent",
            task(2, "kid", "open", false),
            false,
        ),
        (
            vec!["parent", "2", "1"],
            "parent",
            subtask(2, "kid", "open", false, 1),
            true,
        ),
        (
            vec!["parent", "2", "1"],
            "parent",
            subtask(2, "kid", "open", false, 1),
            false,
        ),
        (
            vec!["delete", "1"],
            "delete",
            task(1, "parent", "open", true),
            true,
        ),
        (
            vec!["delete", "2"],
            "delete",
            subtask(2, "kid", "open", true, 1),
            false,
        ),
        (
            vec!["restore", "1"],
            "restore",
            task(1, "parent", "open", false),
            true,
        ),
        (
            vec!["delete", "2"],
            "delete",
            subtask(2, "kid", "open", true, 1),
            true,
        ),
        (
            vec!["restore", "2"],
            "restore",
            subtask(2, "kid", "open", false, 1),
            true,
        ),
    ] {
        let mut arguments = vec!["--json"];
        arguments.extend(&args);
        let value = reply(&arguments);
        assert_eq!(value["command"], command, "{args:?}");
        assert_eq!(value["task"], expected, "{args:?}");
        assert_eq!(value["changed"], changed, "{args:?}");
    }
}

#[test]
fn json_lists_should_keep_tree_order_and_return_the_text_mode_ids() {
    let home = tempfile::tempdir().unwrap();
    seed_subtask_tree(home.path());
    for (args, expected) in [
        (vec!["list"], vec![1, 3, 4, 2, 5, 6]),
        (vec!["list", "--sort", "priority"], vec![2, 1, 4, 3, 5, 6]),
        (vec!["list", "--sort", "due"], vec![2, 1, 4, 3, 5, 6]),
        (vec!["list", "--open"], vec![1, 4, 2, 5, 6]),
        (vec!["list", "--roots"], vec![1, 2, 5]),
        (vec!["list", "--parent", "1"], vec![3, 4]),
        (
            vec!["list", "--parent", "1", "--sort", "priority"],
            vec![4, 3],
        ),
        // The parents of 4 and 6 are filtered out, so each sorts as its own group.
        (
            vec!["list", "--priority", "A", "--priority", "none"],
            vec![4, 2, 6],
        ),
        (
            vec![
                "list",
                "--priority",
                "A",
                "--priority",
                "none",
                "--sort",
                "priority",
            ],
            vec![4, 2, 6],
        ),
        (
            vec!["list", "--search", "f", "--sort", "priority"],
            vec![4, 5],
        ),
    ] {
        let value = success(&run(
            home.path(),
            home.path(),
            &[&["--json"], args.as_slice()].concat(),
        ));
        assert_eq!(ids(&value), expected, "{args:?}");
        assert_eq!(text_ids(home.path(), &args), expected, "{args:?}");
        // Every returned parent comes first, and its returned subtasks follow it as one run.
        let tasks = value["tasks"].as_array().unwrap();
        for (position, task) in tasks.iter().enumerate() {
            if let Some(parent) = task["parent_id"].as_u64()
                && let Some(parent_position) = tasks.iter().position(|row| row["id"] == parent)
            {
                assert!(parent_position < position, "{args:?}");
                assert!(
                    tasks[parent_position + 1..position]
                        .iter()
                        .all(|row| row["parent_id"] == parent),
                    "{args:?}"
                );
            }
        }
    }
}

#[test]
fn json_subtask_errors_should_return_their_codes_and_task_ids() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["--json", "add", "--parent"],
        vec!["--json", "list", "--parent"],
        vec!["add", "--parent", "x", "--json"],
        vec!["--json", "list", "--parent", "0"],
        vec!["--json", "parent", "5"],
        vec!["--json", "parent", "x", "3"],
        vec!["--json", "parent", "5", "x"],
        vec!["--json", "parent", "5", "3", "extra"],
        vec!["--json", "list", "--roots", "--parent", "3"],
        vec!["--json", "list", "--roots", "--roots"],
        vec!["add", "--parent", "3", "--print-id", "--json"],
        vec!["add", "--print-id", "--parent", "3", "--json"],
    ] {
        failure(&run(home.path(), home.path(), &args), "invalid_arguments");
        assert!(!home.path().join(".shtodo").exists(), "{args:?}");
    }
    seed_subtask_tree(home.path());
    success(&run(home.path(), home.path(), &["--json", "delete", "2"]));
    let path = home.path().join(".shtodo/global/tasks.json");
    let before = std::fs::read(&path).unwrap();
    for (args, code, task_id) in [
        (vec!["add", "--parent", "9", "x"], "task_not_found", 9),
        (vec!["parent", "9", "1"], "task_not_found", 9),
        (vec!["parent", "4", "9"], "task_not_found", 9),
        (vec!["parent", "2", "1"], "task_deleted", 2),
        (vec!["add", "--parent", "2", "x"], "task_deleted", 2),
        (vec!["parent", "4", "2"], "task_deleted", 2),
        (vec!["parent", "4", "4"], "invalid_parent", 4),
        (vec!["parent", "5", "4"], "invalid_parent", 4),
        (vec!["add", "--parent", "4", "x"], "invalid_parent", 4),
        (vec!["parent", "1", "5"], "invalid_parent", 1),
    ] {
        let mut arguments = vec!["--json"];
        arguments.extend(&args);
        let value = failure(&run(home.path(), home.path(), &arguments), code);
        assert_eq!(value["error"]["task_id"], task_id, "{args:?}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "{args:?}");
        if code == "task_deleted" {
            assert!(
                value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("restore {task_id}")),
                "{value}"
            );
        }
    }
}

#[test]
fn json_restore_of_a_subtask_should_name_its_deleted_parent() {
    let home = tempfile::tempdir().unwrap();
    seed_subtask_tree(home.path());
    success(&run(home.path(), home.path(), &["--json", "delete", "5"]));
    let path = home.path().join(".shtodo/global/tasks.json");
    let before = std::fs::read(&path).unwrap();
    let value = failure(
        &run(home.path(), home.path(), &["--json", "restore", "6"]),
        "task_deleted",
    );
    assert_eq!(value["error"]["task_id"], 5);
    let message = value["error"]["message"].as_str().unwrap();
    assert!(message.contains("parent task 5 is deleted"), "{message}");
    assert!(message.contains("restore 5"), "{message}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let restored = success(&run(home.path(), home.path(), &["--json", "restore", "5"]));
    assert_eq!(
        restored["task"],
        tagged(5, "(C) five", "open", false, Some("C"), &[])
    );
    let listed = success(&run(
        home.path(),
        home.path(),
        &["--json", "list", "--parent", "5"],
    ));
    assert_eq!(
        listed["tasks"],
        json!([subtask(6, "six", "open", false, 5)])
    );
}

#[test]
fn json_lists_with_subtasks_should_be_readable_under_a_writer_lock() {
    let home = tempfile::tempdir().unwrap();
    seed_subtask_tree(home.path());
    let path = home.path().join(".shtodo/global/tasks.json");
    let before = std::fs::read(&path).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .unwrap();
    lock.lock().unwrap();
    let value = success(&run(
        home.path(),
        home.path(),
        &["--json", "list", "--sort", "priority"],
    ));
    assert_eq!(ids(&value), [2, 1, 4, 3, 5, 6]);
    assert_eq!(std::fs::read(path).unwrap(), before);
}
