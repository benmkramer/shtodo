use std::{collections::HashSet, error::Error, fmt, path::Path};

use serde::{Deserialize, Serialize};

/// Lists with no subtask keep schema 1, which every released binary reads.
pub(crate) const SCHEMA_VERSION: u64 = 1;
/// Written only while some task has a parent; older binaries refuse it unread.
pub(crate) const SUBTASK_SCHEMA_VERSION: u64 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct TaskId(u64);

impl TaskId {
    pub(crate) fn from_shell_integer(value: u64) -> Option<Self> {
        (value > 0).then_some(Self(value))
    }

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
    /// Absent on disk for a root, so a list without subtasks keeps its schema 1 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_id: Option<TaskId>,
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

    pub(crate) fn deletion_sequence(&self) -> Option<u64> {
        self.deletion_sequence
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "list and JSON output arrive in Stage 2")
    )]
    pub(crate) fn parent_id(&self) -> Option<TaskId> {
        self.parent_id
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
    TaskDeleted(TaskId),
    TaskIdExhausted,
    DeletionSequenceExhausted,
    InvalidData(String),
    /// The named parent is in trash and must be restored first.
    ParentDeleted(TaskId),
    InvalidParent(ParentError),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ParentError {
    SelfParent(TaskId),
    ParentIsSubtask(TaskId),
    HasSubtasks(TaskId),
}

impl fmt::Display for ListError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidText => formatter.write_str("task text must be a non-empty single line"),
            Self::TaskNotFound(id) => write!(formatter, "task {} was not found", id.get()),
            Self::TaskDeleted(id) => write!(
                formatter,
                "task {} is deleted; restore it before changing it",
                id.get()
            ),
            Self::TaskIdExhausted => formatter.write_str("task ID space is exhausted"),
            Self::DeletionSequenceExhausted => {
                formatter.write_str("deletion sequence space is exhausted")
            }
            Self::InvalidData(message) => write!(formatter, "invalid task list data: {message}"),
            Self::ParentDeleted(id) => write!(formatter, "parent task {} is deleted", id.get()),
            Self::InvalidParent(ParentError::SelfParent(id)) => {
                write!(formatter, "task {} cannot be its own parent", id.get())
            }
            Self::InvalidParent(ParentError::ParentIsSubtask(id)) => write!(
                formatter,
                "task {} is a subtask; subtasks cannot have subtasks",
                id.get()
            ),
            Self::InvalidParent(ParentError::HasSubtasks(id)) => write!(
                formatter,
                "task {} has subtasks; promote them first",
                id.get()
            ),
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

    /// Every stored task, tombstones included, in canonical order.
    pub(crate) fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub(crate) fn visible_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks
            .iter()
            .filter(|task| task.deletion_sequence.is_none())
    }

    /// Lists tombstones from most recently deleted to oldest without changing canonical order.
    /// Stable, so a group deleted together keeps its parent first.
    pub(crate) fn deleted_tasks(&self) -> Vec<&Task> {
        let mut deleted = self
            .tasks
            .iter()
            .filter(|task| task.is_deleted())
            .collect::<Vec<_>>();
        deleted.sort_by_key(|task| std::cmp::Reverse(task.deletion_sequence));
        deleted
    }

    /// Sets the stored version to the lowest schema that can hold these tasks.
    pub(crate) fn stamp_schema_version(&mut self) {
        self.schema_version = required_schema_version(&self.tasks);
    }

    fn index_of(&self, id: TaskId) -> Result<usize, ListError> {
        self.tasks
            .iter()
            .position(|task| task.id == id)
            .ok_or(ListError::TaskNotFound(id))
    }

    fn next_id(&self) -> Result<(TaskId, u64), ListError> {
        let next_task_id = self
            .next_task_id
            .checked_add(1)
            .ok_or(ListError::TaskIdExhausted)?;
        Ok((TaskId(self.next_task_id), next_task_id))
    }

    pub(crate) fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub(crate) fn add(&mut self, text: &str) -> Result<TaskId, ListError> {
        let text = validated_text(text)?;
        let (id, next_task_id) = self.next_id()?;

        self.tasks.push(Task {
            id,
            text: text.into(),
            completed: false,
            deletion_sequence: None,
            parent_id: None,
        });
        self.next_task_id = next_task_id;

        Ok(id)
    }

    /// Adds a subtask at the end of a live top-level task's group.
    pub(crate) fn add_subtask(&mut self, parent: TaskId, text: &str) -> Result<TaskId, ListError> {
        let text = validated_text(text)?;
        let parent_index = self.index_of(parent)?;
        let parent_task = &self.tasks[parent_index];
        if parent_task.is_deleted() {
            return Err(ListError::ParentDeleted(parent));
        }
        if parent_task.parent_id.is_some() {
            return Err(ListError::InvalidParent(ParentError::ParentIsSubtask(
                parent,
            )));
        }
        let (id, next_task_id) = self.next_id()?;
        let end = block(&self.tasks, parent_index).end;
        self.tasks.insert(
            end,
            Task {
                id,
                text: text.into(),
                completed: false,
                deletion_sequence: None,
                parent_id: Some(parent),
            },
        );
        self.next_task_id = next_task_id;
        Ok(id)
    }

    /// Nests a live task at the end of `parent`'s group, or promotes it right after its
    /// former group with `None`. Returns whether the link changed.
    pub(crate) fn set_parent(
        &mut self,
        id: TaskId,
        parent: Option<TaskId>,
    ) -> Result<bool, ListError> {
        let index = self.index_of(id)?;
        let task = &self.tasks[index];
        if task.is_deleted() {
            return Err(ListError::TaskDeleted(id));
        }
        let Some(parent) = parent else {
            let Some(former) = task.parent_id else {
                return Ok(false);
            };
            let mut task = self.tasks.remove(index);
            task.parent_id = None;
            let end = block(&self.tasks, self.index_of(former)?).end;
            self.tasks.insert(end, task);
            return Ok(true);
        };
        let parent_index = self.index_of(parent)?;
        if self.tasks[parent_index].is_deleted() {
            return Err(ListError::ParentDeleted(parent));
        }
        if parent == id {
            return Err(ListError::InvalidParent(ParentError::SelfParent(id)));
        }
        if self.tasks[parent_index].parent_id.is_some() {
            return Err(ListError::InvalidParent(ParentError::ParentIsSubtask(
                parent,
            )));
        }
        if task.parent_id == Some(parent) {
            return Ok(false);
        }
        let top_level = task.parent_id.is_none();
        if top_level
            && self.tasks[block(&self.tasks, index)]
                .iter()
                .skip(1)
                .any(|task| !task.is_deleted())
        {
            return Err(ListError::InvalidParent(ParentError::HasSubtasks(id)));
        }
        let mut task = self.tasks.remove(index);
        if top_level {
            // Its subtasks are all in trash; they stay in place as top-level tombstones.
            for orphan in self.tasks[index..]
                .iter_mut()
                .take_while(|task| task.parent_id.is_some())
            {
                orphan.parent_id = None;
            }
        }
        task.parent_id = Some(parent);
        let end = block(&self.tasks, self.index_of(parent)?).end;
        self.tasks.insert(end, task);
        Ok(true)
    }

    /// Returns whether the live task's canonical text changed.
    pub(crate) fn edit(&mut self, id: TaskId, text: &str) -> Result<bool, ListError> {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or(ListError::TaskNotFound(id))?;
        if task.is_deleted() {
            return Err(ListError::TaskDeleted(id));
        }
        let text = validated_text(text)?;
        if task.text == text {
            return Ok(false);
        }
        task.text = text.into();
        Ok(true)
    }

    /// Sets a live task's completion state, returning whether it changed.
    pub(crate) fn set_completed(&mut self, id: TaskId, completed: bool) -> Result<bool, ListError> {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or(ListError::TaskNotFound(id))?;
        if task.is_deleted() {
            return Err(ListError::TaskDeleted(id));
        }
        if task.completed == completed {
            return Ok(false);
        }
        task.completed = completed;
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn toggle_complete(&mut self, id: TaskId) -> Result<(), ListError> {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or(ListError::TaskNotFound(id))?;
        task.completed = !task.completed;
        Ok(())
    }

    /// Deletes a live task; a top-level task takes its live subtasks along under one sequence.
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
        let slots = if self.tasks[index].parent_id.is_none() {
            block(&self.tasks, index)
        } else {
            index..index + 1
        };
        for task in &mut self.tasks[slots] {
            if !task.is_deleted() {
                task.deletion_sequence = Some(self.next_deletion_sequence);
            }
        }
        self.next_deletion_sequence = next_deletion_sequence;
        Ok(())
    }

    /// Restores every task holding the highest sequence, returning the first (the parent).
    pub(crate) fn restore_latest(&mut self) -> Result<Option<TaskId>, ListError> {
        let Some(latest) = self
            .tasks
            .iter()
            .filter_map(|task| task.deletion_sequence)
            .max()
        else {
            return Ok(None);
        };
        let mut first = None;
        for task in &mut self.tasks {
            if task.deletion_sequence == Some(latest) {
                task.deletion_sequence = None;
                first.get_or_insert(task.id);
            }
        }
        Ok(first)
    }

    /// Clears the task's tombstone, and on a top-level task those of the subtasks deleted with
    /// it, returning false if it is already live. A subtask waits for its parent.
    pub(crate) fn restore(&mut self, id: TaskId) -> Result<bool, ListError> {
        let index = self.index_of(id)?;
        let task = &self.tasks[index];
        let Some(sequence) = task.deletion_sequence else {
            return Ok(false);
        };
        let slots = match task.parent_id {
            Some(parent) => {
                if self.task(parent).is_some_and(Task::is_deleted) {
                    return Err(ListError::ParentDeleted(parent));
                }
                index..index + 1
            }
            None => block(&self.tasks, index),
        };
        for task in &mut self.tasks[slots] {
            if task.deletion_sequence == Some(sequence) {
                task.deletion_sequence = None;
            }
        }
        Ok(true)
    }

    /// Moves a live task past its adjacent live sibling; a top-level task carries its group.
    pub(crate) fn move_visible(
        &mut self,
        id: TaskId,
        direction: MoveDirection,
    ) -> Result<bool, ListError> {
        if !self.task(id).is_some_and(|task| !task.is_deleted()) {
            return Err(ListError::TaskNotFound(id));
        }
        match self.adjacent_visible(id, direction) {
            Some(neighbor) => self.swap_visible(id, neighbor),
            None => Ok(false),
        }
    }

    /// The adjacent live sibling: a top-level task's neighbors are top-level tasks, and a
    /// subtask's are the subtasks of the same parent.
    pub(crate) fn adjacent_visible(&self, id: TaskId, direction: MoveDirection) -> Option<TaskId> {
        adjacent_sibling(self.visible_tasks(), id, direction, |_| true)
    }

    /// Swap two non-deleted siblings without moving any other canonical slots. Top-level
    /// siblings swap their whole groups, tombstones included.
    pub(crate) fn swap_visible(
        &mut self,
        first: TaskId,
        second: TaskId,
    ) -> Result<bool, ListError> {
        let index_of = |id| {
            self.tasks
                .iter()
                .position(|task| task.id == id && !task.is_deleted())
                .ok_or(ListError::TaskNotFound(id))
        };
        let first_index = index_of(first)?;
        let second_index = index_of(second)?;
        if first_index == second_index {
            return Ok(false);
        }
        if self.tasks[first_index].parent_id.is_none() {
            swap_blocks(&mut self.tasks, first_index, second_index);
        } else {
            self.tasks.swap(first_index, second_index);
        }
        Ok(true)
    }

    pub(crate) fn validate(&self) -> Result<(), ListError> {
        if self.schema_version != SCHEMA_VERSION && self.schema_version != SUBTASK_SCHEMA_VERSION {
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
        // Tree order: each subtask follows its parent's top-level row, so one root of state
        // rejects missing, later, nested, and self parents without an index.
        let mut root: Option<&Task> = None;
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
            let mut shares_parent_sequence = false;
            match task.parent_id {
                None => root = Some(task),
                Some(parent) => {
                    if self.schema_version == SCHEMA_VERSION {
                        return Err(ListError::InvalidData(
                            "a schema 1 list cannot hold subtasks".into(),
                        ));
                    }
                    let Some(root) = root.filter(|root| root.id == parent) else {
                        return Err(ListError::InvalidData(
                            "a subtask must directly follow its parent's group".into(),
                        ));
                    };
                    match (root.deletion_sequence, task.deletion_sequence) {
                        (Some(_), None) => {
                            return Err(ListError::InvalidData(
                                "a live subtask's parent must be live".into(),
                            ));
                        }
                        // Deleted with the parent (equal), or before it (lower).
                        (Some(parent), Some(child)) if child > parent => {
                            return Err(ListError::InvalidData(
                                "a subtask cannot be deleted after its parent".into(),
                            ));
                        }
                        (Some(parent), Some(child)) => shares_parent_sequence = parent == child,
                        _ => {}
                    }
                }
            }
            if let Some(sequence) = task.deletion_sequence {
                if sequence == 0 || !(shares_parent_sequence || deletion_sequences.insert(sequence))
                {
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

/// The lowest schema that can hold these tasks; storage writes exactly this.
pub(crate) fn required_schema_version(tasks: &[Task]) -> u64 {
    if tasks.iter().any(|task| task.parent_id.is_some()) {
        SUBTASK_SCHEMA_VERSION
    } else {
        SCHEMA_VERSION
    }
}

/// The canonical slots of `index`'s root and every subtask after it, tombstones included.
fn block(tasks: &[Task], index: usize) -> std::ops::Range<usize> {
    let end = tasks[index + 1..]
        .iter()
        .position(|task| task.parent_id.is_none())
        .map_or(tasks.len(), |offset| index + 1 + offset);
    index..end
}

/// Exchanges two root groups; every slot between them, such as a deleted group, stays put.
fn swap_blocks(tasks: &mut [Task], first: usize, second: usize) {
    let (first, second) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    let (first, second) = (block(tasks, first), block(tasks, second));
    let (first_len, middle_len) = (first.len(), second.start - first.end);
    let span = &mut tasks[first.start..second.end];
    span.rotate_left(first_len); // middle, second, first
    span[..middle_len + second.len()].rotate_left(middle_len); // second, middle, first
}

/// One pass in canonical order over live `tasks`. A top-level task's siblings are the matching
/// top-level tasks; a subtask's are the matching subtasks of its parent. A target that does
/// not match has no neighbor, as before subtasks.
pub(crate) fn adjacent_sibling<'a>(
    tasks: impl Iterator<Item = &'a Task>,
    id: TaskId,
    direction: MoveDirection,
    matches: impl Fn(&Task) -> bool,
) -> Option<TaskId> {
    let mut previous_root = None;
    let mut previous_child = None;
    // The target's parent, once the target is found while moving down.
    let mut target_parent: Option<Option<TaskId>> = None;
    for task in tasks {
        if task.parent_id.is_none() {
            previous_child = None;
        }
        if let Some(parent) = target_parent {
            if parent.is_some() && task.parent_id.is_none() {
                return None;
            }
            if task.parent_id == parent && matches(task) {
                return Some(task.id);
            }
            continue;
        }
        if task.id == id {
            if !matches(task) {
                return None;
            }
            match direction {
                MoveDirection::Up if task.parent_id.is_none() => return previous_root,
                MoveDirection::Up => return previous_child,
                MoveDirection::Down => target_parent = Some(task.parent_id),
            }
        } else if matches(task) {
            if task.parent_id.is_none() {
                previous_root = Some(task.id);
            } else {
                previous_child = Some(task.id);
            }
        }
    }
    None
}

pub(crate) fn validated_text(text: &str) -> Result<&str, ListError> {
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
    use super::{
        ListError, ListScope, MoveDirection, ParentError, SCHEMA_VERSION, SUBTASK_SCHEMA_VERSION,
        Task, TaskId, TaskList, project_path_is_absolute, required_schema_version, swap_blocks,
    };

    fn ids(list: &TaskList) -> Vec<u64> {
        list.tasks().iter().map(|task| task.id().get()).collect()
    }

    fn parents(list: &TaskList) -> Vec<Option<u64>> {
        list.tasks()
            .iter()
            .map(|task| task.parent_id().map(TaskId::get))
            .collect()
    }

    fn sequences(list: &TaskList) -> Vec<Option<u64>> {
        list.tasks()
            .iter()
            .map(|task| task.deletion_sequence())
            .collect()
    }

    /// Groups 1 [4, 5], 2 [6], and 3, in a schema 2 list.
    fn groups() -> TaskList {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let fourth = list.add("fourth").unwrap();
        list.add("sixth").unwrap();
        assert_eq!(list.add_subtask(first, "second").unwrap(), TaskId(4));
        list.add_subtask(first, "third").unwrap();
        list.add_subtask(fourth, "fifth").unwrap();
        list.stamp_schema_version();
        list
    }

    #[test]
    fn required_schema_version_should_be_two_only_while_a_parent_exists() {
        let mut list = TaskList::new(ListScope::Global);
        assert_eq!(required_schema_version(list.tasks()), SCHEMA_VERSION);
        let parent = list.add("parent").unwrap();
        assert_eq!(required_schema_version(list.tasks()), 1);
        let child = list.add_subtask(parent, "child").unwrap();
        assert_eq!(
            required_schema_version(list.tasks()),
            SUBTASK_SCHEMA_VERSION
        );
        list.stamp_schema_version();
        assert_eq!(list.schema_version, 2);
        list.set_parent(child, None).unwrap();
        list.stamp_schema_version();
        assert_eq!(list.schema_version, 1);
        assert_eq!(list.validate(), Ok(()));
    }

    #[test]
    fn validate_should_accept_groups_with_live_co_deleted_and_earlier_deleted_subtasks() {
        let mut list = groups();
        assert_eq!(ids(&list), [1, 4, 5, 2, 6, 3]);
        assert_eq!(list.validate(), Ok(()));
        list.delete(TaskId(5)).unwrap();
        list.delete(TaskId(1)).unwrap();
        list.delete(TaskId(2)).unwrap();
        assert_eq!(
            sequences(&list),
            [Some(2), Some(2), Some(1), Some(3), Some(3), None]
        );
        assert_eq!(list.validate(), Ok(()));
        // A schema 2 file with no links is read as it is.
        let mut flat = TaskList::new(ListScope::Global);
        flat.add("flat").unwrap();
        flat.schema_version = SUBTASK_SCHEMA_VERSION;
        assert_eq!(flat.validate(), Ok(()));
    }

    #[test]
    fn validate_should_reject_every_bad_link_and_shared_sequence() {
        let base = groups();
        let invalid = |change: &dyn Fn(&mut TaskList)| {
            let mut list = base.clone();
            change(&mut list);
            matches!(list.validate(), Err(ListError::InvalidData(_)))
        };
        let link = |index: usize, parent: Option<u64>| {
            move |list: &mut TaskList| list.tasks[index].parent_id = parent.map(TaskId)
        };
        assert!(invalid(&link(1, Some(4))), "self link");
        assert!(invalid(&link(0, Some(2))), "parent later in order");
        assert!(invalid(&link(5, Some(99))), "unknown parent");
        assert!(
            invalid(&link(5, Some(1))),
            "subtask after another top-level task"
        );
        assert!(invalid(&link(2, Some(4))), "subtask of a subtask");
        assert!(invalid(
            &|list: &mut TaskList| list.schema_version = SCHEMA_VERSION
        ));
        assert!(invalid(&|list: &mut TaskList| list.schema_version = 3));
        assert!(
            invalid(&|list: &mut TaskList| {
                list.tasks[0].deletion_sequence = Some(1);
                list.next_deletion_sequence = 2;
            }),
            "live subtask under a deleted parent"
        );
        assert!(
            invalid(&|list: &mut TaskList| {
                list.tasks[0].deletion_sequence = Some(1);
                list.tasks[1].deletion_sequence = Some(2);
                list.tasks[2].deletion_sequence = Some(1);
                list.next_deletion_sequence = 3;
            }),
            "subtask deleted after its parent"
        );
        assert!(
            invalid(&|list: &mut TaskList| {
                list.tasks[4].deletion_sequence = Some(1);
                list.tasks[5].deletion_sequence = Some(1);
                list.next_deletion_sequence = 2;
            }),
            "duplicate sequences between unrelated tasks"
        );
        assert!(
            invalid(&|list: &mut TaskList| {
                list.tasks[1].deletion_sequence = Some(1);
                list.tasks[4].deletion_sequence = Some(1);
                list.next_deletion_sequence = 2;
            }),
            "duplicate sequences between subtasks of different live parents"
        );
    }

    #[test]
    fn add_subtask_should_insert_at_the_end_of_the_group_and_check_the_parent() {
        let mut list = groups();
        let added = list.add_subtask(TaskId(1), "  last of first  ").unwrap();
        assert_eq!(added, TaskId(7));
        assert_eq!(ids(&list), [1, 4, 5, 7, 2, 6, 3]);
        assert_eq!(list.task(added).unwrap().text(), "last of first");
        assert_eq!(list.task(added).unwrap().parent_id(), Some(TaskId(1)));
        // The group's deleted tail stays inside it, ahead of the insert.
        list.delete(TaskId(6)).unwrap();
        list.add_subtask(TaskId(2), "after the tombstone").unwrap();
        assert_eq!(ids(&list), [1, 4, 5, 7, 2, 6, 8, 3]);
        assert_eq!(list.validate(), Ok(()));
        let before = list.clone();
        assert_eq!(
            list.add_subtask(TaskId(1), " "),
            Err(ListError::InvalidText)
        );
        assert_eq!(
            list.add_subtask(TaskId(99), "x"),
            Err(ListError::TaskNotFound(TaskId(99)))
        );
        assert_eq!(
            list.add_subtask(TaskId(4), "x"),
            Err(ListError::InvalidParent(ParentError::ParentIsSubtask(
                TaskId(4)
            )))
        );
        list.delete(TaskId(3)).unwrap();
        let deleted = list.clone();
        assert_eq!(
            list.add_subtask(TaskId(3), "x"),
            Err(ListError::ParentDeleted(TaskId(3)))
        );
        assert_eq!(list, deleted);
        assert_ne!(before, deleted);
        // A valid parent with an exhausted ID space changes nothing.
        list.next_task_id = u64::MAX;
        let exhausted = list.clone();
        assert_eq!(
            list.add_subtask(TaskId(1), "x"),
            Err(ListError::TaskIdExhausted)
        );
        assert_eq!(list, exhausted);
    }

    #[test]
    fn set_parent_should_place_promote_and_report_no_ops() {
        let mut list = groups();
        // Nest a top-level task: it becomes the last subtask of the parent's group.
        assert_eq!(list.set_parent(TaskId(3), Some(TaskId(1))), Ok(true));
        assert_eq!(ids(&list), [1, 4, 5, 3, 2, 6]);
        assert_eq!(list.set_parent(TaskId(3), Some(TaskId(1))), Ok(false));
        // Move a subtask to another parent, which may come earlier or later.
        assert_eq!(list.set_parent(TaskId(4), Some(TaskId(2))), Ok(true));
        assert_eq!(ids(&list), [1, 5, 3, 2, 6, 4]);
        assert_eq!(list.set_parent(TaskId(6), Some(TaskId(1))), Ok(true));
        assert_eq!(ids(&list), [1, 5, 3, 6, 2, 4]);
        // Promote: right after the former parent's group.
        assert_eq!(list.set_parent(TaskId(5), None), Ok(true));
        assert_eq!(ids(&list), [1, 3, 6, 5, 2, 4]);
        assert_eq!(list.set_parent(TaskId(5), None), Ok(false));
        assert_eq!(
            parents(&list),
            [None, Some(1), Some(1), None, None, Some(2)]
        );
        assert_eq!(list.validate(), Ok(()));
    }

    #[test]
    fn set_parent_should_promote_a_nested_tasks_deleted_subtasks_in_place() {
        let mut list = groups();
        list.delete(TaskId(6)).unwrap();
        // 2's only subtask is in trash, so 2 can be nested, and 6 stays in trash at top level.
        assert_eq!(list.set_parent(TaskId(2), Some(TaskId(3))), Ok(true));
        assert_eq!(ids(&list), [1, 4, 5, 6, 3, 2]);
        assert_eq!(
            parents(&list),
            [None, Some(1), Some(1), None, None, Some(3)]
        );
        assert!(list.task(TaskId(6)).unwrap().is_deleted());
        assert_eq!(list.validate(), Ok(()));
        // The same when the parent is the group right before.
        let mut list = groups();
        list.delete(TaskId(6)).unwrap();
        assert_eq!(list.set_parent(TaskId(2), Some(TaskId(1))), Ok(true));
        assert_eq!(ids(&list), [1, 4, 5, 2, 6, 3]);
        assert_eq!(
            parents(&list),
            [None, Some(1), Some(1), Some(1), None, None]
        );
        assert_eq!(list.validate(), Ok(()));
    }

    #[test]
    fn set_parent_should_check_in_the_stated_order_without_changes() {
        let mut list = groups();
        list.delete(TaskId(3)).unwrap();
        let before = list.clone();
        let cases = [
            (
                TaskId(99),
                Some(TaskId(98)),
                ListError::TaskNotFound(TaskId(99)),
            ),
            (
                TaskId(3),
                Some(TaskId(99)),
                ListError::TaskDeleted(TaskId(3)),
            ),
            (TaskId(3), None, ListError::TaskDeleted(TaskId(3))),
            (
                TaskId(2),
                Some(TaskId(99)),
                ListError::TaskNotFound(TaskId(99)),
            ),
            (
                TaskId(2),
                Some(TaskId(3)),
                ListError::ParentDeleted(TaskId(3)),
            ),
            (
                TaskId(2),
                Some(TaskId(2)),
                ListError::InvalidParent(ParentError::SelfParent(TaskId(2))),
            ),
            (
                TaskId(4),
                Some(TaskId(4)),
                ListError::InvalidParent(ParentError::SelfParent(TaskId(4))),
            ),
            (
                TaskId(1),
                Some(TaskId(5)),
                ListError::InvalidParent(ParentError::ParentIsSubtask(TaskId(5))),
            ),
            (
                TaskId(1),
                Some(TaskId(2)),
                ListError::InvalidParent(ParentError::HasSubtasks(TaskId(1))),
            ),
        ];
        for (id, parent, error) in cases {
            assert_eq!(list.set_parent(id, parent), Err(error), "{id:?} {parent:?}");
            assert_eq!(list, before);
        }
    }

    #[test]
    fn delete_should_share_one_sequence_between_a_parent_and_its_live_subtasks_only() {
        let mut list = groups();
        list.delete(TaskId(4)).unwrap();
        list.delete(TaskId(1)).unwrap();
        assert_eq!(
            sequences(&list),
            [Some(2), Some(1), Some(2), None, None, None]
        );
        // Deleting a subtask on its own takes a fresh sequence.
        list.delete(TaskId(6)).unwrap();
        assert_eq!(list.task(TaskId(6)).unwrap().deletion_sequence(), Some(3));
        assert_eq!(list.next_deletion_sequence, 4);
        assert_eq!(list.validate(), Ok(()));
        // A deleted subtask cannot be deleted again, as for any tombstone.
        assert_eq!(
            list.delete(TaskId(5)),
            Err(ListError::TaskNotFound(TaskId(5)))
        );
        // An exhausted sequence space deletes neither the parent nor any of its subtasks.
        let mut exhausted = groups();
        exhausted.next_deletion_sequence = u64::MAX;
        let before = exhausted.clone();
        assert_eq!(
            exhausted.delete(TaskId(1)),
            Err(ListError::DeletionSequenceExhausted)
        );
        assert_eq!(exhausted, before);
    }

    #[test]
    fn restore_should_bring_back_exactly_the_group_in_place() {
        let mut list = groups();
        list.delete(TaskId(4)).unwrap();
        list.delete(TaskId(1)).unwrap();
        let mut expected = list.clone();
        expected.tasks[0].deletion_sequence = None;
        expected.tasks[2].deletion_sequence = None;
        assert_eq!(list.restore(TaskId(1)), Ok(true));
        assert_eq!(list, expected);
        assert!(list.task(TaskId(4)).unwrap().is_deleted());
        assert_eq!(list.restore(TaskId(1)), Ok(false));
        // A subtask whose parent is live restores in place.
        assert_eq!(list.restore(TaskId(4)), Ok(true));
        assert_eq!(ids(&list), [1, 4, 5, 2, 6, 3]);
        assert_eq!(list.visible_tasks().count(), 6);
    }

    #[test]
    fn restore_of_a_subtask_should_be_refused_while_its_parent_is_in_trash() {
        let mut list = groups();
        list.delete(TaskId(4)).unwrap();
        list.delete(TaskId(1)).unwrap();
        let before = list.clone();
        for subtask in [TaskId(4), TaskId(5)] {
            assert_eq!(
                list.restore(subtask),
                Err(ListError::ParentDeleted(TaskId(1)))
            );
            assert_eq!(list, before);
        }
        list.restore(TaskId(1)).unwrap();
        assert_eq!(list.restore(TaskId(4)), Ok(true));
    }

    #[test]
    fn restore_latest_should_restore_a_group_then_an_earlier_single_subtask() {
        let mut list = groups();
        list.delete(TaskId(4)).unwrap();
        list.delete(TaskId(1)).unwrap();
        assert_eq!(list.restore_latest(), Ok(Some(TaskId(1))));
        assert_eq!(sequences(&list), [None, Some(1), None, None, None, None]);
        assert_eq!(list.restore_latest(), Ok(Some(TaskId(4))));
        assert_eq!(list.restore_latest(), Ok(None));
        assert_eq!(list.visible_tasks().count(), 6);
    }

    #[test]
    fn deleted_tasks_should_keep_a_group_together_in_canonical_order() {
        let mut list = groups();
        list.delete(TaskId(3)).unwrap();
        list.delete(TaskId(1)).unwrap();
        list.delete(TaskId(6)).unwrap();
        assert_eq!(
            list.deleted_tasks()
                .iter()
                .map(|task| task.id().get())
                .collect::<Vec<_>>(),
            [6, 1, 4, 5, 3]
        );
    }

    #[test]
    fn swap_blocks_should_exchange_groups_and_keep_the_slots_between_them() {
        let mut list = groups();
        list.delete(TaskId(2)).unwrap();
        let mut tasks = list.tasks.clone();
        // Groups 1 [4, 5], 2 [6] (deleted), 3.
        swap_blocks(&mut tasks, 5, 0);
        let order = |tasks: &[Task]| tasks.iter().map(|task| task.id().get()).collect::<Vec<_>>();
        assert_eq!(order(&tasks), [3, 2, 6, 1, 4, 5]);
        swap_blocks(&mut tasks, 0, 3);
        assert_eq!(order(&tasks), [1, 4, 5, 2, 6, 3]);
        assert_eq!(tasks, list.tasks);
    }

    #[test]
    fn moves_should_stay_among_siblings_and_carry_groups() {
        let mut list = groups();
        assert_eq!(
            list.adjacent_visible(TaskId(1), MoveDirection::Down),
            Some(TaskId(2))
        );
        assert_eq!(
            list.adjacent_visible(TaskId(2), MoveDirection::Up),
            Some(TaskId(1))
        );
        assert_eq!(
            list.adjacent_visible(TaskId(4), MoveDirection::Down),
            Some(TaskId(5))
        );
        assert_eq!(list.adjacent_visible(TaskId(4), MoveDirection::Up), None);
        assert_eq!(list.adjacent_visible(TaskId(5), MoveDirection::Down), None);
        assert_eq!(list.adjacent_visible(TaskId(6), MoveDirection::Up), None);
        assert!(list.move_visible(TaskId(1), MoveDirection::Down).unwrap());
        assert_eq!(ids(&list), [2, 6, 1, 4, 5, 3]);
        assert!(list.move_visible(TaskId(5), MoveDirection::Up).unwrap());
        assert_eq!(ids(&list), [2, 6, 1, 5, 4, 3]);
        assert!(!list.move_visible(TaskId(5), MoveDirection::Up).unwrap());
        assert!(list.swap_visible(TaskId(3), TaskId(1)).unwrap());
        assert_eq!(ids(&list), [2, 6, 3, 1, 5, 4]);
        assert_eq!(list.validate(), Ok(()));
    }

    #[test]
    fn deleted_tasks_should_use_deletion_order_without_changing_canonical_order() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("same text").unwrap();
        let second = list.add("same text").unwrap();
        list.add("live").unwrap();
        list.delete(second).unwrap();
        list.delete(first).unwrap();
        let before = list.clone();

        assert_eq!(
            list.deleted_tasks()
                .iter()
                .map(|task| task.id())
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(list, before);
        list.restore(second).unwrap();
        list.delete(second).unwrap();
        assert_eq!(
            list.deleted_tasks()
                .iter()
                .map(|task| task.id())
                .collect::<Vec<_>>(),
            vec![second, first]
        );
    }

    #[test]
    fn restore_should_clear_only_the_selected_tombstone_marker() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("done task").unwrap();
        let third = list.add("third").unwrap();
        list.toggle_complete(second).unwrap();
        list.delete(second).unwrap();
        list.delete(first).unwrap();
        let mut expected = list.clone();
        expected.tasks[1].deletion_sequence = None;

        assert_eq!(list.restore(second), Ok(true));
        assert_eq!(list, expected);
        assert_eq!(
            list.visible_tasks()
                .map(|task| task.id())
                .collect::<Vec<_>>(),
            vec![second, third]
        );
        assert_eq!(list.restore_latest().unwrap(), Some(first));
    }

    #[test]
    fn restore_should_be_idempotent_for_live_tasks_and_reject_unknown_ids() {
        let mut list = TaskList::new(ListScope::Global);
        let live = list.add("live").unwrap();
        let before = list.clone();

        assert_eq!(list.restore(live), Ok(false));
        assert_eq!(
            list.restore(TaskId(999)),
            Err(ListError::TaskNotFound(TaskId(999)))
        );
        assert_eq!(list, before);
        assert!(list.deleted_tasks().is_empty());
    }

    #[test]
    fn task_id_should_accept_only_positive_shell_integers() {
        assert_eq!(TaskId::from_shell_integer(1), Some(TaskId(1)));
        assert_eq!(TaskId::from_shell_integer(u64::MAX), Some(TaskId(u64::MAX)));
        assert_eq!(TaskId::from_shell_integer(0), None);
    }

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
    fn set_completed_should_set_explicit_states_and_report_no_ops() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();

        assert!(!list.set_completed(first, false).unwrap());
        assert!(list.set_completed(first, true).unwrap());
        let completed = list.clone();
        assert!(!list.set_completed(first, true).unwrap());
        assert_eq!(list, completed);
        assert!(!list.task(second).unwrap().completed());
        assert!(list.set_completed(first, false).unwrap());
        assert!(!list.task(first).unwrap().completed());
    }

    #[test]
    fn edit_should_validate_trim_and_report_unchanged_canonical_text() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("original").unwrap();
        list.set_completed(id, true).unwrap();
        let before = list.clone();

        assert!(!list.edit(id, "  original\n").unwrap());
        assert_eq!(list, before);
        for text in ["", "   ", "one\ntwo", "one\rtwo"] {
            assert_eq!(list.edit(id, text), Err(ListError::InvalidText));
            assert_eq!(list, before);
        }
        assert!(list.edit(id, "  changed  ").unwrap());
        assert_eq!(list.task(id).unwrap().text(), "changed");
        assert!(list.task(id).unwrap().completed());
    }

    #[test]
    fn lifecycle_mutations_should_reject_deleted_and_unknown_ids_without_changes() {
        let mut list = TaskList::new(ListScope::Global);
        let deleted = list.add("deleted").unwrap();
        list.delete(deleted).unwrap();
        let before = list.clone();
        let unknown = TaskId(42);

        for completed in [true, false] {
            assert_eq!(
                list.set_completed(deleted, completed),
                Err(ListError::TaskDeleted(deleted))
            );
            assert_eq!(
                list.set_completed(unknown, completed),
                Err(ListError::TaskNotFound(unknown))
            );
        }
        assert_eq!(
            list.edit(deleted, "changed"),
            Err(ListError::TaskDeleted(deleted))
        );
        assert_eq!(
            list.edit(unknown, "changed"),
            Err(ListError::TaskNotFound(unknown))
        );
        assert_eq!(list.restore(unknown), Err(ListError::TaskNotFound(unknown)));
        assert_eq!(list, before);
    }

    #[test]
    fn restore_should_only_clear_the_selected_tombstone_in_canonical_position() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let deleted = list.add("deleted").unwrap();
        let third = list.add("third").unwrap();
        list.set_completed(deleted, true).unwrap();
        list.delete(deleted).unwrap();
        list.move_visible(first, MoveDirection::Down).unwrap();
        list.delete(third).unwrap();
        let mut expected = list.clone();
        expected.tasks[1].deletion_sequence = None;

        assert!(list.restore(deleted).unwrap());
        assert_eq!(list, expected);
        assert!(!list.restore(deleted).unwrap());
        assert_eq!(list, expected);
        assert_eq!(list.restore_latest().unwrap(), Some(third));
        assert_eq!(list.restore_latest().unwrap(), None);
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
