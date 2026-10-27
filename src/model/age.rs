//! Ages derived from canonical GEDCOM dates.

use std::fmt;

use super::date::{self, DateParts};

/// A derived age, with enough precision to avoid pretending partial dates
/// are exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Age {
    Exact(u32),
    Approximate(u32),
    AtMost(u32),
    AtLeast(u32),
    Range(u32, u32),
}

impl fmt::Display for Age {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(years) => write!(formatter, "{years}"),
            Self::Approximate(years) => write!(formatter, "about {years}"),
            Self::AtMost(years) => write!(formatter, "at most {years}"),
            Self::AtLeast(years) => write!(formatter, "at least {years}"),
            Self::Range(min, max) => write!(formatter, "{min}-{max}"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Expr {
    Exact(DateParts),
    Approx(DateParts),
    Before(DateParts),
    After(DateParts),
    Between(DateParts, DateParts),
    From(DateParts),
    To(DateParts),
}

/// Derives a person's age at an event from birth and event dates.
pub fn age_at(birth: Option<&str>, event: Option<&str>) -> Option<Age> {
    let birth = birth_point(birth?)?;
    let event = parse_expr(&date::canonicalize(event?).ok()?)?;
    let (birth_parts, birth_approx) = birth;
    match event {
        Expr::Exact(event) => {
            let (years, approximate) = point_age(birth_parts, event)?;
            Some(if approximate || birth_approx {
                Age::Approximate(years)
            } else {
                Age::Exact(years)
            })
        }
        Expr::Approx(event) => Some(Age::Approximate(point_age(birth_parts, event)?.0)),
        Expr::Before(event) => Some(Age::AtMost(point_age(birth_parts, event)?.0)),
        Expr::After(event) => Some(Age::AtLeast(point_age(birth_parts, event)?.0)),
        Expr::From(event) => Some(Age::AtLeast(point_age(birth_parts, event)?.0)),
        Expr::To(event) => Some(Age::AtMost(point_age(birth_parts, event)?.0)),
        Expr::Between(start, end) => {
            let (start_age, start_approx) = point_age(birth_parts, start)?;
            let (end_age, end_approx) = point_age(birth_parts, end)?;
            let (min, max) = if start_age <= end_age {
                (start_age, end_age)
            } else {
                (end_age, start_age)
            };
            if min == max {
                Some(if birth_approx || start_approx || end_approx {
                    Age::Approximate(min)
                } else {
                    Age::Exact(min)
                })
            } else {
                Some(Age::Range(min, max))
            }
        }
    }
}

fn birth_point(text: &str) -> Option<(DateParts, bool)> {
    let canonical = date::canonicalize(text).ok()?;
    match parse_expr(&canonical)? {
        Expr::Exact(parts) => Some((parts, false)),
        Expr::Approx(parts) => Some((parts, true)),
        _ => None,
    }
}

fn parse_expr(text: &str) -> Option<Expr> {
    let text = text.trim();
    if text.is_empty() || text.starts_with('(') {
        return None;
    }
    if let Some(rest) = text.strip_prefix("ABT ") {
        return Some(Expr::Approx(date::exact_parts(rest)?));
    }
    if let Some(rest) = text.strip_prefix("CAL ") {
        return Some(Expr::Approx(date::exact_parts(rest)?));
    }
    if let Some(rest) = text.strip_prefix("EST ") {
        return Some(Expr::Approx(date::exact_parts(rest)?));
    }
    if let Some(rest) = text.strip_prefix("BEF ") {
        return Some(Expr::Before(date::exact_parts(rest)?));
    }
    if let Some(rest) = text.strip_prefix("AFT ") {
        return Some(Expr::After(date::exact_parts(rest)?));
    }
    if let Some(rest) = text.strip_prefix("BET ") {
        let (start, end) = rest.split_once(" AND ")?;
        return Some(Expr::Between(
            date::exact_parts(start)?,
            date::exact_parts(end)?,
        ));
    }
    if let Some(rest) = text.strip_prefix("FROM ") {
        if let Some((start, end)) = rest.split_once(" TO ") {
            return Some(Expr::Between(
                date::exact_parts(start)?,
                date::exact_parts(end)?,
            ));
        }
        return Some(Expr::From(date::exact_parts(rest)?));
    }
    if let Some(rest) = text.strip_prefix("TO ") {
        return Some(Expr::To(date::exact_parts(rest)?));
    }
    Some(Expr::Exact(date::exact_parts(text)?))
}

fn point_age(birth: DateParts, event: DateParts) -> Option<(u32, bool)> {
    if event.year < birth.year {
        return None;
    }
    if event.year == birth.year
        && let (Some(birth_month), Some(event_month)) = (birth.month, event.month)
    {
        if event_month < birth_month {
            return None;
        }
        if event_month == birth_month
            && let (Some(birth_day), Some(event_day)) = (birth.day, event.day)
            && event_day < birth_day
        {
            return None;
        }
    }

    let mut years = event.year - birth.year;
    if let (Some(birth_month), Some(event_month)) = (birth.month, event.month) {
        if event_month < birth_month {
            years -= 1;
        } else if event_month == birth_month
            && let (Some(birth_day), Some(event_day)) = (birth.day, event.day)
            && event_day < birth_day
        {
            years -= 1;
        }
    }
    if years < 0 {
        return None;
    }
    let approximate = birth.month.is_none()
        || birth.day.is_none()
        || event.month.is_none()
        || event.day.is_none();
    Some((years as u32, approximate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derived(birth: &str, event: &str) -> Option<String> {
        age_at(Some(birth), Some(event)).map(|age| age.to_string())
    }

    #[test]
    fn exact_full_dates() {
        assert_eq!(derived("12 JAN 1850", "12 JAN 1885").as_deref(), Some("35"));
        assert_eq!(derived("12 JAN 1850", "11 JAN 1885").as_deref(), Some("34"));
        assert_eq!(derived("12 JAN 1850", "13 JAN 1885").as_deref(), Some("35"));
        assert_eq!(derived("29 FEB 2000", "28 FEB 2024").as_deref(), Some("23"));
        assert_eq!(derived("29 FEB 2000", "29 FEB 2024").as_deref(), Some("24"));
    }

    #[test]
    fn partial_dates_are_approximate() {
        assert_eq!(derived("1850", "1885").as_deref(), Some("about 35"));
        assert_eq!(derived("JAN 1850", "MAR 1885").as_deref(), Some("about 35"));
        assert_eq!(derived("1850-01-12", "1885-01-12").as_deref(), Some("35"));
    }

    #[test]
    fn qualifiers_and_ranges() {
        assert_eq!(derived("ABT 1850", "1885").as_deref(), Some("about 35"));
        assert_eq!(derived("1850", "BEF 1885").as_deref(), Some("at most 35"));
        assert_eq!(derived("1850", "AFT 1885").as_deref(), Some("at least 35"));
        assert_eq!(
            derived("1850", "BET 1880 AND 1885").as_deref(),
            Some("30-35")
        );
        assert_eq!(
            derived("1850", "FROM 1880 TO 1885").as_deref(),
            Some("30-35")
        );
    }

    #[test]
    fn rejects_unusable_dates_and_birth_after_event() {
        assert!(age_at(None, Some("1885")).is_none());
        assert!(age_at(Some("1850"), None).is_none());
        assert!(age_at(Some("1885"), Some("1850")).is_none());
        assert!(age_at(Some("JAN 1850"), Some("DEC 1849")).is_none());
        assert!(age_at(Some("(circa 1850)"), Some("1885")).is_none());
        assert!(age_at(Some("44 BC"), Some("20 BC")).is_none());
        assert!(age_at(Some("1699/1700"), Some("1750")).is_none());
        assert!(age_at(Some("not a date"), Some("1885")).is_none());
        assert!(age_at(Some("1850"), Some("not a date")).is_none());
    }

    #[test]
    fn display_labels_are_readable() {
        assert_eq!(Age::Exact(35).to_string(), "35");
        assert_eq!(Age::Approximate(35).to_string(), "about 35");
        assert_eq!(Age::AtMost(35).to_string(), "at most 35");
        assert_eq!(Age::AtLeast(35).to_string(), "at least 35");
        assert_eq!(Age::Range(30, 35).to_string(), "30-35");
    }
}
