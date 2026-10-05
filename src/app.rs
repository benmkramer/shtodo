use crate::{
    action::Action,
    task::{ListError, MoveDirection, Task, TaskId, TaskList},
};

const CELEBRATION_FRAME_COUNT: u16 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Normal,
    Insert,
    Search,
    Help,
    Trash,
}

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

    fn next(self) -> Self {
        match self {
            Self::All => Self::Open,
            Self::Open => Self::Done,
            Self::Done => Self::All,
        }
    }

    fn previous(self) -> Self {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditKind {
    Add,
    Edit(TaskId),
    Search,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Editor {
    kind: EditKind,
    buffer: String,
    cursor: usize,
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
    Persisted,
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

pub(crate) struct App {
    tasks: TaskList,
    mode: Mode,
    selected: Option<TaskId>,
    trash: Option<TrashState>,
    editor: Option<Editor>,
    message: Option<String>,
    celebration: Option<Celebration>,
    view: TaskView,
    query: String,
    lowercase_query: String,
    search_session: Option<SearchSession>,
}

struct SearchSession {
    query: String,
    selected: Option<TaskId>,
}

impl App {
    pub(crate) fn new(tasks: TaskList) -> Self {
        let selected = tasks.visible_tasks().next().map(Task::id);
        Self {
            tasks,
            mode: Mode::Normal,
            selected,
            trash: None,
            editor: None,
            message: None,
            celebration: None,
            view: TaskView::All,
            query: String::new(),
            lowercase_query: String::new(),
            search_session: None,
        }
    }

    pub(crate) fn apply(&mut self, action: Action) -> Result<Transition, ListError> {
        match self.mode {
            Mode::Normal => self.apply_normal(action),
            Mode::Insert => self.apply_insert(action),
            Mode::Search => self.apply_search(action),
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
        self.view
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// The transient TUI projection, in canonical order. Storage visibility stays unchanged.
    pub(crate) fn visible_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.visible_tasks().filter(|task| self.matches(task))
    }

    fn matches(&self, task: &Task) -> bool {
        !task.is_deleted()
            && self.view.matches(task)
            && (self.lowercase_query.is_empty()
                || task.text().to_lowercase().contains(&self.lowercase_query))
    }

    pub(crate) fn tasks(&self) -> &TaskList {
        &self.tasks
    }

    pub(crate) fn editor(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }

    pub(crate) fn message(&self) -> Option<&str> {
        self.message.as_deref()
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
        let Some(selected) = self.selected() else {
            self.message = Some("Nothing to restore".into());
            return Ok(Transition::Transient);
        };
        let deleted = self.tasks.deleted_tasks();
        let next = deleted
            .iter()
            .position(|task| task.id() == selected)
            .and_then(|index| {
                deleted
                    .get(index + 1)
                    .or_else(|| index.checked_sub(1).and_then(|index| deleted.get(index)))
            })
            .map(|task| task.id());
        if !self.tasks.restore(selected)? {
            return Ok(Transition::Unchanged);
        }
        if let Some(trash) = &mut self.trash {
            trash.selected = next;
        }
        if self
            .tasks
            .task(selected)
            .is_some_and(|task| self.matches(task))
        {
            self.selected = Some(selected);
        } else {
            self.reconcile_selection();
            self.message = Some("Restored task is hidden by the current view/search".into());
        }
        Ok(Transition::Persisted)
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
                    self.lowercase_query = self.query.to_lowercase();
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
                    self.lowercase_query = self.query.to_lowercase();
                    self.reconcile_selection();
                }
                Ok(transition)
            }
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
        });
        self.mode = Mode::Search;
        Transition::Transient
    }

    fn cycle_view(&mut self, direction: MoveDirection) -> Transition {
        self.view = match direction {
            MoveDirection::Down => self.view.next(),
            MoveDirection::Up => self.view.previous(),
        };
        self.reconcile_selection();
        Transition::Transient
    }

    fn clear_search(&mut self) -> Transition {
        if self.query.is_empty() {
            return Transition::Unchanged;
        }
        self.query.clear();
        self.lowercase_query.clear();
        self.reconcile_selection();
        Transition::Transient
    }

    /// Keep identity when possible, otherwise choose the next match, then the previous one.
    fn reconcile_selection(&mut self) {
        let mut previous = None;
        let mut passed_selected = self.selected.is_none();
        for task in self.tasks.visible_tasks() {
            if Some(task.id()) == self.selected {
                if self.matches(task) {
                    return;
                }
                passed_selected = true;
            } else if self.matches(task) {
                if passed_selected {
                    self.selected = Some(task.id());
                    return;
                }
                previous = Some(task.id());
            }
        }
        self.selected = previous;
    }

    fn adjacent_visible(&self, selected: TaskId, direction: MoveDirection) -> Option<TaskId> {
        if self.view == TaskView::All && self.query.is_empty() {
            return self.tasks.adjacent_visible(selected, direction);
        }
        let mut previous = None;
        let mut found = false;
        for task in self.visible_tasks() {
            if found {
                return Some(task.id());
            }
            if task.id() == selected {
                match direction {
                    MoveDirection::Up => return previous,
                    MoveDirection::Down => found = true,
                }
            }
            previous = Some(task.id());
        }
        None
    }

    fn clear_message(&mut self) -> bool {
        self.message.take().is_some()
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
        let Some(next) = self.adjacent_visible(selected, direction) else {
            return Transition::Unchanged;
        };
        self.selected = Some(next);
        Transition::Transient
    }

    fn move_task(&mut self, direction: MoveDirection) -> Result<Transition, ListError> {
        let Some(selected) = self.selected else {
            return Ok(Transition::Unchanged);
        };
        let changed = if self.view == TaskView::All && self.query.is_empty() {
            self.tasks.move_visible(selected, direction)?
        } else if let Some(neighbor) = self.adjacent_visible(selected, direction) {
            self.tasks.swap_visible(selected, neighbor)?
        } else {
            false
        };
        if changed {
            Ok(Transition::Persisted)
        } else {
            Ok(Transition::Unchanged)
        }
    }

    fn toggle_complete(&mut self) -> Result<Transition, ListError> {
        let Some(selected) = self.selected else {
            return Ok(Transition::Unchanged);
        };
        let had_open_tasks = self.tasks.visible_tasks().any(|task| !task.completed());
        self.tasks.toggle_complete(selected)?;
        let has_open_tasks = self.tasks.visible_tasks().any(|task| !task.completed());
        self.celebration = (had_open_tasks && !has_open_tasks).then_some(Celebration { frame: 0 });
        self.reconcile_selection();
        Ok(Transition::Persisted)
    }

    fn delete_selected(&mut self) -> Result<Transition, ListError> {
        let Some(selected) = self.selected else {
            return Ok(Transition::Unchanged);
        };
        let next_selected = self
            .adjacent_visible(selected, MoveDirection::Down)
            .or_else(|| self.adjacent_visible(selected, MoveDirection::Up));
        self.tasks.delete(selected)?;
        self.selected = next_selected;
        Ok(Transition::Persisted)
    }

    fn restore_latest(&mut self) -> Result<Transition, ListError> {
        let Some(restored) = self.tasks.restore_latest()? else {
            self.message = Some("Nothing to restore".into());
            return Ok(Transition::Transient);
        };
        if self
            .tasks
            .task(restored)
            .is_some_and(|task| self.matches(task))
        {
            self.selected = Some(restored);
        } else {
            self.reconcile_selection();
            self.message = Some("Restored task is hidden by the current view/search".into());
        }
        Ok(Transition::Persisted)
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
            buffer,
            cursor,
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
        if !matches!(self.mode, Mode::Insert | Mode::Search) {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        editor.insert(character);
        Transition::Transient
    }

    fn move_cursor_left(&mut self) -> Transition {
        if !matches!(self.mode, Mode::Insert | Mode::Search) {
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
        if !matches!(self.mode, Mode::Insert | Mode::Search) {
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
        if !matches!(self.mode, Mode::Insert | Mode::Search) {
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
        if !matches!(self.mode, Mode::Insert | Mode::Search) {
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
        if !matches!(self.mode, Mode::Insert | Mode::Search) {
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
        if !matches!(self.mode, Mode::Insert | Mode::Search) {
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

        let kind = editor.kind;
        let text = editor.buffer.clone();
        self.message = None;
        match kind {
            EditKind::Add => {
                let id = self.tasks.add(&text)?;
                if self.tasks.task(id).is_some_and(|task| self.matches(task)) {
                    self.selected = Some(id);
                } else {
                    self.reconcile_selection();
                    self.message = Some("Added task is hidden by the current view/search".into());
                }
            }
            EditKind::Edit(id) => {
                if self.tasks.task(id).map(Task::text) == Some(text.trim()) {
                    self.mode = Mode::Normal;
                    self.editor = None;
                    return Ok(Transition::Transient);
                }
                self.tasks.edit(id, &text)?;
                self.reconcile_selection();
                if self.selected != Some(id) {
                    self.message = Some("Edited task is hidden by the current view/search".into());
                }
            }
            EditKind::Search => return Ok(Transition::Unchanged),
        }
        self.mode = Mode::Normal;
        self.editor = None;
        Ok(Transition::Persisted)
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
            Transition::Persisted
        );
        assert_eq!(app.selected(), Some(oldest));
        assert_eq!(app.mode(), Mode::Trash);
        assert_eq!(
            app.apply(Action::RestoreSelected).unwrap(),
            Transition::Persisted
        );
        assert_eq!(app.selected(), Some(newest));
        assert_eq!(
            app.apply(Action::RestoreSelected).unwrap(),
            Transition::Persisted
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

        assert_eq!(
            app.apply(Action::CommitEdit).unwrap(),
            Transition::Persisted
        );
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

        assert_eq!(app.apply(Action::Delete).unwrap(), Transition::Persisted);
        assert_eq!(app.selected(), Some(second));
        assert_eq!(
            app.apply(Action::RestoreLatest).unwrap(),
            Transition::Persisted
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
            Transition::Persisted
        );
        assert!(app.tasks().task(first).unwrap().completed());
        assert_eq!(
            app.apply(Action::MoveTaskUp).unwrap(),
            Transition::Unchanged
        );
        assert_eq!(
            app.apply(Action::MoveTaskDown).unwrap(),
            Transition::Persisted
        );
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
                Transition::Persisted
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
            Transition::Persisted
        );
        assert_eq!(app.selected(), Some(selected));
        assert_eq!(visible_ids(&app), vec![selected]);
        assert!(!app.tasks().task(hidden).unwrap().is_deleted());
        assert_eq!(
            app.message(),
            Some("Restored task is hidden by the current view/search")
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
        assert_eq!(
            app.apply(Action::CommitEdit).unwrap(),
            Transition::Persisted
        );
        assert_eq!(app.selected(), Some(second));
        assert_eq!(app.tasks().task(first).unwrap().text(), "different");
        assert_eq!(app.tasks().task(hidden).unwrap().text(), "hidden");
        assert_eq!(
            app.message(),
            Some("Edited task is hidden by the current view/search")
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
            Some("Added task is hidden by the current view/search")
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
        assert_eq!(
            app.apply(Action::MoveTaskDown).unwrap(),
            Transition::Persisted
        );
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
