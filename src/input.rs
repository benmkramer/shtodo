use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{action::Action, app::Mode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct KeyChord {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl KeyChord {
    const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    fn from_event(event: KeyEvent) -> Self {
        let mut modifiers = event.modifiers;
        if matches!(event.code, KeyCode::Char(_)) {
            modifiers.remove(KeyModifiers::SHIFT);
        }
        Self::new(event.code, modifiers)
    }

    fn is_control_c(self) -> bool {
        self.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(self.code, KeyCode::Char('c' | 'C'))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Binding {
    pub(crate) mode: Mode,
    pub(crate) chord: KeyChord,
    pub(crate) action: Action,
    pub(crate) key_label: &'static str,
    pub(crate) description: &'static str,
    pub(crate) show_in_footer: bool,
}

static BINDINGS: &[Binding] = &[
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('j'), KeyModifiers::NONE),
        action: Action::MoveDown,
        key_label: "j",
        description: "move down",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Down, KeyModifiers::NONE),
        action: Action::MoveDown,
        key_label: "Down",
        description: "move down",
        show_in_footer: false,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('k'), KeyModifiers::NONE),
        action: Action::MoveUp,
        key_label: "k",
        description: "move up",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Up, KeyModifiers::NONE),
        action: Action::MoveUp,
        key_label: "Up",
        description: "move up",
        show_in_footer: false,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('J'), KeyModifiers::NONE),
        action: Action::MoveTaskDown,
        key_label: "J",
        description: "move task down",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('K'), KeyModifiers::NONE),
        action: Action::MoveTaskUp,
        key_label: "K",
        description: "move task up",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('i'), KeyModifiers::NONE),
        action: Action::StartAdd,
        key_label: "i",
        description: "add task",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('e'), KeyModifiers::NONE),
        action: Action::StartEdit,
        key_label: "e",
        description: "edit task",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char(' '), KeyModifiers::NONE),
        action: Action::ToggleComplete,
        key_label: "Space",
        description: "toggle complete",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('d'), KeyModifiers::NONE),
        action: Action::Delete,
        key_label: "d",
        description: "delete task",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('u'), KeyModifiers::NONE),
        action: Action::RestoreLatest,
        key_label: "u",
        description: "restore latest",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('?'), KeyModifiers::NONE),
        action: Action::OpenHelp,
        key_label: "?",
        description: "show help",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('q'), KeyModifiers::NONE),
        action: Action::Quit,
        key_label: "q",
        description: "quit",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Normal,
        chord: KeyChord::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        action: Action::Quit,
        key_label: "Ctrl-C",
        description: "quit",
        show_in_footer: false,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::Left, KeyModifiers::NONE),
        action: Action::MoveCursorLeft,
        key_label: "Left",
        description: "move cursor left",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::Right, KeyModifiers::NONE),
        action: Action::MoveCursorRight,
        key_label: "Right",
        description: "move cursor right",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::Home, KeyModifiers::NONE),
        action: Action::MoveCursorStart,
        key_label: "Home",
        description: "move cursor start",
        show_in_footer: false,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::End, KeyModifiers::NONE),
        action: Action::MoveCursorEnd,
        key_label: "End",
        description: "move cursor end",
        show_in_footer: false,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::Backspace, KeyModifiers::NONE),
        action: Action::DeleteBeforeCursor,
        key_label: "Backspace",
        description: "delete before cursor",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::Delete, KeyModifiers::NONE),
        action: Action::DeleteAtCursor,
        key_label: "Delete",
        description: "delete at cursor",
        show_in_footer: false,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::Enter, KeyModifiers::NONE),
        action: Action::CommitEdit,
        key_label: "Enter",
        description: "save edit",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::Esc, KeyModifiers::NONE),
        action: Action::CancelEdit,
        key_label: "Esc",
        description: "cancel edit",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Insert,
        chord: KeyChord::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        action: Action::Quit,
        key_label: "Ctrl-C",
        description: "quit",
        show_in_footer: false,
    },
    Binding {
        mode: Mode::Help,
        chord: KeyChord::new(KeyCode::Char('?'), KeyModifiers::NONE),
        action: Action::CloseHelp,
        key_label: "?",
        description: "close help",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Help,
        chord: KeyChord::new(KeyCode::Esc, KeyModifiers::NONE),
        action: Action::CloseHelp,
        key_label: "Esc",
        description: "close help",
        show_in_footer: true,
    },
    Binding {
        mode: Mode::Help,
        chord: KeyChord::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        action: Action::Quit,
        key_label: "Ctrl-C",
        description: "quit",
        show_in_footer: false,
    },
];

pub(crate) fn bindings() -> impl Iterator<Item = &'static Binding> {
    BINDINGS.iter()
}

pub(crate) fn bindings_for(mode: Mode) -> impl Iterator<Item = &'static Binding> {
    bindings().filter(move |binding| binding.mode == mode)
}

pub(crate) fn map_key(mode: Mode, event: KeyEvent) -> Option<Action> {
    if !matches!(event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }

    let chord = KeyChord::from_event(event);
    if chord.is_control_c() {
        return Some(Action::Quit);
    }

    if let Some(binding) = bindings_for(mode).find(|binding| binding.chord == chord) {
        return Some(binding.action);
    }

    match (mode, chord) {
        (
            Mode::Insert,
            KeyChord {
                code: KeyCode::Char(character),
                modifiers,
            },
        ) if modifiers == KeyModifiers::NONE => Some(Action::InsertChar(character)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{bindings_for, map_key};
    use crate::{action::Action, app::Mode};

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, kind)
    }

    #[test]
    fn normal_keys_should_map_to_semantic_actions() {
        let cases = [
            (KeyCode::Char('j'), KeyModifiers::NONE, Action::MoveDown),
            (KeyCode::Down, KeyModifiers::NONE, Action::MoveDown),
            (KeyCode::Char('k'), KeyModifiers::NONE, Action::MoveUp),
            (KeyCode::Up, KeyModifiers::NONE, Action::MoveUp),
            (
                KeyCode::Char('J'),
                KeyModifiers::SHIFT,
                Action::MoveTaskDown,
            ),
            (KeyCode::Char('K'), KeyModifiers::SHIFT, Action::MoveTaskUp),
            (KeyCode::Char('i'), KeyModifiers::NONE, Action::StartAdd),
            (KeyCode::Char('e'), KeyModifiers::NONE, Action::StartEdit),
            (
                KeyCode::Char(' '),
                KeyModifiers::NONE,
                Action::ToggleComplete,
            ),
            (KeyCode::Char('d'), KeyModifiers::NONE, Action::Delete),
            (
                KeyCode::Char('u'),
                KeyModifiers::NONE,
                Action::RestoreLatest,
            ),
            (KeyCode::Char('?'), KeyModifiers::SHIFT, Action::OpenHelp),
            (KeyCode::Char('q'), KeyModifiers::NONE, Action::Quit),
        ];
        for (code, modifiers, expected) in cases {
            assert_eq!(
                map_key(Mode::Normal, key(code, modifiers, KeyEventKind::Press)),
                Some(expected)
            );
        }
    }

    #[test]
    fn q_should_type_in_insert_and_quit_in_normal() {
        let q = key(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press);
        assert_eq!(map_key(Mode::Insert, q), Some(Action::InsertChar('q')));
        assert_eq!(map_key(Mode::Normal, q), Some(Action::Quit));
    }

    #[test]
    fn control_c_should_quit_in_every_mode_and_release_should_be_ignored() {
        let control_c = key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Repeat,
        );
        assert_eq!(map_key(Mode::Normal, control_c), Some(Action::Quit));
        assert_eq!(map_key(Mode::Insert, control_c), Some(Action::Quit));
        assert_eq!(map_key(Mode::Help, control_c), Some(Action::Quit));
        let control_shift_c = key(
            KeyCode::Char('C'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyEventKind::Press,
        );
        assert_eq!(map_key(Mode::Insert, control_shift_c), Some(Action::Quit));
        assert_eq!(
            map_key(
                Mode::Normal,
                key(
                    KeyCode::Char('q'),
                    KeyModifiers::NONE,
                    KeyEventKind::Release
                )
            ),
            None
        );
    }

    #[test]
    fn normal_descriptions_should_include_help_and_reorder() {
        let descriptions = bindings_for(Mode::Normal)
            .map(|binding| binding.description)
            .collect::<Vec<_>>();
        assert!(descriptions.contains(&"show help"));
        assert!(descriptions.contains(&"move task down"));
    }

    #[test]
    fn editing_keys_should_map_to_editor_actions() {
        let cases = [
            (KeyCode::Left, Action::MoveCursorLeft),
            (KeyCode::Right, Action::MoveCursorRight),
            (KeyCode::Home, Action::MoveCursorStart),
            (KeyCode::End, Action::MoveCursorEnd),
            (KeyCode::Backspace, Action::DeleteBeforeCursor),
            (KeyCode::Delete, Action::DeleteAtCursor),
            (KeyCode::Enter, Action::CommitEdit),
            (KeyCode::Esc, Action::CancelEdit),
        ];
        for (code, expected) in cases {
            assert_eq!(
                map_key(
                    Mode::Insert,
                    key(code, KeyModifiers::NONE, KeyEventKind::Press)
                ),
                Some(expected)
            );
        }
    }

    #[test]
    fn help_keys_should_close_overlay() {
        for code in [KeyCode::Char('?'), KeyCode::Esc] {
            assert_eq!(
                map_key(
                    Mode::Help,
                    key(code, KeyModifiers::NONE, KeyEventKind::Press)
                ),
                Some(Action::CloseHelp)
            );
        }
    }

    #[test]
    fn insert_should_accept_repeat_and_unmodified_printable_unicode() {
        assert_eq!(
            map_key(
                Mode::Insert,
                key(KeyCode::Char('é'), KeyModifiers::NONE, KeyEventKind::Repeat)
            ),
            Some(Action::InsertChar('é'))
        );
    }

    #[test]
    fn insert_should_reject_modified_printable_characters() {
        for modifiers in [
            KeyModifiers::ALT,
            KeyModifiers::SUPER,
            KeyModifiers::HYPER,
            KeyModifiers::CONTROL,
        ] {
            assert_eq!(
                map_key(
                    Mode::Insert,
                    key(KeyCode::Char('x'), modifiers, KeyEventKind::Press)
                ),
                None
            );
        }
    }
}
