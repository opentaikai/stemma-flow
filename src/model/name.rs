//! Formatting helpers for primary and alternate names.

/// Joins primary/alternate name pieces with single spaces.
pub fn full_name(
    prefix: &str,
    given_name: &str,
    middle_name: &str,
    surname: &str,
    suffix: &str,
) -> String {
    [prefix, given_name, middle_name, surname, suffix]
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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
    }
}
