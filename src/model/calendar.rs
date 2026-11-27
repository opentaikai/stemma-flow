//! Annual calendar events for reports and future dashboard previews.

use chrono::{Datelike, Months, NaiveDate};

/// Kind of annual event shown in calendar reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarEventKind {
    Birth,
    Marriage,
    Death,
}

impl CalendarEventKind {
    /// Stable ordering for events that share a day.
    pub fn rank(self) -> u8 {
        match self {
            Self::Birth => 0,
            Self::Marriage => 1,
            Self::Death => 2,
        }
    }

    /// Short prefix used by dense calendar cells.
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Birth => "b.",
            Self::Marriage => "m.",
            Self::Death => "d.",
        }
    }
}

/// One dated annual event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    pub kind: CalendarEventKind,
    pub month: u32,
    pub day: u32,
    pub year: i32,
    /// One name for births and deaths, both spouses for marriages.
    pub people: Vec<String>,
    pub place: String,
}

impl CalendarEvent {
    /// The next occurrence of this month/day on or after `today`.
    pub fn next_occurrence(&self, today: NaiveDate) -> NaiveDate {
        next_occurrence(self.month, self.day, today)
    }

    /// Whole years between the event year and the occurrence year.
    pub fn years_at(&self, occurrence: NaiveDate) -> u32 {
        (occurrence.year() - self.year).max(0) as u32
    }

    /// Compact label used in year-calendar day cells.
    pub fn short_label(&self) -> String {
        format!(
            "{} {} ({})",
            self.kind.prefix(),
            self.people.join(" & "),
            self.year
        )
    }
}

/// Next occurrence of a recurring month/day on or after `today`.
///
/// 29 February becomes 28 February in non-leap years.
pub fn next_occurrence(month: u32, day: u32, today: NaiveDate) -> NaiveDate {
    let this_year = date_in_year(today.year(), month, day).unwrap_or(today);
    if this_year >= today {
        this_year
    } else {
        date_in_year(today.year().saturating_add(1), month, day).unwrap_or(today)
    }
}

/// Events whose next occurrence falls within `months` months of `today`.
///
/// Results are sorted by occurrence, then event kind, then names.
pub fn upcoming(
    events: &[CalendarEvent],
    today: NaiveDate,
    months: u32,
) -> Vec<(NaiveDate, usize)> {
    let end = today
        .checked_add_months(Months::new(months))
        .unwrap_or(today);
    let mut rows: Vec<(NaiveDate, usize)> = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            let occurrence = event.next_occurrence(today);
            (occurrence <= end).then_some((occurrence, index))
        })
        .collect();
    rows.sort_by(|(left_date, left_index), (right_date, right_index)| {
        left_date.cmp(right_date).then_with(|| {
            let left = &events[*left_index];
            let right = &events[*right_index];
            left.kind.rank().cmp(&right.kind.rank()).then_with(|| {
                left.people
                    .join(" ")
                    .to_lowercase()
                    .cmp(&right.people.join(" ").to_lowercase())
            })
        })
    });
    rows
}

fn date_in_year(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day)
        .or_else(|| NaiveDate::from_ymd_opt(year, month, day.min(28)))
        .or_else(|| NaiveDate::from_ymd_opt(year, month, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
    }

    fn event(
        kind: CalendarEventKind,
        month: u32,
        day: u32,
        year: i32,
        name: &str,
    ) -> CalendarEvent {
        CalendarEvent {
            kind,
            month,
            day,
            year,
            people: vec![name.to_string()],
            place: String::new(),
        }
    }

    #[test]
    fn next_occurrence_stays_in_the_current_year_or_moves_on() {
        let today = date(2026, 11, 16);
        assert_eq!(
            next_occurrence(1, 12, today),
            date(2027, 1, 12),
            "a passed date moves to next year"
        );
        assert_eq!(
            next_occurrence(12, 1, today),
            date(2026, 12, 1),
            "a future date stays this year"
        );
        assert_eq!(
            next_occurrence(11, 16, today),
            today,
            "today counts as upcoming"
        );
    }

    #[test]
    fn leap_day_falls_back_to_the_twenty_eighth() {
        assert_eq!(next_occurrence(2, 29, date(2024, 1, 1)), date(2024, 2, 29));
        assert_eq!(next_occurrence(2, 29, date(2025, 1, 1)), date(2025, 2, 28));
        assert_eq!(next_occurrence(2, 29, date(2025, 3, 1)), date(2026, 2, 28));
    }

    #[test]
    fn upcoming_limits_the_window_and_sorts_by_date_then_kind() {
        let today = date(2026, 1, 1);
        let events = vec![
            event(CalendarEventKind::Death, 1, 15, 1900, "Zed"),
            event(CalendarEventKind::Birth, 1, 15, 1900, "Bea"),
            event(CalendarEventKind::Birth, 1, 15, 1900, "Ada"),
            event(CalendarEventKind::Marriage, 3, 1, 1850, "Ada & Bo"),
        ];
        let rows = upcoming(&events, today, 1);
        let dates: Vec<NaiveDate> = rows.iter().map(|(date, _)| *date).collect();
        assert_eq!(dates, [date(2026, 1, 15); 3]);
        let names: Vec<&str> = rows
            .iter()
            .map(|(_, index)| events[*index].people[0].as_str())
            .collect();
        assert_eq!(names, ["Ada", "Bea", "Zed"], "kind rank then name");
        assert!(
            upcoming(&events, today, 12).len() == 4,
            "the March event fits twelve months"
        );
    }

    #[test]
    fn age_uses_the_occurrence_year() {
        let birth = event(CalendarEventKind::Birth, 1, 12, 1810, "Ada");
        assert_eq!(birth.years_at(date(2026, 1, 12)), 216);
        assert_eq!(birth.years_at(date(2027, 1, 12)), 217);
    }

    #[test]
    fn short_labels_include_kind_names_and_year() {
        let birth = event(CalendarEventKind::Birth, 1, 12, 1810, "Ada");
        assert_eq!(birth.short_label(), "b. Ada (1810)");
        let marriage = CalendarEvent {
            kind: CalendarEventKind::Marriage,
            month: 6,
            day: 5,
            year: 1838,
            people: vec!["Ada".to_string(), "Bo".to_string()],
            place: String::new(),
        };
        assert_eq!(marriage.short_label(), "m. Ada & Bo (1838)");
    }
}
