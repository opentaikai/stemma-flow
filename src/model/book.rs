//! Family history book definitions and chapters.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_TEXT: usize = 200_000;
const MAX_TITLE: usize = 200;

/// One named book with ordered chapters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookDefinition {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub author: String,
    pub chapters: Vec<BookChapter>,
}

impl Default for BookDefinition {
    fn default() -> Self {
        Self::new("Family History")
    }
}

impl BookDefinition {
    pub fn new(title: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            subtitle: String::new(),
            author: String::new(),
            chapters: Vec::new(),
        }
    }

    pub fn sanitize(&mut self) {
        if self.id.trim().is_empty() {
            self.id = Uuid::new_v4().to_string();
        }
        self.title = clean(&self.title, MAX_TITLE);
        if self.title.is_empty() {
            self.title = "Family History".to_string();
        }
        self.subtitle = clean(&self.subtitle, MAX_TITLE);
        self.author = clean(&self.author, MAX_TITLE);
        let mut seen = std::collections::HashSet::new();
        for chapter in &mut self.chapters {
            chapter.sanitize();
            if !seen.insert(chapter.id().to_string()) {
                chapter.set_id(Uuid::new_v4().to_string());
            }
        }
    }
}

/// One chapter in a family history book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BookChapter {
    Text {
        id: String,
        title: String,
        body: String,
    },
    Person {
        id: String,
        title: String,
        person_id: String,
    },
    Family {
        id: String,
        title: String,
        family_id: String,
    },
    Timeline {
        id: String,
        title: String,
        person_id: String,
    },
    Album {
        id: String,
        title: String,
        album_id: String,
    },
    Group {
        id: String,
        title: String,
        group_id: String,
    },
}

impl BookChapter {
    pub fn id(&self) -> &str {
        match self {
            Self::Text { id, .. }
            | Self::Person { id, .. }
            | Self::Family { id, .. }
            | Self::Timeline { id, .. }
            | Self::Album { id, .. }
            | Self::Group { id, .. } => id,
        }
    }

    pub fn set_id(&mut self, value: String) {
        match self {
            Self::Text { id, .. }
            | Self::Person { id, .. }
            | Self::Family { id, .. }
            | Self::Timeline { id, .. }
            | Self::Album { id, .. }
            | Self::Group { id, .. } => *id = value,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Text { title, .. }
            | Self::Person { title, .. }
            | Self::Family { title, .. }
            | Self::Timeline { title, .. }
            | Self::Album { title, .. }
            | Self::Group { title, .. } => title,
        }
    }

    pub fn set_title(&mut self, value: String) {
        match self {
            Self::Text { title, .. }
            | Self::Person { title, .. }
            | Self::Family { title, .. }
            | Self::Timeline { title, .. }
            | Self::Album { title, .. }
            | Self::Group { title, .. } => *title = value,
        }
    }

    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Text { .. } => "Text",
            Self::Person { .. } => "Individual summary",
            Self::Family { .. } => "Family group sheet",
            Self::Timeline { .. } => "Timeline",
            Self::Album { .. } => "Photo album",
            Self::Group { .. } => "Group roster",
        }
    }

    pub fn text(title: &str, body: &str) -> Self {
        Self::Text {
            id: Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            body: body.trim().to_string(),
        }
    }

    pub fn person(title: &str, person_id: &str) -> Self {
        Self::Person {
            id: Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            person_id: person_id.to_string(),
        }
    }

    pub fn family(title: &str, family_id: &str) -> Self {
        Self::Family {
            id: Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            family_id: family_id.to_string(),
        }
    }

    pub fn timeline(title: &str, person_id: &str) -> Self {
        Self::Timeline {
            id: Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            person_id: person_id.to_string(),
        }
    }

    pub fn album(title: &str, album_id: &str) -> Self {
        Self::Album {
            id: Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            album_id: album_id.to_string(),
        }
    }

    pub fn group(title: &str, group_id: &str) -> Self {
        Self::Group {
            id: Uuid::new_v4().to_string(),
            title: title.trim().to_string(),
            group_id: group_id.to_string(),
        }
    }

    pub fn sanitize(&mut self) {
        if self.id().trim().is_empty() {
            self.set_id(Uuid::new_v4().to_string());
        }
        let title = clean(self.title(), MAX_TITLE);
        self.set_title(title);
        if let Self::Text { body, .. } = self {
            *body = clean_multiline(body, MAX_TEXT);
        }
    }
}

fn clean(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

fn clean_multiline(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_books_have_ids_and_defaults() {
        let book = BookDefinition::new("  King family  ");
        assert!(!book.id.is_empty());
        assert_eq!(book.title, "King family");
        assert!(book.chapters.is_empty());
    }

    #[test]
    fn chapters_expose_ids_titles_and_kinds() {
        let mut chapter = BookChapter::person("Ada", "p1");
        assert!(!chapter.id().is_empty());
        assert_eq!(chapter.kind_label(), "Individual summary");
        chapter.set_title("Ada King".to_string());
        assert_eq!(chapter.title(), "Ada King");
        assert_eq!(BookChapter::text("Note", "Body").kind_label(), "Text");
        assert_eq!(
            BookChapter::family("F", "f1").kind_label(),
            "Family group sheet"
        );
        assert_eq!(BookChapter::timeline("T", "p1").kind_label(), "Timeline");
        assert_eq!(BookChapter::album("A", "a1").kind_label(), "Photo album");
        assert_eq!(BookChapter::group("G", "g1").kind_label(), "Group roster");
    }

    #[test]
    fn sanitize_trims_and_replaces_duplicate_chapter_ids() {
        let mut book = BookDefinition {
            id: String::new(),
            title: "   ".to_string(),
            subtitle: " Sub ".to_string(),
            author: "  ".to_string(),
            chapters: vec![
                BookChapter::text(" A ", " body "),
                BookChapter::text("B", ""),
            ],
        };
        if let BookChapter::Text { id, .. } = &mut book.chapters[0] {
            *id = "same".to_string();
        }
        if let BookChapter::Text { id, .. } = &mut book.chapters[1] {
            *id = "same".to_string();
        }
        book.sanitize();
        assert!(!book.id.is_empty());
        assert_eq!(book.title, "Family History");
        assert_eq!(book.subtitle, "Sub");
        assert_eq!(book.chapters[0].title(), "A");
        assert_ne!(book.chapters[0].id(), book.chapters[1].id());
    }

    #[test]
    fn books_round_trip_through_json() {
        let mut book = BookDefinition::new("Book");
        book.chapters.push(BookChapter::text("Intro", "Hello"));
        book.chapters.push(BookChapter::person("Ada", "p1"));
        let text = serde_json::to_string(&book).expect("serialize");
        let parsed: BookDefinition = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, book);
    }
}
