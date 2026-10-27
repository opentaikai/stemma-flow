//! GEDCOM 5.5.1 date-value parsing, validation and canonicalisation.
//!
//! Accepts exact dates (`12 JAN 1850`, `JAN 1850`, `1850`), ISO convenience
//! input (`1850-01-12`, `1850-01`, `1850`), qualifiers (`ABT`, `CAL`, `EST`,
//! `BEF`, `AFT`), ranges (`BET … AND …`), periods (`FROM … TO …`, `FROM …`,
//! `TO …`) and date phrases (`(free text)`). Output is canonical GEDCOM
//! text with uppercase three-letter months and single spaces.

use std::fmt;

/// Why a date value could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateError(String);

impl DateError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for DateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DateError {}

#[derive(Debug)]
struct Year {
    value: u32,
    bc: bool,
    dual: Option<String>,
}

impl Year {
    fn text(&self) -> String {
        let mut text = self.dual.clone().unwrap_or_else(|| self.value.to_string());
        if self.bc {
            text.push_str(" BC");
        }
        text
    }
}

/// Canonicalises `input` (e.g. `12 jan 1850` → `12 JAN 1850`).
pub fn canonicalize(input: &str) -> Result<String, DateError> {
    let text = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err(DateError::new("date is empty"));
    }
    parse_value(&text)
}

/// The components of a canonical exact date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateParts {
    pub year: i32,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

/// Parses a canonical exact date (`12 JAN 1850`, `JAN 1850`, `1850`).
///
/// Returns `None` for qualifiers, ranges, phrases, BC/dual years and ISO
/// input; callers should canonicalize first when accepting user text.
pub fn exact_parts(input: &str) -> Option<DateParts> {
    let text = input.trim();
    if text.is_empty() || text.starts_with('(') || text.contains(" BC") {
        return None;
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    match tokens.as_slice() {
        [year] => {
            let year = parse_year(year).ok()?;
            if year.bc || year.dual.is_some() {
                return None;
            }
            Some(DateParts {
                year: i32::try_from(year.value).ok()?,
                month: None,
                day: None,
            })
        }
        [month, year] => {
            let (_, month) = parse_month(month).ok()?;
            let year = parse_year(year).ok()?;
            if year.bc || year.dual.is_some() {
                return None;
            }
            Some(DateParts {
                year: i32::try_from(year.value).ok()?,
                month: Some(month),
                day: None,
            })
        }
        [day, month, year] => {
            let day = parse_day(day).ok()?;
            let (_, month) = parse_month(month).ok()?;
            let year = parse_year(year).ok()?;
            if year.bc || year.dual.is_some() {
                return None;
            }
            check_day(day, month, year.value).ok()?;
            Some(DateParts {
                year: i32::try_from(year.value).ok()?,
                month: Some(month),
                day: Some(day),
            })
        }
        _ => None,
    }
}

/// Checks `input` without returning its canonical form.
pub fn validate(input: &str) -> Result<(), DateError> {
    canonicalize(input).map(|_| ())
}

fn parse_value(text: &str) -> Result<String, DateError> {
    if is_phrase(text) {
        return parse_phrase(text);
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let Some(first) = tokens.first() else {
        return Err(DateError::new("date is empty"));
    };
    let keyword = first.to_ascii_uppercase();
    match keyword.as_str() {
        "ABT" | "CAL" | "EST" | "BEF" | "AFT" => {
            let rest = tokens.get(1..).unwrap_or_default().join(" ");
            if rest.is_empty() {
                return Err(DateError::new(format!("{keyword} needs a date")));
            }
            Ok(format!("{keyword} {}", parse_exact(&rest)?))
        }
        "BET" => {
            let body = tokens.get(1..).unwrap_or_default();
            let (left, right) = split_keyword(body, "AND")?;
            Ok(format!(
                "BET {} AND {}",
                parse_exact(&left)?,
                parse_exact(&right)?
            ))
        }
        "FROM" => {
            let body = tokens.get(1..).unwrap_or_default();
            if let Some((left, right)) = try_split_keyword(body, "TO") {
                Ok(format!(
                    "FROM {} TO {}",
                    parse_exact(&left)?,
                    parse_exact(&right)?
                ))
            } else {
                let rest = body.join(" ");
                if rest.is_empty() {
                    return Err(DateError::new("FROM needs a date"));
                }
                Ok(format!("FROM {}", parse_exact(&rest)?))
            }
        }
        "TO" => {
            let rest = tokens.get(1..).unwrap_or_default().join(" ");
            if rest.is_empty() {
                return Err(DateError::new("TO needs a date"));
            }
            Ok(format!("TO {}", parse_exact(&rest)?))
        }
        _ => parse_exact(text),
    }
}

fn parse_exact(text: &str) -> Result<String, DateError> {
    if is_phrase(text) {
        return parse_phrase(text);
    }
    let raw: Vec<&str> = text.split_whitespace().collect();
    if let [token] = raw.as_slice()
        && token.contains('-')
    {
        return parse_iso(token);
    }
    let (tokens, bc) = split_bc(&raw);
    match tokens {
        [] => Err(DateError::new("date is empty")),
        [year] => {
            let mut year = parse_year(year)?;
            year.bc = bc;
            Ok(year.text())
        }
        [month, year] => {
            let (month, _) = parse_month(month)?;
            let mut year = parse_year(year)?;
            year.bc = bc;
            Ok(format!("{month} {}", year.text()))
        }
        [day, month, year] => {
            let day = parse_day(day)?;
            let (month, month_index) = parse_month(month)?;
            let mut year = parse_year(year)?;
            year.bc = bc;
            check_day(day, month_index, year.value)?;
            Ok(format!("{day} {month} {}", year.text()))
        }
        _ => Err(DateError::new("date has too many parts")),
    }
}

fn parse_iso(token: &str) -> Result<String, DateError> {
    let parts: Vec<&str> = token.split('-').collect();
    match parts.as_slice() {
        [year, month] => {
            parse_numeric(year, 4, 4, "year")?;
            let month = parse_month_number(month)?;
            Ok(format!("{} {year}", month_name(month)))
        }
        [year, month, day] => {
            parse_numeric(year, 4, 4, "year")?;
            let month = parse_month_number(month)?;
            let day = parse_day(day)?;
            let value = parse_numeric(year, 4, 4, "year")?;
            check_day(day, month, value)?;
            Ok(format!("{day} {} {year}", month_name(month)))
        }
        _ => Err(DateError::new(format!("invalid date \"{token}\""))),
    }
}

fn is_phrase(text: &str) -> bool {
    text.starts_with('(') && text.ends_with(')')
}

fn parse_phrase(text: &str) -> Result<String, DateError> {
    let inner = text
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_default()
        .trim();
    if inner.is_empty() {
        return Err(DateError::new("date phrase is empty"));
    }
    Ok(format!("({inner})"))
}

fn split_bc<'a>(tokens: &'a [&'a str]) -> (&'a [&'a str], bool) {
    match tokens.split_last() {
        Some((last, rest)) if last.eq_ignore_ascii_case("BC") => (rest, true),
        _ => (tokens, false),
    }
}

fn parse_year(token: &str) -> Result<Year, DateError> {
    if let Some((first, second)) = token.split_once('/') {
        if second.contains('/') {
            return Err(DateError::new(format!("invalid dual year \"{token}\"")));
        }
        let value = parse_numeric(first, 1, 4, "year")?;
        let second_ok =
            (second.len() == 2 || second.len() == 4) && second.chars().all(|c| c.is_ascii_digit());
        if !second_ok {
            return Err(DateError::new(format!("invalid dual year \"{token}\"")));
        }
        return Ok(Year {
            value,
            bc: false,
            dual: Some(format!("{first}/{second}")),
        });
    }
    Ok(Year {
        value: parse_numeric(token, 1, 4, "year")?,
        bc: false,
        dual: None,
    })
}

fn parse_day(token: &str) -> Result<u32, DateError> {
    let day = parse_numeric(token, 1, 2, "day")?;
    if !(1..=31).contains(&day) {
        return Err(DateError::new(format!("day {day} is out of range")));
    }
    Ok(day)
}

fn parse_month(token: &str) -> Result<(&'static str, u32), DateError> {
    let index = match token.to_ascii_uppercase().as_str() {
        "JAN" | "JANUARY" => 1,
        "FEB" | "FEBRUARY" => 2,
        "MAR" | "MARCH" => 3,
        "APR" | "APRIL" => 4,
        "MAY" => 5,
        "JUN" | "JUNE" => 6,
        "JUL" | "JULY" => 7,
        "AUG" | "AUGUST" => 8,
        "SEP" | "SEPTEMBER" => 9,
        "OCT" | "OCTOBER" => 10,
        "NOV" | "NOVEMBER" => 11,
        "DEC" | "DECEMBER" => 12,
        other => return Err(DateError::new(format!("unknown month \"{other}\""))),
    };
    Ok((month_name(index), index))
}

fn parse_month_number(token: &str) -> Result<u32, DateError> {
    let month = parse_numeric(token, 1, 2, "month")?;
    if !(1..=12).contains(&month) {
        return Err(DateError::new(format!("month {month} is out of range")));
    }
    Ok(month)
}

fn month_name(index: u32) -> &'static str {
    match index {
        1 => "JAN",
        2 => "FEB",
        3 => "MAR",
        4 => "APR",
        5 => "MAY",
        6 => "JUN",
        7 => "JUL",
        8 => "AUG",
        9 => "SEP",
        10 => "OCT",
        11 => "NOV",
        _ => "DEC",
    }
}

fn parse_numeric(token: &str, min: usize, max: usize, label: &str) -> Result<u32, DateError> {
    if token.len() < min
        || token.len() > max
        || !token.chars().all(|character| character.is_ascii_digit())
    {
        return Err(DateError::new(format!(
            "{label} must be {min}-{max} digits"
        )));
    }
    let value: u32 = token
        .parse()
        .map_err(|_| DateError::new(format!("invalid {label} \"{token}\"")))?;
    if value == 0 {
        return Err(DateError::new(format!("{label} must be positive")));
    }
    Ok(value)
}

fn check_day(day: u32, month: u32, year: u32) -> Result<(), DateError> {
    if day > days_in_month(month, year) {
        return Err(DateError::new(format!(
            "day {day} is out of range for {} {year}",
            month_name(month)
        )));
    }
    Ok(())
}

fn days_in_month(month: u32, year: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn split_keyword(tokens: &[&str], keyword: &str) -> Result<(String, String), DateError> {
    try_split_keyword(tokens, keyword)
        .ok_or_else(|| DateError::new(format!("expected {keyword} between two dates")))
}

fn try_split_keyword(tokens: &[&str], keyword: &str) -> Option<(String, String)> {
    let index = tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case(keyword))?;
    let left = tokens.get(..index)?.join(" ");
    let right = tokens.get(index + 1..)?.join(" ");
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some((left, right))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical(input: &str) -> String {
        canonicalize(input).expect("date should parse")
    }

    #[test]
    fn exact_dates_are_canonicalised() {
        assert_eq!(canonical("12 jan 1850"), "12 JAN 1850");
        assert_eq!(canonical("12 January 1850"), "12 JAN 1850");
        assert_eq!(canonical("jan 1850"), "JAN 1850");
        assert_eq!(canonical("1850"), "1850");
        assert_eq!(canonical("  12   Jan   1850 "), "12 JAN 1850");
    }

    #[test]
    fn iso_dates_convert_to_gedcom() {
        assert_eq!(canonical("1850-01-12"), "12 JAN 1850");
        assert_eq!(canonical("1850-1-2"), "2 JAN 1850");
        assert_eq!(canonical("1850-01"), "JAN 1850");
        assert_eq!(canonical("1850"), "1850");
    }

    #[test]
    fn qualifiers_ranges_and_periods() {
        assert_eq!(canonical("abt 1850"), "ABT 1850");
        assert_eq!(canonical("BEF 12 JAN 1850"), "BEF 12 JAN 1850");
        assert_eq!(canonical("bet 1850 and 1855"), "BET 1850 AND 1855");
        assert_eq!(canonical("from 1850 to 1855"), "FROM 1850 TO 1855");
        assert_eq!(canonical("FROM JAN 1850"), "FROM JAN 1850");
        assert_eq!(canonical("to DEC 1855"), "TO DEC 1855");
    }

    #[test]
    fn phrases_keep_their_text() {
        assert_eq!(canonical("(circa whenever)"), "(circa whenever)");
        assert_eq!(canonical("ABT (spring 1850)"), "ABT (spring 1850)");
    }

    #[test]
    fn bc_and_dual_years() {
        assert_eq!(canonical("44 bc"), "44 BC");
        assert_eq!(canonical("1699/1700"), "1699/1700");
        assert_eq!(canonical("1699/99"), "1699/99");
    }

    #[test]
    fn leap_years_are_respected() {
        assert_eq!(canonical("29 FEB 2000"), "29 FEB 2000");
        assert_eq!(canonical("29 FEB 2024"), "29 FEB 2024");
        assert!(canonicalize("29 FEB 1900").is_err());
        assert!(canonicalize("29 FEB 2023").is_err());
    }

    #[test]
    fn exact_parts_reads_canonical_dates() {
        assert_eq!(
            exact_parts("12 JAN 1850"),
            Some(DateParts {
                year: 1850,
                month: Some(1),
                day: Some(12)
            })
        );
        assert_eq!(
            exact_parts("JAN 1850"),
            Some(DateParts {
                year: 1850,
                month: Some(1),
                day: None
            })
        );
        assert_eq!(
            exact_parts("1850"),
            Some(DateParts {
                year: 1850,
                month: None,
                day: None
            })
        );
        assert!(exact_parts("ABT 1850").is_none());
        assert!(exact_parts("1850-01-12").is_none());
        assert!(exact_parts("1699/1700").is_none());
        assert!(exact_parts("44 BC").is_none());
        assert!(exact_parts("(circa 1850)").is_none());
    }

    #[test]
    fn invalid_dates_are_rejected() {
        for input in [
            "",
            " ",
            "31 FEB 1850",
            "32 JAN 1850",
            "0 JAN 1850",
            "12 FOO 1850",
            "1850-13-01",
            "1850-01-32",
            "12/01/1850",
            "0000",
            "BET 1850",
            "BET 1850 AND",
            "ABT",
            "()",
        ] {
            assert!(canonicalize(input).is_err(), "{input:?} should be rejected");
        }
    }

    #[test]
    fn validate_matches_canonicalize() {
        assert!(validate("12 JAN 1850").is_ok());
        assert!(validate("31 FEB 1850").is_err());
    }
}
