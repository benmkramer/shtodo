//! Versioned shell output, independent of the persisted snapshot schema.

use std::io::Write;

use color_eyre::eyre::{Report, Result};
use serde::Serialize;

use crate::{cli, mutation::Outcome, storage::TransactionError, task};

const SCHEMA_VERSION: u64 = 1;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Scope<'a> {
    Global,
    Project { path: &'a str },
}

impl<'a> From<&'a task::ListScope> for Scope<'a> {
    fn from(scope: &'a task::ListScope) -> Self {
        match scope {
            task::ListScope::Global => Self::Global,
            task::ListScope::Project { path } => Self::Project { path },
        }
    }
}

#[derive(Serialize)]
struct Task<'a> {
    id: u64,
    text: &'a str,
    state: &'static str,
    deleted: bool,
}

impl<'a> From<&'a task::Task> for Task<'a> {
    fn from(task: &'a task::Task) -> Self {
        Self {
            id: task.id().get(),
            text: task.text(),
            state: if task.completed() { "done" } else { "open" },
            deleted: task.is_deleted(),
        }
    }
}

#[derive(Serialize)]
struct ListReply<'a> {
    schema_version: u64,
    command: &'static str,
    scope: Scope<'a>,
    tasks: Vec<Task<'a>>,
}

#[derive(Serialize)]
struct MutationReply<'a> {
    schema_version: u64,
    command: &'a str,
    scope: Scope<'a>,
    task: Task<'a>,
    changed: bool,
}

pub(crate) fn write_list(list: &task::TaskList) -> Result<()> {
    write_json(
        std::io::stdout().lock(),
        &ListReply {
            schema_version: SCHEMA_VERSION,
            command: "list",
            scope: list.scope().into(),
            tasks: list.visible_tasks().map(Task::from).collect(),
        },
    )
}

pub(crate) fn write_mutation(
    command: &str,
    list: &task::TaskList,
    id: task::TaskId,
    changed: bool,
) -> Result<()> {
    let task = list.task(id).ok_or(task::ListError::TaskNotFound(id))?;
    write_json(
        std::io::stdout().lock(),
        &MutationReply {
            schema_version: SCHEMA_VERSION,
            command,
            scope: list.scope().into(),
            task: task.into(),
            changed,
        },
    )
}

#[derive(Serialize)]
struct ErrorReply {
    schema_version: u64,
    error: Error,
}

#[derive(Serialize)]
struct Error {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<u64>,
}

pub(crate) fn write_error(error: &Report) -> Result<()> {
    write_json(std::io::stderr().lock(), &error_reply(error))
}

fn error_reply(error: &Report) -> ErrorReply {
    let (code, task_id) = error_details(error);
    ErrorReply {
        schema_version: SCHEMA_VERSION,
        error: Error {
            code,
            message: format!("{error:#}"),
            task_id,
        },
    }
}

fn error_details(error: &Report) -> (&'static str, Option<u64>) {
    if error.downcast_ref::<cli::CliError>().is_some() {
        return ("invalid_arguments", None);
    }
    if let Some(error) = error.downcast_ref::<TransactionError>() {
        return match error {
            TransactionError::Busy(_) => ("busy", None),
            TransactionError::Failed(source) => error_details(source),
            TransactionError::ReplacedButNotSynced { reply, .. } => (
                "durability_unconfirmed",
                match reply.outcome {
                    Outcome::Changed { id, .. } => Some(id.get()),
                    _ => None,
                },
            ),
        };
    }
    match error.downcast_ref::<task::ListError>() {
        Some(task::ListError::InvalidText) => ("invalid_task_text", None),
        Some(task::ListError::TaskNotFound(id)) => ("task_not_found", Some(id.get())),
        Some(task::ListError::TaskDeleted(id)) => ("task_deleted", Some(id.get())),
        _ => ("command_failed", None),
    }
}

fn write_json(mut writer: impl Write, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    writer.write_all(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mutation::Mutation, storage};

    #[test]
    fn uncertain_save_should_report_the_visible_task_id_without_success() {
        let home = tempfile::tempdir().unwrap();
        let store = storage::Store::open(home.path(), task::ListScope::Global).unwrap();
        let error: Report = store
            .mutate_unsynced(&Mutation::Add("visible task".into()))
            .unwrap_err()
            .into();
        let reply = serde_json::to_value(error_reply(&error)).unwrap();
        assert_eq!(reply["schema_version"], 1);
        assert_eq!(reply["error"]["code"], "durability_unconfirmed");
        assert_eq!(reply["error"]["task_id"], 1);
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("visible")
        );
        assert!(reply.get("task").is_none());
        assert_eq!(store.load().unwrap().visible_tasks().count(), 1);
    }
}
