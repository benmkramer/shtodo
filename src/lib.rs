mod action;
mod app;
mod cli;
mod clock;
mod config;
mod input;
mod json;
mod metadata;
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
            add_task(choice, None, argument, print_id, format)?;
        }
        cli::Command::AddSubtask(choice, parent, argument, print_id) => {
            add_task(choice, Some(parent), argument, print_id, format)?;
        }
        cli::Command::Parent(choice, id, parent) => {
            set_parent(choice, id, parent, format)?;
        }
        cli::Command::List(choice, filter) => {
            list_tasks(choice, filter, format)?;
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
            let text = metadata::normalize_due(&text, clock::today);
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
    parent: Option<u64>,
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

    // Stored text never holds a relative word, so its date cannot drift.
    let text = metadata::normalize_due(&text, clock::today).into_owned();
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let request = match parent.and_then(task::TaskId::from_shell_integer) {
        Some(parent) => mutation::Mutation::AddSubtask { parent, text },
        None => mutation::Mutation::Add(text),
    };
    let reply = store.mutate(
        &request,
        storage::ScopePresence::AllowMissing,
        storage::SHELL_LOCK_BUDGET,
    )?;
    let (id, tasks, changed) =
        shell_result(reply).map_err(|error| with_restore_hint(store.scope(), error))?;
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

fn list_tasks(
    choice: cli::ScopeChoice,
    mut list: cli::ListFilter,
    format: cli::OutputFormat,
) -> Result<()> {
    // Reads the clock only for a relative term such as `today` or `overdue`.
    list.filter.due.resolve(clock::today);
    let projection = projection::Projection {
        view: list.view,
        lowercase_query: list
            .search
            .as_deref()
            .map(str::to_lowercase)
            .unwrap_or_default(),
        filter: list.filter,
    };
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let tasks = storage::load_read_only(&home, &scope)?;
    let unfiltered = projection.is_unfiltered();
    let structure = list.structure;
    // The structure flag is the cheapest check, so it runs first.
    let matches = |task: &task::Task| {
        structure.matches(task.parent_id()) && (unfiltered || projection.matches(task))
    };
    let sort: fn(&mut [&task::Task]) = match list.order {
        projection::TaskOrder::Manual => {
            let rows = projection::list_rows(tasks.visible_tasks(), matches);
            return write_list_rows(tasks.scope(), rows, format);
        }
        projection::TaskOrder::Priority => projection::sort_by_priority,
        projection::TaskOrder::Due => projection::sort_by_due,
    };
    let mut sorted: Vec<&task::Task> = tasks.visible_tasks().filter(|task| matches(task)).collect();
    sort(&mut sorted);
    let rows = projection::list_rows(sorted.into_iter(), |_| true);
    write_list_rows(tasks.scope(), rows, format)
}

fn write_list_rows<'a>(
    scope: &'a task::ListScope,
    rows: impl Iterator<Item = projection::ListRow<'a>>,
    format: cli::OutputFormat,
) -> Result<()> {
    if format == cli::OutputFormat::Json {
        return json::write_list(scope, rows.map(|row| row.task));
    }
    let mut stdout = std::io::stdout().lock();
    for projection::ListRow { task, indented } in rows {
        let state = if task.completed() { "done" } else { "open" };
        // Stored text never starts with whitespace, so the indent always means "under the
        // row above".
        let indent = if indented { "  " } else { "" };
        writeln!(
            stdout,
            "{}  {}  {indent}{}",
            task.id().get(),
            state,
            task.text()
        )?;
    }
    Ok(())
}

/// ` with 2 subtasks` when a delete or restore moved subtasks with their parent.
fn subtask_count(count: usize) -> String {
    match count {
        0 => String::new(),
        1 => " with 1 subtask".into(),
        count => format!(" with {count} subtasks"),
    }
}

fn set_parent(
    choice: cli::ScopeChoice,
    raw_id: u64,
    raw_parent: Option<u64>,
    format: cli::OutputFormat,
) -> Result<()> {
    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let id = task::TaskId::from_shell_integer(raw_id)
        .ok_or_else(|| eyre!("task ID must be a positive integer"))?;
    let parent = raw_parent
        .map(|raw| {
            task::TaskId::from_shell_integer(raw)
                .ok_or_else(|| eyre!("parent task ID must be a positive integer"))
        })
        .transpose()?;
    let reply = store.mutate(
        &mutation::Mutation::SetParent { id, parent },
        storage::ScopePresence::AllowMissing,
        storage::SHELL_LOCK_BUDGET,
    )?;
    let (id, tasks, changed) =
        shell_result(reply).map_err(|error| with_restore_hint(store.scope(), error))?;
    if format == cli::OutputFormat::Json {
        return json::write_mutation("parent", &tasks, id, changed);
    }
    let task = tasks.task(id).ok_or(task::ListError::TaskNotFound(id))?;
    let state = if changed { "now" } else { "already" };
    let place = match task.parent_id() {
        Some(parent) => format!("under {}", parent.get()),
        None => "top-level".into(),
    };
    writeln!(
        std::io::stdout().lock(),
        "{} is {state} {place}: {}",
        id.get(),
        task.text()
    )?;
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
    let task = tasks.task(id).ok_or(task::ListError::TaskNotFound(id))?;

    let (prefix, count) = if !changed {
        ("Already deleted", String::new())
    } else {
        let sequence = task.deletion_sequence();
        let count = tasks
            .subtasks(id)
            .iter()
            .filter(|subtask| subtask.deletion_sequence() == sequence)
            .count();
        ("Deleted", subtask_count(count))
    };
    writeln!(
        std::io::stdout().lock(),
        "{prefix} {}{count}: {}",
        id.get(),
        task.text()
    )?;
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
    // A restored parent brings back its group; no subtask under a deleted parent was live.
    let count = match (mutation, changed) {
        (TaskMutation::Restore, true) => subtask_count(
            tasks
                .subtasks(id)
                .iter()
                .filter(|subtask| !subtask.is_deleted())
                .count(),
        ),
        _ => String::new(),
    };
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
    writeln!(
        std::io::stdout().lock(),
        "{prefix} {}{count}: {}",
        id.get(),
        text
    )?;
    Ok(())
}

/// Names the task to restore first, scoped like the command, for a deleted task or parent.
fn with_restore_hint(scope: &task::ListScope, error: color_eyre::Report) -> color_eyre::Report {
    match error.downcast_ref::<task::ListError>() {
        Some(task::ListError::TaskDeleted(id) | task::ListError::ParentDeleted(id)) => {
            let scope_option = match scope {
                task::ListScope::Global => "",
                task::ListScope::Project { .. } => " --local",
            };
            let message = format!(
                "{error}; run `shtodo{scope_option} restore {}` first",
                id.get()
            );
            error.wrap_err(message)
        }
        _ => error,
    }
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
            Some(task::ListError::TaskDeleted(_) | task::ListError::ParentDeleted(_)) => {
                with_restore_hint(store.scope(), error)
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
