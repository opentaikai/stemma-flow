//! Calendar arithmetic used by the date calculator and pop-up calendar.

use chrono::{Datelike, Days, Local, Months, NaiveDate, Weekday};

use crate::model::date::{self, DateParts};

/// Unit for adding or subtracting from a date.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShiftUnit {
    #[default]
    Days,
    Months,
    Years,
}

/// Whole-year/month/day difference between two full dates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateDifference {
    pub total_days: u64,
    pub years: u32,
    pub months: u32,
    pub days: u32,
    pub reversed: bool,
}

/// Result of one date calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateCalculation {
    Difference(DateDifference),
    Shifted { date: DateParts, weekday: Weekday },
}

/// Today's local calendar date.
pub fn today() -> DateParts {
    from_naive(Local::now().date_naive())
}

/// Parses a full day/month/year date.
pub fn parse_full(input: &str) -> Result<DateParts, String> {
    let canonical = date::canonicalize(input).map_err(|error| error.to_string())?;
    let parts = date::exact_parts(&canonical)
        .ok_or_else(|| "enter a full day, month and year".to_string())?;
    if parts.month.is_none() || parts.day.is_none() {
        return Err("enter a full day, month and year".to_string());
    }
    to_naive(&parts).ok_or_else(|| "date is out of range".to_string())?;
    Ok(parts)
}

/// Converts full date parts to a chrono date.
pub fn to_naive(parts: &DateParts) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(parts.year, parts.month?, parts.day?)
}

/// Converts a chrono date to date parts.
pub fn from_naive(value: NaiveDate) -> DateParts {
    DateParts {
        year: value.year(),
        month: Some(value.month()),
        day: Some(value.day()),
    }
}

/// Computes the calendar difference between two full dates.
pub fn difference(left: &str, right: &str) -> Result<DateDifference, String> {
    let left = to_naive(&parse_full(left)?).ok_or_else(|| "date is out of range".to_string())?;
    let right = to_naive(&parse_full(right)?).ok_or_else(|| "date is out of range".to_string())?;
    let reversed = right < left;
    let (start, end) = if reversed {
        (right, left)
    } else {
        (left, right)
    };
    let total_days = (end - start).num_days().unsigned_abs();

    let mut years = end.year().saturating_sub(start.year()) as u32;
    let mut anchor = start
        .checked_add_months(Months::new(years.saturating_mul(12)))
        .unwrap_or(start);
    if anchor > end {
        years = years.saturating_sub(1);
        anchor = start
            .checked_add_months(Months::new(years.saturating_mul(12)))
            .unwrap_or(start);
    }
    let mut months = 0;
    while let Some(next) = anchor.checked_add_months(Months::new(months + 1)) {
        if next > end {
            break;
        }
        months += 1;
    }
    anchor = anchor
        .checked_add_months(Months::new(months))
        .unwrap_or(anchor);
    let days = (end - anchor).num_days().unsigned_abs() as u32;

    Ok(DateDifference {
        total_days,
        years,
        months,
        days,
        reversed,
    })
}

/// Adds or subtracts days, months or years from a full date.
pub fn shift(
    base: &str,
    amount: u32,
    unit: ShiftUnit,
    subtract: bool,
) -> Result<DateParts, String> {
    let base = to_naive(&parse_full(base)?).ok_or_else(|| "date is out of range".to_string())?;
    let shifted = match unit {
        ShiftUnit::Days => {
            if subtract {
                base.checked_sub_days(Days::new(amount as u64))
            } else {
                base.checked_add_days(Days::new(amount as u64))
            }
        }
        ShiftUnit::Months => {
            if subtract {
                base.checked_sub_months(Months::new(amount))
            } else {
                base.checked_add_months(Months::new(amount))
            }
        }
        ShiftUnit::Years => {
            let months = amount
                .checked_mul(12)
                .ok_or_else(|| "amount is too large".to_string())?;
            if subtract {
                base.checked_sub_months(Months::new(months))
            } else {
                base.checked_add_months(Months::new(months))
            }
        }
    }
    .ok_or_else(|| "result is outside the supported date range".to_string())?;
    Ok(from_naive(shifted))
}

/// Returns the weekday for full date parts.
pub fn weekday(parts: &DateParts) -> Option<Weekday> {
    to_naive(parts).map(|date| date.weekday())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difference_handles_leap_days_and_month_ends() {
        let result = difference("1 JAN 2000", "1 JAN 2001").expect("difference");
        assert_eq!(result.total_days, 366);
        assert_eq!((result.years, result.months, result.days), (1, 0, 0));
        assert!(!result.reversed);

        let result = difference("31 JAN 2023", "1 MAR 2023").expect("difference");
        assert_eq!(result.total_days, 29);
        assert_eq!((result.years, result.months, result.days), (0, 1, 1));
    }

    #[test]
    fn reversed_differences_are_reported() {
        let result = difference("1 JAN 2001", "1 JAN 2000").expect("difference");
        assert!(result.reversed);
        assert_eq!(result.total_days, 366);
    }

    #[test]
    fn shifts_days_months_and_years() {
        assert_eq!(
            date::format_parts(shift("29 FEB 2024", 1, ShiftUnit::Years, false).unwrap()),
            "28 FEB 2025"
        );
        assert_eq!(
            date::format_parts(shift("31 JAN 2024", 1, ShiftUnit::Months, false).unwrap()),
            "29 FEB 2024"
        );
        assert_eq!(
            date::format_parts(shift("1 JAN 2024", 31, ShiftUnit::Days, true).unwrap()),
            "1 DEC 2023"
        );
    }

    #[test]
    fn rejects_partial_and_invalid_dates() {
        assert!(parse_full("JAN 1850").is_err());
        assert!(parse_full("31 FEB 1850").is_err());
        assert!(difference("", "1 JAN 1850").is_err());
        assert!(shift("1850", 1, ShiftUnit::Days, false).is_err());
    }
}
