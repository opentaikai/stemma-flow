//! Pinned, modeless person editor state.

use crate::db::associations::AssociationRecord;
use crate::db::groups::GroupSummary;
use crate::db::privacy::PersonPrivacy;
use crate::gui::citations::CitationsState;
use crate::gui::inspector::InspectorForm;
use crate::gui::names::NamesState;

/// A deferred editor action that needs confirmation first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorPrompt {
    Close,
    Switch(String),
}

/// State owned by the modeless person editor.
pub struct PersonEditor {
    pub person_id: String,
    pub form: InspectorForm,
    pub saved_form: InspectorForm,
    pub names: NamesState,
    pub citations: CitationsState,
    pub associations: Vec<AssociationRecord>,
    pub associations_owner: Option<String>,
    pub privacy: Option<PersonPrivacy>,
    pub groups: Vec<GroupSummary>,
    pub prompt: Option<EditorPrompt>,
    pub prompt_error: Option<String>,
}

impl PersonEditor {
    pub fn is_dirty(&self) -> bool {
        self.form != self.saved_form
    }
}
