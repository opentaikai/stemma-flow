//! Formatting helpers for primary and alternate names.

/// Joins primary/alternate name pieces with single spaces.
pub fn full_name(
    prefix: &str,
    given_name: &str,
    middle_name: &str,
    surname: &str,
    suffix: &str,
) -> String {
    format_name(prefix, given_name, middle_name, surname, suffix, false)
}

/// Formats a name in normal or surname-first order.
pub fn format_name(
    prefix: &str,
    given_name: &str,
    middle_name: &str,
    surname: &str,
    suffix: &str,
    surname_first: bool,
) -> String {
    let given = [prefix, given_name, middle_name, suffix]
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let surname = surname.trim();
    if !surname_first {
        return [prefix, given_name, middle_name, surname, suffix]
            .into_iter()
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
    }
    match (surname.is_empty(), given.is_empty()) {
        (true, true) => String::new(),
        (true, false) => given,
        (false, true) => surname.to_string(),
        (false, false) => format!("{surname}, {given}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_all_name_pieces() {
        assert_eq!(
            full_name("Dr.", "Ada", "Lynn", "King", "Jr."),
            "Dr. Ada Lynn King Jr."
        );
        assert_eq!(full_name("", "Ada", "", "King", ""), "Ada King");
        assert_eq!(full_name(" ", " ", "", " ", ""), "");
        assert_eq!(
            format_name("Dr.", "Ada", "Lynn", "King", "Jr.", true),
            "King, Dr. Ada Lynn Jr."
        );
        assert_eq!(format_name("", "Ada", "", "King", "", true), "King, Ada");
        assert_eq!(format_name("", "Ada", "", "", "", true), "Ada");
        assert_eq!(format_name("", "", "", "King", "", true), "King");
    }
}
