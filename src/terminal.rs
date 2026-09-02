use std::time::Duration;

use color_eyre::eyre::{Result, WrapErr};
use crossterm::event::{self, Event, KeyEventKind};

use crate::{
    action::Action,
    app::{App, Mode, Transition},
    input,
    storage::Store,
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

fn action_for_event(mode: Mode, event: Event) -> Option<Action> {
    match event {
        Event::Key(key) => input::map_key(mode, key),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CelebrationEvent {
    Dismiss,
    Quit,
    Wait,
}

fn celebration_event(event: Event) -> CelebrationEvent {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            if input::map_key(Mode::Normal, key) == Some(Action::Quit) {
                CelebrationEvent::Quit
            } else {
                CelebrationEvent::Dismiss
            }
        }
        _ => CelebrationEvent::Wait,
    }
}

fn handle_celebration_event(app: &mut App, event: Event) -> bool {
    match celebration_event(event) {
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

pub(crate) fn run(mut app: App, store: &Store) -> Result<()> {
    let mut terminal = TerminalGuard::init().wrap_err("could not initialize terminal")?;

    loop {
        terminal
            .terminal
            .draw(|frame| ui::render(frame, &app))
            .wrap_err("could not draw terminal")?;

        if app.celebration().is_some() {
            if event::poll(CELEBRATION_FRAME_INTERVAL).wrap_err("could not poll terminal event")? {
                let event = event::read().wrap_err("could not read terminal event")?;
                if handle_celebration_event(&mut app, event) {
                    break;
                }
            } else {
                app.advance_celebration();
            }
            continue;
        }

        let event = event::read().wrap_err("could not read terminal event")?;
        let Some(action) = action_for_event(app.mode(), event) else {
            continue;
        };

        match app.apply(action)? {
            Transition::Persisted => store
                .save(app.tasks())
                .wrap_err("could not save task list")?,
            Transition::Quit => break,
            Transition::Unchanged | Transition::Transient => {}
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
    fn action_for_event_should_ignore_resize_and_key_release() {
        assert_eq!(action_for_event(Mode::Normal, Event::Resize(80, 24)), None);
        assert_eq!(
            action_for_event(
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
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(celebration_event(event), CelebrationEvent::Dismiss);
    }

    #[test]
    fn celebration_event_should_quit_for_normal_quit_keys() {
        for key in [
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            assert_eq!(celebration_event(Event::Key(key)), CelebrationEvent::Quit);
        }
    }

    #[test]
    fn handling_an_ordinary_celebration_key_should_dismiss_the_animation() {
        let mut list = crate::task::TaskList::new(crate::task::ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        handle_celebration_event(&mut app, event);

        assert_eq!(app.celebration(), None);
    }

    #[test]
    fn handling_a_non_key_event_should_advance_the_animation() {
        let mut list = crate::task::TaskList::new(crate::task::ListScope::Global);
        list.add("task").unwrap();
        let mut app = App::new(list);
        app.apply(Action::ToggleComplete).unwrap();

        handle_celebration_event(&mut app, Event::Resize(100, 30));

        assert_eq!(app.celebration().unwrap().frame(), 1);
    }
}
