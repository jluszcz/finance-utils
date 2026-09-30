//! Code shared by Paychecker and MisterManager, two local finance
//! applications with the same shape: a ratatui front end over SQLite, an HTML
//! page of the figures written on quit, and a scheduled S3 backup.
//!
//! Every feature is off by default. An application names itself once, as a
//! [`backup::Spec`] and a report file name, and every path, key and profile
//! that differs between the two is derived from that.

#![warn(missing_docs)]

#[cfg(feature = "backup")]
pub mod backup;
#[cfg(feature = "config")]
pub mod config;
#[cfg(feature = "money")]
pub mod money;
#[cfg(feature = "report")]
pub mod report;
// #[cfg(feature = "tui")]
// pub mod tui;

/// A size for a person to read in a one-line message: whole KiB, rounded up
/// so a small file never reads as `0 KiB`, and whole MiB from one up.
///
/// Integer arithmetic on purpose: a size to one decimal place is not worth
/// the first float in either application.
pub fn human_bytes(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{} MiB", bytes / MIB)
    } else {
        format!("{} KiB", bytes.div_ceil(1024))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_under_a_mebibyte_is_whole_kibibytes_rounded_up() {
        assert_eq!(human_bytes(2048), "2 KiB");
        assert_eq!(human_bytes(1025), "2 KiB");
    }

    #[test]
    fn a_size_of_a_mebibyte_or_more_is_whole_mebibytes() {
        assert_eq!(human_bytes(4 * 1024 * 1024), "4 MiB");
    }
}
