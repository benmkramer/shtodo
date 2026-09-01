use crate::{
    action::Action,
    task::{ListError, MoveDirection, Task, TaskId, TaskList},
};

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Transition {
    Unchanged,
    Transient,
    Persisted,
    Quit,
}

pub(crate) struct App {
    tasks: TaskList,
    mode: Mode,
    selected: Option<TaskId>,
    editor: Option<Editor>,
    message: Option<String>,
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
        }
    }

    pub(crate) fn apply(&mut self, action: Action) -> Result<Transition, ListError> {
        match action {
            Action::MoveDown => Ok(self.move_selection(MoveDirection::Down)),
            Action::MoveUp => Ok(self.move_selection(MoveDirection::Up)),
            Action::StartAdd => Ok(self.start_add()),
            Action::StartEdit => Ok(self.start_edit()),
            Action::OpenHelp => Ok(self.open_help()),
            Action::CloseHelp => Ok(self.close_help()),
            Action::InsertChar(character) => Ok(self.insert_char(character)),
            Action::MoveCursorLeft => Ok(self.move_cursor_left()),
            Action::MoveCursorRight => Ok(self.move_cursor_right()),
            Action::MoveCursorStart => Ok(self.move_cursor_start()),
            Action::MoveCursorEnd => Ok(self.move_cursor_end()),
            Action::DeleteBeforeCursor => Ok(self.delete_before_cursor()),
            Action::DeleteAtCursor => Ok(self.delete_at_cursor()),
            Action::CommitEdit => self.commit_edit(),
            Action::CancelEdit => Ok(self.cancel_edit()),
            Action::Quit => Ok(Transition::Quit),
            Action::MoveTaskDown
            | Action::MoveTaskUp
            | Action::ToggleComplete
            | Action::Delete
            | Action::RestoreLatest => Ok(Transition::Unchanged),
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
            EditKind::Edit(id) => self.tasks.edit(id, &text)?,
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
}
