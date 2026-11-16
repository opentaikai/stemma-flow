//! Chronological ordering for timeline reports.

/// Returns a sortable key for a GEDCOM date.
///
/// Partial dates sort before later months or days of the same year, and
/// qualifiers or ranges use their first usable date. Unparseable values
/// return `None` so callers can place them in an undated section.
pub fn date_sort_key(value: &str) -> Option<(i32, u32, u32)> {
    let canonical = crate::model::date::canonicalize(value).ok()?;
    parts_key(canonical.trim())
}

fn parts_key(text: &str) -> Option<(i32, u32, u32)> {
    for prefix in ["ABT ", "CAL ", "EST ", "BEF ", "AFT ", "TO "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return parts_key(rest);
        }
    }
    if let Some(rest) = text.strip_prefix("FROM ")
        && let Some((start, _)) = rest.split_once(" TO ")
    {
        return parts_key(start);
    }
    if let Some(rest) = text.strip_prefix("BET ")
        && let Some((start, _)) = rest.split_once(" AND ")
    {
        return parts_key(start);
    }
    let parts = crate::model::date::exact_parts(text)?;
    Some((parts.year, parts.month.unwrap_or(0), parts.day.unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::date;

    fn key(value: &str) -> Option<(i32, u32, u32)> {
        date_sort_key(value)
    }

    #[test]
    fn exact_dates_sort_by_year_month_and_day() {
        assert_eq!(key("12 JAN 1850"), Some((1850, 1, 12)));
        assert_eq!(key("3 MAR 1850"), Some((1850, 3, 3)));
        assert!(key("12 JAN 1850") < key("3 MAR 1850"));
    }

    #[test]
    fn partial_dates_sort_within_their_year() {
        assert_eq!(key("1850"), Some((1850, 0, 0)));
        assert_eq!(key("JAN 1850"), Some((1850, 1, 0)));
        assert!(key("1850") < key("JAN 1850"));
        assert!(key("JAN 1850") < key("12 JAN 1850"));
    }

    #[test]
    fn qualifiers_use_the_reference_date() {
        assert_eq!(key("ABT 1850"), Some((1850, 0, 0)));
        assert_eq!(key("CAL 12 JAN 1850"), Some((1850, 1, 12)));
        assert_eq!(key("EST 1850"), Some((1850, 0, 0)));
        assert_eq!(key("BEF 1850"), Some((1850, 0, 0)));
        assert_eq!(key("AFT 1850"), Some((1850, 0, 0)));
        assert_eq!(key("TO 1850"), Some((1850, 0, 0)));
    }

    #[test]
    fn ranges_use_their_first_date() {
        assert_eq!(key("BET 1850 AND 1860"), Some((1850, 0, 0)));
        assert_eq!(key("FROM JAN 1850 TO MAR 1860"), Some((1850, 1, 0)));
    }

    #[test]
    fn lowercase_input_is_canonicalized() {
        assert_eq!(key("12 jan 1850"), Some((1850, 1, 12)));
        let canonical = date::canonicalize("abt 1850").expect("canonical");
        assert_eq!(canonical, "ABT 1850");
    }

    #[test]
    fn unusable_values_have_no_key() {
        assert_eq!(key(""), None);
        assert_eq!(key("not a date"), None);
        assert_eq!(key("(circa spring)"), None);
    }
}
