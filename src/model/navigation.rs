//! Session navigation history for Back/Forward person selection.

/// Maximum number of visited people retained.
pub const HISTORY_LIMIT: usize = 50;

/// A bounded person-selection history with a back/forward cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NavigationHistory {
    entries: Vec<String>,
    index: usize,
}

impl NavigationHistory {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index = 0;
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    pub fn current(&self) -> Option<&str> {
        self.entries.get(self.index).map(String::as_str)
    }

    pub fn can_back(&self) -> bool {
        !self.entries.is_empty() && self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    /// Records a selection unless it is already current.
    pub fn record(&mut self, person_id: impl Into<String>) {
        let person_id = person_id.into();
        if self.current() == Some(person_id.as_str()) {
            return;
        }
        if self.entries.is_empty() {
            self.entries.push(person_id);
            self.index = 0;
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(person_id);
        if self.entries.len() > HISTORY_LIMIT {
            let excess = self.entries.len() - HISTORY_LIMIT;
            self.entries.drain(0..excess);
        }
        self.index = self.entries.len() - 1;
    }

    pub fn back(&mut self) -> Option<&str> {
        if !self.can_back() {
            return None;
        }
        self.index -= 1;
        self.current()
    }

    pub fn forward(&mut self) -> Option<&str> {
        if !self.can_forward() {
            return None;
        }
        self.index += 1;
        self.current()
    }

    /// Drops entries that no longer resolve to a loaded person.
    pub fn retain_existing(&mut self, exists: impl Fn(&str) -> bool) {
        if self.entries.is_empty() {
            self.index = 0;
            return;
        }
        let current = self.current().map(str::to_string);
        self.entries.retain(|person_id| exists(person_id));
        if self.entries.is_empty() {
            self.index = 0;
            return;
        }
        self.index = current
            .and_then(|current| self.entries.iter().position(|entry| entry == &current))
            .unwrap_or(self.entries.len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_deduplicates_and_branches() {
        let mut history = NavigationHistory::default();
        history.record("a");
        history.record("a");
        history.record("b");
        history.record("c");
        assert_eq!(history.entries(), ["a", "b", "c"]);
        assert_eq!(history.back(), Some("b"));
        history.record("d");
        assert_eq!(history.entries(), ["a", "b", "d"]);
        assert_eq!(history.current(), Some("d"));
        assert!(!history.can_forward());
    }

    #[test]
    fn caps_entries_and_clamps_boundaries() {
        let mut history = NavigationHistory::default();
        for index in 0..HISTORY_LIMIT + 5 {
            history.record(format!("p{index}"));
        }
        assert_eq!(history.entries().len(), HISTORY_LIMIT);
        assert_eq!(history.entries()[0], "p5");
        assert_eq!(history.current(), Some("p54"));
        assert_eq!(history.back(), Some("p53"));
        assert_eq!(history.forward(), Some("p54"));
        history.clear();
        assert!(!history.can_back());
        assert!(!history.can_forward());
        assert_eq!(history.back(), None);
        assert_eq!(history.forward(), None);
    }

    #[test]
    fn prunes_missing_people_and_keeps_the_current_entry() {
        let mut history = NavigationHistory::default();
        history.record("a");
        history.record("b");
        history.record("c");
        let _ = history.back();
        history.retain_existing(|id| id != "b");
        assert_eq!(history.entries(), ["a", "c"]);
        assert!(history.entries().contains(&"c".to_string()));
    }
}
