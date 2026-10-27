//! Shared-event participants and witness roles.

use rusqlite::{Connection, OptionalExtension, params};

use crate::db::facts::{FactTypeRole, load_fact_type};

/// One additional person attached to an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventParticipant {
    pub person_id: String,
    pub person_name: String,
    pub role_key: String,
    pub role_label: String,
    pub sort_order: i64,
}

/// Event metadata and roles needed by the participant editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventContext {
    pub event_id: String,
    pub event_type: String,
    pub event_label: String,
    pub date: Option<String>,
    pub place: Option<String>,
    pub principal_id: Option<String>,
    pub principal_name: Option<String>,
    pub family_id: Option<String>,
    pub excluded_people: Vec<String>,
    pub roles: Vec<FactTypeRole>,
    pub participants: Vec<EventParticipant>,
}

/// A participant in an editable event draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantDraft {
    pub person_id: String,
    pub role_key: String,
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

/// Loads an event with its fact-type roles and current participants.
pub fn load_event_context(
    conn: &Connection,
    event_id: &str,
) -> Result<Option<EventContext>, String> {
    let row = conn
        .query_row(
            "SELECT e.event_type, COALESCE(ft.label, e.event_type), e.date, pl.name,
                    e.person_id, person.given_name, person.middle_name, person.surname,
                    e.family_id,
                    fam.husband_id, h.given_name, h.middle_name, h.surname,
                    fam.wife_id, w.given_name, w.middle_name, w.surname
             FROM events e
             LEFT JOIN fact_types ft ON ft.key = e.event_type
             LEFT JOIN places pl ON pl.id = e.place_id
             LEFT JOIN people person ON person.id = e.person_id
             LEFT JOIN families fam ON fam.id = e.family_id
             LEFT JOIN people h ON h.id = fam.husband_id
             LEFT JOIN people w ON w.id = fam.wife_id
             WHERE e.id = ?1",
            params![event_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    display_name(row.get(5)?, row.get(6)?, row.get(7)?),
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    display_name(row.get(10)?, row.get(11)?, row.get(12)?),
                    row.get::<_, Option<String>>(13)?,
                    display_name(row.get(14)?, row.get(15)?, row.get(16)?),
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((
        event_type,
        event_label,
        date,
        place,
        person_id,
        person_name,
        family_id,
        husband_id,
        husband_name,
        wife_id,
        wife_name,
    )) = row
    else {
        return Ok(None);
    };

    let principal_name = if person_id.is_some() {
        person_name.clone()
    } else {
        match (husband_name.as_deref(), wife_name.as_deref()) {
            (Some(husband), Some(wife)) => Some(format!("{husband} & {wife}")),
            (Some(name), None) | (None, Some(name)) => Some(name.to_string()),
            (None, None) => family_id.as_ref().map(|_| "Family".to_string()),
        }
    };
    let mut excluded_people = Vec::new();
    if let Some(person_id) = &person_id {
        excluded_people.push(person_id.clone());
    }
    if let Some(husband_id) = husband_id {
        excluded_people.push(husband_id);
    }
    if let Some(wife_id) = wife_id {
        excluded_people.push(wife_id);
    }

    let roles = load_fact_type(conn, &event_type)?
        .map(|record| record.roles)
        .unwrap_or_default();
    let mut stmt = conn
        .prepare(
            "SELECT ep.person_id, p.given_name, p.middle_name, p.surname,
                    ep.role_key, COALESCE(r.label, ep.role_key), ep.sort_order
             FROM event_participants ep
             JOIN people p ON p.id = ep.person_id
             LEFT JOIN fact_type_roles r
                 ON r.fact_type_key = ?1 AND r.role_key = ep.role_key
             WHERE ep.event_id = ?2
             ORDER BY ep.sort_order, p.surname COLLATE NOCASE,
                      p.given_name COLLATE NOCASE, ep.person_id",
        )
        .map_err(|error| error.to_string())?;
    let participants = stmt
        .query_map(params![event_type, event_id], |row| {
            Ok(EventParticipant {
                person_id: row.get(0)?,
                person_name: display_name(row.get(1)?, row.get(2)?, row.get(3)?)
                    .unwrap_or_else(|| "(unnamed)".to_string()),
                role_key: row.get(4)?,
                role_label: row.get(5)?,
                sort_order: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;

    Ok(Some(EventContext {
        event_id: event_id.to_string(),
        event_type,
        event_label,
        date,
        place,
        principal_id: person_id,
        principal_name,
        family_id,
        excluded_people,
        roles,
        participants,
    }))
}

/// Replaces the participants of an event.
pub fn save_event_participants(
    conn: &mut Connection,
    event_id: &str,
    participants: &[ParticipantDraft],
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let context =
        load_event_context(&tx, event_id)?.ok_or_else(|| format!("no event with id {event_id}"))?;
    let roles: std::collections::HashSet<&str> = context
        .roles
        .iter()
        .map(|role| role.role_key.as_str())
        .collect();
    let excluded: std::collections::HashSet<&str> =
        context.excluded_people.iter().map(String::as_str).collect();
    let mut seen = std::collections::HashSet::new();
    for participant in participants {
        if !seen.insert(participant.person_id.as_str()) {
            return Err(format!("person {} is listed twice", participant.person_id));
        }
        if excluded.contains(participant.person_id.as_str()) {
            return Err("the event principal cannot be a participant".to_string());
        }
        if !roles.contains(participant.role_key.as_str()) {
            return Err(format!(
                "role {} does not belong to {}",
                participant.role_key, context.event_type
            ));
        }
        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM people WHERE id = ?1",
                params![participant.person_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if exists.is_none() {
            return Err(format!("no person with id {}", participant.person_id));
        }
    }

    tx.execute(
        "DELETE FROM event_participants WHERE event_id = ?1",
        params![event_id],
    )
    .map_err(|error| error.to_string())?;
    for (index, participant) in participants.iter().enumerate() {
        tx.execute(
            "INSERT INTO event_participants (event_id, person_id, role_key, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                event_id,
                participant.person_id,
                participant.role_key,
                index as i64
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::facts::{FactRoleDraft, save_fact_roles};
    use crate::db::init_db;

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma applies");
        init_db(&conn).expect("schema initialises");
        conn
    }

    fn people(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO people (id, given_name, middle_name, surname, gender) VALUES
                ('p1', 'Ada', '', 'King', 'F'),
                ('p2', 'Otto', '', 'King', 'M'),
                ('p3', 'Bo', 'Lynn', 'King', 'M');
             INSERT INTO families (id, husband_id, wife_id) VALUES ('f1', 'p2', 'p1');
             INSERT INTO events (id, event_type, date, person_id, family_id) VALUES
                ('e1', 'BIRTH', '1850', 'p1', NULL),
                ('e2', 'MARRIAGE', '1875', NULL, 'f1');",
        )
        .expect("fixtures insert");
    }

    fn birth_roles(conn: &mut Connection) {
        save_fact_roles(
            conn,
            "BIRTH",
            &[
                FactRoleDraft {
                    key: None,
                    label: "Witness".to_string(),
                },
                FactRoleDraft {
                    key: None,
                    label: "Godparent".to_string(),
                },
            ],
        )
        .expect("birth roles");
    }

    #[test]
    fn saves_and_lists_person_event_participants() {
        let mut conn = connection();
        people(&conn);
        birth_roles(&mut conn);
        let context = load_event_context(&conn, "e1")
            .expect("context")
            .expect("event");
        assert_eq!(context.event_type, "BIRTH");
        assert_eq!(context.principal_name.as_deref(), Some("Ada King"));
        assert_eq!(context.roles.len(), 2);
        assert_eq!(context.excluded_people, vec!["p1"]);

        save_event_participants(
            &mut conn,
            "e1",
            &[ParticipantDraft {
                person_id: "p3".to_string(),
                role_key: "GODPARENT".to_string(),
            }],
        )
        .expect("participants save");
        let context = load_event_context(&conn, "e1")
            .expect("context")
            .expect("event");
        assert_eq!(context.participants.len(), 1);
        assert_eq!(context.participants[0].person_name, "Bo Lynn King");
        assert_eq!(context.participants[0].role_label, "Godparent");
    }

    #[test]
    fn family_context_excludes_spouses_and_allows_witnesses() {
        let mut conn = connection();
        people(&conn);
        let context = load_event_context(&conn, "e2")
            .expect("context")
            .expect("event");
        assert_eq!(
            context.principal_name.as_deref(),
            Some("Otto King & Ada King")
        );
        assert_eq!(context.excluded_people, vec!["p2", "p1"]);

        save_event_participants(
            &mut conn,
            "e2",
            &[ParticipantDraft {
                person_id: "p3".to_string(),
                role_key: "WITNESS".to_string(),
            }],
        )
        .expect_err("marriage has no roles yet");
        save_fact_roles(
            &mut conn,
            "MARRIAGE",
            &[FactRoleDraft {
                key: None,
                label: "Witness".to_string(),
            }],
        )
        .expect("marriage roles");
        save_event_participants(
            &mut conn,
            "e2",
            &[ParticipantDraft {
                person_id: "p3".to_string(),
                role_key: "WITNESS".to_string(),
            }],
        )
        .expect("family witness saves");
        let context = load_event_context(&conn, "e2")
            .expect("context")
            .expect("event");
        assert_eq!(context.participants.len(), 1);
    }

    #[test]
    fn rejects_principals_duplicates_and_unknown_roles() {
        let mut conn = connection();
        people(&conn);
        birth_roles(&mut conn);
        assert!(
            save_event_participants(
                &mut conn,
                "e1",
                &[ParticipantDraft {
                    person_id: "p1".to_string(),
                    role_key: "WITNESS".to_string(),
                }]
            )
            .is_err()
        );
        assert!(
            save_event_participants(
                &mut conn,
                "e1",
                &[
                    ParticipantDraft {
                        person_id: "p3".to_string(),
                        role_key: "WITNESS".to_string(),
                    },
                    ParticipantDraft {
                        person_id: "p3".to_string(),
                        role_key: "GODPARENT".to_string(),
                    },
                ]
            )
            .is_err()
        );
        assert!(
            save_event_participants(
                &mut conn,
                "e1",
                &[ParticipantDraft {
                    person_id: "p3".to_string(),
                    role_key: "MISSING".to_string(),
                }]
            )
            .is_err()
        );
    }
}
