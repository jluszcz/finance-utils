//! Dates as a person types and reads them.

use super::text::is_bare;
use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, Months, NaiveDate, TimeDelta};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

/// `YYYY-MM-DD`, or the `M/D` shorthand [`parse_shorthand`] reads. A slash
/// means shorthand, so `2026/01/16` is refused rather than guessed at.
pub fn parse(raw: &str, today: NaiveDate) -> Result<NaiveDate> {
    let raw = raw.trim();
    if raw.contains('/') {
        return parse_shorthand(raw, today);
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .with_context(|| format!("not a YYYY-MM-DD or M/D date: {raw:?}"))
}

/// How far one keypress moves a date: a day, a week with `Shift`, or a month
/// on `[`/`]`.
///
/// One value rather than a direction and a magnitude, so a form's answer to
/// the keys is one match on its focus. A selector that has no week or month
/// to move reads [`Step::direction`] and ignores the size, so a modified
/// arrow is never a dead key on it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Step {
    amount: i64,
    unit: Unit,
}

/// A month is not a number of days, so the unit travels with the amount
/// rather than being flattened into days before the month it lands in is
/// known.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Unit {
    Days,
    Months,
}

impl Step {
    /// `→`.
    pub const NEXT: Step = Step::days(1);
    /// `←`.
    pub const PREVIOUS: Step = Step::days(-1);
    /// `Shift`+`→`.
    pub const NEXT_WEEK: Step = Step::days(7);
    /// `Shift`+`←`.
    pub const PREVIOUS_WEEK: Step = Step::days(-7);
    /// `]`.
    pub const NEXT_MONTH: Step = Step::months(1);
    /// `[`.
    pub const PREVIOUS_MONTH: Step = Step::months(-1);

    const fn days(amount: i64) -> Step {
        Step {
            amount,
            unit: Unit::Days,
        }
    }

    const fn months(amount: i64) -> Step {
        Step {
            amount,
            unit: Unit::Months,
        }
    }

    /// The date `from` steps to, or `None` where the calendar runs out. A
    /// month step clamps the day into the month it lands on, as chrono does:
    /// the 31st has nowhere else to go in a thirty-day month.
    pub fn apply(self, from: NaiveDate) -> Option<NaiveDate> {
        match self.unit {
            Unit::Days => from.checked_add_signed(TimeDelta::days(self.amount)),
            Unit::Months => {
                let months = Months::new(u32::try_from(self.amount.unsigned_abs()).ok()?);
                match self.amount {
                    ..0 => from.checked_sub_months(months),
                    _ => from.checked_add_months(months),
                }
            }
        }
    }

    /// Which way, which is all a selector takes.
    pub fn direction(self) -> isize {
        self.amount.signum() as isize
    }

    /// The step a date field's key means, or `None` for any other key.
    /// Read through [`is_bare`]: `Ctrl` means text editing everywhere, and a
    /// modifier nothing binds must not fall through to the bare key.
    pub fn from_key(key: KeyEvent) -> Option<Step> {
        if !is_bare(key) {
            return None;
        }
        let week = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Right if week => Some(Step::NEXT_WEEK),
            KeyCode::Left if week => Some(Step::PREVIOUS_WEEK),
            KeyCode::Right => Some(Step::NEXT),
            KeyCode::Left => Some(Step::PREVIOUS),
            KeyCode::Char(']') => Some(Step::NEXT_MONTH),
            KeyCode::Char('[') => Some(Step::PREVIOUS_MONTH),
            _ => None,
        }
    }
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

    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn a_full_date_or_m_d_shorthand_parses() {
        assert_eq!(parse("2026-03-04", today()).unwrap(), day(2026, 3, 4));
        assert_eq!(parse(" 3/4 ", today()).unwrap(), day(2026, 3, 4));
    }

    #[test]
    fn a_year_first_date_written_with_slashes_is_refused() {
        assert!(parse("2026/01/16", today()).is_err());
    }

    #[test]
    fn a_full_date_that_does_not_exist_is_refused() {
        let err = parse("2026-02-30", today()).unwrap_err();
        assert!(format!("{err:#}").contains("2026-02-30"), "{err:#}");
    }

    #[test]
    fn a_day_and_a_week_step_either_way() {
        assert_eq!(Step::NEXT.apply(day(2026, 1, 31)), Some(day(2026, 2, 1)));
        assert_eq!(
            Step::PREVIOUS.apply(day(2026, 1, 1)),
            Some(day(2025, 12, 31))
        );
        assert_eq!(
            Step::NEXT_WEEK.apply(day(2026, 1, 28)),
            Some(day(2026, 2, 4))
        );
        assert_eq!(
            Step::PREVIOUS_WEEK.apply(day(2026, 1, 4)),
            Some(day(2025, 12, 28))
        );
    }

    #[test]
    fn a_month_step_clamps_the_day_into_a_shorter_month() {
        assert_eq!(
            Step::NEXT_MONTH.apply(day(2026, 1, 31)),
            Some(day(2026, 2, 28))
        );
        assert_eq!(
            Step::NEXT_MONTH.apply(day(2028, 1, 31)),
            Some(day(2028, 2, 29))
        );
        assert_eq!(
            Step::PREVIOUS_MONTH.apply(day(2026, 3, 31)),
            Some(day(2026, 2, 28))
        );
    }

    #[test]
    fn a_step_reports_only_its_direction() {
        assert_eq!(Step::NEXT_MONTH.direction(), 1);
        assert_eq!(Step::PREVIOUS_WEEK.direction(), -1);
    }

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn arrows_step_a_day_shift_arrows_a_week_and_brackets_a_month() {
        let none = KeyModifiers::NONE;
        assert_eq!(
            Step::from_key(press(KeyCode::Right, none)),
            Some(Step::NEXT)
        );
        assert_eq!(
            Step::from_key(press(KeyCode::Left, none)),
            Some(Step::PREVIOUS)
        );
        assert_eq!(
            Step::from_key(press(KeyCode::Right, KeyModifiers::SHIFT)),
            Some(Step::NEXT_WEEK)
        );
        assert_eq!(
            Step::from_key(press(KeyCode::Left, KeyModifiers::SHIFT)),
            Some(Step::PREVIOUS_WEEK)
        );
        assert_eq!(
            Step::from_key(press(KeyCode::Char(']'), none)),
            Some(Step::NEXT_MONTH)
        );
        assert_eq!(
            Step::from_key(press(KeyCode::Char('['), none)),
            Some(Step::PREVIOUS_MONTH)
        );
    }

    #[test]
    fn a_key_with_ctrl_held_or_one_nothing_binds_is_no_step() {
        assert_eq!(
            Step::from_key(press(KeyCode::Right, KeyModifiers::CONTROL)),
            None
        );
        assert_eq!(
            Step::from_key(press(KeyCode::Char('x'), KeyModifiers::NONE)),
            None
        );
    }
}
