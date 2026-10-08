//! The flags every application's binary takes, flattened into its own `Cli`
//! with `#[command(flatten)]`.

use crate::{config, scratch};
use anyhow::Result;
use chrono::{Local, NaiveDate};
use std::path::{Path, PathBuf};

/// `--db`, `--scratch`, `--today`, and `--config`, global so they may follow
/// a subcommand.
#[derive(clap::Args, Clone, Debug, Default)]
pub struct CommonArgs {
    /// Database file, in place of the application's default one.
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,
    /// Run against a copy of the default database in a fresh temporary
    /// directory, leaving the real one untouched -- for trying a migration
    /// before it reaches the file that matters. The copy is left behind so it
    /// can be inspected afterwards.
    #[arg(long, global = true, conflicts_with = "db")]
    pub scratch: bool,
    /// Treat this date as today. Defaults to the local date.
    #[arg(long, global = true)]
    pub today: Option<NaiveDate>,
    /// Config file, in place of the application's default one.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
}

impl CommonArgs {
    /// The directory a `--scratch` copy sits in, where that run writes what
    /// it would otherwise write beside the real database. `None` for any
    /// other run.
    pub fn scratch_dir(&self, db: &Path) -> Option<PathBuf> {
        db.parent().filter(|_| self.scratch).map(Path::to_path_buf)
    }

    /// Whether this run is on the default database, the only one the backup
    /// schedule belongs to: a `--db` or a `--scratch` copy would otherwise
    /// take the real database's turn.
    pub fn is_default_db(&self) -> bool {
        self.db.is_none() && !self.scratch
    }

    /// Whether this run is pointed at another database or another day, so
    /// that what it writes outside the database is not the real thing's.
    pub fn is_scratch_session(&self) -> bool {
        self.scratch || self.db.is_some() || self.today.is_some()
    }

    /// `--today`, or the local date.
    pub fn today_or_local(&self) -> NaiveDate {
        self.today.unwrap_or_else(|| Local::now().date_naive())
    }

    /// `--config`, or `$XDG_CONFIG_HOME/<app>/config.toml`.
    pub fn config_path(&self, app: &str) -> Result<PathBuf> {
        match &self.config {
            Some(path) => Ok(path.clone()),
            None => config::default_path(app),
        }
    }

    /// The database this run opens: `--db`, a fresh [`scratch::copy`] of
    /// `default()` under `--scratch`, or `default()`. `default` is a closure
    /// because computing it can fail or create the application's data
    /// directory, neither of which an explicit `--db` should pay for. The
    /// caller prints a scratch copy's path, since only it knows where its
    /// output goes.
    pub fn db_path(
        &self,
        app: &str,
        default: impl FnOnce() -> Result<PathBuf>,
        snapshot: impl FnOnce(&Path, &Path) -> Result<()>,
    ) -> Result<PathBuf> {
        match &self.db {
            Some(path) => Ok(path.clone()),
            None if self.scratch => scratch::copy(app, &default()?, snapshot),
            None => default(),
        }
    }
}

/// What `--scratch` says in every application's help.
const SCRATCH_HELP: &str = "Run against a copy of the default database in a fresh temporary \
    directory, leaving the real one untouched -- for trying a migration before it reaches the \
    file that matters. The copy is left behind and its path printed, so it can be inspected \
    afterwards";

/// `cmd` with `--db`, `--config` and `--scratch` saying where this
/// application's own files are. The paths are written as `~/...` rather than
/// resolved, because help is read on machines with other homes and other XDG
/// settings. `writes_report` adds where a scratch run's page goes.
pub fn name_defaults(
    cmd: clap::Command,
    app: &str,
    db_file: &str,
    writes_report: bool,
) -> clap::Command {
    let scratch = if writes_report {
        format!(
            "{SCRATCH_HELP}, and the report is written beside it rather than into the \
             configured directory"
        )
    } else {
        SCRATCH_HELP.to_string()
    };
    cmd.mut_arg("db", |a| {
        a.help(format!(
            "Database file. Defaults to ~/.local/share/{app}/{db_file}"
        ))
    })
    .mut_arg("config", |a| {
        a.help(format!(
            "Config file. Defaults to ~/.config/{app}/config.toml"
        ))
    })
    .mut_arg("scratch", |a| a.help(scratch))
}

/// The process's arguments parsed into `T` with [`name_defaults`] applied:
/// `T::parse()` for a `Cli` that flattens [`CommonArgs`]. An error prints
/// clap's usage message and exits exactly as `T::parse()` would.
pub fn parse<T: clap::Parser>(app: &str, db_file: &str, writes_report: bool) -> T {
    let mut cmd = name_defaults(T::command(), app, db_file, writes_report);
    let matches = cmd.get_matches_mut();
    T::from_arg_matches(&matches).unwrap_or_else(|e| e.format(&mut cmd).exit())
}

#[cfg(test)]
mod tests {
    use super::CommonArgs;
    use clap::Parser;
    use std::path::{Path, PathBuf};

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        common: CommonArgs,
        #[command(subcommand)]
        command: Option<Sub>,
    }

    #[derive(clap::Subcommand)]
    enum Sub {
        Run,
    }

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("app").chain(args.iter().copied()))
    }

    #[test]
    fn no_flags_means_the_default_database_today_and_no_scratch_session() {
        let cli = parse(&[]).unwrap();
        assert!(cli.common.is_default_db());
        assert!(!cli.common.is_scratch_session());
    }

    #[test]
    fn the_flags_are_accepted_after_a_subcommand_too() {
        let cli = parse(&["run", "--db", "/tmp/x.db", "--today", "2026-01-16"]).unwrap();
        assert_eq!(
            cli.common.db.as_deref(),
            Some(std::path::Path::new("/tmp/x.db"))
        );
        assert_eq!(
            cli.common.today_or_local(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 16).unwrap()
        );
        assert!(cli.common.is_scratch_session());
        assert!(!cli.common.is_default_db());
        assert!(matches!(cli.command, Some(Sub::Run)));
    }

    #[test]
    fn a_scratch_run_names_the_directory_its_copy_sits_in() {
        let cli = parse(&["--scratch"]).unwrap();
        assert_eq!(
            cli.common.scratch_dir(Path::new("/tmp/copy/app.db")),
            Some(PathBuf::from("/tmp/copy"))
        );
    }

    #[test]
    fn a_run_that_is_not_scratch_has_no_scratch_directory() {
        let cli = parse(&["--db", "/tmp/given/app.db"]).unwrap();
        assert_eq!(cli.common.scratch_dir(Path::new("/tmp/given/app.db")), None);
    }

    #[test]
    fn scratch_and_db_together_are_refused() {
        assert!(parse(&["--scratch", "--db", "/tmp/x.db"]).is_err());
    }

    #[test]
    fn a_today_that_is_not_a_date_is_refused() {
        assert!(parse(&["--today", "tomorrow"]).is_err());
    }

    #[test]
    fn db_path_is_the_flag_or_else_the_default() {
        let never = |_: &std::path::Path, _: &std::path::Path| -> anyhow::Result<()> {
            panic!("no snapshot without --scratch")
        };
        let given = parse(&["--db", "/tmp/given.db"]).unwrap().common;
        assert_eq!(
            given.db_path("app", || Ok("/d.db".into()), never).unwrap(),
            std::path::PathBuf::from("/tmp/given.db")
        );
        let plain = parse(&[]).unwrap().common;
        assert_eq!(
            plain.db_path("app", || Ok("/d.db".into()), never).unwrap(),
            std::path::PathBuf::from("/d.db")
        );
    }

    #[test]
    fn the_default_database_is_not_computed_when_db_is_given() {
        let given = parse(&["--db", "/tmp/given.db"]).unwrap().common;
        let path = given
            .db_path(
                "app",
                || panic!("no default path under --db"),
                |_, _| panic!("no snapshot under --db"),
            )
            .unwrap();
        assert_eq!(path, std::path::PathBuf::from("/tmp/given.db"));
    }

    #[test]
    fn scratch_copies_the_default_database_and_returns_the_copy() {
        let src = std::env::temp_dir().join(format!("cli_scratch_src_{}.db", std::process::id()));
        std::fs::write(&src, b"db").unwrap();
        let common = parse(&["--scratch"]).unwrap().common;
        let copy = common
            .db_path(
                "cli-test",
                || Ok(src.clone()),
                |from, to| {
                    std::fs::copy(from, to)?;
                    Ok(())
                },
            )
            .unwrap();
        assert_ne!(copy, src);
        assert_eq!(std::fs::read(&copy).unwrap(), b"db");
        let _ = std::fs::remove_dir_all(copy.parent().unwrap());
        let _ = std::fs::remove_file(&src);
    }

    fn help(writes_report: bool) -> String {
        use clap::CommandFactory;
        super::name_defaults(Cli::command(), "app", "app.db", writes_report)
            .render_help()
            .to_string()
    }

    #[test]
    fn the_help_names_the_default_database_and_config_paths() {
        let help = help(false);
        assert!(help.contains("~/.local/share/app/app.db"), "{help}");
        assert!(help.contains("~/.config/app/config.toml"), "{help}");
    }

    #[test]
    fn only_an_application_that_writes_a_report_says_where_a_scratch_runs_page_goes() {
        assert!(help(true).contains("the report is written beside it"));
        let without = help(false);
        assert!(!without.contains("report"), "{without}");
    }
}
