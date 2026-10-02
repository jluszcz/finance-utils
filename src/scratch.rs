//! A throwaway copy of the database, for a run that must not touch the real
//! one -- trying a schema migration on real data before it reaches the file
//! that matters.

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};

/// Snapshot the database at `src` into a new directory under the system temp
/// dir, and return the copy's path.
///
/// `snapshot` copies the database at the first path to the second, as
/// `backup` takes it, so the crate never depends on `rusqlite`. An
/// application's snapshot opens nothing through its own `open`, so the copy
/// carries the schema version the original has and the run against it is the
/// one that migrates.
///
/// The directory is `<app>-scratch-<timestamp>-<pid>`, created mode 0700
/// because the copy is the whole database in cleartext. It is left behind on
/// success, so the copy can be inspected after the run, and removed on
/// failure.
pub fn copy(
    app: &str,
    src: &Path,
    snapshot: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<PathBuf> {
    copy_into(&std::env::temp_dir(), app, src, Local::now(), snapshot)
}

/// `copy` with the base directory and the clock passed in, so a test never
/// shares a directory with a test beside it.
fn copy_into(
    base: &Path,
    app: &str,
    src: &Path,
    now: DateTime<Local>,
    snapshot: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<PathBuf> {
    // Checked here rather than left to `snapshot`: an application's `open`
    // creates a missing file, and a snapshot that did the same would hand
    // back an empty database as a copy of the real one.
    anyhow::ensure!(src.is_file(), "no database at {} to copy", src.display());
    let file_name = src
        .file_name()
        .with_context(|| format!("{} names no file", src.display()))?;
    let dir = base.join(format!(
        "{app}-scratch-{}-{}",
        now.format("%Y%m%dT%H%M%S"),
        std::process::id()
    ));
    crate::private_dir::create(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let dest = dir.join(file_name);
    if let Err(e) = snapshot(src, &dest) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// A fresh directory named for the test, so tests running at once in one
    /// process never share one.
    fn base(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "finance_utils_scratch_{label}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 8, 20, 14, 3, 5).unwrap()
    }

    fn file_copy(src: &Path, dest: &Path) -> Result<()> {
        std::fs::copy(src, dest)?;
        Ok(())
    }

    #[test]
    fn a_copy_lands_in_a_directory_named_for_the_app_and_keeps_the_file_name() {
        let base = base("named");
        let src = base.join("ledger.db");
        std::fs::write(&src, b"rows").unwrap();

        let dest = copy_into(&base, "app", &src, now(), file_copy).unwrap();

        let dir = dest
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            dir,
            format!("app-scratch-20260820T140305-{}", std::process::id())
        );
        assert_eq!(dest.file_name().unwrap(), "ledger.db");
        assert_eq!(std::fs::read(&dest).unwrap(), b"rows");
        assert_eq!(std::fs::read(&src).unwrap(), b"rows");
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let base = base("mode");
        let src = base.join("ledger.db");
        std::fs::write(&src, b"rows").unwrap();

        let dest = copy_into(&base, "app", &src, now(), file_copy).unwrap();

        let mode = std::fs::metadata(dest.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn a_missing_database_is_an_error_rather_than_an_empty_copy() {
        let base = base("missing");
        let err = copy_into(&base, "app", &base.join("ledger.db"), now(), |_, _| {
            panic!("snapshot ran without a database to copy")
        })
        .unwrap_err();
        assert!(err.to_string().contains("no database at"), "{err:#}");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn a_failed_snapshot_leaves_no_directory_behind() {
        let base = base("failed");
        let src = base.join("ledger.db");
        std::fs::write(&src, b"rows").unwrap();

        copy_into(&base, "app", &src, now(), |_, _| anyhow::bail!("disk full")).unwrap_err();

        let left: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("ledger.db")]);
        let _ = std::fs::remove_dir_all(base);
    }
}
