//! Dates as a person types and reads them.

use anyhow::{Result, anyhow};
use chrono::{Datelike, NaiveDate};

/// `M/D` -- a month and a day, taking the next year that month occurs in.
///
/// The year turns on the month alone: `1/2` typed on January 20th is this
/// January, a backdated entry, while `1/2` typed in December is next year's.
pub fn parse_shorthand(raw: &str, today: NaiveDate) -> Result<NaiveDate> {
    let raw = raw.trim();
    let malformed = || anyhow!("not a M/D date: {raw:?}");
    let (month, day) = raw.split_once('/').ok_or_else(malformed)?;
    let month: u32 = month.trim().parse().map_err(|_| malformed())?;
    let day: u32 = day.trim().parse().map_err(|_| malformed())?;
    let year = if month >= today.month() {
        today.year()
    } else {
        today.year() + 1
    };
    NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| anyhow!("no such date: {raw:?}"))
}

/// The date as `YYYY-MM-DD`.
pub fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn today() -> NaiveDate {
        day(2026, 1, 20)
    }

    #[test]
    fn m_d_shorthand_takes_this_year_when_the_month_is_not_behind() {
        assert_eq!(parse_shorthand("1/30", today()).unwrap(), day(2026, 1, 30));
        assert_eq!(parse_shorthand("1/2", today()).unwrap(), day(2026, 1, 2));
        assert_eq!(parse_shorthand("3/1", today()).unwrap(), day(2026, 3, 1));
    }

    #[test]
    fn m_d_typed_in_late_december_for_january_is_next_year() {
        assert_eq!(
            parse_shorthand("1/2", day(2026, 12, 30)).unwrap(),
            day(2027, 1, 2)
        );
    }

    #[test]
    fn shorthand_is_trimmed_and_needs_a_slash() {
        assert_eq!(
            parse_shorthand(" 1/30 ", today()).unwrap(),
            day(2026, 1, 30)
        );
        for bad in ["130", ""] {
            assert!(parse_shorthand(bad, today()).is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn text_that_is_not_a_date_is_refused() {
        for bad in ["13/1", "2/30", "2026/01/16"] {
            assert!(parse_shorthand(bad, today()).is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn a_date_is_written_year_first() {
        assert_eq!(iso(day(2026, 1, 6)), "2026-01-06");
    }
}
