//! When the last backup succeeded, kept under `$XDG_STATE_HOME` rather than in
//! or beside the config file: one is edited by a person, the other is written
//! by the program and means nothing on another machine.
//!
//! Written only after a successful upload, so a failed run leaves the
//! schedule due rather than recording an attempt that moved no bytes.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What the last successful upload left behind.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct State {
    /// When the last upload finished, which the schedule counts from.
    pub last_backup_at: DateTime<Utc>,
    /// Read by nothing but a status command, and by a person wondering what
    /// the last upload was.
    pub last_key: String,
}

/// `Ok(None)` means no backup has ever been recorded; an unreadable file is
/// an `Err`.
pub fn read(path: &Path) -> Result<Option<State>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let state = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(state))
}

/// Records `state`, creating the directory it lives in.
pub fn write(path: &Path, state: &State) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let text = toml::to_string(state).context("serializing backup state")?;
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "finance_utils_state_{label}_{}.toml",
            std::process::id()
        ))
    }

    fn a_state() -> State {
        State {
            last_backup_at: Utc.with_ymd_and_hms(2026, 8, 20, 14, 3, 5).unwrap(),
            last_key: "ledger-20260820T140305Z.db.zst".to_string(),
        }
    }

    #[test]
    fn a_written_state_reads_back_identical() {
        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        write(&path, &a_state()).unwrap();
        assert_eq!(read(&path).unwrap(), Some(a_state()));
    }

    /// Nothing else writes to `~/.local/state`, so the first backup on a
    /// machine finds the directory missing.
    #[test]
    fn writing_creates_the_directory_it_needs() {
        let dir =
            std::env::temp_dir().join(format!("finance_utils_state_mkdir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("backup.toml");
        write(&path, &a_state()).unwrap();
        assert_eq!(read(&path).unwrap(), Some(a_state()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_state_file_reads_as_never_backed_up() {
        let path = temp_path("absent");
        let _ = std::fs::remove_file(&path);
        assert_eq!(read(&path).unwrap(), None);
    }

    /// Distinct from a missing file: the caller decides that an unreadable
    /// one is worth a warning, and it cannot if this has already shrugged.
    #[test]
    fn a_state_file_that_does_not_parse_is_an_error() {
        let path = temp_path("garbage");
        std::fs::write(&path, "this is not toml {{{").unwrap();
        assert!(read(&path).is_err());
    }
}
