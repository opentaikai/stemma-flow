//! Streaming line-by-line parser for GEDCOM 5.5.1 records.
//!
//! Reads through any [`BufRead`] one physical line at a time, reassembles
//! `CONC`/`CONT`-wrapped lines byte-wise *before* UTF-8 decoding (exporters
//! wrap at a fixed byte width and can split multi-byte characters across the
//! break), and yields completed `INDI`/`FAM` records, so memory stays
//! proportional to the number of pointers rather than the file size.

use std::collections::HashMap;
use std::io::BufRead;

use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::gedcom::{GedcomError, ImportReport};

/// A parsed GEDCOM line: `[level] [@XREF@] tag [payload]`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Line<'a> {
    pub level: u8,
    pub xref: Option<&'a str>,
    pub tag: &'a str,
    pub payload: &'a str,
}

/// A `2 SOUR` citation attached to an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRef {
    /// `@X@` pointer into a level-0 `SOUR` record.
    pub xref: Option<String>,
    /// Inline citation text for unlinked `2 SOUR Text` payloads.
    pub text: Option<String>,
    pub page: Option<String>,
}

/// An event recovered from a `BIRT`/`DEAT`/`MARR` sub-block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedEvent {
    /// Database event type: `BIRTH`, `DEATH` or `MARRIAGE`.
    pub event_type: String,
    pub date: Option<String>,
    pub place: Option<String>,
    pub source_refs: Vec<SourceRef>,
}

/// A parsed `0 @I1@ INDI` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndiRecord {
    pub xref: String,
    pub given_name: String,
    pub surname: String,
    pub gender: String,
    pub events: Vec<ParsedEvent>,
    pub famc: Vec<String>,
    pub fams: Vec<String>,
}

/// A parsed `0 @F1@ FAM` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FamRecord {
    pub xref: String,
    pub husband: Option<String>,
    pub wife: Option<String>,
    pub children: Vec<String>,
    pub events: Vec<ParsedEvent>,
}

/// A parsed `0 @S1@ SOUR` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourRecord {
    pub xref: String,
    pub title: String,
    pub note: Option<String>,
}

/// A completed level-0 record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Record {
    Indi(IndiRecord),
    Fam(FamRecord),
    Sour(SourRecord),
}

impl IndiRecord {
    fn new(xref: &str) -> Self {
        Self {
            xref: xref.to_string(),
            given_name: String::new(),
            surname: String::new(),
            gender: "U".to_string(),
            events: Vec::new(),
            famc: Vec::new(),
            fams: Vec::new(),
        }
    }
}

impl FamRecord {
    fn new(xref: &str) -> Self {
        Self {
            xref: xref.to_string(),
            husband: None,
            wife: None,
            children: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// Splits off the first whitespace-separated token and the remainder.
fn split_first(s: &str) -> Option<(&str, &str)> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.find(char::is_whitespace) {
        Some(index) => Some((&trimmed[..index], trimmed[index..].trim_start())),
        None => Some((trimmed, "")),
    }
}

/// Parses one raw line into level, optional cross-reference, tag and payload.
/// Returns `None` for malformed lines.
fn parse_line(raw: &str) -> Option<Line<'_>> {
    let (level, rest) = split_first(raw)?;
    let level: u8 = level.parse().ok()?;
    let (first, after_first) = split_first(rest)?;

    let (xref, tag, payload) = if let Some(inner) = first.strip_prefix('@') {
        if inner.len() < 2 || !inner.ends_with('@') {
            return None;
        }
        let (tag, payload) = split_first(after_first)?;
        (Some(first), tag, payload)
    } else {
        (None, first, after_first)
    };

    Some(Line {
        level,
        xref,
        tag,
        payload,
    })
}

/// Splits `Given /Surname/` into its two parts, tolerating missing or
/// trailing slashes.
pub(crate) fn parse_name(payload: &str) -> (String, String) {
    let mut parts = payload.split('/');
    let given_name = parts.next().unwrap_or("").trim();
    let surname = parts.next().unwrap_or("").trim();
    (given_name.to_string(), surname.to_string())
}

fn normalize_gender(payload: &str) -> String {
    match payload.trim() {
        gender @ ("M" | "F" | "U") => gender.to_string(),
        _ => "U".to_string(),
    }
}

fn optional_payload(payload: &str) -> Option<String> {
    if payload.is_empty() {
        None
    } else {
        Some(payload.to_string())
    }
}

enum Current {
    Indi {
        rec: Box<IndiRecord>,
        event_slot: Option<usize>,
    },
    Fam {
        rec: Box<FamRecord>,
        event_slot: Option<usize>,
    },
    Sour(SourRecord),
    Skipped,
}

/// Level, tag and payload offset of a physical line, parsed straight from
/// bytes so lines with invalid UTF-8 are still recognised as continuations.
struct BytePrefix<'a> {
    level: u8,
    tag: &'a [u8],
    payload_start: usize,
}

fn byte_prefix(bytes: &[u8]) -> Option<BytePrefix<'_>> {
    let mut pos = 0;
    while matches!(bytes.get(pos), Some(b' ' | b'\t')) {
        pos += 1;
    }
    let level_start = pos;
    while matches!(bytes.get(pos), Some(b'0'..=b'9')) {
        pos += 1;
    }
    if level_start == pos {
        return None;
    }
    let level: u8 = std::str::from_utf8(bytes.get(level_start..pos)?)
        .ok()?
        .parse()
        .ok()?;
    while matches!(bytes.get(pos), Some(b' ' | b'\t')) {
        pos += 1;
    }
    if bytes.get(pos) == Some(&b'@') {
        pos += 1;
        let close = bytes.get(pos..)?.iter().position(|&byte| byte == b'@')?;
        pos += close + 1;
        while matches!(bytes.get(pos), Some(b' ' | b'\t')) {
            pos += 1;
        }
    }
    let tag_start = pos;
    while bytes
        .get(pos)
        .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        pos += 1;
    }
    if tag_start == pos {
        return None;
    }
    let payload_start = if pos < bytes.len() { pos + 1 } else { pos };
    Some(BytePrefix {
        level,
        tag: bytes.get(tag_start..pos)?,
        payload_start,
    })
}

/// Streams completed records out of a GEDCOM text stream.
///
/// Yields `INDI`/`FAM` records plus the `SOUR` records that event citations
/// point at. Physical `CONC`/`CONT` lines are joined onto their base line at
/// the byte level before UTF-8 decoding; `line_no` refers to the first
/// physical line of the current logical line.
pub(crate) struct RecordReader<R: BufRead> {
    reader: R,
    line_buf: Vec<u8>,
    peeked: Option<(usize, Vec<u8>)>,
    physical_line: usize,
    line_no: usize,
    current: Option<Current>,
    pending: Option<String>,
    warnings: Vec<String>,
    saw_trlr: bool,
    warned_eof: bool,
    done: bool,
}

impl<R: BufRead> RecordReader<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self {
            reader,
            line_buf: Vec::new(),
            peeked: None,
            physical_line: 0,
            line_no: 0,
            current: None,
            pending: None,
            warnings: Vec::new(),
            saw_trlr: false,
            warned_eof: false,
            done: false,
        }
    }

    pub(crate) fn into_warnings(self) -> Vec<String> {
        self.warnings
    }

    /// Reads one physical line, stripping the line feed, a trailing carriage
    /// return and a UTF-8 byte-order mark on the very first line.
    fn read_physical(&mut self) -> std::io::Result<Option<(usize, Vec<u8>)>> {
        self.line_buf.clear();
        let read = self.reader.read_until(b'\n', &mut self.line_buf)?;
        if read == 0 {
            return Ok(None);
        }
        self.physical_line += 1;
        if self.line_buf.last() == Some(&b'\n') {
            self.line_buf.pop();
        }
        if self.line_buf.last() == Some(&b'\r') {
            self.line_buf.pop();
        }
        if self.physical_line == 1 && self.line_buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.line_buf.drain(..3);
        }
        Ok(Some((
            self.physical_line,
            std::mem::take(&mut self.line_buf),
        )))
    }

    /// Returns the next logical line: the base line plus every following
    /// `CONC` (append) or `CONT` (newline then append) payload at
    /// `base level + 1`, decoded as UTF-8.
    fn read_logical(&mut self) -> std::io::Result<Option<String>> {
        let first = match self.peeked.take() {
            Some(entry) => entry,
            None => match self.read_physical()? {
                Some(entry) => entry,
                None => return Ok(None),
            },
        };
        let (start_no, mut bytes) = first;
        self.line_no = start_no;
        loop {
            if self.peeked.is_none() {
                self.peeked = self.read_physical()?;
            }
            let absorb = match (&bytes, &self.peeked) {
                (base, Some((_, next))) => match (byte_prefix(base), byte_prefix(next)) {
                    (Some(base_prefix), Some(next_prefix))
                        if base_prefix.level.checked_add(1) == Some(next_prefix.level) =>
                    {
                        if next_prefix.tag == b"CONT" {
                            Some((true, next_prefix.payload_start))
                        } else if next_prefix.tag == b"CONC" {
                            Some((false, next_prefix.payload_start))
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                _ => None,
            };
            let Some((prepend_newline, payload_start)) = absorb else {
                break;
            };
            let Some((_, next_bytes)) = self.peeked.take() else {
                break;
            };
            if prepend_newline {
                bytes.push(b'\n');
            }
            if let Some(tail) = next_bytes.get(payload_start..) {
                bytes.extend_from_slice(tail);
            }
        }
        let raw = String::from_utf8(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        Ok(Some(raw))
    }

    /// Returns the next completed `INDI`/`FAM` record, or `None` at the end
    /// of the stream.
    pub(crate) fn next_record(&mut self) -> Result<Option<Record>, GedcomError> {
        loop {
            if self.done {
                return Ok(None);
            }
            let raw = match self.pending.take() {
                Some(raw) => raw,
                None => match self.read_logical()? {
                    Some(raw) => raw,
                    None => return self.finish_at_eof(),
                },
            };

            let Some(parsed) = parse_line(&raw) else {
                if !raw.trim().is_empty() {
                    let message = format!("line {}: malformed line skipped", self.line_no);
                    self.warnings.push(message);
                }
                continue;
            };

            if parsed.level == 0 {
                if let Some(record) = self.finish_current() {
                    self.pending = Some(raw);
                    return Ok(Some(record));
                }
                self.start_record(parsed);
            } else {
                self.route_subline(parsed);
            }
        }
    }

    fn start_record(&mut self, line: Line<'_>) {
        match line.tag {
            "INDI" => {
                self.current = match line.xref {
                    Some(xref) => Some(Current::Indi {
                        rec: Box::new(IndiRecord::new(xref)),
                        event_slot: None,
                    }),
                    None => {
                        let message =
                            format!("line {}: INDI record without @XREF@ skipped", self.line_no);
                        self.warnings.push(message);
                        Some(Current::Skipped)
                    }
                };
            }
            "FAM" => {
                self.current = match line.xref {
                    Some(xref) => Some(Current::Fam {
                        rec: Box::new(FamRecord::new(xref)),
                        event_slot: None,
                    }),
                    None => {
                        let message =
                            format!("line {}: FAM record without @XREF@ skipped", self.line_no);
                        self.warnings.push(message);
                        Some(Current::Skipped)
                    }
                };
            }
            "SOUR" => {
                self.current = match line.xref {
                    Some(xref) => Some(Current::Sour(SourRecord {
                        xref: xref.to_string(),
                        title: String::new(),
                        note: None,
                    })),
                    None => {
                        let message =
                            format!("line {}: SOUR record without @XREF@ skipped", self.line_no);
                        self.warnings.push(message);
                        Some(Current::Skipped)
                    }
                };
            }
            "TRLR" => {
                self.saw_trlr = true;
                self.done = true;
                self.current = None;
            }
            _ => {
                // HEAD, REPO, OBJE, NOTE, ...: skip the whole block.
                self.current = Some(Current::Skipped);
            }
        }
    }

    fn route_subline(&mut self, line: Line<'_>) {
        let line_no = self.line_no;
        let mut warning: Option<String> = None;
        match &mut self.current {
            None | Some(Current::Skipped) => {}
            Some(Current::Sour(sour)) => {
                if line.level == 1 {
                    match line.tag {
                        "TITL" => sour.title = line.payload.to_string(),
                        "NOTE" => sour.note = Some(line.payload.to_string()),
                        "CONT" => {
                            let continuation = line.payload.to_string();
                            sour.note = match sour.note.take() {
                                Some(existing) => Some(format!("{existing}\n{continuation}")),
                                None => Some(continuation),
                            };
                        }
                        _ => {}
                    }
                }
            }
            Some(Current::Indi { rec, event_slot }) => {
                if line.level == 1 {
                    *event_slot = None;
                    match line.tag {
                        "NAME" => {
                            let (given_name, surname) = parse_name(line.payload);
                            rec.given_name = given_name;
                            rec.surname = surname;
                        }
                        "SEX" => rec.gender = normalize_gender(line.payload),
                        "FAMC" => push_pointer(&mut rec.famc, line, "FAMC", line_no, &mut warning),
                        "FAMS" => push_pointer(&mut rec.fams, line, "FAMS", line_no, &mut warning),
                        "BIRT" | "DEAT" => {
                            let event_type = if line.tag == "BIRT" { "BIRTH" } else { "DEATH" };
                            rec.events.push(ParsedEvent {
                                event_type: event_type.to_string(),
                                date: None,
                                place: None,
                                source_refs: Vec::new(),
                            });
                            *event_slot = Some(rec.events.len() - 1);
                        }
                        _ => {}
                    }
                } else if let Some(slot) = event_slot {
                    fill_event(&mut rec.events, *slot, line);
                }
            }
            Some(Current::Fam { rec, event_slot }) => {
                if line.level == 1 {
                    *event_slot = None;
                    match line.tag {
                        "HUSB" => {
                            rec.husband = optional_pointer(line, "HUSB", line_no, &mut warning)
                        }
                        "WIFE" => rec.wife = optional_pointer(line, "WIFE", line_no, &mut warning),
                        "CHIL" => {
                            push_pointer(&mut rec.children, line, "CHIL", line_no, &mut warning)
                        }
                        "MARR" => {
                            rec.events.push(ParsedEvent {
                                event_type: "MARRIAGE".to_string(),
                                date: None,
                                place: None,
                                source_refs: Vec::new(),
                            });
                            *event_slot = Some(rec.events.len() - 1);
                        }
                        _ => {}
                    }
                } else if let Some(slot) = event_slot {
                    fill_event(&mut rec.events, *slot, line);
                }
            }
        }
        if let Some(message) = warning {
            self.warnings.push(message);
        }
    }

    fn finish_current(&mut self) -> Option<Record> {
        match self.current.take() {
            Some(Current::Indi { rec, .. }) => Some(Record::Indi(*rec)),
            Some(Current::Fam { rec, .. }) => Some(Record::Fam(*rec)),
            Some(Current::Sour(sour)) => Some(Record::Sour(sour)),
            Some(Current::Skipped) | None => None,
        }
    }

    fn finish_at_eof(&mut self) -> Result<Option<Record>, GedcomError> {
        if !self.saw_trlr && !self.warned_eof {
            self.warned_eof = true;
            let message = "missing 0 TRLR terminator".to_string();
            self.warnings.push(message);
        }
        match self.finish_current() {
            Some(record) => Ok(Some(record)),
            None => {
                self.done = true;
                Ok(None)
            }
        }
    }
}

fn push_pointer(
    target: &mut Vec<String>,
    line: Line<'_>,
    tag: &str,
    line_no: usize,
    warning: &mut Option<String>,
) {
    if line.payload.is_empty() {
        *warning = Some(format!("line {line_no}: {tag} without pointer ignored"));
    } else {
        target.push(line.payload.to_string());
    }
}

fn optional_pointer(
    line: Line<'_>,
    tag: &str,
    line_no: usize,
    warning: &mut Option<String>,
) -> Option<String> {
    if line.payload.is_empty() {
        *warning = Some(format!("line {line_no}: {tag} without pointer ignored"));
        None
    } else {
        Some(line.payload.to_string())
    }
}

fn fill_event(events: &mut [ParsedEvent], slot: usize, line: Line<'_>) {
    if let Some(event) = events.get_mut(slot) {
        match line.tag {
            "DATE" => event.date = optional_payload(line.payload),
            "PLAC" => event.place = optional_payload(line.payload),
            "SOUR" => {
                let is_pointer = line.payload.len() >= 3
                    && line.payload.starts_with('@')
                    && line.payload.ends_with('@');
                if is_pointer {
                    event.source_refs.push(SourceRef {
                        xref: Some(line.payload.to_string()),
                        text: None,
                        page: None,
                    });
                } else if !line.payload.is_empty() {
                    event.source_refs.push(SourceRef {
                        xref: None,
                        text: Some(line.payload.to_string()),
                        page: None,
                    });
                }
            }
            "PAGE" => {
                if let Some(last) = event.source_refs.last_mut() {
                    last.page = optional_payload(line.payload);
                }
            }
            _ => {}
        }
    }
}

/// Cross-references recorded during pass 1, resolved after every record is
/// known.
struct PersonLinks {
    id: String,
    xref: String,
    gender: String,
    famc: Vec<String>,
    fams: Vec<String>,
}

struct FamilyLinks {
    id: String,
    xref: String,
    husband: Option<String>,
    wife: Option<String>,
    children: Vec<String>,
}

/// A `2 SOUR @X@` pointer waiting for its level-0 `SOUR` record.
struct PendingCitation {
    event_id: String,
    xref: String,
    page: Option<String>,
}

/// Streams a GEDCOM 5.5.1 document from `reader` into `conn`.
///
/// Runs entirely inside a single transaction: any I/O or database error rolls
/// back every write, while recoverable parse problems are collected in
/// [`ImportReport::warnings`].
pub fn import_gedcom<R: BufRead>(
    conn: &mut Connection,
    reader: R,
) -> Result<ImportReport, GedcomError> {
    let mut reader = RecordReader::new(reader);
    let tx = conn.transaction()?;
    let mut report = ImportReport::default();
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut person_links: Vec<PersonLinks> = Vec::new();
    let mut family_links: Vec<FamilyLinks> = Vec::new();
    let mut sour_meta: HashMap<String, (String, Option<String>)> = HashMap::new();
    let mut sour_order: Vec<String> = Vec::new();
    let mut pending_citations: Vec<PendingCitation> = Vec::new();

    // Pass 1: stream records into the tables, recording xref -> UUID mappings.
    while let Some(record) = reader.next_record()? {
        match record {
            Record::Indi(indi) => {
                if id_map.contains_key(&indi.xref) {
                    let message = format!("duplicate INDI {} skipped", indi.xref);
                    report.warnings.push(message);
                    continue;
                }
                let id = Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO people (id, given_name, surname, gender) VALUES (?1, ?2, ?3, ?4)",
                    params![id, indi.given_name, indi.surname, indi.gender],
                )?;
                report.people += 1;
                for event in &indi.events {
                    let event_id = insert_event(&tx, event, Some(&id), None, &mut report)?;
                    collect_source_refs(
                        &tx,
                        event,
                        &event_id,
                        &mut report,
                        &mut pending_citations,
                    )?;
                }
                id_map.insert(indi.xref.clone(), id.clone());
                person_links.push(PersonLinks {
                    id,
                    xref: indi.xref,
                    gender: indi.gender,
                    famc: indi.famc,
                    fams: indi.fams,
                });
            }
            Record::Fam(fam) => {
                if id_map.contains_key(&fam.xref) {
                    let message = format!("duplicate FAM {} skipped", fam.xref);
                    report.warnings.push(message);
                    continue;
                }
                let id = Uuid::new_v4().to_string();
                tx.execute("INSERT INTO families (id) VALUES (?1)", [&id])?;
                report.families += 1;
                for event in &fam.events {
                    let event_id = insert_event(&tx, event, None, Some(&id), &mut report)?;
                    collect_source_refs(
                        &tx,
                        event,
                        &event_id,
                        &mut report,
                        &mut pending_citations,
                    )?;
                }
                id_map.insert(fam.xref.clone(), id.clone());
                family_links.push(FamilyLinks {
                    id,
                    xref: fam.xref,
                    husband: fam.husband,
                    wife: fam.wife,
                    children: fam.children,
                });
            }
            Record::Sour(sour) => {
                if sour_meta.contains_key(&sour.xref) {
                    let message = format!("duplicate SOUR {} skipped", sour.xref);
                    report.warnings.push(message);
                    continue;
                }
                sour_meta.insert(sour.xref.clone(), (sour.title, sour.note));
                sour_order.push(sour.xref);
            }
        }
    }
    report.warnings.extend(reader.into_warnings());

    // Pass 2: resolve pointers into spouse slots and the child junction.
    for family in &family_links {
        if let Some(husband) = &family.husband {
            match id_map.get(husband) {
                Some(uuid) => {
                    tx.execute(
                        "UPDATE families SET husband_id = ?1 WHERE id = ?2",
                        params![uuid, family.id],
                    )?;
                }
                None => report.warnings.push(format!(
                    "dangling HUSB pointer {husband} on family {}",
                    family.xref
                )),
            }
        }
        if let Some(wife) = &family.wife {
            match id_map.get(wife) {
                Some(uuid) => {
                    tx.execute(
                        "UPDATE families SET wife_id = ?1 WHERE id = ?2",
                        params![uuid, family.id],
                    )?;
                }
                None => report.warnings.push(format!(
                    "dangling WIFE pointer {wife} on family {}",
                    family.xref
                )),
            }
        }
        for child in &family.children {
            match id_map.get(child) {
                Some(uuid) => {
                    let changed = tx.execute(
                        "INSERT OR IGNORE INTO family_children (family_id, child_id) VALUES (?1, ?2)",
                        params![family.id, uuid],
                    )?;
                    report.child_links += changed;
                }
                None => report.warnings.push(format!(
                    "dangling CHIL pointer {child} on family {}",
                    family.xref
                )),
            }
        }
    }

    for person in &person_links {
        for family in &person.famc {
            match id_map.get(family) {
                Some(uuid) => {
                    let changed = tx.execute(
                        "INSERT OR IGNORE INTO family_children (family_id, child_id) VALUES (?1, ?2)",
                        params![uuid, person.id],
                    )?;
                    report.child_links += changed;
                }
                None => report
                    .warnings
                    .push(format!("dangling FAMC pointer {family} on {}", person.xref)),
            }
        }
        for family in &person.fams {
            let Some(uuid) = id_map.get(family) else {
                report
                    .warnings
                    .push(format!("dangling FAMS pointer {family} on {}", person.xref));
                continue;
            };
            // Fill an empty spouse slot only when SEX makes it unambiguous.
            if person.gender == "M" {
                tx.execute(
                    "UPDATE families SET husband_id = ?1 WHERE id = ?2 AND husband_id IS NULL",
                    params![person.id, uuid],
                )?;
            } else if person.gender == "F" {
                tx.execute(
                    "UPDATE families SET wife_id = ?1 WHERE id = ?2 AND wife_id IS NULL",
                    params![person.id, uuid],
                )?;
            }
        }
    }

    // Resolve event source pointers against the collected SOUR records.
    // Insertion follows document order so repeated exports stay deterministic.
    let mut refs_by_source: HashMap<&str, Vec<&PendingCitation>> = HashMap::new();
    for citation in &pending_citations {
        if sour_meta.contains_key(&citation.xref) {
            refs_by_source
                .entry(&citation.xref)
                .or_default()
                .push(citation);
        } else {
            report
                .warnings
                .push(format!("dangling SOUR pointer {}", citation.xref));
        }
    }
    for xref in &sour_order {
        let Some((title, note)) = sour_meta.get(xref.as_str()) else {
            continue;
        };
        match refs_by_source.get(xref.as_str()) {
            Some(refs) => {
                for citation in refs {
                    insert_citation(
                        &tx,
                        title,
                        citation.page.as_deref(),
                        note.as_deref(),
                        Some(&citation.event_id),
                        &mut report,
                    )?;
                }
            }
            None => insert_citation(&tx, title, None, note.as_deref(), None, &mut report)?,
        }
    }

    tx.commit()?;
    Ok(report)
}

/// Persists inline source texts immediately and queues `@X@` pointers for the
/// SOUR resolution pass.
fn collect_source_refs(
    conn: &Connection,
    event: &ParsedEvent,
    event_id: &str,
    report: &mut ImportReport,
    pending: &mut Vec<PendingCitation>,
) -> rusqlite::Result<()> {
    for source in &event.source_refs {
        if let Some(xref) = &source.xref {
            pending.push(PendingCitation {
                event_id: event_id.to_string(),
                xref: xref.clone(),
                page: source.page.clone(),
            });
        } else if let Some(title) = &source.text {
            insert_citation(
                conn,
                title,
                source.page.as_deref(),
                None,
                Some(event_id),
                report,
            )?;
        }
    }
    Ok(())
}

fn insert_citation(
    conn: &Connection,
    source_title: &str,
    page_or_reference: Option<&str>,
    notes: Option<&str>,
    event_id: Option<&str>,
    report: &mut ImportReport,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO citations (id, source_title, page_or_reference, notes, event_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            Uuid::new_v4().to_string(),
            source_title,
            page_or_reference,
            notes,
            event_id
        ],
    )?;
    report.citations += 1;
    Ok(())
}

fn insert_event(
    conn: &Connection,
    event: &ParsedEvent,
    person_id: Option<&str>,
    family_id: Option<&str>,
    report: &mut ImportReport,
) -> rusqlite::Result<String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO events (id, event_type, date, place, person_id, family_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            event.event_type,
            event.date,
            event.place,
            person_id,
            family_id
        ],
    )?;
    report.events += 1;
    Ok(id)
}
