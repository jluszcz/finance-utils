//! The terminal's lifetime and the loop that draws and reads keys, around
//! an application's own [`App`].

use super::is_press;
use anyhow::Result;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event, KeyEvent};
use ratatui::{Frame, Terminal};
use std::time::Duration;

/// How long the loop waits for an event before checking again. Nothing
/// animates, so this decides one thing: how closely a status message's
/// expiry follows its deadline.
const TICK: Duration = Duration::from_millis(250);

/// What the loop asks of an application.
pub trait App {
    /// Whether the loop should stop before its next frame.
    fn should_quit(&self) -> bool;

    /// Drop a status message whose time is up, and say whether one went: an
    /// expiry is the one change with no event behind it, so the loop redraws
    /// on this answer.
    fn expire_status(&mut self) -> bool;

    /// Draw the whole screen.
    fn render(&mut self, frame: &mut Frame);

    /// Answer a key press. Releases never arrive here.
    fn on_key(&mut self, key: KeyEvent);

    /// Run work a key deferred until the status line announcing it had been
    /// drawn, and say whether there was any. The loop calls it after every
    /// frame, so blocking work is announced before the screen stops
    /// answering.
    fn run_deferred(&mut self) -> bool {
        false
    }
}

/// Run `app` until it quits, then hand it back, so the caller can take its
/// database for the work that happens after a quit.
///
/// `try_init` enables raw mode, enters the alternate screen, and installs a
/// panic hook that restores the terminal before unwinding. The terminal is
/// restored on an error too, before the error is returned.
pub fn run<A: App>(mut app: A) -> Result<A> {
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, &mut app, || {
        Ok(if event::poll(TICK)? {
            Some(event::read()?)
        } else {
            None
        })
    });
    ratatui::try_restore()?;
    result?;
    Ok(app)
}

/// Whether an event leaves the screen owing a redraw: a key press always
/// does, whether or not anything is bound to it, since a press clears the
/// status line; a resize does, since every layout is computed per frame.
fn redraws(event: &Event) -> bool {
    match event {
        Event::Key(key) => is_press(key),
        Event::Resize(..) => true,
        _ => false,
    }
}

/// Draw only when something changed: an event that [`redraws`], a status
/// message running out, or deferred work finishing. `next` waits up to a
/// tick and yields `None` when nothing arrived, so the expiry is still
/// noticed on an idle screen.
fn event_loop<B, A>(
    terminal: &mut Terminal<B>,
    app: &mut A,
    mut next: impl FnMut() -> Result<Option<Event>>,
) -> Result<()>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
    A: App,
{
    let mut dirty = true;
    while !app.should_quit() {
        dirty |= app.expire_status();
        if dirty {
            terminal.draw(|frame| app.render(frame))?;
            dirty = false;
        }
        // After the draw: the work blocks the loop, so what announces it
        // has to be on screen first. Its result is drawn without waiting
        // out a tick.
        if app.run_deferred() {
            dirty = true;
            continue;
        }
        let Some(event) = next()? else {
            continue;
        };
        dirty |= redraws(&event);
        if let Event::Key(key) = event
            && is_press(&key)
        {
            app.on_key(key);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    use std::collections::VecDeque;

    /// Records what the loop did, in order. `q` quits; `d` defers one piece
    /// of work; `expiries` answers `expire_status` once per entry.
    #[derive(Default)]
    struct Script {
        calls: Vec<String>,
        quit: bool,
        deferred: bool,
        expiries: VecDeque<bool>,
    }

    impl App for Script {
        fn should_quit(&self) -> bool {
            self.quit
        }

        fn expire_status(&mut self) -> bool {
            self.expiries.pop_front().unwrap_or(false)
        }

        fn render(&mut self, _frame: &mut Frame) {
            self.calls.push("draw".into());
        }

        fn on_key(&mut self, key: KeyEvent) {
            if let KeyCode::Char(c) = key.code {
                self.calls.push(format!("key {c}"));
                self.quit |= c == 'q';
                self.deferred |= c == 'd';
            }
        }

        fn run_deferred(&mut self) -> bool {
            let ran = std::mem::take(&mut self.deferred);
            if ran {
                self.calls.push("deferred".into());
            }
            ran
        }
    }

    fn press(c: char) -> Option<Event> {
        Some(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )))
    }

    fn release(c: char) -> Option<Event> {
        let mut key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        Some(Event::Key(key))
    }

    /// Run the loop over `events`, then `q`, and return the calls it made.
    fn run_script(mut app: Script, events: Vec<Option<Event>>) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(10, 2)).unwrap();
        let mut events: VecDeque<_> = events.into();
        event_loop(&mut terminal, &mut app, || {
            Ok(events.pop_front().unwrap_or_else(|| press('q')))
        })
        .unwrap();
        app.calls
    }

    /// A key press owes a frame whatever it means, since `on_key` clears the
    /// status line; nothing but a press and a resize changes the screen.
    #[test]
    fn only_a_key_press_and_a_resize_owe_a_frame() {
        use ratatui::crossterm::event::{KeyEventState, MouseEvent, MouseEventKind};

        let key = |kind| {
            Event::Key(KeyEvent {
                code: KeyCode::Char('~'),
                modifiers: KeyModifiers::NONE,
                kind,
                state: KeyEventState::NONE,
            })
        };
        assert!(redraws(&key(KeyEventKind::Press)));
        assert!(redraws(&Event::Resize(80, 24)));

        assert!(!redraws(&key(KeyEventKind::Release)));
        assert!(!redraws(&key(KeyEventKind::Repeat)));
        assert!(!redraws(&Event::FocusGained));
        assert!(!redraws(&Event::FocusLost));
        assert!(!redraws(&Event::Paste("pasted".to_string())));
        assert!(!redraws(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })));
    }

    #[test]
    fn the_first_frame_is_drawn_before_any_event_is_read() {
        assert_eq!(run_script(Script::default(), vec![]), ["draw", "key q"]);
    }

    #[test]
    fn a_tick_with_nothing_in_it_draws_nothing() {
        let calls = run_script(Script::default(), vec![None, None, press('a')]);
        assert_eq!(calls, ["draw", "key a", "draw", "key q"]);
    }

    #[test]
    fn a_key_release_reaches_no_handler_and_draws_nothing() {
        let calls = run_script(Script::default(), vec![release('a'), press('b')]);
        assert_eq!(calls, ["draw", "key b", "draw", "key q"]);
    }

    #[test]
    fn a_resize_redraws() {
        let calls = run_script(Script::default(), vec![Some(Event::Resize(10, 2))]);
        assert_eq!(calls, ["draw", "draw", "key q"]);
    }

    #[test]
    fn an_expired_status_redraws_without_an_event() {
        let app = Script {
            expiries: [false, true].into(),
            ..Script::default()
        };
        let calls = run_script(app, vec![None]);
        assert_eq!(calls, ["draw", "draw", "key q"]);
    }

    #[test]
    fn deferred_work_runs_after_the_frame_announcing_it_and_its_result_is_drawn() {
        let calls = run_script(Script::default(), vec![press('d')]);
        assert_eq!(
            calls,
            ["draw", "key d", "draw", "deferred", "draw", "key q"]
        );
    }
}
