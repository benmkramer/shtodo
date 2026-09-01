use std::{collections::HashSet, error::Error, fmt, path::Path};

use serde::{Deserialize, Serialize};

pub(crate) const SCHEMA_VERSION: u64 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct TaskId(u64);

impl TaskId {
    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ListScope {
    Global,
    Project { path: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Task {
    id: TaskId,
    text: String,
    completed: bool,
    deletion_sequence: Option<u64>,
}

impl Task {
    pub(crate) fn id(&self) -> TaskId {
        self.id
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn completed(&self) -> bool {
        self.completed
    }

    pub(crate) fn is_deleted(&self) -> bool {
        self.deletion_sequence.is_some()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskList {
    schema_version: u64,
    scope: ListScope,
    next_task_id: u64,
    next_deletion_sequence: u64,
    tasks: Vec<Task>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MoveDirection {
    Up,
    Down,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ListError {
    InvalidText,
    TaskNotFound(TaskId),
    TaskIdExhausted,
    DeletionSequenceExhausted,
    InvalidData(String),
}

impl fmt::Display for ListError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidText => formatter.write_str("task text must be a non-empty single line"),
            Self::TaskNotFound(id) => write!(formatter, "task {} was not found", id.get()),
            Self::TaskIdExhausted => formatter.write_str("task ID space is exhausted"),
            Self::DeletionSequenceExhausted => {
                formatter.write_str("deletion sequence space is exhausted")
            }
            Self::InvalidData(message) => write!(formatter, "invalid task list data: {message}"),
        }
    }
}

impl Error for ListError {}

impl TaskList {
    pub(crate) fn new(scope: ListScope) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            scope,
            next_task_id: 1,
            next_deletion_sequence: 1,
            tasks: Vec::new(),
        }
    }

    pub(crate) fn scope(&self) -> &ListScope {
        &self.scope
    }

    #[cfg(test)]
    pub(crate) fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub(crate) fn visible_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks
            .iter()
            .filter(|task| task.deletion_sequence.is_none())
    }

    pub(crate) fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub(crate) fn add(&mut self, text: &str) -> Result<TaskId, ListError> {
        let text = validated_text(text)?;
        let id = TaskId(self.next_task_id);
        let next_task_id = self
            .next_task_id
            .checked_add(1)
            .ok_or(ListError::TaskIdExhausted)?;

        self.tasks.push(Task {
            id,
            text: text.into(),
            completed: false,
            deletion_sequence: None,
        });
        self.next_task_id = next_task_id;

        Ok(id)
    }

    pub(crate) fn edit(&mut self, id: TaskId, text: &str) -> Result<(), ListError> {
        let text = validated_text(text)?;
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or(ListError::TaskNotFound(id))?;
        task.text = text.into();
        Ok(())
    }

    pub(crate) fn toggle_complete(&mut self, id: TaskId) -> Result<(), ListError> {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or(ListError::TaskNotFound(id))?;
        task.completed = !task.completed;
        Ok(())
    }

    pub(crate) fn delete(&mut self, id: TaskId) -> Result<(), ListError> {
        let index = self
            .tasks
            .iter()
            .position(|task| task.id == id && !task.is_deleted())
            .ok_or(ListError::TaskNotFound(id))?;
        let next_deletion_sequence = self
            .next_deletion_sequence
            .checked_add(1)
            .ok_or(ListError::DeletionSequenceExhausted)?;
        self.tasks[index].deletion_sequence = Some(self.next_deletion_sequence);
        self.next_deletion_sequence = next_deletion_sequence;
        Ok(())
    }

    pub(crate) fn restore_latest(&mut self) -> Result<Option<TaskId>, ListError> {
        let index = self
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(index, task)| task.deletion_sequence.map(|sequence| (index, sequence)))
            .max_by_key(|(_, sequence)| *sequence)
            .map(|(index, _)| index);
        let Some(index) = index else {
            return Ok(None);
        };
        let task = &mut self.tasks[index];
        task.deletion_sequence = None;
        Ok(Some(task.id))
    }

    pub(crate) fn move_visible(
        &mut self,
        id: TaskId,
        direction: MoveDirection,
    ) -> Result<bool, ListError> {
        let index = self
            .tasks
            .iter()
            .position(|task| task.id == id && !task.is_deleted())
            .ok_or(ListError::TaskNotFound(id))?;
        let swap_index = match direction {
            MoveDirection::Up => (0..index)
                .rev()
                .find(|candidate| !self.tasks[*candidate].is_deleted()),
            MoveDirection::Down => ((index + 1)..self.tasks.len())
                .find(|candidate| !self.tasks[*candidate].is_deleted()),
        };
        let Some(swap_index) = swap_index else {
            return Ok(false);
        };
        self.tasks.swap(index, swap_index);
        Ok(true)
    }

    pub(crate) fn adjacent_visible(&self, id: TaskId, direction: MoveDirection) -> Option<TaskId> {
        match direction {
            MoveDirection::Up => {
                let mut previous = None;
                for task in self.visible_tasks() {
                    if task.id == id {
                        return previous;
                    }
                    previous = Some(task.id);
                }
                None
            }
            MoveDirection::Down => {
                let mut found = false;
                for task in self.visible_tasks() {
                    if found {
                        return Some(task.id);
                    }
                    found = task.id == id;
                }
                None
            }
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ListError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ListError::InvalidData("unsupported schema version".into()));
        }
        validate_scope(&self.scope)?;
        if self.next_task_id == 0 {
            return Err(ListError::InvalidData(
                "next task ID must be nonzero".into(),
            ));
        }
        if self.next_deletion_sequence == 0 {
            return Err(ListError::InvalidData(
                "next deletion sequence must be nonzero".into(),
            ));
        }

        let mut task_ids = HashSet::new();
        let mut deletion_sequences = HashSet::new();
        for task in &self.tasks {
            if task.id.0 == 0 || !task_ids.insert(task.id) {
                return Err(ListError::InvalidData(
                    "task IDs must be unique and nonzero".into(),
                ));
            }
            if task.id.0 >= self.next_task_id {
                return Err(ListError::InvalidData(
                    "next task ID must be above all task IDs".into(),
                ));
            }
            if validated_text(&task.text)? != task.text {
                return Err(ListError::InvalidData("task text is not canonical".into()));
            }
            if let Some(sequence) = task.deletion_sequence {
                if sequence == 0 || !deletion_sequences.insert(sequence) {
                    return Err(ListError::InvalidData(
                        "deletion sequences must be unique and nonzero".into(),
                    ));
                }
                if sequence >= self.next_deletion_sequence {
                    return Err(ListError::InvalidData(
                        "next deletion sequence must be above all deletion sequences".into(),
                    ));
                }
            }
        }

        Ok(())
    }
}

fn validated_text(text: &str) -> Result<&str, ListError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains(['\n', '\r']) {
        return Err(ListError::InvalidText);
    }
    Ok(trimmed)
}

fn validate_scope(scope: &ListScope) -> Result<(), ListError> {
    match scope {
        ListScope::Global => Ok(()),
        ListScope::Project { path } if project_path_is_absolute(path) => Ok(()),
        ListScope::Project { .. } => Err(ListError::InvalidData(
            "project path must be non-empty and absolute".into(),
        )),
    }
}

fn project_path_is_absolute(path: &str) -> bool {
    Path::new(path).is_absolute()
}

#[cfg(test)]
mod tests {
    use super::{ListScope, MoveDirection, TaskId, TaskList, project_path_is_absolute};

    #[test]
    fn validate_should_use_current_platform_absolute_path_semantics() {
        let absolute = std::env::current_dir().unwrap();
        let path = absolute.to_str().unwrap().to_owned();
        assert!(std::path::Path::new(&path).is_absolute());
        assert!(project_path_is_absolute(&path));

        let list = TaskList::new(ListScope::Project { path });

        assert!(list.validate().is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn validate_should_accept_canonical_windows_absolute_project_path() {
        let path = r"\\?\C:\work\shtodo".to_owned();
        assert!(std::path::Path::new(&path).is_absolute());

        let list = TaskList::new(ListScope::Project { path });

        assert!(list.validate().is_ok());
    }

    #[test]
    fn add_should_assign_id_trim_text_and_preserve_order() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("  first  ").unwrap();
        let second = list.add("second").unwrap();

        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 2);
        assert_eq!(
            list.visible_tasks()
                .map(|task| task.text())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
    }

    #[test]
    fn edit_and_toggle_should_change_only_selected_task() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        list.edit(second, "changed").unwrap();
        list.toggle_complete(first).unwrap();

        assert!(list.task(first).unwrap().completed());
        assert_eq!(list.task(second).unwrap().text(), "changed");
    }

    #[test]
    fn add_should_reject_blank_or_multiline_text() {
        let mut list = TaskList::new(ListScope::Global);
        assert!(list.add("   ").is_err());
        assert!(list.add("one\ntwo").is_err());
    }

    #[test]
    fn validate_should_reject_stale_counter_duplicate_id_and_noncanonical_text() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("one").unwrap();

        list.next_task_id = 1;
        assert!(list.validate().is_err());
        list.next_task_id = 2;

        list.tasks[0].id = TaskId(0);
        assert!(list.validate().is_err());
        list.tasks[0].id = TaskId(1);

        list.tasks.push(list.tasks[0].clone());
        assert!(list.validate().is_err());
        list.tasks.pop();

        list.tasks[0].text = " one ".into();
        assert!(list.validate().is_err());
        list.tasks[0].text = "one\ntwo".into();
        assert!(list.validate().is_err());
    }

    #[test]
    fn restore_latest_should_survive_interleaved_deletes() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        list.delete(first).unwrap();
        list.delete(second).unwrap();

        assert_eq!(list.restore_latest().unwrap(), Some(second));
        assert_eq!(list.restore_latest().unwrap(), Some(first));
        assert_eq!(list.restore_latest().unwrap(), None);
    }

    #[test]
    fn move_visible_should_swap_across_hidden_tombstone() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let hidden = list.add("hidden").unwrap();
        let third = list.add("third").unwrap();
        list.delete(hidden).unwrap();

        assert!(list.move_visible(first, MoveDirection::Down).unwrap());
        assert_eq!(
            list.visible_tasks()
                .map(|task| task.id())
                .collect::<Vec<_>>(),
            vec![third, first]
        );
        assert_eq!(list.restore_latest().unwrap(), Some(hidden));
        assert_eq!(
            list.tasks()
                .iter()
                .map(|task| task.text())
                .collect::<Vec<_>>(),
            vec!["third", "hidden", "first"]
        );
    }

    #[test]
    fn validate_should_reject_stale_deletion_counter() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("task").unwrap();
        list.delete(id).unwrap();
        list.next_deletion_sequence = 1;

        assert!(list.validate().is_err());
    }
}
