mod action;
mod app;
mod cli;
mod config;
mod input;
mod json;
mod mutation;
mod projection;
mod session;
mod storage;
mod task;
mod terminal;
mod ui;

use std::{
    ffi::OsString,
    io::{IsTerminal as _, Read as _, Write as _},
    process::ExitCode,
};

use color_eyre::eyre::{Result, WrapErr as _, eyre};

/// Runs shtodo using process arguments and local environment state.
///
/// JSON command failures are reported on stderr and return a failure exit code.
///
/// # Errors
///
/// Returns an error when a text-mode command fails, or when JSON error output
/// cannot be written.
pub fn run() -> Result<ExitCode> {
    let (format, command) = cli::parse_invocation(std::env::args_os().skip(1));
    match command
        .map_err(Into::into)
        .and_then(|command| execute(command, format))
    {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(error) if format == cli::OutputFormat::Json => {
            json::write_error(&error)?;
            Ok(ExitCode::FAILURE)
        }
        Err(error) => Err(error),
    }
}

fn execute(command: cli::Command, format: cli::OutputFormat) -> Result<()> {
    match command {
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
            add_task(choice, argument, print_id, format)?;
        }
        cli::Command::List(choice) => {
            list_tasks(choice, format)?;
        }
        cli::Command::Delete(choice, id) => {
            delete_task(choice, id, format)?;
        }
        cli::Command::Done(choice, id) => {
            mutate_task(choice, id, TaskMutation::SetCompleted(true), format)?;
        }
        cli::Command::Reopen(choice, id) => {
            mutate_task(choice, id, TaskMutation::SetCompleted(false), format)?;
        }
        cli::Command::Edit(choice, id, argument) => {
            let text = task_text_from_argument(argument)?;
            mutate_task(choice, id, TaskMutation::Edit(&text), format)?;
        }
        cli::Command::Restore(choice, id) => {
            mutate_task(choice, id, TaskMutation::Restore, format)?;
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
            let snapshot = store.read_snapshot(storage::ScopePresence::AllowMissing)?;
            terminal::run(snapshot, &store, loaded.keymap())?;
        }
    }
    Ok(())
}

fn add_task(
    choice: cli::ScopeChoice,
    argument: Option<OsString>,
    print_id: bool,
    format: cli::OutputFormat,
) -> Result<()> {
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
    let reply = store.mutate(
        &mutation::Mutation::Add(text),
        storage::ScopePresence::AllowMissing,
        storage::SHELL_LOCK_BUDGET,
    )?;
    let (id, tasks, changed) = shell_result(reply)?;
    if format == cli::OutputFormat::Json {
        return json::write_mutation("add", &tasks, id, changed);
    }
    let text = tasks
        .task(id)
        .ok_or(task::ListError::TaskNotFound(id))?
        .text();
    if print_id {
        writeln!(std::io::stdout().lock(), "{}", id.get())?;
    } else {
        writeln!(std::io::stdout().lock(), "Added: {text}")?;
    }
    Ok(())
}

fn task_text_from_argument(value: OsString) -> Result<String> {
    value.into_string().map_err(|value| {
        color_eyre::Report::from(task::ListError::InvalidText).wrap_err(format!(
            "task text is not valid UTF-8: {value:?}\n\n{}",
            cli::usage()
        ))
    })
}

fn list_tasks(choice: cli::ScopeChoice, format: cli::OutputFormat) -> Result<()> {
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let tasks = storage::load_read_only(&home, &scope)?;
    if format == cli::OutputFormat::Json {
        return json::write_list(&tasks);
    }
    let mut stdout = std::io::stdout().lock();
    for task in tasks.visible_tasks() {
        let state = if task.completed() { "done" } else { "open" };
        writeln!(stdout, "{}  {}  {}", task.id().get(), state, task.text())?;
    }
    Ok(())
}

fn delete_task(choice: cli::ScopeChoice, raw_id: u64, format: cli::OutputFormat) -> Result<()> {
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let (id, tasks, changed) = delete_task_in_store(&store, raw_id)?;
    if format == cli::OutputFormat::Json {
        return json::write_mutation("delete", &tasks, id, changed);
    }
    let text = tasks
        .task(id)
        .ok_or(task::ListError::TaskNotFound(id))?
        .text();

    let prefix = if !changed {
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
) -> Result<(task::TaskId, task::TaskList, bool)> {
    let id = task::TaskId::from_shell_integer(raw_id)
        .ok_or_else(|| eyre!("task ID must be a positive integer"))?;
    let reply = store.mutate(
        &mutation::Mutation::Delete { id, observed: None },
        storage::ScopePresence::AllowMissing,
        storage::SHELL_LOCK_BUDGET,
    )?;
    shell_result(reply)
}

fn shell_result(reply: storage::MutationReply) -> Result<(task::TaskId, task::TaskList, bool)> {
    match reply.outcome {
        mutation::Outcome::Changed { id, .. } => Ok((id, reply.snapshot.list, true)),
        mutation::Outcome::AlreadyInState { id } => Ok((id, reply.snapshot.list, false)),
        mutation::Outcome::Rejected(error) => Err(error.into()),
        mutation::Outcome::Conflict {
            id,
            kind: mutation::ConflictKind::NoLongerActive,
        } => Err(task::ListError::TaskDeleted(id).into()),
        outcome => Err(eyre!("could not apply shell mutation: {outcome:?}")),
    }
}

#[derive(Clone, Copy)]
enum TaskMutation<'a> {
    SetCompleted(bool),
    Edit(&'a str),
    Restore,
}

fn mutate_task(
    choice: cli::ScopeChoice,
    raw_id: u64,
    mutation: TaskMutation<'_>,
    format: cli::OutputFormat,
) -> Result<()> {
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let (id, tasks, changed) = mutate_task_in_store(&store, raw_id, mutation)?;
    if format == cli::OutputFormat::Json {
        let command = match mutation {
            TaskMutation::SetCompleted(true) => "done",
            TaskMutation::SetCompleted(false) => "reopen",
            TaskMutation::Edit(_) => "edit",
            TaskMutation::Restore => "restore",
        };
        return json::write_mutation(command, &tasks, id, changed);
    }
    let text = tasks
        .task(id)
        .ok_or(task::ListError::TaskNotFound(id))?
        .text();
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
) -> Result<(task::TaskId, task::TaskList, bool)> {
    let id = task::TaskId::from_shell_integer(raw_id)
        .ok_or_else(|| eyre!("task ID must be a positive integer"))?;
    let request = match mutation {
        TaskMutation::SetCompleted(completed) => mutation::Mutation::SetCompleted { id, completed },
        TaskMutation::Edit(text) => mutation::Mutation::EditText {
            id,
            text: text.into(),
            original: None,
        },
        TaskMutation::Restore => mutation::Mutation::Restore { id, observed: None },
    };
    let reply = store.mutate(
        &request,
        storage::ScopePresence::AllowMissing,
        storage::SHELL_LOCK_BUDGET,
    );
    reply
        .map_err(|error| match error {
            storage::TransactionError::Failed(error) => error,
            other => other.into(),
        })
        .and_then(shell_result)
        .map_err(|error| match error.downcast_ref::<task::ListError>() {
            Some(task::ListError::TaskDeleted(id)) => {
                let scope_option = match store.scope() {
                    task::ListScope::Global => "",
                    task::ListScope::Project { .. } => " --local",
                };
                let message = format!(
                    "{error}; run `shtodo{scope_option} restore {}` first",
                    id.get()
                );
                error.wrap_err(message)
            }
            Some(task::ListError::InvalidText) => {
                let message = format!("{error}\n\n{}", cli::usage());
                error.wrap_err(message)
            }
            _ => error,
        })
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
    color_eyre::Report::from(task::ListError::InvalidText)
        .wrap_err(format!("task text is required\n\n{}", cli::usage()))
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

        assert_eq!(result.0, first);
        assert_eq!(result.1.task(first).unwrap().text(), "first");
        assert!(result.2);
        assert_eq!(loaded.tasks()[0].id(), first);
        assert!(loaded.task(first).unwrap().completed());
        assert!(!loaded.task(first).unwrap().is_deleted());
        assert_eq!(loaded.restore_latest().unwrap(), Some(second));
        assert_eq!(loaded.restore_latest().unwrap(), None);
    }
}
