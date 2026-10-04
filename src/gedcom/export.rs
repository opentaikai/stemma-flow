//! Serializes the SQLite state back into GEDCOM 5.5.1.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::gedcom::GedcomError;

struct PersonRow {
    id: String,
    given_name: String,
    surname: String,
    gender: String,
}

struct FamilyRow {
    id: String,
    husband_id: Option<String>,
    wife_id: Option<String>,
}

struct EventRow {
    event_type: String,
    date: Option<String>,
    place: Option<String>,
    person_id: Option<String>,
    family_id: Option<String>,
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

fn push_event(out: &mut String, event: &EventRow, tag: &str) {
    out.push_str(&format!("1 {tag}\n"));
    if let Some(date) = &event.date {
        out.push_str(&format!("2 DATE {date}\n"));
    }
    if let Some(place) = &event.place {
        out.push_str(&format!("2 PLAC {place}\n"));
    }
}

/// Serializes `people`, `families`, `family_children` and `events` into a
/// GEDCOM 5.5.1 document.
///
/// Row order (`ORDER BY rowid`) makes the output deterministic: exporting the
/// same database twice yields identical text.
pub fn export_to_gedcom(conn: &Connection) -> Result<String, GedcomError> {
    let mut out = String::new();
    out.push_str("0 HEAD\n");
    out.push_str("1 SOUR STEMMA_FLOW\n");
    out.push_str("1 GEDC\n");
    out.push_str("2 VERS 5.5.1\n");
    out.push_str("1 CHAR UTF-8\n");

    let mut people = Vec::new();
    let mut stmt =
        conn.prepare("SELECT id, given_name, surname, gender FROM people ORDER BY rowid")?;
    let rows = stmt.query_map([], |row| {
        Ok(PersonRow {
            id: row.get(0)?,
            given_name: row.get(1)?,
            surname: row.get(2)?,
            gender: row.get(3)?,
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
        "SELECT event_type, date, place, person_id, family_id FROM events ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(EventRow {
            event_type: row.get(0)?,
            date: row.get(1)?,
            place: row.get(2)?,
            person_id: row.get(3)?,
            family_id: row.get(4)?,
        })
    })?;
    for row in rows {
        events.push(row?);
    }

    let mut person_xref: HashMap<&str, String> = HashMap::new();
    for (index, person) in people.iter().enumerate() {
        person_xref.insert(&person.id, format!("@I{}@", index + 1));
    }
    let mut family_xref: HashMap<&str, String> = HashMap::new();
    for (index, family) in families.iter().enumerate() {
        family_xref.insert(&family.id, format!("@F{}@", index + 1));
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

    for (index, person) in people.iter().enumerate() {
        out.push_str(&format!("0 @I{}@ INDI\n", index + 1));
        out.push_str(&format!(
            "1 NAME {} /{}/\n",
            person.given_name, person.surname
        ));
        out.push_str(&format!("1 SEX {}\n", person.gender));

        if let Some(person_events) = events_by_person.get(person.id.as_str()) {
            for event in person_events {
                push_event(&mut out, event, gedcom_event_tag(&event.event_type));
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
                push_event(&mut out, event, gedcom_event_tag(&event.event_type));
            }
        }
    }

    out.push_str("0 TRLR\n");
    Ok(out)
}
