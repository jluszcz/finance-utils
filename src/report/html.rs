//! The pieces every report page is drawn with: the document around it, and a
//! tab bar whose panels a script switches.

use super::escape;

/// The shell's own stylesheet: the base tokens, the page frame and both
/// tab-bar forms, with no table metrics, which are each application's.
const SHELL_CSS: &str = include_str!("shell.css");

/// The shell's own script: switching panels on a tab click, and opening and
/// closing the narrow page's tab menu. Plain ES2020.
const SHELL_JS: &str = include_str!("shell.js");

/// One tab of a script-capable page.
pub struct NavTab {
    /// Names the tab's `data-t` and its panel, `<id>-panel`.
    pub id: String,
    /// What the bar and the menu show. Escaped on the way out.
    pub label: String,
    /// The inside of a 24x24 `<svg>`, stroked in the text color. The caller's
    /// own markup. A bar draws the icon form only when every tab has one.
    pub icon: Option<&'static str>,
}

impl NavTab {
    /// A tab with `id`, shown as `label`.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> NavTab {
        NavTab {
            id: id.into(),
            label: label.into(),
            icon: None,
        }
    }

    /// The icon, as the inside of its `<svg>`.
    pub fn icon(mut self, svg_inner: &'static str) -> NavTab {
        self.icon = Some(svg_inner);
        self
    }
}

/// Everything [`app_page`] draws a page from.
pub struct App<'a> {
    /// The document title. Escaped.
    pub title: &'a str,
    /// A favicon glyph, drawn by [`icon_link`].
    pub favicon: Option<&'a str>,
    /// The application's own CSS, appended after the shell's.
    pub style: &'a str,
    /// The tabs, in bar order.
    pub tabs: &'a [NavTab],
    /// The index of the tab shown first.
    pub open: usize,
    /// One panel body per tab, in order: static HTML, or empty for a script
    /// to fill.
    pub panels: &'a [String],
    /// JSON text for the script to read from `#d`. Must already have passed
    /// through [`script_json`].
    pub data: Option<&'a str>,
    /// The application's script, run after the shell's.
    pub script: Option<&'a str>,
    /// When the page was written. Escaped.
    pub stamp: &'a str,
}

/// `json` made safe to sit in a `<script>`: every `<` becomes `\u003c`, which
/// JSON reads back as the same character, so no `</script>` or `<!--` inside a
/// string can end or confuse the element.
pub fn script_json(json: &str) -> String {
    json.replace('<', "\\u003c")
}

/// The menu toggle's icon: three bars, stroked like a tab's.
const MENU_ICON: &str = "<path d=\"M4 7h16M4 12h16M4 17h16\"/>";

/// A page whose tabs a script switches. The bar is `nav.tabs.icons` when
/// every tab has an icon and `nav.tabs.wrap` otherwise; the panels follow it,
/// every one but `open` hidden. The icon bar leads with a menu toggle naming
/// the open tab, which the narrow page shows in place of the row. The data
/// block precedes both scripts so they find it at load.
pub fn app_page(app: &App) -> String {
    let icons = !app.tabs.is_empty() && app.tabs.iter().all(|t| t.icon.is_some());
    let buttons: String = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let svg = tab
                .icon
                .map(|inner| format!("<svg viewBox=\"0 0 24 24\">{inner}</svg>"))
                .unwrap_or_default();
            format!(
                "<button role=\"tab\" data-t=\"{}\" aria-selected=\"{}\">{svg}\
                 <span class=\"l\">{}</span></button>",
                tab.id,
                i == app.open,
                escape(&tab.label)
            )
        })
        .collect();
    let nav = if icons {
        let open = app
            .tabs
            .get(app.open)
            .map_or_else(String::new, |t| escape(&t.label));
        format!(
            "<nav class=\"tabs icons\">\
             <button class=\"menu\" aria-expanded=\"false\">\
             <svg viewBox=\"0 0 24 24\">{MENU_ICON}</svg><span class=\"cur\">{open}</span></button>\
             <div class=\"list\" role=\"tablist\">{buttons}</div></nav>"
        )
    } else {
        format!("<nav class=\"tabs wrap\" role=\"tablist\">{buttons}</nav>")
    };
    let panels: String = app
        .tabs
        .iter()
        .zip(app.panels)
        .enumerate()
        .map(|(i, (tab, body))| {
            let hidden = if i == app.open { "" } else { " hidden" };
            format!(
                "<section id=\"{}-panel\" role=\"tabpanel\"{hidden}>{body}</section>",
                tab.id
            )
        })
        .collect();
    let data = app
        .data
        .map(|d| format!("<script type=\"application/json\" id=\"d\">{d}</script>"))
        .unwrap_or_default();
    let script = app
        .script
        .map(|s| format!("<script>{s}</script>"))
        .unwrap_or_default();
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\"><head>\
         <meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\
         <title>{}</title>{}<style>{SHELL_CSS}{}</style></head><body>\
         {nav}<main>{panels}</main>\
         <footer><p class=\"stamp\">{}</p></footer>\
         {data}<script>{SHELL_JS}</script>{script}\
         </body></html>",
        escape(app.title),
        app.favicon.map(icon_link).unwrap_or_default(),
        app.style,
        escape(app.stamp),
    )
}

/// `glyph` as a favicon, inlined so the page still fetches nothing: an SVG
/// that draws the glyph as text, so an emoji is drawn in the reader's own
/// emoji font. Base64 rather than percent-encoded, because an SVG used as an
/// image must declare its namespace, and that URI spelled out in the page
/// would read as the external reference every caller asserts the page has
/// none of.
fn icon_link(glyph: &str) -> String {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\">\
         <text x=\"50\" y=\".9em\" font-size=\"90\" text-anchor=\"middle\">{}</text></svg>",
        escape(glyph)
    );
    format!(
        "<link rel=\"icon\" href=\"data:image/svg+xml;base64,{}\">",
        base64(svg.as_bytes())
    )
}

/// Standard, padded base64. A favicon is the one thing here that needs it,
/// which is too little to spend a dependency on.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_every_remainder_a_chunk_can_leave() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"\xff\xfe\xfd\xfc"), "//79/A==");
    }

    #[test]
    fn an_icon_is_linked_inline_and_no_icon_links_nothing() {
        let tabs = vec![NavTab::new("a", "A")];
        let panels = vec![String::new()];
        let with = app_page(&app(&tabs, &panels, 0));
        assert!(
            with.contains("<link rel=\"icon\" href=\"data:image/svg+xml;base64,"),
            "{with}"
        );
        let without = app_page(&App {
            favicon: None,
            ..app(&tabs, &panels, 0)
        });
        assert!(!without.contains("<link"));
    }

    #[test]
    fn the_title_and_the_stamp_are_escaped() {
        let tabs = vec![NavTab::new("a", "A")];
        let panels = vec!["<p>x</p>".to_string()];
        let page = app_page(&App {
            title: "A&B",
            stamp: "<now>",
            ..app(&tabs, &panels, 0)
        });
        assert!(page.contains("<title>A&amp;B</title>"), "{page}");
        assert!(
            page.contains("<p class=\"stamp\">&lt;now&gt;</p>"),
            "{page}"
        );
        assert!(page.contains("<p>x</p>"), "{page}");
    }

    fn nav_tabs(icons: bool) -> Vec<NavTab> {
        let tab = |id: &str, label: &str| {
            let t = NavTab::new(id, label);
            if icons {
                t.icon("<path d=\"M0 0\"/>")
            } else {
                t
            }
        };
        vec![tab("a", "First"), tab("b", "Second"), tab("c", "Third")]
    }

    fn app<'a>(tabs: &'a [NavTab], panels: &'a [String], open: usize) -> App<'a> {
        App {
            title: "Report",
            favicon: Some("\u{1F4B0}"),
            style: "p{margin:0}",
            tabs,
            open,
            panels,
            data: Some("{\"k\":1}"),
            script: Some("window.appRan=1"),
            stamp: "Written today",
        }
    }

    fn panels() -> Vec<String> {
        vec!["<p>one</p>".into(), "<p>two</p>".into(), String::new()]
    }

    #[test]
    fn script_json_escapes_every_angle_bracket() {
        let out = script_json(r#"{"a":"</script><!--"}"#);
        assert!(!out.contains('<'), "{out}");
        assert!(out.contains("\\u003c/script>"), "{out}");
    }

    #[test]
    fn app_page_hides_every_panel_but_the_open_one() {
        let tabs = nav_tabs(true);
        let page = app_page(&app(&tabs, &panels(), 1));
        let hidden = page
            .split("<section")
            .skip(1)
            .filter(|s| s.split('>').next().unwrap().contains(" hidden"))
            .count();
        assert_eq!(hidden, 2, "{page}");
        let open = page.split("id=\"b-panel\"").nth(1).unwrap();
        assert!(
            !open.split('>').next().unwrap().contains("hidden"),
            "{page}"
        );
    }

    #[test]
    fn a_tab_list_with_icons_draws_the_icon_bar_and_one_without_draws_the_wrapping_row() {
        let (with, without) = (nav_tabs(true), nav_tabs(false));
        let p = panels();
        assert!(app_page(&app(&with, &p, 0)).contains("class=\"tabs icons\""));
        assert!(app_page(&app(&without, &p, 0)).contains("class=\"tabs wrap\" role=\"tablist\""));
        let mixed = vec![
            NavTab::new("a", "A").icon("<path/>"),
            NavTab::new("b", "B"),
            NavTab::new("c", "C"),
        ];
        assert!(app_page(&app(&mixed, &p, 0)).contains("class=\"tabs wrap\""));
    }

    #[test]
    fn the_icon_bar_leads_with_a_menu_naming_the_open_tab_and_the_wrapping_row_has_none() {
        let (with, without) = (nav_tabs(true), nav_tabs(false));
        let p = panels();
        let page = app_page(&app(&with, &p, 1));
        let menu = page.find("class=\"menu\"").unwrap();
        let list = page.find("<div class=\"list\" role=\"tablist\">").unwrap();
        assert!(menu < list, "{page}");
        assert!(
            page[menu..list].contains("<span class=\"cur\">Second</span>"),
            "{page}"
        );
        assert!(!app_page(&app(&without, &p, 1)).contains("class=\"menu\""));
    }

    #[test]
    fn app_page_embeds_data_and_scripts_in_order() {
        let tabs = nav_tabs(true);
        let page = app_page(&app(&tabs, &panels(), 0));
        let data = page
            .find("<script type=\"application/json\" id=\"d\">")
            .unwrap();
        let shell = page.find(SHELL_JS.trim()).unwrap();
        let app_js = page.find("window.appRan=1").unwrap();
        assert!(data < shell && shell < app_js, "{page}");
    }

    #[test]
    fn app_page_makes_no_external_request() {
        let tabs = nav_tabs(true);
        let page = app_page(&app(&tabs, &panels(), 0));
        assert!(!page.contains("http"), "the page reaches out of itself");
    }

    #[test]
    fn the_shell_sets_no_table_metrics() {
        assert!(!SHELL_CSS.contains("table"));
    }
}
