//! Shared entry data for genealogy list reports.

/// One marriage shown next to a list entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMarriage {
    pub spouse: String,
    pub date: String,
    pub place: String,
}

/// Combines a date and place into one printable cell.
pub fn event_cell(date: &str, place: &str) -> String {
    let date = date.trim();
    let place = place.trim();
    match (date.is_empty(), place.is_empty()) {
        (true, true) => String::new(),
        (false, true) => date.to_string(),
        (true, false) => place.to_string(),
        (false, false) => format!("{date} \u{00b7} {place}"),
    }
}

/// Formats one marriage as `m. 1838 · Bath — Ada King`.
pub fn marriage_cell(marriage: &ListMarriage) -> String {
    let event = event_cell(&marriage.date, &marriage.place);
    if event.is_empty() {
        format!("m. {}", marriage.spouse)
    } else {
        format!("m. {event} \u{2014} {}", marriage.spouse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_cells_skip_missing_parts() {
        assert_eq!(event_cell("1815", "London"), "1815 \u{00b7} London");
        assert_eq!(event_cell("1815", ""), "1815");
        assert_eq!(event_cell("", "London"), "London");
        assert_eq!(event_cell(" ", " "), "");
    }

    #[test]
    fn marriage_cells_include_the_spouse() {
        let marriage = ListMarriage {
            spouse: "Ada King".to_string(),
            date: "1838".to_string(),
            place: "Bath".to_string(),
        };
        assert_eq!(
            marriage_cell(&marriage),
            "m. 1838 \u{00b7} Bath \u{2014} Ada King"
        );
        let no_date = ListMarriage {
            spouse: "Ada King".to_string(),
            date: String::new(),
            place: String::new(),
        };
        assert_eq!(marriage_cell(&no_date), "m. Ada King");
    }
}
