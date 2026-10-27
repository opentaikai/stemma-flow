//! Fact catalogue persistence and read-only person facts.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};

use crate::db::participants::EventParticipant;
use crate::model::fact::{FactKind, FactScope, builtin_facts};

/// A stored fact type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactTypeRow {
    pub key: String,
    pub label: String,
    pub category: Option<String>,
    pub gedcom_tag: Option<String>,
    pub scope: FactScope,
    pub kind: FactKind,
    pub is_builtin: bool,
}

/// One user-defined role on a fact type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactTypeRole {
    pub role_key: String,
    pub label: String,
    pub sort_order: i64,
}

/// A fact type row with catalogue usage counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactTypeSummary {
    pub key: String,
    pub label: String,
    pub category: Option<String>,
    pub gedcom_tag: Option<String>,
    pub scope: FactScope,
    pub kind: FactKind,
    pub is_builtin: bool,
    pub role_count: i64,
    pub event_count: i64,
}

/// A fact type with its editable role list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactTypeRecord {
    pub key: String,
    pub label: String,
    pub category: Option<String>,
    pub gedcom_tag: Option<String>,
    pub scope: FactScope,
    pub kind: FactKind,
    pub is_builtin: bool,
    pub roles: Vec<FactTypeRole>,
}

/// A role in an editable fact type draft.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactRoleDraft {
    pub key: Option<String>,
    pub label: String,
}

/// An editable fact type draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactTypeDraft {
    pub key: Option<String>,
    pub label: String,
    pub category: String,
    pub gedcom_tag: String,
    pub scope: FactScope,
    pub kind: FactKind,
    pub roles: Vec<FactRoleDraft>,
}

impl Default for FactTypeDraft {
    fn default() -> Self {
        Self {
            key: None,
            label: String::new(),
            category: String::new(),
            gedcom_tag: String::new(),
            scope: FactScope::Person,
            kind: FactKind::Event,
            roles: Vec::new(),
        }
    }
}

/// A read-only fact shown on a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonFact {
    pub event_id: String,
    pub event_type: String,
    pub label: String,
    pub date: Option<String>,
    pub place: Option<String>,
    pub value: Option<String>,
    /// Set when the person is a participant rather than the event principal.
    pub role: Option<String>,
    /// Principal person or couple for a shared event.
    pub principal: Option<String>,
    pub participants: Vec<EventParticipant>,
}

/// The stored catalogue used by GEDCOM import and export.
#[derive(Debug, Clone, Default)]
pub struct FactLookup {
    rows: Vec<FactTypeRow>,
}

impl FactLookup {
    pub fn by_tag(&self, tag: &str, scope: FactScope) -> Option<&FactTypeRow> {
        self.rows.iter().find(|fact| {
            fact.gedcom_tag.as_deref() == Some(tag)
                && match scope {
                    FactScope::Person => fact.scope != FactScope::Family,
                    FactScope::Family => fact.scope != FactScope::Person,
                    FactScope::Both => true,
                }
        })
    }

    pub fn tag_for(&self, key: &str) -> Option<&str> {
        self.rows
            .iter()
            .find(|fact| fact.key == key)
            .and_then(|fact| fact.gedcom_tag.as_deref())
    }
}

/// Loads a DB-backed lookup for GEDCOM.
pub fn load_fact_lookup(conn: &Connection) -> Result<FactLookup, String> {
    Ok(FactLookup {
        rows: load_fact_types(conn)?,
    })
}

/// Inserts or refreshes the built-in catalogue without touching custom rows.
pub fn seed_fact_types(conn: &Connection) -> rusqlite::Result<()> {
    for fact in builtin_facts() {
        conn.execute(
            "INSERT OR IGNORE INTO fact_types
             (key, label, category, gedcom_tag, scope, kind, is_builtin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            params![
                fact.key,
                fact.label,
                fact.category,
                fact.gedcom_tag,
                fact.scope.as_str(),
                fact.kind.as_str()
            ],
        )?;
        conn.execute(
            "UPDATE fact_types
             SET label = ?2, category = ?3, gedcom_tag = ?4, scope = ?5, kind = ?6
             WHERE key = ?1 AND is_builtin = 1",
            params![
                fact.key,
                fact.label,
                fact.category,
                fact.gedcom_tag,
                fact.scope.as_str(),
                fact.kind.as_str()
            ],
        )?;
    }
    Ok(())
}

/// Loads the fact catalogue ordered by category and label.
pub fn load_fact_types(conn: &Connection) -> Result<Vec<FactTypeRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT key, label, category, gedcom_tag, scope, kind, is_builtin
             FROM fact_types
             ORDER BY category COLLATE NOCASE, label COLLATE NOCASE, key",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let scope: String = row.get(4)?;
            let kind: String = row.get(5)?;
            Ok(FactTypeRow {
                key: row.get(0)?,
                label: row.get(1)?,
                category: row.get(2)?,
                gedcom_tag: row.get(3)?,
                scope: parse_scope(&scope),
                kind: parse_kind(&kind),
                is_builtin: row.get::<_, i64>(6)? != 0,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn parse_scope(value: &str) -> FactScope {
    match value {
        "family" => FactScope::Family,
        "both" => FactScope::Both,
        _ => FactScope::Person,
    }
}

fn parse_kind(value: &str) -> FactKind {
    match value {
        "attribute" => FactKind::Attribute,
        _ => FactKind::Event,
    }
}

fn clean_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// A stable key suggestion for a new custom fact type.
pub fn suggested_key(label: &str) -> String {
    let slug = slug(label);
    if slug.is_empty() {
        "CUSTOM".to_string()
    } else {
        slug
    }
}

fn unique_key(conn: &Connection, label: &str) -> Result<String, String> {
    let base = suggested_key(label);
    let mut candidate = base.clone();
    let mut suffix = 2;
    loop {
        let exists = conn
            .query_row(
                "SELECT 1 FROM fact_types WHERE key = ?1",
                params![candidate],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if exists.is_none() {
            return Ok(candidate);
        }
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
}

fn validate_tag(tag: &str) -> Result<String, String> {
    let tag = clean_text(tag).ok_or_else(|| "GEDCOM tag is required".to_string())?;
    if tag.len() > 31 {
        return Err("GEDCOM tag must be at most 31 characters".to_string());
    }
    let mut chars = tag.chars();
    let Some(first) = chars.next() else {
        return Err("GEDCOM tag is required".to_string());
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err("GEDCOM tag must start with a letter or underscore".to_string());
    }
    if !tag
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err("GEDCOM tag must use uppercase letters, digits and underscores".to_string());
    }
    Ok(tag)
}

fn save_roles_tx(
    conn: &Connection,
    fact_type_key: &str,
    roles: &[FactRoleDraft],
) -> Result<(), String> {
    let mut used: HashSet<String> = HashSet::new();
    let mut final_roles: Vec<(String, String)> = Vec::with_capacity(roles.len());
    for role in roles {
        let label = clean_text(&role.label).ok_or_else(|| "Role label is required".to_string())?;
        let role_key = match role
            .key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            Some(key) => {
                if !used.insert(key.to_string()) {
                    return Err(format!("role key {key} is duplicated"));
                }
                key.to_string()
            }
            None => {
                let base = {
                    let slug = slug(&label);
                    if slug.is_empty() {
                        "ROLE".to_string()
                    } else {
                        slug
                    }
                };
                let mut candidate = base.clone();
                let mut suffix = 2;
                while used.contains(&candidate) {
                    candidate = format!("{base}_{suffix}");
                    suffix += 1;
                }
                used.insert(candidate.clone());
                candidate
            }
        };
        final_roles.push((role_key, label));
    }

    let final_keys: HashSet<&str> = final_roles.iter().map(|(key, _)| key.as_str()).collect();
    let mut stmt = conn
        .prepare("SELECT role_key FROM fact_type_roles WHERE fact_type_key = ?1")
        .map_err(|error| error.to_string())?;
    let existing = stmt
        .query_map(params![fact_type_key], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    drop(stmt);
    for key in existing {
        if final_keys.contains(key.as_str()) {
            continue;
        }
        let used: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM event_participants ep
                 JOIN events e ON e.id = ep.event_id
                 WHERE e.event_type = ?1 AND ep.role_key = ?2",
                params![fact_type_key, key],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if used > 0 {
            return Err(format!("role {key} is used by {used} participants"));
        }
    }

    conn.execute(
        "DELETE FROM fact_type_roles WHERE fact_type_key = ?1",
        params![fact_type_key],
    )
    .map_err(|error| error.to_string())?;
    for (index, (role_key, label)) in final_roles.iter().enumerate() {
        conn.execute(
            "INSERT INTO fact_type_roles (fact_type_key, role_key, label, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
            params![fact_type_key, role_key, label, index as i64],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Lists the fact catalogue with role and event counts.
pub fn list_fact_type_summaries(conn: &Connection) -> Result<Vec<FactTypeSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT f.key, f.label, f.category, f.gedcom_tag, f.scope, f.kind, f.is_builtin,
                    (SELECT COUNT(*) FROM fact_type_roles r WHERE r.fact_type_key = f.key),
                    (SELECT COUNT(*) FROM events e WHERE e.event_type = f.key)
             FROM fact_types f
             ORDER BY f.label COLLATE NOCASE, f.key",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let scope: String = row.get(4)?;
            let kind: String = row.get(5)?;
            Ok(FactTypeSummary {
                key: row.get(0)?,
                label: row.get(1)?,
                category: row.get(2)?,
                gedcom_tag: row.get(3)?,
                scope: parse_scope(&scope),
                kind: parse_kind(&kind),
                is_builtin: row.get::<_, i64>(6)? != 0,
                role_count: row.get(7)?,
                event_count: row.get(8)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads one fact type and its roles.
pub fn load_fact_type(conn: &Connection, key: &str) -> Result<Option<FactTypeRecord>, String> {
    let record = conn
        .query_row(
            "SELECT key, label, category, gedcom_tag, scope, kind, is_builtin
             FROM fact_types WHERE key = ?1",
            params![key],
            |row| {
                let scope: String = row.get(4)?;
                let kind: String = row.get(5)?;
                Ok(FactTypeRecord {
                    key: row.get(0)?,
                    label: row.get(1)?,
                    category: row.get(2)?,
                    gedcom_tag: row.get(3)?,
                    scope: parse_scope(&scope),
                    kind: parse_kind(&kind),
                    is_builtin: row.get::<_, i64>(6)? != 0,
                    roles: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(mut record) = record else {
        return Ok(None);
    };
    let mut stmt = conn
        .prepare(
            "SELECT role_key, label, sort_order FROM fact_type_roles
             WHERE fact_type_key = ?1 ORDER BY sort_order, role_key",
        )
        .map_err(|error| error.to_string())?;
    let roles = stmt
        .query_map(params![key], |row| {
            Ok(FactTypeRole {
                role_key: row.get(0)?,
                label: row.get(1)?,
                sort_order: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    record.roles = roles;
    Ok(Some(record))
}

/// Inserts or updates a custom fact type and its roles.
pub fn save_fact_type(conn: &mut Connection, draft: &FactTypeDraft) -> Result<String, String> {
    let label = clean_text(&draft.label).ok_or_else(|| "Fact label is required".to_string())?;
    let category = clean_text(&draft.category);
    let tag = validate_tag(&draft.gedcom_tag)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;

    let key = if let Some(key) = draft.key.as_deref() {
        let builtin: Option<i64> = tx
            .query_row(
                "SELECT is_builtin FROM fact_types WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        match builtin {
            Some(1) => return Err("Built-in fact types cannot be edited".to_string()),
            Some(_) => key.to_string(),
            None => return Err(format!("no fact type with key {key}")),
        }
    } else {
        unique_key(&tx, &label)?
    };

    let duplicate: Option<String> = tx
        .query_row(
            "SELECT key FROM fact_types WHERE gedcom_tag = ?1 AND key <> ?2",
            params![tag, key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(other) = duplicate {
        return Err(format!("GEDCOM tag {tag} is already used by {other}"));
    }

    let updated = tx
        .execute(
            "UPDATE fact_types
             SET label = ?1, category = ?2, gedcom_tag = ?3, scope = ?4, kind = ?5
             WHERE key = ?6 AND is_builtin = 0",
            params![
                label,
                category,
                tag,
                draft.scope.as_str(),
                draft.kind.as_str(),
                key
            ],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO fact_types
             (key, label, category, gedcom_tag, scope, kind, is_builtin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![
                key,
                label,
                category,
                tag,
                draft.scope.as_str(),
                draft.kind.as_str()
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    save_roles_tx(&tx, &key, &draft.roles)?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(key)
}

/// Replaces the roles of any fact type, including built-ins.
pub fn save_fact_roles(
    conn: &mut Connection,
    key: &str,
    roles: &[FactRoleDraft],
) -> Result<(), String> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM fact_types WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if exists.is_none() {
        return Err(format!("no fact type with key {key}"));
    }
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    save_roles_tx(&tx, key, roles)?;
    tx.commit().map_err(|error| error.to_string())
}

/// Deletes a custom fact type when no events use it.
pub fn delete_fact_type(conn: &mut Connection, key: &str) -> Result<(), String> {
    let builtin: Option<i64> = conn
        .query_row(
            "SELECT is_builtin FROM fact_types WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match builtin {
        Some(1) => return Err("Built-in fact types cannot be deleted".to_string()),
        Some(_) => {}
        None => return Err(format!("no fact type with key {key}")),
    }
    let events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event_type = ?1",
            params![key],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if events > 0 {
        return Err(format!("{events} events still use this fact type"));
    }
    conn.execute("DELETE FROM fact_types WHERE key = ?1", params![key])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Loads the non-vital facts for every person, grouped by person id.
pub fn load_person_facts(conn: &Connection) -> Result<HashMap<String, Vec<PersonFact>>, String> {
    let participants = load_participants_map(conn)?;
    let mut facts: HashMap<String, Vec<PersonFact>> = HashMap::new();

    let mut stmt = conn
        .prepare(
            "SELECT e.person_id, e.id, e.event_type,
                    COALESCE(f.label, e.event_type), e.date, p.name, e.description
             FROM events e
             LEFT JOIN fact_types f ON f.key = e.event_type
             LEFT JOIN places p ON p.id = e.place_id
             WHERE e.person_id IS NOT NULL
               AND (e.event_type NOT IN ('BIRTH', 'DEATH')
                    OR EXISTS (SELECT 1 FROM event_participants ep
                               WHERE ep.event_id = e.id))
             ORDER BY e.person_id, e.event_type, e.date, e.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                PersonFact {
                    event_id: row.get(1)?,
                    event_type: row.get(2)?,
                    label: row.get(3)?,
                    date: row.get(4)?,
                    place: row.get(5)?,
                    value: row.get(6)?,
                    role: None,
                    principal: None,
                    participants: Vec::new(),
                },
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (person_id, mut fact) = row.map_err(|error| error.to_string())?;
        if let Some(event_participants) = participants.get(&fact.event_id) {
            fact.participants = event_participants.clone();
        }
        facts.entry(person_id).or_default().push(fact);
    }

    let mut stmt = conn
        .prepare(
            "SELECT ep.person_id, e.id, e.event_type,
                    COALESCE(f.label, e.event_type), e.date, pl.name, e.description,
                    COALESCE(r.label, ep.role_key),
                    e.person_id, person.given_name, person.middle_name, person.surname,
                    e.family_id,
                    fam.husband_id, h.given_name, h.middle_name, h.surname,
                    fam.wife_id, w.given_name, w.middle_name, w.surname
             FROM event_participants ep
             JOIN events e ON e.id = ep.event_id
             LEFT JOIN fact_types f ON f.key = e.event_type
             LEFT JOIN places pl ON pl.id = e.place_id
             LEFT JOIN fact_type_roles r
                 ON r.fact_type_key = e.event_type AND r.role_key = ep.role_key
             LEFT JOIN people person ON person.id = e.person_id
             LEFT JOIN families fam ON fam.id = e.family_id
             LEFT JOIN people h ON h.id = fam.husband_id
             LEFT JOIN people w ON w.id = fam.wife_id
             WHERE ep.person_id IS NOT NULL
               AND (e.person_id IS NULL OR e.person_id <> ep.person_id)
             ORDER BY ep.person_id, e.event_type, e.date, e.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let event_person = display_name(row.get(9)?, row.get(10)?, row.get(11)?);
            let husband = display_name(row.get(14)?, row.get(15)?, row.get(16)?);
            let wife = display_name(row.get(18)?, row.get(19)?, row.get(20)?);
            let principal = if event_person.is_some() {
                event_person
            } else {
                match (husband.as_deref(), wife.as_deref()) {
                    (Some(husband), Some(wife)) => Some(format!("{husband} & {wife}")),
                    (Some(name), None) | (None, Some(name)) => Some(name.to_string()),
                    (None, None) => Some("Family".to_string()),
                }
            };
            Ok((
                row.get::<_, String>(0)?,
                PersonFact {
                    event_id: row.get(1)?,
                    event_type: row.get(2)?,
                    label: row.get(3)?,
                    date: row.get(4)?,
                    place: row.get(5)?,
                    value: row.get(6)?,
                    role: row.get(7)?,
                    principal,
                    participants: Vec::new(),
                },
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (person_id, mut fact) = row.map_err(|error| error.to_string())?;
        if let Some(event_participants) = participants.get(&fact.event_id) {
            fact.participants = event_participants.clone();
        }
        facts.entry(person_id).or_default().push(fact);
    }

    for list in facts.values_mut() {
        list.sort_by(|left, right| {
            left.event_type
                .cmp(&right.event_type)
                .then_with(|| left.date.cmp(&right.date))
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
    }
    Ok(facts)
}

fn load_participants_map(
    conn: &Connection,
) -> Result<HashMap<String, Vec<EventParticipant>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT ep.event_id, ep.person_id, p.given_name, p.middle_name, p.surname,
                    ep.role_key, COALESCE(r.label, ep.role_key), ep.sort_order
             FROM event_participants ep
             JOIN events e ON e.id = ep.event_id
             JOIN people p ON p.id = ep.person_id
             LEFT JOIN fact_type_roles r
                 ON r.fact_type_key = e.event_type AND r.role_key = ep.role_key
             ORDER BY ep.event_id, ep.sort_order, p.surname COLLATE NOCASE,
                      p.given_name COLLATE NOCASE, ep.person_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                EventParticipant {
                    person_id: row.get(1)?,
                    person_name: display_name(row.get(2)?, row.get(3)?, row.get(4)?)
                        .unwrap_or_else(|| "(unnamed)".to_string()),
                    role_key: row.get(5)?,
                    role_label: row.get(6)?,
                    sort_order: row.get(7)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut participants: HashMap<String, Vec<EventParticipant>> = HashMap::new();
    for row in rows {
        let (event_id, participant) = row.map_err(|error| error.to_string())?;
        participants.entry(event_id).or_default().push(participant);
    }
    Ok(participants)
}

fn display_name(
    given: Option<String>,
    middle: Option<String>,
    surname: Option<String>,
) -> Option<String> {
    let parts = [given, middle, surname]
        .into_iter()
        .flatten()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma applies");
        init_db(&conn).expect("schema initialises");
        conn
    }

    #[test]
    fn seeds_the_built_in_catalogue() {
        let conn = connection();
        let kinds = load_fact_types(&conn).expect("fact types load");
        assert_eq!(kinds.len(), builtin_facts().len());
        assert!(kinds.iter().all(|fact| fact.is_builtin));
        let birth = kinds
            .iter()
            .find(|fact| fact.key == "BIRTH")
            .expect("birth");
        assert_eq!(birth.scope, FactScope::Person);
        assert_eq!(birth.kind, FactKind::Event);
        assert_eq!(birth.gedcom_tag.as_deref(), Some("BIRT"));
    }

    #[test]
    fn seeding_is_idempotent_and_custom_rows_survive() {
        let conn = connection();
        conn.execute(
            "INSERT INTO fact_types (key, label, category, gedcom_tag, scope, kind, is_builtin)
             VALUES ('CUSTOM', 'Custom', 'Other', '_CUST', 'person', 'event', 0)",
            [],
        )
        .expect("custom fact insert");
        let first = load_fact_types(&conn).expect("load").len();
        seed_fact_types(&conn).expect("reseed");
        let second = load_fact_types(&conn).expect("load").len();
        assert_eq!(first, second);
        assert_eq!(second, builtin_facts().len() + 1);
    }

    #[test]
    fn creates_and_updates_custom_fact_types() {
        let mut conn = connection();
        let mut draft = FactTypeDraft {
            label: "Heirloom".to_string(),
            category: "Other".to_string(),
            gedcom_tag: "_HEIR".to_string(),
            scope: FactScope::Person,
            kind: FactKind::Attribute,
            roles: vec![FactRoleDraft {
                key: None,
                label: "Owner".to_string(),
            }],
            ..FactTypeDraft::default()
        };
        let key = save_fact_type(&mut conn, &draft).expect("custom fact saves");
        assert_eq!(key, "HEIRLOOM");
        let loaded = load_fact_type(&conn, &key)
            .expect("load")
            .expect("fact exists");
        assert!(!loaded.is_builtin);
        assert_eq!(loaded.gedcom_tag.as_deref(), Some("_HEIR"));
        assert_eq!(loaded.scope, FactScope::Person);
        assert_eq!(loaded.kind, FactKind::Attribute);
        assert_eq!(loaded.roles.len(), 1);
        assert_eq!(loaded.roles[0].role_key, "OWNER");

        let owner_key = loaded.roles[0].role_key.clone();
        draft.key = Some(key.clone());
        draft.label = "Family Heirloom".to_string();
        draft.gedcom_tag = "_HEIRLOOM".to_string();
        draft.roles = vec![
            FactRoleDraft {
                key: Some(owner_key),
                label: "Owner".to_string(),
            },
            FactRoleDraft {
                key: None,
                label: "Keeper".to_string(),
            },
        ];
        save_fact_type(&mut conn, &draft).expect("custom fact updates");

        let loaded = load_fact_type(&conn, &key)
            .expect("load")
            .expect("fact exists");
        assert_eq!(loaded.label, "Family Heirloom");
        assert_eq!(loaded.gedcom_tag.as_deref(), Some("_HEIRLOOM"));
        assert_eq!(loaded.roles.len(), 2);
        assert_eq!(loaded.roles[1].role_key, "KEEPER");
        let summary = list_fact_type_summaries(&conn)
            .expect("summaries")
            .into_iter()
            .find(|fact| fact.key == key)
            .expect("custom summary");
        assert_eq!(summary.role_count, 2);
        assert_eq!(summary.event_count, 0);
    }

    #[test]
    fn built_in_metadata_is_locked_but_roles_are_editable() {
        let mut conn = connection();
        let draft = FactTypeDraft {
            key: Some("BIRTH".to_string()),
            label: "Birth".to_string(),
            gedcom_tag: "BIRT".to_string(),
            ..FactTypeDraft::default()
        };
        assert!(save_fact_type(&mut conn, &draft).is_err());
        assert!(delete_fact_type(&mut conn, "BIRTH").is_err());

        save_fact_roles(
            &mut conn,
            "BIRTH",
            &[FactRoleDraft {
                key: None,
                label: "Witness".to_string(),
            }],
        )
        .expect("built-in roles save");
        let loaded = load_fact_type(&conn, "BIRTH")
            .expect("load")
            .expect("birth exists");
        assert_eq!(loaded.roles.len(), 1);
        assert_eq!(loaded.roles[0].role_key, "WITNESS");
    }

    #[test]
    fn rejects_bad_custom_fields_and_duplicate_tags() {
        let mut conn = connection();
        save_fact_type(
            &mut conn,
            &FactTypeDraft {
                label: "First".to_string(),
                gedcom_tag: "_ONE".to_string(),
                ..FactTypeDraft::default()
            },
        )
        .expect("first custom fact saves");
        assert!(
            save_fact_type(
                &mut conn,
                &FactTypeDraft {
                    label: "Second".to_string(),
                    gedcom_tag: "_ONE".to_string(),
                    ..FactTypeDraft::default()
                }
            )
            .is_err(),
            "duplicate tags are rejected"
        );
        assert!(
            save_fact_type(
                &mut conn,
                &FactTypeDraft {
                    label: "Bad tag".to_string(),
                    gedcom_tag: "bad".to_string(),
                    ..FactTypeDraft::default()
                }
            )
            .is_err()
        );
        assert!(
            save_fact_type(
                &mut conn,
                &FactTypeDraft {
                    label: "  ".to_string(),
                    gedcom_tag: "_TWO".to_string(),
                    ..FactTypeDraft::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn deleting_custom_types_requires_no_events() {
        let mut conn = connection();
        let key = save_fact_type(
            &mut conn,
            &FactTypeDraft {
                label: "Heirloom".to_string(),
                gedcom_tag: "_HEIR".to_string(),
                roles: vec![FactRoleDraft {
                    key: None,
                    label: "Owner".to_string(),
                }],
                ..FactTypeDraft::default()
            },
        )
        .expect("custom fact saves");
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person insert");
        conn.execute(
            "INSERT INTO events (id, event_type, person_id) VALUES ('e1', ?1, 'p1')",
            params![key],
        )
        .expect("event insert");
        let error = delete_fact_type(&mut conn, &key).expect_err("used type is blocked");
        assert!(error.contains("1 events"), "{error}");
        conn.execute("DELETE FROM events", [])
            .expect("event delete");
        delete_fact_type(&mut conn, &key).expect("custom fact deletes");
        let roles: i64 = conn
            .query_row("SELECT COUNT(*) FROM fact_type_roles", [], |row| row.get(0))
            .expect("role count");
        assert_eq!(roles, 0);
    }

    #[test]
    fn lookup_uses_custom_tags_and_keys() {
        let mut conn = connection();
        save_fact_type(
            &mut conn,
            &FactTypeDraft {
                label: "Heirloom".to_string(),
                gedcom_tag: "_HEIR".to_string(),
                ..FactTypeDraft::default()
            },
        )
        .expect("custom fact saves");
        let lookup = load_fact_lookup(&conn).expect("lookup");
        let fact = lookup
            .by_tag("_HEIR", FactScope::Person)
            .expect("custom tag");
        assert_eq!(fact.key, "HEIRLOOM");
        assert_eq!(lookup.tag_for("HEIRLOOM"), Some("_HEIR"));
        assert_eq!(lookup.tag_for("BIRTH"), Some("BIRT"));
        assert!(lookup.by_tag("_HEIR", FactScope::Family).is_none());
    }

    #[test]
    fn blocks_removing_roles_used_by_participants() {
        use crate::db::participants::{ParticipantDraft, save_event_participants};

        let mut conn = connection();
        save_fact_roles(
            &mut conn,
            "BIRTH",
            &[FactRoleDraft {
                key: None,
                label: "Witness".to_string(),
            }],
        )
        .expect("role saves");
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender) VALUES
                ('p1', 'Ada', 'King', 'F'),
                ('p2', 'Bo', 'King', 'M')",
            [],
        )
        .expect("people insert");
        conn.execute(
            "INSERT INTO events (id, event_type, person_id) VALUES ('e1', 'BIRTH', 'p1')",
            [],
        )
        .expect("event insert");
        save_event_participants(
            &mut conn,
            "e1",
            &[ParticipantDraft {
                person_id: "p2".to_string(),
                role_key: "WITNESS".to_string(),
            }],
        )
        .expect("participant saves");

        let error = save_fact_roles(&mut conn, "BIRTH", &[]).expect_err("used role blocked");
        assert!(error.contains("1 participants"), "{error}");
        save_fact_roles(
            &mut conn,
            "BIRTH",
            &[FactRoleDraft {
                key: Some("WITNESS".to_string()),
                label: "Witness".to_string(),
            }],
        )
        .expect("retaining the role works");
    }

    #[test]
    fn loads_shared_and_principal_facts() {
        use crate::db::participants::{ParticipantDraft, save_event_participants};

        let mut conn = connection();
        save_fact_roles(
            &mut conn,
            "BIRTH",
            &[FactRoleDraft {
                key: None,
                label: "Godparent".to_string(),
            }],
        )
        .expect("role saves");
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender) VALUES
                ('p1', 'Ada', 'King', 'F'),
                ('p2', 'Bo', 'King', 'M')",
            [],
        )
        .expect("people insert");
        conn.execute(
            "INSERT INTO events (id, event_type, date, person_id) VALUES
                ('e1', 'BIRTH', '1850', 'p1')
        ",
            [],
        )
        .expect("event insert");
        conn.execute(
            "INSERT INTO events (id, event_type, date, person_id) VALUES
                ('e2', 'CENSUS', '1860', 'p1')",
            [],
        )
        .expect("census insert");
        save_event_participants(
            &mut conn,
            "e1",
            &[ParticipantDraft {
                person_id: "p2".to_string(),
                role_key: "GODPARENT".to_string(),
            }],
        )
        .expect("participant saves");

        let facts = load_person_facts(&conn).expect("facts");
        let ada = facts.get("p1").expect("ada facts");
        let birth = ada
            .iter()
            .find(|fact| fact.event_id == "e1")
            .expect("birth fact");
        assert_eq!(birth.participants.len(), 1);
        assert_eq!(birth.participants[0].person_name, "Bo King");
        let census = ada
            .iter()
            .find(|fact| fact.event_id == "e2")
            .expect("census fact");
        assert!(census.participants.is_empty());

        let bo = facts.get("p2").expect("bo shared facts");
        assert_eq!(bo.len(), 1);
        assert_eq!(bo[0].event_type, "BIRTH");
        assert_eq!(bo[0].role.as_deref(), Some("Godparent"));
        assert_eq!(bo[0].principal.as_deref(), Some("Ada King"));
    }

    #[test]
    fn loads_person_facts_without_vitals() {
        let conn = connection();
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person insert");
        conn.execute_batch(
            "INSERT INTO places (id, name) VALUES ('pl1', 'Uppsala');
             INSERT INTO events (id, event_type, date, place_id, person_id, description) VALUES
                ('e1', 'BIRTH', '1850', 'pl1', 'p1', NULL),
                ('e2', 'OCCUPATION', '1870', 'pl1', 'p1', 'Teacher'),
                ('e3', 'CENSUS', '1860', NULL, 'p1', NULL);",
        )
        .expect("events insert");

        let facts = load_person_facts(&conn).expect("facts load");
        let person = facts.get("p1").expect("person facts");
        assert_eq!(person.len(), 2, "birth is excluded");
        assert_eq!(person[0].event_type, "CENSUS");
        assert_eq!(person[0].label, "Census");
        assert_eq!(person[1].event_type, "OCCUPATION");
        assert_eq!(person[1].label, "Occupation");
        assert_eq!(person[1].place.as_deref(), Some("Uppsala"));
        assert_eq!(person[1].value.as_deref(), Some("Teacher"));
    }
}
