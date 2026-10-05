//! Shared transient projection for rendering and latest-snapshot reorder checks.
use crate::task::{MoveDirection, Task, TaskId, TaskList};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskView {
    All,
    Open,
    Done,
}

impl TaskView {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Open => "Open",
            Self::Done => "Done",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::All => Self::Open,
            Self::Open => Self::Done,
            Self::Done => Self::All,
        }
    }

    pub(crate) fn previous(self) -> Self {
        match self {
            Self::All => Self::Done,
            Self::Open => Self::All,
            Self::Done => Self::Open,
        }
    }

    fn matches(self, task: &Task) -> bool {
        match self {
            Self::All => true,
            Self::Open => !task.completed(),
            Self::Done => task.completed(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Projection {
    pub(crate) view: TaskView,
    pub(crate) lowercase_query: String,
}

impl Default for Projection {
    fn default() -> Self {
        Self {
            view: TaskView::All,
            lowercase_query: String::new(),
        }
    }
}

impl Projection {
    pub(crate) fn matches(&self, task: &Task) -> bool {
        matches(task, self.view, &self.lowercase_query)
    }

    pub(crate) fn adjacent(
        &self,
        list: &TaskList,
        id: TaskId,
        direction: MoveDirection,
    ) -> Option<TaskId> {
        adjacent(list, id, direction, self.view, &self.lowercase_query)
    }
}

pub(crate) fn matches(task: &Task, view: TaskView, lowercase_query: &str) -> bool {
    !task.is_deleted()
        && view.matches(task)
        && (lowercase_query.is_empty() || task.text().to_lowercase().contains(lowercase_query))
}

pub(crate) fn adjacent(
    list: &TaskList,
    id: TaskId,
    direction: MoveDirection,
    view: TaskView,
    lowercase_query: &str,
) -> Option<TaskId> {
    if view == TaskView::All && lowercase_query.is_empty() {
        return list.adjacent_visible(id, direction);
    }
    let mut previous = None;
    let mut found = false;
    for task in list
        .visible_tasks()
        .filter(|task| matches(task, view, lowercase_query))
    {
        if found {
            return Some(task.id());
        }
        if task.id() == id {
            match direction {
                MoveDirection::Up => return previous,
                MoveDirection::Down => found = true,
            }
        }
        previous = Some(task.id());
    }
    None
}
