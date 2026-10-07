//! Key tables: one list per screen or form, from which both the footer and
//! the help panel are drawn, so a footer cannot drift from the panel that
//! explains it. Each application keeps its own tables.

use super::centered;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

/// How an entry's key joins the footer, if at all.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Label {
    /// Live and in the panel, but not named in the footer.
    Hidden,
    /// The entry's own footer word: `{key} {word}`.
    Own(&'static str),
    /// A word shared with the entries beside it: their keys join with `/`
    /// under one word. Adjacency is what groups them, so grouping can never
    /// reorder a footer.
    Shared(&'static str),
}

/// One key: how it is printed, how it joins the footer, and the sentence the
/// panel shows for it.
#[derive(Copy, Clone, Debug)]
pub struct Entry {
    /// How the key is printed: `Tab`, `[ ]`, `←/→`.
    pub key: &'static str,
    /// Whether and how the footer names it.
    pub label: Label,
    /// What the key does, in the fewest words that say it.
    pub detail: &'static str,
}

impl Entry {
    /// A key with its own footer word.
    pub const fn own(key: &'static str, word: &'static str, detail: &'static str) -> Entry {
        Entry {
            key,
            label: Label::Own(word),
            detail,
        }
    }

    /// A key sharing its footer word with the entries beside it.
    pub const fn shared(key: &'static str, word: &'static str, detail: &'static str) -> Entry {
        Entry {
            key,
            label: Label::Shared(word),
            detail,
        }
    }

    /// A key the panel explains and the footer leaves out.
    pub const fn hidden(key: &'static str, detail: &'static str) -> Entry {
        Entry {
            key,
            label: Label::Hidden,
            detail,
        }
    }
}

/// One footer item per word, the tables flattened in order. Each
/// application joins them with its own separator.
pub fn footer_items(tables: &[&[Entry]]) -> Vec<String> {
    let entries: Vec<Entry> = tables.iter().flat_map(|t| t.iter().copied()).collect();
    let mut items = Vec::new();
    let mut i = 0;
    while i < entries.len() {
        match entries[i].label {
            Label::Hidden => i += 1,
            Label::Own(word) => {
                items.push(format!("{} {word}", entries[i].key));
                i += 1;
            }
            Label::Shared(word) => {
                let start = i;
                while i < entries.len() && entries[i].label == Label::Shared(word) {
                    i += 1;
                }
                let keys: Vec<&str> = entries[start..i].iter().map(|e| e.key).collect();
                items.push(format!("{} {word}", keys.join("/")));
            }
        }
    }
    items
}

/// The keys `table` names more than once, for an application's test that
/// no table offers one key two meanings.
pub fn duplicate_keys(table: &[Entry]) -> Vec<&'static str> {
    let mut seen = Vec::new();
    let mut twice = Vec::new();
    for e in table {
        if seen.contains(&e.key) {
            if !twice.contains(&e.key) {
                twice.push(e.key);
            }
        } else {
            seen.push(e.key);
        }
    }
    twice
}

/// The help panel: each topic's title in bold, then its keys and details in
/// two aligned columns, centered over `area`.
pub fn render_panel(frame: &mut Frame, area: Rect, topics: &[(&str, &[Entry])]) {
    let key_w = topics
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .map(|e| e.key.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line> = Vec::new();
    for (title, entries) in topics {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::styled(
            title.to_string(),
            Style::new().add_modifier(Modifier::BOLD),
        ));
        for e in *entries {
            lines.push(Line::from(format!("  {:<key_w$}  {}", e.key, e.detail)));
        }
    }
    let width = lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 2;
    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Help ")),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    const TABLE: &[Entry] = &[
        Entry::own("a", "add", "Add one"),
        Entry::hidden("↑/↓", "Select"),
        Entry::shared("e", "row", "Edit the row"),
        Entry::shared("d", "row", "Delete the row"),
        Entry::own("q", "quit", "Quit"),
    ];

    #[test]
    fn own_entries_stand_alone_and_adjacent_shared_ones_join_under_one_word() {
        assert_eq!(footer_items(&[TABLE]), ["a add", "e/d row", "q quit"]);
    }

    #[test]
    fn a_hidden_entry_between_two_shared_ones_splits_the_group() {
        const SPLIT: &[Entry] = &[
            Entry::shared("e", "row", "Edit"),
            Entry::hidden("x", "Hidden"),
            Entry::shared("d", "row", "Delete"),
        ];
        assert_eq!(footer_items(&[SPLIT]), ["e row", "d row"]);
    }

    #[test]
    fn tables_join_in_the_order_given() {
        const GLOBAL: &[Entry] = &[Entry::own("?", "help", "Help")];
        assert_eq!(footer_items(&[GLOBAL, TABLE])[0], "? help");
    }

    #[test]
    fn a_key_named_twice_in_one_table_is_reported() {
        const TWICE: &[Entry] = &[Entry::own("a", "add", "x"), Entry::own("a", "again", "y")];
        assert_eq!(duplicate_keys(TWICE), ["a"]);
        assert!(duplicate_keys(TABLE).is_empty());
    }

    #[test]
    fn the_panel_lists_each_topic_and_every_entry_hidden_ones_included() {
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render_panel(f, area, &[("Rows", TABLE)])
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        for needle in ["Help", "Rows", "Add one", "Select", "Delete the row"] {
            assert!(text.contains(needle), "{needle} missing");
        }
    }
}
