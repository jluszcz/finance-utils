//! The pieces of a configuration file every application reads, and the XDG
//! paths the file and its sibling state live at.
//!
//! An absent file, or one missing a section, means that section's feature is
//! off: a clean checkout and an unconfigured machine both do nothing. A file
//! that is present but does not parse is an error instead. `ReportConfig`'s
//! `dir` and `BackupConfig`'s `bucket` have no default, so the typo that would
//! otherwise switch a feature off silently (`directory =`, `bucketname =`) is
//! a missing field. Keys nothing reads are ignored, so a file written for
//! another build still configures every key this one understands.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// `$<var>` when it is set and non-empty, else `$HOME/<fallback>`. `var_os`
/// rather than `var`, so a home that is not UTF-8 still works.
fn xdg_dir(var: &str, fallback: &[&str]) -> Result<PathBuf> {
    match std::env::var_os(var) {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir)),
        _ => {
            let home: OsString = std::env::var_os("HOME").context("HOME is not set")?;
            Ok(fallback.iter().fold(PathBuf::from(home), |p, c| p.join(c)))
        }
    }
}

/// `$XDG_CONFIG_HOME/<app>/config.toml`, or `~/.config` when it is unset or
/// empty.
pub fn default_path(app: &str) -> Result<PathBuf> {
    Ok(xdg_dir("XDG_CONFIG_HOME", &[".config"])?
        .join(app)
        .join("config.toml"))
}

/// `$XDG_STATE_HOME/<app>/<file>`, or `~/.local/state` when it is unset or
/// empty. State is written by the program and means nothing on another
/// machine, which is why it is not kept beside the config file.
pub fn state_path(app: &str, file: &str) -> Result<PathBuf> {
    Ok(xdg_dir("XDG_STATE_HOME", &[".local", "state"])?
        .join(app)
        .join(file))
}

/// `~/.local/share/<app>/<file>`: where an application keeps its database.
///
/// Fixed under `$HOME` rather than following `$XDG_DATA_HOME`, so a variable
/// set for other programs cannot move the database out from under the
/// backups and scratch copies that expect it here.
pub fn data_path(app: &str, file: &str) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join(app)
        .join(file))
}

/// The file at `path`, or `T::default()` when there is none. A file that does
/// not parse is an error naming the path.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// `[report]`: where the HTML report is written on quit.
#[derive(Debug, Deserialize, PartialEq)]
pub struct ReportConfig {
    dir: String,
}

impl ReportConfig {
    /// A section naming `dir`, as the file would.
    pub fn new(dir: impl Into<String>) -> ReportConfig {
        ReportConfig { dir: dir.into() }
    }

    /// The directory, with a leading `~` or `~/` expanded against `$HOME`.
    /// TOML does not expand it, and a `~` anywhere else is an ordinary
    /// character in a directory name. A relative path is an error: it would
    /// resolve against whichever directory the application was started in.
    pub fn dir(&self) -> Result<PathBuf> {
        let home = || std::env::var_os("HOME").context("HOME is not set");
        let dir = if self.dir == "~" {
            PathBuf::from(home()?)
        } else if let Some(rest) = self.dir.strip_prefix("~/") {
            PathBuf::from(home()?).join(rest)
        } else {
            PathBuf::from(&self.dir)
        };
        anyhow::ensure!(
            dir.is_absolute(),
            "[report] dir {:?} is not an absolute path",
            self.dir
        );
        Ok(dir)
    }
}

/// `[backup]`: where the database is copied, and how often.
#[derive(Debug, Deserialize, PartialEq)]
pub struct BackupConfig {
    /// The bucket. No default: it names where the owner's finances are kept,
    /// so it cannot be a literal in a public repository.
    pub bucket: String,
    /// The `~/.aws/credentials` profile to authenticate as. Unset means the
    /// application's own name; see [`BackupConfig::profile_or`].
    #[serde(default)]
    pub profile: Option<String>,
    /// Days between uploads. Zero uploads on every run, which is what setting
    /// this up wants.
    #[serde(default = "default_interval_days")]
    pub interval_days: u32,
}

fn default_interval_days() -> u32 {
    7
}

impl BackupConfig {
    /// The named profile, or `default` when the file names none.
    pub fn profile_or<'a>(&'a self, default: &'a str) -> &'a str {
        self.profile.as_deref().unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The shape an application's own `Config` takes.
    #[derive(Debug, Default, Deserialize, PartialEq)]
    struct Config {
        report: Option<ReportConfig>,
        backup: Option<BackupConfig>,
    }

    /// Writes `body` to a temp file named for the test, since the tests run in
    /// one process at once and a shared name would have them reading each
    /// other's files.
    fn fixture(label: &str, body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "finance_utils_config_{label}_{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, body).unwrap();
        path
    }

    fn load_config(path: &Path) -> Result<Config> {
        load(path)
    }

    #[test]
    fn a_report_section_naming_only_a_dir_is_a_complete_configuration() {
        let path = fixture("report", "[report]\ndir = \"/tmp/reports\"\n");
        let report = load_config(&path).unwrap().report.unwrap();
        assert_eq!(report.dir().unwrap(), PathBuf::from("/tmp/reports"));
    }

    #[test]
    fn a_misspelled_dir_is_an_error_rather_than_a_silently_disabled_report() {
        let path = fixture("typo", "[report]\ndirectory = \"/tmp/reports\"\n");
        assert!(load_config(&path).is_err());
    }

    #[test]
    fn a_config_file_with_no_report_section_leaves_reports_off() {
        let path = fixture("no_report", "[other]\nkey = 1\n");
        assert_eq!(load_config(&path).unwrap(), Config::default());
    }

    #[test]
    fn an_absent_config_file_leaves_reports_off() {
        let path = std::env::temp_dir().join("finance_utils_config_absent_nowhere.toml");
        assert_eq!(load_config(&path).unwrap(), Config::default());
    }

    #[test]
    fn a_config_file_that_does_not_parse_is_an_error_naming_its_path() {
        let path = fixture("broken", "[report\n");
        let err = load_config(&path).unwrap_err();
        assert!(
            format!("{err:#}").contains(&path.display().to_string()),
            "{err:#}"
        );
    }

    #[test]
    fn a_leading_tilde_in_the_report_dir_expands_against_home() {
        let path = fixture("tilde", "[report]\ndir = \"~/reports\"\n");
        let report = load_config(&path).unwrap().report.unwrap();
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(report.dir().unwrap(), home.join("reports"));
    }

    #[test]
    fn a_tilde_in_the_middle_of_the_report_dir_is_left_alone() {
        let path = fixture("mid_tilde", "[report]\ndir = \"/tmp/a~b\"\n");
        let report = load_config(&path).unwrap().report.unwrap();
        assert_eq!(report.dir().unwrap(), PathBuf::from("/tmp/a~b"));
    }

    #[test]
    fn a_bare_tilde_as_the_report_dir_is_home() {
        let path = fixture("bare_tilde", "[report]\ndir = \"~\"\n");
        let report = load_config(&path).unwrap().report.unwrap();
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(report.dir().unwrap(), home);
    }

    #[test]
    fn a_relative_report_dir_is_an_error_rather_than_a_path_under_wherever_the_application_started()
    {
        let path = fixture("relative", "[report]\ndir = \"Dropbox/pay\"\n");
        let report = load_config(&path).unwrap().report.unwrap();
        let err = report.dir().unwrap_err();
        assert!(format!("{err:#}").contains("Dropbox/pay"), "{err:#}");
    }

    #[test]
    fn a_fully_specified_backup_section_parses() {
        let path = fixture(
            "backup_full",
            "[backup]\nbucket = \"a-bucket\"\nprofile = \"a-profile\"\ninterval_days = 3\n",
        );
        assert_eq!(
            load_config(&path).unwrap().backup,
            Some(BackupConfig {
                bucket: "a-bucket".to_string(),
                profile: Some("a-profile".to_string()),
                interval_days: 3,
            })
        );
    }

    #[test]
    fn a_backup_section_naming_only_a_bucket_takes_every_default() {
        let path = fixture("backup_minimal", "[backup]\nbucket = \"a-bucket\"\n");
        let backup = load_config(&path).unwrap().backup.unwrap();
        assert_eq!(backup.profile, None);
        assert_eq!(backup.profile_or("an-app"), "an-app");
        assert_eq!(backup.interval_days, 7);
    }

    #[test]
    fn a_misspelled_bucket_is_an_error_rather_than_silently_disabled_backups() {
        let path = fixture("backup_typo", "[backup]\nbucketname = \"a-bucket\"\n");
        assert!(load_config(&path).is_err());
    }

    /// There is no key prefix: a backup sits at the root of a bucket that
    /// holds nothing else, so a `prefix` line is a key nothing reads.
    #[test]
    fn a_prefix_in_the_backup_section_is_ignored() {
        let path = fixture(
            "backup_prefix",
            "[backup]\nbucket = \"a-bucket\"\nprefix = \"a-prefix\"\n",
        );
        assert_eq!(
            load_config(&path).unwrap().backup.unwrap().bucket,
            "a-bucket"
        );
    }

    #[test]
    fn a_config_file_with_only_a_report_section_leaves_backups_off() {
        let path = fixture("report_only", "[report]\ndir = \"/tmp/reports\"\n");
        assert_eq!(load_config(&path).unwrap().backup, None);
    }

    #[test]
    fn a_negative_backup_interval_is_an_error_naming_the_config_file() {
        let path = fixture(
            "backup_negative",
            "[backup]\nbucket = \"a-bucket\"\ninterval_days = -1\n",
        );
        let err = load_config(&path).unwrap_err();
        assert!(
            format!("{err:#}").contains(&path.display().to_string()),
            "{err:#}"
        );
    }

    /// A file written for another build may name a section this one has no
    /// field for.
    #[test]
    fn an_unknown_section_does_not_stop_the_rest_of_the_file_loading() {
        let path = fixture(
            "section",
            "[charts]\nstyle = \"wide\"\n\n[backup]\nbucket = \"a-bucket\"\n",
        );
        assert_eq!(
            load_config(&path).unwrap().backup.unwrap().bucket,
            "a-bucket"
        );
    }

    #[test]
    fn a_named_profile_is_used_over_the_default() {
        let backup = BackupConfig {
            bucket: "a-bucket".to_string(),
            profile: Some("a-profile".to_string()),
            interval_days: 7,
        };
        assert_eq!(backup.profile_or("an-app"), "a-profile");
    }

    #[test]
    fn the_config_path_is_the_apps_own_directory_under_the_config_home() {
        let path = default_path("an-app").unwrap();
        assert!(path.ends_with("an-app/config.toml"), "{}", path.display());
    }

    #[test]
    fn a_data_path_is_the_named_file_in_the_apps_own_directory_under_local_share() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(
            data_path("an-app", "an-app.db").unwrap(),
            home.join(".local/share/an-app/an-app.db")
        );
    }

    #[test]
    fn a_state_path_is_the_named_file_in_the_apps_own_directory_under_the_state_home() {
        let path = state_path("an-app", "backup.toml").unwrap();
        assert!(path.ends_with("an-app/backup.toml"), "{}", path.display());
    }
}
