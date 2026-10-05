//! Reconciliation and deadlines shared by the terminal and deterministic tests.

use std::time::{Duration, Instant};

use color_eyre::eyre::{Result, eyre};

use crate::{
    action::Action,
    app::{App, Transition},
    mutation::{Mutation, Outcome},
    storage::{
        INTERACTIVE_LOCK_BUDGET, MutationReply, ScopePresence, Snapshot, SnapshotToken, Store,
        TransactionError,
    },
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

struct RetryDeadline {
    next: Instant,
    delay: Duration,
}

impl RetryDeadline {
    fn new(now: Instant) -> Self {
        Self {
            next: now + REFRESH_INTERVAL,
            delay: REFRESH_INTERVAL,
        }
    }
    fn completed(&mut self, now: Instant, success: bool) {
        self.delay = if success {
            REFRESH_INTERVAL
        } else {
            (self.delay * 2).min(MAX_BACKOFF)
        };
        self.next = now + self.delay;
    }
}

pub(crate) struct Session {
    pub(crate) app: App,
    token: SnapshotToken,
    initialized: bool,
    refresh: RetryDeadline,
    recovery: Option<RetryDeadline>,
    pending_sync: Option<Outcome>,
}

impl Session {
    pub(crate) fn new(snapshot: Snapshot, now: Instant) -> Self {
        Self {
            initialized: matches!(snapshot.token, SnapshotToken::Present(_)),
            app: App::new(snapshot.list),
            token: snapshot.token,
            refresh: RetryDeadline::new(now),
            recovery: None,
            pending_sync: None,
        }
    }

    fn presence(&self) -> ScopePresence {
        if self.initialized {
            ScopePresence::RequireExisting
        } else {
            ScopePresence::AllowMissing
        }
    }

    pub(crate) fn next_deadline(&self) -> Instant {
        self.recovery
            .as_ref()
            .map_or(self.refresh.next, |recovery| {
                recovery.next.min(self.refresh.next)
            })
    }

    pub(crate) fn action(&mut self, store: &Store, action: Action) -> Result<(bool, bool)> {
        let previous_message = self.app.message().map(str::to_owned);
        let transition = match self.app.prepare(action) {
            Ok(transition) => transition,
            Err(error) => return Ok((false, self.app.set_message(error.to_string()))),
        };
        let dirty = match transition {
            Transition::Quit => return Ok((true, false)),
            Transition::Submit => {
                let request = self
                    .app
                    .take_mutation()
                    .ok_or_else(|| eyre!("missing mutation intent"))?;
                let reply = store.mutate(&request, self.presence(), INTERACTIVE_LOCK_BUDGET);
                self.receive(&request, reply, Instant::now())
            }
            Transition::Refresh => self.refresh_using(store, Instant::now),
            Transition::RecoverSync => self.recover_using(store, Instant::now),
            Transition::Transient => true,
            Transition::Unchanged => false,
        };
        Ok((
            false,
            dirty || previous_message.as_deref() != self.app.message(),
        ))
    }

    fn remember(&mut self, token: SnapshotToken) {
        self.initialized |= matches!(token, SnapshotToken::Present(_));
        self.token = token;
    }

    fn receive(
        &mut self,
        request: &Mutation,
        result: std::result::Result<MutationReply, TransactionError>,
        now: Instant,
    ) -> bool {
        match result {
            Ok(reply) => {
                self.remember(reply.snapshot.token);
                self.refresh.completed(now, true);
                let cleared = self.app.set_storage_error(None);
                self.app.finish(request, reply.snapshot.list, reply.outcome) || cleared
            }
            Err(TransactionError::ReplacedButNotSynced { reply, source }) => {
                self.remember(reply.snapshot.token);
                self.app.reconcile(reply.snapshot.list);
                self.pending_sync = Some(reply.outcome);
                self.app.await_sync(format!(
                    "Change visible; durability unconfirmed; sync will retry: {source:#}"
                ));
                self.recovery = Some(RetryDeadline::new(now));
                self.refresh.completed(now, true);
                true
            }
            Err(error) => self.app.set_message(error.to_string()),
        }
    }

    fn refresh_using(&mut self, store: &Store, clock: impl Fn() -> Instant) -> bool {
        match store.refresh(&self.token, self.presence()) {
            Ok(snapshot) => {
                self.refresh.completed(clock(), true);
                let mut dirty = self.app.set_storage_error(None);
                if let Some(snapshot) = snapshot {
                    self.remember(snapshot.token);
                    dirty |= self.app.reconcile(snapshot.list);
                }
                dirty
            }
            Err(error) => {
                self.refresh.completed(clock(), false);
                self.app
                    .set_storage_error(Some(format!("Storage unavailable: {error:#}")))
            }
        }
    }

    fn recover_using(&mut self, store: &Store, clock: impl Fn() -> Instant) -> bool {
        if self.pending_sync.is_none() {
            return false;
        }
        match store.recover_sync(INTERACTIVE_LOCK_BUDGET) {
            Ok(snapshot) => {
                self.remember(snapshot.token);
                self.app.recovered(snapshot.list);
                self.pending_sync = None;
                self.recovery = None;
                self.refresh.completed(clock(), true);
                true
            }
            Err(error) => {
                if let Some(recovery) = &mut self.recovery {
                    recovery.completed(clock(), false);
                }
                let message =
                    format!("Change visible; durability unconfirmed; sync will retry: {error}");
                let dirty = self.app.message() != Some(&message);
                self.app.await_sync(message);
                dirty
            }
        }
    }

    pub(crate) fn tick(&mut self, store: &Store, now: Instant) -> bool {
        self.tick_using(store, now, Instant::now)
    }

    fn tick_using(&mut self, store: &Store, now: Instant, clock: impl Fn() -> Instant) -> bool {
        let mut dirty = false;
        if now >= self.refresh.next {
            dirty |= self.refresh_using(store, &clock);
        }
        if self
            .recovery
            .as_ref()
            .is_some_and(|recovery| now >= recovery.next)
        {
            dirty |= self.recover_using(store, &clock);
        }
        dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{EditKind, Mode},
        task::{ListScope, MoveDirection, TaskId},
    };

    fn fixture() -> (tempfile::TempDir, Store, Session, Instant) {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path(), ListScope::Global).unwrap();
        store
            .mutate(
                &Mutation::Add("base 東京".into()),
                ScopePresence::AllowMissing,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        store
            .mutate(
                &Mutation::Add("second".into()),
                ScopePresence::AllowMissing,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        let now = Instant::now();
        let session = Session::new(
            store.read_snapshot(ScopePresence::AllowMissing).unwrap(),
            now,
        );
        (home, store, session, now)
    }

    fn id(value: u64) -> TaskId {
        TaskId::from_shell_integer(value).unwrap()
    }

    fn write(store: &Store, request: Mutation) {
        store
            .mutate(
                &request,
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
    }

    fn search(session: &mut Session, store: &Store, query: &str) {
        session.action(store, Action::StartSearch).unwrap();
        for character in query.chars() {
            session
                .action(store, Action::InsertChar(character))
                .unwrap();
        }
        session.action(store, Action::CommitEdit).unwrap();
    }

    #[test]
    fn refresh_should_preserve_live_search_editor_and_reconcile_the_active_projection() {
        let (_home, store, mut session, now) = fixture();
        session.action(&store, Action::CycleView).unwrap();
        session.action(&store, Action::StartSearch).unwrap();
        for character in "base".chars() {
            session
                .action(&store, Action::InsertChar(character))
                .unwrap();
        }
        session.action(&store, Action::MoveCursorLeft).unwrap();
        let before = session.app.editor().unwrap().clone();
        write(
            &store,
            Mutation::SetCompleted {
                id: id(1),
                completed: true,
            },
        );
        session.refresh_using(&store, || now);
        assert_eq!(session.app.mode(), Mode::Search);
        assert_eq!(session.app.selected(), None);
        assert_eq!(session.app.editor().unwrap(), &before);
        write(&store, Mutation::Add("BASE later".into()));
        session.refresh_using(&store, || now);
        assert_eq!(session.app.selected(), Some(id(3)));
        assert_eq!(session.app.editor().unwrap(), &before);
        session.action(&store, Action::CancelEdit).unwrap();
        assert_eq!(session.app.query(), "");
        assert_eq!(session.app.view(), crate::app::TaskView::Open);
        assert_eq!(session.app.selected(), Some(id(2)));
    }

    #[test]
    fn externally_hidden_live_edit_should_keep_its_draft_and_commit_without_losing_completion() {
        let (_home, store, mut session, now) = fixture();
        session.action(&store, Action::CycleView).unwrap();
        session.action(&store, Action::StartEdit).unwrap();
        session.action(&store, Action::InsertChar('é')).unwrap();
        let before = session.app.editor().unwrap().clone();
        write(
            &store,
            Mutation::SetCompleted {
                id: id(1),
                completed: true,
            },
        );
        session.refresh_using(&store, || now);
        assert_eq!(session.app.editor().unwrap(), &before);
        assert_eq!(session.app.selected(), Some(id(2)));
        session.action(&store, Action::CommitEdit).unwrap();
        let latest = store.load().unwrap();
        assert!(latest.task(id(1)).unwrap().completed());
        assert_eq!(latest.task(id(1)).unwrap().text(), "base 東京é");
        assert_eq!(session.app.selected(), Some(id(2)));
        assert!(session.app.message().unwrap().contains("hidden"));
    }

    #[test]
    fn trash_refresh_should_preserve_help_and_live_projection_and_guard_restore_cycles() {
        let (_home, store, mut session, now) = fixture();
        session.action(&store, Action::CycleView).unwrap();
        search(&mut session, &store, "second");
        for target in [1, 2] {
            write(
                &store,
                Mutation::Delete {
                    id: id(target),
                    observed: None,
                },
            );
        }
        session.refresh_using(&store, || now);
        session.action(&store, Action::OpenTrash).unwrap();
        assert_eq!(session.app.selected(), Some(id(2)));
        write(
            &store,
            Mutation::Restore {
                id: id(2),
                observed: None,
            },
        );
        write(
            &store,
            Mutation::Delete {
                id: id(2),
                observed: None,
            },
        );
        let bytes = std::fs::read(&store.paths().data_file).unwrap();
        session.action(&store, Action::RestoreSelected).unwrap();
        assert_eq!(std::fs::read(&store.paths().data_file).unwrap(), bytes);
        assert!(
            session
                .app
                .message()
                .unwrap()
                .contains("review before restoring")
        );
        session.action(&store, Action::OpenHelp).unwrap();
        write(
            &store,
            Mutation::Restore {
                id: id(2),
                observed: None,
            },
        );
        session.refresh_using(&store, || now);
        assert_eq!(session.app.mode(), Mode::Help);
        assert!(session.app.is_trash_view());
        assert_eq!(session.app.selected(), Some(id(1)));
        session.action(&store, Action::CloseHelp).unwrap();
        session.action(&store, Action::RestoreSelected).unwrap();
        assert_eq!(session.app.mode(), Mode::Trash);
        assert_eq!(session.app.selected(), None);
        session.action(&store, Action::CloseTrash).unwrap();
        assert_eq!(session.app.query(), "second");
        assert_eq!(session.app.view(), crate::app::TaskView::Open);
        assert_eq!(session.app.selected(), Some(id(2)));
        assert_eq!(store.load().unwrap().deleted_tasks().len(), 0);
    }

    #[test]
    fn storage_errors_and_sync_recovery_should_not_discard_or_block_transient_search() {
        let (_home, store, mut session, now) = fixture();
        let request = Mutation::SetCompleted {
            id: id(1),
            completed: true,
        };
        session.receive(&request, store.mutate_unsynced(&request), now);
        session.action(&store, Action::StartSearch).unwrap();
        session.action(&store, Action::InsertChar('é')).unwrap();
        let before = session.app.editor().unwrap().clone();
        session.recover_using(&store, || now);
        assert_eq!(session.app.mode(), Mode::Search);
        assert_eq!(session.app.editor().unwrap(), &before);
        let good = std::fs::read(&store.paths().data_file).unwrap();
        std::fs::write(&store.paths().data_file, b"{").unwrap();
        session.refresh_using(&store, || now);
        session.action(&store, Action::CommitEdit).unwrap();
        assert_eq!(session.app.mode(), Mode::Normal);
        assert_eq!(session.app.query(), "é");
        assert!(
            session
                .app
                .message()
                .unwrap()
                .contains("Storage unavailable")
        );
        std::fs::write(&store.paths().data_file, good).unwrap();
    }

    #[test]
    fn unchanged_idle_refresh_should_not_parse_or_redraw_and_input_should_not_starve_it() {
        let (_home, store, mut session, now) = fixture();
        let before = store.counts();
        for step in 1..=20 {
            session.action(&store, Action::MoveDown).unwrap();
            assert!(
                !session.tick_using(&store, now + Duration::from_millis(step * 100), || now
                    + Duration::from_millis(step * 100))
            );
        }
        let after = store.counts();
        assert_eq!(after.0 - before.0, 2);
        assert_eq!(after.1, before.1);
        assert_eq!(session.next_deadline(), now + Duration::from_secs(3));
    }

    #[test]
    fn slow_refresh_should_advance_from_now_without_a_catchup_burst() {
        let (_home, store, mut session, now) = fixture();
        let before = store.counts();
        assert!(
            !session.tick_using(&store, now + Duration::from_secs(10), || now
                + Duration::from_secs(12))
        );
        assert!(
            !session.tick_using(&store, now + Duration::from_secs(12), || now
                + Duration::from_secs(12))
        );
        assert_eq!(store.counts().0, before.0 + 1);
        assert_eq!(session.next_deadline(), now + Duration::from_secs(13));
    }

    #[test]
    fn equivalent_snapshot_rewrite_should_update_token_without_redraw_or_repeat_parse() {
        let (_home, store, mut session, now) = fixture();
        let bytes = std::fs::read(&store.paths().data_file).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        std::fs::write(
            &store.paths().data_file,
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let before = store.counts();
        assert!(!session.refresh_using(&store, || now));
        assert_eq!(store.counts().1, before.1 + 1);
        assert!(!session.refresh_using(&store, || now));
        assert_eq!(store.counts().1, before.1 + 1);
    }

    #[test]
    fn action_should_keep_its_displayed_target_when_storage_changes_before_due_refresh() {
        let (_home, store, mut session, now) = fixture();
        store
            .mutate(
                &Mutation::Delete {
                    id: id(1),
                    observed: None,
                },
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        // The terminal processes input before the overdue refresh, through action().
        session.action(&store, Action::ToggleComplete).unwrap();
        session.tick_using(&store, now + Duration::from_secs(1), || {
            now + Duration::from_secs(1)
        });
        assert_eq!(session.app.selected(), Some(id(2)));
        assert!(session.app.tasks().task(id(1)).unwrap().is_deleted());
        assert!(!session.app.tasks().task(id(2)).unwrap().completed());
        assert!(session.app.celebration().is_none());
    }

    #[test]
    fn celebration_should_require_our_durable_transaction_to_complete_the_latest_list() {
        let (_home, store, mut session, now) = fixture();
        session.action(&store, Action::ToggleComplete).unwrap();
        assert!(session.app.celebration().is_none());
        store
            .mutate(
                &Mutation::SetCompleted {
                    id: id(2),
                    completed: true,
                },
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        session.refresh_using(&store, || now);
        assert!(session.app.celebration().is_none());
        session.action(&store, Action::ToggleComplete).unwrap();
        session.action(&store, Action::ToggleComplete).unwrap();
        assert!(session.app.celebration().is_some());
        store
            .mutate(
                &Mutation::Add("new open task".into()),
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        session.refresh_using(&store, || now);
        assert!(session.app.celebration().is_none());
    }

    #[test]
    fn stale_editors_on_distinct_tasks_should_preserve_both_changes() {
        let (_home, store, mut first, now) = fixture();
        let mut second = Session::new(
            store.read_snapshot(ScopePresence::RequireExisting).unwrap(),
            now,
        );
        first.action(&store, Action::StartEdit).unwrap();
        first.action(&store, Action::InsertChar('é')).unwrap();
        second.action(&store, Action::MoveDown).unwrap();
        second.action(&store, Action::StartEdit).unwrap();
        second.action(&store, Action::InsertChar('界')).unwrap();
        first.action(&store, Action::CommitEdit).unwrap();
        second.action(&store, Action::CommitEdit).unwrap();
        let latest = store.load().unwrap();
        assert_eq!(latest.task(id(1)).unwrap().text(), "base 東京é");
        assert_eq!(latest.task(id(2)).unwrap().text(), "second界");
        assert!(first.app.editor().is_none());
        assert!(second.app.editor().is_none());
        first.refresh_using(&store, || now);
        assert_eq!(first.app.tasks(), &latest);
    }

    #[test]
    fn selection_should_keep_identity_then_fall_back_by_ordinal_and_select_arrivals() {
        let (_home, store, mut session, now) = fixture();
        let mutate = |request| {
            store
                .mutate(
                    &request,
                    ScopePresence::RequireExisting,
                    INTERACTIVE_LOCK_BUDGET,
                )
                .unwrap();
        };
        session.action(&store, Action::MoveDown).unwrap();
        mutate(Mutation::Add("third".into()));
        session.refresh_using(&store, || now);
        assert_eq!(session.app.selected(), Some(id(2)));
        for (deleted, expected) in [(2, Some(id(3))), (3, Some(id(1))), (1, None)] {
            mutate(Mutation::Delete {
                id: id(deleted),
                observed: None,
            });
            session.refresh_using(&store, || now);
            assert_eq!(session.app.selected(), expected);
        }
        mutate(Mutation::Add("arrival".into()));
        session.refresh_using(&store, || now);
        assert_eq!(session.app.selected(), Some(id(4)));
    }

    #[test]
    fn refresh_should_keep_utf8_drafts_and_help_without_external_celebrations() {
        let (_home, store, mut session, now) = fixture();
        session.action(&store, Action::StartEdit).unwrap();
        session.action(&store, Action::InsertChar('é')).unwrap();
        session.action(&store, Action::MoveCursorLeft).unwrap();
        let before = session.app.editor().unwrap().clone();
        store
            .mutate(
                &Mutation::SetCompleted {
                    id: id(1),
                    completed: true,
                },
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        assert!(
            session.tick_using(&store, now + Duration::from_secs(1), || now
                + Duration::from_secs(1))
        );
        assert_eq!(session.app.editor().unwrap(), &before);
        assert!(session.app.celebration().is_none());
        session.action(&store, Action::CancelEdit).unwrap();
        session.action(&store, Action::OpenHelp).unwrap();
        store
            .mutate(
                &Mutation::Add("arrived".into()),
                ScopePresence::AllowMissing,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        session.tick_using(&store, now + Duration::from_secs(2), || {
            now + Duration::from_secs(2)
        });
        assert_eq!(session.app.mode(), Mode::Help);
    }

    #[test]
    fn conflicting_edit_should_keep_the_original_and_cursor_even_after_typing() {
        let (_home, store, mut session, _now) = fixture();
        session.action(&store, Action::StartEdit).unwrap();
        session.action(&store, Action::InsertChar('é')).unwrap();
        let before = session.app.editor().unwrap().clone();
        store
            .mutate(
                &Mutation::EditText {
                    id: id(1),
                    text: "external".into(),
                    original: None,
                },
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        session.action(&store, Action::CommitEdit).unwrap();
        let editor = session.app.editor().unwrap();
        assert_eq!(editor.buffer(), before.buffer());
        assert_eq!(editor.cursor(), before.cursor());
        assert!(editor.conflicted());
        session.action(&store, Action::InsertChar('!')).unwrap();
        assert!(session.app.editor().unwrap().conflicted());
        session.action(&store, Action::CommitEdit).unwrap();
        assert_eq!(
            store.load().unwrap().task(id(1)).unwrap().text(),
            "external"
        );
    }

    #[test]
    fn observed_deletion_should_invalidate_draft_even_after_restore() {
        let (_home, store, mut session, now) = fixture();
        session.action(&store, Action::StartEdit).unwrap();
        session.action(&store, Action::InsertChar('!')).unwrap();
        store
            .mutate(
                &Mutation::Delete {
                    id: id(1),
                    observed: None,
                },
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        session.tick_using(&store, now + Duration::from_secs(1), || {
            now + Duration::from_secs(1)
        });
        assert_eq!(session.app.selected(), Some(id(2)));
        assert_eq!(session.app.editor().unwrap().kind(), EditKind::Edit(id(1)));
        store
            .mutate(
                &Mutation::RestoreLatest,
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        session.tick_using(&store, now + Duration::from_secs(2), || {
            now + Duration::from_secs(2)
        });
        session.action(&store, Action::CommitEdit).unwrap();
        assert_eq!(session.app.mode(), Mode::Insert);
        assert_eq!(
            store.load().unwrap().task(id(1)).unwrap().text(),
            "base 東京"
        );
        session.action(&store, Action::CancelEdit).unwrap();
    }

    #[test]
    fn unchanged_draft_should_close_without_overwriting_an_unseen_edit() {
        let (_home, store, mut session, _now) = fixture();
        session.action(&store, Action::StartEdit).unwrap();
        store
            .mutate(
                &Mutation::EditText {
                    id: id(1),
                    text: "external".into(),
                    original: None,
                },
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        session.action(&store, Action::CommitEdit).unwrap();
        assert!(session.app.editor().is_none());
        assert_eq!(session.app.tasks().task(id(1)).unwrap().text(), "external");
    }

    #[test]
    fn value_cycles_that_were_not_observed_should_follow_schema_one_policy() {
        let (_home, store, mut session, _now) = fixture();
        session.action(&store, Action::StartEdit).unwrap();
        session.action(&store, Action::InsertChar('!')).unwrap();
        store
            .mutate(
                &Mutation::Delete {
                    id: id(1),
                    observed: None,
                },
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        store
            .mutate(
                &Mutation::RestoreLatest,
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        session.action(&store, Action::CommitEdit).unwrap();
        assert!(session.app.editor().is_none());
        assert_eq!(
            store.load().unwrap().task(id(1)).unwrap().text(),
            "base 東京!"
        );
    }

    #[test]
    fn failed_save_and_busy_should_keep_draft_and_never_claim_success() {
        let (_home, store, mut session, _now) = fixture();
        session.action(&store, Action::StartAdd).unwrap();
        session.action(&store, Action::InsertChar('é')).unwrap();
        let before = session.app.editor().unwrap().clone();
        std::fs::create_dir(&store.paths().temp_file).unwrap();
        session.action(&store, Action::CommitEdit).unwrap();
        assert_eq!(session.app.editor().unwrap(), &before);
        assert_eq!(store.load().unwrap().visible_tasks().count(), 2);
        std::fs::remove_dir(&store.paths().temp_file).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&store.paths().lock_file)
            .unwrap();
        lock.try_lock().unwrap();
        session.action(&store, Action::CommitEdit).unwrap();
        assert_eq!(session.app.editor().unwrap(), &before);
        assert!(session.app.message().unwrap().contains("timed out"));
        drop(lock);
        session.action(&store, Action::CommitEdit).unwrap();
        assert!(session.app.editor().is_none());
        assert_eq!(store.load().unwrap().visible_tasks().count(), 3);
    }

    #[test]
    fn corrupt_refresh_should_back_off_keep_last_good_and_clear_error_with_equal_bytes() {
        let (_home, store, mut session, now) = fixture();
        let good = std::fs::read(&store.paths().data_file).unwrap();
        let before = session.app.tasks().clone();
        std::fs::write(&store.paths().data_file, b"{").unwrap();
        assert!(
            session.tick_using(&store, now + Duration::from_secs(1), || now
                + Duration::from_secs(1))
        );
        assert_eq!(session.app.tasks(), &before);
        assert_eq!(session.next_deadline(), now + Duration::from_secs(3));
        assert!(
            !session.tick_using(&store, now + Duration::from_secs(3), || now
                + Duration::from_secs(3))
        );
        assert_eq!(session.next_deadline(), now + Duration::from_secs(7));
        assert!(
            !session.tick_using(&store, now + Duration::from_secs(7), || now
                + Duration::from_secs(7))
        );
        assert_eq!(session.next_deadline(), now + Duration::from_secs(12));
        std::fs::write(&store.paths().data_file, good).unwrap();
        let parses = store.counts().1;
        assert!(
            session.tick_using(&store, now + Duration::from_secs(12), || now
                + Duration::from_secs(12))
        );
        assert!(session.app.message().is_none());
        assert_eq!(store.counts().1, parses);
    }

    #[test]
    fn initialized_write_should_recreate_missing_lock_without_resetting_the_snapshot() {
        let (_home, store, mut session, now) = fixture();
        let mut expected = session.app.tasks().clone();
        expected.set_completed(id(1), true).unwrap();
        std::fs::remove_file(&store.paths().lock_file).unwrap();

        assert!(!session.refresh_using(&store, || now));
        assert!(!store.paths().lock_file.exists());
        session.action(&store, Action::ToggleComplete).unwrap();

        assert_eq!(store.load().unwrap(), expected);
        assert_eq!(session.app.tasks(), &expected);
        assert!(store.paths().lock_file.exists());
        assert!(session.app.message().is_none());
    }

    #[test]
    fn initialized_missing_snapshot_should_not_reset_counters_or_recreate_data() {
        let (_home, store, mut session, now) = fixture();
        std::fs::remove_file(&store.paths().data_file).unwrap();
        std::fs::remove_file(&store.paths().lock_file).unwrap();
        session.tick_using(&store, now + Duration::from_secs(1), || {
            now + Duration::from_secs(1)
        });
        session.action(&store, Action::Delete).unwrap();
        assert!(!store.paths().data_file.exists());
        assert!(!store.paths().lock_file.exists());
        assert!(
            session
                .app
                .message()
                .unwrap()
                .contains("previously initialized")
        );
        assert_eq!(session.app.tasks().visible_tasks().count(), 2);
        let error = store
            .mutate(
                &Mutation::Add("reset?".into()),
                session.presence(),
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap_err();
        assert!(error.to_string().contains("previously initialized"));
        assert!(store.paths().lock_file.exists());
        let error = store.recover_sync(INTERACTIVE_LOCK_BUDGET).unwrap_err();
        assert!(error.to_string().contains("previously initialized"));
        assert!(!store.paths().data_file.exists());
    }

    #[test]
    fn missing_scope_should_be_read_only_until_first_add_and_presence_should_be_sticky() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path(), ListScope::Global).unwrap();
        let now = Instant::now();
        let mut session = Session::new(
            store.read_snapshot(ScopePresence::AllowMissing).unwrap(),
            now,
        );
        session.tick_using(&store, now + Duration::from_secs(1), || {
            now + Duration::from_secs(1)
        });
        assert!(!home.path().join(".shtodo").exists());
        session.action(&store, Action::StartAdd).unwrap();
        session.action(&store, Action::InsertChar('a')).unwrap();
        session.action(&store, Action::CommitEdit).unwrap();
        assert_eq!(session.presence(), ScopePresence::RequireExisting);
        std::fs::remove_file(&store.paths().data_file).unwrap();
        session.refresh_using(&store, || now + Duration::from_secs(3));
        assert_eq!(session.presence(), ScopePresence::RequireExisting);
    }

    #[test]
    fn sync_recovery_with_missing_lock_should_preserve_later_writes_without_replaying_add() {
        let (_home, store, mut session, now) = fixture();
        session.action(&store, Action::StartAdd).unwrap();
        session.action(&store, Action::InsertChar('é')).unwrap();
        session.app.prepare(Action::CommitEdit).unwrap();
        let request = session.app.take_mutation().unwrap();
        let result = store.mutate_unsynced(&request);
        assert!(matches!(
            result,
            Err(TransactionError::ReplacedButNotSynced { .. })
        ));
        session.receive(&request, result, now);
        let before = session.app.editor().unwrap().clone();
        assert!(session.app.needs_sync());
        assert!(!session.refresh_using(&store, || now));
        assert!(session.app.needs_sync());
        session.action(&store, Action::InsertChar('x')).unwrap();
        assert_eq!(session.app.editor().unwrap(), &before);
        store
            .mutate(
                &Mutation::Delete {
                    id: id(3),
                    observed: None,
                },
                ScopePresence::RequireExisting,
                INTERACTIVE_LOCK_BUDGET,
            )
            .unwrap();
        let bytes = std::fs::read(&store.paths().data_file).unwrap();
        std::fs::remove_file(&store.paths().lock_file).unwrap();
        session.action(&store, Action::CommitEdit).unwrap();
        assert!(!session.app.needs_sync());
        assert!(store.paths().lock_file.exists());
        assert!(session.app.editor().is_none());
        assert_eq!(std::fs::read(&store.paths().data_file).unwrap(), bytes);
        assert!(session.app.tasks().task(id(3)).unwrap().is_deleted());
        assert_eq!(session.app.tasks().tasks().len(), 3);
    }

    #[test]
    fn sync_recovery_backoff_should_be_independent_and_cancel_should_not_clear_warning() {
        let (_home, store, mut session, now) = fixture();
        session.action(&store, Action::StartAdd).unwrap();
        session.action(&store, Action::InsertChar('a')).unwrap();
        session.app.prepare(Action::CommitEdit).unwrap();
        let request = session.app.take_mutation().unwrap();
        session.receive(&request, store.mutate_unsynced(&request), now);
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&store.paths().lock_file)
            .unwrap();
        lock.try_lock().unwrap();
        session.tick_using(&store, now + Duration::from_secs(1), || {
            now + Duration::from_secs(1)
        });
        assert_eq!(
            session.recovery.as_ref().unwrap().next,
            now + Duration::from_secs(3)
        );
        session.refresh_using(&store, || now + Duration::from_secs(2));
        assert_eq!(
            session.recovery.as_ref().unwrap().next,
            now + Duration::from_secs(3)
        );
        session.action(&store, Action::CancelEdit).unwrap();
        session.action(&store, Action::StartAdd).unwrap();
        assert_eq!(session.app.mode(), Mode::Normal);
        assert!(session.app.needs_sync());
        drop(lock);
        session.tick_using(&store, now + Duration::from_secs(3), || {
            now + Duration::from_secs(3)
        });
        assert!(!session.app.needs_sync());
    }

    #[test]
    fn sync_recovery_should_never_replay_restore_or_reorder() {
        for kind in [0, 1, 2] {
            let restore = kind != 1;
            let (_home, store, mut session, now) = fixture();
            if restore {
                for target in [1, 2] {
                    store
                        .mutate(
                            &Mutation::Delete {
                                id: id(target),
                                observed: None,
                            },
                            ScopePresence::RequireExisting,
                            INTERACTIVE_LOCK_BUDGET,
                        )
                        .unwrap();
                }
                session.refresh_using(&store, || now);
            }
            let action = if kind == 2 {
                session.app.prepare(Action::OpenTrash).unwrap();
                Action::RestoreSelected
            } else if restore {
                Action::RestoreLatest
            } else {
                Action::MoveTaskDown
            };
            session.app.prepare(action).unwrap();
            let request = session.app.take_mutation().unwrap();
            session.receive(&request, store.mutate_unsynced(&request), now);
            assert!(session.app.needs_sync());
            let later = if restore {
                Mutation::Delete {
                    id: id(2),
                    observed: None,
                }
            } else {
                Mutation::MoveAdjacent {
                    id: id(2),
                    direction: MoveDirection::Down,
                    neighbor: Some(id(1)),
                    projection: crate::projection::Projection::default(),
                }
            };
            store
                .mutate(
                    &later,
                    ScopePresence::RequireExisting,
                    INTERACTIVE_LOCK_BUDGET,
                )
                .unwrap();
            let bytes = std::fs::read(&store.paths().data_file).unwrap();
            assert!(session.recover_using(&store, || now));
            assert!(!session.app.needs_sync());
            assert_eq!(std::fs::read(&store.paths().data_file).unwrap(), bytes);
            assert_eq!(session.app.tasks(), &store.load().unwrap());
            if restore {
                assert_eq!(session.app.tasks().visible_tasks().count(), 0);
            } else {
                assert_eq!(
                    session
                        .app
                        .tasks()
                        .visible_tasks()
                        .map(|task| task.id())
                        .collect::<Vec<_>>(),
                    [id(1), id(2)]
                );
            }
        }
    }
}

#[cfg(test)]
mod measurements {
    use super::*;
    use crate::task::{ListScope, TaskList};

    struct Group {
        _home: tempfile::TempDir,
        store: Store,
        sessions: Vec<Session>,
        records: usize,
        samples: Vec<serde_json::Value>,
    }

    fn until(groups: &mut [Group], end: Instant) -> Vec<Vec<u64>> {
        let mut durations = vec![Vec::new(); groups.len()];
        while Instant::now() < end {
            let next = groups
                .iter()
                .flat_map(|group| &group.sessions)
                .map(Session::next_deadline)
                .min()
                .unwrap()
                .min(end);
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
            if Instant::now() >= end {
                break;
            }
            for (index, group) in groups.iter_mut().enumerate() {
                for session in &mut group.sessions {
                    if Instant::now() >= session.next_deadline() {
                        let start = Instant::now();
                        assert!(
                            !session.tick(&group.store, start),
                            "idle data must not request redraw"
                        );
                        durations[index].push(start.elapsed().as_nanos() as u64);
                    }
                }
            }
        }
        durations
    }

    #[test]
    #[ignore = "three 60-second refresh samples; invoke explicitly in release mode"]
    fn idle_refresh_measurements() {
        let mut groups = Vec::new();
        for records in [0, 1000, 10000] {
            for clients in [1, 2] {
                let home = tempfile::tempdir().unwrap();
                let store = Store::open(home.path(), ListScope::Global).unwrap();
                let mut list = TaskList::new(ListScope::Global);
                for index in 0..records {
                    let id = list
                        .add(&format!("Task {index} café 東京 {}", "x".repeat(40)))
                        .unwrap();
                    if index % 3 == 0 {
                        list.delete(id).unwrap();
                    }
                }
                store.save(&list).unwrap();
                let sessions = (0..clients)
                    .map(|_| {
                        Session::new(
                            store.read_snapshot(ScopePresence::RequireExisting).unwrap(),
                            Instant::now(),
                        )
                    })
                    .collect();
                groups.push(Group {
                    _home: home,
                    store,
                    sessions,
                    records,
                    samples: Vec::new(),
                });
            }
        }
        until(&mut groups, Instant::now() + Duration::from_secs(5));
        for sample in 0..3 {
            let before = groups
                .iter()
                .map(|group| group.store.counts())
                .collect::<Vec<_>>();
            let durations = until(&mut groups, Instant::now() + Duration::from_secs(60));
            for ((group, before), raw) in groups.iter_mut().zip(before).zip(durations) {
                let after = group.store.counts();
                let reads = after.0 - before.0;
                assert_eq!(after.1, before.1, "unchanged bytes must skip parsing");
                assert!(reads <= 61 * group.sessions.len() as u64);
                assert!(reads >= 58 * group.sessions.len() as u64);
                let mut sorted = raw.clone();
                sorted.sort_unstable();
                let percentile =
                    |p: usize| sorted[((sorted.len() - 1) * p) / 100] as f64 / 1_000_000.0;
                group.samples.push(serde_json::json!({
                    "sample": sample + 1, "reads": reads, "bytes_read": after.2 - before.2,
                    "parses": after.1 - before.1, "redraw_requests": 0,
                    "p50_ms": percentile(50), "p95_ms": percentile(95), "max_ms": percentile(100),
                    "raw_read_nanoseconds": raw,
                }));
            }
            println!("refresh sample {} complete", sample + 1);
        }
        let groups = groups.iter().map(|group| serde_json::json!({
            "records": group.records, "clients": group.sessions.len(),
            "snapshot_bytes": std::fs::metadata(&group.store.paths().data_file).unwrap().len(),
            "samples": group.samples,
        })).collect::<Vec<_>>();
        let result = serde_json::json!({
            "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "method": "Release test executable; private read/parse counters and Session redraw requests. Independent fixture groups share one process and are polled by earliest deadline. Measures refresh read/token comparison, not actual terminal CPU or terminal draw calls.",
            "groups": groups,
        });
        let output = std::env::var_os("SHTODO_REFRESH_REPORT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("target/concurrent-refresh.json"));
        std::fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
}
