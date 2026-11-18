//! Family history book editor.

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::db::albums::AlbumSummary;
use crate::db::groups::GroupSummary;
use crate::gui::inspector::person_label;
use crate::i18n::tr;
use crate::model::book::{BookChapter, BookDefinition};

/// Chapter type selected in the editor form.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChapterKind {
    #[default]
    Text,
    Person,
    Family,
    Timeline,
    Album,
    Group,
}

impl ChapterKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "family_book.kind_text",
            Self::Person => "family_book.kind_person",
            Self::Family => "family_book.kind_family",
            Self::Timeline => "family_book.kind_timeline",
            Self::Album => "family_book.kind_album",
            Self::Group => "family_book.kind_group",
        }
    }
}

fn chapter_kind_label(chapter: &BookChapter) -> String {
    tr(match chapter {
        BookChapter::Text { .. } => "family_book.kind_text",
        BookChapter::Person { .. } => "family_book.kind_person",
        BookChapter::Family { .. } => "family_book.kind_family",
        BookChapter::Timeline { .. } => "family_book.kind_timeline",
        BookChapter::Album { .. } => "family_book.kind_album",
        BookChapter::Group { .. } => "family_book.kind_group",
    })
}

/// State owned by the family history book editor.
#[derive(Debug)]
pub struct FamilyBookState {
    pub open: bool,
    pub books: Vec<BookDefinition>,
    pub selected: Option<String>,
    pub draft: BookDefinition,
    pub chapter_selected: Option<String>,
    pub chapter_kind: ChapterKind,
    pub chapter_title: String,
    pub chapter_body: String,
    pub chapter_person: Option<String>,
    pub chapter_family: Option<String>,
    pub chapter_album: Option<String>,
    pub chapter_group: Option<String>,
    pub albums: Vec<AlbumSummary>,
    pub groups: Vec<GroupSummary>,
    pub families: Vec<(String, String)>,
    pub error: Option<String>,
}

impl Default for FamilyBookState {
    fn default() -> Self {
        Self {
            open: false,
            books: Vec::new(),
            selected: None,
            draft: BookDefinition::default(),
            chapter_selected: None,
            chapter_kind: ChapterKind::Text,
            chapter_title: String::new(),
            chapter_body: String::new(),
            chapter_person: None,
            chapter_family: None,
            chapter_album: None,
            chapter_group: None,
            albums: Vec::new(),
            groups: Vec::new(),
            families: Vec::new(),
            error: None,
        }
    }
}

impl FamilyBookState {
    pub fn open_with(
        &mut self,
        books: Vec<BookDefinition>,
        albums: Vec<AlbumSummary>,
        groups: Vec<GroupSummary>,
        families: Vec<(String, String)>,
    ) {
        self.open = true;
        self.books = books;
        self.albums = albums;
        self.groups = groups;
        self.families = families;
        self.selected = self.books.first().map(|book| book.id.clone());
        self.draft = self
            .books
            .first()
            .cloned()
            .unwrap_or_else(BookDefinition::default);
        self.reset_chapter();
        self.error = None;
    }

    fn reset_chapter(&mut self) {
        self.chapter_selected = None;
        self.chapter_kind = ChapterKind::Text;
        self.chapter_title.clear();
        self.chapter_body.clear();
        self.chapter_person = None;
        self.chapter_family = None;
        self.chapter_album = None;
        self.chapter_group = None;
    }

    fn store_draft(&mut self) {
        self.draft.sanitize();
        if let Some(existing) = self.books.iter_mut().find(|book| book.id == self.draft.id) {
            *existing = self.draft.clone();
        } else {
            self.books.push(self.draft.clone());
        }
    }

    fn select_book(&mut self, id: &str) {
        self.store_draft();
        if let Some(book) = self.books.iter().find(|book| book.id == id) {
            self.selected = Some(id.to_string());
            self.draft = book.clone();
            self.reset_chapter();
        }
    }

    fn new_book(&mut self) {
        self.store_draft();
        let book = BookDefinition::new("New book");
        self.selected = Some(book.id.clone());
        self.draft = book;
        self.reset_chapter();
    }

    fn delete_book(&mut self) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        self.books.retain(|book| book.id != id);
        if let Some(book) = self.books.first().cloned() {
            self.selected = Some(book.id.clone());
            self.draft = book;
        } else {
            self.selected = None;
            self.draft = BookDefinition::default();
        }
        self.reset_chapter();
    }

    fn load_chapter(&mut self, chapter: &BookChapter) {
        self.chapter_selected = Some(chapter.id().to_string());
        self.chapter_title = chapter.title().to_string();
        self.reset_chapter_values();
        match chapter {
            BookChapter::Text { body, .. } => {
                self.chapter_kind = ChapterKind::Text;
                self.chapter_body = body.clone();
            }
            BookChapter::Person { person_id, .. } => {
                self.chapter_kind = ChapterKind::Person;
                self.chapter_person = Some(person_id.clone());
            }
            BookChapter::Family { family_id, .. } => {
                self.chapter_kind = ChapterKind::Family;
                self.chapter_family = Some(family_id.clone());
            }
            BookChapter::Timeline { person_id, .. } => {
                self.chapter_kind = ChapterKind::Timeline;
                self.chapter_person = Some(person_id.clone());
            }
            BookChapter::Album { album_id, .. } => {
                self.chapter_kind = ChapterKind::Album;
                self.chapter_album = Some(album_id.clone());
            }
            BookChapter::Group { group_id, .. } => {
                self.chapter_kind = ChapterKind::Group;
                self.chapter_group = Some(group_id.clone());
            }
        }
    }

    fn reset_chapter_values(&mut self) {
        self.chapter_body.clear();
        self.chapter_person = None;
        self.chapter_family = None;
        self.chapter_album = None;
        self.chapter_group = None;
    }

    fn build_chapter(&self) -> Result<BookChapter, String> {
        let title = self.chapter_title.trim();
        let chapter = match self.chapter_kind {
            ChapterKind::Text => BookChapter::text(title, &self.chapter_body),
            ChapterKind::Person => BookChapter::person(
                title,
                self.chapter_person
                    .as_deref()
                    .ok_or_else(|| "select a person for this chapter".to_string())?,
            ),
            ChapterKind::Family => BookChapter::family(
                title,
                self.chapter_family
                    .as_deref()
                    .ok_or_else(|| "select a family for this chapter".to_string())?,
            ),
            ChapterKind::Timeline => BookChapter::timeline(
                title,
                self.chapter_person
                    .as_deref()
                    .ok_or_else(|| "select a person for this chapter".to_string())?,
            ),
            ChapterKind::Album => BookChapter::album(
                title,
                self.chapter_album
                    .as_deref()
                    .ok_or_else(|| "select an album for this chapter".to_string())?,
            ),
            ChapterKind::Group => BookChapter::group(
                title,
                self.chapter_group
                    .as_deref()
                    .ok_or_else(|| "select a group for this chapter".to_string())?,
            ),
        };
        Ok(chapter)
    }

    fn save_chapter(&mut self) {
        match self.build_chapter() {
            Ok(mut chapter) => {
                if let Some(selected) = &self.chapter_selected
                    && let Some(existing) = self
                        .draft
                        .chapters
                        .iter_mut()
                        .find(|candidate| candidate.id() == selected)
                {
                    chapter.set_id(selected.clone());
                    *existing = chapter;
                    self.error = None;
                    return;
                }
                self.draft.chapters.push(chapter);
                self.error = None;
                self.reset_chapter();
            }
            Err(message) => self.error = Some(message),
        }
    }

    fn move_chapter(&mut self, delta: isize) {
        let Some(selected) = self.chapter_selected.as_deref() else {
            return;
        };
        let Some(index) = self
            .draft
            .chapters
            .iter()
            .position(|chapter| chapter.id() == selected)
        else {
            return;
        };
        let target = index as isize + delta;
        if target < 0 || target >= self.draft.chapters.len() as isize {
            return;
        }
        self.draft.chapters.swap(index, target as usize);
    }

    fn remove_chapter(&mut self) {
        let Some(selected) = self.chapter_selected.clone() else {
            return;
        };
        self.draft
            .chapters
            .retain(|chapter| chapter.id() != selected);
        self.reset_chapter();
    }
}

/// Result of saving or exporting from the editor.
#[derive(Debug, Clone, PartialEq)]
pub struct BookEditorOutcome {
    pub books: Vec<BookDefinition>,
    pub export: Option<BookDefinition>,
}

/// Draws the family history book editor.
pub fn show(
    ctx: &Context,
    state: &mut FamilyBookState,
    people: &[Person],
    surname_first: bool,
) -> Option<BookEditorOutcome> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut outcome = None;
    Window::new(tr("family_book.title"))
        .id(egui::Id::unique("family_book_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(860.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("family_book.description")).weak().small());
            ui.separator();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr("family_book.new_book")).clicked() {
                            state.new_book();
                        }
                        if ui
                            .add_enabled(
                                state.selected.is_some(),
                                egui::Button::new(tr("common.delete")),
                            )
                            .clicked()
                        {
                            state.delete_book();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("book_list")
                        .max_height(360.0)
                        .show(ui, |ui| {
                            if state.books.is_empty() {
                                ui.label(RichText::new(tr("family_book.no_books")).weak().small());
                            }
                            let selected = state.selected.clone();
                            let mut pick = None;
                            for book in &state.books {
                                if ui
                                    .selectable_label(
                                        selected.as_deref() == Some(book.id.as_str()),
                                        &book.title,
                                    )
                                    .clicked()
                                {
                                    pick = Some(book.id.clone());
                                }
                            }
                            if let Some(id) = pick {
                                state.select_book(&id);
                            }
                        });
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::Grid::new("book_fields")
                        .num_columns(2)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(tr("family_book.title_field"));
                            ui.text_edit_singleline(&mut state.draft.title);
                            ui.end_row();
                            ui.label(tr("family_book.subtitle"));
                            ui.text_edit_singleline(&mut state.draft.subtitle);
                            ui.end_row();
                            ui.label(tr("family_book.author"));
                            ui.text_edit_singleline(&mut state.draft.author);
                            ui.end_row();
                        });
                    ui.separator();
                    ui.label(RichText::new(tr("family_book.chapters")).strong());
                    egui::ScrollArea::vertical()
                        .id_salt("book_chapters")
                        .max_height(200.0)
                        .show(ui, |ui| {
                            if state.draft.chapters.is_empty() {
                                ui.label(RichText::new(tr("family_book.add_chapter_hint")).weak());
                            }
                            let selected = state.chapter_selected.clone();
                            let mut pick = None;
                            for (index, chapter) in state.draft.chapters.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        selected.as_deref() == Some(chapter.id()),
                                        format!(
                                            "{}. {} ({})",
                                            index + 1,
                                            chapter.title(),
                                            chapter_kind_label(chapter)
                                        ),
                                    )
                                    .clicked()
                                {
                                    pick = Some(chapter.clone());
                                }
                            }
                            if let Some(chapter) = pick {
                                state.load_chapter(&chapter);
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                state.chapter_selected.is_some(),
                                egui::Button::new(tr("family_book.up")),
                            )
                            .clicked()
                        {
                            state.move_chapter(-1);
                        }
                        if ui
                            .add_enabled(
                                state.chapter_selected.is_some(),
                                egui::Button::new(tr("family_book.down")),
                            )
                            .clicked()
                        {
                            state.move_chapter(1);
                        }
                        if ui
                            .add_enabled(
                                state.chapter_selected.is_some(),
                                egui::Button::new(tr("common.remove")),
                            )
                            .clicked()
                        {
                            state.remove_chapter();
                        }
                        if ui.button(tr("family_book.new_chapter")).clicked() {
                            state.reset_chapter();
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(tr("family_book.type"));
                        egui::ComboBox::from_id_salt("book_chapter_kind")
                            .selected_text(state.chapter_kind.label())
                            .show_ui(ui, |ui| {
                                for kind in [
                                    ChapterKind::Text,
                                    ChapterKind::Person,
                                    ChapterKind::Family,
                                    ChapterKind::Timeline,
                                    ChapterKind::Album,
                                    ChapterKind::Group,
                                ] {
                                    ui.selectable_value(
                                        &mut state.chapter_kind,
                                        kind,
                                        kind.label(),
                                    );
                                }
                            });
                        ui.label(tr("family_book.title_field"));
                        ui.text_edit_singleline(&mut state.chapter_title);
                    });
                    match state.chapter_kind {
                        ChapterKind::Text => {
                            ui.add(
                                egui::TextEdit::multiline(&mut state.chapter_body)
                                    .desired_rows(5)
                                    .desired_width(f32::INFINITY)
                                    .hint_text(tr("family_book.chapter_text_hint")),
                            );
                        }
                        ChapterKind::Person => {
                            person_picker(
                                ui,
                                &tr("report_dialog.person"),
                                people,
                                surname_first,
                                &mut state.chapter_person,
                            );
                        }
                        ChapterKind::Timeline => {
                            person_picker(
                                ui,
                                &tr("report_dialog.person"),
                                people,
                                surname_first,
                                &mut state.chapter_person,
                            );
                        }
                        ChapterKind::Family => {
                            choice_picker(
                                ui,
                                &tr("family_book.family"),
                                &state.families,
                                &mut state.chapter_family,
                            );
                        }
                        ChapterKind::Album => {
                            let choices: Vec<(String, String)> = state
                                .albums
                                .iter()
                                .map(|album| (album.id.clone(), album.name.clone()))
                                .collect();
                            choice_picker(
                                ui,
                                &tr("family_book.album"),
                                &choices,
                                &mut state.chapter_album,
                            );
                        }
                        ChapterKind::Group => {
                            let choices: Vec<(String, String)> = state
                                .groups
                                .iter()
                                .map(|group| (group.id.clone(), group.name.clone()))
                                .collect();
                            choice_picker(
                                ui,
                                &tr("family_book.group"),
                                &choices,
                                &mut state.chapter_group,
                            );
                        }
                    }
                    if let Some(error) = &state.error {
                        ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
                    }
                    if ui.button(tr("family_book.add_chapter")).clicked() {
                        state.save_chapter();
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new(tr("common.save")).strong()))
                            .clicked()
                        {
                            state.store_draft();
                            state.open = false;
                            outcome = Some(BookEditorOutcome {
                                books: state.books.clone(),
                                export: None,
                            });
                        }
                        if ui
                            .add_enabled(
                                state.selected.is_some(),
                                egui::Button::new(RichText::new(tr("common.export")).strong()),
                            )
                            .clicked()
                        {
                            state.store_draft();
                            state.open = false;
                            outcome = Some(BookEditorOutcome {
                                books: state.books.clone(),
                                export: Some(state.draft.clone()),
                            });
                        }
                        if ui.button(tr("common.cancel")).clicked() {
                            state.open = false;
                        }
                    });
                });
            });
        });
    if !open {
        state.open = false;
    }
    outcome
}

fn person_picker(
    ui: &mut egui::Ui,
    label: &str,
    people: &[Person],
    surname_first: bool,
    selected: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let current = selected
            .as_deref()
            .and_then(|id| people.iter().find(|person| person.id == id))
            .map(|person| person_label(person, surname_first))
            .unwrap_or_else(|| tr("family_book.select"));
        egui::ComboBox::from_id_salt(format!("book_{label}"))
            .selected_text(current)
            .show_ui(ui, |ui| {
                for person in people {
                    let active = selected.as_deref() == Some(person.id.as_str());
                    if ui
                        .selectable_label(active, person_label(person, surname_first))
                        .clicked()
                    {
                        *selected = Some(person.id.clone());
                    }
                }
            });
    });
}

fn choice_picker(
    ui: &mut egui::Ui,
    label: &str,
    choices: &[(String, String)],
    selected: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let current = selected
            .as_deref()
            .and_then(|id| choices.iter().find(|(choice, _)| choice == id))
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| tr("family_book.select"));
        egui::ComboBox::from_id_salt(format!("book_{label}"))
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (id, name) in choices {
                    if ui
                        .selectable_label(selected.as_deref() == Some(id.as_str()), name)
                        .clicked()
                    {
                        *selected = Some(id.clone());
                    }
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::groups::GroupSummary;

    #[test]
    fn opens_with_books_and_renders() {
        use egui::{Context, RawInput};
        let ctx = Context::default();
        let mut state = FamilyBookState::default();
        state.open_with(
            vec![BookDefinition::new("King family")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let person = Person::new("Ada", "King", "F");
            assert!(show(ui.ctx(), &mut state, &[person], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }

    #[test]
    fn adds_text_chapters_and_reorders_them() {
        let mut state = FamilyBookState::default();
        state.open_with(Vec::new(), Vec::new(), Vec::new(), Vec::new());
        state.chapter_kind = ChapterKind::Text;
        state.chapter_title = "Introduction".to_string();
        state.chapter_body = "First chapter".to_string();
        state.save_chapter();
        state.chapter_title = "Second".to_string();
        state.chapter_body = "Second chapter".to_string();
        state.save_chapter();
        assert_eq!(state.draft.chapters.len(), 2);
        let second = state.draft.chapters[1].id().to_string();
        state.chapter_selected = Some(second);
        state.move_chapter(-1);
        assert_eq!(state.draft.chapters[0].title(), "Second");
    }

    #[test]
    fn report_chapters_require_a_selection() {
        let mut state = FamilyBookState::default();
        state.open_with(Vec::new(), Vec::new(), Vec::new(), Vec::new());
        state.chapter_kind = ChapterKind::Person;
        state.save_chapter();
        assert!(
            state
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("person")
        );
        assert!(state.draft.chapters.is_empty());
    }

    #[test]
    fn group_choices_are_available() {
        let state = FamilyBookState {
            groups: vec![GroupSummary {
                id: "g1".to_string(),
                name: "Researchers".to_string(),
                description: None,
                rule: None,
                color: None,
                member_count: 2,
            }],
            ..FamilyBookState::default()
        };
        assert_eq!(state.groups.len(), 1);
    }
}
