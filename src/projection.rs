//! Shared transient projection for rendering and latest-snapshot reorder checks.
use crate::{
    metadata::{self, TaskFilter},
    task::{MoveDirection, Task, TaskId, TaskList, adjacent_sibling, block},
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

    /// Whether the task at canonical `index` is a displayed row: see `displays_with`.
    pub(crate) fn displays(&self, list: &TaskList, index: usize) -> bool {
        displays_with(list, index, |task| self.matches(task))
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
    matches_with(task, projection, |text, completed| {
        projection.filter.matches(text, completed)
    })
}

/// The shared predicate with `filter` in place of `projection.filter`, for the TUI filter preview.
pub(crate) fn matches_with(
    task: &Task,
    projection: &Projection,
    filter: impl Fn(&str, bool) -> bool,
) -> bool {
    !task.is_deleted()
        && projection.view.matches(task)
        && filter(task.text(), task.completed())
        && (projection.lowercase_query.is_empty()
            || task
                .text()
                .to_lowercase()
                .contains(&projection.lowercase_query))
}

/// True when the task at canonical `index` satisfies `matches`, or when it is a top-level task
/// with a live subtask in its group that does: a context row. Scans only that group, and only
/// for a top-level task that does not match.
pub(crate) fn displays_with(
    list: &TaskList,
    index: usize,
    matches: impl Fn(&Task) -> bool,
) -> bool {
    let tasks = list.tasks();
    let task = &tasks[index];
    matches(task)
        || (task.parent_id().is_none()
            && !task.is_deleted()
            && tasks[block(tasks, index)][1..]
                .iter()
                .any(|subtask| !subtask.is_deleted() && matches(subtask)))
}

/// Top-level siblings are the displayed top-level tasks, context rows included; subtask
/// siblings are the matching subtasks of the same parent.
pub(crate) fn adjacent(
    list: &TaskList,
    id: TaskId,
    direction: MoveDirection,
    projection: &Projection,
) -> Option<TaskId> {
    if projection.is_unfiltered() {
        return list.adjacent_visible(id, direction);
    }
    let rows = list
        .tasks()
        .iter()
        .enumerate()
        .filter(|(_, task)| !task.is_deleted())
        .map(|(index, task)| {
            let displayed = if task.parent_id().is_none() {
                projection.displays(list, index)
            } else {
                projection.matches(task)
            };
            (task, displayed)
        });
    adjacent_sibling(rows, id, direction)
}

/// A TUI row: a context row is a top-level task shown only because a subtask matches.
/// `progress` is done over total live subtasks, for a top-level task that has any.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DisplayedRow<'a> {
    pub(crate) task: &'a Task,
    pub(crate) context: bool,
    pub(crate) progress: Option<(usize, usize)>,
}

/// The TUI's displayed rows over live tasks in tree order (canonical, or `sort_tree`'s), so
/// each subtask follows its parent's row. A top-level task shows when it matches or when one of
/// its live subtasks does; progress counts every live subtask, matching or not. Holds the
/// matching subtasks of one group at a time.
pub(crate) fn displayed_rows<'a>(
    tasks: impl Iterator<Item = &'a Task>,
    matches: impl Fn(&Task) -> bool,
) -> impl Iterator<Item = DisplayedRow<'a>> {
    let mut tasks = tasks.peekable();
    let mut pending = std::collections::VecDeque::new();
    std::iter::from_fn(move || {
        loop {
            if let Some(row) = pending.pop_front() {
                return Some(row);
            }
            let task = tasks.next()?;
            let row = |task, context, progress| DisplayedRow {
                task,
                context,
                progress,
            };
            if task.parent_id().is_some() {
                // Only a stream that is not in tree order has a subtask outside its group.
                if matches(task) {
                    return Some(row(task, false, None));
                }
                continue;
            }
            let (mut done, mut total) = (0, 0);
            while let Some(subtask) = tasks.next_if(|next| next.parent_id() == Some(task.id())) {
                total += 1;
                done += usize::from(subtask.completed());
                if matches(subtask) {
                    pending.push_back(row(subtask, false, None));
                }
            }
            let matched = matches(task);
            if matched || !pending.is_empty() {
                let progress = (total > 0).then_some((done, total));
                return Some(row(task, !matched, progress));
            }
        }
    })
}

/// Display order for a projection. Storage and reorder guards never need it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TaskOrder {
    #[default]
    Manual,
    Priority,
    Due,
}

/// Sorts rows that are in canonical order as a tree. A group is a root followed by those of
/// its subtasks among the rows; a subtask whose parent is not among the rows is a group of its
/// own. Groups sort by their first row's key and subtasks sort within their group; the sort is
/// stable, so equal keys keep canonical order. Each row's key is computed once.
pub(crate) fn sort_tree<K: Clone + Ord>(rows: &mut [&Task], key: impl Fn(&Task) -> K) {
    sort_tree_by(rows, |task| task, key);
}

/// `sort_tree` over rows that carry a task, such as the TUI's canonical indices.
pub(crate) fn sort_tree_by<'a, T: Copy, K: Clone + Ord>(
    rows: &mut [T],
    task_of: impl Fn(T) -> &'a Task,
    key: impl Fn(&Task) -> K,
) {
    let mut head: Option<(TaskId, usize, K)> = None;
    let mut keyed = Vec::with_capacity(rows.len());
    for (position, row) in rows.iter().enumerate() {
        let task = task_of(*row);
        let own = key(task);
        let sort_key = match (task.parent_id(), &head) {
            (Some(parent), Some((id, group, group_key))) if *id == parent => {
                (group_key.clone(), *group, Some(own))
            }
            _ => {
                head = Some((task.id(), position, own.clone()));
                (own, position, None)
            }
        };
        keyed.push((sort_key, position));
    }
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    let sorted = keyed
        .iter()
        .map(|(_, position)| rows[*position])
        .collect::<Vec<_>>();
    rows.copy_from_slice(&sorted);
}

/// A `list` row; `indented` only when its parent was printed just above its group.
pub(crate) struct ListRow<'a> {
    pub(crate) task: &'a Task,
    pub(crate) indented: bool,
}

/// Streams matching tasks in tree order with O(1) state: canonical order, or `sort_tree`'s.
pub(crate) fn list_rows<'a>(
    tasks: impl Iterator<Item = &'a Task>,
    matches: impl Fn(&Task) -> bool,
) -> impl Iterator<Item = ListRow<'a>> {
    let mut printed_root = None;
    tasks.filter_map(move |task| {
        let matched = matches(task);
        match task.parent_id() {
            None => {
                printed_root = matched.then_some(task.id());
                matched.then_some(ListRow {
                    task,
                    indented: false,
                })
            }
            Some(parent) => matched.then_some(ListRow {
                task,
                indented: printed_root == Some(parent),
            }),
        }
    })
}

/// Priority order's sort key: `A` to `Z`, then no priority.
pub(crate) fn priority_key(task: &Task) -> (bool, Option<metadata::Priority>) {
    let priority = metadata::priority(task.text());
    (priority.is_none(), priority)
}

/// Stable: `A` to `Z`, then no priority; equal keys keep canonical order. Sorted as a tree.
pub(crate) fn sort_by_priority(tasks: &mut [&Task]) {
    sort_tree(tasks, priority_key);
}

type DueKey = (
    bool,
    Option<metadata::Date>,
    (bool, Option<metadata::Priority>),
);

/// Due order's sort key: earliest date first, then no due date; priority breaks date ties.
pub(crate) fn due_key(task: &Task) -> DueKey {
    let due = metadata::due(task.text());
    (due.is_none(), due, priority_key(task))
}

/// Stable, and parses each task once: equal keys keep canonical order. Sorted as a tree.
pub(crate) fn sort_by_due(tasks: &mut [&Task]) {
    sort_tree(tasks, due_key);
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

    #[test]
    fn adjacent_should_move_only_among_siblings_filtered_and_unfiltered() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first +x").unwrap();
        let second = list.add("second").unwrap();
        let third = list.add("third +x").unwrap();
        let child_a = list.add_subtask(first, "child a +x").unwrap();
        let child_b = list.add_subtask(first, "child b").unwrap();
        let child_c = list.add_subtask(first, "child c +x").unwrap();
        let other = list.add_subtask(second, "other child +x").unwrap();
        // Canonical order: first [a, b, c], second [other], third.
        let unfiltered = Projection::default();
        let tagged = Projection {
            filter: filter(&["+x"]),
            ..Projection::default()
        };
        let cases = [
            (&unfiltered, first, MoveDirection::Down, Some(second)),
            (&unfiltered, second, MoveDirection::Up, Some(first)),
            (&unfiltered, child_a, MoveDirection::Down, Some(child_b)),
            (&unfiltered, child_a, MoveDirection::Up, None),
            (&unfiltered, child_c, MoveDirection::Down, None),
            (&unfiltered, other, MoveDirection::Up, None),
            (&unfiltered, other, MoveDirection::Down, None),
            // `second` does not match, but its subtask does, so it is a context row and a
            // displayed top-level sibling.
            (&tagged, first, MoveDirection::Down, Some(second)),
            (&tagged, second, MoveDirection::Up, Some(first)),
            (&tagged, second, MoveDirection::Down, Some(third)),
            (&tagged, third, MoveDirection::Up, Some(second)),
            (&tagged, child_a, MoveDirection::Down, Some(child_c)),
            (&tagged, child_c, MoveDirection::Up, Some(child_a)),
            (&tagged, child_c, MoveDirection::Down, None),
            (&tagged, other, MoveDirection::Up, None),
            (&tagged, child_b, MoveDirection::Down, None),
        ];
        for (projection, id, direction, expected) in cases {
            assert_eq!(
                projection.adjacent(&list, id, direction),
                expected,
                "{id:?} {direction:?}"
            );
        }
    }

    #[test]
    fn due_filters_should_see_completion_and_count_as_filtering() {
        let mut list = TaskList::new(ListScope::Global);
        let late = list.add("late due:2026-10-20").unwrap();
        let done_late = list.add("done late due:2026-10-19").unwrap();
        let future = list.add("future due:2026-10-30").unwrap();
        let late_again = list.add("late again due:2026-10-01.").unwrap();
        list.set_completed(done_late, true).unwrap();
        let mut overdue = TaskFilter::default();
        overdue
            .due
            .add(metadata::parse_due_term("overdue").unwrap());
        overdue
            .due
            .resolve(|| metadata::Date::new(2026, 10, 21).unwrap());
        let projection = Projection {
            filter: overdue,
            ..Projection::default()
        };
        assert!(!projection.is_unfiltered());
        assert_eq!(
            ids(list.tasks().iter().filter(|task| projection.matches(task))),
            [late.get(), late_again.get()]
        );
        assert_eq!(
            projection.adjacent(&list, late, MoveDirection::Down),
            Some(late_again)
        );
        assert_eq!(
            projection.adjacent(&list, late_again, MoveDirection::Up),
            Some(late)
        );
        assert_eq!(
            Projection::default().adjacent(&list, late, MoveDirection::Down),
            Some(done_late)
        );
        assert_ne!(future, late);
    }

    /// Tasks with `(id, parent, text)`, added in canonical order by `add` and `add_subtask`.
    fn tree(rows: &[(u64, Option<u64>, &str)]) -> TaskList {
        let mut list = TaskList::new(ListScope::Global);
        for (id, parent, text) in rows {
            let added = match parent {
                None => list.add(text),
                Some(parent) => {
                    list.add_subtask(TaskId::from_shell_integer(*parent).unwrap(), text)
                }
            }
            .unwrap();
            assert_eq!(added.get(), *id);
        }
        list
    }

    fn sorted_tree(tasks: &[&Task]) -> Vec<u64> {
        let mut rows = tasks.to_vec();
        sort_tree(&mut rows, priority_key);
        ids(rows.into_iter())
    }

    #[test]
    fn sort_tree_should_sort_groups_by_head_then_subtasks_within_with_canonical_ties() {
        let list = tree(&[
            (1, None, "(C) one"),
            (2, Some(1), "(B) one b"),
            (3, Some(1), "(A) one a"),
            (4, None, "(A) four"),
            (5, Some(4), "plain"),
            (6, Some(4), "plain too"),
            (7, None, "(C) seven"),
            (8, None, "(B) eight, filtered out"),
            (9, Some(8), "(A) nine"),
            (10, Some(8), "(C) ten"),
            (11, None, "(B) eleven"),
        ]);
        let live: Vec<&Task> = list.visible_tasks().collect();
        assert_eq!(sorted_tree(&live), [4, 5, 6, 8, 9, 10, 11, 1, 3, 2, 7]);
        // 9 and 10 lost their parent to a filter: each heads its own group, so 11 sorts between.
        let orphans: Vec<&Task> = list
            .visible_tasks()
            .filter(|task| task.id().get() >= 9)
            .collect();
        assert_eq!(sorted_tree(&orphans), [9, 11, 10]);
        let with_root: Vec<&Task> = live
            .iter()
            .copied()
            .filter(|task| task.id().get() != 8)
            .collect();
        assert_eq!(sorted_tree(&with_root), [4, 5, 6, 9, 11, 1, 3, 2, 7, 10]);
        assert!(sorted_tree(&[]).is_empty());
        // A list without subtasks sorts exactly as the flat stable sort did.
        let flat = tree(&[
            (1, None, "plain"),
            (2, None, "(B) b"),
            (3, None, "(A) a"),
            (4, None, "(B) b2"),
        ]);
        let mut expected: Vec<&Task> = flat.visible_tasks().collect();
        expected.sort_by_key(|task| priority_key(task));
        let rows: Vec<&Task> = flat.visible_tasks().collect();
        assert_eq!(sorted_tree(&rows), ids(expected.into_iter()));
        let mut due_rows: Vec<&Task> = list.visible_tasks().collect();
        sort_by_due(&mut due_rows);
        assert_eq!(due_rows.len(), 11);
    }

    #[test]
    fn list_rows_should_indent_only_under_the_printed_parent() {
        let list = tree(&[
            (1, None, "one"),
            (2, Some(1), "one a"),
            (3, None, "three"),
            (4, Some(3), "three a"),
            (5, None, "five"),
        ]);
        let rows = |keep: &[u64]| {
            list_rows(list.visible_tasks(), |task| keep.contains(&task.id().get()))
                .map(|row| (row.task.id().get(), row.indented))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&[1, 2, 3, 4, 5]),
            [(1, false), (2, true), (3, false), (4, true), (5, false)]
        );
        // 4's parent is filtered out, so 4 is not drawn inside 1's group.
        assert_eq!(rows(&[1, 2, 4]), [(1, false), (2, true), (4, false)]);
        assert_eq!(rows(&[2, 4]), [(2, false), (4, false)]);
        assert!(rows(&[]).is_empty());
    }

    #[test]
    fn sort_by_due_should_order_dates_then_undated_with_priority_ties() {
        let mut list = TaskList::new(ListScope::Global);
        for text in [
            "undated first",
            "(B) b due:2026-11-01",
            "late due:2026-10-01",
            "(A) a due:2026-11-01",
            "plain due:2026-11-01",
            "(A) undated a",
            "(B) second b due:2026-11-01.",
            "invalid due:2026-02-30",
            "first token wins due:2026-12-01 due:2026-01-01",
        ] {
            list.add(text).unwrap();
        }
        let sorted = |list: &TaskList| {
            let mut tasks: Vec<&Task> = list.visible_tasks().collect();
            sort_by_due(&mut tasks);
            ids(tasks.into_iter())
        };

        assert_eq!(sorted(&list), [3, 4, 2, 7, 5, 9, 6, 1, 8]);
        let mut ordered = TaskList::new(ListScope::Global);
        for text in ["x due:2026-01-01", "(A) y due:2026-01-02", "(A) z", "w"] {
            ordered.add(text).unwrap();
        }
        assert_eq!(sorted(&ordered), [1, 2, 3, 4]);
        sort_by_due(&mut []);
        let task = list.visible_tasks().next().unwrap();
        assert_eq!(due_key(task), (true, None, (true, None)));
    }

    #[test]
    fn sort_by_priority_should_be_stable_with_unprioritized_rows_last() {
        let mut list = TaskList::new(ListScope::Global);
        for text in [
            "plain first",
            "(B) b first",
            "(a) lowercase is plain",
            "(A) a first",
            "(A)",
            "(B) b second",
            "(A) a second",
            "plain last",
        ] {
            list.add(text).unwrap();
        }
        let sorted = |list: &TaskList| {
            let mut tasks: Vec<&Task> = list.visible_tasks().collect();
            sort_by_priority(&mut tasks);
            ids(tasks.into_iter())
        };

        assert_eq!(sorted(&list), [4, 7, 2, 6, 1, 3, 5, 8]);
        let mut ordered = TaskList::new(ListScope::Global);
        for text in ["(A) x", "(C) y", "z"] {
            ordered.add(text).unwrap();
        }
        assert_eq!(sorted(&ordered), [1, 2, 3]);
        sort_by_priority(&mut []);
        assert_eq!(TaskOrder::default(), TaskOrder::Manual);
    }

    /// Group 1 [2 matches, 3], group 4 [5], 6 matches, and group 7 [8 matches, done].
    fn context_tree() -> TaskList {
        let mut list = tree(&[
            (1, None, "alpha"),
            (2, Some(1), "beta match"),
            (3, Some(1), "gamma"),
            (4, None, "delta"),
            (5, Some(4), "epsilon"),
            (6, None, "zeta match"),
            (7, None, "eta"),
            (8, Some(7), "theta match"),
        ]);
        list.set_completed(TaskId::from_shell_integer(3).unwrap(), true)
            .unwrap();
        list.set_completed(TaskId::from_shell_integer(8).unwrap(), true)
            .unwrap();
        list
    }

    fn search(query: &str) -> Projection {
        Projection {
            lowercase_query: query.into(),
            ..Projection::default()
        }
    }

    #[test]
    fn displays_should_show_matches_and_top_level_tasks_with_a_matching_live_subtask() {
        let mut list = context_tree();
        let projection = search("match");
        let shown = |list: &TaskList| {
            (0..list.tasks().len())
                .filter(|index| projection.displays(list, *index))
                .map(|index| list.tasks()[index].id().get())
                .collect::<Vec<_>>()
        };
        assert_eq!(shown(&list), [1, 2, 6, 7, 8]);
        // `displays_with` over the same predicate gives the same answers.
        for index in 0..list.tasks().len() {
            assert_eq!(
                displays_with(&list, index, |task| projection.matches(task)),
                projection.displays(&list, index),
                "{index}"
            );
        }
        assert_eq!(
            (0..list.tasks().len())
                .filter(|index| displays_with(&list, *index, |task| task.text() == "epsilon"))
                .count(),
            2
        );
        // A matching subtask in trash does not make its parent a context row.
        list.delete(TaskId::from_shell_integer(2).unwrap()).unwrap();
        assert_eq!(shown(&list), [6, 7, 8]);
    }

    #[test]
    fn adjacent_should_treat_context_parents_as_displayed_top_level_siblings() {
        let list = context_tree();
        let projection = search("match");
        let id = |value| TaskId::from_shell_integer(value).unwrap();
        let next = |task, direction| {
            projection
                .adjacent(&list, id(task), direction)
                .map(TaskId::get)
        };
        assert_eq!(next(1, MoveDirection::Down), Some(6));
        assert_eq!(next(6, MoveDirection::Up), Some(1));
        assert_eq!(next(6, MoveDirection::Down), Some(7));
        assert_eq!(next(7, MoveDirection::Up), Some(6));
        // 4 is not displayed, so it has no neighbor and is skipped.
        assert_eq!(next(4, MoveDirection::Down), None);
        // A subtask's siblings still have to match: 3 is not one of 2's.
        assert_eq!(next(2, MoveDirection::Down), None);
        assert_eq!(next(2, MoveDirection::Up), None);
    }

    #[test]
    fn displayed_rows_should_add_context_parents_and_count_every_live_subtask() {
        let mut list = context_tree();
        let rows = |list: &TaskList, matches: &dyn Fn(&Task) -> bool| {
            displayed_rows(list.visible_tasks(), matches)
                .map(|row| (row.task.id().get(), row.context, row.progress))
                .collect::<Vec<_>>()
        };
        let projection = search("match");
        assert_eq!(
            rows(&list, &|task| projection.matches(task)),
            [
                (1, true, Some((1, 2))),
                (2, false, None),
                (6, false, None),
                (7, true, Some((1, 1))),
                (8, false, None),
            ]
        );
        assert_eq!(
            rows(&list, &|_| true),
            [
                (1, false, Some((1, 2))),
                (2, false, None),
                (3, false, None),
                (4, false, Some((0, 1))),
                (5, false, None),
                (6, false, None),
                (7, false, Some((1, 1))),
                (8, false, None),
            ]
        );
        // Progress counts live subtasks only.
        list.delete(TaskId::from_shell_integer(3).unwrap()).unwrap();
        assert_eq!(
            rows(&list, &|task| task.id().get() <= 2),
            [(1, false, Some((0, 1))), (2, false, None)]
        );
        // Sorted rows keep groups together, so the pass reads them the same way.
        let mut sorted = list.visible_tasks().collect::<Vec<_>>();
        sort_tree(&mut sorted, |task| std::cmp::Reverse(task.id().get()));
        assert_eq!(
            displayed_rows(sorted.into_iter(), |task| projection.matches(task))
                .map(|row| (row.task.id().get(), row.context))
                .collect::<Vec<_>>(),
            [(7, true), (8, false), (6, false), (1, true), (2, false)]
        );
    }
}
