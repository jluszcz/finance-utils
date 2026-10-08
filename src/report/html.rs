//! The pieces every report page is drawn with: the document around it, and a
//! tab bar switched by CSS alone, since the page carries no script.

use super::escape;

/// One tab. `id` is its radio's id and also names its panel, `<id>-panel`.
/// An id must not start with a digit, or the `#<id>` selectors
/// [`tab_rules`] writes match nothing.
pub struct Tab {
    /// The radio's id.
    pub id: String,
    /// What the bar shows. Escaped on the way out.
    pub label: String,
}

impl Tab {
    /// A tab with radio id `id`, shown as `label`.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Tab {
        Tab {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// The tabs' radios, with `open` checked. They go ahead of the nav and every
/// panel in the body, because `:checked ~` only looks forward.
pub fn tab_inputs(tabs: &[Tab], open: Option<usize>) -> String {
    tabs.iter()
        .enumerate()
        .map(|(i, tab)| {
            let checked = if Some(i) == open { " checked" } else { "" };
            format!(
                "<input class=\"tab\" type=\"radio\" name=\"tab\" id=\"{}\"{checked}>",
                tab.id
            )
        })
        .collect()
}

/// The bar: one label per radio.
pub fn tab_nav(tabs: &[Tab]) -> String {
    let labels: String = tabs
        .iter()
        .map(|tab| format!("<label for=\"{}\">{}</label>", tab.id, escape(&tab.label)))
        .collect();
    format!("<nav>{labels}</nav>")
}

/// Which panel shows, which label is lit, and where the focus ring goes: one
/// rule set per tab, generated from the same list as the markup.
pub fn tab_rules(tabs: &[Tab]) -> String {
    tabs.iter()
        .map(|tab| {
            let id = &tab.id;
            format!(
                "#{id}:checked~nav label[for={id}]\
                 {{color:inherit;border-bottom-color:currentColor}}\
                 #{id}:focus-visible~nav label[for={id}]\
                 {{outline:2px solid currentColor;outline-offset:-2px}}\
                 #{id}:checked~#{id}-panel{{display:block}}"
            )
        })
        .collect()
}

/// The whole page: `style` inline in the head, then `body`, then a footer
/// stamp saying when it was written. `title` and `stamp` are escaped.
/// `style` and `body` are the caller's own markup.
pub fn page(title: &str, style: &str, body: &str, stamp: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\"><head>\
         <meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{}</title><style>{style}</style></head><body>\
         {body}\
         <footer><p class=\"stamp\">{}</p></footer>\
         </body></html>",
        escape(title),
        escape(stamp),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Vec<Tab> {
        vec![Tab::new("a", "First"), Tab::new("b", "Second")]
    }

    #[test]
    fn the_open_tab_is_the_only_checked_radio() {
        let inputs = tab_inputs(&tabs(), Some(1));
        assert_eq!(inputs.matches(" checked").count(), 1);
        assert!(inputs.contains("id=\"b\" checked"), "{inputs}");
    }

    #[test]
    fn with_no_open_tab_no_radio_is_checked() {
        assert!(!tab_inputs(&tabs(), None).contains("checked"));
    }

    #[test]
    fn every_tab_gets_a_rule_showing_its_own_panel() {
        let rules = tab_rules(&tabs());
        assert!(
            rules.contains("#a:checked~#a-panel{display:block}"),
            "{rules}"
        );
        assert!(
            rules.contains("#b:checked~#b-panel{display:block}"),
            "{rules}"
        );
    }

    #[test]
    fn a_label_carrying_markup_is_escaped() {
        let nav = tab_nav(&[Tab::new("a", "<b>&")]);
        assert!(nav.contains("&lt;b&gt;&amp;"), "{nav}");
    }

    #[test]
    fn the_page_makes_no_external_request_and_carries_no_script() {
        let page = page("Report", "p{margin:0}", "<p>figures</p>", "Written today");
        assert!(!page.contains("http"), "the page reaches out of itself");
        assert!(!page.contains("<script"), "the page carries script");
    }

    #[test]
    fn the_title_and_the_stamp_are_escaped_but_the_body_is_not() {
        let page = page("A&B", "", "<p>x</p>", "<now>");
        assert!(page.contains("<title>A&amp;B</title>"), "{page}");
        assert!(
            page.contains("<p class=\"stamp\">&lt;now&gt;</p>"),
            "{page}"
        );
        assert!(page.contains("<p>x</p>"), "{page}");
    }
}
