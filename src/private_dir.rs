//! A directory only its owner can read, for a cleartext copy of a database.

use std::path::Path;

// The copy is the whole database in cleartext, in a directory under `/tmp` on
// Linux. The leaf is created non-recursively with mode 0700, and that is the
// guard: a recursive create returns `Ok` for a directory someone else made
// first, keeps its mode, and follows a symlink planted there. A directory
// already at this path (a killed run that had the same pid) is removed first;
// anything that survives the removal is left for the `create` to fail on.
#[cfg(unix)]
pub(crate) fn create(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::fs::DirBuilder::new().mode(0o700).create(dir)
}

#[cfg(not(unix))]
pub(crate) fn create(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn base(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "finance_utils_private_dir_{label}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_private_directory_is_readable_only_by_its_owner() {
        let dir = base("mode").join("leaf");
        create(&dir).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// The path is predictable, so another local account can create it first.
    /// A recursive create would accept that directory and its mode; this fails
    /// if anyone reaches for `create_dir_all` here.
    #[test]
    fn a_private_directory_that_already_exists_does_not_keep_its_permissions() {
        let dir = base("squatted").join("leaf");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        std::fs::write(dir.join("planted"), b"planted").unwrap();

        create(&dir).unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
        assert!(!dir.join("planted").exists());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
