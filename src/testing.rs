//! Helpers for an application's tests that are not about the TUI. Enable
//! `test-support` from `[dev-dependencies]` only.

use chrono::NaiveDate;
use std::process::Command;

/// The date `y`-`m`-`d`. Panics on one that does not exist, which in a test
/// is a typo.
pub fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// `cmd` with an AWS environment that can reach nothing: no config or
/// credentials file, no instance metadata, and none of the variables that
/// would name a region or a profile. For a test that runs a backup command
/// that must fail before anything is uploaded.
pub fn unreachable_aws(cmd: &mut Command) -> &mut Command {
    cmd.env("AWS_CONFIG_FILE", "/dev/null")
        .env("AWS_SHARED_CREDENTIALS_FILE", "/dev/null")
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .env_remove("AWS_REGION")
        .env_remove("AWS_DEFAULT_REGION")
        .env_remove("AWS_PROFILE")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_is_the_calendar_date_it_names() {
        assert_eq!(day(2026, 2, 3).to_string(), "2026-02-03");
    }

    #[test]
    fn an_unreachable_aws_environment_names_no_files_region_or_profile() {
        let mut cmd = Command::new("true");
        unreachable_aws(&mut cmd);
        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        for file in ["AWS_CONFIG_FILE", "AWS_SHARED_CREDENTIALS_FILE"] {
            assert!(
                envs.contains(&(file.into(), Some("/dev/null".into()))),
                "{envs:?}"
            );
        }
        assert!(envs.contains(&("AWS_EC2_METADATA_DISABLED".into(), Some("true".into()))));
        for unset in ["AWS_REGION", "AWS_DEFAULT_REGION", "AWS_PROFILE"] {
            assert!(envs.contains(&(unset.into(), None)), "{envs:?}");
        }
    }
}
