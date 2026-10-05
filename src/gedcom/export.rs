//! Serializes the SQLite state back into GEDCOM 5.5.1.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::gedcom::GedcomError;

struct PersonRow {
    id: String,
    given_name: String,
    middle_name: String,
    surname: String,
    nickname: String,
    gender: String,
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
    person_id: Option<String>,
    family_id: Option<String>,
}

struct CitationRow {
    id: String,
    source_title: String,
    page_or_reference: Option<String>,
    notes: Option<String>,
    event_id: Option<String>,
}

/// Maps a database event type back to its GEDCOM tag.
fn gedcom_event_tag(event_type: &str) -> &str {
    match event_type {
        "BIRTH" => "BIRT",
        "DEATH" => "DEAT",
        "MARRIAGE" => "MARR",
        other => other,
    }
}

type CitationsByEvent<'a> = HashMap<&'a str, Vec<&'a CitationRow>>;

fn push_event(
    out: &mut String,
    event: &EventRow,
    tag: &str,
    citations: &CitationsByEvent<'_>,
    source_xrefs: &HashMap<&str, String>,
) {
    out.push_str(&format!("1 {tag}\n"));
    if let Some(date) = &event.date {
        out.push_str(&format!("2 DATE {date}\n"));
    }
    if let Some(place) = &event.place {
        out.push_str(&format!("2 PLAC {place}\n"));
    }
    if let Some(cited) = citations.get(event.id.as_str()) {
        for citation in cited {
            let Some(xref) = source_xrefs.get(citation.id.as_str()) else {
                continue;
            };
            out.push_str(&format!("2 SOUR {xref}\n"));
            if let Some(page) = &citation.page_or_reference {
                out.push_str(&format!("3 PAGE {page}\n"));
            }
        }
    }
}

/// Serializes `people`, `families`, `family_children`, `events` and
/// `citations` into a GEDCOM 5.5.1 document.
///
/// Row order (`ORDER BY rowid`) makes the output deterministic: exporting the
/// same database twice yields identical text.
pub fn export_to_gedcom(conn: &Connection) -> Result<String, GedcomError> {
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
        "SELECT id, given_name, middle_name, surname, nickname, gender
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
        })
    })?;
    for row in rows {
        people.push(row?);
    }

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

    let mut events = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, event_type, date, place, person_id, family_id
         FROM events ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(EventRow {
            id: row.get(0)?,
            event_type: row.get(1)?,
            date: row.get(2)?,
            place: row.get(3)?,
            person_id: row.get(4)?,
            family_id: row.get(5)?,
        })
    })?;
    for row in rows {
        events.push(row?);
    }

    let mut citations = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, source_title, page_or_reference, notes, event_id
         FROM citations ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CitationRow {
            id: row.get(0)?,
            source_title: row.get(1)?,
            page_or_reference: row.get(2)?,
            notes: row.get(3)?,
            event_id: row.get(4)?,
        })
    })?;
    for row in rows {
        citations.push(row?);
    }

    let mut person_xref: HashMap<&str, String> = HashMap::new();
    for (index, person) in people.iter().enumerate() {
        person_xref.insert(&person.id, format!("@I{}@", index + 1));
    }
    let mut family_xref: HashMap<&str, String> = HashMap::new();
    for (index, family) in families.iter().enumerate() {
        family_xref.insert(&family.id, format!("@F{}@", index + 1));
    }
    let mut source_xref: HashMap<&str, String> = HashMap::new();
    for (index, citation) in citations.iter().enumerate() {
        source_xref.insert(&citation.id, format!("@S{}@", index + 1));
    }

    let mut famc_by_child: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut children_by_family: HashMap<&str, Vec<&str>> = HashMap::new();
    for (family_id, child_id) in &junction {
        famc_by_child.entry(child_id).or_default().push(family_id);
        children_by_family
            .entry(family_id)
            .or_default()
            .push(child_id);
    }
    let mut families_by_spouse: HashMap<&str, Vec<&str>> = HashMap::new();
    for family in &families {
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

    // Source records precede INDI/FAM so a streaming importer can resolve
    // event pointers in a single forward pass.
    for (index, citation) in citations.iter().enumerate() {
        out.push_str(&format!("0 @S{}@ SOUR\n", index + 1));
        out.push_str(&format!("1 TITL {}\n", citation.source_title));
        if let Some(note) = &citation.notes {
            out.push_str(&format!("1 NOTE {note}\n"));
        }
    }

    for (index, person) in people.iter().enumerate() {
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

        if let Some(person_events) = events_by_person.get(person.id.as_str()) {
            for event in person_events {
                push_event(
                    &mut out,
                    event,
                    gedcom_event_tag(&event.event_type),
                    &citations_by_event,
                    &source_xref,
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

    for (index, family) in families.iter().enumerate() {
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
                    gedcom_event_tag(&event.event_type),
                    &citations_by_event,
                    &source_xref,
                );
            }
        }
    }

    out.push_str("0 TRLR\n");
    Ok(out)
}
