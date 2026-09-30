//! Helpers for an application's TUI tests. Enable `test-support` from `[dev-dependencies]` only.

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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::widgets::Paragraph;

    #[test]
    fn a_drawn_buffer_reads_back_as_trimmed_lines() {
        let text = draw(4, 2, |f| f.render_widget(Paragraph::new("hi"), f.area()));
        assert_eq!(text, "hi\n");
    }
}
