use crate::{
    action::Action,
    task::{ListError, MoveDirection, Task, TaskId, TaskList},
};

const CELEBRATION_FRAME_COUNT: u16 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Normal,
    Insert,
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditKind {
    Add,
    Edit(TaskId),
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

pub(crate) struct App {
    tasks: TaskList,
    mode: Mode,
    selected: Option<TaskId>,
    editor: Option<Editor>,
    message: Option<String>,
    celebration: Option<Celebration>,
}

impl App {
    pub(crate) fn new(tasks: TaskList) -> Self {
        let selected = tasks.visible_tasks().next().map(Task::id);
        Self {
            tasks,
            mode: Mode::Normal,
            selected,
            editor: None,
            message: None,
            celebration: None,
        }
    }

    pub(crate) fn apply(&mut self, action: Action) -> Result<Transition, ListError> {
        match self.mode {
            Mode::Normal => self.apply_normal(action),
            Mode::Insert => self.apply_insert(action),
            Mode::Help => self.apply_help(action),
        }
    }

    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    pub(crate) fn selected(&self) -> Option<TaskId> {
        self.selected
    }

    pub(crate) fn selected_task(&self) -> Option<&Task> {
        self.selected.and_then(|id| self.tasks.task(id))
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
            Action::OpenHelp => self.open_help(),
            Action::Quit => Transition::Quit,
            _ => Transition::Unchanged,
        };
        Ok(Self::after_message_clear(transition, message_was_cleared))
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
        let Some(next) = self.tasks.adjacent_visible(selected, direction) else {
            return Transition::Unchanged;
        };
        self.selected = Some(next);
        Transition::Transient
    }

    fn move_task(&mut self, direction: MoveDirection) -> Result<Transition, ListError> {
        let Some(selected) = self.selected else {
            return Ok(Transition::Unchanged);
        };
        if self.tasks.move_visible(selected, direction)? {
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
        Ok(Transition::Persisted)
    }

    fn delete_selected(&mut self) -> Result<Transition, ListError> {
        let Some(selected) = self.selected else {
            return Ok(Transition::Unchanged);
        };
        let next_selected = self
            .tasks
            .adjacent_visible(selected, MoveDirection::Down)
            .or_else(|| self.tasks.adjacent_visible(selected, MoveDirection::Up));
        self.tasks.delete(selected)?;
        self.selected = next_selected;
        Ok(Transition::Persisted)
    }

    fn restore_latest(&mut self) -> Result<Transition, ListError> {
        let Some(restored) = self.tasks.restore_latest()? else {
            self.message = Some("Nothing to restore".into());
            return Ok(Transition::Transient);
        };
        self.selected = Some(restored);
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
        if self.mode != Mode::Normal {
            return Transition::Unchanged;
        }
        self.mode = Mode::Help;
        Transition::Transient
    }

    fn close_help(&mut self) -> Transition {
        if self.mode != Mode::Help {
            return Transition::Unchanged;
        }
        self.mode = Mode::Normal;
        Transition::Transient
    }

    fn insert_char(&mut self, character: char) -> Transition {
        if self.mode != Mode::Insert {
            return Transition::Unchanged;
        }
        let Some(editor) = self.editor.as_mut() else {
            return Transition::Unchanged;
        };
        editor.insert(character);
        Transition::Transient
    }

    fn move_cursor_left(&mut self) -> Transition {
        if self.mode != Mode::Insert {
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
        if self.mode != Mode::Insert {
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
        if self.mode != Mode::Insert {
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
        if self.mode != Mode::Insert {
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
        if self.mode != Mode::Insert {
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
        if self.mode != Mode::Insert {
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
        match kind {
            EditKind::Add => {
                let id = self.tasks.add(&text)?;
                self.selected = Some(id);
            }
            EditKind::Edit(id) => {
                if self.tasks.task(id).map(Task::text) == Some(text.trim()) {
                    self.mode = Mode::Normal;
                    self.editor = None;
                    return Ok(Transition::Transient);
                }
                self.tasks.edit(id, &text)?;
            }
        }
        self.mode = Mode::Normal;
        self.editor = None;
        self.message = None;
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
