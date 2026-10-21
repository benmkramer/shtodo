//! Shared transient projection for rendering and latest-snapshot reorder checks.
use crate::{
    metadata::TaskFilter,
    task::{MoveDirection, Task, TaskId, TaskList},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TaskView {
    #[default]
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Projection {
    pub(crate) view: TaskView,
    pub(crate) lowercase_query: String,
    pub(crate) filter: TaskFilter,
}

impl Projection {
    pub(crate) fn matches(&self, task: &Task) -> bool {
        matches(task, self)
    }

    pub(crate) fn is_unfiltered(&self) -> bool {
        self.view == TaskView::All && self.lowercase_query.is_empty() && self.filter.is_empty()
    }

    pub(crate) fn adjacent(
        &self,
        list: &TaskList,
        id: TaskId,
        direction: MoveDirection,
    ) -> Option<TaskId> {
        adjacent(list, id, direction, self)
    }
}

/// Checks run cheapest first: tombstone, view, filter, then the lowercase search.
pub(crate) fn matches(task: &Task, projection: &Projection) -> bool {
    !task.is_deleted()
        && projection.view.matches(task)
        && projection.filter.matches(task.text())
        && (projection.lowercase_query.is_empty()
            || task
                .text()
                .to_lowercase()
                .contains(&projection.lowercase_query))
}

pub(crate) fn adjacent(
    list: &TaskList,
    id: TaskId,
    direction: MoveDirection,
    projection: &Projection,
) -> Option<TaskId> {
    if projection.is_unfiltered() {
        return list.adjacent_visible(id, direction);
    }
    let mut previous = None;
    let mut found = false;
    for task in list
        .visible_tasks()
        .filter(|task| matches(task, projection))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        metadata::{TaskFilter, parse_filter_token},
        task::ListScope,
    };

    fn filter(tokens: &[&str]) -> TaskFilter {
        let mut filter = TaskFilter::default();
        for token in tokens {
            filter.add(parse_filter_token(token).unwrap());
        }
        filter
    }

    fn ids<'a>(tasks: impl Iterator<Item = &'a Task>) -> Vec<u64> {
        tasks.map(|task| task.id().get()).collect()
    }

    #[test]
    fn matches_should_compose_tombstones_view_filter_and_search() {
        let mut list = TaskList::new(ListScope::Global);
        let tagged = list.add("(A) Ship it +shtodo.").unwrap();
        let done = list.add("(B) Release +shtodo @laptop").unwrap();
        list.add("Plain SHIP task").unwrap();
        let deleted = list.add("(A) gone +shtodo").unwrap();
        list.set_completed(done, true).unwrap();
        list.delete(deleted).unwrap();
        let visible = |projection: &Projection| {
            ids(list.tasks().iter().filter(|task| matches(task, projection)))
        };

        assert_eq!(visible(&Projection::default()), [1, 2, 3]);
        let tag = Projection {
            filter: filter(&["+shtodo"]),
            ..Projection::default()
        };
        assert_eq!(visible(&tag), [1, 2]);
        let open_tag = Projection {
            view: TaskView::Open,
            ..tag.clone()
        };
        assert_eq!(visible(&open_tag), [tagged.get()]);
        let priority_search = Projection {
            lowercase_query: "ship".into(),
            filter: filter(&["a-b", "none"]),
            ..Projection::default()
        };
        assert_eq!(visible(&priority_search), [1, 3]);
        let context_search = Projection {
            view: TaskView::Done,
            lowercase_query: "release".into(),
            filter: filter(&["@laptop", "B"]),
        };
        assert_eq!(visible(&context_search), [2]);
        assert!(Projection::default().is_unfiltered());
        assert!(!tag.is_unfiltered());
    }

    #[test]
    fn adjacent_should_skip_rows_outside_the_filter() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first +x").unwrap();
        let hidden = list.add("hidden").unwrap();
        let deleted = list.add("deleted +x").unwrap();
        let second = list.add("second +x").unwrap();
        list.delete(deleted).unwrap();
        let projection = Projection {
            filter: filter(&["+x"]),
            ..Projection::default()
        };

        assert_eq!(
            projection.adjacent(&list, first, MoveDirection::Down),
            Some(second)
        );
        assert_eq!(
            projection.adjacent(&list, second, MoveDirection::Up),
            Some(first)
        );
        assert_eq!(
            projection.adjacent(&list, second, MoveDirection::Down),
            None
        );
        assert_eq!(
            Projection::default().adjacent(&list, first, MoveDirection::Down),
            Some(hidden)
        );
    }
}
