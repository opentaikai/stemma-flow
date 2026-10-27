//! Serializes the SQLite state back into GEDCOM 5.5.1.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::db::facts::FactLookup;
use crate::gedcom::GedcomError;
use crate::model::date;

struct PersonRow {
    id: String,
    given_name: String,
    middle_name: String,
    surname: String,
    nickname: String,
    gender: String,
    notes: String,
    private: bool,
    living: bool,
}

struct FamilyRow {
    id: String,
    husband_id: Option<String>,
    wife_id: Option<String>,
}

struct EventRow {
    id: String,
    event_type: String,
    date: Option<String>,
    place: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    description: Option<String>,
    private: bool,
    person_id: Option<String>,
    family_id: Option<String>,
}

/// Options controlling privacy filtering and GEDCOM privacy markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOptions {
    pub include_living: bool,
    pub include_private: bool,
    pub living_age: u32,
    /// 0 asks the exporter to read the current year from SQLite.
    pub current_year: i32,
    pub write_markers: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_living: false,
            include_private: false,
            living_age: crate::db::privacy::DEFAULT_LIVING_AGE,
            current_year: 0,
            write_markers: true,
        }
    }
}

struct ParticipantRow {
    event_id: String,
    person_id: String,
    role_label: String,
}

struct RepositoryRow {
    id: String,
    name: String,
    address: Option<String>,
    notes: Option<String>,
}

struct SourceRow {
    id: String,
    title: String,
    author: Option<String>,
    publication: Option<String>,
    abbreviation: Option<String>,
    notes: Option<String>,
    data_date: Option<String>,
    data_text: Option<String>,
}

struct RepoLink {
    source_id: String,
    repository_id: String,
    call_number: Option<String>,
    media_type: Option<String>,
}

struct CitationRow {
    source_id: String,
    page_or_reference: Option<String>,
    quality: Option<String>,
    note: Option<String>,
    media_type: Option<String>,
    data_date: Option<String>,
    data_text: Option<String>,
    event_id: Option<String>,
}

type CitationsByEvent<'a> = HashMap<&'a str, Vec<&'a CitationRow>>;

/// Canonicalises a stored date for export. Unparseable legacy text is
/// emitted as a GEDCOM date phrase so the file stays valid and the text
/// survives; whitespace (including newlines) collapses to single spaces.
fn date_payload(raw: &str) -> Option<String> {
    let cleaned = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return None;
    }
    Some(match date::canonicalize(&cleaned) {
        Ok(canonical) => canonical,
        Err(_) => format!("({cleaned})"),
    })
}

/// Writes a level/tag line, turning embedded newlines into CONT lines.
fn push_text(out: &mut String, level: u8, tag: &str, text: &str) {
    let mut lines = text.split('\n');
    let Some(first) = lines.next() else {
        return;
    };
    out.push_str(&format!("{level} {tag} {first}\n"));
    for line in lines {
        out.push_str(&format!("{} CONT {line}\n", level + 1));
    }
}

fn format_latitude(value: f64) -> String {
    format_hemisphere(value, 'N', 'S')
}

fn format_longitude(value: f64) -> String {
    format_hemisphere(value, 'E', 'W')
}

fn format_hemisphere(value: f64, positive: char, negative: char) -> String {
    let hemisphere = if value < 0.0 { negative } else { positive };
    format!("{hemisphere}{}", value.abs())
}

fn push_event(
    out: &mut String,
    event: &EventRow,
    facts: &FactLookup,
    citations: &CitationsByEvent<'_>,
    source_xrefs: &HashMap<&str, String>,
    participants: &HashMap<&str, Vec<(&str, &str)>>,
    write_markers: bool,
) {
    let tag = facts
        .tag_for(&event.event_type)
        .unwrap_or(&event.event_type);
    match event
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => out.push_str(&format!("1 {tag} {value}\n")),
        None => out.push_str(&format!("1 {tag}\n")),
    }
    if let Some(date) = &event.date
        && let Some(payload) = date_payload(date)
    {
        out.push_str(&format!("2 DATE {payload}\n"));
    }
    if let Some(place) = &event.place {
        out.push_str(&format!("2 PLAC {place}\n"));
        if event.latitude.is_some() || event.longitude.is_some() {
            out.push_str("3 MAP\n");
            if let Some(latitude) = event.latitude {
                out.push_str(&format!("4 LATI {}\n", format_latitude(latitude)));
            }
            if let Some(longitude) = event.longitude {
                out.push_str(&format!("4 LONG {}\n", format_longitude(longitude)));
            }
        }
    }
    if let Some(cited) = citations.get(event.id.as_str()) {
        for citation in cited {
            let Some(xref) = source_xrefs.get(citation.source_id.as_str()) else {
                continue;
            };
            out.push_str(&format!("2 SOUR {xref}\n"));
            if let Some(page) = &citation.page_or_reference {
                out.push_str(&format!("3 PAGE {page}\n"));
            }
            if let Some(quality) = &citation.quality {
                out.push_str(&format!("3 QUAY {quality}\n"));
            }
            if let Some(note) = &citation.note {
                push_text(out, 3, "NOTE", note);
            }
            if let Some(media) = &citation.media_type {
                out.push_str(&format!("3 MEDI {media}\n"));
            }
            if citation.data_date.is_some() || citation.data_text.is_some() {
                out.push_str("3 DATA\n");
                if let Some(date) = &citation.data_date {
                    out.push_str(&format!("4 DATE {date}\n"));
                }
                if let Some(text) = &citation.data_text {
                    push_text(out, 4, "TEXT", text);
                }
            }
        }
    }
    if let Some(witnesses) = participants.get(event.id.as_str()) {
        for (xref, role) in witnesses {
            out.push_str(&format!("2 _WITN {xref}\n3 _ROLE {role}\n"));
        }
    }
    if write_markers && event.private {
        out.push_str("2 _PRIV Y\n");
    }
}

/// Serializes people, families, sources, repositories, events and
/// citations into a GEDCOM 5.5.1 document.
///
/// Row order (`ORDER BY rowid`) makes the output deterministic: exporting the
/// same database twice yields identical text.
pub fn export_to_gedcom(conn: &Connection) -> Result<String, GedcomError> {
    export_to_gedcom_with_options(
        conn,
        &ExportOptions {
            include_living: true,
            include_private: true,
            write_markers: false,
            ..ExportOptions::default()
        },
    )
}

/// Serializes the database with the requested living/private filtering.
pub fn export_to_gedcom_with_options(
    conn: &Connection,
    options: &ExportOptions,
) -> Result<String, GedcomError> {
    let facts = crate::db::facts::load_fact_lookup(conn)
        .map_err(|error| GedcomError::Db(rusqlite::Error::InvalidParameterName(error)))?;
    let mut out = String::new();
    out.push_str("0 HEAD\n");
    out.push_str("1 SOUR STEMMA_FLOW\n");
    let version = env!("CARGO_PKG_VERSION");
    out.push_str(&format!("2 VERS {version}\n"));
    out.push_str("1 GEDC\n");
    out.push_str("2 VERS 5.5.1\n");
    out.push_str("2 FORM LINEAGE-LINKED\n");
    out.push_str("1 CHAR UTF-8\n");

    let mut people = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, given_name, middle_name, surname, nickname, gender, notes, private
         FROM people ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(PersonRow {
            id: row.get(0)?,
            given_name: row.get(1)?,
            middle_name: row.get(2)?,
            surname: row.get(3)?,
            nickname: row.get(4)?,
            gender: row.get(5)?,
            notes: row.get(6)?,
            private: row.get::<_, i64>(7)? != 0,
            living: false,
        })
    })?;
    for row in rows {
        people.push(row?);
    }
    let current_year = if options.current_year != 0 {
        options.current_year
    } else {
        crate::db::privacy::current_year(conn)
            .map_err(|error| GedcomError::Db(rusqlite::Error::InvalidParameterName(error)))?
    };
    for person in &mut people {
        person.living = if !options.include_living || options.write_markers {
            crate::db::privacy::is_living(conn, &person.id, current_year, options.living_age)
                .map_err(|error| GedcomError::Db(rusqlite::Error::InvalidParameterName(error)))?
        } else {
            false
        };
    }
    let visible_people: Vec<&PersonRow> = people
        .iter()
        .filter(|person| {
            (options.include_private || !person.private)
                && (options.include_living || !person.living)
        })
        .collect();
    let visible_people_ids: HashSet<&str> = visible_people
        .iter()
        .map(|person| person.id.as_str())
        .collect();

    let mut families = Vec::new();
    let mut stmt = conn.prepare("SELECT id, husband_id, wife_id FROM families ORDER BY rowid")?;
    let rows = stmt.query_map([], |row| {
        Ok(FamilyRow {
            id: row.get(0)?,
            husband_id: row.get(1)?,
            wife_id: row.get(2)?,
        })
    })?;
    for row in rows {
        families.push(row?);
    }

    let mut junction = Vec::new();
    let mut stmt =
        conn.prepare("SELECT family_id, child_id FROM family_children ORDER BY rowid")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        junction.push(row?);
    }
    let mut family_ids: HashSet<&str> = HashSet::new();
    for family in &families {
        if [family.husband_id.as_deref(), family.wife_id.as_deref()]
            .into_iter()
            .flatten()
            .any(|id| visible_people_ids.contains(id))
        {
            family_ids.insert(family.id.as_str());
        }
    }
    for (family_id, child_id) in &junction {
        if visible_people_ids.contains(child_id.as_str()) {
            family_ids.insert(family_id.as_str());
        }
    }
    let visible_families: Vec<&FamilyRow> = families
        .iter()
        .filter(|family| family_ids.contains(family.id.as_str()))
        .collect();
    let visible_family_ids: HashSet<&str> = visible_families
        .iter()
        .map(|family| family.id.as_str())
        .collect();

    let mut events = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT e.id, e.event_type, e.date, p.name, p.latitude, p.longitude,
                e.description, e.private, e.person_id, e.family_id
         FROM events e
         LEFT JOIN places p ON p.id = e.place_id
         ORDER BY e.rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(EventRow {
            id: row.get(0)?,
            event_type: row.get(1)?,
            date: row.get(2)?,
            place: row.get(3)?,
            latitude: row.get(4)?,
            longitude: row.get(5)?,
            description: row.get(6)?,
            private: row.get::<_, i64>(7)? != 0,
            person_id: row.get(8)?,
            family_id: row.get(9)?,
        })
    })?;
    for row in rows {
        events.push(row?);
    }
    let visible_event_ids: HashSet<&str> = events
        .iter()
        .filter(|event| {
            if event.private && !options.include_private {
                return false;
            }
            if let Some(person_id) = &event.person_id {
                visible_people_ids.contains(person_id.as_str())
            } else if let Some(family_id) = &event.family_id {
                visible_family_ids.contains(family_id.as_str())
            } else {
                false
            }
        })
        .map(|event| event.id.as_str())
        .collect();

    let mut repositories = Vec::new();
    let mut stmt =
        conn.prepare("SELECT id, name, address, notes FROM repositories ORDER BY rowid")?;
    let rows = stmt.query_map([], |row| {
        Ok(RepositoryRow {
            id: row.get(0)?,
            name: row.get(1)?,
            address: row.get(2)?,
            notes: row.get(3)?,
        })
    })?;
    for row in rows {
        repositories.push(row?);
    }

    let mut sources = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, title, author, publication, abbreviation, notes, data_date, data_text
         FROM sources ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SourceRow {
            id: row.get(0)?,
            title: row.get(1)?,
            author: row.get(2)?,
            publication: row.get(3)?,
            abbreviation: row.get(4)?,
            notes: row.get(5)?,
            data_date: row.get(6)?,
            data_text: row.get(7)?,
        })
    })?;
    for row in rows {
        sources.push(row?);
    }

    let mut repo_links = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT source_id, repository_id, call_number, media_type
         FROM source_repositories ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(RepoLink {
            source_id: row.get(0)?,
            repository_id: row.get(1)?,
            call_number: row.get(2)?,
            media_type: row.get(3)?,
        })
    })?;
    for row in rows {
        repo_links.push(row?);
    }

    let mut citations = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT c.source_id, c.page_or_reference, c.quality_certainty, c.notes,
             c.media_type, c.data_date, c.data_text, l.event_id
         FROM citations c
         JOIN citation_links l ON l.citation_id = c.id
         ORDER BY c.rowid, l.event_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CitationRow {
            source_id: row.get(0)?,
            page_or_reference: row.get(1)?,
            quality: row.get(2)?,
            note: row.get(3)?,
            media_type: row.get(4)?,
            data_date: row.get(5)?,
            data_text: row.get(6)?,
            event_id: row.get(7)?,
        })
    })?;
    for row in rows {
        citations.push(row?);
    }

    let mut participants = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT ep.event_id, ep.person_id, COALESCE(r.label, ep.role_key)
         FROM event_participants ep
         JOIN events e ON e.id = ep.event_id
         LEFT JOIN fact_type_roles r
             ON r.fact_type_key = e.event_type AND r.role_key = ep.role_key
         ORDER BY ep.event_id, ep.sort_order, ep.person_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ParticipantRow {
            event_id: row.get(0)?,
            person_id: row.get(1)?,
            role_label: row.get(2)?,
        })
    })?;
    for row in rows {
        participants.push(row?);
    }

    let mut person_xref: HashMap<&str, String> = HashMap::new();
    for (index, person) in visible_people.iter().enumerate() {
        person_xref.insert(&person.id, format!("@I{}@", index + 1));
    }
    let mut participants_by_event: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for participant in &participants {
        if !visible_event_ids.contains(participant.event_id.as_str()) {
            continue;
        }
        let Some(xref) = person_xref.get(participant.person_id.as_str()) else {
            continue;
        };
        participants_by_event
            .entry(participant.event_id.as_str())
            .or_default()
            .push((xref.as_str(), participant.role_label.as_str()));
    }

    let mut family_xref: HashMap<&str, String> = HashMap::new();
    for (index, family) in visible_families.iter().enumerate() {
        family_xref.insert(&family.id, format!("@F{}@", index + 1));
    }
    let mut repository_xref: HashMap<&str, String> = HashMap::new();
    for (index, repository) in repositories.iter().enumerate() {
        repository_xref.insert(&repository.id, format!("@R{}@", index + 1));
    }
    let mut source_xref: HashMap<&str, String> = HashMap::new();
    for (index, source) in sources.iter().enumerate() {
        source_xref.insert(&source.id, format!("@S{}@", index + 1));
    }

    let mut links_by_source: HashMap<&str, Vec<&RepoLink>> = HashMap::new();
    for link in &repo_links {
        links_by_source
            .entry(link.source_id.as_str())
            .or_default()
            .push(link);
    }

    let mut famc_by_child: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut children_by_family: HashMap<&str, Vec<&str>> = HashMap::new();
    for (family_id, child_id) in &junction {
        if !visible_family_ids.contains(family_id.as_str())
            || !visible_people_ids.contains(child_id.as_str())
        {
            continue;
        }
        famc_by_child.entry(child_id).or_default().push(family_id);
        children_by_family
            .entry(family_id)
            .or_default()
            .push(child_id);
    }
    let mut families_by_spouse: HashMap<&str, Vec<&str>> = HashMap::new();
    for family in &visible_families {
        if let Some(husband) = &family.husband_id {
            families_by_spouse
                .entry(husband)
                .or_default()
                .push(&family.id);
        }
        if let Some(wife) = &family.wife_id {
            families_by_spouse.entry(wife).or_default().push(&family.id);
        }
    }
    let mut events_by_person: HashMap<&str, Vec<&EventRow>> = HashMap::new();
    let mut events_by_family: HashMap<&str, Vec<&EventRow>> = HashMap::new();
    for event in &events {
        if !visible_event_ids.contains(event.id.as_str()) {
            continue;
        }
        if let Some(person_id) = &event.person_id {
            events_by_person.entry(person_id).or_default().push(event);
        } else if let Some(family_id) = &event.family_id {
            events_by_family.entry(family_id).or_default().push(event);
        }
    }
    let mut citations_by_event: CitationsByEvent<'_> = HashMap::new();
    for citation in &citations {
        if let Some(event_id) = &citation.event_id {
            citations_by_event
                .entry(event_id.as_str())
                .or_default()
                .push(citation);
        }
    }

    // Repositories and sources precede INDI/FAM so a streaming importer can
    // resolve event pointers in a single forward pass.
    for (index, repository) in repositories.iter().enumerate() {
        out.push_str(&format!("0 @R{}@ REPO\n", index + 1));
        out.push_str(&format!("1 NAME {}\n", repository.name));
        if let Some(address) = &repository.address {
            push_text(&mut out, 1, "ADDR", address);
        }
        if let Some(note) = &repository.notes {
            push_text(&mut out, 1, "NOTE", note);
        }
    }

    for (index, source) in sources.iter().enumerate() {
        out.push_str(&format!("0 @S{}@ SOUR\n", index + 1));
        out.push_str(&format!("1 TITL {}\n", source.title));
        if let Some(author) = &source.author {
            out.push_str(&format!("1 AUTH {author}\n"));
        }
        if let Some(publication) = &source.publication {
            out.push_str(&format!("1 PUBL {publication}\n"));
        }
        if let Some(abbreviation) = &source.abbreviation {
            out.push_str(&format!("1 ABBR {abbreviation}\n"));
        }
        if let Some(links) = links_by_source.get(source.id.as_str()) {
            for link in links {
                let Some(xref) = repository_xref.get(link.repository_id.as_str()) else {
                    continue;
                };
                out.push_str(&format!("1 REPO {xref}\n"));
                if let Some(call_number) = &link.call_number {
                    out.push_str(&format!("2 CALN {call_number}\n"));
                }
                if let Some(media_type) = &link.media_type {
                    out.push_str(&format!("2 MEDI {media_type}\n"));
                }
            }
        }
        if let Some(note) = &source.notes {
            push_text(&mut out, 1, "NOTE", note);
        }
        if source.data_date.is_some() || source.data_text.is_some() {
            out.push_str("1 DATA\n");
            if let Some(date) = &source.data_date {
                out.push_str(&format!("2 DATE {date}\n"));
            }
            if let Some(text) = &source.data_text {
                push_text(&mut out, 2, "TEXT", text);
            }
        }
    }

    for (index, person) in visible_people.iter().enumerate() {
        out.push_str(&format!("0 @I{}@ INDI\n", index + 1));
        let given_part = [person.given_name.as_str(), person.middle_name.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!("1 NAME {given_part} /{}/\n", person.surname));
        // Vendor name-part children keep the given name bare on re-import.
        if !person.middle_name.is_empty() || !person.nickname.is_empty() {
            out.push_str(&format!("2 GIVN {}\n", person.given_name));
            if !person.middle_name.is_empty() {
                out.push_str(&format!("2 _MIDN {}\n", person.middle_name));
            }
            if !person.nickname.is_empty() {
                out.push_str(&format!("2 _PGVN {}\n", person.nickname));
            }
            out.push_str(&format!("2 SURN {}\n", person.surname));
        }
        out.push_str(&format!("1 SEX {}\n", person.gender));
        if options.write_markers && (person.private || person.living) {
            out.push_str("1 RESN privacy\n");
        }
        if !person.notes.is_empty() {
            push_text(&mut out, 1, "NOTE", &person.notes);
        }

        if let Some(person_events) = events_by_person.get(person.id.as_str()) {
            for event in person_events {
                push_event(
                    &mut out,
                    event,
                    &facts,
                    &citations_by_event,
                    &source_xref,
                    &participants_by_event,
                    options.write_markers,
                );
            }
        }
        if let Some(families) = famc_by_child.get(person.id.as_str()) {
            for family_id in families {
                if let Some(xref) = family_xref.get(family_id) {
                    out.push_str(&format!("1 FAMC {xref}\n"));
                }
            }
        }
        if let Some(families) = families_by_spouse.get(person.id.as_str()) {
            for family_id in families {
                if let Some(xref) = family_xref.get(family_id) {
                    out.push_str(&format!("1 FAMS {xref}\n"));
                }
            }
        }
    }

    for (index, family) in visible_families.iter().enumerate() {
        out.push_str(&format!("0 @F{}@ FAM\n", index + 1));
        if let Some(husband) = &family.husband_id
            && let Some(xref) = person_xref.get(husband.as_str())
        {
            out.push_str(&format!("1 HUSB {xref}\n"));
        }
        if let Some(wife) = &family.wife_id
            && let Some(xref) = person_xref.get(wife.as_str())
        {
            out.push_str(&format!("1 WIFE {xref}\n"));
        }
        if let Some(children) = children_by_family.get(family.id.as_str()) {
            for child_id in children {
                if let Some(xref) = person_xref.get(child_id) {
                    out.push_str(&format!("1 CHIL {xref}\n"));
                }
            }
        }
        if let Some(family_events) = events_by_family.get(family.id.as_str()) {
            for event in family_events {
                push_event(
                    &mut out,
                    event,
                    &facts,
                    &citations_by_event,
                    &source_xref,
                    &participants_by_event,
                    options.write_markers,
                );
            }
        }
    }

    out.push_str("0 TRLR\n");
    Ok(out)
}
