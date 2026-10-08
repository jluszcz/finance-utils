//! Helpers for an application's TUI tests. Enable `test-support` from `[dev-dependencies]` only.

use super::app::App;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, Terminal};

/// A key press with no modifiers.
pub fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// A key press with `Shift` held.
pub fn shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

/// A `Ctrl`+letter key press.
pub fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

/// Render into a `width` × `height` test terminal and return what it drew.
pub fn draw_buffer(width: u16, height: u16, render: impl FnOnce(&mut Frame)) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(render).unwrap();
    terminal.backend().buffer().clone()
}

/// Render into a `width` × `height` test terminal and return it as text; see
/// [`buffer_text`].
pub fn draw(width: u16, height: u16, render: impl FnOnce(&mut Frame)) -> String {
    buffer_text(&draw_buffer(width, height, render))
}

/// The buffer as text, one line per row with trailing spaces trimmed.
pub fn buffer_text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            let line: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Send `app` one unmodified key.
pub fn press<A: App>(app: &mut A, code: KeyCode) {
    app.on_key(key(code));
}

/// Send `app` each character of `text` as its own key.
pub fn type_text<A: App>(app: &mut A, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// `app`'s whole screen at `width` × `height`, as [`draw`] returns it.
pub fn screen<A: App>(app: &mut A, width: u16, height: u16) -> String {
    draw(width, height, |frame| app.render(frame))
}

/// The rows inside a bordered screen: the border's sides removed and
/// trailing spaces trimmed. The top border (which carries the title) and the
/// last two rows (the bottom border and the footer under it) are dropped.
///
/// # Panics
///
/// On text of fewer than three rows: it expects a bordered screen.
pub fn inside(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    lines[1..lines.len().saturating_sub(2)]
        .iter()
        .map(|l| {
            let l = l.strip_prefix('│').unwrap_or(l);
            let l = l.strip_suffix('│').unwrap_or(l);
            l.trim_end().to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::widgets::Paragraph;

    #[test]
    fn a_drawn_buffer_reads_back_as_trimmed_lines() {
        let text = draw(4, 2, |f| f.render_widget(Paragraph::new("hi"), f.area()));
        assert_eq!(text, "hi\n");
    }

    #[derive(Default)]
    struct Echo {
        typed: String,
    }

    impl App for Echo {
        fn should_quit(&self) -> bool {
            false
        }
        fn expire_status(&mut self) -> bool {
            false
        }
        fn render(&mut self, frame: &mut Frame) {
            frame.render_widget(
                Paragraph::new(self.typed.clone()).block(ratatui::widgets::Block::bordered()),
                frame.area(),
            );
        }
        fn on_key(&mut self, key: KeyEvent) {
            if let KeyCode::Char(c) = key.code {
                self.typed.push(c);
            }
        }
    }

    #[test]
    fn typed_text_reaches_the_app_one_key_at_a_time_and_draws() {
        let mut app = Echo::default();
        type_text(&mut app, "ab");
        press(&mut app, KeyCode::Char('c'));
        assert!(screen(&mut app, 8, 3).contains("abc"));
    }

    #[test]
    fn inside_is_the_rows_within_the_border_without_title_or_footer() {
        let text = "┌──┐\n│ab│\n│c │\n└──┘\nfooter";
        assert_eq!(inside(text), vec!["ab".to_string(), "c".to_string()]);
    }
}
