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
            Self::Add(text) | Self::EditText { text, .. } => {
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
                let moved = if projection == &Projection::default() {
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
    use crate::task::ListScope;

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
