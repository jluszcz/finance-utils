//! The `backup` subcommand and the check after every run, as both
//! applications' binaries use them. Neither opens the database: opening
//! creates a missing file, and a mistyped path would then be uploaded.

use super::state::{self, State};
use super::{Outcome, Spec, next_due, run_if_due};
use crate::config::BackupConfig;
use crate::human_bytes;
use anyhow::Result;
use chrono::Utc;
use std::path::Path;

const TIME: &str = "%Y-%m-%d %H:%M UTC";
const OFF: &str = "backups are off: no [backup] section in the config file";

/// The `backup` subcommand's flags, flattened into an application's own
/// `clap` subcommand.
#[derive(clap::Args, Debug, Default)]
pub struct BackupArgs {
    /// Upload even if the last backup is recent enough.
    #[arg(long)]
    pub force: bool,
    /// Print when the last backup ran and when the next is due, then exit.
    #[arg(long, conflicts_with = "force")]
    pub status: bool,
}

/// The `backup` subcommand. Asked for outright, so a failed upload is an
/// error exit rather than a line on stderr, and it uploads whatever database
/// it was pointed at.
pub fn command(
    spec: &Spec,
    db_path: &Path,
    cfg: Option<&BackupConfig>,
    args: &BackupArgs,
    snapshot: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<()> {
    // The state path needs `HOME` or `XDG_STATE_HOME`, so it is resolved only
    // once there is a backup to report on or run.
    if cfg.is_none() {
        if args.status {
            print!("{}", status(spec, None, None));
        } else {
            println!("{}", describe(&Outcome::Disabled));
        }
        return Ok(());
    }
    let state_path = spec.state_path()?;
    if args.status {
        print!("{}", status(spec, cfg, read_state(&state_path).as_ref()));
        return Ok(());
    }
    let outcome = run_if_due(
        spec,
        db_path,
        cfg,
        &state_path,
        Utc::now(),
        args.force,
        snapshot,
    )?;
    println!("{}", describe(&outcome));
    Ok(())
}

/// The check after every run. Never fatal: the session's work is saved, and a
/// failed upload leaves the schedule due, so the next run tries again. Silent
/// unless it uploaded, because it runs after every quit.
///
/// The caller runs it only on its default database: the state file records
/// when an upload last happened, not what was uploaded, so a scratch copy on
/// the schedule would take the real database's turn.
pub fn scheduled(
    spec: &Spec,
    db_path: &Path,
    cfg: Option<&BackupConfig>,
    snapshot: impl FnOnce(&Path, &Path) -> Result<()>,
) {
    // The state path needs `HOME` or `XDG_STATE_HOME`, so it is resolved only
    // once there is a backup to schedule.
    if cfg.is_none() {
        return;
    }
    let result = spec
        .state_path()
        .and_then(|path| run_if_due(spec, db_path, cfg, &path, Utc::now(), false, snapshot));
    match result {
        Ok(outcome @ Outcome::BackedUp { .. }) => println!("{}", describe(&outcome)),
        Ok(_) => {}
        Err(e) => eprintln!("backup failed: {e:#}"),
    }
}

/// The rules every binary puts around a backup, for one that takes the
/// common flags.
#[cfg(feature = "cli")]
impl crate::cli::CommonArgs {
    /// Refuse `--scratch` with the `backup` subcommand, before the copy is
    /// made: a throwaway copy has nothing worth restoring, and an upload of
    /// one would sit beside the real backups looking like one.
    pub fn refuse_scratch_backup(&self) -> Result<()> {
        anyhow::ensure!(
            !self.scratch,
            "--scratch cannot be backed up: drop the flag to back up the real database"
        );
        Ok(())
    }

    /// [`scheduled`], on the default database only: the state file records
    /// when an upload last happened, not what was uploaded, so a `--db` or
    /// `--scratch` run on the schedule would take the real database's turn.
    pub fn scheduled_backup(
        &self,
        spec: &Spec,
        db_path: &Path,
        cfg: Option<&BackupConfig>,
        snapshot: impl FnOnce(&Path, &Path) -> Result<()>,
    ) {
        if self.is_default_db() {
            scheduled(spec, db_path, cfg, snapshot);
        }
    }
}

/// One line for what a run did.
pub fn describe(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Disabled => OFF.to_string(),
        Outcome::NotDue { next } => format!("not due until {}", next.format(TIME)),
        Outcome::BackedUp { bucket, key, bytes } => {
            format!("backed up {} to s3://{bucket}/{key}", human_bytes(*bytes))
        }
    }
}

/// What `--status` prints, one line each: the configuration with the profile
/// it resolves to, then the last backup and the next one due.
pub fn status(spec: &Spec, cfg: Option<&BackupConfig>, state: Option<&State>) -> String {
    let Some(cfg) = cfg else {
        return format!("{OFF}\n");
    };
    let mut out = format!(
        "bucket {}, profile {}, every {} days\n",
        cfg.bucket,
        cfg.profile_or(spec.app),
        cfg.interval_days
    );
    match state {
        None => out.push_str("never backed up\n"),
        Some(state) => {
            out.push_str(&format!(
                "last {} ({})\n",
                state.last_backup_at.format(TIME),
                state.last_key
            ));
            out.push_str(&format!(
                "next {}\n",
                next_due(state.last_backup_at, cfg.interval_days).format(TIME)
            ));
        }
    }
    out
}

/// An unreadable state file warns rather than failing: `--status` must not be
/// stricter than the scheduled check it reports on.
fn read_state(path: &Path) -> Option<State> {
    state::read(path).unwrap_or_else(|e| {
        eprintln!("ignoring unreadable backup state: {e:#}");
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "cli")]
    #[test]
    fn scratch_is_refused_for_a_backup_and_nothing_else_is() {
        use crate::cli::CommonArgs;
        let scratch = CommonArgs {
            scratch: true,
            ..CommonArgs::default()
        };
        let err = scratch.refuse_scratch_backup().unwrap_err();
        assert!(
            err.to_string().contains("--scratch cannot be backed up"),
            "{err}"
        );
        let db = CommonArgs {
            db: Some("/tmp/other.db".into()),
            ..CommonArgs::default()
        };
        db.refuse_scratch_backup().unwrap();
    }

    #[cfg(feature = "cli")]
    #[test]
    fn a_run_on_another_database_never_takes_the_scheduled_backup() {
        use crate::cli::CommonArgs;
        let cfg = BackupConfig {
            bucket: "a-bucket".into(),
            profile: None,
            interval_days: 7,
        };
        for args in [
            CommonArgs {
                db: Some("/tmp/other.db".into()),
                ..CommonArgs::default()
            },
            CommonArgs {
                scratch: true,
                ..CommonArgs::default()
            },
        ] {
            args.scheduled_backup(
                &Spec {
                    app: "an-app",
                    stem: "an-app",
                },
                Path::new("/tmp/other.db"),
                Some(&cfg),
                |_, _| panic!("a non-default database was snapshotted for the schedule"),
            );
        }
    }
    use chrono::{DateTime, TimeZone};

    const SPEC: Spec = Spec {
        app: "finance-utils-test",
        stem: "ledger",
    };

    fn cfg(profile: Option<&str>) -> BackupConfig {
        BackupConfig {
            bucket: "a-bucket".to_string(),
            profile: profile.map(str::to_string),
            interval_days: 7,
        }
    }

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, 14, 0, 0).unwrap()
    }

    #[test]
    fn an_upload_is_described_by_its_size_and_where_it_landed() {
        let outcome = Outcome::BackedUp {
            bucket: "a-bucket".to_string(),
            key: "ledger-20260820T140000Z.db.zst".to_string(),
            bytes: 2048,
        };
        assert_eq!(
            describe(&outcome),
            "backed up 2 KiB to s3://a-bucket/ledger-20260820T140000Z.db.zst"
        );
    }

    #[test]
    fn a_backup_that_is_not_due_says_when_it_will_be() {
        assert_eq!(
            describe(&Outcome::NotDue { next: at(27) }),
            "not due until 2026-08-27 14:00 UTC"
        );
    }

    #[test]
    fn a_config_with_no_backup_section_is_described_as_off() {
        assert_eq!(
            describe(&Outcome::Disabled),
            "backups are off: no [backup] section in the config file"
        );
        assert_eq!(
            status(&SPEC, None, None),
            "backups are off: no [backup] section in the config file\n"
        );
    }

    #[test]
    fn status_names_the_default_profile_when_the_config_names_none() {
        let text = status(&SPEC, Some(&cfg(None)), None);
        assert_eq!(
            text,
            "bucket a-bucket, profile finance-utils-test, every 7 days\nnever backed up\n"
        );
    }

    #[test]
    fn status_gives_the_last_backup_and_the_next_one_due() {
        let state = State {
            last_backup_at: at(20),
            last_key: "ledger-20260820T140000Z.db.zst".to_string(),
        };
        let text = status(&SPEC, Some(&cfg(Some("a-profile"))), Some(&state));
        assert_eq!(
            text,
            "bucket a-bucket, profile a-profile, every 7 days\n\
             last 2026-08-20 14:00 UTC (ledger-20260820T140000Z.db.zst)\n\
             next 2026-08-27 14:00 UTC\n"
        );
    }

    #[test]
    fn the_subcommand_with_no_backup_section_touches_neither_state_nor_database() {
        for status in [false, true] {
            let args = BackupArgs {
                force: false,
                status,
            };
            command(
                &SPEC,
                Path::new("/nonexistent/ledger.db"),
                None,
                &args,
                |_, _| panic!("the database was read"),
            )
            .unwrap();
        }
    }

    /// Resolving the state path needs `HOME`; with nothing to schedule it
    /// must not be asked for, nor the database read.
    #[test]
    fn a_scheduled_check_with_no_backup_section_does_nothing() {
        scheduled(&SPEC, Path::new("/nonexistent/ledger.db"), None, |_, _| {
            panic!("the database was read")
        });
    }
}
