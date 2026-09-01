use color_eyre::eyre::{Result, WrapErr};
use crossterm::event::{self, Event};

use crate::{
    action::Action,
    app::{App, Mode, Transition},
    input,
    storage::Store,
    ui,
};

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

pub(crate) fn run(mut app: App, store: &Store) -> Result<()> {
    let mut terminal = TerminalGuard::init().wrap_err("could not initialize terminal")?;

    loop {
        terminal
            .terminal
            .draw(|frame| ui::render(frame, &app))
            .wrap_err("could not draw terminal")?;

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
}
