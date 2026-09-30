//! Writing an HTML page to a synced directory: minified, atomically, and only when it is due.

use crate::config::ReportConfig;
use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate};
use minify_html::Cfg;
use std::path::{Path, PathBuf};

/// The page as it goes to the disk. Callers render readable HTML, which their
/// own tests assert against, and this is the one place between it and the disk.
/// `minify_css` because the whole layout is one inline `<style>`; no JS
/// minifier because the page carries no script.
pub fn minify(page: &str) -> Vec<u8> {
    let cfg = Cfg {
        minify_css: true,
        ..Cfg::new()
    };
    minify_html::minify(page.as_bytes(), &cfg)
}

/// Escapes `&`, `<`, `>` and `"`. Safe for text content and for
/// double-quoted attribute values; not for single-quoted or unquoted
/// attributes, since it does not escape `'`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// A page that reached the disk: where it landed, and how big it is.
#[derive(Debug)]
pub struct Written {
    /// The page's final path.
    pub path: PathBuf,
    /// The size of the minified page, in bytes.
    pub bytes: u64,
}

/// What [`write_if_enabled`] did.
#[derive(Debug)]
pub enum Outcome {
    /// No `[report]` section.
    Disabled,
    /// The caller's own reason to leave the page alone: a scratch or demo run.
    Skipped,
    /// Today's page is already there, and this run wrote no rows.
    Unchanged,
    /// The page was rendered and written.
    Written(Written),
}

/// Whether the page is owed a rewrite: this run changed something, or the
/// page on disk was not written today.
///
/// The directory is usually a synced one, so a rename that produces the same
/// bytes still costs an upload and a download on the phone.
///
/// Both halves are approximations. `wrote_rows` sees only this run's
/// connection. `last_written` is the page's mtime, which is the day it was
/// *written*, not the day it quotes, so a `--today` run or a session held
/// across midnight can leave a stale page standing until the next run that
/// writes a row.
pub fn is_due(last_written: Option<NaiveDate>, today: NaiveDate, wrote_rows: bool) -> bool {
    wrote_rows || last_written != Some(today)
}

/// The local day `path` was last written, or `None` for every reason it
/// cannot be read. All of those mean the page is due.
fn written_on(path: &Path) -> Option<NaiveDate> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some(DateTime::<Local>::from(modified).date_naive())
}

/// The two steps that can fail with a temporary file on disk, so `write` has
/// one error path to clean up after.
fn write_then_rename(temp: &Path, path: &Path, page: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file =
        std::fs::File::create(temp).with_context(|| format!("creating {}", temp.display()))?;
    // The rename replaces the file rather than rewriting it, so a page the
    // owner narrowed to themselves would otherwise reopen at the umask default.
    // Narrowed before the figures go in, so they are never readable at it.
    if let Some(existing) = std::fs::metadata(path).ok().filter(|m| m.is_file()) {
        file.set_permissions(existing.permissions())
            .with_context(|| format!("setting permissions on {}", temp.display()))?;
    }
    file.write_all(page)
        .with_context(|| format!("writing {}", temp.display()))?;
    drop(file);
    std::fs::rename(temp, path).with_context(|| format!("renaming onto {}", path.display()))
}

/// Write `page` into `dir` as `file_name`, whatever any config says.
///
/// A temporary file beside the target, renamed onto it: same directory, so
/// the rename stays atomic rather than crossing a filesystem, and a sync
/// client never sees the page half-written. The pid is in the temporary name
/// because an explicit report command can overlap an open app's quit in the
/// same directory.
pub fn write(dir: &Path, file_name: &str, page: &str) -> Result<Written> {
    let page = minify(page);
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let temp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let path = dir.join(file_name);
    write_then_rename(&temp, &path, &page).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })?;
    Ok(Written {
        path,
        bytes: page.len() as u64,
    })
}

/// Write the page on quit, if the config asks for one and it is due.
///
/// `skip` is the caller's own rule for a run whose page must not replace the
/// real one. `render` runs only once a page is going to be written.
pub fn write_if_enabled(
    cfg: Option<&ReportConfig>,
    skip: bool,
    file_name: &str,
    today: NaiveDate,
    wrote_rows: bool,
    render: impl FnOnce() -> Result<String>,
) -> Result<Outcome> {
    let Some(report) = cfg else {
        return Ok(Outcome::Disabled);
    };
    if skip {
        return Ok(Outcome::Skipped);
    }
    let dir = report.dir()?;
    if !is_due(written_on(&dir.join(file_name)), today, wrote_rows) {
        return Ok(Outcome::Unchanged);
    }
    Ok(Outcome::Written(write(&dir, file_name, &render()?)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    const NAME: &str = "Report.html";
    const PAGE: &str = "<!DOCTYPE html>\n<html>\n  <head><style>\n    p { color : red ; }\n  </style></head>\n  <body><p>An invented page.</p></body>\n</html>\n";

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "finance_utils_report_{label}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn names(dir: &Path) -> Vec<std::ffi::OsString> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn a_page_is_due_unless_it_was_written_today_by_a_run_that_changed_nothing() {
        let today = day(2026, 1, 16);
        assert!(!is_due(Some(today), today, false));
        assert!(is_due(Some(today), today, true));
        assert!(is_due(Some(day(2026, 1, 15)), today, false));
        assert!(is_due(None, today, false));
    }

    #[test]
    fn writing_lands_the_page_under_its_name_and_leaves_no_temporary_file() {
        let root = scratch("lands");
        let dir = root.join("not").join("yet");
        let written = write(&dir, NAME, PAGE).unwrap();
        assert_eq!(written.path, dir.join(NAME));
        assert_eq!(
            written.bytes,
            std::fs::metadata(&written.path).unwrap().len()
        );
        assert_eq!(names(&dir), [NAME]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_page_reaches_the_disk_smaller_than_it_was_written() {
        let dir = scratch("smaller");
        let written = write(&dir, NAME, PAGE).unwrap();
        assert!(written.bytes < PAGE.len() as u64);
        let text = std::fs::read_to_string(&written.path).unwrap();
        assert!(
            text.to_ascii_lowercase().starts_with("<!doctype html>"),
            "{text}"
        );
        assert!(text.contains("An invented page."), "{text}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A non-empty directory squatting on the name: the rename fails, and the
    /// temporary file must not be left behind in a synced folder.
    #[test]
    fn a_page_that_cannot_be_renamed_into_place_leaves_no_temporary_file() {
        let dir = scratch("blocked");
        std::fs::create_dir_all(dir.join(NAME).join("occupied")).unwrap();
        assert!(write(&dir, NAME, PAGE).is_err());
        assert_eq!(names(&dir), [NAME]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rewriting_the_page_keeps_the_permissions_it_was_given() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("perms");
        let written = write(&dir, NAME, PAGE).unwrap();
        std::fs::set_permissions(&written.path, std::fs::Permissions::from_mode(0o600)).unwrap();
        write(&dir, NAME, PAGE).unwrap();
        let mode = std::fs::metadata(&written.path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_report_section_writes_nothing_and_renders_nothing() {
        let outcome = write_if_enabled(None, false, NAME, day(2026, 1, 2), true, || {
            panic!("rendered with reports off")
        })
        .unwrap();
        assert!(matches!(outcome, Outcome::Disabled), "{outcome:?}");
    }

    #[test]
    fn a_skipped_run_leaves_the_configured_directory_alone() {
        let dir = scratch("skipped");
        let cfg = ReportConfig::new(dir.display().to_string());
        let outcome = write_if_enabled(Some(&cfg), true, NAME, day(2026, 1, 2), true, || {
            panic!("rendered a skipped page")
        })
        .unwrap();
        assert!(matches!(outcome, Outcome::Skipped), "{outcome:?}");
        assert!(!dir.exists());
    }

    /// `today` is the real local date because the gate reads the page's mtime,
    /// which is the real day it was written.
    #[test]
    fn a_run_that_changed_nothing_leaves_todays_page_alone() {
        let dir = scratch("unchanged");
        let cfg = ReportConfig::new(dir.display().to_string());
        let today = Local::now().date_naive();
        write(&dir, NAME, PAGE).unwrap();
        let outcome = write_if_enabled(Some(&cfg), false, NAME, today, false, || {
            panic!("rendered an unchanged page")
        })
        .unwrap();
        assert!(matches!(outcome, Outcome::Unchanged), "{outcome:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_run_that_wrote_rows_rewrites_the_page() {
        let dir = scratch("rewrite");
        let cfg = ReportConfig::new(dir.display().to_string());
        let today = Local::now().date_naive();
        write(&dir, NAME, PAGE).unwrap();
        let outcome =
            write_if_enabled(
                Some(&cfg),
                false,
                NAME,
                today,
                true,
                || Ok(PAGE.to_string()),
            )
            .unwrap();
        assert!(matches!(outcome, Outcome::Written(_)), "{outcome:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn escaping_covers_the_characters_that_break_out_of_text_and_double_quoted_attributes() {
        assert_eq!(
            escape(r#"a & <b> "c" 'd'"#),
            "a &amp; &lt;b&gt; &quot;c&quot; 'd'"
        );
    }
}
