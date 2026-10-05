use super::*;
use crate::{mutation::ConflictKind, task::TaskId};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
};

fn fixture() -> (tempfile::TempDir, Store) {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path(), ListScope::Global).unwrap();
    store
        .mutate(
            &Mutation::Add("base".into()),
            ScopePresence::AllowMissing,
            SHELL_LOCK_BUDGET,
        )
        .unwrap();
    (home, store)
}

fn id(value: u64) -> TaskId {
    TaskId::from_shell_integer(value).unwrap()
}

#[test]
fn replies_should_keep_their_model_and_bytes_paired_across_later_commits() {
    let (_home, store) = fixture();
    let add = store
        .mutate(
            &Mutation::Add("second".into()),
            ScopePresence::RequireExisting,
            SHELL_LOCK_BUDGET,
        )
        .unwrap();
    store
        .mutate(
            &Mutation::Add("third".into()),
            ScopePresence::RequireExisting,
            SHELL_LOCK_BUDGET,
        )
        .unwrap();
    let newer = store
        .refresh(&add.snapshot.token, ScopePresence::RequireExisting)
        .unwrap()
        .unwrap();
    assert_eq!(add.snapshot.list.visible_tasks().count(), 2);
    assert_eq!(newer.list.visible_tasks().count(), 3);
    for request in [
        Mutation::SetCompleted {
            id: id(1),
            completed: false,
        },
        Mutation::EditText {
            id: id(1),
            text: "mine".into(),
            original: Some("stale".into()),
        },
    ] {
        let reply = store
            .mutate(&request, ScopePresence::RequireExisting, SHELL_LOCK_BUDGET)
            .unwrap();
        if let SnapshotToken::Present(bytes) = &reply.snapshot.token {
            assert_eq!(
                parse_snapshot(bytes, &store.paths.data_file, &ListScope::Global).unwrap(),
                reply.snapshot.list
            );
        } else {
            panic!("expected a present snapshot");
        }
        store
            .mutate(
                &Mutation::Add("later".into()),
                ScopePresence::RequireExisting,
                SHELL_LOCK_BUDGET,
            )
            .unwrap();
        assert!(
            store
                .refresh(&reply.snapshot.token, ScopePresence::RequireExisting)
                .unwrap()
                .is_some()
        );
    }
}

#[test]
fn successful_no_ops_and_conflicts_should_preserve_bytes_and_file_identity() {
    let (_home, store) = fixture();
    let before = fs::read(&store.paths.data_file).unwrap();
    let metadata = fs::metadata(&store.paths.data_file).unwrap();
    let requests = [
        Mutation::SetCompleted {
            id: id(1),
            completed: false,
        },
        Mutation::EditText {
            id: id(1),
            text: " base ".into(),
            original: Some("stale".into()),
        },
        Mutation::EditText {
            id: id(1),
            text: "new".into(),
            original: Some("stale".into()),
        },
        Mutation::RestoreLatest,
        Mutation::MoveAdjacent {
            id: id(1),
            direction: crate::task::MoveDirection::Down,
            neighbor: None,
            projection: crate::projection::Projection::default(),
        },
    ];
    for request in requests {
        assert!(
            !store
                .mutate(&request, ScopePresence::RequireExisting, SHELL_LOCK_BUDGET)
                .unwrap()
                .outcome
                .changed()
        );
        assert_eq!(fs::read(&store.paths.data_file).unwrap(), before);
        assert_eq!(
            fs::metadata(&store.paths.data_file)
                .unwrap()
                .modified()
                .unwrap(),
            metadata.modified().unwrap()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                fs::metadata(&store.paths.data_file).unwrap().ino(),
                metadata.ino()
            );
        }
        assert!(!store.paths.temp_file.exists());
    }
}

#[test]
fn invalid_input_and_missing_scope_no_op_should_not_create_canonical_data() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path(), ListScope::Global).unwrap();
    assert!(
        store
            .mutate(
                &Mutation::Add("one\ntwo".into()),
                ScopePresence::AllowMissing,
                SHELL_LOCK_BUDGET
            )
            .is_err()
    );
    assert!(!home.path().join(".shtodo").exists());
    assert_eq!(
        store
            .mutate(
                &Mutation::RestoreLatest,
                ScopePresence::AllowMissing,
                SHELL_LOCK_BUDGET
            )
            .unwrap()
            .outcome,
        Outcome::NothingToRestore
    );
    assert!(!store.paths.data_file.exists());
    assert!(!store.paths.temp_file.exists());
}

#[test]
fn changed_bytes_with_equal_length_and_timestamp_should_still_refresh() {
    let (_home, store) = fixture();
    let snapshot = store.read_snapshot(ScopePresence::RequireExisting).unwrap();
    let timestamp = fs::metadata(&store.paths.data_file)
        .unwrap()
        .modified()
        .unwrap();
    store
        .mutate(
            &Mutation::EditText {
                id: id(1),
                text: "same".into(),
                original: None,
            },
            ScopePresence::RequireExisting,
            SHELL_LOCK_BUDGET,
        )
        .unwrap();
    File::options()
        .write(true)
        .open(&store.paths.data_file)
        .unwrap()
        .set_modified(timestamp)
        .unwrap();
    let refreshed = store
        .refresh(&snapshot.token, ScopePresence::RequireExisting)
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.list.task(id(1)).unwrap().text(), "same");
    let SnapshotToken::Present(old) = snapshot.token else {
        panic!("missing old snapshot")
    };
    let SnapshotToken::Present(new) = refreshed.token else {
        panic!("missing new snapshot")
    };
    assert_eq!(old.len(), new.len());
}

#[test]
fn open_reader_should_finish_the_old_complete_snapshot_after_replacement() {
    let (_home, store) = fixture();
    let mut reader = File::open(&store.paths.data_file).unwrap();
    store
        .mutate(
            &Mutation::Add("second".into()),
            ScopePresence::RequireExisting,
            SHELL_LOCK_BUDGET,
        )
        .unwrap();
    let mut old = Vec::new();
    reader.read_to_end(&mut old).unwrap();
    assert_eq!(
        parse_snapshot(&old, &store.paths.data_file, &ListScope::Global)
            .unwrap()
            .visible_tasks()
            .count(),
        1
    );
    assert_eq!(store.load().unwrap().visible_tasks().count(), 2);
}

#[test]
fn each_pre_replace_failure_should_preserve_canonical_and_release_the_lock() {
    let (_home, store) = fixture();
    let before = fs::read(&store.paths.data_file).unwrap();
    for failure in [SaveStep::Write, SaveStep::TempSync, SaveStep::Replace] {
        let result = store.mutate_with_hook(
            &Mutation::Add("failed".into()),
            ScopePresence::RequireExisting,
            SHELL_LOCK_BUDGET,
            |step| {
                if step == failure {
                    Err(eyre!("injected {step:?}"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(matches!(result, Err(TransactionError::Failed(_))));
        assert_eq!(fs::read(&store.paths.data_file).unwrap(), before);
        let _guard = store
            .acquire(ScopePresence::RequireExisting, Duration::ZERO)
            .unwrap();
    }
}

#[test]
fn uncertainty_should_carry_visible_id_and_sync_only_recovery_should_preserve_later_writes() {
    let (_home, store) = fixture();
    let result = store.mutate_unsynced(&Mutation::Add("visible".into()));
    let Err(TransactionError::ReplacedButNotSynced { reply, .. }) = result else {
        panic!("expected uncertainty")
    };
    assert_eq!(
        reply.outcome,
        Outcome::Changed {
            id: id(2),
            celebrate: false
        }
    );
    assert_eq!(store.load().unwrap(), reply.snapshot.list);
    store
        .mutate(
            &Mutation::EditText {
                id: id(2),
                text: "later edit".into(),
                original: None,
            },
            ScopePresence::RequireExisting,
            SHELL_LOCK_BUDGET,
        )
        .unwrap();
    let before = fs::read(&store.paths.data_file).unwrap();
    assert_eq!(
        store
            .recover_sync(SHELL_LOCK_BUDGET)
            .unwrap()
            .list
            .task(id(2))
            .unwrap()
            .text(),
        "later edit"
    );
    assert_eq!(fs::read(&store.paths.data_file).unwrap(), before);
    assert!(!store.paths.temp_file.exists());
}

#[test]
fn corruption_and_counter_exhaustion_should_not_touch_temp_or_canonical() {
    let (_home, store) = fixture();
    let valid: serde_json::Value =
        serde_json::from_slice(&fs::read(&store.paths.data_file).unwrap()).unwrap();
    for (field, request) in [
        ("next_task_id", Mutation::Add("overflow".into())),
        (
            "next_deletion_sequence",
            Mutation::Delete {
                id: id(1),
                observed: None,
            },
        ),
    ] {
        let mut value = valid.clone();
        value[field] = u64::MAX.into();
        fs::write(&store.paths.data_file, serde_json::to_vec(&value).unwrap()).unwrap();
        let before = fs::read(&store.paths.data_file).unwrap();
        let reply = store
            .mutate(&request, ScopePresence::RequireExisting, SHELL_LOCK_BUDGET)
            .unwrap();
        assert!(matches!(reply.outcome, Outcome::Rejected(_)));
        assert_eq!(fs::read(&store.paths.data_file).unwrap(), before);
        assert!(!store.paths.temp_file.exists());
    }
    fs::write(&store.paths.data_file, b"{").unwrap();
    fs::write(&store.paths.temp_file, b"unused").unwrap();
    assert!(
        store
            .mutate(
                &Mutation::Add("bad".into()),
                ScopePresence::RequireExisting,
                SHELL_LOCK_BUDGET
            )
            .is_err()
    );
    assert_eq!(fs::read(&store.paths.data_file).unwrap(), b"{");
    assert_eq!(fs::read(&store.paths.temp_file).unwrap(), b"unused");
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn child(home: &Path, control: &Path, mode: &str, label: &str) -> ChildGuard {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "storage::concurrency_tests::process_helper",
            "--nocapture",
        ])
        .env("SHTODO_TEST_HOME", home)
        .env("SHTODO_TEST_CONTROL", control)
        .env("SHTODO_TEST_MODE", mode)
        .env("SHTODO_TEST_LABEL", label)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    ChildGuard(child)
}

fn finish(child: &mut ChildGuard) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            let mut stderr = String::new();
            child
                .0
                .stderr
                .as_mut()
                .unwrap()
                .read_to_string(&mut stderr)
                .unwrap();
            assert!(status.success(), "child failed: {stderr}");
            return;
        }
        assert!(Instant::now() < deadline, "child did not finish");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "child helper, invoked with an isolated home and rendezvous paths"]
fn process_helper() {
    let home = PathBuf::from(env::var_os("SHTODO_TEST_HOME").unwrap());
    let control = PathBuf::from(env::var_os("SHTODO_TEST_CONTROL").unwrap());
    let mode = env::var("SHTODO_TEST_MODE").unwrap();
    let label = env::var("SHTODO_TEST_LABEL").unwrap();
    let store = Store::open(&home, ListScope::Global).unwrap();
    let seen = store.read_snapshot(ScopePresence::AllowMissing).unwrap();
    if mode == "hold" {
        let _guard = store
            .acquire(ScopePresence::AllowMissing, SHELL_LOCK_BUDGET)
            .unwrap();
        let _loaded = store.read_snapshot(ScopePresence::AllowMissing).unwrap();
        fs::write(control.join(format!("{label}.ready")), b"ready").unwrap();
        wait_file(&control.join("go"));
        return;
    }
    if mode == "before_replace" || mode == "after_replace" {
        let boundary = if mode == "before_replace" {
            SaveStep::Replace
        } else {
            SaveStep::DirectorySync
        };
        store
            .mutate_with_hook(
                &Mutation::Add(label.clone()),
                ScopePresence::AllowMissing,
                SHELL_LOCK_BUDGET,
                |step| {
                    if step == boundary {
                        fs::write(control.join(format!("{label}.ready")), b"ready")?;
                        wait_file(&control.join("go"));
                    }
                    Ok(())
                },
            )
            .unwrap();
        return;
    }
    fs::write(control.join(format!("{label}.ready")), b"ready").unwrap();
    if mode == "read" {
        let mut reads = 0;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !control.join("go").exists() {
            assert!(
                Instant::now() < deadline,
                "reader did not receive completion"
            );
            store
                .read_snapshot(ScopePresence::RequireExisting)
                .unwrap()
                .list
                .validate()
                .unwrap();
            reads += 1;
        }
        fs::write(control.join("read-count"), reads.to_string()).unwrap();
        return;
    }
    wait_file(&control.join("go"));
    let request = match mode.as_str() {
        "edit" => Mutation::EditText {
            id: id(1),
            text: label,
            original: Some(seen.list.task(id(1)).unwrap().text().into()),
        },
        "complete" => Mutation::SetCompleted {
            id: id(1),
            completed: true,
        },
        "reorder" => Mutation::MoveAdjacent {
            id: id(1),
            direction: crate::task::MoveDirection::Down,
            neighbor: seen
                .list
                .adjacent_visible(id(1), crate::task::MoveDirection::Down),
            projection: crate::projection::Projection::default(),
        },
        "delete" => Mutation::Delete {
            id: id(label.parse().unwrap()),
            observed: None,
        },
        "restore" => Mutation::RestoreLatest,
        _ => Mutation::Add(label),
    };
    let reply = store
        .mutate(&request, ScopePresence::AllowMissing, SHELL_LOCK_BUDGET)
        .unwrap();
    if mode == "edit" {
        assert!(
            reply.outcome.changed()
                || matches!(
                    reply.outcome,
                    Outcome::Conflict {
                        kind: ConflictKind::TextChanged,
                        ..
                    }
                )
        );
    }
    assert!(!matches!(reply.outcome, Outcome::Rejected(_)));
}

#[test]
fn process_completion_reorder_and_lifecycle_races_should_follow_serialized_outcomes() {
    let (home, store) = fixture();
    store
        .mutate(
            &Mutation::Add("second".into()),
            ScopePresence::RequireExisting,
            SHELL_LOCK_BUDGET,
        )
        .unwrap();
    for mode in ["complete", "reorder", "delete", "restore"] {
        let control = tempfile::tempdir().unwrap();
        let labels = if mode == "delete" {
            ["1", "2"]
        } else {
            ["one", "two"]
        };
        let mut first = child(home.path(), control.path(), mode, labels[0]);
        let mut second = child(home.path(), control.path(), mode, labels[1]);
        for label in labels {
            wait_file(&control.path().join(format!("{label}.ready")));
        }
        fs::write(control.path().join("go"), b"go").unwrap();
        finish(&mut first);
        finish(&mut second);
        let list = store.load().unwrap();
        list.validate().unwrap();
        match mode {
            "complete" => {
                assert!(list.task(id(1)).unwrap().completed());
                assert!(!list.task(id(2)).unwrap().completed());
            }
            "reorder" => assert_eq!(
                list.visible_tasks()
                    .map(|task| task.id())
                    .collect::<Vec<_>>(),
                [id(2), id(1)]
            ),
            "delete" => {
                assert_eq!(list.visible_tasks().count(), 0);
                let value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&store.paths.data_file).unwrap()).unwrap();
                assert_eq!(value["next_deletion_sequence"], 3);
                let mut sequences = value["tasks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|task| task["deletion_sequence"].as_u64().unwrap())
                    .collect::<Vec<_>>();
                sequences.sort_unstable();
                assert_eq!(sequences, [1, 2]);
            }
            "restore" => {
                assert_eq!(list.visible_tasks().count(), 2);
                assert!(list.task(id(1)).unwrap().completed());
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn stale_process_clients_should_preserve_both_additions_and_reject_conflicting_edits() {
    let (home, store) = fixture();
    for mode in ["add", "edit"] {
        let control = tempfile::tempdir().unwrap();
        let mut first = child(home.path(), control.path(), mode, "one");
        let mut second = child(home.path(), control.path(), mode, "two");
        wait_file(&control.path().join("one.ready"));
        wait_file(&control.path().join("two.ready"));
        fs::write(control.path().join("go"), b"go").unwrap();
        finish(&mut first);
        finish(&mut second);
        let list = store.load().unwrap();
        list.validate().unwrap();
        if mode == "add" {
            assert_eq!(list.visible_tasks().count(), 3);
        } else {
            assert!(matches!(list.task(id(1)).unwrap().text(), "one" | "two"));
        }
    }
}

#[test]
fn held_process_lock_should_bound_wait_and_leave_an_independent_scope_available() {
    let (home, store) = fixture();
    let control = tempfile::tempdir().unwrap();
    let mut holder = child(home.path(), control.path(), "hold", "holder");
    wait_file(&control.path().join("holder.ready"));
    let before = fs::read(&store.paths.data_file).unwrap();
    let start = Instant::now();
    assert!(matches!(
        store.mutate(
            &Mutation::Add("busy".into()),
            ScopePresence::RequireExisting,
            INTERACTIVE_LOCK_BUDGET
        ),
        Err(TransactionError::Busy(_))
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(fs::read(&store.paths.data_file).unwrap(), before);
    assert!(!store.paths.temp_file.exists());
    assert_eq!(store.load().unwrap().visible_tasks().count(), 1);
    let local = Store::open(
        home.path(),
        ListScope::Project {
            path: fs::canonicalize(control.path())
                .unwrap()
                .to_str()
                .unwrap()
                .into(),
        },
    )
    .unwrap();
    local
        .mutate(
            &Mutation::Add("independent".into()),
            ScopePresence::AllowMissing,
            INTERACTIVE_LOCK_BUDGET,
        )
        .unwrap();
    fs::write(control.path().join("go"), b"go").unwrap();
    finish(&mut holder);
    store
        .mutate(
            &Mutation::Add("after release".into()),
            ScopePresence::RequireExisting,
            INTERACTIVE_LOCK_BUDGET,
        )
        .unwrap();
}

#[test]
fn killed_writers_should_release_lock_and_leave_a_complete_old_or_new_snapshot() {
    for (mode, expected) in [("before_replace", 1), ("after_replace", 2)] {
        let (home, store) = fixture();
        let control = tempfile::tempdir().unwrap();
        let mut writer = child(home.path(), control.path(), mode, "paused");
        wait_file(&control.path().join("paused.ready"));
        writer.0.kill().unwrap();
        writer.0.wait().unwrap();
        let list = store.load().unwrap();
        list.validate().unwrap();
        assert_eq!(list.visible_tasks().count(), expected);
        store
            .mutate(
                &Mutation::Add("after crash".into()),
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        assert_eq!(store.load().unwrap().visible_tasks().count(), expected + 1);
    }
}

#[test]
fn process_reader_should_observe_complete_snapshots_during_replacements() {
    let (home, store) = fixture();
    let control = tempfile::tempdir().unwrap();
    let mut reader = child(home.path(), control.path(), "read", "reader");
    wait_file(&control.path().join("reader.ready"));
    for index in 0..30 {
        store
            .mutate(
                &Mutation::Add(format!("task {index}")),
                ScopePresence::RequireExisting,
                SHELL_LOCK_BUDGET,
            )
            .unwrap();
    }
    fs::write(control.path().join("go"), b"go").unwrap();
    finish(&mut reader);
    let reads: usize = fs::read_to_string(control.path().join("read-count"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(reads > 0);
    assert_eq!(store.load().unwrap().visible_tasks().count(), 31);
}
