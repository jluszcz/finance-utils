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

/// `index` stepped by `step` through a list of `len` choices, wrapping. An
/// empty list has no index to step to, so it stays at zero rather than
/// dividing by zero.
pub fn step_index(index: usize, len: usize, step: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + step).rem_euclid(len as isize) as usize
}

/// `focus` stepped by `step` around `order`, wrapping: a form's tab order
/// over its own field enum. A `focus` not in `order` counts from the first.
/// Panics on an empty `order`, since a form always has a field.
pub fn next_in<T: Copy + PartialEq>(order: &[T], focus: T, step: isize) -> T {
    let i = order.iter().position(|f| *f == focus).unwrap_or(0);
    order[step_index(i, order.len(), step)]
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

    #[test]
    fn stepping_an_index_wraps_at_both_ends() {
        assert_eq!(step_index(2, 3, 1), 0);
        assert_eq!(step_index(0, 3, -1), 2);
    }

    #[test]
    fn an_empty_list_steps_to_zero_rather_than_dividing_by_it() {
        assert_eq!(step_index(0, 0, 1), 0);
    }

    #[test]
    fn stepping_around_an_order_wraps_and_counts_from_the_first_when_focus_is_not_in_it() {
        let order = ['a', 'b', 'c'];
        assert_eq!(next_in(&order, 'c', 1), 'a');
        assert_eq!(next_in(&order, 'a', -1), 'c');
        assert_eq!(next_in(&order, 'z', 1), 'b');
    }
}
