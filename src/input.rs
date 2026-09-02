use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{action::Action, app::Mode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BindingId {
    MoveDown,
    MoveUp,
    MoveTaskDown,
    MoveTaskUp,
    StartAdd,
    StartEdit,
    ToggleComplete,
    Delete,
    RestoreLatest,
    OpenHelp,
    NormalQuit,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorStart,
    MoveCursorEnd,
    MoveWordLeft,
    MoveWordRight,
    DeleteBeforeCursor,
    DeleteAtCursor,
    DeleteWordBeforeCursor,
    DeleteWordAtCursor,
    CommitEdit,
    CancelEdit,
    CloseHelp,
    InsertEmergencyQuit,
    HelpEmergencyQuit,
}

impl BindingId {
    pub(crate) fn from_config(mode: Mode, name: &str) -> Option<Self> {
        Some(match (mode, name) {
            (Mode::Normal, "move_down") => Self::MoveDown,
            (Mode::Normal, "move_up") => Self::MoveUp,
            (Mode::Normal, "move_task_down") => Self::MoveTaskDown,
            (Mode::Normal, "move_task_up") => Self::MoveTaskUp,
            (Mode::Normal, "add_task") => Self::StartAdd,
            (Mode::Normal, "edit_task") => Self::StartEdit,
            (Mode::Normal, "toggle_complete") => Self::ToggleComplete,
            (Mode::Normal, "delete_task") => Self::Delete,
            (Mode::Normal, "restore_latest") => Self::RestoreLatest,
            (Mode::Normal, "open_help") => Self::OpenHelp,
            (Mode::Normal, "quit") => Self::NormalQuit,
            (Mode::Insert, "move_cursor_left") => Self::MoveCursorLeft,
            (Mode::Insert, "move_cursor_right") => Self::MoveCursorRight,
            (Mode::Insert, "move_cursor_start") => Self::MoveCursorStart,
            (Mode::Insert, "move_cursor_end") => Self::MoveCursorEnd,
            (Mode::Insert, "move_word_left") => Self::MoveWordLeft,
            (Mode::Insert, "move_word_right") => Self::MoveWordRight,
            (Mode::Insert, "delete_before_cursor") => Self::DeleteBeforeCursor,
            (Mode::Insert, "delete_at_cursor") => Self::DeleteAtCursor,
            (Mode::Insert, "delete_word_before_cursor") => Self::DeleteWordBeforeCursor,
            (Mode::Insert, "delete_word_at_cursor") => Self::DeleteWordAtCursor,
            (Mode::Insert, "commit_edit") => Self::CommitEdit,
            (Mode::Insert, "cancel_edit") => Self::CancelEdit,
            (Mode::Help, "close_help") => Self::CloseHelp,
            _ => return None,
        })
    }

    pub(crate) const fn config_name(self) -> Option<&'static str> {
        match self {
            Self::MoveDown => Some("move_down"),
            Self::MoveUp => Some("move_up"),
            Self::MoveTaskDown => Some("move_task_down"),
            Self::MoveTaskUp => Some("move_task_up"),
            Self::StartAdd => Some("add_task"),
            Self::StartEdit => Some("edit_task"),
            Self::ToggleComplete => Some("toggle_complete"),
            Self::Delete => Some("delete_task"),
            Self::RestoreLatest => Some("restore_latest"),
            Self::OpenHelp => Some("open_help"),
            Self::NormalQuit => Some("quit"),
            Self::MoveCursorLeft => Some("move_cursor_left"),
            Self::MoveCursorRight => Some("move_cursor_right"),
            Self::MoveCursorStart => Some("move_cursor_start"),
            Self::MoveCursorEnd => Some("move_cursor_end"),
            Self::MoveWordLeft => Some("move_word_left"),
            Self::MoveWordRight => Some("move_word_right"),
            Self::DeleteBeforeCursor => Some("delete_before_cursor"),
            Self::DeleteAtCursor => Some("delete_at_cursor"),
            Self::DeleteWordBeforeCursor => Some("delete_word_before_cursor"),
            Self::DeleteWordAtCursor => Some("delete_word_at_cursor"),
            Self::CommitEdit => Some("commit_edit"),
            Self::CancelEdit => Some("cancel_edit"),
            Self::CloseHelp => Some("close_help"),
            Self::InsertEmergencyQuit | Self::HelpEmergencyQuit => None,
        }
    }

    pub(crate) const fn mode(self) -> Mode {
        match self {
            Self::MoveDown
            | Self::MoveUp
            | Self::MoveTaskDown
            | Self::MoveTaskUp
            | Self::StartAdd
            | Self::StartEdit
            | Self::ToggleComplete
            | Self::Delete
            | Self::RestoreLatest
            | Self::OpenHelp
            | Self::NormalQuit => Mode::Normal,
            Self::MoveCursorLeft
            | Self::MoveCursorRight
            | Self::MoveCursorStart
            | Self::MoveCursorEnd
            | Self::MoveWordLeft
            | Self::MoveWordRight
            | Self::DeleteBeforeCursor
            | Self::DeleteAtCursor
            | Self::DeleteWordBeforeCursor
            | Self::DeleteWordAtCursor
            | Self::CommitEdit
            | Self::CancelEdit
            | Self::InsertEmergencyQuit => Mode::Insert,
            Self::CloseHelp | Self::HelpEmergencyQuit => Mode::Help,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct KeyChord {
    code: KeyCode,
    modifiers: KeyModifiers,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KeyParseError(String);
impl fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for KeyParseError {}

impl KeyChord {
    const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, KeyParseError> {
        let mut modifiers = KeyModifiers::NONE;
        let mut remainder = value;
        loop {
            let lower = remainder.to_ascii_lowercase();
            if lower.starts_with("ctrl-") {
                if modifiers.contains(KeyModifiers::CONTROL) {
                    return Err(KeyParseError("duplicate Ctrl modifier".into()));
                }
                modifiers.insert(KeyModifiers::CONTROL);
                remainder = &remainder[5..];
            } else if lower.starts_with("alt-") {
                if modifiers.contains(KeyModifiers::ALT) {
                    return Err(KeyParseError("duplicate Alt modifier".into()));
                }
                modifiers.insert(KeyModifiers::ALT);
                remainder = &remainder[4..];
            } else if lower.starts_with("shift-") {
                return Err(KeyParseError("Shift modifier is not supported".into()));
            } else {
                break;
            }
        }
        if remainder.is_empty() {
            return Err(KeyParseError("key is missing after modifier".into()));
        }
        let lower = remainder.to_ascii_lowercase();
        let code = match lower.as_str() {
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "page-up" => KeyCode::PageUp,
            "page-down" => KeyCode::PageDown,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "enter" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            "space" => KeyCode::Char(' '),
            "backspace" => KeyCode::Backspace,
            "delete" => KeyCode::Delete,
            "insert" => KeyCode::Insert,
            _ => {
                let mut chars = remainder.chars();
                let character = chars
                    .next()
                    .filter(|_| chars.next().is_none())
                    .ok_or_else(|| KeyParseError(format!("unknown key {remainder:?}")))?;
                if character.is_control() {
                    return Err(KeyParseError("control characters are not supported".into()));
                }
                KeyCode::Char(normalize_character(character, modifiers))
            }
        };
        Ok(Self::new(code, modifiers))
    }

    fn from_event(event: KeyEvent) -> Self {
        let mut modifiers = event.modifiers;
        if matches!(event.code, KeyCode::Char(_) | KeyCode::BackTab) {
            modifiers.remove(KeyModifiers::SHIFT);
        }
        let code = match event.code {
            KeyCode::Char(character) => KeyCode::Char(normalize_character(character, modifiers)),
            code => code,
        };
        Self::new(code, modifiers)
    }

    pub(crate) fn label(&self) -> String {
        let mut label = String::new();
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            label.push_str("Ctrl-");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            label.push_str("Alt-");
        }
        label.push_str(match self.code {
            KeyCode::Up => "Up",
            KeyCode::Down => "Down",
            KeyCode::Left => "Left",
            KeyCode::Right => "Right",
            KeyCode::Home => "Home",
            KeyCode::End => "End",
            KeyCode::PageUp => "Page-Up",
            KeyCode::PageDown => "Page-Down",
            KeyCode::Tab => "Tab",
            KeyCode::BackTab => "Backtab",
            KeyCode::Enter => "Enter",
            KeyCode::Esc => "Esc",
            KeyCode::Backspace => "Backspace",
            KeyCode::Delete => "Delete",
            KeyCode::Insert => "Insert",
            KeyCode::Char(' ') => "Space",
            KeyCode::Char('c') if self.modifiers == KeyModifiers::CONTROL => "C",
            KeyCode::Char(character) => return format!("{label}{character}"),
            _ => "Unknown",
        });
        label
    }
}

const fn normalize_character(character: char, modifiers: KeyModifiers) -> char {
    if !modifiers.is_empty() && character.is_ascii_alphabetic() {
        character.to_ascii_lowercase()
    } else {
        character
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BindingOverride {
    pub(crate) order: usize,
    pub(crate) path: String,
    pub(crate) id: BindingId,
    pub(crate) keys: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KeymapIssue {
    pub(crate) order: usize,
    pub(crate) path: String,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedBinding {
    id: BindingId,
    mode: Mode,
    action: Action,
    description: &'static str,
    footer_priority: Option<u8>,
    chords: Vec<KeyChord>,
    labels: BindingLabels,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BindingLabels {
    preferred: String,
    aliases: Vec<String>,
}

impl BindingLabels {
    fn from_chords(chords: &[KeyChord]) -> Option<Self> {
        let mut labels = chords.iter().map(KeyChord::label);
        Some(Self {
            preferred: labels.next()?,
            aliases: labels.collect(),
        })
    }

    fn iter(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.preferred.as_str()).chain(self.aliases.iter().map(String::as_str))
    }
}

impl ResolvedBinding {
    pub(crate) const fn id(&self) -> BindingId {
        self.id
    }
    pub(crate) fn preferred_label(&self) -> &str {
        &self.labels.preferred
    }
    pub(crate) fn labels(&self) -> impl Iterator<Item = &str> {
        self.labels.iter()
    }
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "wired by Task 3 doctor integration")
    )]
    pub(crate) const fn action(&self) -> Action {
        self.action
    }
    pub(crate) const fn description(&self) -> &'static str {
        self.description
    }
    pub(crate) const fn footer_priority(&self) -> Option<u8> {
        self.footer_priority
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Keymap {
    bindings: Vec<ResolvedBinding>,
}

#[derive(Clone, Copy)]
struct Definition {
    id: BindingId,
    action: Action,
    description: &'static str,
    footer_priority: Option<u8>,
    defaults: &'static [KeyChord],
    fixed: &'static [KeyChord],
}
const fn chord(code: KeyCode, modifiers: KeyModifiers) -> KeyChord {
    KeyChord::new(code, modifiers)
}

static DEFINITIONS: &[Definition] = &[
    Definition {
        id: BindingId::MoveDown,
        action: Action::MoveDown,
        description: "move down",
        footer_priority: Some(2),
        defaults: &[
            chord(KeyCode::Char('j'), KeyModifiers::NONE),
            chord(KeyCode::Down, KeyModifiers::NONE),
        ],
        fixed: &[],
    },
    Definition {
        id: BindingId::MoveUp,
        action: Action::MoveUp,
        description: "move up",
        footer_priority: Some(2),
        defaults: &[
            chord(KeyCode::Char('k'), KeyModifiers::NONE),
            chord(KeyCode::Up, KeyModifiers::NONE),
        ],
        fixed: &[],
    },
    Definition {
        id: BindingId::MoveTaskDown,
        action: Action::MoveTaskDown,
        description: "move task down",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Char('J'), KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::MoveTaskUp,
        action: Action::MoveTaskUp,
        description: "move task up",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Char('K'), KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::StartAdd,
        action: Action::StartAdd,
        description: "add task",
        footer_priority: Some(0),
        defaults: &[chord(KeyCode::Char('i'), KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::StartEdit,
        action: Action::StartEdit,
        description: "edit task",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Char('e'), KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::ToggleComplete,
        action: Action::ToggleComplete,
        description: "toggle complete",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Char(' '), KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::Delete,
        action: Action::Delete,
        description: "delete task",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Char('d'), KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::RestoreLatest,
        action: Action::RestoreLatest,
        description: "restore latest",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Char('u'), KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::OpenHelp,
        action: Action::OpenHelp,
        description: "show help",
        footer_priority: Some(1),
        defaults: &[chord(KeyCode::Char('?'), KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::NormalQuit,
        action: Action::Quit,
        description: "quit",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Char('q'), KeyModifiers::NONE)],
        fixed: &[chord(KeyCode::Char('c'), KeyModifiers::CONTROL)],
    },
    Definition {
        id: BindingId::MoveCursorLeft,
        action: Action::MoveCursorLeft,
        description: "move cursor left",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Left, KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::MoveCursorRight,
        action: Action::MoveCursorRight,
        description: "move cursor right",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Right, KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::MoveCursorStart,
        action: Action::MoveCursorStart,
        description: "move cursor start",
        footer_priority: None,
        defaults: &[chord(KeyCode::Home, KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::MoveCursorEnd,
        action: Action::MoveCursorEnd,
        description: "move cursor end",
        footer_priority: None,
        defaults: &[chord(KeyCode::End, KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::MoveWordLeft,
        action: Action::MoveWordLeft,
        description: "move one word left",
        footer_priority: None,
        defaults: &[
            chord(KeyCode::Left, KeyModifiers::ALT),
            chord(KeyCode::Char('b'), KeyModifiers::ALT),
        ],
        fixed: &[],
    },
    Definition {
        id: BindingId::MoveWordRight,
        action: Action::MoveWordRight,
        description: "move one word right",
        footer_priority: None,
        defaults: &[
            chord(KeyCode::Right, KeyModifiers::ALT),
            chord(KeyCode::Char('f'), KeyModifiers::ALT),
        ],
        fixed: &[],
    },
    Definition {
        id: BindingId::DeleteBeforeCursor,
        action: Action::DeleteBeforeCursor,
        description: "delete before cursor",
        footer_priority: Some(2),
        defaults: &[chord(KeyCode::Backspace, KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::DeleteAtCursor,
        action: Action::DeleteAtCursor,
        description: "delete at cursor",
        footer_priority: None,
        defaults: &[chord(KeyCode::Delete, KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::DeleteWordBeforeCursor,
        action: Action::DeleteWordBeforeCursor,
        description: "delete previous word",
        footer_priority: None,
        defaults: &[
            chord(KeyCode::Backspace, KeyModifiers::ALT),
            chord(KeyCode::Char('w'), KeyModifiers::CONTROL),
        ],
        fixed: &[],
    },
    Definition {
        id: BindingId::DeleteWordAtCursor,
        action: Action::DeleteWordAtCursor,
        description: "delete next word",
        footer_priority: None,
        defaults: &[chord(KeyCode::Delete, KeyModifiers::ALT)],
        fixed: &[],
    },
    Definition {
        id: BindingId::CommitEdit,
        action: Action::CommitEdit,
        description: "save edit",
        footer_priority: Some(0),
        defaults: &[chord(KeyCode::Enter, KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::CancelEdit,
        action: Action::CancelEdit,
        description: "cancel edit",
        footer_priority: Some(1),
        defaults: &[chord(KeyCode::Esc, KeyModifiers::NONE)],
        fixed: &[],
    },
    Definition {
        id: BindingId::InsertEmergencyQuit,
        action: Action::Quit,
        description: "quit",
        footer_priority: None,
        defaults: &[],
        fixed: &[chord(KeyCode::Char('c'), KeyModifiers::CONTROL)],
    },
    Definition {
        id: BindingId::CloseHelp,
        action: Action::CloseHelp,
        description: "close help",
        footer_priority: Some(0),
        defaults: &[
            chord(KeyCode::Char('?'), KeyModifiers::NONE),
            chord(KeyCode::Esc, KeyModifiers::NONE),
        ],
        fixed: &[],
    },
    Definition {
        id: BindingId::HelpEmergencyQuit,
        action: Action::Quit,
        description: "quit",
        footer_priority: None,
        defaults: &[],
        fixed: &[chord(KeyCode::Char('c'), KeyModifiers::CONTROL)],
    },
];

impl Keymap {
    pub(crate) fn defaults() -> Self {
        Self {
            bindings: DEFINITIONS
                .iter()
                .filter_map(|definition| resolved(*definition, definition.defaults.to_vec()))
                .collect(),
        }
    }

    pub(crate) fn with_overrides(overrides: &[BindingOverride]) -> Result<Self, Vec<KeymapIssue>> {
        let mut ordered = overrides.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|override_| override_.order);
        let mut keys = DEFINITIONS
            .iter()
            .map(|definition| definition.defaults.to_vec())
            .collect::<Vec<_>>();
        let mut sources = vec![None::<(usize, String)>; DEFINITIONS.len()];
        let mut issues = Vec::new();
        for override_ in ordered {
            if override_.keys.is_empty() {
                issues.push(issue(override_, "must contain at least one key"));
                continue;
            }
            let mut parsed = Vec::new();
            for key in &override_.keys {
                match KeyChord::parse(key) {
                    Ok(chord)
                        if chord == KeyChord::new(KeyCode::Char('c'), KeyModifiers::CONTROL) =>
                    {
                        issues.push(issue(
                            override_,
                            "Ctrl-C is reserved and cannot be configured",
                        ))
                    }
                    Ok(chord) if parsed.contains(&chord) => issues.push(issue(
                        override_,
                        format!("duplicate key \"{}\"", chord.label()),
                    )),
                    Ok(chord) => parsed.push(chord),
                    Err(error) => {
                        issues.push(issue(override_, format!("invalid key {key:?}: {error}")))
                    }
                }
            }
            if !parsed.is_empty() {
                let index = definition_index(override_.id);
                keys[index] = parsed;
                sources[index] = Some((override_.order, override_.path.clone()));
            }
        }
        let bindings = DEFINITIONS
            .iter()
            .enumerate()
            .filter_map(|(index, definition)| resolved(*definition, keys[index].clone()))
            .collect::<Vec<_>>();
        let mut seen = Vec::<(KeyChord, usize)>::new();
        for (index, binding) in bindings.iter().enumerate() {
            for chord in &binding.chords {
                if let Some((_, previous)) = seen.iter().find(|(seen, previous)| {
                    seen == chord && bindings[*previous].mode == binding.mode
                }) {
                    let (report, other) = match (&sources[*previous], &sources[index]) {
                        (Some((old, _)), Some((new, _))) if old > new => (*previous, index),
                        (Some(_), Some(_)) => (index, *previous),
                        (Some(_), None) => (*previous, index),
                        (None, Some(_)) => (index, *previous),
                        (None, None) => continue,
                    };
                    let Some((order, path)) = sources[report].as_ref() else {
                        continue;
                    };
                    issues.push(KeymapIssue {
                        order: *order,
                        path: path.clone(),
                        message: format!(
                            "\"{}\" conflicts with {}",
                            chord.label(),
                            bindings[other].id.config_name().unwrap_or("quit")
                        ),
                    });
                } else {
                    seen.push((*chord, index));
                }
            }
        }
        issues.sort_by_key(|issue| issue.order);
        if issues.is_empty() {
            Ok(Self { bindings })
        } else {
            Err(issues)
        }
    }

    pub(crate) fn map_key(&self, mode: Mode, event: KeyEvent) -> Option<Action> {
        if !matches!(event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return None;
        }
        let chord = KeyChord::from_event(event);
        if let Some(binding) = self
            .bindings_for(mode)
            .find(|binding| binding.chords.contains(&chord))
        {
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

    pub(crate) fn bindings_for(&self, mode: Mode) -> impl Iterator<Item = &ResolvedBinding> {
        self.bindings
            .iter()
            .filter(move |binding| binding.mode == mode)
    }
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "wired by Task 3 doctor integration")
    )]
    pub(crate) fn configurable_action_count(&self) -> usize {
        self.bindings
            .iter()
            .filter(|binding| binding.id.config_name().is_some())
            .count()
    }
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "wired by Task 3 doctor integration")
    )]
    pub(crate) fn active_binding_count(&self) -> usize {
        self.bindings
            .iter()
            .map(|binding| binding.chords.len())
            .sum()
    }
}

fn resolved(definition: Definition, mut chords: Vec<KeyChord>) -> Option<ResolvedBinding> {
    chords.extend_from_slice(definition.fixed);
    let labels = BindingLabels::from_chords(&chords)?;
    Some(ResolvedBinding {
        id: definition.id,
        mode: definition.id.mode(),
        action: definition.action,
        description: definition.description,
        footer_priority: definition.footer_priority,
        chords,
        labels,
    })
}
fn definition_index(id: BindingId) -> usize {
    match id {
        BindingId::MoveDown => 0,
        BindingId::MoveUp => 1,
        BindingId::MoveTaskDown => 2,
        BindingId::MoveTaskUp => 3,
        BindingId::StartAdd => 4,
        BindingId::StartEdit => 5,
        BindingId::ToggleComplete => 6,
        BindingId::Delete => 7,
        BindingId::RestoreLatest => 8,
        BindingId::OpenHelp => 9,
        BindingId::NormalQuit => 10,
        BindingId::MoveCursorLeft => 11,
        BindingId::MoveCursorRight => 12,
        BindingId::MoveCursorStart => 13,
        BindingId::MoveCursorEnd => 14,
        BindingId::MoveWordLeft => 15,
        BindingId::MoveWordRight => 16,
        BindingId::DeleteBeforeCursor => 17,
        BindingId::DeleteAtCursor => 18,
        BindingId::DeleteWordBeforeCursor => 19,
        BindingId::DeleteWordAtCursor => 20,
        BindingId::CommitEdit => 21,
        BindingId::CancelEdit => 22,
        BindingId::InsertEmergencyQuit => 23,
        BindingId::CloseHelp => 24,
        BindingId::HelpEmergencyQuit => 25,
    }
}
fn issue(override_: &BindingOverride, message: impl Into<String>) -> KeymapIssue {
    KeymapIssue {
        order: override_.order,
        path: override_.path.clone(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{BindingId, BindingOverride, KeyChord, Keymap};
    use crate::{action::Action, app::Mode};

    fn pressed(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, KeyEventKind::Press)
    }

    #[test]
    fn key_chord_should_parse_supported_forms_and_generate_canonical_labels() {
        for (source, expected) in [
            ("J", "J"),
            ("down", "Down"),
            ("space", "Space"),
            ("ctrl-n", "Ctrl-n"),
            ("ALT-left", "Alt-Left"),
            ("ctrl-alt-x", "Ctrl-Alt-x"),
        ] {
            assert_eq!(KeyChord::parse(source).unwrap().label(), expected);
        }
    }

    #[test]
    fn with_overrides_should_replace_one_action_and_preserve_other_defaults() {
        let keymap = Keymap::with_overrides(&[BindingOverride {
            order: 0,
            path: "keybindings.normal.move_down".into(),
            id: BindingId::MoveDown,
            keys: vec!["x".into(), "ctrl-n".into()],
        }])
        .unwrap();
        assert_eq!(
            keymap.map_key(
                Mode::Normal,
                pressed(KeyCode::Char('x'), KeyModifiers::NONE)
            ),
            Some(Action::MoveDown)
        );
        assert_eq!(
            keymap.map_key(
                Mode::Normal,
                pressed(KeyCode::Char('j'), KeyModifiers::NONE)
            ),
            None
        );
        assert_eq!(
            keymap.map_key(
                Mode::Normal,
                pressed(KeyCode::Char('k'), KeyModifiers::NONE)
            ),
            Some(Action::MoveUp)
        );
    }

    #[test]
    fn with_overrides_should_collect_empty_duplicate_conflict_and_reserved_issues() {
        let result = Keymap::with_overrides(&[
            BindingOverride {
                order: 0,
                path: "keybindings.normal.move_down".into(),
                id: BindingId::MoveDown,
                keys: Vec::new(),
            },
            BindingOverride {
                order: 1,
                path: "keybindings.normal.move_up".into(),
                id: BindingId::MoveUp,
                keys: vec!["x".into(), "x".into()],
            },
            BindingOverride {
                order: 2,
                path: "keybindings.normal.add_task".into(),
                id: BindingId::StartAdd,
                keys: vec!["d".into(), "ctrl-c".into()],
            },
        ]);
        assert_eq!(
            result
                .unwrap_err()
                .into_iter()
                .map(|issue| issue.message)
                .collect::<Vec<_>>(),
            vec![
                "must contain at least one key",
                "duplicate key \"x\"",
                "Ctrl-C is reserved and cannot be configured",
                "\"d\" conflicts with delete_task"
            ]
        );
    }

    #[test]
    fn defaults_should_preserve_normal_insert_and_help_behavior() {
        let keymap = Keymap::defaults();
        for (mode, code, modifiers, expected) in [
            (
                Mode::Normal,
                KeyCode::Char('j'),
                KeyModifiers::NONE,
                Action::MoveDown,
            ),
            (
                Mode::Normal,
                KeyCode::Char('J'),
                KeyModifiers::SHIFT,
                Action::MoveTaskDown,
            ),
            (
                Mode::Insert,
                KeyCode::Char('B'),
                KeyModifiers::ALT | KeyModifiers::SHIFT,
                Action::MoveWordLeft,
            ),
            (
                Mode::Help,
                KeyCode::Esc,
                KeyModifiers::NONE,
                Action::CloseHelp,
            ),
            (
                Mode::Insert,
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                Action::Quit,
            ),
        ] {
            assert_eq!(
                keymap.map_key(mode, pressed(code, modifiers)),
                Some(expected)
            );
        }
    }

    #[test]
    fn defaults_should_map_all_normal_actions_and_keep_q_as_insert_text() {
        let keymap = Keymap::defaults();
        for (code, modifiers, action) in [
            (KeyCode::Down, KeyModifiers::NONE, Action::MoveDown),
            (KeyCode::Char('k'), KeyModifiers::NONE, Action::MoveUp),
            (KeyCode::Up, KeyModifiers::NONE, Action::MoveUp),
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
        ] {
            assert_eq!(
                keymap.map_key(Mode::Normal, pressed(code, modifiers)),
                Some(action)
            );
        }
        assert_eq!(
            keymap.map_key(
                Mode::Insert,
                pressed(KeyCode::Char('q'), KeyModifiers::NONE)
            ),
            Some(Action::InsertChar('q'))
        );
    }

    #[test]
    fn defaults_should_support_word_aliases_and_fixed_control_c_in_every_mode() {
        let keymap = Keymap::defaults();
        for (code, modifiers, action) in [
            (KeyCode::Left, KeyModifiers::ALT, Action::MoveWordLeft),
            (KeyCode::Char('b'), KeyModifiers::ALT, Action::MoveWordLeft),
            (KeyCode::Right, KeyModifiers::ALT, Action::MoveWordRight),
            (KeyCode::Char('f'), KeyModifiers::ALT, Action::MoveWordRight),
            (
                KeyCode::Backspace,
                KeyModifiers::ALT,
                Action::DeleteWordBeforeCursor,
            ),
            (
                KeyCode::Char('w'),
                KeyModifiers::CONTROL,
                Action::DeleteWordBeforeCursor,
            ),
            (
                KeyCode::Delete,
                KeyModifiers::ALT,
                Action::DeleteWordAtCursor,
            ),
        ] {
            assert_eq!(
                keymap.map_key(Mode::Insert, pressed(code, modifiers)),
                Some(action)
            );
        }
        for mode in [Mode::Normal, Mode::Insert, Mode::Help] {
            assert_eq!(
                keymap.map_key(
                    mode,
                    pressed(
                        KeyCode::Char('C'),
                        KeyModifiers::CONTROL | KeyModifiers::SHIFT
                    )
                ),
                Some(Action::Quit)
            );
            assert_eq!(
                keymap.map_key(
                    mode,
                    pressed(
                        KeyCode::Char('c'),
                        KeyModifiers::CONTROL | KeyModifiers::ALT
                    )
                ),
                None
            );
        }
    }

    #[test]
    fn defaults_should_map_all_standard_editor_actions_and_help_aliases() {
        let keymap = Keymap::defaults();
        for (code, action) in [
            (KeyCode::Left, Action::MoveCursorLeft),
            (KeyCode::Right, Action::MoveCursorRight),
            (KeyCode::Home, Action::MoveCursorStart),
            (KeyCode::End, Action::MoveCursorEnd),
            (KeyCode::Backspace, Action::DeleteBeforeCursor),
            (KeyCode::Delete, Action::DeleteAtCursor),
            (KeyCode::Enter, Action::CommitEdit),
            (KeyCode::Esc, Action::CancelEdit),
        ] {
            assert_eq!(
                keymap.map_key(Mode::Insert, pressed(code, KeyModifiers::NONE)),
                Some(action)
            );
        }
        for code in [KeyCode::Char('?'), KeyCode::Esc] {
            assert_eq!(
                keymap.map_key(Mode::Help, pressed(code, KeyModifiers::NONE)),
                Some(Action::CloseHelp)
            );
        }
    }

    #[test]
    fn control_c_should_have_one_group_in_each_mode_and_normal_should_describe_help_and_reorder() {
        let keymap = Keymap::defaults();
        for mode in [Mode::Normal, Mode::Insert, Mode::Help] {
            assert_eq!(
                keymap
                    .bindings_for(mode)
                    .filter(|binding| binding.labels().any(|label| label == "Ctrl-C"))
                    .count(),
                1
            );
        }
        let descriptions = keymap
            .bindings_for(Mode::Normal)
            .map(|binding| binding.description())
            .collect::<Vec<_>>();
        assert!(descriptions.contains(&"show help"));
        assert!(descriptions.contains(&"move task down"));
    }

    #[test]
    fn keymap_should_expose_configurable_and_active_binding_counts() {
        let keymap = Keymap::defaults();
        assert_eq!(keymap.configurable_action_count(), 24);
        assert_eq!(keymap.active_binding_count(), 33);
        assert_eq!(
            BindingId::from_config(Mode::Normal, "move_down"),
            Some(BindingId::MoveDown)
        );
        assert_eq!(BindingId::from_config(Mode::Help, "quit"), None);
        let binding = keymap
            .bindings_for(Mode::Normal)
            .find(|binding| binding.id() == BindingId::MoveDown)
            .unwrap();
        assert_eq!(binding.action(), Action::MoveDown);
    }

    #[test]
    fn map_key_should_ignore_releases_and_fall_back_to_unmodified_insert_characters() {
        let keymap = Keymap::defaults();
        assert_eq!(
            keymap.map_key(
                Mode::Normal,
                KeyEvent::new_with_kind(
                    KeyCode::Char('q'),
                    KeyModifiers::NONE,
                    KeyEventKind::Release
                )
            ),
            None
        );
        assert_eq!(
            keymap.map_key(
                Mode::Insert,
                KeyEvent::new_with_kind(
                    KeyCode::Char('é'),
                    KeyModifiers::NONE,
                    KeyEventKind::Repeat
                )
            ),
            Some(Action::InsertChar('é'))
        );
        assert_eq!(
            keymap.map_key(Mode::Insert, pressed(KeyCode::Char('x'), KeyModifiers::ALT)),
            None
        );
    }
}
