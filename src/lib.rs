mod action;
mod app;
mod cli;
mod config;
mod input;
mod storage;
mod task;
mod terminal;
mod ui;

use std::{
    ffi::OsString,
    io::{IsTerminal as _, Read as _, Write as _},
};

use color_eyre::eyre::{Result, WrapErr as _, eyre};

/// Runs shtodo using process arguments and local environment state.
///
/// # Errors
///
/// Returns an error when arguments, storage, terminal setup, input, rendering,
/// persistence, or terminal restoration fails.
pub fn run() -> Result<()> {
    match cli::parse_args(std::env::args_os().skip(1))? {
        cli::Command::Help => {
            std::io::stdout()
                .lock()
                .write_all(cli::usage().as_bytes())?;
        }
        cli::Command::Version => {
            writeln!(
                std::io::stdout().lock(),
                "shtodo {}",
                env!("CARGO_PKG_VERSION")
            )?;
        }
        cli::Command::Add(choice, argument, print_id) => {
            add_task(choice, argument, print_id)?;
        }
        cli::Command::List(choice) => {
            list_tasks(choice)?;
        }
        cli::Command::Delete(choice, id) => {
            delete_task(choice, id)?;
        }
        cli::Command::Done(choice, id) => {
            mutate_task(choice, id, TaskMutation::SetCompleted(true))?;
        }
        cli::Command::Reopen(choice, id) => {
            mutate_task(choice, id, TaskMutation::SetCompleted(false))?;
        }
        cli::Command::Edit(choice, id, argument) => {
            let text = task_text_from_argument(argument)?;
            mutate_task(choice, id, TaskMutation::Edit(&text))?;
        }
        cli::Command::Restore(choice, id) => {
            mutate_task(choice, id, TaskMutation::Restore)?;
        }
        cli::Command::Doctor => {
            let home = storage::home_from_environment()?;
            match config::load(&home) {
                Ok(loaded) => std::io::stdout()
                    .lock()
                    .write_all(loaded.doctor_report().as_bytes())?,
                Err(error) => return Err(eyre!("{}", error.doctor_report())),
            }
        }
        cli::Command::Run(choice) => {
            let home = storage::home_from_environment()?;
            let loaded = config::load(&home).map_err(|error| {
                eyre!("{}\nRun `shtodo doctor` for a focused config check.", error)
            })?;
            let scope = storage::scope_from_environment(choice)?;
            let store = storage::Store::open(&home, scope)?;
            let app = app::App::new(store.load()?);
            terminal::run(app, &store, loaded.keymap())?;
        }
    }
    Ok(())
}

fn add_task(choice: cli::ScopeChoice, argument: Option<OsString>, print_id: bool) -> Result<()> {
    let text = match argument {
        Some(value) => task_text_from_argument(value)?,
        None => read_task_from_stdin()?,
    };
    if text.trim().is_empty() {
        return Err(missing_task_text());
    }

    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let mut tasks = store.load()?;
    let id = tasks.add(&text)?;
    store.save(&tasks)?;

    if print_id {
        writeln!(std::io::stdout().lock(), "{}", id.get())?;
    } else {
        writeln!(std::io::stdout().lock(), "Added: {}", text.trim())?;
    }
    Ok(())
}

fn task_text_from_argument(value: OsString) -> Result<String> {
    value.into_string().map_err(|value| {
        eyre!(
            "task text is not valid UTF-8: {value:?}\n\n{}",
            cli::usage()
        )
    })
}

fn list_tasks(choice: cli::ScopeChoice) -> Result<()> {
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let tasks = storage::load_read_only(&home, &scope)?;
    let mut stdout = std::io::stdout().lock();
    for task in tasks.visible_tasks() {
        let state = if task.completed() { "done" } else { "open" };
        writeln!(stdout, "{}  {}  {}", task.id().get(), state, task.text())?;
    }
    Ok(())
}

fn delete_task(choice: cli::ScopeChoice, raw_id: u64) -> Result<()> {
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let (id, text, already_deleted) = delete_task_in_store(&store, raw_id)?;

    let prefix = if already_deleted {
        "Already deleted"
    } else {
        "Deleted"
    };
    writeln!(std::io::stdout().lock(), "{prefix} {}: {}", id.get(), text)?;
    Ok(())
}

fn delete_task_in_store(
    store: &storage::Store,
    raw_id: u64,
) -> Result<(task::TaskId, String, bool)> {
    let mut tasks = store.load()?;
    let id = task::TaskId::from_shell_integer(raw_id)
        .ok_or_else(|| eyre!("task ID must be a positive integer"))?;
    let (text, already_deleted) = {
        let selected = tasks.task(id).ok_or(task::ListError::TaskNotFound(id))?;
        (selected.text().to_owned(), selected.is_deleted())
    };

    if already_deleted {
        return Ok((id, text, true));
    }

    tasks.delete(id)?;
    store.save(&tasks)?;
    Ok((id, text, false))
}

#[derive(Clone, Copy)]
enum TaskMutation<'a> {
    SetCompleted(bool),
    Edit(&'a str),
    Restore,
}

fn mutate_task(choice: cli::ScopeChoice, raw_id: u64, mutation: TaskMutation<'_>) -> Result<()> {
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let (id, text, changed) = mutate_task_in_store(&store, raw_id, mutation)?;
    let prefix = match (mutation, changed) {
        (TaskMutation::SetCompleted(true), true) => "Completed",
        (TaskMutation::SetCompleted(true), false) => "Already done",
        (TaskMutation::SetCompleted(false), true) => "Reopened",
        (TaskMutation::SetCompleted(false), false) => "Already open",
        (TaskMutation::Edit(_), true) => "Edited",
        (TaskMutation::Edit(_), false) => "Unchanged",
        (TaskMutation::Restore, true) => "Restored",
        (TaskMutation::Restore, false) => "Already live",
    };
    writeln!(std::io::stdout().lock(), "{prefix} {}: {}", id.get(), text)?;
    Ok(())
}

fn mutate_task_in_store(
    store: &storage::Store,
    raw_id: u64,
    mutation: TaskMutation<'_>,
) -> Result<(task::TaskId, String, bool)> {
    let mut tasks = store.load()?;
    let id = task::TaskId::from_shell_integer(raw_id)
        .ok_or_else(|| eyre!("task ID must be a positive integer"))?;
    let changed = match mutation {
        TaskMutation::SetCompleted(completed) => tasks.set_completed(id, completed),
        TaskMutation::Edit(text) => tasks.edit(id, text),
        TaskMutation::Restore => tasks.restore(id),
    }
    .map_err(|error| match error {
        task::ListError::TaskDeleted(id) => {
            let scope_option = match store.scope() {
                task::ListScope::Global => "",
                task::ListScope::Project { .. } => " --local",
            };
            eyre!(
                "{error}; run `shtodo{scope_option} restore {}` first",
                id.get()
            )
        }
        task::ListError::InvalidText => eyre!("{error}\n\n{}", cli::usage()),
        _ => error.into(),
    })?;

    if changed {
        store.save(&tasks)?;
    }
    let text = tasks
        .task(id)
        .ok_or(task::ListError::TaskNotFound(id))?
        .text()
        .to_owned();
    Ok((id, text, changed))
}

fn read_task_from_stdin() -> Result<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(missing_task_text());
    }

    let mut text = String::new();
    stdin
        .lock()
        .read_to_string(&mut text)
        .wrap_err("could not read task text from standard input")?;
    Ok(text)
}

fn missing_task_text() -> color_eyre::Report {
    eyre!("task text is required\n\n{}", cli::usage())
}

#[cfg(test)]
mod tests {
    use super::{TaskMutation, delete_task_in_store, mutate_task_in_store, storage, task};

    #[test]
    fn cli_delete_should_remain_restorable_after_storage_round_trip() {
        let home = tempfile::tempdir().unwrap();
        let store = storage::Store::open(home.path(), task::ListScope::Global).unwrap();
        let mut tasks = task::TaskList::new(task::ListScope::Global);
        let id = tasks.add("restore me").unwrap();
        store.save(&tasks).unwrap();

        delete_task_in_store(&store, id.get()).unwrap();
        let mut loaded = store.load().unwrap();

        assert_eq!(loaded.restore_latest().unwrap(), Some(id));
    }

    #[test]
    fn cli_restore_by_id_should_preserve_completion_order_and_latest_restore_after_reload() {
        let home = tempfile::tempdir().unwrap();
        let store = storage::Store::open(home.path(), task::ListScope::Global).unwrap();
        let mut tasks = task::TaskList::new(task::ListScope::Global);
        let first = tasks.add("first").unwrap();
        let second = tasks.add("second").unwrap();
        tasks.set_completed(first, true).unwrap();
        store.save(&tasks).unwrap();
        delete_task_in_store(&store, first.get()).unwrap();
        delete_task_in_store(&store, second.get()).unwrap();

        let result = mutate_task_in_store(&store, first.get(), TaskMutation::Restore).unwrap();
        let mut loaded = store.load().unwrap();

        assert_eq!(result, (first, "first".into(), true));
        assert_eq!(loaded.tasks()[0].id(), first);
        assert!(loaded.task(first).unwrap().completed());
        assert!(!loaded.task(first).unwrap().is_deleted());
        assert_eq!(loaded.restore_latest().unwrap(), Some(second));
        assert_eq!(loaded.restore_latest().unwrap(), None);
    }
}
