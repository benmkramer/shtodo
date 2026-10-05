use std::time::Duration;

use color_eyre::eyre::{Result, WrapErr};
use crossterm::event::{self, Event, KeyEventKind};

use crate::{
    action::Action,
    app::{App, Mode},
    input::Keymap,
    session::Session,
    storage::{Snapshot, Store},
    ui,
};

const CELEBRATION_FRAME_INTERVAL: Duration = Duration::from_millis(50);

struct TerminalGuard {
    terminal: ratatui::DefaultTerminal,
    restored: bool,
}

impl TerminalGuard {
    fn init() -> std::io::Result<Self> {
        match ratatui::try_init() {
            Ok(terminal) => Ok(Self {
                terminal,
                restored: false,
            }),
            Err(error) => {
                ratatui::restore();
                Err(error)
            }
        }
    }

    fn restore(&mut self) -> std::io::Result<()> {
        ratatui::try_restore()?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.restored {
            ratatui::restore();
        }
    }
}

fn action_for_event(keymap: &Keymap, mode: Mode, event: Event) -> Option<Action> {
    match event {
        Event::Key(key) => keymap.map_key(mode, key),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CelebrationEvent {
    Dismiss,
    Quit,
    Wait,
}

fn celebration_event(keymap: &Keymap, event: Event) -> CelebrationEvent {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if keymap.map_key(Mode::Normal, key) == Some(Action::Quit) {
                CelebrationEvent::Quit
            } else {
                CelebrationEvent::Dismiss
            }
        }
        _ => CelebrationEvent::Wait,
    }
}

fn handle_celebration_event(app: &mut App, keymap: &Keymap, event: Event) -> bool {
    match celebration_event(keymap, event) {
        CelebrationEvent::Dismiss => {
            app.dismiss_celebration();
            false
        }
        CelebrationEvent::Quit => true,
        CelebrationEvent::Wait => {
            app.advance_celebration();
            false
        }
    }
}

pub(crate) fn run(snapshot: Snapshot, store: &Store, keymap: &Keymap) -> Result<()> {
    let mut session = Session::new(snapshot, std::time::Instant::now());
    let mut terminal = TerminalGuard::init().wrap_err("could not initialize terminal")?;
    let mut dirty = true;
    let mut animation_deadline = std::time::Instant::now() + CELEBRATION_FRAME_INTERVAL;
    loop {
        if dirty {
            terminal
                .terminal
                .draw(|frame| ui::render(frame, &session.app, keymap))
                .wrap_err("could not draw terminal")?;
            dirty = false;
        }
        let mut deadline = session.next_deadline();
        if session.app.celebration().is_some() {
            deadline = deadline.min(animation_deadline);
        }
        let timeout = deadline.saturating_duration_since(std::time::Instant::now());
        if event::poll(timeout).wrap_err("could not poll terminal event")? {
            let event = event::read().wrap_err("could not read terminal event")?;
            if matches!(event, Event::Resize(..)) {
                dirty = true;
            } else if session.app.celebration().is_some() {
                if celebration_event(keymap, event.clone()) != CelebrationEvent::Wait {
                    if handle_celebration_event(&mut session.app, keymap, event) {
                        break;
                    }
                    dirty = true;
                }
            } else if let Some(action) = action_for_event(keymap, session.app.mode(), event) {
                let (quit, changed) = session.action(store, action)?;
                if session.app.celebration().is_some() {
                    animation_deadline = std::time::Instant::now() + CELEBRATION_FRAME_INTERVAL;
                }
                if quit {
                    break;
                }
                dirty |= changed;
            }
        }
        let now = std::time::Instant::now();
        dirty |= session.tick(store, now);
        if session.app.celebration().is_some() {
            if now >= animation_deadline {
                session.app.advance_celebration();
                animation_deadline = now + CELEBRATION_FRAME_INTERVAL;
                dirty = true;
            }
        } else {
            animation_deadline = now + CELEBRATION_FRAME_INTERVAL;
        }
    }
    terminal.restore().wrap_err("could not restore terminal")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    };

    use super::*;
    use crate::app::Mode;

    #[test]
    fn trash_restore_should_persist_and_leave_latest_restore_available_after_restarting() {
        use crate::{
            storage::ScopePresence,
            task::{ListScope, TaskList},
        };
        let home = tempfile::tempdir().unwrap();
        let mut tasks = TaskList::new(ListScope::Global);
        let older = tasks.add("completed older").unwrap();
        let newest = tasks.add("newest").unwrap();
        tasks.toggle_complete(older).unwrap();
        tasks.delete(older).unwrap();
        tasks.delete(newest).unwrap();
        let store = Store::open(home.path(), ListScope::Global).unwrap();
        store.save(&tasks).unwrap();
        let keymap = Keymap::defaults();
        let mut session = Session::new(
            store.read_snapshot(ScopePresence::RequireExisting).unwrap(),
            std::time::Instant::now(),
        );
        let before = std::fs::read(&store.paths().data_file).unwrap();
        for key in ['t', 'j', 'r'] {
            let action = action_for_event(
                &keymap,
                session.app.mode(),
                Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
            )
            .unwrap();
            assert_eq!(session.action(&store, action).unwrap(), (false, true));
            if key != 'r' {
                assert_eq!(std::fs::read(&store.paths().data_file).unwrap(), before);
            }
        }
        assert!(!store.paths().temp_file.exists());
        let mut session = Session::new(
            store.read_snapshot(ScopePresence::RequireExisting).unwrap(),
            std::time::Instant::now(),
        );
        let restored = session.app.tasks().task(older).unwrap();
        assert_eq!(restored.text(), "completed older");
        assert!(restored.completed());
        assert!(!restored.is_deleted());
        let action = action_for_event(
            &keymap,
            session.app.mode(),
            Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE)),
        )
        .unwrap();
        session.action(&store, action).unwrap();
        assert_eq!(session.app.selected(), Some(newest));
        let loaded = store.load().unwrap();
        assert_eq!(
            loaded
                .visible_tasks()
                .map(|task| task.id())
                .collect::<Vec<_>>(),
            vec![older, newest]
        );
        assert!(loaded.deleted_tasks().is_empty());
    }

    #[test]
    fn action_for_event_should_ignore_resize_and_key_release() {
        let keymap = Keymap::defaults();
        assert_eq!(
            action_for_event(&keymap, Mode::Normal, Event::Resize(80, 24)),
            None
        );
        assert_eq!(
            action_for_event(
                &keymap,
                Mode::Normal,
                Event::Key(KeyEvent::new_with_kind(
                    KeyCode::Char('q'),
                    KeyModifiers::NONE,
                    KeyEventKind::Release,
                )),
            ),
            None
        );
        assert_eq!(
            action_for_event(
                &keymap,
                Mode::Normal,
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: 0,
                    row: 0,
                    modifiers: KeyModifiers::NONE,
                }),
            ),
            None
        );
    }

    #[test]
    fn celebration_event_should_dismiss_for_an_ordinary_key() {
        let keymap = Keymap::defaults();
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(celebration_event(&keymap, event), CelebrationEvent::Dismiss);
    }

    #[test]
    fn celebration_event_should_quit_for_normal_quit_keys() {
        let keymap = Keymap::defaults();
        for key in [
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            assert_eq!(
                celebration_event(&keymap, Event::Key(key)),
                CelebrationEvent::Quit
            );
        }
    }

    #[test]
    fn handling_an_ordinary_celebration_key_should_dismiss_the_animation() {
        let keymap = Keymap::defaults();
        let mut list = crate::task::TaskList::new(crate::task::ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        handle_celebration_event(&mut app, &keymap, event);

        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn handling_a_non_key_event_should_advance_the_animation() {
        let keymap = Keymap::defaults();
        let mut list = crate::task::TaskList::new(crate::task::ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();

        handle_celebration_event(&mut app, &keymap, Event::Resize(100, 30));

        assert_eq!(app.celebration().unwrap().frame(), 1);
    }
}
