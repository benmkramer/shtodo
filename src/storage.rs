use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions, TryLockError},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Result, WrapErr, ensure, eyre};
use serde::Deserialize;

use crate::{
    cli::ScopeChoice,
    task::{ListScope, SCHEMA_VERSION, TaskList},
};

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
const MAX_PROJECT_SLUG_BYTES: usize = 48;

#[derive(Debug)]
pub(crate) struct StoragePaths {
    pub(crate) directory: PathBuf,
    pub(crate) data_file: PathBuf,
    pub(crate) temp_file: PathBuf,
    pub(crate) lock_file: PathBuf,
}

#[derive(Debug)]
pub(crate) struct Store {
    paths: StoragePaths,
    scope: ListScope,
    _lock: File,
}

impl Store {
    pub(crate) fn open(home: &Path, scope: ListScope) -> Result<Self> {
        let paths = paths_for_home(home, &scope)?;
        fs::create_dir_all(&paths.directory).wrap_err_with(|| {
            format!(
                "could not create storage directory {}",
                paths.directory.display()
            )
        })?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&paths.lock_file)
            .wrap_err_with(|| format!("could not open lock file {}", paths.lock_file.display()))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(eyre!(
                    "another shtodo process is already using this list ({})",
                    paths.lock_file.display()
                ));
            }
            Err(TryLockError::Error(error)) => {
                return Err(error)
                    .wrap_err_with(|| format!("could not lock {}", paths.lock_file.display()));
            }
        }

        Ok(Self {
            paths,
            scope,
            _lock: lock,
        })
    }

    pub(crate) fn scope(&self) -> &ListScope {
        &self.scope
    }

    pub(crate) fn paths(&self) -> &StoragePaths {
        &self.paths
    }

    pub(crate) fn load(&self) -> Result<TaskList> {
        load_snapshot(&self.paths.data_file, &self.scope)
    }

    pub(crate) fn save(&self, list: &TaskList) -> Result<()> {
        list.validate()?;
        ensure_scope_matches(list.scope(), &self.scope)?;

        let mut bytes = serde_json::to_vec_pretty(list).wrap_err_with(|| {
            format!(
                "could not serialize task list for {}",
                self.paths.data_file.display()
            )
        })?;
        bytes.push(b'\n');
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.paths.temp_file)
            .wrap_err_with(|| {
                format!(
                    "could not open temporary snapshot {}",
                    self.paths.temp_file.display()
                )
            })?;
        file.write_all(&bytes).wrap_err_with(|| {
            format!(
                "could not write temporary snapshot {}",
                self.paths.temp_file.display()
            )
        })?;
        file.sync_all().wrap_err_with(|| {
            format!(
                "could not sync temporary snapshot {}",
                self.paths.temp_file.display()
            )
        })?;
        fs::rename(&self.paths.temp_file, &self.paths.data_file).wrap_err_with(|| {
            format!(
                "could not rename temporary snapshot {} to {}",
                self.paths.temp_file.display(),
                self.paths.data_file.display()
            )
        })?;

        #[cfg(unix)]
        File::open(&self.paths.directory)
            .wrap_err_with(|| {
                format!(
                    "could not open storage directory {} for syncing",
                    self.paths.directory.display()
                )
            })?
            .sync_all()
            .wrap_err_with(|| {
                format!(
                    "could not sync storage directory {}",
                    self.paths.directory.display()
                )
            })?;

        Ok(())
    }
}

pub(crate) fn resolve_home(
    home: Option<OsString>,
    userprofile: Option<OsString>,
) -> Result<PathBuf> {
    home.filter(|value| !value.is_empty())
        .or_else(|| userprofile.filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .ok_or_else(|| eyre!("could not determine home directory from HOME or USERPROFILE"))
}

pub(crate) fn home_from_environment() -> Result<PathBuf> {
    resolve_home(env::var_os("HOME"), env::var_os("USERPROFILE"))
}

pub(crate) fn resolve_scope(choice: ScopeChoice, cwd: Option<&Path>) -> Result<ListScope> {
    match choice {
        ScopeChoice::Global => Ok(ListScope::Global),
        ScopeChoice::Local => {
            let cwd = cwd.ok_or_else(|| eyre!("current directory is unavailable"))?;
            let canonical = fs::canonicalize(cwd)
                .wrap_err_with(|| format!("could not canonicalize {}", cwd.display()))?;
            let path = canonical_path_string(&canonical)?;
            Ok(ListScope::Project { path })
        }
    }
}

fn canonical_path_string(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        eyre!(
            "canonical project path is not valid UTF-8: {}",
            path.display()
        )
    })
}

pub(crate) fn scope_from_environment(choice: ScopeChoice) -> Result<ListScope> {
    match choice {
        ScopeChoice::Global => resolve_scope(choice, None),
        ScopeChoice::Local => {
            let cwd = env::current_dir().wrap_err("could not read current directory")?;
            resolve_scope(choice, Some(&cwd))
        }
    }
}

pub(crate) fn project_folder_name(path: &str) -> String {
    let component = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mut slug = String::new();
    let mut replacing_run = false;
    for character in component.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            slug.push(character);
            replacing_run = false;
        } else if !replacing_run {
            slug.push('-');
            replacing_run = true;
        }
    }

    let mut slug = slug.trim_matches('-').to_owned();
    slug.truncate(MAX_PROJECT_SLUG_BYTES);
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("project");
    }

    format!("{slug}-{:016x}", fnv1a(path.as_bytes()))
}

pub(crate) fn paths_for_home(home: &Path, scope: &ListScope) -> Result<StoragePaths> {
    ensure!(!home.as_os_str().is_empty(), "home directory path is empty");
    let root = home.join(".shtodo");
    let directory = match scope {
        ListScope::Global => root.join("global"),
        ListScope::Project { path } => {
            ensure!(
                !path.is_empty() && Path::new(path).is_absolute(),
                "project scope path must be non-empty and absolute"
            );
            root.join("projects").join(project_folder_name(path))
        }
    };

    Ok(StoragePaths {
        data_file: directory.join("tasks.json"),
        temp_file: directory.join("tasks.json.tmp"),
        lock_file: directory.join("tasks.lock"),
        directory,
    })
}

pub(crate) fn load_snapshot(path: &Path, expected_scope: &ListScope) -> Result<TaskList> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(TaskList::new(expected_scope.clone()));
        }
        Err(error) => {
            return Err(error).wrap_err_with(|| format!("could not read {}", path.display()));
        }
    };

    #[derive(Deserialize)]
    struct VersionProbe {
        schema_version: u64,
    }

    let probe: VersionProbe = serde_json::from_slice(&bytes)
        .wrap_err_with(|| format!("could not read schema version from {}", path.display()))?;
    if probe.schema_version != SCHEMA_VERSION {
        return Err(eyre!(
            "unsupported schema version {} in {}",
            probe.schema_version,
            path.display()
        ));
    }

    let list: TaskList = serde_json::from_slice(&bytes)
        .wrap_err_with(|| format!("could not parse {}", path.display()))?;
    list.validate()?;
    ensure_scope_matches(list.scope(), expected_scope)?;
    Ok(list)
}

fn ensure_scope_matches(actual: &ListScope, expected: &ListScope) -> Result<()> {
    ensure!(
        actual == expected,
        "snapshot scope {actual:?} does not match expected scope {expected:?}"
    );
    Ok(())
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::Path};

    use super::{
        Store, canonical_path_string, load_snapshot, paths_for_home, project_folder_name,
        resolve_home, resolve_scope,
    };
    use crate::{
        cli::ScopeChoice,
        task::{ListScope, TaskList},
    };

    #[test]
    fn home_should_prefer_nonempty_home_then_userprofile() {
        assert_eq!(
            resolve_home(
                Some(OsString::from("/home/first")),
                Some(OsString::from("/home/second")),
            )
            .unwrap(),
            Path::new("/home/first")
        );
        assert_eq!(
            resolve_home(Some(OsString::new()), Some(OsString::from("/home/second"))).unwrap(),
            Path::new("/home/second")
        );
        assert!(resolve_home(None, Some(OsString::new())).is_err());
    }

    #[test]
    fn scope_should_ignore_cwd_for_global_and_canonicalize_exact_local_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let child = temp.path().join("child");
        std::fs::create_dir(&child).unwrap();

        assert_eq!(
            resolve_scope(ScopeChoice::Global, Some(Path::new("/does/not/exist"))).unwrap(),
            ListScope::Global
        );
        assert_eq!(
            resolve_scope(ScopeChoice::Local, Some(&child.join(".."))).unwrap(),
            ListScope::Project {
                path: std::fs::canonicalize(temp.path())
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into(),
            }
        );
        assert!(resolve_scope(ScopeChoice::Local, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn local_scope_should_reject_a_non_utf8_canonical_path() {
        use std::os::unix::ffi::OsStringExt;

        let path = std::path::PathBuf::from(OsString::from_vec(vec![b'p', 0xff]));

        assert!(canonical_path_string(&path).is_err());
    }

    #[test]
    fn project_folder_should_be_readable_and_deterministic() {
        assert_eq!(
            project_folder_name("/Users/ben/code/shtodo"),
            "shtodo-2200fde3358e9316"
        );
        assert_eq!(project_folder_name("/tmp/###"), "project-cc418e70bb33a8cb");
        assert_eq!(
            project_folder_name("/tmp/a b__c...d"),
            "a-b__c...d-c1c4faba9523fe86"
        );
        assert_eq!(
            project_folder_name(
                "/tmp/abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
            ),
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUV-ed1a94cf860765a3"
        );
    }

    #[test]
    fn paths_should_match_the_selected_scope_layout() {
        let global = paths_for_home(Path::new("/home/ben"), &ListScope::Global).unwrap();
        assert_eq!(global.directory, Path::new("/home/ben/.shtodo/global"));
        assert_eq!(global.data_file, global.directory.join("tasks.json"));
        assert_eq!(global.temp_file, global.directory.join("tasks.json.tmp"));
        assert_eq!(global.lock_file, global.directory.join("tasks.lock"));

        #[cfg(unix)]
        {
            let scope = ListScope::Project {
                path: "/Users/ben/code/shtodo".into(),
            };
            let project = paths_for_home(Path::new("/home/ben"), &scope).unwrap();
            assert_eq!(
                project.directory,
                Path::new("/home/ben/.shtodo/projects/shtodo-2200fde3358e9316")
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn paths_should_accept_canonical_windows_project_scope() {
        let scope = ListScope::Project {
            path: r"\\?\C:\work\shtodo".into(),
        };

        let project = paths_for_home(Path::new(r"C:\Users\ben"), &scope).unwrap();

        assert_eq!(
            project.directory,
            Path::new(r"C:\Users\ben\.shtodo\projects\shtodo-d5ee64f14ef8d32b")
        );
    }

    #[test]
    fn missing_snapshot_should_load_empty_requested_scope() {
        let temp = tempfile::tempdir().unwrap();
        let scope = ListScope::Global;
        let paths = paths_for_home(temp.path(), &scope).unwrap();

        let list = load_snapshot(&paths.data_file, &scope).unwrap();

        assert_eq!(list.scope(), &scope);
        assert_eq!(list.visible_tasks().count(), 0);
    }

    #[test]
    fn load_should_reject_future_version_without_replacing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tasks.json");
        std::fs::write(&path, br#"{"schema_version":2}"#).unwrap();

        let error = load_snapshot(&path, &ListScope::Global).unwrap_err();

        assert!(error.to_string().contains("schema version 2"));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            r#"{"schema_version":2}"#
        );
    }

    #[test]
    fn load_should_reject_malformed_unknown_invalid_and_wrong_scope_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tasks.json");
        let invalid_documents = [
            "{",
            r#"{
              "schema_version": 1,
              "scope": {"kind": "global"},
              "next_task_id": 1,
              "next_deletion_sequence": 1,
              "tasks": [],
              "extra": true
            }"#,
            r#"{
              "schema_version": 1,
              "scope": {"kind": "global"},
              "next_task_id": 2,
              "next_deletion_sequence": 1,
              "tasks": [
                {"id": 1, "text": "one", "completed": false, "deletion_sequence": null},
                {"id": 1, "text": "two", "completed": false, "deletion_sequence": null}
              ]
            }"#,
        ];
        for document in invalid_documents {
            std::fs::write(&path, document).unwrap();
            assert!(load_snapshot(&path, &ListScope::Global).is_err());
        }

        std::fs::write(
            &path,
            r#"{
              "schema_version": 1,
              "scope": {"kind": "global"},
              "next_task_id": 1,
              "next_deletion_sequence": 1,
              "tasks": []
            }"#,
        )
        .unwrap();
        let project = ListScope::Project {
            path: "/tmp/project".into(),
        };
        assert!(load_snapshot(&path, &project).is_err());
    }

    #[test]
    fn load_should_invoke_task_list_invariant_checks() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tasks.json");
        let invalid_documents = [
            r#"{"schema_version":1,"scope":{"kind":"global"},"next_task_id":1,"next_deletion_sequence":1,"tasks":[{"id":0,"text":"one","completed":false,"deletion_sequence":null}]}"#,
            r#"{"schema_version":1,"scope":{"kind":"global"},"next_task_id":1,"next_deletion_sequence":1,"tasks":[{"id":1,"text":"one","completed":false,"deletion_sequence":null}]}"#,
            r#"{"schema_version":1,"scope":{"kind":"global"},"next_task_id":2,"next_deletion_sequence":2,"tasks":[{"id":1,"text":"one","completed":false,"deletion_sequence":1},{"id":2,"text":"two","completed":false,"deletion_sequence":1}]}"#,
            r#"{"schema_version":1,"scope":{"kind":"global"},"next_task_id":2,"next_deletion_sequence":1,"tasks":[{"id":1,"text":"one","completed":false,"deletion_sequence":1}]}"#,
            r#"{"schema_version":1,"scope":{"kind":"global"},"next_task_id":2,"next_deletion_sequence":1,"tasks":[{"id":1,"text":" ","completed":false,"deletion_sequence":null}]}"#,
            r#"{"schema_version":1,"scope":{"kind":"global"},"next_task_id":2,"next_deletion_sequence":1,"tasks":[{"id":1,"text":" one ","completed":false,"deletion_sequence":null}]}"#,
            "{\"schema_version\":1,\"scope\":{\"kind\":\"global\"},\"next_task_id\":2,\"next_deletion_sequence\":1,\"tasks\":[{\"id\":1,\"text\":\"one\\ntwo\",\"completed\":false,\"deletion_sequence\":null}]}",
        ];

        for document in invalid_documents {
            std::fs::write(&path, document).unwrap();
            assert!(load_snapshot(&path, &ListScope::Global).is_err());
        }
    }

    #[test]
    fn non_not_found_read_error_should_include_the_path() {
        let temp = tempfile::tempdir().unwrap();

        let error = load_snapshot(temp.path(), &ListScope::Global).unwrap_err();

        assert!(
            error
                .to_string()
                .contains(&temp.path().display().to_string())
        );
    }

    #[test]
    fn second_store_should_not_lock_same_scope() {
        let temp = tempfile::tempdir().unwrap();
        let first = Store::open(temp.path(), ListScope::Global).unwrap();

        let error = Store::open(temp.path(), ListScope::Global).unwrap_err();

        assert!(error.to_string().contains("already using"));
        drop(first);
        assert!(Store::open(temp.path(), ListScope::Global).is_ok());
    }

    #[test]
    fn stores_should_lock_different_scopes_independently() {
        let temp = tempfile::tempdir().unwrap();
        let global = Store::open(temp.path(), ListScope::Global).unwrap();
        let project_scope = ListScope::Project {
            path: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        };
        let project = Store::open(temp.path(), project_scope).unwrap();

        assert_ne!(global.paths().lock_file, project.paths().lock_file);
    }

    #[test]
    fn save_should_write_pretty_json_newline_and_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), ListScope::Global).unwrap();
        let mut list = TaskList::new(ListScope::Global);
        list.add("persist me").unwrap();

        store.save(&list).unwrap();

        let bytes = std::fs::read(&store.paths().data_file).unwrap();
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(store.load().unwrap(), list);
    }

    #[cfg(unix)]
    #[test]
    fn save_should_atomically_replace_existing_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), ListScope::Global).unwrap();
        let mut list = TaskList::new(ListScope::Global);
        list.add("first").unwrap();
        store.save(&list).unwrap();
        list.add("second").unwrap();

        store.save(&list).unwrap();

        assert_eq!(store.load().unwrap(), list);
    }

    #[test]
    fn failed_temp_write_should_preserve_canonical_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), ListScope::Global).unwrap();
        let mut list = TaskList::new(ListScope::Global);
        list.add("first").unwrap();
        store.save(&list).unwrap();
        std::fs::create_dir(&store.paths().temp_file).unwrap();
        list.add("second").unwrap();

        assert!(store.save(&list).is_err());
        assert_eq!(store.load().unwrap().visible_tasks().count(), 1);
    }

    #[test]
    fn latest_delete_should_restore_after_storage_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), ListScope::Global).unwrap();
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        list.delete(first).unwrap();
        list.delete(second).unwrap();
        store.save(&list).unwrap();

        let mut loaded = store.load().unwrap();

        assert_eq!(loaded.restore_latest().unwrap(), Some(second));
    }

    #[test]
    fn save_should_reject_wrong_scope_before_touching_temp_file() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), ListScope::Global).unwrap();
        std::fs::write(&store.paths().temp_file, b"stale temp data").unwrap();
        let project_scope = ListScope::Project {
            path: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        };
        let list = TaskList::new(project_scope);

        assert!(store.save(&list).is_err());
        assert_eq!(
            std::fs::read(&store.paths().temp_file).unwrap(),
            b"stale temp data"
        );
    }

    #[test]
    fn stale_temp_file_should_never_load_as_canonical() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path(), ListScope::Global).unwrap();
        let mut stale = TaskList::new(ListScope::Global);
        stale.add("stale").unwrap();
        std::fs::write(
            &store.paths().temp_file,
            serde_json::to_vec(&stale).unwrap(),
        )
        .unwrap();

        let loaded = store.load().unwrap();

        assert_eq!(loaded.visible_tasks().count(), 0);
    }
}
