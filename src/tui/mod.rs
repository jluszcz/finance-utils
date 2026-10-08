//! Pieces of a ratatui front end every application draws the same way.

pub mod app;
pub mod date;
pub mod help;
pub mod status;
#[cfg(feature = "test-support")]
pub mod testing;
pub mod text;

use ratatui::crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

/// Windows reports releases too; acting on both would run every key twice.
pub fn is_press(key: &KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
}

/// A `width` × `height` rectangle centered in `area`, shrunk to fit it.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_is_centered_in_the_area() {
        let r = centered(Rect::new(0, 0, 100, 40), 20, 10);
        assert_eq!(r, Rect::new(40, 15, 20, 10));
    }

    #[test]
    fn a_rectangle_larger_than_the_area_shrinks_to_fit_it() {
        let r = centered(Rect::new(5, 5, 10, 4), 20, 10);
        assert_eq!(r, Rect::new(5, 5, 10, 4));
    }
}
