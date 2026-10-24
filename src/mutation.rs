//! ID-based intents evaluated only against the transaction's latest list.

use crate::{
    projection::Projection,
    task::{ListError, MoveDirection, Task, TaskId, TaskList, validated_text},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TaskObservation(Task);

impl TaskObservation {
    pub(crate) fn new(task: &Task) -> Self {
        Self(task.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Mutation {
    Add(String),
    AddSubtask {
        parent: TaskId,
        text: String,
    },
    /// Nests `id` under `parent`, or promotes it with `None`; both IDs are named, so no
    /// adjacency guard is needed.
    SetParent {
        id: TaskId,
        parent: Option<TaskId>,
    },
    SetCompleted {
        id: TaskId,
        completed: bool,
    },
    EditText {
        id: TaskId,
        text: String,
        original: Option<String>,
    },
    Delete {
        id: TaskId,
        observed: Option<TaskObservation>,
    },
    RestoreLatest,
    Restore {
        id: TaskId,
        observed: Option<TaskObservation>,
    },
    MoveAdjacent {
        id: TaskId,
        direction: MoveDirection,
        neighbor: Option<TaskId>,
        projection: Projection,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConflictKind {
    TextChanged,
    TaskChanged,
    OrderChanged,
    NoLongerActive,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Changed { id: TaskId, celebrate: bool },
    AlreadyInState { id: TaskId },
    NothingToRestore,
    Conflict { id: TaskId, kind: ConflictKind },
    Rejected(ListError),
}

impl Outcome {
    pub(crate) fn changed(&self) -> bool {
        matches!(self, Self::Changed { .. })
    }
}

impl Mutation {
    pub(crate) fn validate(&self) -> Result<(), ListError> {
        match self {
            Self::Add(text) | Self::AddSubtask { text, .. } | Self::EditText { text, .. } => {
                validated_text(text)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn apply(&self, list: &mut TaskList) -> Outcome {
        self.apply_checked(list).unwrap_or_else(Outcome::Rejected)
    }

    fn apply_checked(&self, list: &mut TaskList) -> Result<Outcome, ListError> {
        self.validate()?;
        let changed = |id| Outcome::Changed {
            id,
            celebrate: false,
        };
        match self {
            Self::Add(text) => Ok(changed(list.add(text)?)),
            Self::AddSubtask { parent, text } => Ok(changed(list.add_subtask(*parent, text)?)),
            Self::SetParent { id, parent } => Ok(if list.set_parent(*id, *parent)? {
                changed(*id)
            } else {
                Outcome::AlreadyInState { id: *id }
            }),
            Self::RestoreLatest => Ok(match list.restore_latest()? {
                Some(id) => changed(id),
                None => Outcome::NothingToRestore,
            }),
            Self::Restore { id, observed } => {
                let task = list.task(*id).ok_or(ListError::TaskNotFound(*id))?;
                if !task.is_deleted() {
                    return Ok(Outcome::AlreadyInState { id: *id });
                }
                if observed
                    .as_ref()
                    .is_some_and(|seen| seen.0.deletion_sequence() != task.deletion_sequence())
                {
                    return Ok(Outcome::Conflict {
                        id: *id,
                        kind: ConflictKind::TaskChanged,
                    });
                }
                list.restore(*id)?;
                Ok(changed(*id))
            }
            Self::Delete { id, observed } => {
                let task = list.task(*id).ok_or(ListError::TaskNotFound(*id))?;
                if task.is_deleted() {
                    return Ok(Outcome::AlreadyInState { id: *id });
                }
                if observed.as_ref().is_some_and(|seen| seen.0 != *task) {
                    return Ok(Outcome::Conflict {
                        id: *id,
                        kind: ConflictKind::TaskChanged,
                    });
                }
                list.delete(*id)?;
                Ok(changed(*id))
            }
            Self::SetCompleted { id, completed } => {
                let task = list.task(*id).ok_or(ListError::TaskNotFound(*id))?;
                if task.is_deleted() {
                    return Ok(Outcome::Conflict {
                        id: *id,
                        kind: ConflictKind::NoLongerActive,
                    });
                }
                if task.completed() == *completed {
                    return Ok(Outcome::AlreadyInState { id: *id });
                }
                let had_open = list.visible_tasks().any(|task| !task.completed());
                list.set_completed(*id, *completed)?;
                let celebrate =
                    *completed && had_open && !list.visible_tasks().any(|task| !task.completed());
                Ok(Outcome::Changed { id: *id, celebrate })
            }
            Self::EditText { id, text, original } => {
                let task = list.task(*id).ok_or(ListError::TaskNotFound(*id))?;
                if task.is_deleted() {
                    return Ok(Outcome::Conflict {
                        id: *id,
                        kind: ConflictKind::NoLongerActive,
                    });
                }
                let text = validated_text(text)?;
                if task.text() == text {
                    return Ok(Outcome::AlreadyInState { id: *id });
                }
                if original.as_ref().is_some_and(|seen| seen != task.text()) {
                    return Ok(Outcome::Conflict {
                        id: *id,
                        kind: ConflictKind::TextChanged,
                    });
                }
                list.edit(*id, text)?;
                Ok(changed(*id))
            }
            Self::MoveAdjacent {
                id,
                direction,
                neighbor,
                projection,
            } => {
                let task = list.task(*id).ok_or(ListError::TaskNotFound(*id))?;
                if task.is_deleted() {
                    return Ok(Outcome::Conflict {
                        id: *id,
                        kind: ConflictKind::NoLongerActive,
                    });
                }
                if !projection.matches(task)
                    || projection.adjacent(list, *id, *direction) != *neighbor
                {
                    return Ok(Outcome::Conflict {
                        id: *id,
                        kind: ConflictKind::OrderChanged,
                    });
                }
                let moved = if projection.is_unfiltered() {
                    list.move_visible(*id, *direction)?
                } else if let Some(neighbor) = neighbor {
                    list.swap_visible(*id, *neighbor)?
                } else {
                    false
                };
                if moved {
                    Ok(changed(*id))
                } else {
                    Ok(Outcome::AlreadyInState { id: *id })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{ListScope, ParentError};

    fn id(value: u64) -> TaskId {
        TaskId::from_shell_integer(value).unwrap()
    }

    fn order(list: &TaskList) -> Vec<u64> {
        list.tasks().iter().map(|task| task.id().get()).collect()
    }

    /// Groups 1 [4, 5], 2 [6], and 3.
    fn groups() -> TaskList {
        let mut list = TaskList::new(ListScope::Global);
        for text in ["first", "second", "third"] {
            list.add(text).unwrap();
        }
        for (parent, text) in [(1, "first a"), (1, "first b"), (2, "second a")] {
            list.add_subtask(id(parent), text).unwrap();
        }
        list
    }

    /// Validates as storage does, after stamping the schema version.
    fn valid(list: &TaskList) -> bool {
        let mut list = list.clone();
        list.stamp_schema_version();
        list.validate().is_ok()
    }

    fn move_adjacent(list: &TaskList, task: u64, direction: MoveDirection) -> Mutation {
        let projection = Projection::default();
        Mutation::MoveAdjacent {
            id: id(task),
            direction,
            neighbor: projection.adjacent(list, id(task), direction),
            projection,
        }
    }

    #[test]
    fn subtask_intents_should_report_changes_no_ops_and_rejections() {
        let mut list = groups();
        assert_eq!(
            Mutation::AddSubtask {
                parent: id(2),
                text: " second b ".into()
            }
            .apply(&mut list),
            Outcome::Changed {
                id: id(7),
                celebrate: false
            }
        );
        assert_eq!(order(&list), [1, 4, 5, 2, 6, 7, 3]);
        let nest = Mutation::SetParent {
            id: id(3),
            parent: Some(id(1)),
        };
        assert!(nest.apply(&mut list).changed());
        assert_eq!(nest.apply(&mut list), Outcome::AlreadyInState { id: id(3) });
        let promote = Mutation::SetParent {
            id: id(3),
            parent: None,
        };
        assert!(promote.apply(&mut list).changed());
        assert_eq!(
            promote.apply(&mut list),
            Outcome::AlreadyInState { id: id(3) }
        );
        assert_eq!(order(&list), [1, 4, 5, 3, 2, 6, 7]);
        let before = list.clone();
        assert_eq!(
            Mutation::AddSubtask {
                parent: id(1),
                text: "\n".into()
            }
            .apply(&mut list),
            Outcome::Rejected(ListError::InvalidText)
        );
        assert_eq!(
            Mutation::SetParent {
                id: id(1),
                parent: Some(id(2))
            }
            .apply(&mut list),
            Outcome::Rejected(ListError::InvalidParent(ParentError::HasSubtasks(id(1))))
        );
        assert_eq!(list, before);
        assert!(
            Mutation::AddSubtask {
                parent: id(1),
                text: " ".into()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn moving_a_top_level_task_should_swap_groups_around_a_deleted_group() {
        let mut list = groups();
        list.delete(id(2)).unwrap();
        // 1 [4, 5], then the deleted 2 [6], then 3.
        let request = move_adjacent(&list, 1, MoveDirection::Down);
        assert!(request.apply(&mut list).changed());
        assert_eq!(order(&list), [3, 2, 6, 1, 4, 5]);
        assert!(valid(&list));
        let request = move_adjacent(&list, 1, MoveDirection::Down);
        assert_eq!(
            request.apply(&mut list),
            Outcome::AlreadyInState { id: id(1) }
        );
        assert!(
            move_adjacent(&list, 1, MoveDirection::Up)
                .apply(&mut list)
                .changed()
        );
        assert_eq!(order(&list), [1, 4, 5, 2, 6, 3]);
    }

    #[test]
    fn moving_a_subtask_should_stay_in_its_group_and_stop_at_the_edges() {
        let mut list = groups();
        assert!(
            move_adjacent(&list, 4, MoveDirection::Down)
                .apply(&mut list)
                .changed()
        );
        assert_eq!(order(&list), [1, 5, 4, 2, 6, 3]);
        for (task, direction) in [
            (4, MoveDirection::Down),
            (5, MoveDirection::Up),
            (6, MoveDirection::Up),
            (6, MoveDirection::Down),
        ] {
            let before = list.clone();
            let request = move_adjacent(&list, task, direction);
            assert_eq!(
                request.apply(&mut list),
                Outcome::AlreadyInState { id: id(task) }
            );
            assert_eq!(list, before);
        }
        // Filtered: a subtask's siblings are the matching subtasks of its parent.
        let mut filter = crate::metadata::TaskFilter::default();
        filter.add(crate::metadata::parse_filter_token("+x").unwrap());
        let projection = Projection {
            filter,
            ..Projection::default()
        };
        let mut list = groups();
        list.edit(id(4), "first a +x").unwrap();
        list.edit(id(6), "second a +x").unwrap();
        list.edit(id(3), "third +x").unwrap();
        assert_eq!(projection.adjacent(&list, id(4), MoveDirection::Down), None);
        assert_eq!(projection.adjacent(&list, id(6), MoveDirection::Up), None);
        assert_eq!(projection.adjacent(&list, id(3), MoveDirection::Up), None);
    }

    #[test]
    fn the_move_guard_should_conflict_when_a_sibling_is_promoted_or_reparented() {
        let list = groups();
        let request = move_adjacent(&list, 4, MoveDirection::Down);
        let conflicts = |change: &dyn Fn(&mut TaskList)| {
            let mut changed = list.clone();
            change(&mut changed);
            let before = changed.clone();
            let outcome = request.apply(&mut changed);
            assert_eq!(changed, before);
            outcome
                == Outcome::Conflict {
                    id: id(4),
                    kind: ConflictKind::OrderChanged,
                }
        };
        assert!(conflicts(&|list| {
            list.set_parent(id(5), None).unwrap();
        }));
        assert!(conflicts(&|list| {
            list.set_parent(id(5), Some(id(2))).unwrap();
        }));
        assert!(conflicts(&|list| {
            list.set_parent(id(4), Some(id(2))).unwrap();
        }));
        let top = move_adjacent(&list, 1, MoveDirection::Down);
        let mut nested = list.clone();
        nested.set_parent(id(3), Some(id(2))).unwrap();
        nested.set_parent(id(3), None).unwrap();
        assert!(top.apply(&mut nested.clone()).changed());
        let mut demoted = list.clone();
        demoted.delete(id(6)).unwrap();
        demoted.set_parent(id(2), Some(id(3))).unwrap();
        assert_eq!(
            top.apply(&mut demoted),
            Outcome::Conflict {
                id: id(1),
                kind: ConflictKind::OrderChanged
            }
        );
    }

    #[test]
    fn deleting_an_observed_parent_should_take_the_subtasks_live_at_apply_time() {
        let mut list = groups();
        let delete = Mutation::Delete {
            id: id(1),
            observed: Some(TaskObservation::new(list.task(id(1)).unwrap())),
        };
        list.add_subtask(id(1), "arrived later").unwrap();
        list.delete(id(4)).unwrap();
        assert!(delete.apply(&mut list).changed());
        let sequence = list.task(id(1)).unwrap().deletion_sequence();
        assert_eq!(list.task(id(5)).unwrap().deletion_sequence(), sequence);
        assert_eq!(list.task(id(7)).unwrap().deletion_sequence(), sequence);
        assert_ne!(list.task(id(4)).unwrap().deletion_sequence(), sequence);
        assert!(valid(&list));
        assert_eq!(
            Mutation::Restore {
                id: id(5),
                observed: None
            }
            .apply(&mut list),
            Outcome::Rejected(ListError::ParentDeleted(id(1)))
        );
        assert!(Mutation::RestoreLatest.apply(&mut list).changed());
        assert_eq!(list.visible_tasks().count(), 6);
    }

    #[test]
    fn projected_reorder_should_preserve_hidden_slots_and_reject_changed_membership() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("Needle 東京 first").unwrap();
        let hidden = list.add("unrelated").unwrap();
        let second = list.add("needle 東京 second").unwrap();
        let deleted = list.add("needle 東京 deleted").unwrap();
        list.delete(deleted).unwrap();
        let projection = Projection {
            view: crate::projection::TaskView::Open,
            lowercase_query: "needle 東京".into(),
            filter: crate::metadata::TaskFilter::default(),
        };
        let request = Mutation::MoveAdjacent {
            id: first,
            direction: MoveDirection::Down,
            neighbor: Some(second),
            projection: projection.clone(),
        };
        assert!(request.apply(&mut list).changed());
        assert_eq!(
            list.tasks().iter().map(Task::id).collect::<Vec<_>>(),
            [second, hidden, first, deleted]
        );
        let before = list.clone();
        assert!(matches!(
            request.apply(&mut list),
            Outcome::Conflict {
                kind: ConflictKind::OrderChanged,
                ..
            }
        ));
        assert_eq!(list, before);
        list.set_completed(first, true).unwrap();
        let boundary = Mutation::MoveAdjacent {
            id: first,
            direction: MoveDirection::Down,
            neighbor: None,
            projection,
        };
        let before = list.clone();
        assert!(matches!(
            boundary.apply(&mut list),
            Outcome::Conflict {
                kind: ConflictKind::OrderChanged,
                ..
            }
        ));
        assert_eq!(list, before);
    }

    #[test]
    fn filtered_reorder_should_reject_changed_filter_membership_and_allow_unrelated_edits() {
        let mut filter = crate::metadata::TaskFilter::default();
        filter.add(crate::metadata::parse_filter_token("+x").unwrap());
        let projection = Projection {
            filter,
            ..Projection::default()
        };
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first +x").unwrap();
        let between = list.add("between").unwrap();
        let second = list.add("second +x").unwrap();
        let unrelated = list.add("unrelated").unwrap();
        let request = Mutation::MoveAdjacent {
            id: first,
            direction: MoveDirection::Down,
            neighbor: Some(second),
            projection,
        };
        let conflicts = |list: &mut TaskList| {
            let before = list.clone();
            let conflicted = matches!(
                request.apply(list),
                Outcome::Conflict {
                    kind: ConflictKind::OrderChanged,
                    ..
                }
            );
            assert_eq!(*list, before);
            conflicted
        };

        let mut left = list.clone();
        left.edit(second, "second").unwrap();
        assert!(conflicts(&mut left), "neighbor left the filter");
        let mut joined = list.clone();
        joined.edit(between, "between +x.").unwrap();
        assert!(conflicts(&mut joined), "a row between now matches");
        let mut moved_away = list.clone();
        moved_away.edit(first, "first").unwrap();
        assert!(conflicts(&mut moved_away), "the moved row left the filter");

        list.edit(unrelated, "unrelated but changed").unwrap();
        list.set_completed(between, true).unwrap();
        assert!(request.apply(&mut list).changed());
        assert_eq!(
            list.tasks().iter().map(Task::id).collect::<Vec<_>>(),
            [second, between, first, unrelated]
        );
    }

    #[test]
    fn due_filtered_reorder_should_compare_rows_on_the_projection_date() {
        let mut filter = crate::metadata::TaskFilter::default();
        filter
            .due
            .add(crate::metadata::parse_due_term("today").unwrap());
        // Resolved for the day the user saw; the guard never reads the clock again.
        filter
            .due
            .resolve(|| crate::metadata::Date::new(2026, 10, 21).unwrap());
        let projection = Projection {
            filter,
            ..Projection::default()
        };
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first due:2026-10-21").unwrap();
        let between = list.add("between due:2026-10-22").unwrap();
        let second = list.add("second due:2026-10-21").unwrap();
        let request = Mutation::MoveAdjacent {
            id: first,
            direction: MoveDirection::Down,
            neighbor: Some(second),
            projection,
        };
        let conflicts = |list: &mut TaskList| {
            let before = list.clone();
            let conflicted = matches!(
                request.apply(list),
                Outcome::Conflict {
                    kind: ConflictKind::OrderChanged,
                    ..
                }
            );
            assert_eq!(*list, before);
            conflicted
        };

        let mut left = list.clone();
        left.edit(second, "second due:2026-10-23").unwrap();
        assert!(conflicts(&mut left), "the neighbor's date left the filter");
        let mut joined = list.clone();
        joined.edit(between, "between due:2026-10-21.").unwrap();
        assert!(conflicts(&mut joined), "a newly due row between the pair");

        assert!(request.apply(&mut list).changed());
        assert_eq!(
            list.tasks().iter().map(Task::id).collect::<Vec<_>>(),
            [second, between, first]
        );
    }

    #[test]
    fn a_due_filtered_reorder_should_follow_the_day_its_projection_was_resolved_for() {
        let mut filter = crate::metadata::TaskFilter::default();
        filter.add(crate::metadata::parse_filter_token("due:today").unwrap());
        let day = |day| crate::metadata::Date::new(2026, 10, day).unwrap();
        filter.due.resolve(|| day(21));
        let yesterday = Projection {
            filter,
            ..Projection::default()
        };
        let mut today = yesterday.clone();
        today.filter.due.resolve(|| day(22));
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first due:2026-10-21").unwrap();
        let second = list.add("second due:2026-10-22").unwrap();
        let third = list.add("third due:2026-10-21").unwrap();
        let fourth = list.add("fourth due:2026-10-22").unwrap();
        assert_eq!(
            yesterday.adjacent(&list, first, MoveDirection::Down),
            Some(third)
        );
        assert_eq!(
            today.adjacent(&list, second, MoveDirection::Down),
            Some(fourth)
        );

        // A pair seen on the old day, sent with the new day's projection, is refused.
        let before = list.clone();
        let stale = Mutation::MoveAdjacent {
            id: first,
            direction: MoveDirection::Down,
            neighbor: Some(third),
            projection: today.clone(),
        };
        assert!(matches!(
            stale.apply(&mut list),
            Outcome::Conflict {
                kind: ConflictKind::OrderChanged,
                ..
            }
        ));
        assert_eq!(list, before);

        // Each day's own pair is consistent under the lock, whatever the clock says now.
        for (id, neighbor, projection) in [(first, third, yesterday), (second, fourth, today)] {
            let mut moved = before.clone();
            let request = Mutation::MoveAdjacent {
                id,
                direction: MoveDirection::Down,
                neighbor: Some(neighbor),
                projection,
            };
            assert!(request.apply(&mut moved).changed());
            assert_eq!(
                moved.task(id).map(Task::text),
                before.task(id).map(Task::text)
            );
            let order = moved.tasks().iter().map(Task::id).collect::<Vec<_>>();
            assert!(
                order.iter().position(|task| *task == neighbor)
                    < order.iter().position(|task| *task == id)
            );
        }
    }

    #[test]
    fn selective_restore_should_reject_a_new_deletion_sequence_but_allow_already_live() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("task").unwrap();
        list.delete(id).unwrap();
        let restore = Mutation::Restore {
            id,
            observed: Some(TaskObservation::new(list.task(id).unwrap())),
        };
        assert!(restore.apply(&mut list).changed());
        assert_eq!(restore.apply(&mut list), Outcome::AlreadyInState { id });
        list.delete(id).unwrap();
        let before = list.clone();
        assert_eq!(
            restore.apply(&mut list),
            Outcome::Conflict {
                id,
                kind: ConflictKind::TaskChanged
            }
        );
        assert_eq!(list, before);
        assert!(
            Mutation::Restore { id, observed: None }
                .apply(&mut list)
                .changed()
        );
    }

    #[test]
    fn stale_completion_and_reorder_should_not_reverse_another_client() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        let complete = Mutation::SetCompleted {
            id: first,
            completed: true,
        };
        assert!(complete.apply(&mut list).changed());
        assert_eq!(
            complete.apply(&mut list),
            Outcome::AlreadyInState { id: first }
        );
        assert!(list.task(first).unwrap().completed());
        let reorder = Mutation::MoveAdjacent {
            id: first,
            direction: MoveDirection::Down,
            neighbor: Some(second),
            projection: crate::projection::Projection::default(),
        };
        assert!(reorder.apply(&mut list).changed());
        let before = list.clone();
        assert_eq!(
            reorder.apply(&mut list),
            Outcome::Conflict {
                id: first,
                kind: ConflictKind::OrderChanged
            }
        );
        assert_eq!(list, before);
    }

    #[test]
    fn edits_should_guard_text_while_preserving_other_fields_and_allowing_convergence() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("base").unwrap();
        list.toggle_complete(id).unwrap();
        let edit = Mutation::EditText {
            id,
            text: "mine".into(),
            original: Some("base".into()),
        };
        assert!(edit.apply(&mut list).changed());
        assert!(list.task(id).unwrap().completed());
        assert_eq!(edit.apply(&mut list), Outcome::AlreadyInState { id });
        let before = list.clone();
        let stale = Mutation::EditText {
            id,
            text: "other".into(),
            original: Some("base".into()),
        };
        assert_eq!(
            stale.apply(&mut list),
            Outcome::Conflict {
                id,
                kind: ConflictKind::TextChanged
            }
        );
        assert_eq!(list, before);
    }

    #[test]
    fn deleted_targets_and_changed_observations_should_write_nothing() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("base").unwrap();
        let observed = Some(TaskObservation::new(list.task(id).unwrap()));
        list.toggle_complete(id).unwrap();
        let guarded = Mutation::Delete { id, observed };
        assert_eq!(
            guarded.apply(&mut list),
            Outcome::Conflict {
                id,
                kind: ConflictKind::TaskChanged
            }
        );
        list.delete(id).unwrap();
        let before = list.clone();
        assert_eq!(guarded.apply(&mut list), Outcome::AlreadyInState { id });
        assert_eq!(
            Mutation::EditText {
                id,
                text: "new".into(),
                original: None
            }
            .apply(&mut list),
            Outcome::Conflict {
                id,
                kind: ConflictKind::NoLongerActive
            }
        );
        assert_eq!(list, before);
    }
}
