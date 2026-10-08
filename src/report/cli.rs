//! The `report` subcommand and the page written on quit, as the binaries
//! that write a report use them.

use super::{Outcome, Written};
use crate::config::ReportConfig;
use crate::human_bytes;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The `report` subcommand's flags, flattened into an application's own
/// `clap` subcommand.
#[derive(clap::Args, Debug, Default)]
pub struct ReportArgs {
    /// Directory to write the report into. Defaults to the config file's
    /// [report] dir, which this makes optional.
    #[arg(long)]
    pub dir: Option<PathBuf>,
}

/// Where the `report` subcommand writes: `--dir`, else a scratch run's own
/// directory, else the config's `[report] dir`.
///
/// An unset `[report]` section means "not on every quit", which is a
/// different question from the one the subcommand asks, so it is an error
/// naming the config file rather than a page silently not written.
pub fn dir(
    args: &ReportArgs,
    scratch_dir: Option<&Path>,
    cfg: Option<&ReportConfig>,
    config_path: &Path,
) -> Result<PathBuf> {
    if let Some(dir) = args.dir.as_deref().or(scratch_dir) {
        return Ok(dir.to_path_buf());
    }
    cfg.with_context(|| {
        format!(
            "no --dir given, and no [report] section naming one in {}",
            config_path.display()
        )
    })?
    .dir()
}

/// One line for a page that reached the disk.
pub fn describe(written: &Written) -> String {
    format!(
        "wrote {} to {}",
        human_bytes(written.bytes),
        written.path.display()
    )
}

/// The page written on quit. Never fatal: the session's work is already
/// saved, and the next quit writes the page again. Silent unless a page was
/// written, because it runs after every quit.
pub fn after_quit(result: Result<Outcome>) {
    match result {
        Ok(Outcome::Written(written)) => println!("{}", describe(&written)),
        Ok(_) => {}
        Err(e) => eprintln!("report failed: {e:#}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(dir: Option<&str>) -> ReportArgs {
        ReportArgs {
            dir: dir.map(PathBuf::from),
        }
    }

    #[test]
    fn the_flag_comes_before_the_scratch_directory_and_the_config() {
        let cfg = ReportConfig::new("/configured");
        let got = dir(
            &args(Some("/flag")),
            Some(Path::new("/scratch")),
            Some(&cfg),
            Path::new("/c.toml"),
        )
        .unwrap();
        assert_eq!(got, PathBuf::from("/flag"));
    }

    #[test]
    fn a_scratch_run_writes_beside_its_copy_rather_than_into_the_configured_directory() {
        let cfg = ReportConfig::new("/configured");
        let got = dir(
            &args(None),
            Some(Path::new("/scratch")),
            Some(&cfg),
            Path::new("/c.toml"),
        )
        .unwrap();
        assert_eq!(got, PathBuf::from("/scratch"));
    }

    #[test]
    fn with_neither_the_configured_directory_is_used() {
        let cfg = ReportConfig::new("/configured");
        let got = dir(&args(None), None, Some(&cfg), Path::new("/c.toml")).unwrap();
        assert_eq!(got, PathBuf::from("/configured"));
    }

    #[test]
    fn with_no_directory_anywhere_the_error_names_the_config_file() {
        let err = dir(&args(None), None, None, Path::new("/c.toml")).unwrap_err();
        assert!(err.to_string().contains("in /c.toml"), "{err}");
    }

    #[test]
    fn a_written_page_is_described_by_its_size_and_path() {
        let written = Written {
            path: PathBuf::from("/out/Report.html"),
            bytes: 2048,
        };
        assert_eq!(describe(&written), "wrote 2 KiB to /out/Report.html");
    }
}
