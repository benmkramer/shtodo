use std::collections::HashMap;

use crate::{
    action::Action,
    metadata::{self, Date, FilterToken, TaskFilter},
    mutation::{ConflictKind, Mutation, Outcome, TaskObservation},
    projection::{self, Projection, TaskOrder},
    task::{ListError, MoveDirection, Task, TaskId, TaskList},
};

const CELEBRATION_FRAME_COUNT: u16 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Normal,
    Insert,
    Search,
    Filter,
    Help,
    Trash,
}

pub(crate) use crate::projection::TaskView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditKind {
    Add,
    Edit(TaskId),
    Search,
    Filter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Editor {
    kind: EditKind,
    buffer: String,
    cursor: usize,
    original: Option<String>,
    conflicted: bool,
    invalidated: bool,
}

impl Editor {
    pub(crate) fn kind(&self) -> EditKind {
        self.kind
    }

    pub(crate) fn buffer(&self) -> &str {
        &self.buffer
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn conflicted(&self) -> bool {
        self.conflicted
    }

    fn insert(&mut self, character: char) {
        self.buffer.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    fn move_left(&mut self) -> bool {
        let Some((index, _)) = self.buffer[..self.cursor].char_indices().last() else {
            return false;
        };
        self.cursor = index;
        true
    }

    fn move_right(&mut self) -> bool {
        if self.cursor == self.buffer.len() {
            return false;
        }
        let next = self.buffer[self.cursor..]
            .char_indices()
            .nth(1)
            .map_or(self.buffer.len(), |(index, _)| self.cursor + index);
        self.cursor = next;
        true
    }

    fn move_start(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor = 0;
        true
    }

    fn move_end(&mut self) -> bool {
        if self.cursor == self.buffer.len() {
            return false;
        }
        self.cursor = self.buffer.len();
        true
    }

    fn move_word_left(&mut self) -> bool {
        let target = previous_word_start(&self.buffer, self.cursor);
        if target == self.cursor {
            return false;
        }
        self.cursor = target;
        true
    }

    fn move_word_right(&mut self) -> bool {
        let target = next_word_end(&self.buffer, self.cursor);
        if target == self.cursor {
            return false;
        }
        self.cursor = target;
        true
    }

    fn delete_before_cursor(&mut self) -> bool {
        let Some((index, _)) = self.buffer[..self.cursor].char_indices().last() else {
            return false;
        };
        self.buffer.drain(index..self.cursor);
        self.cursor = index;
        true
    }

    fn delete_at_cursor(&mut self) -> bool {
        if self.cursor == self.buffer.len() {
            return false;
        }
        let next = self.buffer[self.cursor..]
            .char_indices()
            .nth(1)
            .map_or(self.buffer.len(), |(index, _)| self.cursor + index);
        self.buffer.drain(self.cursor..next);
        true
    }

    fn delete_word_before_cursor(&mut self) -> bool {
        let target = previous_word_start(&self.buffer, self.cursor);
        if target == self.cursor {
            return false;
        }
        self.buffer.drain(target..self.cursor);
        self.cursor = target;
        true
    }

    fn delete_word_at_cursor(&mut self) -> bool {
        let target = next_word_end(&self.buffer, self.cursor);
        if target == self.cursor {
            return false;
        }
        self.buffer.drain(self.cursor..target);
        true
    }
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn previous_word_start(buffer: &str, cursor: usize) -> usize {
    let mut target = cursor;
    let mut characters = buffer[..cursor].char_indices().rev().peekable();

    while let Some(&(index, character)) = characters.peek() {
        if is_word_character(character) {
            break;
        }
        target = index;
        characters.next();
    }
    while let Some(&(index, character)) = characters.peek() {
        if !is_word_character(character) {
            break;
        }
        target = index;
        characters.next();
    }

    target
}

fn next_word_end(buffer: &str, cursor: usize) -> usize {
    let mut target = cursor;
    let mut characters = buffer[cursor..].char_indices().peekable();

    while let Some(&(index, character)) = characters.peek() {
        if is_word_character(character) {
            break;
        }
        target = cursor + index + character.len_utf8();
        characters.next();
    }
    while let Some(&(index, character)) = characters.peek() {
        if !is_word_character(character) {
            break;
        }
        target = cursor + index + character.len_utf8();
        characters.next();
    }

    target
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Transition {
    Unchanged,
    Transient,
    Submit,
    Refresh,
    RecoverSync,
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Celebration {
    frame: u16,
}

impl Celebration {
    #[cfg(test)]
    pub(crate) fn frame(self) -> u16 {
        self.frame
    }
}

struct TrashState {
    selected: Option<TaskId>,
}

/// A footer notice that names a configured key, so the UI renders its text from the keymap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Notice {
    /// Reorder was requested while the list is sorted by priority or due date.
    ManualOrderRequired,
    /// Enter on a tag prefix shared by these tags, in hint-line order.
    FinishTag(Vec<String>),
    /// Trash restore of a subtask whose parent, named here, is still in trash.
    RestoreParentFirst(TaskId),
}

/// Filter-mode preview. It never leaves `App`, so no shell or `Mutation` value can carry it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FilterPreview {
    exact: TaskFilter,
    /// The trailing tag token with trailing `.,;:!?` stripped, as for `--tag` values.
    trailing_prefix: Option<String>,
}

impl FilterPreview {
    fn matches(&self, text: &str, completed: bool) -> bool {
        self.exact.matches(text, completed)
            && self
                .trailing_prefix
                .as_deref()
                .is_none_or(|prefix| metadata::tags(text).any(|tag| tag.starts_with(prefix)))
    }
}

struct FilterSession {
    selected: Option<TaskId>,
    preview: FilterPreview,
    /// The buffer's completed tokens: the rows the hint line counts and Enter completes from.
    completed: TaskFilter,
    /// Tag counts for `completed`, kept across keystrokes that only extend the trailing token.
    tag_counts: Vec<(String, usize)>,
}

/// Splits a filter buffer into completed tokens and the trailing token still being typed.
fn split_filter_buffer(buffer: &str) -> (Vec<&str>, Option<&str>) {
    let mut tokens = buffer.split_whitespace().collect::<Vec<_>>();
    let trailing = if buffer.ends_with(char::is_whitespace) {
        None
    } else {
        tokens.pop()
    };
    (tokens, trailing)
}

/// Resolves due terms against the displayed date. An empty due filter is left as parsed, so
/// tag- and priority-only filters keep their shape.
fn resolve_due(filter: &mut TaskFilter, today: Date) {
    if !filter.due.is_empty() {
        filter.due.resolve(|| today);
    }
}

/// Parses completed tokens, with due terms resolved against the displayed date.
fn parse_filter_tokens<'a>(tokens: &[&'a str], today: Date) -> Result<TaskFilter, &'a str> {
    let mut filter = TaskFilter::default();
    for token in tokens {
        filter.add(metadata::parse_filter_token(token).ok_or(*token)?);
    }
    resolve_due(&mut filter, today);
    Ok(filter)
}

fn invalid_filter(token: &str) -> String {
    format!("Invalid filter \"{token}\"; use +project, @context, A-C, none, or due:WHEN")
}

const HIDDEN: &str = "hidden by the current view, search, or filter";

pub(crate) struct App {
    tasks: TaskList,
    mode: Mode,
    selected: Option<TaskId>,
    trash: Option<TrashState>,
    editor: Option<Editor>,
    message: Option<String>,
    notice: Option<Notice>,
    celebration: Option<Celebration>,
    query: String,
    projection: Projection,
    /// The accepted filter's normalized tokens, as shown in the status line and editor.
    filter_text: String,
    filter_session: Option<FilterSession>,
    order: TaskOrder,
    /// Canonical indices of live tasks in Priority or Due order; rebuilt only when tasks or the
    /// order change. Due keys are absolute dates, so a new day never rebuilds it.
    sorted: Option<Vec<usize>>,
    /// How often `sorted` was rebuilt, so tests can prove when it is not.
    #[cfg(test)]
    order_rebuilds: usize,
    search_session: Option<SearchSession>,
    pending: Option<Mutation>,
    storage_error: Option<String>,
    durability_warning: Option<String>,
    /// The displayed local date: it styles due tokens, resolves relative due filter terms, and
    /// rewrites `due:today` and `due:tomorrow` on add and edit.
    today: Date,
}

/// The fixed date every test `App` starts on, so no unit test reads the real clock.
#[cfg(test)]
pub(crate) fn test_today() -> Date {
    Date::new(2026, 10, 21).unwrap()
}

struct SearchSession {
    query: String,
    selected: Option<TaskId>,
}

impl App {
    #[cfg(test)]
    pub(crate) fn new(tasks: TaskList) -> Self {
        Self::with_today(tasks, test_today())
    }

    pub(crate) fn with_today(tasks: TaskList, today: Date) -> Self {
        let selected = tasks.visible_tasks().next().map(Task::id);
        Self {
            tasks,
            mode: Mode::Normal,
            selected,
            trash: None,
            editor: None,
            message: None,
            notice: None,
            celebration: None,
            query: String::new(),
            projection: Projection::default(),
            filter_text: String::new(),
            filter_session: None,
            order: TaskOrder::Manual,
            sorted: None,
            #[cfg(test)]
            order_rebuilds: 0,
            search_session: None,
            pending: None,
            storage_error: None,
            durability_warning: None,
            today,
        }
    }

    pub(crate) fn today(&self) -> Date {
        self.today
    }

    /// Records a new local date and re-resolves relative due terms against it. Returns whether
    /// anything shown changed: row styling depends on the date, so every new date redraws.
    pub(crate) fn set_today(&mut self, today: Date) -> bool {
        if self.today == today {
            return false;
        }
        self.today = today;
        // Terms keep the typed words, so a new day only re-resolves them; the Due order holds
        // absolute dates and is not rebuilt.
        resolve_due(&mut self.projection.filter, today);
        if let Some(session) = &mut self.filter_session {
            resolve_due(&mut session.preview.exact, today);
            resolve_due(&mut session.completed, today);
        }
        self.refresh_tag_counts();
        self.reconcile_selection();
        true
    }

    pub(crate) fn prepare(&mut self, action: Action) -> Result<Transition, ListError> {
        self.pending = None;
        if self.needs_sync() {
            if action == Action::CommitEdit && self.mode == Mode::Insert {
                return Ok(Transition::RecoverSync);
            }
            if self.mode == Mode::Insert && !matches!(action, Action::CancelEdit | Action::Quit) {
                return Ok(Transition::Unchanged);
            }
            if matches!(
                action,
                Action::StartAdd
                    | Action::StartEdit
                    | Action::ToggleComplete
                    | Action::Delete
                    | Action::RestoreLatest
                    | Action::RestoreSelected
                    | Action::MoveTaskDown
                    | Action::MoveTaskUp
            ) {
                return Ok(Transition::Unchanged);
            }
        }
        if self.storage_error.is_some()
            && ((action == Action::CommitEdit && self.mode == Mode::Insert)
                || matches!(
                    action,
                    Action::ToggleComplete
                        | Action::Delete
                        | Action::RestoreLatest
                        | Action::RestoreSelected
                        | Action::MoveTaskDown
                        | Action::MoveTaskUp
                ))
        {
            return Ok(Transition::Unchanged);
        }
        match self.mode {
            Mode::Normal => self.apply_normal(action),
            Mode::Insert => self.apply_insert(action),
            Mode::Search => self.apply_search(action),
            Mode::Filter => self.apply_filter(action),
            Mode::Help => self.apply_help(action),
            Mode::Trash => self.apply_trash(action),
        }
    }

    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    pub(crate) fn selected(&self) -> Option<TaskId> {
        self.trash
            .as_ref()
            .map_or(self.selected, |trash| trash.selected)
    }

    pub(crate) fn is_trash_view(&self) -> bool {
        self.trash.is_some()
    }

    pub(crate) fn selected_task(&self) -> Option<&Task> {
        self.selected()
            .and_then(|id| self.tasks.task(id))
            .filter(|task| {
                if self.is_trash_view() {
                    task.is_deleted()
                } else {
                    self.matches(task)
                }
            })
    }

    pub(crate) fn view(&self) -> TaskView {
        self.projection.view
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// The accepted filter; the Filter-mode preview never replaces it.
    pub(crate) fn filter(&self) -> &TaskFilter {
        &self.projection.filter
    }

    pub(crate) fn filter_text(&self) -> &str {
        &self.filter_text
    }

    #[cfg(test)]
    pub(crate) fn filter_preview(&self) -> Option<&FilterPreview> {
        self.filter_session.as_ref().map(|session| &session.preview)
    }

    /// Filter mode's tag discovery: distinct tags with row counts, highest first, then by name.
    pub(crate) fn filter_hint(&self) -> Option<Vec<(&str, usize)>> {
        let session = self.filter_session.as_ref()?;
        let prefix = session.preview.trailing_prefix.as_deref().unwrap_or("");
        Some(
            session
                .tag_counts
                .iter()
                .filter(|(tag, _)| tag.starts_with(prefix))
                .map(|(tag, count)| (tag.as_str(), *count))
                .collect(),
        )
    }

    fn refresh_tag_counts(&mut self) {
        let Some(session) = &self.filter_session else {
            return;
        };
        let counts = self
            .tag_counts(&session.completed)
            .into_iter()
            .map(|(tag, count)| (tag.to_owned(), count))
            .collect();
        if let Some(session) = &mut self.filter_session {
            session.tag_counts = counts;
        }
    }

    /// Tags in rows matching the view, the search, and `filter`, ignoring any preview prefix.
    fn tag_counts(&self, filter: &TaskFilter) -> Vec<(&str, usize)> {
        let mut counts = HashMap::<&str, usize>::new();
        for task in self.tasks.visible_tasks().filter(|task| {
            projection::matches_with(task, &self.projection, |text, completed| {
                filter.matches(text, completed)
            })
        }) {
            for tag in metadata::distinct_tags(task.text()) {
                *counts.entry(tag).or_default() += 1;
            }
        }
        let mut counts = counts.into_iter().collect::<Vec<_>>();
        counts.sort_unstable_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
        counts
    }

    pub(crate) fn order(&self) -> TaskOrder {
        self.order
    }

    /// The transient TUI projection, in display order, context rows included. Storage
    /// visibility stays unchanged.
    pub(crate) fn visible_tasks(&self) -> impl Iterator<Item = &Task> {
        self.displayed_rows().map(|row| row.task)
    }

    /// Displayed rows with their context flag and group progress, in display order.
    pub(crate) fn displayed_rows(&self) -> impl Iterator<Item = projection::DisplayedRow<'_>> {
        projection::displayed_rows(self.ordered_live_tasks(), |task| self.matches(task))
    }

    /// The rows that match the view, search, and filter; context rows are not matches.
    pub(crate) fn match_count(&self) -> usize {
        self.displayed_rows().filter(|row| !row.context).count()
    }

    /// Live tasks in display order: canonical, or the cached Priority or Due order.
    fn ordered_live_tasks(&self) -> impl Iterator<Item = &Task> {
        let tasks = self.tasks.tasks();
        let sorted = self.sorted.as_deref();
        let canonical = sorted.is_none().then(|| self.tasks.visible_tasks());
        sorted
            .into_iter()
            .flatten()
            .map(move |&index| &tasks[index])
            .chain(canonical.into_iter().flatten())
    }

    #[cfg(test)]
    pub(crate) fn order_rebuilds(&self) -> usize {
        self.order_rebuilds
    }

    fn rebuild_order(&mut self) {
        #[cfg(test)]
        {
            self.order_rebuilds += 1;
        }
        let order = self.order;
        self.sorted = (order != TaskOrder::Manual).then(|| {
            let mut live = self
                .tasks
                .tasks()
                .iter()
                .enumerate()
                .filter(|(_, task)| !task.is_deleted())
                .collect::<Vec<_>>();
            // The same tree sort as `projection::sort_by_priority` and `sort_by_due`: groups
            // stay together, equal keys keep canonical order, and each key is computed once.
            if order == TaskOrder::Due {
                projection::sort_tree_by(&mut live, |(_, task)| task, projection::due_key);
            } else {
                projection::sort_tree_by(&mut live, |(_, task)| task, projection::priority_key);
            }
            live.into_iter().map(|(index, _)| index).collect()
        });
    }

    fn matches(&self, task: &Task) -> bool {
        match &self.filter_session {
            Some(session) => projection::matches_with(task, &self.projection, |text, completed| {
                session.preview.matches(text, completed)
            }),
            None => self.projection.matches(task),
        }
    }

    pub(crate) fn tasks(&self) -> &TaskList {
        &self.tasks
    }

    pub(crate) fn editor(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }

    pub(crate) fn notice(&self) -> Option<&Notice> {
        self.notice.as_ref()
    }

    pub(crate) fn message(&self) -> Option<&str> {
        self.durability_warning
            .as_deref()
            .or(self.storage_error.as_deref())
            .or(self.message.as_deref())
    }

    pub(crate) fn celebration(&self) -> Option<Celebration> {
        self.celebration
    }

    pub(crate) fn advance_celebration(&mut self) {
        if let Some(mut celebration) = self.celebration {
            celebration.frame = celebration.frame.saturating_add(1);
            self.celebration = (celebration.frame < CELEBRATION_FRAME_COUNT).then_some(celebration);
        }
    }

    pub(crate) fn dismiss_celebration(&mut self) {
        self.celebration = None;
    }

    fn apply_normal(&mut self, action: Action) -> Result<Transition, ListError> {
        if !matches!(
            action,
            Action::MoveDown
                | Action::MoveUp
                | Action::MoveTaskDown
                | Action::MoveTaskUp
                | Action::StartAdd
                | Action::StartEdit
                | Action::ToggleComplete
                | Action::Delete
                | Action::RestoreLatest
                | Action::StartSearch
                | Action::CycleView
                | Action::PreviousView
                | Action::ClearSearch
                | Action::ToggleSort
                | Action::EditFilter
                | Action::OpenTrash
                | Action::OpenHelp
                | Action::Quit
        ) {
            return Ok(Transition::Unchanged);
        }

        let message_was_cleared = self.clear_message();
        let transition = match action {
            Action::MoveDown => self.move_selection(MoveDirection::Down),
            Action::MoveUp => self.move_selection(MoveDirection::Up),
            Action::MoveTaskDown => self.move_task(MoveDirection::Down)?,
            Action::MoveTaskUp => self.move_task(MoveDirection::Up)?,
            Action::StartAdd => self.start_add(),
            Action::StartEdit => self.start_edit(),
            Action::ToggleComplete => self.toggle_complete()?,
            Action::Delete => self.delete_selected()?,
            Action::RestoreLatest => self.restore_latest()?,
            Action::StartSearch => self.start_search(),
            Action::CycleView => self.cycle_view(MoveDirection::Down),
            Action::PreviousView => self.cycle_view(MoveDirection::Up),
            Action::ClearSearch => self.clear_search(),
            Action::ToggleSort => self.toggle_sort(),
            Action::EditFilter => self.start_filter(),
            Action::OpenTrash => self.open_trash(),
            Action::OpenHelp => self.open_help(),
            Action::Quit => Transition::Quit,
            _ => Transition::Unchanged,
        };
        Ok(Self::after_message_clear(transition, message_was_cleared))
    }

    fn apply_trash(&mut self, action: Action) -> Result<Transition, ListError> {
        if !matches!(
            action,
            Action::MoveDown
                | Action::MoveUp
                | Action::RestoreSelected
                | Action::CloseTrash
                | Action::OpenHelp
                | Action::Quit
        ) {
            return Ok(Transition::Unchanged);
        }
        let message_was_cleared = self.clear_message();
        let transition = match action {
            Action::MoveDown => self.move_trash_selection(MoveDirection::Down),
            Action::MoveUp => self.move_trash_selection(MoveDirection::Up),
            Action::RestoreSelected => self.restore_selected()?,
            Action::CloseTrash => self.close_trash(),
            Action::OpenHelp => self.open_help(),
            Action::Quit => Transition::Quit,
            _ => Transition::Unchanged,
        };
        Ok(Self::after_message_clear(transition, message_was_cleared))
    }

    fn open_trash(&mut self) -> Transition {
        self.trash = Some(TrashState {
            selected: self.tasks.deleted_tasks().first().map(|task| task.id()),
        });
        self.mode = Mode::Trash;
        Transition::Transient
    }

    fn close_trash(&mut self) -> Transition {
        self.trash = None;
        self.mode = Mode::Normal;
        self.reconcile_selection();
        Transition::Transient
    }

    fn move_trash_selection(&mut self, direction: MoveDirection) -> Transition {
        let deleted = self.tasks.deleted_tasks();
        let Some(index) = deleted
            .iter()
            .position(|task| Some(task.id()) == self.selected())
        else {
            return Transition::Unchanged;
        };
        let next = match direction {
            MoveDirection::Up => index.checked_sub(1),
            MoveDirection::Down => Some(index + 1),
        }
        .and_then(|index| deleted.get(index))
        .map(|task| task.id());
        if let (Some(trash), Some(next)) = (&mut self.trash, next) {
            trash.selected = Some(next);
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn restore_selected(&mut self) -> Result<Transition, ListError> {
        let Some(task) = self.selected_task() else {
            self.message = Some("Nothing to restore".into());
            return Ok(Transition::Transient);
        };
        if let Some(parent) = task
            .parent_id()
            .filter(|parent| self.tasks.task(*parent).is_some_and(Task::is_deleted))
        {
            self.notice = Some(Notice::RestoreParentFirst(parent));
            return Ok(Transition::Transient);
        }
        let request = Mutation::Restore {
            id: task.id(),
            observed: Some(TaskObservation::new(task)),
        };
        Ok(self.submit(request))
    }

    fn apply_insert(&mut self, action: Action) -> Result<Transition, ListError> {
        if !matches!(
            action,
            Action::InsertChar(_)
                | Action::MoveCursorLeft
                | Action::MoveCursorRight
                | Action::MoveCursorStart
                | Action::MoveCursorEnd
                | Action::MoveWordLeft
                | Action::MoveWordRight
                | Action::DeleteBeforeCursor
                | Action::DeleteAtCursor
                | Action::DeleteWordBeforeCursor
                | Action::DeleteWordAtCursor
                | Action::CommitEdit
                | Action::CancelEdit
                | Action::Quit
        ) {
            return Ok(Transition::Unchanged);
        }

        let message_was_cleared = self.clear_message();
        let transition = match action {
            Action::InsertChar(character) => self.insert_char(character),
            Action::MoveCursorLeft => self.move_cursor_left(),
            Action::MoveCursorRight => self.move_cursor_right(),
            Action::MoveCursorStart => self.move_cursor_start(),
            Action::MoveCursorEnd => self.move_cursor_end(),
            Action::MoveWordLeft => self.move_word_left(),
            Action::MoveWordRight => self.move_word_right(),
            Action::DeleteBeforeCursor => self.delete_before_cursor(),
            Action::DeleteAtCursor => self.delete_at_cursor(),
            Action::DeleteWordBeforeCursor => self.delete_word_before_cursor(),
            Action::DeleteWordAtCursor => self.delete_word_at_cursor(),
            Action::CommitEdit => self.commit_edit()?,
            Action::CancelEdit => self.cancel_edit(),
            Action::Quit => Transition::Quit,
            _ => Transition::Unchanged,
        };
        Ok(Self::after_message_clear(transition, message_was_cleared))
    }

    fn apply_help(&mut self, action: Action) -> Result<Transition, ListError> {
        if !matches!(action, Action::CloseHelp | Action::Quit) {
            return Ok(Transition::Unchanged);
        }

        let message_was_cleared = self.clear_message();
        let transition = match action {
            Action::CloseHelp => self.close_help(),
            Action::Quit => Transition::Quit,
            _ => Transition::Unchanged,
        };
        Ok(Self::after_message_clear(transition, message_was_cleared))
    }

    fn apply_search(&mut self, action: Action) -> Result<Transition, ListError> {
        match action {
            Action::CommitEdit => {
                self.mode = Mode::Normal;
                self.editor = None;
                self.search_session = None;
                Ok(Transition::Transient)
            }
            Action::CancelEdit => {
                if let Some(session) = self.search_session.take() {
                    self.query = session.query;
                    self.projection.lowercase_query = self.query.to_lowercase();
                    self.selected = session.selected;
                    self.reconcile_selection();
                }
                self.mode = Mode::Normal;
                self.editor = None;
                Ok(Transition::Transient)
            }
            _ => {
                let transition = self.apply_insert(action)?;
                if let Some(editor) = &self.editor
                    && self.query != editor.buffer
                {
                    self.query.clone_from(&editor.buffer);
                    self.projection.lowercase_query = self.query.to_lowercase();
                    self.reconcile_selection();
                }
                Ok(transition)
            }
        }
    }

    fn apply_filter(&mut self, action: Action) -> Result<Transition, ListError> {
        match action {
            Action::CommitEdit => {
                let message_was_cleared = self.clear_message();
                Ok(Self::after_message_clear(
                    self.commit_filter(),
                    message_was_cleared,
                ))
            }
            Action::CancelEdit => {
                if let Some(session) = self.filter_session.take() {
                    self.selected = session.selected;
                }
                self.clear_message();
                self.mode = Mode::Normal;
                self.editor = None;
                self.reconcile_selection();
                Ok(Transition::Transient)
            }
            _ => {
                let transition = self.apply_insert(action)?;
                if transition == Transition::Transient {
                    self.update_filter_preview();
                }
                Ok(transition)
            }
        }
    }

    fn start_filter(&mut self) -> Transition {
        let buffer = if self.filter_text.is_empty() {
            String::new()
        } else {
            // A trailing space keeps every accepted token exact while the editor is open.
            format!("{} ", self.filter_text)
        };
        self.filter_session = Some(FilterSession {
            selected: self.selected,
            preview: FilterPreview {
                exact: self.projection.filter.clone(),
                trailing_prefix: None,
            },
            completed: self.projection.filter.clone(),
            tag_counts: Vec::new(),
        });
        self.refresh_tag_counts();
        self.editor = Some(Editor {
            kind: EditKind::Filter,
            cursor: buffer.len(),
            buffer,
            original: None,
            conflicted: false,
            invalidated: false,
        });
        self.mode = Mode::Filter;
        Transition::Transient
    }

    /// Completed tokens are exact; a trailing tag token matches by prefix. An invalid completed
    /// token keeps the last valid preview and is named in the message slot.
    fn update_filter_preview(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };
        let (tokens, trailing) = split_filter_buffer(&editor.buffer);
        let completed = match parse_filter_tokens(&tokens, self.today) {
            Ok(filter) => filter,
            Err(token) => {
                self.message = Some(invalid_filter(token));
                return;
            }
        };
        let mut exact = completed.clone();
        let mut trailing_prefix = None;
        // A trailing token that does not parse yet (`A-`, `due:2026-1`) adds no constraint.
        match trailing.and_then(metadata::parse_filter_token) {
            Some(FilterToken::Tag(tag)) => trailing_prefix = Some(tag.to_owned()),
            Some(token @ FilterToken::Due(_)) => {
                exact.add(token);
                resolve_due(&mut exact, self.today);
            }
            Some(token) => exact.add(token),
            None => {}
        }
        let mut recount = false;
        if let Some(session) = &mut self.filter_session {
            session.preview = FilterPreview {
                exact,
                trailing_prefix,
            };
            recount = session.completed != completed;
            session.completed = completed;
        }
        if recount {
            self.refresh_tag_counts();
        }
        self.reconcile_selection();
    }

    /// Enter: a trailing tag that is present is kept, a unique prefix is completed, an ambiguous
    /// one is refused, and an unknown one is accepted as typed. Other tokens are never changed.
    fn commit_filter(&mut self) -> Transition {
        let Some(editor) = &self.editor else {
            return Transition::Unchanged;
        };
        let (tokens, trailing) = split_filter_buffer(&editor.buffer);
        let completed = match parse_filter_tokens(&tokens, self.today) {
            Ok(filter) => filter,
            Err(token) => {
                self.message = Some(invalid_filter(token));
                return Transition::Transient;
            }
        };
        let mut words = tokens
            .iter()
            .map(|token| match metadata::parse_filter_token(token) {
                Some(FilterToken::Tag(tag)) => tag.to_owned(),
                _ => (*token).to_owned(),
            })
            .collect::<Vec<_>>();
        let mut filter = completed.clone();
        if let Some(token) = trailing {
            match metadata::parse_filter_token(token) {
                None => {
                    self.message = Some(invalid_filter(token));
                    return Transition::Transient;
                }
                Some(FilterToken::Tag(tag)) => match self.resolve_tag(&completed, tag) {
                    Ok(tag) => {
                        filter.add(FilterToken::Tag(&tag));
                        words.push(tag);
                    }
                    Err(candidates) => {
                        self.notice = Some(Notice::FinishTag(candidates));
                        return Transition::Transient;
                    }
                },
                // Priority and due tokens are exact and kept as typed, never completed.
                Some(token @ (FilterToken::Priorities(_) | FilterToken::Due(_))) => {
                    filter.add(token);
                    words.push(trailing.unwrap_or_default().to_owned());
                }
            }
        }
        resolve_due(&mut filter, self.today);
        self.projection.filter = filter;
        self.filter_text = words.join(" ");
        self.filter_session = None;
        self.editor = None;
        self.mode = Mode::Normal;
        self.reconcile_selection();
        Transition::Transient
    }

    fn resolve_tag(&self, completed: &TaskFilter, tag: &str) -> Result<String, Vec<String>> {
        let counts = self.tag_counts(completed);
        if counts.iter().any(|(present, _)| *present == tag) {
            return Ok(tag.to_owned());
        }
        let mut candidates = counts
            .into_iter()
            .filter(|(present, _)| present.starts_with(tag))
            .map(|(present, _)| present.to_owned())
            .collect::<Vec<_>>();
        match candidates.len() {
            0 => Ok(tag.to_owned()),
            1 => Ok(candidates.remove(0)),
            _ => Err(candidates),
        }
    }

    fn start_search(&mut self) -> Transition {
        self.search_session = Some(SearchSession {
            query: self.query.clone(),
            selected: self.selected,
        });
        self.editor = Some(Editor {
            kind: EditKind::Search,
            buffer: self.query.clone(),
            cursor: self.query.len(),
            original: None,
            conflicted: false,
            invalidated: false,
        });
        self.mode = Mode::Search;
        Transition::Transient
    }

    fn cycle_view(&mut self, direction: MoveDirection) -> Transition {
        self.projection.view = match direction {
            MoveDirection::Down => self.projection.view.next(),
            MoveDirection::Up => self.projection.view.previous(),
        };
        self.reconcile_selection();
        Transition::Transient
    }

    /// Layered: clears the search first; with no search, clears the filter.
    fn clear_search(&mut self) -> Transition {
        if !self.query.is_empty() {
            self.query.clear();
            self.projection.lowercase_query.clear();
        } else if !self.projection.filter.is_empty() {
            self.projection.filter = TaskFilter::default();
            self.filter_text.clear();
        } else {
            return Transition::Unchanged;
        }
        self.reconcile_selection();
        Transition::Transient
    }

    fn toggle_sort(&mut self) -> Transition {
        self.order = match self.order {
            TaskOrder::Manual => TaskOrder::Priority,
            TaskOrder::Priority => TaskOrder::Due,
            TaskOrder::Due => TaskOrder::Manual,
        };
        self.rebuild_order();
        self.reconcile_selection();
        Transition::Transient
    }

    fn reconcile_selection(&mut self) {
        self.selected = self.reconciled_selection();
    }

    /// Keep identity when possible, otherwise choose the next match, then the previous one,
    /// in display order.
    fn reconciled_selection(&self) -> Option<TaskId> {
        let mut previous = None;
        let mut passed_selected = self.selected.is_none();
        // Displayed rows are a subsequence of the live tasks in the same order, so one walk
        // over both finds where a hidden selection was.
        let mut displayed = self.visible_tasks().peekable();
        for task in self.ordered_live_tasks() {
            let shown = displayed.next_if(|row| row.id() == task.id()).is_some();
            if Some(task.id()) == self.selected {
                if shown {
                    return self.selected;
                }
                passed_selected = true;
            } else if shown {
                if passed_selected {
                    return Some(task.id());
                }
                previous = Some(task.id());
            }
        }
        previous
    }

    /// Canonical sibling in the projection; only reorder and the reorder guard use this.
    fn adjacent_visible(&self, selected: TaskId, direction: MoveDirection) -> Option<TaskId> {
        self.projection.adjacent(&self.tasks, selected, direction)
    }

    /// Neighbor in display order, for selection movement. It walks the displayed rows in every
    /// order, so subtasks are visited and never trap the selection in a group.
    fn adjacent_displayed(&self, selected: TaskId, direction: MoveDirection) -> Option<TaskId> {
        let mut previous = None;
        let mut tasks = self.visible_tasks();
        while let Some(task) = tasks.next() {
            if task.id() == selected {
                return match direction {
                    MoveDirection::Up => previous,
                    MoveDirection::Down => tasks.next().map(Task::id),
                };
            }
            previous = Some(task.id());
        }
        None
    }

    fn clear_message(&mut self) -> bool {
        let notice_was_cleared = self.notice.take().is_some();
        self.message.take().is_some() || notice_was_cleared
    }

    fn after_message_clear(transition: Transition, message_was_cleared: bool) -> Transition {
        if message_was_cleared && transition == Transition::Unchanged {
            Transition::Transient
        } else {
            transition
        }
    }

    fn move_selection(&mut self, direction: MoveDirection) -> Transition {
        if self.mode != Mode::Normal {
            return Transition::Unchanged;
        }
        let Some(selected) = self.selected else {
            return Transition::Unchanged;
        };
        let Some(next) = self.adjacent_displayed(selected, direction) else {
            return Transition::Unchanged;
        };
        self.selected = Some(next);
        Transition::Transient
    }

    fn submit(&mut self, mutation: Mutation) -> Transition {
        self.pending = Some(mutation);
        Transition::Submit
    }

    pub(crate) fn take_mutation(&mut self) -> Option<Mutation> {
        self.pending.take()
    }

    fn move_task(&mut self, direction: MoveDirection) -> Result<Transition, ListError> {
        if self.order != TaskOrder::Manual {
            // A swap in a sorted order has no canonical meaning, so no mutation is submitted.
            self.notice = Some(Notice::ManualOrderRequired);
            return Ok(Transition::Transient);
        }
        let Some(id) = self.selected else {
            return Ok(Transition::Unchanged);
        };
        let neighbor = self.adjacent_visible(id, direction);
        Ok(self.submit(Mutation::MoveAdjacent {
            id,
            direction,
            neighbor,
            projection: self.projection.clone(),
        }))
    }

    fn toggle_complete(&mut self) -> Result<Transition, ListError> {
        let Some(task) = self.selected_task() else {
            return Ok(Transition::Unchanged);
        };
        let mutation = Mutation::SetCompleted {
            id: task.id(),
            completed: !task.completed(),
        };
        Ok(self.submit(mutation))
    }

    fn delete_selected(&mut self) -> Result<Transition, ListError> {
        let Some(task) = self.selected_task() else {
            return Ok(Transition::Unchanged);
        };
        let mutation = Mutation::Delete {
            id: task.id(),
            observed: Some(TaskObservation::new(task)),
        };
        Ok(self.submit(mutation))
    }

    fn restore_latest(&mut self) -> Result<Transition, ListError> {
        Ok(self.submit(Mutation::RestoreLatest))
    }

    /// Install fresh data without losing the editor or transient view state.
    pub(crate) fn reconcile(&mut self, tasks: TaskList) -> bool {
        if self.tasks == tasks {
            return false;
        }
        let shown = self.visible_tasks().map(Task::id).collect::<Vec<_>>();
        let ordinal = shown
            .iter()
            .position(|id| Some(*id) == self.selected)
            .unwrap_or(0);
        let trash_ordinal = self
            .tasks
            .deleted_tasks()
            .iter()
            .position(|task| {
                self.trash
                    .as_ref()
                    .is_some_and(|trash| Some(task.id()) == trash.selected)
            })
            .unwrap_or(0);
        self.tasks = tasks;
        self.rebuild_order();
        self.refresh_tag_counts();
        let displayed = self.visible_tasks().map(Task::id).collect::<Vec<_>>();
        if !self.selected.is_some_and(|id| displayed.contains(&id)) {
            // The next row the user saw that is still shown, so a group that left together
            // does not skip the rows after it; otherwise the row now at the same position.
            self.selected = shown
                .iter()
                .skip(ordinal + 1)
                .find(|id| displayed.contains(id))
                .or_else(|| displayed.get(ordinal).or(displayed.last()))
                .copied();
        }
        if let Some(trash) = &mut self.trash {
            let deleted = self.tasks.deleted_tasks();
            if !deleted.iter().any(|task| Some(task.id()) == trash.selected) {
                trash.selected = deleted
                    .get(trash_ordinal)
                    .or_else(|| deleted.last())
                    .map(|task| task.id());
            }
        }
        if let Some(editor) = &mut self.editor
            && let EditKind::Edit(id) = editor.kind
        {
            match self.tasks.task(id).filter(|task| !task.is_deleted()) {
                Some(task) => editor.conflicted |= editor.original.as_deref() != Some(task.text()),
                None => {
                    editor.invalidated = true;
                    editor.conflicted = true;
                }
            }
        }
        if self.tasks.visible_tasks().any(|task| !task.completed()) {
            self.celebration = None;
        }
        true
    }

    pub(crate) fn finish(&mut self, request: &Mutation, tasks: TaskList, outcome: Outcome) -> bool {
        let mut dirty = self.reconcile(tasks);
        let previous = self.message.clone();
        match outcome {
            Outcome::Changed { id, celebrate } => {
                dirty |= self.finish_success(request, id, celebrate);
            }
            Outcome::AlreadyInState { id } => {
                dirty |= self.finish_success(request, id, false);
            }
            Outcome::NothingToRestore => self.message = Some("Nothing to restore".into()),
            Outcome::Conflict { id, kind } => {
                self.message = Some(match kind {
                    ConflictKind::TextChanged => {
                        format!("Task {} text changed; draft retained", id.get())
                    }
                    ConflictKind::TaskChanged => {
                        format!(
                            "Task {} changed; review before {}",
                            id.get(),
                            if matches!(request, Mutation::Restore { .. }) {
                                "restoring"
                            } else {
                                "deleting"
                            }
                        )
                    }
                    ConflictKind::OrderChanged => "Task order changed; review and retry".into(),
                    ConflictKind::NoLongerActive => {
                        format!("Task {} is no longer active", id.get())
                    }
                });
                if let Some(editor) = &mut self.editor {
                    editor.conflicted = true;
                    editor.invalidated |= kind == ConflictKind::NoLongerActive;
                }
            }
            Outcome::Rejected(error) => self.message = Some(error.to_string()),
        }
        dirty |= previous != self.message;
        dirty
    }

    fn finish_success(&mut self, request: &Mutation, id: TaskId, celebrate: bool) -> bool {
        let mut dirty = false;
        self.message = None;
        let visible = self.tasks.task(id).is_some_and(|task| self.matches(task));
        if matches!(
            request,
            Mutation::Add(_) | Mutation::RestoreLatest | Mutation::Restore { .. }
        ) {
            if visible {
                dirty |= self.selected != Some(id);
                self.selected = Some(id);
            } else {
                self.reconcile_selection();
                let action = if matches!(request, Mutation::Add(_)) {
                    "Added"
                } else {
                    "Restored"
                };
                self.message = Some(format!("{action} task is {HIDDEN}"));
            }
        } else if matches!(request, Mutation::EditText { .. }) && !visible {
            self.message = Some(format!("Edited task is {HIDDEN}"));
        }
        if matches!(request, Mutation::Add(_) | Mutation::EditText { .. }) {
            dirty |= self.editor.is_some();
            self.editor = None;
            self.mode = Mode::Normal;
        }
        if celebrate {
            self.celebration = Some(Celebration { frame: 0 });
            dirty = true;
        }
        dirty
    }

    pub(crate) fn set_message(&mut self, message: String) -> bool {
        if self.message.as_ref() == Some(&message) {
            return false;
        }
        self.message = Some(message);
        true
    }

    pub(crate) fn set_storage_error(&mut self, error: Option<String>) -> bool {
        let dirty = self.storage_error != error;
        self.storage_error = error;
        dirty
    }

    pub(crate) fn storage_unavailable(&self) -> bool {
        self.storage_error.is_some()
    }

    pub(crate) fn needs_sync(&self) -> bool {
        self.durability_warning.is_some()
    }

    pub(crate) fn await_sync(&mut self, message: String) {
        self.durability_warning = Some(message);
        self.celebration = None;
    }

    pub(crate) fn recovered(&mut self, tasks: TaskList) {
        self.reconcile(tasks);
        self.durability_warning = None;
        self.storage_error = None;
        if self.mode == Mode::Insert {
            self.editor = None;
            self.mode = Mode::Normal;
        }
    }

    // Simulate a successful fresh-list transaction for action and rendering tests.
    // Failure and stale-client tests exercise prepare/finish independently.
    #[cfg(test)]
    pub(crate) fn apply(&mut self, action: Action) -> Result<Transition, ListError> {
        let previous_message = self.message.clone();
        let previous_mode = self.mode;
        let transition = self.prepare(action)?;
        if transition == Transition::Refresh {
            return Ok(Transition::Transient);
        }
        let Some(request) = self.take_mutation() else {
            return Ok(transition);
        };
        let mut latest = self.tasks.clone();
        let outcome = request.apply(&mut latest);
        let changed = outcome.changed();
        self.finish(&request, latest, outcome);
        Ok(if changed {
            Transition::Submit
        } else if previous_message != self.message || previous_mode != self.mode {
            Transition::Transient
        } else {
            Transition::Unchanged
        })
    }

    fn start_add(&mut self) -> Transition {
        if self.mode != Mode::Normal {
            return Transition::Unchanged;
        }
        self.mode = Mode::Insert;
        self.editor = Some(Editor {
            kind: EditKind::Add,
            buffer: String::new(),
            cursor: 0,
            original: None,
            conflicted: false,
            invalidated: false,
        });
        self.message = None;
        Transition::Transient
    }

    fn start_edit(&mut self) -> Transition {
        if self.mode != Mode::Normal {
            return Transition::Unchanged;
        }
        let Some(task) = self.selected_task() else {
            return Transition::Unchanged;
        };
        let buffer = task.text().to_owned();
        let cursor = buffer.len();
        let Some(selected) = self.selected else {
            return Transition::Unchanged;
        };
        self.mode = Mode::Insert;
        self.editor = Some(Editor {
            kind: EditKind::Edit(selected),
            original: Some(buffer.clone()),
            buffer,
            cursor,
            conflicted: false,
            invalidated: false,
        });
        self.message = None;
        Transition::Transient
    }

    fn open_help(&mut self) -> Transition {
        if !matches!(self.mode, Mode::Normal | Mode::Trash) {
            return Transition::Unchanged;
        }
        self.mode = Mode::Help;
        Transition::Transient
    }

    fn close_help(&mut self) -> Transition {
        if self.mode != Mode::Help {
            return Transition::Unchanged;
        }
        self.mode = if self.is_trash_view() {
            Mode::Trash
        } else {
            Mode::Normal
        };
        Transition::Transient
    }

    fn insert_char(&mut self, character: char) -> Transition {
        if !matches!(self.mode, Mode::Insert | Mode::Search | Mode::Filter) {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        editor.insert(character);
        Transition::Transient
    }

    fn move_cursor_left(&mut self) -> Transition {
        if !matches!(self.mode, Mode::Insert | Mode::Search | Mode::Filter) {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.move_left() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn move_cursor_right(&mut self) -> Transition {
        if !matches!(self.mode, Mode::Insert | Mode::Search | Mode::Filter) {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.move_right() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn move_cursor_start(&mut self) -> Transition {
        if !matches!(self.mode, Mode::Insert | Mode::Search | Mode::Filter) {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.move_start() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn move_cursor_end(&mut self) -> Transition {
        if !matches!(self.mode, Mode::Insert | Mode::Search | Mode::Filter) {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.move_end() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn move_word_left(&mut self) -> Transition {
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.move_word_left() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn move_word_right(&mut self) -> Transition {
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.move_word_right() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn delete_before_cursor(&mut self) -> Transition {
        if !matches!(self.mode, Mode::Insert | Mode::Search | Mode::Filter) {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.delete_before_cursor() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn delete_at_cursor(&mut self) -> Transition {
        if !matches!(self.mode, Mode::Insert | Mode::Search | Mode::Filter) {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.delete_at_cursor() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn delete_word_before_cursor(&mut self) -> Transition {
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.delete_word_before_cursor() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn delete_word_at_cursor(&mut self) -> Transition {
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        if editor.delete_word_at_cursor() {
            Transition::Transient
        } else {
            Transition::Unchanged
        }
    }

    fn commit_edit(&mut self) -> Result<Transition, ListError> {
        if self.mode != Mode::Insert {
            return Ok(Transition::Unchanged);
        }
        let Some(editor) = self.editor.as_ref() else {
            return Ok(Transition::Unchanged);
        };
        if editor.buffer.trim().is_empty() {
            self.message = Some("Task text cannot be empty".into());
            return Ok(Transition::Transient);
        }

        if editor.invalidated {
            self.message = Some("Task was deleted; draft retained".into());
            return Ok(Transition::Transient);
        }
        let today = self.today;
        let text = crate::task::validated_text(&editor.buffer)?;
        let text = metadata::normalize_due(text, || today).into_owned();
        let request = match editor.kind {
            EditKind::Add => Mutation::Add(text),
            EditKind::Edit(id) => {
                if editor.original.as_deref() == Some(&text) {
                    self.mode = Mode::Normal;
                    self.editor = None;
                    return Ok(Transition::Refresh);
                }
                Mutation::EditText {
                    id,
                    text,
                    original: editor.original.clone(),
                }
            }
            EditKind::Search | EditKind::Filter => return Ok(Transition::Unchanged),
        };
        Ok(self.submit(request))
    }

    fn cancel_edit(&mut self) -> Transition {
        if self.mode != Mode::Insert {
            return Transition::Unchanged;
        }
        self.mode = Mode::Normal;
        self.editor = None;
        self.message = None;
        Transition::Transient
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn submitted_delete_should_leave_displayed_tasks_unchanged_until_committed() {
        let mut list = crate::task::TaskList::new(crate::task::ListScope::Global);
        list.add("keep until saved").unwrap();
        let before = list.clone();
        let mut app = super::App::new(list);
        app.prepare(crate::action::Action::Delete).unwrap();
        assert_eq!(app.tasks(), &before);
    }
    use crate::{
        action::Action,
        task::{ListScope, TaskList},
    };

    use super::{App, Mode, Transition};

    fn app_with_editor(text: &str) -> App {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();
        for character in text.chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
        app
    }

    #[test]
    fn trash_should_open_at_newest_and_preserve_normal_selection_on_return() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("live first").unwrap();
        let live_second = list.add("live second").unwrap();
        let older = list.add("older").unwrap();
        let newest = list.add("newest").unwrap();
        list.delete(older).unwrap();
        list.delete(newest).unwrap();
        let mut app = App::new(list);
        app.apply(Action::MoveDown).unwrap();
        let before = app.tasks().clone();

        assert_eq!(app.apply(Action::OpenTrash).unwrap(), Transition::Transient);
        assert_eq!(app.mode(), Mode::Trash);
        assert_eq!(app.selected(), Some(newest));
        assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Unchanged);
        assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Transient);
        assert_eq!(app.selected(), Some(older));
        assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Unchanged);
        assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Transient);
        assert_eq!(app.selected(), Some(newest));
        app.apply(Action::CloseTrash).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.selected(), Some(live_second));
        assert_eq!(app.tasks(), &before);
    }

    #[test]
    fn selective_restore_should_stay_in_trash_select_neighbors_and_empty_safely() {
        let mut list = TaskList::new(ListScope::Global);
        let oldest = list.add("oldest").unwrap();
        let middle = list.add("middle").unwrap();
        let newest = list.add("newest").unwrap();
        for id in [oldest, middle, newest] {
            list.delete(id).unwrap();
        }
        let mut app = App::new(list);
        app.apply(Action::OpenTrash).unwrap();
        app.apply(Action::MoveDown).unwrap();

        assert_eq!(
            app.apply(Action::RestoreSelected).unwrap(),
            Transition::Submit
        );
        assert_eq!(app.selected(), Some(oldest));
        assert_eq!(app.mode(), Mode::Trash);
        assert_eq!(
            app.apply(Action::RestoreSelected).unwrap(),
            Transition::Submit
        );
        assert_eq!(app.selected(), Some(newest));
        assert_eq!(
            app.apply(Action::RestoreSelected).unwrap(),
            Transition::Submit
        );
        assert_eq!(app.selected(), None);
        assert_eq!(app.mode(), Mode::Trash);
        assert_eq!(
            app.apply(Action::RestoreSelected).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.message(), Some("Nothing to restore"));
        app.apply(Action::CloseTrash).unwrap();
        assert_eq!(app.selected(), Some(newest));
        assert_eq!(app.message(), None);
        assert_eq!(
            app.tasks()
                .visible_tasks()
                .map(|task| task.id())
                .collect::<Vec<_>>(),
            vec![oldest, middle, newest]
        );
        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn empty_trash_should_allow_navigation_help_and_return_without_saving() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::OpenTrash).unwrap();
        assert_eq!(app.selected(), None);
        assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Unchanged);
        assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Unchanged);
        assert_eq!(
            app.apply(Action::RestoreSelected).unwrap(),
            Transition::Transient
        );
        app.apply(Action::OpenHelp).unwrap();
        app.apply(Action::CloseHelp).unwrap();
        assert_eq!(app.mode(), Mode::Trash);
        app.apply(Action::CloseTrash).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.selected(), None);
    }

    #[test]
    fn trash_should_block_all_normal_mutations_and_editor_actions() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("deleted").unwrap();
        list.delete(id).unwrap();
        let mut app = App::new(list);
        app.apply(Action::OpenTrash).unwrap();
        let before = app.tasks().clone();
        for action in [
            Action::StartAdd,
            Action::StartEdit,
            Action::ToggleComplete,
            Action::Delete,
            Action::MoveTaskDown,
            Action::MoveTaskUp,
            Action::RestoreLatest,
            Action::InsertChar('x'),
            Action::CommitEdit,
            Action::CancelEdit,
            Action::OpenTrash,
        ] {
            assert_eq!(
                app.apply(action).unwrap(),
                Transition::Unchanged,
                "unexpected trash action: {action:?}"
            );
        }
        assert_eq!(app.tasks(), &before);
        assert_eq!(app.mode(), Mode::Trash);
        assert!(app.editor().is_none());
        assert_eq!(app.selected(), Some(id));
        assert_eq!(app.apply(Action::Quit).unwrap(), Transition::Quit);
    }

    #[test]
    fn help_from_trash_should_preserve_selection_and_block_restoration_until_closed() {
        let mut list = TaskList::new(ListScope::Global);
        let older = list.add("older").unwrap();
        let newer = list.add("newer").unwrap();
        list.delete(older).unwrap();
        list.delete(newer).unwrap();
        let mut app = App::new(list);
        app.apply(Action::OpenTrash).unwrap();
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::OpenHelp).unwrap();
        assert!(app.is_trash_view());
        for action in [
            Action::RestoreSelected,
            Action::CloseTrash,
            Action::MoveDown,
        ] {
            assert_eq!(app.apply(action).unwrap(), Transition::Unchanged);
        }
        app.apply(Action::CloseHelp).unwrap();
        assert_eq!(app.mode(), Mode::Trash);
        assert_eq!(app.selected(), Some(older));
        app.apply(Action::RestoreSelected).unwrap();
        assert!(!app.tasks().task(older).unwrap().is_deleted());
        assert!(app.tasks().task(newer).unwrap().is_deleted());
    }

    #[test]
    fn trash_actions_should_not_leak_into_normal_or_insert_modes() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        for action in [Action::RestoreSelected, Action::CloseTrash] {
            assert_eq!(app.apply(action).unwrap(), Transition::Unchanged);
        }
        app.apply(Action::StartAdd).unwrap();
        for action in [
            Action::OpenTrash,
            Action::RestoreSelected,
            Action::CloseTrash,
        ] {
            assert_eq!(app.apply(action).unwrap(), Transition::Unchanged);
        }
        assert_eq!(app.mode(), Mode::Insert);
        assert!(!app.is_trash_view());
    }

    #[test]
    fn add_editor_should_commit_trimmed_text_and_select_new_task() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();
        for character in "  ship it  ".chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }

        assert_eq!(app.apply(Action::CommitEdit).unwrap(), Transition::Submit);
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.selected_task().unwrap().text(), "ship it");
    }

    #[test]
    fn edit_editor_should_cancel_without_mutating_task() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("original").unwrap();
        let mut app = App::new(list);
        app.apply(Action::StartEdit).unwrap();
        app.apply(Action::InsertChar('!')).unwrap();

        assert_eq!(
            app.apply(Action::CancelEdit).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.tasks().task(id).unwrap().text(), "original");
    }

    #[test]
    fn navigation_should_follow_visible_tasks_by_identity() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        let mut app = App::new(list);

        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(second));
        app.apply(Action::MoveUp).unwrap();
        assert_eq!(app.selected(), Some(first));
    }

    #[test]
    fn editor_should_move_and_delete_on_utf8_char_boundaries() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();
        for character in "aéc".chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
        app.apply(Action::MoveCursorLeft).unwrap();
        app.apply(Action::DeleteBeforeCursor).unwrap();
        app.apply(Action::MoveCursorStart).unwrap();
        app.apply(Action::DeleteAtCursor).unwrap();
        app.apply(Action::MoveCursorEnd).unwrap();
        app.apply(Action::InsertChar('!')).unwrap();
        app.apply(Action::CommitEdit).unwrap();

        assert_eq!(app.selected_task().unwrap().text(), "c!");
    }

    #[test]
    fn editor_should_move_left_to_previous_unicode_word_starts_across_punctuation() {
        let mut app = app_with_editor("café API-v2");

        assert_eq!(
            app.apply(Action::MoveWordLeft).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.editor().unwrap().cursor(), 10);
        assert_eq!(
            app.apply(Action::MoveWordLeft).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.editor().unwrap().cursor(), 6);
    }

    #[test]
    fn editor_should_move_right_to_unicode_word_ends_across_punctuation() {
        let mut app = app_with_editor("café API-v2");
        app.apply(Action::MoveCursorStart).unwrap();

        assert_eq!(
            app.apply(Action::MoveWordRight).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.editor().unwrap().cursor(), 5);
        assert_eq!(
            app.apply(Action::MoveWordRight).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.editor().unwrap().cursor(), 9);
    }

    #[test]
    fn editor_should_delete_the_previous_word_without_crossing_punctuation() {
        let mut app = app_with_editor("ship API-v2");

        assert_eq!(
            app.apply(Action::DeleteWordBeforeCursor).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.editor().unwrap().buffer(), "ship API-");
        assert_eq!(app.editor().unwrap().cursor(), 9);
    }

    #[test]
    fn editor_should_delete_the_next_word_from_the_cursor() {
        let mut app = app_with_editor("ship API-v2");
        app.apply(Action::MoveCursorStart).unwrap();

        assert_eq!(
            app.apply(Action::DeleteWordAtCursor).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.editor().unwrap().buffer(), " API-v2");
        assert_eq!(app.editor().unwrap().cursor(), 0);
    }

    #[test]
    fn editor_should_delete_separator_space_with_the_previous_word() {
        let mut app = app_with_editor("ship   ");

        app.apply(Action::DeleteWordBeforeCursor).unwrap();

        assert_eq!(app.editor().unwrap().buffer(), "");
    }

    #[test]
    fn blank_commit_should_remain_in_insert_mode_with_validation_message() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();

        assert_eq!(
            app.apply(Action::CommitEdit).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.mode(), Mode::Insert);
        assert_eq!(app.message(), Some("Task text cannot be empty"));
    }

    #[test]
    fn editor_boundary_actions_should_be_unchanged_when_buffer_is_empty() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();

        for action in [
            Action::MoveCursorLeft,
            Action::MoveCursorRight,
            Action::MoveCursorStart,
            Action::MoveCursorEnd,
            Action::DeleteBeforeCursor,
            Action::DeleteAtCursor,
            Action::MoveWordLeft,
            Action::MoveWordRight,
            Action::DeleteWordBeforeCursor,
            Action::DeleteWordAtCursor,
        ] {
            assert_eq!(app.apply(action).unwrap(), Transition::Unchanged);
        }
    }

    #[test]
    fn editor_should_report_utf8_rightward_cursor_movement_only_when_it_advances() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();
        app.apply(Action::InsertChar('é')).unwrap();
        assert_eq!(
            app.apply(Action::MoveCursorStart).unwrap(),
            Transition::Transient
        );

        assert_eq!(
            app.apply(Action::MoveCursorRight).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.editor().unwrap().cursor(), 'é'.len_utf8());
        assert_eq!(
            app.apply(Action::MoveCursorRight).unwrap(),
            Transition::Unchanged
        );
    }

    #[test]
    fn delete_should_select_next_and_restore_should_reselect_tombstone() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        let mut app = App::new(list);

        assert_eq!(app.apply(Action::Delete).unwrap(), Transition::Submit);
        assert_eq!(app.selected(), Some(second));
        assert_eq!(
            app.apply(Action::RestoreLatest).unwrap(),
            Transition::Submit
        );
        assert_eq!(app.selected(), Some(first));
    }

    #[test]
    fn restore_without_tombstone_should_show_message_without_persisting() {
        let mut app = App::new(TaskList::new(ListScope::Global));

        assert_eq!(
            app.apply(Action::RestoreLatest).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.message(), Some("Nothing to restore"));
    }

    #[test]
    fn help_should_block_task_actions_until_closed() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::OpenHelp).unwrap();

        assert_eq!(
            app.apply(Action::ToggleComplete).unwrap(),
            Transition::Unchanged
        );
        assert!(!app.tasks().task(id).unwrap().completed());
        assert_eq!(app.apply(Action::CloseHelp).unwrap(), Transition::Transient);
    }

    #[test]
    fn completion_and_reordering_should_persist_only_real_changes() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        list.add("second").unwrap();
        let mut app = App::new(list);

        assert_eq!(
            app.apply(Action::ToggleComplete).unwrap(),
            Transition::Submit
        );
        assert!(app.tasks().task(first).unwrap().completed());
        assert_eq!(
            app.apply(Action::MoveTaskUp).unwrap(),
            Transition::Unchanged
        );
        assert_eq!(app.apply(Action::MoveTaskDown).unwrap(), Transition::Submit);
        assert_eq!(app.selected(), Some(first));
    }

    #[test]
    fn reopening_a_task_should_clear_the_active_celebration() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();

        app.apply(Action::ToggleComplete).unwrap();

        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn completing_a_task_should_not_celebrate_while_another_task_is_open() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("first").unwrap();
        list.add("second").unwrap();
        let mut app = App::new(list);

        app.apply(Action::ToggleComplete).unwrap();

        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn loading_an_already_completed_list_should_not_celebrate() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("task").unwrap();
        list.toggle_complete(id).unwrap();

        let app = App::new(list);

        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn deleting_the_final_open_task_should_not_celebrate() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);

        app.apply(Action::Delete).unwrap();

        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn recompleting_a_reopened_task_should_start_a_new_celebration() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();
        app.dismiss_celebration();
        app.apply(Action::ToggleComplete).unwrap();

        app.apply(Action::ToggleComplete).unwrap();

        assert_eq!(app.celebration().unwrap().frame(), 0);
    }

    #[test]
    fn celebration_should_finish_after_twenty_four_frames() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();

        for _ in 0..24 {
            app.advance_celebration();
        }

        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn dismissing_celebration_should_restore_the_normal_interface() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();

        app.dismiss_celebration();

        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn unchanged_edit_should_close_without_persisting() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::StartEdit).unwrap();

        assert_eq!(
            app.apply(Action::CommitEdit).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn valid_noop_action_should_clear_previous_message() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::RestoreLatest).unwrap();

        assert_eq!(app.message(), Some("Nothing to restore"));
        assert_eq!(
            app.apply(Action::MoveTaskUp).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.message(), None);
    }

    #[test]
    fn insert_action_should_clear_blank_editor_message() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartAdd).unwrap();
        app.apply(Action::CommitEdit).unwrap();

        assert_eq!(app.message(), Some("Task text cannot be empty"));
        assert_eq!(
            app.apply(Action::InsertChar('t')).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.message(), None);
    }
}

#[cfg(test)]
mod view_tests {
    use super::{App, Mode, TaskView, Transition};
    use crate::{
        action::Action,
        task::{ListScope, TaskId, TaskList},
    };

    fn replace_buffer(app: &mut App, text: &str) {
        let length = app.editor().unwrap().buffer().chars().count();
        app.apply(Action::MoveCursorStart).unwrap();
        for _ in 0..length {
            app.apply(Action::DeleteAtCursor).unwrap();
        }
        for character in text.chars() {
            assert_eq!(
                app.apply(Action::InsertChar(character)).unwrap(),
                Transition::Transient
            );
        }
    }

    fn search(app: &mut App, query: &str) {
        assert_eq!(
            app.apply(Action::StartSearch).unwrap(),
            Transition::Transient
        );
        replace_buffer(app, query);
        assert_eq!(
            app.apply(Action::CommitEdit).unwrap(),
            Transition::Transient
        );
    }

    fn visible_ids(app: &App) -> Vec<TaskId> {
        app.visible_tasks().map(|task| task.id()).collect()
    }

    #[test]
    fn views_and_search_should_be_transient_and_reset_on_relaunch() {
        let mut list = TaskList::new(ListScope::Global);
        let done = list.add("done task").unwrap();
        list.add("open task").unwrap();
        list.toggle_complete(done).unwrap();
        let snapshot = list.clone();
        let mut app = App::new(list);
        assert_eq!(app.view(), TaskView::All);
        assert_eq!(app.query(), "");
        search(&mut app, "task");
        for view in [TaskView::Open, TaskView::Done, TaskView::All] {
            assert_eq!(app.apply(Action::CycleView).unwrap(), Transition::Transient);
            assert_eq!(app.view(), view);
        }
        assert_eq!(app.tasks(), &snapshot);
        let reopened = App::new(app.tasks().clone());
        assert_eq!(reopened.view(), TaskView::All);
        assert_eq!(reopened.query(), "");
        assert_eq!(reopened.selected(), Some(done));
    }

    #[test]
    fn live_search_should_match_unicode_lowercase_substrings_and_exclude_tombstones() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("Ship CAFÉ today").unwrap();
        let hidden = list.add("café deleted").unwrap();
        list.add("Other work").unwrap();
        let last = list.add("Visit café").unwrap();
        list.delete(hidden).unwrap();
        let mut app = App::new(list);
        app.apply(Action::StartSearch).unwrap();
        replace_buffer(&mut app, "CaFÉ");
        assert_eq!(app.mode(), Mode::Search);
        assert_eq!(app.query(), "CaFÉ");
        assert_eq!(visible_ids(&app), vec![first, last]);
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(visible_ids(&app), vec![first, last]);
        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(last));
        app.apply(Action::MoveUp).unwrap();
        assert_eq!(app.selected(), Some(first));
    }

    #[test]
    fn search_and_state_should_compose_without_sorting_or_clearing_query() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("MATCH open").unwrap();
        list.add("other open").unwrap();
        let done = list.add("match done").unwrap();
        list.toggle_complete(done).unwrap();
        let last = list.add("match later").unwrap();
        let mut app = App::new(list);
        search(&mut app, "match");
        app.apply(Action::CycleView).unwrap();
        assert_eq!(visible_ids(&app), vec![first, last]);
        app.apply(Action::CycleView).unwrap();
        assert_eq!(visible_ids(&app), vec![done]);
        assert_eq!(app.selected(), Some(done));
        app.apply(Action::CycleView).unwrap();
        assert_eq!(visible_ids(&app), vec![first, done, last]);
        assert_eq!(app.selected(), Some(done));
        assert_eq!(app.query(), "match");
    }

    #[test]
    fn previous_view_should_wrap_and_preserve_matching_selection_and_order() {
        let mut list = TaskList::new(ListScope::Global);
        let open = list.add("match open").unwrap();
        let done = list.add("match done").unwrap();
        list.toggle_complete(done).unwrap();
        list.add("hidden work").unwrap();
        let snapshot = list.clone();
        let mut app = App::new(list);
        search(&mut app, "match");
        for (view, selected, ids) in [
            (TaskView::Done, done, vec![done]),
            (TaskView::Open, open, vec![open]),
            (TaskView::All, open, vec![open, done]),
        ] {
            assert_eq!(
                app.apply(Action::PreviousView).unwrap(),
                Transition::Transient
            );
            assert_eq!(app.view(), view);
            assert_eq!(app.selected(), Some(selected));
            assert_eq!(visible_ids(&app), ids);
        }
        assert_eq!(app.tasks(), &snapshot);
        assert_eq!(app.query(), "match");
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::PreviousView).unwrap();
        assert_eq!(app.view(), TaskView::All);
        assert_eq!(app.selected(), Some(open));
    }

    #[test]
    fn previous_view_should_handle_empty_matches_and_recover_selection() {
        let mut list = TaskList::new(ListScope::Global);
        let open = list.add("open task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::PreviousView).unwrap();
        assert_eq!(app.view(), TaskView::Done);
        assert_eq!(app.selected(), None);
        app.apply(Action::PreviousView).unwrap();
        assert_eq!(app.view(), TaskView::Open);
        assert_eq!(app.selected(), Some(open));
        search(&mut app, "absent");
        for _ in 0..3 {
            app.apply(Action::PreviousView).unwrap();
            assert_eq!(app.selected(), None);
            assert!(visible_ids(&app).is_empty());
            assert!(app.celebration().is_none());
        }
        app.apply(Action::ClearSearch).unwrap();
        assert_eq!(app.selected(), Some(open));
    }

    #[test]
    fn trash_restore_should_preserve_live_selection_when_query_or_state_hides_restored_task() {
        for (text, completed) in [("other work", false), ("match done", true)] {
            let mut list = TaskList::new(ListScope::Global);
            let live = list.add("match open").unwrap();
            let deleted = list.add(text).unwrap();
            list.set_completed(deleted, completed).unwrap();
            list.delete(deleted).unwrap();
            let mut app = App::new(list);
            search(&mut app, "match");
            app.apply(Action::CycleView).unwrap();
            app.apply(Action::OpenTrash).unwrap();
            assert_eq!(app.selected_task().unwrap().id(), deleted);
            app.apply(Action::OpenHelp).unwrap();
            app.apply(Action::CloseHelp).unwrap();
            assert_eq!(app.mode(), Mode::Trash);
            assert_eq!(app.selected_task().unwrap().id(), deleted);
            assert_eq!(
                app.apply(Action::RestoreSelected).unwrap(),
                Transition::Submit
            );
            assert_eq!(app.selected(), None);
            assert!(app.message().unwrap().contains("hidden"));
            app.apply(Action::CloseTrash).unwrap();
            assert_eq!(app.view(), TaskView::Open);
            assert_eq!(app.query(), "match");
            assert_eq!(app.selected(), Some(live));
            assert_eq!(visible_ids(&app), vec![live]);
            assert!(!app.tasks().task(deleted).unwrap().is_deleted());
            assert_eq!(app.tasks().task(deleted).unwrap().completed(), completed);
        }
    }

    #[test]
    fn matching_trash_restore_should_select_restored_identity_after_returning_to_live_view() {
        let mut list = TaskList::new(ListScope::Global);
        let live = list.add("match live").unwrap();
        let restored = list.add("match restored").unwrap();
        let remaining = list.add("unrelated deleted").unwrap();
        list.delete(remaining).unwrap();
        list.delete(restored).unwrap();
        let mut app = App::new(list);
        search(&mut app, "match");
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::OpenTrash).unwrap();
        app.apply(Action::RestoreSelected).unwrap();
        assert_eq!(app.selected_task().unwrap().id(), remaining);
        app.apply(Action::CloseTrash).unwrap();
        assert_eq!(app.selected_task().unwrap().id(), restored);
        assert_eq!(app.query(), "match");
        assert_eq!(app.view(), TaskView::Open);
        assert_eq!(
            app.tasks()
                .tasks()
                .iter()
                .map(|task| task.id())
                .collect::<Vec<_>>(),
            vec![live, restored, remaining]
        );
        assert!(app.celebration().is_none());
    }

    #[test]
    fn trash_should_ignore_search_controls_and_return_safely_to_an_empty_filtered_view() {
        let mut list = TaskList::new(ListScope::Global);
        let deleted = list.add("deleted work").unwrap();
        list.delete(deleted).unwrap();
        let mut app = App::new(list);
        search(&mut app, "absent");
        app.apply(Action::PreviousView).unwrap();
        app.apply(Action::OpenTrash).unwrap();
        for action in [
            Action::StartSearch,
            Action::CycleView,
            Action::PreviousView,
            Action::ClearSearch,
        ] {
            assert_eq!(app.apply(action).unwrap(), Transition::Unchanged);
            assert_eq!(app.mode(), Mode::Trash);
            assert_eq!(app.selected_task().unwrap().id(), deleted);
        }
        app.apply(Action::RestoreSelected).unwrap();
        app.apply(Action::CloseTrash).unwrap();
        assert_eq!(app.view(), TaskView::Done);
        assert_eq!(app.query(), "absent");
        assert_eq!(app.selected(), None);
        assert!(visible_ids(&app).is_empty());
        app.apply(Action::ClearSearch).unwrap();
        assert_eq!(app.selected(), None);
        app.apply(Action::PreviousView).unwrap();
        assert_eq!(app.selected(), Some(deleted));
    }

    #[test]
    fn cycling_view_should_preserve_identity_or_choose_next_then_previous_match() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("open first").unwrap();
        let second = list.add("done middle").unwrap();
        let third = list.add("open last").unwrap();
        let fourth = list.add("done last").unwrap();
        list.toggle_complete(second).unwrap();
        list.toggle_complete(fourth).unwrap();
        let mut app = App::new(list);
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::CycleView).unwrap();
        assert_eq!(app.selected(), Some(third));
        app.apply(Action::CycleView).unwrap();
        assert_eq!(app.selected(), Some(fourth));
        app.apply(Action::CycleView).unwrap();
        assert_eq!(app.selected(), Some(fourth));
        app.apply(Action::CycleView).unwrap();
        assert_eq!(app.selected(), Some(third));
        app.apply(Action::MoveUp).unwrap();
        assert_eq!(app.selected(), Some(first));
    }

    #[test]
    fn cancelling_search_should_restore_previous_query_and_selected_identity() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("match one").unwrap();
        let second = list.add("match two").unwrap();
        let other = list.add("other").unwrap();
        let mut app = App::new(list);
        search(&mut app, "match");
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::StartSearch).unwrap();
        replace_buffer(&mut app, "other");
        assert_eq!(app.selected(), Some(other));
        assert_eq!(
            app.apply(Action::CancelEdit).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.query(), "match");
        assert_eq!(app.selected(), Some(second));
        assert_eq!(app.mode(), Mode::Normal);
        assert!(app.editor().is_none());
    }

    #[test]
    fn clearing_or_accepting_an_empty_search_should_preserve_state_filter() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        let mut app = App::new(list);
        app.apply(Action::CycleView).unwrap();
        search(&mut app, "second");
        assert_eq!(
            app.apply(Action::ClearSearch).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.view(), TaskView::Open);
        assert_eq!(visible_ids(&app), vec![first, second]);
        assert_eq!(app.selected(), Some(second));
        assert_eq!(
            app.apply(Action::ClearSearch).unwrap(),
            Transition::Unchanged
        );
        search(&mut app, "missing");
        assert_eq!(app.selected(), None);
        search(&mut app, "");
        assert_eq!(app.selected(), Some(first));
        assert_eq!(app.view(), TaskView::Open);
    }

    #[test]
    fn search_editor_should_support_unicode_cursor_word_and_delete_actions() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::StartSearch).unwrap();
        replace_buffer(&mut app, "café needle");
        app.apply(Action::DeleteWordBeforeCursor).unwrap();
        assert_eq!(app.query(), "café ");
        app.apply(Action::MoveCursorStart).unwrap();
        app.apply(Action::MoveWordRight).unwrap();
        app.apply(Action::DeleteBeforeCursor).unwrap();
        assert_eq!(app.query(), "caf ");
        app.apply(Action::MoveCursorLeft).unwrap();
        app.apply(Action::MoveCursorRight).unwrap();
        app.apply(Action::MoveWordLeft).unwrap();
        app.apply(Action::DeleteWordAtCursor).unwrap();
        assert_eq!(app.query(), " ");
        app.apply(Action::MoveCursorEnd).unwrap();
        app.apply(Action::DeleteBeforeCursor).unwrap();
        assert_eq!(app.query(), "");
    }

    #[test]
    fn search_and_help_should_block_task_and_view_mutations() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("task").unwrap();
        let snapshot = list.clone();
        let mut app = App::new(list);
        for open in [Action::StartSearch, Action::OpenHelp] {
            app.apply(open).unwrap();
            for action in [
                Action::ToggleComplete,
                Action::Delete,
                Action::RestoreLatest,
                Action::StartAdd,
                Action::StartEdit,
                Action::CycleView,
                Action::PreviousView,
                Action::ClearSearch,
                Action::MoveTaskDown,
                Action::MoveDown,
            ] {
                assert_eq!(app.apply(action).unwrap(), Transition::Unchanged);
            }
            assert_eq!(app.tasks(), &snapshot);
            app.apply(if open == Action::StartSearch {
                Action::CancelEdit
            } else {
                Action::CloseHelp
            })
            .unwrap();
        }
    }

    #[test]
    fn completing_in_open_view_should_select_next_then_previous_and_celebrate_actual_final_task() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        let third = list.add("third").unwrap();
        let mut app = App::new(list);
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::ToggleComplete).unwrap();
        assert_eq!(app.selected(), Some(third));
        assert!(app.tasks().task(second).unwrap().completed());
        app.apply(Action::ToggleComplete).unwrap();
        assert_eq!(app.selected(), Some(first));
        assert!(app.celebration().is_none());
        app.apply(Action::ToggleComplete).unwrap();
        assert_eq!(app.selected(), None);
        assert!(app.celebration().is_some());
    }

    #[test]
    fn reopening_in_done_view_should_select_another_done_task() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("first").unwrap();
        let second = list.add("second").unwrap();
        list.toggle_complete(first).unwrap();
        list.toggle_complete(second).unwrap();
        let mut app = App::new(list);
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::ToggleComplete).unwrap();
        assert_eq!(app.selected(), Some(second));
        assert_eq!(visible_ids(&app), vec![second]);
        assert!(!app.tasks().task(first).unwrap().completed());
        assert!(app.celebration().is_none());
    }

    #[test]
    fn completing_last_search_match_should_not_celebrate_with_hidden_open_tasks() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("match").unwrap();
        list.add("hidden open").unwrap();
        let mut app = App::new(list);
        search(&mut app, "match");
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::ToggleComplete).unwrap();
        assert!(app.celebration().is_none());
        assert_eq!(app.selected(), None);
        assert!(visible_ids(&app).is_empty());
        app.apply(Action::ClearSearch).unwrap();
        app.apply(Action::ToggleComplete).unwrap();
        assert!(app.celebration().is_some());
    }

    #[test]
    fn filtered_delete_should_select_only_matching_neighbors_and_restore_identity() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("match first").unwrap();
        let hidden = list.add("hidden").unwrap();
        let second = list.add("match second").unwrap();
        let third = list.add("match third").unwrap();
        let mut app = App::new(list);
        search(&mut app, "match");
        app.apply(Action::MoveDown).unwrap();
        app.apply(Action::Delete).unwrap();
        assert_eq!(app.selected(), Some(third));
        app.apply(Action::Delete).unwrap();
        assert_eq!(app.selected(), Some(first));
        app.apply(Action::Delete).unwrap();
        assert_eq!(app.selected(), None);
        assert!(!app.tasks().task(hidden).unwrap().is_deleted());
        app.apply(Action::RestoreLatest).unwrap();
        assert_eq!(app.selected(), Some(first));
        app.apply(Action::RestoreLatest).unwrap();
        assert_eq!(app.selected(), Some(third));
        app.apply(Action::RestoreLatest).unwrap();
        assert_eq!(app.selected(), Some(second));
    }

    #[test]
    fn restoring_a_hidden_task_should_keep_selection_and_explain_the_result() {
        let mut list = TaskList::new(ListScope::Global);
        let hidden = list.add("hidden").unwrap();
        let selected = list.add("match").unwrap();
        list.delete(hidden).unwrap();
        let mut app = App::new(list);
        search(&mut app, "match");
        assert_eq!(
            app.apply(Action::RestoreLatest).unwrap(),
            Transition::Submit
        );
        assert_eq!(app.selected(), Some(selected));
        assert_eq!(visible_ids(&app), vec![selected]);
        assert!(!app.tasks().task(hidden).unwrap().is_deleted());
        assert_eq!(
            app.message(),
            Some("Restored task is hidden by the current view, search, or filter")
        );
    }

    #[test]
    fn restoring_a_state_hidden_task_into_an_empty_view_should_not_select_it() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("open").unwrap();
        list.delete(id).unwrap();
        let mut app = App::new(list);
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::RestoreLatest).unwrap();
        assert_eq!(app.selected(), None);
        assert!(visible_ids(&app).is_empty());
        assert!(!app.tasks().task(id).unwrap().is_deleted());
        app.apply(Action::CycleView).unwrap();
        assert_eq!(app.selected(), Some(id));
    }

    #[test]
    fn editing_out_of_search_should_select_next_match_without_touching_hidden_tasks() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("match first").unwrap();
        let hidden = list.add("hidden").unwrap();
        let second = list.add("match second").unwrap();
        let mut app = App::new(list);
        search(&mut app, "match");
        app.apply(Action::StartEdit).unwrap();
        replace_buffer(&mut app, "different");
        assert_eq!(app.selected(), Some(first));
        assert_eq!(app.apply(Action::CommitEdit).unwrap(), Transition::Submit);
        assert_eq!(app.selected(), Some(second));
        assert_eq!(app.tasks().task(first).unwrap().text(), "different");
        assert_eq!(app.tasks().task(hidden).unwrap().text(), "hidden");
        assert_eq!(
            app.message(),
            Some("Edited task is hidden by the current view, search, or filter")
        );
        app.apply(Action::StartEdit).unwrap();
        replace_buffer(&mut app, "MATCH still here");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.selected(), Some(second));
    }

    #[test]
    fn adding_matching_and_hidden_tasks_should_keep_selection_in_the_projection() {
        let mut list = TaskList::new(ListScope::Global);
        let original = list.add("match original").unwrap();
        let mut app = App::new(list);
        search(&mut app, "match");
        app.apply(Action::StartAdd).unwrap();
        replace_buffer(&mut app, "hidden new");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.selected(), Some(original));
        assert_eq!(
            app.message(),
            Some("Added task is hidden by the current view, search, or filter")
        );
        app.apply(Action::StartAdd).unwrap();
        replace_buffer(&mut app, "match new");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.selected_task().unwrap().text(), "match new");
        assert_eq!(visible_ids(&app).len(), 2);
        assert_eq!(app.tasks().visible_tasks().count(), 3);
    }

    #[test]
    fn adding_an_open_task_in_done_view_should_leave_empty_selection() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::StartAdd).unwrap();
        replace_buffer(&mut app, "new open");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.selected(), None);
        assert_eq!(app.tasks().visible_tasks().count(), 1);
        assert_eq!(app.view(), TaskView::Done);
    }

    #[test]
    fn filtered_reorder_should_swap_matching_slots_and_leave_hidden_rows_and_tombstones_fixed() {
        let mut list = TaskList::new(ListScope::Global);
        let first = list.add("match first").unwrap();
        let done = list.add("match done").unwrap();
        let deleted = list.add("match deleted").unwrap();
        let other = list.add("other open").unwrap();
        let second = list.add("match second").unwrap();
        let third = list.add("match third").unwrap();
        list.toggle_complete(done).unwrap();
        list.delete(deleted).unwrap();
        let snapshot = list.clone();
        let mut app = App::new(list);
        search(&mut app, "match");
        app.apply(Action::CycleView).unwrap();
        assert_eq!(
            app.apply(Action::MoveTaskUp).unwrap(),
            Transition::Unchanged
        );
        assert_eq!(app.apply(Action::MoveTaskDown).unwrap(), Transition::Submit);
        assert_eq!(
            app.tasks()
                .tasks()
                .iter()
                .map(|task| task.id())
                .collect::<Vec<_>>(),
            vec![second, done, deleted, other, first, third]
        );
        assert_eq!(app.selected(), Some(first));
        assert_eq!(visible_ids(&app), vec![second, first, third]);
        app.apply(Action::MoveTaskUp).unwrap();
        assert_eq!(app.tasks(), &snapshot);
        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(second));
    }

    #[test]
    fn single_match_and_empty_view_should_ignore_navigation_reorder_edit_toggle_and_delete() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("match").unwrap();
        list.add("hidden").unwrap();
        let snapshot = list.clone();
        let mut app = App::new(list);
        search(&mut app, "match");
        for action in [
            Action::MoveDown,
            Action::MoveUp,
            Action::MoveTaskDown,
            Action::MoveTaskUp,
        ] {
            assert_eq!(app.apply(action).unwrap(), Transition::Unchanged);
            assert_eq!(app.selected(), Some(id));
        }
        search(&mut app, "missing");
        for action in [
            Action::MoveDown,
            Action::MoveUp,
            Action::MoveTaskDown,
            Action::MoveTaskUp,
            Action::StartEdit,
            Action::ToggleComplete,
            Action::Delete,
        ] {
            assert_eq!(app.apply(action).unwrap(), Transition::Unchanged);
            assert_eq!(app.selected(), None);
        }
        assert_eq!(app.tasks(), &snapshot);
        assert!(app.selected_task().is_none());
    }
}

#[cfg(test)]
mod sort_tests {
    use super::{App, Mode, Notice, TaskView, Transition};
    use crate::{
        action::Action,
        projection::TaskOrder,
        task::{ListScope, TaskId, TaskList},
    };

    fn visible_ids(app: &App) -> Vec<u64> {
        app.visible_tasks().map(|task| task.id().get()).collect()
    }

    fn id(value: u64) -> TaskId {
        TaskId::from_shell_integer(value).unwrap()
    }

    #[test]
    fn selection_should_visit_every_displayed_row_through_a_group_in_manual_order() {
        let mut list = TaskList::new(ListScope::Global);
        let parent = list.add("parent").unwrap();
        list.add("other root").unwrap();
        list.add_subtask(parent, "child").unwrap();
        let mut app = App::new(list);
        assert_eq!(app.order(), TaskOrder::Manual);
        assert_eq!(visible_ids(&app), [1, 3, 2]);
        assert_eq!(app.selected(), Some(id(1)));
        for expected in [3, 2] {
            assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Transient);
            assert_eq!(app.selected(), Some(id(expected)));
        }
        assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Unchanged);
        for expected in [3, 1] {
            assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Transient);
            assert_eq!(app.selected(), Some(id(expected)));
        }
        assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Unchanged);
    }

    #[test]
    fn a_selection_resting_on_a_sole_subtask_should_still_leave_its_group() {
        let mut list = TaskList::new(ListScope::Global);
        for text in ["first", "second", "third"] {
            list.add(text).unwrap();
        }
        let mut app = App::new(list.clone());
        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(id(2)));
        // Another process nests the selected task under the row above it.
        list.set_parent(id(2), Some(id(1))).unwrap();
        assert!(app.reconcile(list));
        assert_eq!(visible_ids(&app), [1, 2, 3]);
        assert_eq!(app.selected(), Some(id(2)));
        assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Transient);
        assert_eq!(app.selected(), Some(id(3)));
        app.apply(Action::MoveUp).unwrap();
        assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Transient);
        assert_eq!(app.selected(), Some(id(1)));
    }

    /// Canonical 1..=5; Priority order is 3, 5, 2, 4, 1.
    fn prioritized() -> App {
        let mut list = TaskList::new(ListScope::Global);
        for text in [
            "plain one",
            "(B) b first",
            "(A) a open",
            "(B) b second",
            "(A) a done",
        ] {
            list.add(text).unwrap();
        }
        list.set_completed(id(5), true).unwrap();
        App::new(list)
    }

    #[test]
    fn priority_sort_should_follow_displayed_order_and_keep_selection_identity() {
        let mut app = prioritized();
        assert_eq!(app.order(), TaskOrder::Manual);
        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(id(2)));
        let before = app.tasks().clone();

        assert_eq!(
            app.apply(Action::ToggleSort).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.order(), TaskOrder::Priority);
        assert_eq!(visible_ids(&app), [3, 5, 2, 4, 1]);
        assert_eq!(app.selected(), Some(id(2)));
        for expected in [4, 1] {
            assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Transient);
            assert_eq!(app.selected(), Some(id(expected)));
        }
        assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Unchanged);
        for expected in [4, 2, 5, 3] {
            app.apply(Action::MoveUp).unwrap();
            assert_eq!(app.selected(), Some(id(expected)));
        }
        assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Unchanged);

        app.apply(Action::CycleView).unwrap();
        assert_eq!(visible_ids(&app), [3, 2, 4, 1]);
        assert_eq!(app.selected(), Some(id(3)));
        app.apply(Action::StartSearch).unwrap();
        app.apply(Action::InsertChar('b')).unwrap();
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(visible_ids(&app), [2, 4]);
        assert_eq!(app.selected(), Some(id(2)));

        // No fixture task has a date, so Due order falls back to Priority order.
        assert_eq!(
            app.apply(Action::ToggleSort).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.order(), TaskOrder::Due);
        assert_eq!(visible_ids(&app), [2, 4]);
        assert_eq!(app.selected(), Some(id(2)));
        app.apply(Action::ClearSearch).unwrap();
        assert_eq!(visible_ids(&app), [3, 2, 4, 1]);
        assert_eq!(app.selected(), Some(id(2)));
        app.apply(Action::StartSearch).unwrap();
        app.apply(Action::InsertChar('b')).unwrap();
        app.apply(Action::CommitEdit).unwrap();

        assert_eq!(
            app.apply(Action::ToggleSort).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.order(), TaskOrder::Manual);
        assert_eq!(visible_ids(&app), [2, 4]);
        assert_eq!(app.selected(), Some(id(2)));
        assert_eq!(app.tasks(), &before);
        assert_eq!(App::new(before).order(), TaskOrder::Manual);
    }

    #[test]
    fn priority_sort_should_block_reorder_with_a_notice_and_submit_nothing() {
        let mut app = prioritized();
        app.apply(Action::ToggleSort).unwrap();
        let before = app.tasks().clone();
        for action in [Action::MoveTaskDown, Action::MoveTaskUp] {
            assert_eq!(app.prepare(action).unwrap(), Transition::Transient);
            assert!(app.take_mutation().is_none());
            assert_eq!(app.notice(), Some(&Notice::ManualOrderRequired));
            assert_eq!(app.tasks(), &before);
        }
        assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Transient);
        assert_eq!(app.notice(), None);
        app.apply(Action::MoveTaskDown).unwrap();
        assert_eq!(app.apply(Action::OpenHelp).unwrap(), Transition::Transient);
        assert_eq!(app.notice(), None);
        app.apply(Action::CloseHelp).unwrap();

        app.apply(Action::ToggleSort).unwrap();
        assert_eq!(app.order(), TaskOrder::Due);
        for action in [Action::MoveTaskDown, Action::MoveTaskUp] {
            assert_eq!(app.prepare(action).unwrap(), Transition::Transient);
            assert!(app.take_mutation().is_none());
            assert_eq!(app.notice(), Some(&Notice::ManualOrderRequired));
            assert_eq!(app.tasks(), &before);
        }

        app.apply(Action::ToggleSort).unwrap();
        assert_eq!(app.order(), TaskOrder::Manual);
        for _ in 0..4 {
            app.apply(Action::MoveDown).unwrap();
        }
        assert_eq!(app.selected(), Some(id(5)));
        assert_eq!(app.apply(Action::MoveTaskUp).unwrap(), Transition::Submit);
        assert_eq!(app.notice(), None);
        assert_eq!(
            app.tasks()
                .tasks()
                .iter()
                .map(|task| task.id().get())
                .collect::<Vec<_>>(),
            [1, 2, 3, 5, 4]
        );
    }

    #[test]
    fn priority_sort_should_fall_back_in_displayed_order_and_follow_reconciled_priorities() {
        let mut app = prioritized();
        app.apply(Action::ToggleSort).unwrap();
        app.apply(Action::MoveUp).unwrap();
        app.apply(Action::MoveUp).unwrap();
        assert_eq!(app.selected(), Some(id(2)));
        assert_eq!(app.apply(Action::Delete).unwrap(), Transition::Submit);
        assert_eq!(visible_ids(&app), [3, 5, 4, 1]);
        assert_eq!(app.selected(), Some(id(4)));

        let mut latest = app.tasks().clone();
        latest.edit(id(1), "(A) promoted").unwrap();
        latest.add("(C) new").unwrap();
        assert!(app.reconcile(latest));
        assert_eq!(visible_ids(&app), [1, 3, 5, 4, 6]);
        assert_eq!(app.selected(), Some(id(4)));

        assert_eq!(
            app.apply(Action::RestoreLatest).unwrap(),
            Transition::Submit
        );
        assert_eq!(visible_ids(&app), [1, 3, 5, 2, 4, 6]);
        assert_eq!(app.selected(), Some(id(2)));
    }

    /// Canonical 1..=5; Priority order is 3, 2, 5, 1, 4 and Due order 4, 3, 2, 5, 1.
    fn dated() -> App {
        let mut list = TaskList::new(ListScope::Global);
        for text in [
            "plain one",
            "(B) b due:2026-10-25",
            "(A) a due:2026-10-25",
            "c due:2026-10-01",
            "(C) no date",
        ] {
            list.add(text).unwrap();
        }
        App::new(list)
    }

    #[test]
    fn sorted_orders_should_keep_groups_together() {
        let mut list = TaskList::new(ListScope::Global);
        for text in ["(C) one due:2026-11-03", "(A) two due:2026-11-09", "three"] {
            list.add(text).unwrap();
        }
        // 1 [4, 5], 2 [6], 3: subtasks sort within their group, never among top-level rows.
        list.add_subtask(id(1), "(B) four due:2026-11-02").unwrap();
        list.add_subtask(id(1), "(A) five due:2026-11-01").unwrap();
        list.add_subtask(id(2), "six").unwrap();
        let mut app = App::new(list);
        for (order, rows) in [
            (TaskOrder::Priority, [2, 6, 1, 5, 4, 3]),
            (TaskOrder::Due, [1, 5, 4, 2, 6, 3]),
            (TaskOrder::Manual, [1, 4, 5, 2, 6, 3]),
        ] {
            app.apply(Action::ToggleSort).unwrap();
            assert_eq!(app.order(), order);
            assert_eq!(visible_ids(&app), rows, "{order:?}");
        }
    }

    #[test]
    fn sort_should_cycle_manual_priority_and_due_and_keep_selection_identity() {
        let mut app = dated();
        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(id(2)));
        for (order, rows) in [
            (TaskOrder::Priority, [3, 2, 5, 1, 4]),
            (TaskOrder::Due, [4, 3, 2, 5, 1]),
            (TaskOrder::Manual, [1, 2, 3, 4, 5]),
            (TaskOrder::Priority, [3, 2, 5, 1, 4]),
        ] {
            assert_eq!(
                app.apply(Action::ToggleSort).unwrap(),
                Transition::Transient
            );
            assert_eq!(app.order(), order);
            assert_eq!(visible_ids(&app), rows, "{order:?}");
            assert_eq!(app.selected(), Some(id(2)), "{order:?}");
        }
    }

    #[test]
    fn due_order_should_drive_selection_movement_and_refuse_reorder() {
        let mut app = dated();
        app.apply(Action::ToggleSort).unwrap();
        app.apply(Action::ToggleSort).unwrap();
        assert_eq!(app.order(), TaskOrder::Due);
        assert_eq!(app.selected(), Some(id(1)));
        for expected in [5, 2, 3, 4] {
            assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Transient);
            assert_eq!(app.selected(), Some(id(expected)));
        }
        assert_eq!(app.apply(Action::MoveUp).unwrap(), Transition::Unchanged);
        for expected in [3, 2, 5, 1] {
            app.apply(Action::MoveDown).unwrap();
            assert_eq!(app.selected(), Some(id(expected)));
        }
        assert_eq!(app.apply(Action::MoveDown).unwrap(), Transition::Unchanged);

        let before = app.tasks().clone();
        for action in [Action::MoveTaskDown, Action::MoveTaskUp] {
            assert_eq!(app.prepare(action).unwrap(), Transition::Transient);
            assert!(app.take_mutation().is_none());
            assert_eq!(app.notice(), Some(&Notice::ManualOrderRequired));
        }
        assert_eq!(app.tasks(), &before);
    }

    #[test]
    fn due_order_should_be_rebuilt_only_on_reconcile_and_order_changes() {
        let mut app = dated();
        app.apply(Action::ToggleSort).unwrap();
        app.apply(Action::ToggleSort).unwrap();
        let rebuilds = app.order_rebuilds;

        app.apply(Action::CycleView).unwrap();
        app.apply(Action::StartSearch).unwrap();
        app.apply(Action::InsertChar('a')).unwrap();
        app.apply(Action::CommitEdit).unwrap();
        app.apply(Action::ClearSearch).unwrap();
        app.apply(Action::EditFilter).unwrap();
        app.apply(Action::InsertChar('A')).unwrap();
        app.apply(Action::CommitEdit).unwrap();
        app.apply(Action::ClearSearch).unwrap();
        app.apply(Action::PreviousView).unwrap();
        assert!(app.set_today(crate::metadata::Date::new(2026, 10, 30).unwrap()));
        app.apply(Action::OpenTrash).unwrap();
        app.apply(Action::CloseTrash).unwrap();
        assert_eq!(
            app.order_rebuilds, rebuilds,
            "views, search, filter, and days reuse it"
        );
        assert_eq!(visible_ids(&app), [4, 3, 2, 5, 1]);

        let mut latest = app.tasks().clone();
        latest.edit(id(1), "plain one due:2026-09-01").unwrap();
        assert!(app.reconcile(latest));
        assert_eq!(app.order_rebuilds, rebuilds + 1);
        assert_eq!(visible_ids(&app), [1, 4, 3, 2, 5]);
        assert!(!app.reconcile(app.tasks().clone()), "unchanged data");
        assert_eq!(app.order_rebuilds, rebuilds + 1);

        app.apply(Action::ToggleSort).unwrap();
        assert_eq!(app.order(), TaskOrder::Manual);
        assert_eq!(app.order_rebuilds, rebuilds + 2);
    }

    #[test]
    fn sort_should_survive_trash_and_help_and_be_ignored_outside_normal_mode() {
        let mut app = prioritized();
        app.apply(Action::ToggleSort).unwrap();
        for (open, close) in [
            (Action::OpenTrash, Action::CloseTrash),
            (Action::OpenHelp, Action::CloseHelp),
            (Action::StartSearch, Action::CancelEdit),
            (Action::StartAdd, Action::CancelEdit),
        ] {
            app.apply(open).unwrap();
            if app.mode() != Mode::Insert && app.mode() != Mode::Search {
                assert_eq!(
                    app.apply(Action::ToggleSort).unwrap(),
                    Transition::Unchanged
                );
            }
            app.apply(close).unwrap();
            assert_eq!(app.mode(), Mode::Normal);
            assert_eq!(app.order(), TaskOrder::Priority);
            assert_eq!(app.view(), TaskView::All);
            assert_eq!(visible_ids(&app), [3, 5, 2, 4, 1]);
        }
    }
}

#[cfg(test)]
mod filter_tests {
    use super::{App, EditKind, FilterPreview, Mode, Notice, TaskView, Transition};
    use crate::{
        action::Action,
        metadata::{TaskFilter, parse_filter_token},
        mutation::Mutation,
        task::{ListScope, TaskId, TaskList},
    };

    fn id(value: u64) -> TaskId {
        TaskId::from_shell_integer(value).unwrap()
    }

    fn visible_ids(app: &App) -> Vec<u64> {
        app.visible_tasks().map(|task| task.id().get()).collect()
    }

    fn filter(tokens: &[&str]) -> TaskFilter {
        let mut filter = TaskFilter::default();
        for token in tokens {
            filter.add(parse_filter_token(token).unwrap());
        }
        filter
    }

    fn type_text(app: &mut App, text: &str) {
        for character in text.chars() {
            assert_eq!(
                app.apply(Action::InsertChar(character)).unwrap(),
                Transition::Transient
            );
        }
    }

    fn clear_buffer(app: &mut App) {
        while app
            .editor()
            .is_some_and(|editor| !editor.buffer().is_empty())
        {
            app.apply(Action::DeleteBeforeCursor).unwrap();
        }
    }

    fn edit_filter(app: &mut App, text: &str) {
        assert_eq!(
            app.apply(Action::EditFilter).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.mode(), Mode::Filter);
        clear_buffer(app);
        type_text(app, text);
    }

    /// 1 `+shtodo`, 2 `+shell @x`, 3 `+other`, 4 `+shtodo @x`, 5 plain.
    fn tagged() -> App {
        let mut list = TaskList::new(ListScope::Global);
        for text in [
            "(A) ship +shtodo",
            "(B) script +shell @x",
            "other +other",
            "release +shtodo. @x",
            "plain",
        ] {
            list.add(text).unwrap();
        }
        App::new(list)
    }

    #[test]
    fn filter_editor_should_preview_trailing_tag_by_prefix_and_accept_exactly() {
        let mut app = tagged();
        assert_eq!(
            app.apply(Action::EditFilter).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.editor().unwrap().kind(), EditKind::Filter);
        let mut typed = String::new();
        for character in "+shtodo".chars() {
            type_text(&mut app, &character.to_string());
            typed.push(character);
            assert!(visible_ids(&app).contains(&1), "{typed:?}");
            assert!(visible_ids(&app).contains(&4), "{typed:?}");
        }
        assert_eq!(visible_ids(&app), [1, 4]);
        clear_buffer(&mut app);
        type_text(&mut app, "+");
        assert_eq!(visible_ids(&app), [1, 2, 3, 4, 5]);
        type_text(&mut app, "sh");
        assert_eq!(visible_ids(&app), [1, 2, 4]);
        type_text(&mut app, " ");
        assert!(visible_ids(&app).is_empty());
        clear_buffer(&mut app);
        type_text(&mut app, "+.");
        assert_eq!(visible_ids(&app), [1, 2, 3, 4, 5]);
        clear_buffer(&mut app);
        type_text(&mut app, "@x +sh");
        assert_eq!(visible_ids(&app), [2, 4]);
        assert_eq!(
            app.filter_preview(),
            Some(&FilterPreview {
                exact: filter(&["@x"]),
                trailing_prefix: Some("+sh".into()),
            })
        );
        assert!(app.filter().is_empty());

        clear_buffer(&mut app);
        type_text(&mut app, "+shtodo.");
        assert_eq!(
            app.filter_preview().unwrap().trailing_prefix.as_deref(),
            Some("+shtodo")
        );
        assert_eq!(visible_ids(&app), [1, 4]);
        assert_eq!(
            app.apply(Action::CommitEdit).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.mode(), Mode::Normal);
        assert!(app.editor().is_none());
        assert!(app.filter_preview().is_none());
        assert_eq!(app.filter(), &filter(&["+shtodo"]));
        assert_eq!(app.filter_text(), "+shtodo");
        assert_eq!(visible_ids(&app), [1, 4]);

        // The preview moved the selection from 1 to 2, then to 4.
        assert_eq!(app.selected(), Some(id(4)));
        assert_eq!(app.prepare(Action::MoveTaskUp).unwrap(), Transition::Submit);
        match app.take_mutation() {
            Some(Mutation::MoveAdjacent {
                id: moved,
                neighbor,
                projection,
                ..
            }) => {
                assert_eq!(moved, id(4));
                assert_eq!(neighbor, Some(id(1)));
                assert_eq!(projection.filter, filter(&["+shtodo"]));
                assert_eq!(projection.view, TaskView::All);
            }
            other => panic!("unexpected mutation {other:?}"),
        }
    }

    #[test]
    fn filter_editor_enter_should_complete_unique_prefix_refuse_ambiguous_and_accept_unknown() {
        let mut app = tagged();
        edit_filter(&mut app, "+sh");
        let cursor = app.editor().unwrap().cursor();
        assert_eq!(
            app.apply(Action::CommitEdit).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.mode(), Mode::Filter);
        assert_eq!(app.editor().unwrap().buffer(), "+sh");
        assert_eq!(app.editor().unwrap().cursor(), cursor);
        assert_eq!(
            app.notice(),
            Some(&Notice::FinishTag(vec!["+shtodo".into(), "+shell".into()]))
        );
        assert!(app.filter().is_empty());

        type_text(&mut app, "t");
        assert_eq!(app.notice(), None);
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.filter_text(), "+shtodo");
        assert_eq!(visible_ids(&app), [1, 4]);

        edit_filter(&mut app, "+shtodo");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.filter(), &filter(&["+shtodo"]));

        edit_filter(&mut app, "+zzz");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.filter_text(), "+zzz");
        assert!(visible_ids(&app).is_empty());
        assert_eq!(app.selected(), None);

        edit_filter(&mut app, "+");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Filter);
        assert_eq!(
            app.message(),
            Some("Invalid filter \"+\"; use +project, @context, A-C, none, or due:WHEN")
        );
        clear_buffer(&mut app);
        type_text(&mut app, "+.");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Filter);

        // Only tags in rows that also match the completed `@x` are candidates.
        clear_buffer(&mut app);
        type_text(&mut app, "@x +s");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(
            app.notice(),
            Some(&Notice::FinishTag(vec!["+shell".into(), "+shtodo".into()]))
        );
        clear_buffer(&mut app);
        type_text(&mut app, "@x +sht");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.filter_text(), "@x +shtodo");
        assert_eq!(visible_ids(&app), [4]);

        // The view and search narrow the candidates too.
        app.apply(Action::StartSearch).unwrap();
        type_text(&mut app, "script");
        app.apply(Action::CommitEdit).unwrap();
        edit_filter(&mut app, "+sh");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.filter_text(), "+shell");
        assert_eq!(visible_ids(&app), [2]);

        // Completed tokens stay as typed apart from normalization; priorities never complete.
        edit_filter(&mut app, "+sh @x. a");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.filter_text(), "+sh @x a");
        assert_eq!(app.filter(), &filter(&["+sh", "@x", "A"]));
    }

    #[test]
    fn filter_editor_should_keep_the_last_valid_preview_and_reject_invalid_buffers() {
        let mut app = tagged();
        edit_filter(&mut app, "@x");
        assert_eq!(visible_ids(&app), [2, 4]);
        type_text(&mut app, " work");
        assert_eq!(visible_ids(&app), [2, 4]);
        assert_eq!(app.message(), None);
        type_text(&mut app, " ");
        assert_eq!(visible_ids(&app), [2, 4]);
        assert_eq!(
            app.message(),
            Some("Invalid filter \"work\"; use +project, @context, A-C, none, or due:WHEN")
        );
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Filter);
        assert!(app.filter().is_empty());

        clear_buffer(&mut app);
        assert_eq!(app.message(), None);
        type_text(&mut app, "A-");
        assert_eq!(visible_ids(&app), [1, 2, 3, 4, 5]);
        assert_eq!(app.message(), None);
        type_text(&mut app, "B");
        assert_eq!(visible_ids(&app), [1, 2]);
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.filter_text(), "A-B");

        edit_filter(&mut app, "A-");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Filter);
        assert_eq!(
            app.message(),
            Some("Invalid filter \"A-\"; use +project, @context, A-C, none, or due:WHEN")
        );

        clear_buffer(&mut app);
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
        assert!(app.filter().is_empty());
        assert_eq!(app.filter_text(), "");
    }

    #[test]
    fn filter_editor_should_reopen_with_the_accepted_filter_and_escape_should_restore_it() {
        let mut app = tagged();
        edit_filter(&mut app, "@x");
        app.apply(Action::CommitEdit).unwrap();
        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(id(4)));

        app.apply(Action::EditFilter).unwrap();
        assert_eq!(app.editor().unwrap().buffer(), "@x ");
        assert_eq!(app.editor().unwrap().cursor(), 3);
        assert_eq!(visible_ids(&app), [2, 4]);
        clear_buffer(&mut app);
        type_text(&mut app, "+other");
        assert_eq!(visible_ids(&app), [3]);
        assert_eq!(app.selected(), Some(id(3)));
        assert_eq!(
            app.apply(Action::CancelEdit).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.mode(), Mode::Normal);
        assert!(app.editor().is_none());
        assert_eq!(app.filter_text(), "@x");
        assert_eq!(visible_ids(&app), [2, 4]);
        assert_eq!(app.selected(), Some(id(4)));
    }

    #[test]
    fn layered_escape_should_clear_search_then_filter_one_press_at_a_time() {
        let mut app = tagged();
        edit_filter(&mut app, "@x");
        app.apply(Action::CommitEdit).unwrap();
        app.apply(Action::StartSearch).unwrap();
        type_text(&mut app, "release");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(visible_ids(&app), [4]);

        assert_eq!(
            app.apply(Action::ClearSearch).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.query(), "");
        assert_eq!(app.filter_text(), "@x");
        assert_eq!(visible_ids(&app), [2, 4]);
        assert_eq!(
            app.apply(Action::ClearSearch).unwrap(),
            Transition::Transient
        );
        assert!(app.filter().is_empty());
        assert_eq!(app.filter_text(), "");
        assert_eq!(visible_ids(&app), [1, 2, 3, 4, 5]);
        assert_eq!(app.selected(), Some(id(4)));
        assert_eq!(
            app.apply(Action::ClearSearch).unwrap(),
            Transition::Unchanged
        );
    }

    #[test]
    fn filter_hint_should_count_tags_in_the_view_narrowed_by_the_trailing_prefix() {
        let mut list = TaskList::new(ListScope::Global);
        for text in [
            "a +shtodo +shtodo. @x",
            "b +shtodo @laptop",
            "c +shell @laptop",
            "d +shtodo @x",
            "e +zeta",
        ] {
            list.add(text).unwrap();
        }
        let done = list.add("f +shell @x").unwrap();
        list.set_completed(done, true).unwrap();
        let mut app = App::new(list);
        assert!(app.filter_hint().is_none());
        app.apply(Action::EditFilter).unwrap();
        assert_eq!(
            app.filter_hint().unwrap(),
            [
                ("+shtodo", 3),
                ("@x", 3),
                ("+shell", 2),
                ("@laptop", 2),
                ("+zeta", 1)
            ]
        );
        type_text(&mut app, "@x +sh");
        assert_eq!(app.filter_hint().unwrap(), [("+shtodo", 2), ("+shell", 1)]);
        app.apply(Action::CancelEdit).unwrap();
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "+sh");
        assert_eq!(app.filter_hint().unwrap(), [("+shtodo", 3), ("+shell", 1)]);
        type_text(&mut app, "x");
        assert!(app.filter_hint().unwrap().is_empty());
    }

    #[test]
    fn filter_hint_should_recount_after_refresh_and_after_erasing_a_completed_token() {
        let mut list = TaskList::new(ListScope::Global);
        list.add("a +shtodo @x").unwrap();
        list.add("b +shell").unwrap();
        let mut app = App::new(list.clone());
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "@x +sh");
        assert_eq!(app.filter_hint().unwrap(), [("+shtodo", 1)]);
        list.add("c +shine @x").unwrap();
        assert!(app.reconcile(list));
        assert_eq!(app.filter_hint().unwrap(), [("+shine", 1), ("+shtodo", 1)]);
        for _ in "@x +sh".chars() {
            app.apply(Action::DeleteBeforeCursor).unwrap();
        }
        type_text(&mut app, "+sh");
        assert_eq!(
            app.filter_hint().unwrap(),
            [("+shell", 1), ("+shine", 1), ("+shtodo", 1)]
        );
    }

    #[test]
    fn filter_mode_should_block_mutations_and_hidden_messages_should_mention_the_filter() {
        let mut app = tagged();
        let before = app.tasks().clone();
        app.apply(Action::EditFilter).unwrap();
        for action in [
            Action::ToggleComplete,
            Action::Delete,
            Action::RestoreLatest,
            Action::StartAdd,
            Action::MoveTaskDown,
            Action::MoveDown,
            Action::ToggleSort,
            Action::CycleView,
            Action::OpenHelp,
            Action::OpenTrash,
            Action::EditFilter,
        ] {
            assert_eq!(
                app.apply(action).unwrap(),
                Transition::Unchanged,
                "{action:?}"
            );
        }
        assert_eq!(app.tasks(), &before);
        type_text(&mut app, "+shtodo");
        app.apply(Action::CommitEdit).unwrap();

        app.apply(Action::StartAdd).unwrap();
        type_text(&mut app, "untagged");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(
            app.message(),
            Some("Added task is hidden by the current view, search, or filter")
        );
        assert_eq!(app.selected(), Some(id(1)));
        app.apply(Action::StartEdit).unwrap();
        clear_buffer(&mut app);
        type_text(&mut app, "(A) ship");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(
            app.message(),
            Some("Edited task is hidden by the current view, search, or filter")
        );
        assert_eq!(app.selected(), Some(id(4)));

        let mut list = app.tasks().clone();
        list.delete(id(3)).unwrap();
        app.reconcile(list);
        app.apply(Action::RestoreLatest).unwrap();
        assert_eq!(
            app.message(),
            Some("Restored task is hidden by the current view, search, or filter")
        );
        assert_eq!(app.selected(), Some(id(4)));
    }

    #[test]
    fn filter_and_sort_should_survive_trash_and_help_and_compose_with_views() {
        let mut app = tagged();
        app.apply(Action::ToggleSort).unwrap();
        edit_filter(&mut app, "@x");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(visible_ids(&app), [2, 4]);
        for (open, close) in [
            (Action::OpenTrash, Action::CloseTrash),
            (Action::OpenHelp, Action::CloseHelp),
        ] {
            app.apply(open).unwrap();
            app.apply(close).unwrap();
            assert_eq!(app.filter_text(), "@x");
            assert_eq!(visible_ids(&app), [2, 4]);
        }
        let mut list = app.tasks().clone();
        list.set_completed(id(4), true).unwrap();
        list.edit(id(5), "(A) plain now @x").unwrap();
        app.reconcile(list);
        assert_eq!(visible_ids(&app), [5, 2, 4]);
        app.apply(Action::CycleView).unwrap();
        assert_eq!(visible_ids(&app), [5, 2]);
    }
}

#[cfg(test)]
mod due_tests {
    use super::{App, Mode, Transition, test_today};
    use crate::{
        action::Action,
        metadata::Date,
        mutation::Mutation,
        task::{ListScope, TaskList},
    };

    fn type_text(app: &mut App, text: &str) {
        for character in text.chars() {
            app.prepare(Action::InsertChar(character)).unwrap();
        }
    }

    /// Commits the editor and applies the submitted mutation as a fresh-list transaction would.
    fn commit(app: &mut App) -> Option<Mutation> {
        app.prepare(Action::CommitEdit).unwrap();
        let request = app.take_mutation()?;
        let mut latest = app.tasks().clone();
        let outcome = request.apply(&mut latest);
        app.finish(&request, latest, outcome);
        Some(request)
    }

    #[test]
    fn tui_add_should_rewrite_relative_words_to_the_app_date() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        assert_eq!(app.today(), test_today());
        app.prepare(Action::StartAdd).unwrap();
        type_text(&mut app, "  (A) Pay due:Today. due:tomorrow +home  ");
        assert_eq!(
            commit(&mut app),
            Some(Mutation::Add(
                "(A) Pay due:2026-10-21. due:2026-10-22 +home".into()
            ))
        );
        assert_eq!(app.mode(), Mode::Normal);

        app.prepare(Action::StartAdd).unwrap();
        type_text(&mut app, "plain due:soon");
        assert_eq!(
            commit(&mut app),
            Some(Mutation::Add("plain due:soon".into()))
        );
    }

    #[test]
    fn tui_edit_should_rewrite_relative_words_before_the_unchanged_check() {
        let mut list = TaskList::new(ListScope::Global);
        let id = list.add("Pay due:2026-10-21").unwrap();
        let mut app = App::new(list);

        app.prepare(Action::StartEdit).unwrap();
        for _ in 0.."2026-10-21".len() {
            app.prepare(Action::DeleteBeforeCursor).unwrap();
        }
        type_text(&mut app, "today");
        assert_eq!(
            app.prepare(Action::CommitEdit).unwrap(),
            Transition::Refresh,
            "the rewrite equals the original, so the editor closes"
        );
        assert_eq!(app.take_mutation(), None);
        assert_eq!(app.mode(), Mode::Normal);

        app.prepare(Action::StartEdit).unwrap();
        for _ in 0.."2026-10-21".len() {
            app.prepare(Action::DeleteBeforeCursor).unwrap();
        }
        type_text(&mut app, "tomorrow");
        assert_eq!(
            commit(&mut app),
            Some(Mutation::EditText {
                id,
                text: "Pay due:2026-10-22".into(),
                original: Some("Pay due:2026-10-21".into()),
            })
        );
    }

    #[test]
    fn set_today_should_change_what_the_next_add_writes_and_redraw_only_on_a_new_date() {
        let mut app = App::new(TaskList::new(ListScope::Global));
        let new_year = Date::new(2026, 12, 31).unwrap();
        assert!(
            !app.set_today(test_today()),
            "the same date needs no redraw"
        );
        assert!(app.set_today(new_year), "a new date restyles rows");
        assert_eq!(app.today(), new_year);
        assert!(!app.set_today(new_year));
        app.prepare(Action::StartAdd).unwrap();
        type_text(&mut app, "x due:tomorrow");
        assert_eq!(
            commit(&mut app),
            Some(Mutation::Add("x due:2027-01-01".into()))
        );
    }
}

#[cfg(test)]
mod due_filter_tests {
    use super::{App, Mode, Transition};
    use crate::{
        action::Action,
        metadata::Date,
        mutation::Mutation,
        task::{ListScope, MoveDirection, TaskId, TaskList},
    };

    const INVALID: &str = "; use +project, @context, A-C, none, or due:WHEN";

    fn id(value: u64) -> TaskId {
        TaskId::from_shell_integer(value).unwrap()
    }

    fn day(day: u8) -> Date {
        Date::new(2026, 10, day).unwrap()
    }

    fn visible_ids(app: &App) -> Vec<u64> {
        app.visible_tasks().map(|task| task.id().get()).collect()
    }

    fn type_text(app: &mut App, text: &str) {
        for character in text.chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
    }

    fn accept(app: &mut App, text: &str) {
        app.apply(Action::EditFilter).unwrap();
        type_text(app, text);
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Normal, "{text:?}: {:?}", app.message());
    }

    /// Displayed on 2026-10-21: 1 overdue, 2 done and overdue, 3 and 7 today, 4 and 8
    /// tomorrow, 5 later, 6 undated.
    fn dated() -> App {
        let mut list = TaskList::new(ListScope::Global);
        for text in [
            "late due:2026-10-20 +x",
            "done late due:2026-10-19 +x",
            "now due:2026-10-21 +x",
            "soon due:2026-10-22 +x",
            "later due:2026-11-02 +x",
            "plain +x",
            "now too due:2026-10-21",
            "also soon due:2026-10-22",
        ] {
            list.add(text).unwrap();
        }
        list.set_completed(id(2), true).unwrap();
        App::new(list)
    }

    #[test]
    fn due_overdue_preview_should_hide_done_and_future_rows() {
        let mut app = dated();
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "due:overdue");
        assert_eq!(visible_ids(&app), [1]);
        assert_eq!(app.message(), None);
        type_text(&mut app, " due:none");
        assert_eq!(visible_ids(&app), [1, 6], "due terms are a union");
    }

    #[test]
    fn an_in_progress_due_token_should_drop_out_of_the_preview_without_an_error() {
        let mut app = dated();
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "+x due:2026-1");
        assert_eq!(visible_ids(&app), [1, 2, 3, 4, 5, 6]);
        assert_eq!(app.message(), None);
        assert!(app.filter_preview().unwrap().exact.due.is_empty());
        type_text(&mut app, "0-22");
        assert_eq!(visible_ids(&app), [4]);
    }

    #[test]
    fn typing_a_due_range_should_preview_each_prefix_that_parses_and_never_error() {
        let mut app = dated();
        app.apply(Action::EditFilter).unwrap();
        let all = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut typed = String::new();
        for character in "due:today..2026-11-01".chars() {
            type_text(&mut app, &character.to_string());
            typed.push(character);
            let expected = match typed.as_str() {
                "due:today" => vec![3, 7],
                "due:today.." => vec![3, 4, 5, 7, 8],
                "due:today..2026-11-01" => vec![3, 4, 7, 8],
                // A lone letter is a priority token, and no task has priority D.
                "d" => vec![],
                // `due:today.`, `due:today..2` to `due:today..2026-11-0`, and every shorter
                // prefix do not parse yet, so they add no due constraint.
                _ => all.clone(),
            };
            assert_eq!(visible_ids(&app), expected, "{typed:?}");
            assert_eq!(app.message(), None, "{typed:?}");
            assert!(app.notice().is_none(), "{typed:?}");
        }
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.filter_text(), "due:today..2026-11-01");
        assert_eq!(visible_ids(&app), [3, 4, 7, 8]);
    }

    #[test]
    fn enter_should_accept_due_tokens_as_typed_and_never_complete_them() {
        let mut app = dated();
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "due:tod");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Filter);
        assert_eq!(
            app.message(),
            Some(format!("Invalid filter \"due:tod\"{INVALID}").as_str())
        );
        assert_eq!(app.editor().unwrap().buffer(), "due:tod");
        assert!(app.filter().is_empty());
        assert!(app.notice().is_none(), "never completed like a tag prefix");

        type_text(&mut app, "AY");
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.filter_text(), "due:todAY", "kept as typed");
        assert_eq!(visible_ids(&app), [3, 7]);
    }

    #[test]
    fn an_invalid_due_token_should_keep_the_editor_with_the_message() {
        let mut app = dated();
        accept(&mut app, "due:today");
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "due:soon");
        assert_eq!(app.message(), None, "in progress");
        assert_eq!(
            visible_ids(&app),
            [3, 7],
            "the completed token still applies"
        );
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Filter);
        let message = format!("Invalid filter \"due:soon\"{INVALID}");
        assert_eq!(app.message(), Some(message.as_str()));
        assert_eq!(app.filter_text(), "due:today");

        type_text(&mut app, " ");
        assert_eq!(
            app.message(),
            Some(message.as_str()),
            "a completed invalid token"
        );
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(app.mode(), Mode::Filter);
        assert_eq!(visible_ids(&app), [3, 7], "the last valid preview is kept");
        assert_eq!(app.filter_text(), "due:today");
    }

    #[test]
    fn due_tokens_should_or_together_and_and_with_tags() {
        let mut app = dated();
        accept(&mut app, "+x due:today due:tomorrow");
        assert_eq!(visible_ids(&app), [3, 4]);
        assert_eq!(app.filter_text(), "+x due:today due:tomorrow");
        accept(&mut app, " due:overdue");
        assert_eq!(app.filter_text(), "+x due:today due:tomorrow due:overdue");
        assert_eq!(visible_ids(&app), [1, 3, 4]);
    }

    #[test]
    fn escape_should_restore_and_layered_escape_should_clear_due_tokens_with_the_filter() {
        let mut app = dated();
        accept(&mut app, "due:overdue");
        app.apply(Action::EditFilter).unwrap();
        assert_eq!(app.editor().unwrap().buffer(), "due:overdue ");
        type_text(&mut app, "due:tomorrow");
        assert_eq!(visible_ids(&app), [1, 4, 8]);
        app.apply(Action::CancelEdit).unwrap();
        assert_eq!(app.filter_text(), "due:overdue");
        assert_eq!(visible_ids(&app), [1]);

        app.apply(Action::StartSearch).unwrap();
        type_text(&mut app, "zzz");
        app.apply(Action::CommitEdit).unwrap();
        assert!(visible_ids(&app).is_empty());
        app.apply(Action::ClearSearch).unwrap();
        assert_eq!(visible_ids(&app), [1]);
        app.apply(Action::ClearSearch).unwrap();
        assert!(app.filter().is_empty());
        assert_eq!(app.filter_text(), "");
        assert_eq!(visible_ids(&app), [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn a_new_day_should_re_resolve_the_accepted_filter_and_an_open_preview() {
        let mut app = dated();
        accept(&mut app, "due:today");
        assert_eq!(visible_ids(&app), [3, 7]);
        assert_eq!(app.selected(), Some(id(3)));
        assert!(app.set_today(day(22)));
        assert_eq!(app.filter_text(), "due:today", "the words are kept");
        assert_eq!(visible_ids(&app), [4, 8]);
        assert_eq!(app.selected(), Some(id(4)), "selection reconciled");

        // Completed `due:today +x`, trailing `due:overdue`: today or overdue, and `+x`.
        app.apply(Action::EditFilter).unwrap();
        type_text(&mut app, "+x due:overdue");
        assert_eq!(visible_ids(&app), [1, 3, 4]);
        assert_eq!(app.filter_hint().unwrap(), [("+x", 1)]);
        assert!(app.set_today(day(19)), "a date that moves backward");
        assert_eq!(visible_ids(&app), [2], "the preview moved");
        assert_eq!(app.filter_hint().unwrap(), [("+x", 1)]);
        assert!(app.set_today(day(25)));
        assert_eq!(visible_ids(&app), [1, 3, 4]);
        assert!(
            app.filter_hint().unwrap().is_empty(),
            "counts follow the completed tokens on the new day"
        );
        assert_eq!(app.editor().unwrap().buffer(), "due:today +x due:overdue");
        assert_eq!(app.mode(), Mode::Filter);
        app.apply(Action::CancelEdit).unwrap();
        assert!(
            visible_ids(&app).is_empty(),
            "the accepted filter moved too"
        );
        assert!(app.set_today(day(21)));
        assert_eq!(visible_ids(&app), [3, 7]);
    }

    #[test]
    fn a_filtered_move_after_a_new_day_should_carry_the_new_day_projection() {
        let mut app = dated();
        accept(&mut app, "due:today");
        assert!(app.set_today(day(22)));
        assert_eq!(visible_ids(&app), [4, 8]);
        assert_eq!(app.selected(), Some(id(4)));
        assert_eq!(
            app.prepare(Action::MoveTaskDown).unwrap(),
            Transition::Submit
        );
        let request = app.take_mutation().unwrap();
        let Mutation::MoveAdjacent {
            id: moved,
            direction,
            neighbor,
            projection,
        } = &request
        else {
            panic!("{request:?}");
        };
        assert_eq!(
            (*moved, *direction, *neighbor),
            (id(4), MoveDirection::Down, Some(id(8)))
        );
        assert_eq!(
            projection, &app.projection,
            "resolved for the displayed day"
        );

        let mut latest = app.tasks().clone();
        assert!(request.apply(&mut latest).changed());
        assert_eq!(
            latest
                .visible_tasks()
                .map(|task| task.id().get())
                .collect::<Vec<_>>(),
            [1, 2, 3, 8, 5, 6, 7, 4]
        );
    }
}

#[cfg(test)]
mod subtask_tests {
    use super::*;
    use crate::task::ListScope;

    fn id(value: u64) -> TaskId {
        TaskId::from_shell_integer(value).unwrap()
    }

    fn ids(app: &App) -> Vec<u64> {
        app.visible_tasks().map(|task| task.id().get()).collect()
    }

    /// Group 1 [4 match, 5 done], 2, group 3 [6, 7 match], 8 match.
    fn tree() -> TaskList {
        let mut list = TaskList::new(ListScope::Global);
        for text in [
            "one +home",
            "two",
            "three",
            "four match",
            "five",
            "six",
            "seven match",
        ] {
            list.add(text).unwrap();
        }
        for (task, parent) in [(4, 1), (5, 1), (6, 3), (7, 3)] {
            list.set_parent(id(task), Some(id(parent))).unwrap();
        }
        list.add("eight match").unwrap();
        list.set_completed(id(5), true).unwrap();
        list.set_completed(id(1), true).unwrap();
        assert_eq!(
            list.visible_tasks()
                .map(|task| task.id().get())
                .collect::<Vec<_>>(),
            [1, 4, 5, 2, 3, 6, 7, 8]
        );
        list
    }

    fn search(app: &mut App, query: &str) {
        app.apply(Action::StartSearch).unwrap();
        for character in query.chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
        app.apply(Action::CommitEdit).unwrap();
    }

    fn rows(app: &App) -> Vec<(u64, bool)> {
        app.displayed_rows()
            .map(|row| (row.task.id().get(), row.context))
            .collect()
    }

    #[test]
    fn context_rows_should_show_in_search_open_and_filter_views_without_counting_as_matches() {
        let mut app = App::new(tree());
        search(&mut app, "match");
        assert_eq!(
            rows(&app),
            [(1, true), (4, false), (3, true), (7, false), (8, false)]
        );
        assert_eq!(app.match_count(), 3);
        // Open hides the done parent 1, but its open matching subtask brings it back as context.
        app.apply(Action::ClearSearch).unwrap();
        app.apply(Action::CycleView).unwrap();
        assert_eq!(app.view(), TaskView::Open);
        assert_eq!(
            rows(&app),
            [
                (1, true),
                (4, false),
                (2, false),
                (3, false),
                (6, false),
                (7, false),
                (8, false)
            ]
        );
        assert_eq!(app.match_count(), 6);
        // A filter on the parent's tag shows the parent only; its subtasks do not match.
        app.apply(Action::CycleView).unwrap();
        app.apply(Action::CycleView).unwrap();
        assert_eq!(app.view(), TaskView::All);
        app.apply(Action::EditFilter).unwrap();
        for character in "+home".chars() {
            app.apply(Action::InsertChar(character)).unwrap();
        }
        app.apply(Action::CommitEdit).unwrap();
        assert_eq!(rows(&app), [(1, false)]);
        assert_eq!(app.match_count(), 1);
    }

    #[test]
    fn a_filter_preview_should_drive_context_rows_while_typing() {
        let mut list = tree();
        list.edit(id(7), "seven match +x").unwrap();
        let mut app = App::new(list);
        app.apply(Action::EditFilter).unwrap();
        app.apply(Action::InsertChar('+')).unwrap();
        app.apply(Action::InsertChar('x')).unwrap();
        assert_eq!(app.mode(), Mode::Filter);
        assert_eq!(rows(&app), [(3, true), (7, false)]);
        assert_eq!(app.match_count(), 1);
        app.apply(Action::CancelEdit).unwrap();
        assert_eq!(app.match_count(), 8);
    }

    #[test]
    fn selection_should_visit_subtasks_and_context_rows_and_moves_stay_among_siblings() {
        let mut app = App::new(tree());
        search(&mut app, "match");
        assert_eq!(app.selected(), Some(id(1)));
        for expected in [4, 3, 7, 8] {
            app.apply(Action::MoveDown).unwrap();
            assert_eq!(app.selected(), Some(id(expected)));
        }
        // J on the context parent 3 moves its group among displayed top-level rows.
        app.apply(Action::MoveUp).unwrap();
        app.apply(Action::MoveUp).unwrap();
        assert_eq!(app.selected(), Some(id(3)));
        assert_eq!(
            app.prepare(Action::MoveTaskDown).unwrap(),
            Transition::Submit
        );
        let request = app.take_mutation().unwrap();
        let Mutation::MoveAdjacent {
            id: moved,
            neighbor,
            ..
        } = &request
        else {
            panic!("{request:?}");
        };
        assert_eq!((*moved, *neighbor), (id(3), Some(id(8))));
        let mut latest = app.tasks().clone();
        assert!(request.apply(&mut latest).changed());
        assert_eq!(
            latest
                .visible_tasks()
                .map(|task| task.id().get())
                .collect::<Vec<_>>(),
            [1, 4, 5, 2, 8, 3, 6, 7]
        );
        // A subtask's only displayed sibling is itself, so K finds no neighbor.
        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(id(7)));
        assert_eq!(app.prepare(Action::MoveTaskUp).unwrap(), Transition::Submit);
        let Some(Mutation::MoveAdjacent { neighbor, .. }) = app.take_mutation() else {
            panic!("expected a move");
        };
        assert_eq!(neighbor, None);
    }

    #[test]
    fn selection_should_survive_reparenting_and_fall_back_in_display_order_after_a_co_delete() {
        let mut list = tree();
        let mut app = App::new(list.clone());
        for _ in 0..5 {
            app.apply(Action::MoveDown).unwrap();
        }
        assert_eq!(app.selected(), Some(id(6)));
        list.set_parent(id(6), None).unwrap();
        assert!(app.reconcile(list.clone()));
        assert_eq!(ids(&app), [1, 4, 5, 2, 3, 7, 6, 8]);
        assert_eq!(app.selected(), Some(id(6)));
        list.set_parent(id(6), Some(id(1))).unwrap();
        assert!(app.reconcile(list.clone()));
        assert_eq!(ids(&app), [1, 4, 5, 6, 2, 3, 7, 8]);
        assert_eq!(app.selected(), Some(id(6)));
        // Deleting 1 moves its group to trash; the selection takes the next displayed row.
        list.delete(id(1)).unwrap();
        assert!(app.reconcile(list));
        assert_eq!(ids(&app), [2, 3, 7, 8]);
        assert_eq!(app.selected(), Some(id(2)));
    }

    #[test]
    fn trash_restore_of_a_subtask_under_a_deleted_parent_should_name_the_parent_and_submit_nothing()
    {
        let mut list = tree();
        list.delete(id(3)).unwrap();
        let mut app = App::new(list);
        app.apply(Action::OpenTrash).unwrap();
        let trash = app
            .tasks()
            .deleted_tasks()
            .iter()
            .map(|task| task.id().get())
            .collect::<Vec<_>>();
        assert_eq!(trash, [3, 6, 7]);
        app.apply(Action::MoveDown).unwrap();
        assert_eq!(app.selected(), Some(id(6)));
        assert_eq!(
            app.apply(Action::RestoreSelected).unwrap(),
            Transition::Transient
        );
        assert_eq!(app.notice(), Some(&Notice::RestoreParentFirst(id(3))));
        assert_eq!(app.take_mutation(), None);
        // The parent row restores the group.
        app.apply(Action::MoveUp).unwrap();
        assert_eq!(
            app.prepare(Action::RestoreSelected).unwrap(),
            Transition::Submit
        );
        assert!(matches!(
            app.take_mutation(),
            Some(Mutation::Restore { id: restored, .. }) if restored == id(3)
        ));
    }
}
