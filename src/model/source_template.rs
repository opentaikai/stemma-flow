//! Source template definitions in the Evidence Explained style.
//!
//! A template declares its fields, the footnote/short-footnote/bibliography
//! formats and how the fields are cached into the GEDCOM-compatible source
//! columns. Formats use `{field}` placeholders; rendering lives in this
//! module's companion functions.

use std::collections::BTreeMap;

/// Whether a template field belongs to the source or to one citation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldScope {
    Source,
    Citation,
}

/// One editable field of a source template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateField {
    pub key: String,
    pub label: String,
    pub scope: FieldScope,
    pub required: bool,
    pub hint: Option<String>,
}

/// A source template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTemplate {
    pub key: String,
    pub name: String,
    pub category: String,
    pub fields: Vec<TemplateField>,
    pub footnote: String,
    pub short_footnote: String,
    pub bibliography: String,
    /// Source column -> format rendered from the source fields.
    pub gedcom: BTreeMap<String, String>,
}

impl SourceTemplate {
    /// Source-scoped fields in declaration order.
    pub fn source_fields(&self) -> impl Iterator<Item = &TemplateField> {
        self.fields
            .iter()
            .filter(|field| field.scope == FieldScope::Source)
    }

    /// Citation-scoped fields, such as the page.
    pub fn citation_fields(&self) -> impl Iterator<Item = &TemplateField> {
        self.fields
            .iter()
            .filter(|field| field.scope == FieldScope::Citation)
    }
}

fn field(
    key: &str,
    label: &str,
    scope: FieldScope,
    required: bool,
    hint: Option<&str>,
) -> TemplateField {
    TemplateField {
        key: key.to_string(),
        label: label.to_string(),
        scope,
        required,
        hint: hint.map(str::to_string),
    }
}

fn gedcom(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(column, format)| ((*column).to_string(), (*format).to_string()))
        .collect()
}

/// The built-in template catalogue.
pub fn builtin_templates() -> Vec<SourceTemplate> {
    vec![
        SourceTemplate {
            key: "book".to_string(),
            name: "Book".to_string(),
            category: "Books".to_string(),
            fields: vec![
                field("author", "Author", FieldScope::Source, false, None),
                field("title", "Title", FieldScope::Source, true, None),
                field("place", "Publication place", FieldScope::Source, false, None),
                field("publisher", "Publisher", FieldScope::Source, false, None),
                field("year", "Year", FieldScope::Source, false, None),
                field("page", "Page", FieldScope::Citation, false, None),
            ],
            footnote: "{author}, {title} ({place}: {publisher}, {year}), {page}.".to_string(),
            short_footnote: "{author}, {title}, {page}.".to_string(),
            bibliography: "{author}. {title}. {place}: {publisher}, {year}.".to_string(),
            gedcom: gedcom(&[
                ("title", "{title}"),
                ("author", "{author}"),
                ("publication", "{place}: {publisher}, {year}"),
            ]),
        },
        SourceTemplate {
            key: "census".to_string(),
            name: "Census".to_string(),
            category: "Census".to_string(),
            fields: vec![
                field("year", "Census year", FieldScope::Source, true, None),
                field("country", "Country", FieldScope::Source, false, None),
                field("state", "State", FieldScope::Source, false, None),
                field("county", "County", FieldScope::Source, false, None),
                field("locality", "Locality", FieldScope::Source, false, None),
                field("dwelling", "Dwelling", FieldScope::Source, false, None),
                field("family", "Family", FieldScope::Source, false, None),
                field("person", "Person", FieldScope::Source, false, None),
                field("website", "Website", FieldScope::Source, false, None),
                field("accessed", "Accessed", FieldScope::Source, false, None),
                field("page", "Page", FieldScope::Citation, false, None),
            ],
            footnote: "{year} {country} census, {state}, {county}, {locality}, population schedule, page {page}, dwelling {dwelling}, family {family}, {person}; digital image, {website} (accessed {accessed}).".to_string(),
            short_footnote: "{year} census, {state}, {county}, {locality}, {page}.".to_string(),
            bibliography: "{country}. {state}. {county}. {year} census. {website}. Accessed {accessed}.".to_string(),
            gedcom: gedcom(&[
                (
                    "title",
                    "{year} {country} census, {state}, {county}, {locality}",
                ),
                ("publication", "{website}"),
                ("notes", "Accessed {accessed}"),
            ]),
        },
        SourceTemplate {
            key: "certificate".to_string(),
            name: "Certificate".to_string(),
            category: "Vital records".to_string(),
            fields: vec![
                field("jurisdiction", "Jurisdiction", FieldScope::Source, true, None),
                field("event_type", "Event type", FieldScope::Source, true, None),
                field("name", "Name", FieldScope::Source, true, None),
                field("event_date", "Event date", FieldScope::Source, false, None),
                field("registration", "Registration", FieldScope::Source, false, None),
                field("repository", "Repository", FieldScope::Source, false, None),
                field("page", "Page", FieldScope::Citation, false, None),
            ],
            footnote: "{jurisdiction}, {event_type} certificate, {name}, {event_date}, {registration}; {repository}, {page}.".to_string(),
            short_footnote: "{jurisdiction}, {event_type} certificate, {name}, {page}.".to_string(),
            bibliography: "{jurisdiction}. {event_type} certificate, {name}. {registration}. {repository}.".to_string(),
            gedcom: gedcom(&[
                (
                    "title",
                    "{jurisdiction} {event_type} certificate, {name}",
                ),
                ("publication", "{repository}"),
                ("notes", "{registration}"),
            ]),
        },
        SourceTemplate {
            key: "website".to_string(),
            name: "Website".to_string(),
            category: "Websites".to_string(),
            fields: vec![
                field("creator", "Creator", FieldScope::Source, false, None),
                field("title", "Title", FieldScope::Source, true, None),
                field("website", "Website", FieldScope::Source, false, None),
                field("url", "URL", FieldScope::Source, false, None),
                field("accessed", "Accessed", FieldScope::Source, false, None),
            ],
            footnote: "{creator}, \"{title},\" {website} ({url}: accessed {accessed}).".to_string(),
            short_footnote: "{creator}, \"{title}.\"".to_string(),
            bibliography: "{creator}. \"{title}.\" {website}. {url} (accessed {accessed})."
                .to_string(),
            gedcom: gedcom(&[
                ("title", "{title}"),
                ("author", "{creator}"),
                ("publication", "{website}"),
                ("notes", "{url} (accessed {accessed})"),
            ]),
        },
        SourceTemplate {
            key: "cemetery".to_string(),
            name: "Cemetery memorial".to_string(),
            category: "Cemeteries".to_string(),
            fields: vec![
                field("cemetery", "Cemetery", FieldScope::Source, true, None),
                field("location", "Location", FieldScope::Source, false, None),
                field("memorial_id", "Memorial ID", FieldScope::Source, false, None),
                field("person", "Person", FieldScope::Source, false, None),
                field("website", "Website", FieldScope::Source, false, None),
                field("accessed", "Accessed", FieldScope::Source, false, None),
            ],
            footnote: "{cemetery}, {location}, memorial {memorial_id}, {person}; {website} (accessed {accessed}).".to_string(),
            short_footnote: "{cemetery}, {person}, memorial {memorial_id}.".to_string(),
            bibliography: "{cemetery}. {location}. Memorial {memorial_id}, {person}. {website}. Accessed {accessed}.".to_string(),
            gedcom: gedcom(&[
                (
                    "title",
                    "{cemetery}, {person}, memorial {memorial_id}",
                ),
                ("publication", "{website}"),
                ("notes", "Accessed {accessed}"),
            ]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_unique_keys_and_required_fields() {
        let templates = builtin_templates();
        let mut keys = std::collections::HashSet::new();
        for template in &templates {
            assert!(
                keys.insert(template.key.clone()),
                "duplicate {}",
                template.key
            );
            assert!(!template.name.is_empty());
            assert!(
                template.fields.iter().any(|field| field.required),
                "{} needs at least one required field",
                template.key
            );
            assert!(
                template
                    .fields
                    .iter()
                    .any(|field| field.scope == FieldScope::Source),
                "{} needs source fields",
                template.key
            );
            assert!(
                template.footnote.contains('{'),
                "{} needs placeholders",
                template.key
            );
        }
        assert_eq!(templates.len(), 5);
    }
}
