//! Streaming line-by-line parser for GEDCOM 5.5.1 records.
//!
//! Reads through any [`BufRead`] one physical line at a time, reassembles
//! `CONC`/`CONT`-wrapped lines byte-wise *before* decoding (exporters wrap
//! at a fixed byte width and can split multi-byte characters across the
//! break), honours the `1 CHAR` header charset with a Windows-1251 fallback,
//! and yields completed `INDI`/`FAM` records, so memory stays proportional
//! to the number of pointers rather than the file size.

use std::collections::HashMap;
use std::io::BufRead;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::gedcom::{GedcomError, ImportReport};
use crate::model::fact::{FactScope, by_tag};

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
    /// GEDCOM certainty (`QUAY`).
    pub quality: Option<String>,
    /// Citation-level note (`3 NOTE`).
    pub note: Option<String>,
    pub media_type: Option<String>,
    pub data_date: Option<String>,
    pub data_text: Option<String>,
}

/// A fact recovered from an event or attribute sub-block.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedEvent {
    /// Database fact key, such as `BIRTH`, `OCCUPATION` or `MARRIAGE`.
    pub event_type: String,
    pub date: Option<String>,
    pub place: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Attribute value, such as `OCCU Teacher`.
    pub description: Option<String>,
    pub source_refs: Vec<SourceRef>,
}

/// A parsed `0 @I1@ INDI` record.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IndiRecord {
    pub xref: String,
    pub given_name: String,
    /// `2 _MIDN` patronymic or middle name.
    pub middle_name: String,
    pub surname: String,
    /// `2 _PGVN`/`2 NICK` preferred given name or nickname.
    pub nickname: String,
    pub gender: String,
    /// First `1 NOTE` block, including `2 CONT`/`2 CONC` continuations;
    /// later note blocks on the same person are ignored.
    pub note: Option<String>,
    pub events: Vec<ParsedEvent>,
    pub famc: Vec<String>,
    pub fams: Vec<String>,
}

/// A parsed `0 @F1@ FAM` record.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FamRecord {
    pub xref: String,
    pub husband: Option<String>,
    pub wife: Option<String>,
    pub children: Vec<String>,
    pub events: Vec<ParsedEvent>,
}

/// One `1 REPO @R@` link on a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositoryRef {
    pub xref: String,
    pub call_number: Option<String>,
    pub media_type: Option<String>,
}

/// A parsed `0 @S1@ SOUR` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourRecord {
    pub xref: String,
    pub title: String,
    pub author: Option<String>,
    pub publication: Option<String>,
    pub abbreviation: Option<String>,
    pub repositories: Vec<RepositoryRef>,
    pub note: Option<String>,
    pub data_date: Option<String>,
    pub data_text: Option<String>,
}

impl SourRecord {
    fn new(xref: &str) -> Self {
        Self {
            xref: xref.to_string(),
            title: String::new(),
            author: None,
            publication: None,
            abbreviation: None,
            repositories: Vec::new(),
            note: None,
            data_date: None,
            data_text: None,
        }
    }
}

/// A parsed `0 @R1@ REPO` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositoryRecord {
    pub xref: String,
    pub name: String,
    pub address: Option<String>,
    pub note: Option<String>,
}

impl RepositoryRecord {
    fn new(xref: &str) -> Self {
        Self {
            xref: xref.to_string(),
            name: String::new(),
            address: None,
            note: None,
        }
    }
}

/// A completed level-0 record.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Record {
    Indi(IndiRecord),
    Fam(FamRecord),
    Sour(SourRecord),
    Repo(RepositoryRecord),
}

impl IndiRecord {
    fn new(xref: &str) -> Self {
        Self {
            xref: xref.to_string(),
            given_name: String::new(),
            middle_name: String::new(),
            surname: String::new(),
            nickname: String::new(),
            gender: "U".to_string(),
            note: None,
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

fn is_pointer(payload: &str) -> bool {
    payload.len() >= 3 && payload.starts_with('@') && payload.ends_with('@')
}

/// Appends a `2 CONT` (new line) or `2 CONC` (same line) segment to the
/// person's first note; an empty note starts with the segment itself.
fn append_note_continuation(rec: &mut IndiRecord, payload: &str, new_line: bool) {
    let Some(note) = rec.note.as_mut() else {
        return;
    };
    if note.is_empty() {
        note.push_str(payload);
        return;
    }
    if new_line {
        note.push('\n');
    }
    note.push_str(payload);
}

/// Warning text for a tag the importer recognises but does not model: the
/// record keeps importing, the tag is listed for review with its payload
/// flattened (CONT newlines) and truncated to 100 characters.
fn unknown_tag_warning(line_no: usize, line: &Line<'_>) -> String {
    if line.payload.is_empty() {
        return format!(
            "Line {line_no}: Tag recognized/unsupported or ignored [{}]",
            line.tag
        );
    }
    let flat = line.payload.replace('\n', "\\n");
    let mut chars = flat.chars();
    let value: String = chars.by_ref().take(100).collect();
    if chars.next().is_some() {
        format!(
            "Line {line_no}: Tag recognized/unsupported or ignored [{} {value}…]",
            line.tag
        )
    } else {
        format!(
            "Line {line_no}: Tag recognized/unsupported or ignored [{} {value}]",
            line.tag
        )
    }
}

enum Current {
    Indi {
        rec: Box<IndiRecord>,
        event_slot: Option<usize>,
        /// True while level-2 lines belong to the person's first `1 NOTE`.
        in_note: bool,
    },
    Fam {
        rec: Box<FamRecord>,
        event_slot: Option<usize>,
    },
    Sour {
        rec: SourRecord,
        section: SourSection,
    },
    Repo(RepositoryRecord),
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourSection {
    None,
    Data,
    Repository,
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

/// Legacy single-byte charsets the decode fallback can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Charset {
    Cp1251,
    Latin1,
}

impl Charset {
    /// Decodes `bytes`, replacing malformed sequences instead of failing
    /// (the final safety net of the fallback chain).
    fn decode(self, bytes: &[u8]) -> String {
        match self {
            Self::Cp1251 => {
                let (text, _, _) = encoding_rs::WINDOWS_1251.decode(bytes);
                text.into_owned()
            }
            Self::Latin1 => encoding_rs::mem::decode_latin1(bytes).into_owned(),
        }
    }
}

/// What a `1 CHAR` header line declares.
enum CharHeader {
    Utf8,
    Legacy(Charset),
    Unknown,
}

/// Maps common `1 CHAR` header values onto system charsets; unknown values
/// fall through as [`CharHeader::Unknown`] and keep the UTF-8 chain.
fn parse_char_header(payload: &str) -> CharHeader {
    match payload.trim().to_ascii_uppercase().as_str() {
        "UTF-8" | "UTF8" => CharHeader::Utf8,
        "ANSI" | "WINDOWS" | "CP1251" | "CP-1251" | "WINDOWS1251" | "WINDOWS-1251" => {
            CharHeader::Legacy(Charset::Cp1251)
        }
        "LATIN1" | "LATIN-1" | "ISO-8859-1" | "ISO8859-1" | "ANSEL" | "ASCII" => {
            CharHeader::Legacy(Charset::Latin1)
        }
        _ => CharHeader::Unknown,
    }
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
    /// Active non-UTF-8 decoder: set by a legacy `1 CHAR` header or by the
    /// first UTF-8 failure, and then applied to every remaining line.
    decoder: Option<Charset>,
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
            decoder: None,
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
        self.decode_logical(bytes).map(Some)
    }

    /// Decodes one assembled logical line: the active legacy charset when a
    /// header or an earlier failure engaged one, otherwise strict UTF-8 with
    /// a one-time fall back to Windows-1251 (Cyrillic compatibility).
    fn decode_logical(&mut self, bytes: Vec<u8>) -> std::io::Result<String> {
        if let Some(charset) = self.decoder {
            return Ok(charset.decode(&bytes));
        }
        match String::from_utf8(bytes) {
            Ok(text) => Ok(text),
            Err(error) => {
                self.decoder = Some(Charset::Cp1251);
                let message = format!(
                    "Line {}: invalid UTF-8; decoding the rest of the stream as windows-1251",
                    self.line_no
                );
                self.warnings.push(message);
                Ok(Charset::Cp1251.decode(error.as_bytes()))
            }
        }
    }

    /// Applies a `1 CHAR` header to the decode state of the stream.
    fn apply_char_header(&mut self, payload: &str) {
        match parse_char_header(payload) {
            CharHeader::Utf8 => {}
            CharHeader::Legacy(charset) => self.decoder = Some(charset),
            CharHeader::Unknown => {
                let message = format!(
                    "Line {}: unknown CHAR charset [{}]; assuming UTF-8",
                    self.line_no,
                    payload.trim()
                );
                self.warnings.push(message);
            }
        }
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
            if parsed.level == 1 && parsed.tag == "CHAR" {
                self.apply_char_header(parsed.payload);
            }

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
                        in_note: false,
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
                    Some(xref) => Some(Current::Sour {
                        rec: SourRecord::new(xref),
                        section: SourSection::None,
                    }),
                    None => {
                        let message =
                            format!("line {}: SOUR record without @XREF@ skipped", self.line_no);
                        self.warnings.push(message);
                        Some(Current::Skipped)
                    }
                };
            }
            "REPO" => {
                self.current = match line.xref {
                    Some(xref) => Some(Current::Repo(RepositoryRecord::new(xref))),
                    None => {
                        let message =
                            format!("line {}: REPO record without @XREF@ skipped", self.line_no);
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
                // OBJE, NOTE, ...: list the tag once and skip the whole
                // block. HEAD/SUBM/REPO/SUBN are known structure that the
                // importer simply does not need, so they stay silent.
                if !matches!(line.tag, "HEAD" | "SUBM" | "REPO" | "SUBN") {
                    let message = unknown_tag_warning(self.line_no, &line);
                    self.warnings.push(message);
                }
                self.current = Some(Current::Skipped);
            }
        }
    }

    fn route_subline(&mut self, line: Line<'_>) {
        let line_no = self.line_no;
        let mut warning: Option<String> = None;
        match &mut self.current {
            None | Some(Current::Skipped) => {}
            Some(Current::Sour { rec, section }) => {
                if line.level == 1 {
                    match line.tag {
                        "TITL" => rec.title = line.payload.to_string(),
                        "AUTH" => rec.author = optional_payload(line.payload),
                        "PUBL" => rec.publication = optional_payload(line.payload),
                        "ABBR" => rec.abbreviation = optional_payload(line.payload),
                        "NOTE" => rec.note = Some(line.payload.to_string()),
                        "DATA" => *section = SourSection::Data,
                        "REPO" => {
                            if is_pointer(line.payload) {
                                rec.repositories.push(RepositoryRef {
                                    xref: line.payload.to_string(),
                                    call_number: None,
                                    media_type: None,
                                });
                                *section = SourSection::Repository;
                            } else {
                                *section = SourSection::None;
                                warning =
                                    Some(format!("line {line_no}: REPO without pointer ignored"));
                            }
                        }
                        "CONT" => {
                            let continuation = line.payload.to_string();
                            rec.note = match rec.note.take() {
                                Some(existing) => Some(format!("{existing}\n{continuation}")),
                                None => Some(continuation),
                            };
                        }
                        _ => warning = Some(unknown_tag_warning(line_no, &line)),
                    }
                    if line.tag != "DATA" && line.tag != "REPO" {
                        *section = SourSection::None;
                    }
                } else if line.level == 2 {
                    match *section {
                        SourSection::Data => match line.tag {
                            "DATE" => rec.data_date = optional_payload(line.payload),
                            "TEXT" => rec.data_text = optional_payload(line.payload),
                            _ => warning = Some(unknown_tag_warning(line_no, &line)),
                        },
                        SourSection::Repository => match rec.repositories.last_mut() {
                            Some(repository) => match line.tag {
                                "CALN" => repository.call_number = optional_payload(line.payload),
                                "MEDI" => repository.media_type = optional_payload(line.payload),
                                _ => warning = Some(unknown_tag_warning(line_no, &line)),
                            },
                            None => warning = Some(unknown_tag_warning(line_no, &line)),
                        },
                        SourSection::None => warning = Some(unknown_tag_warning(line_no, &line)),
                    }
                } else {
                    warning = Some(unknown_tag_warning(line_no, &line));
                }
            }
            Some(Current::Repo(repo)) => {
                if line.level == 1 {
                    match line.tag {
                        "NAME" => repo.name = line.payload.to_string(),
                        "ADDR" => repo.address = optional_payload(line.payload),
                        "NOTE" => repo.note = Some(line.payload.to_string()),
                        "CONT" => {
                            let continuation = line.payload.to_string();
                            repo.note = match repo.note.take() {
                                Some(existing) => Some(format!("{existing}\n{continuation}")),
                                None => Some(continuation),
                            };
                        }
                        "PHON" | "EMAIL" | "FAX" | "WWW" => {}
                        _ => warning = Some(unknown_tag_warning(line_no, &line)),
                    }
                } else if line.level == 2 {
                    match line.tag {
                        "ADR1" | "ADR2" | "CITY" | "STAE" | "POST" | "CTRY" => {
                            let text = line.payload.trim();
                            if !text.is_empty() {
                                match &mut repo.address {
                                    Some(address) => {
                                        address.push('\n');
                                        address.push_str(text);
                                    }
                                    None => repo.address = Some(text.to_string()),
                                }
                            }
                        }
                        _ => warning = Some(unknown_tag_warning(line_no, &line)),
                    }
                }
            }
            Some(Current::Indi {
                rec,
                event_slot,
                in_note,
            }) => {
                if line.level == 1 {
                    *event_slot = None;
                    *in_note = false;
                    match line.tag {
                        "NAME" => {
                            let (given_name, surname) = parse_name(line.payload);
                            rec.given_name = given_name;
                            rec.surname = surname;
                        }
                        "SEX" => rec.gender = normalize_gender(line.payload),
                        "FAMC" => push_pointer(&mut rec.famc, line, "FAMC", line_no, &mut warning),
                        "FAMS" => push_pointer(&mut rec.fams, line, "FAMS", line_no, &mut warning),
                        "NOTE" => {
                            // First note wins; later blocks are swallowed
                            // so their continuations stay silent too.
                            *in_note = rec.note.is_none();
                            if *in_note {
                                rec.note = Some(line.payload.to_string());
                            }
                        }
                        other => {
                            if let Some(fact) = by_tag(other, FactScope::Person) {
                                rec.events.push(ParsedEvent {
                                    event_type: fact.key.to_string(),
                                    date: None,
                                    place: None,
                                    latitude: None,
                                    longitude: None,
                                    description: optional_payload(line.payload),
                                    source_refs: Vec::new(),
                                });
                                *event_slot = Some(rec.events.len() - 1);
                            } else {
                                warning = Some(unknown_tag_warning(line_no, &line));
                            }
                        }
                    }
                } else if *in_note {
                    match line.tag {
                        "CONT" => append_note_continuation(rec, line.payload, true),
                        "CONC" => append_note_continuation(rec, line.payload, false),
                        _ => {
                            *in_note = false;
                            warning = Some(unknown_tag_warning(line_no, &line));
                        }
                    }
                } else if event_slot.is_none() {
                    // Name-part children of `1 NAME` (GIVN/SURN/_MIDN/_PGVN…)
                    // land here because events are closed by the level-1 line.
                    match line.tag {
                        "GIVN" => rec.given_name = line.payload.to_string(),
                        "SURN" => rec.surname = line.payload.to_string(),
                        "_MIDN" => rec.middle_name = line.payload.to_string(),
                        "_PGVN" | "NICK" => rec.nickname = line.payload.to_string(),
                        _ => warning = Some(unknown_tag_warning(line_no, &line)),
                    }
                } else if let Some(slot) = event_slot {
                    fill_event(&mut rec.events, *slot, line, line_no, &mut warning);
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
                        other => {
                            if let Some(fact) = by_tag(other, FactScope::Family) {
                                rec.events.push(ParsedEvent {
                                    event_type: fact.key.to_string(),
                                    date: None,
                                    place: None,
                                    latitude: None,
                                    longitude: None,
                                    description: optional_payload(line.payload),
                                    source_refs: Vec::new(),
                                });
                                *event_slot = Some(rec.events.len() - 1);
                            } else {
                                warning = Some(unknown_tag_warning(line_no, &line));
                            }
                        }
                    }
                } else if event_slot.is_none() {
                    // Children of an unknown level-1 tag (REFN/TYPE…).
                    warning = Some(unknown_tag_warning(line_no, &line));
                } else if let Some(slot) = event_slot {
                    fill_event(&mut rec.events, *slot, line, line_no, &mut warning);
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
            Some(Current::Sour { rec, .. }) => Some(Record::Sour(rec)),
            Some(Current::Repo(repo)) => Some(Record::Repo(repo)),
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

fn fill_event(
    events: &mut [ParsedEvent],
    slot: usize,
    line: Line<'_>,
    line_no: usize,
    warning: &mut Option<String>,
) {
    let Some(event) = events.get_mut(slot) else {
        return;
    };
    match line.level {
        2 => match line.tag {
            "DATE" => event.date = optional_payload(line.payload),
            "PLAC" => event.place = optional_payload(line.payload),
            "SOUR" => {
                let mut source = SourceRef {
                    xref: None,
                    text: None,
                    page: None,
                    quality: None,
                    note: None,
                    media_type: None,
                    data_date: None,
                    data_text: None,
                };
                if is_pointer(line.payload) {
                    source.xref = Some(line.payload.to_string());
                } else if !line.payload.is_empty() {
                    source.text = Some(line.payload.to_string());
                } else {
                    *warning = Some(format!(
                        "line {line_no}: SOUR without pointer or text ignored"
                    ));
                    return;
                }
                event.source_refs.push(source);
            }
            _ => *warning = Some(unknown_tag_warning(line_no, &line)),
        },
        3 => match line.tag {
            "DATA" => {}
            "MAP" => {}
            "PAGE" | "QUAY" | "NOTE" | "MEDI" => {
                if let Some(source) = event.source_refs.last_mut() {
                    match line.tag {
                        "PAGE" => source.page = optional_payload(line.payload),
                        "QUAY" => source.quality = optional_payload(line.payload),
                        "NOTE" => source.note = optional_payload(line.payload),
                        _ => source.media_type = optional_payload(line.payload),
                    }
                } else {
                    *warning = Some(format!(
                        "line {line_no}: {} without a preceding SOUR ignored",
                        line.tag
                    ));
                }
            }
            _ => *warning = Some(unknown_tag_warning(line_no, &line)),
        },
        4 => match line.tag {
            "DATE" | "TEXT" => {
                if let Some(source) = event.source_refs.last_mut() {
                    if line.tag == "DATE" {
                        source.data_date = optional_payload(line.payload);
                    } else {
                        source.data_text = optional_payload(line.payload);
                    }
                } else {
                    *warning = Some(format!(
                        "line {line_no}: {} without a preceding SOUR ignored",
                        line.tag
                    ));
                }
            }
            "LATI" => match parse_latitude(line.payload) {
                Some(value) => event.latitude = Some(value),
                None => {
                    *warning = Some(format!("line {line_no}: invalid latitude {}", line.payload));
                }
            },
            "LONG" => match parse_longitude(line.payload) {
                Some(value) => event.longitude = Some(value),
                None => {
                    *warning = Some(format!(
                        "line {line_no}: invalid longitude {}",
                        line.payload
                    ));
                }
            },
            _ => *warning = Some(unknown_tag_warning(line_no, &line)),
        },
        _ => *warning = Some(unknown_tag_warning(line_no, &line)),
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
    quality: Option<String>,
    note: Option<String>,
    media_type: Option<String>,
    data_date: Option<String>,
    data_text: Option<String>,
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
    let mut sour_meta: HashMap<String, SourRecord> = HashMap::new();
    let mut sour_order: Vec<String> = Vec::new();
    let mut repo_ids: HashMap<String, String> = HashMap::new();
    let mut inline_sources: HashMap<String, String> = HashMap::new();
    let mut pending_citations: Vec<PendingCitation> = Vec::new();
    let mut place_cache: HashMap<String, String> = HashMap::new();

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
                    "INSERT INTO people (id, given_name, middle_name, surname, nickname, gender, notes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        indi.given_name,
                        indi.middle_name,
                        indi.surname,
                        indi.nickname,
                        indi.gender,
                        indi.note.unwrap_or_default()
                    ],
                )?;
                report.people += 1;
                for event in &indi.events {
                    let event_id =
                        insert_event(&tx, event, Some(&id), None, &mut report, &mut place_cache)?;
                    collect_source_refs(
                        &tx,
                        event,
                        &event_id,
                        &mut report,
                        &mut pending_citations,
                        &mut inline_sources,
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
                    let event_id =
                        insert_event(&tx, event, None, Some(&id), &mut report, &mut place_cache)?;
                    collect_source_refs(
                        &tx,
                        event,
                        &event_id,
                        &mut report,
                        &mut pending_citations,
                        &mut inline_sources,
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
                let xref = sour.xref.clone();
                sour_meta.insert(xref.clone(), sour);
                sour_order.push(xref);
            }
            Record::Repo(repo) => {
                if repo_ids.contains_key(&repo.xref) {
                    let message = format!("duplicate REPO {} skipped", repo.xref);
                    report.warnings.push(message);
                    continue;
                }
                let id = Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO repositories (id, name, address, notes)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, repo.name, repo.address, repo.note],
                )?;
                repo_ids.insert(repo.xref, id);
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

    // Materialise SOUR records as sources, then attach the pending citations.
    // Insertion follows document order so repeated exports stay deterministic.
    let mut source_ids: HashMap<&str, String> = HashMap::with_capacity(sour_order.len());
    for xref in &sour_order {
        let Some(source) = sour_meta.get(xref.as_str()) else {
            continue;
        };
        let source_id = insert_source(&tx, source, &repo_ids, &mut report)?;
        source_ids.insert(xref.as_str(), source_id);
    }
    for citation in &pending_citations {
        match source_ids.get(citation.xref.as_str()) {
            Some(source_id) => insert_citation(
                &tx,
                source_id,
                citation.page.as_deref(),
                citation.quality.as_deref(),
                citation.note.as_deref(),
                citation.media_type.as_deref(),
                citation.data_date.as_deref(),
                citation.data_text.as_deref(),
                Some(&citation.event_id),
                &mut report,
            )?,
            None => report
                .warnings
                .push(format!("dangling SOUR pointer {}", citation.xref)),
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
    inline_sources: &mut HashMap<String, String>,
) -> rusqlite::Result<()> {
    for source in &event.source_refs {
        if let Some(xref) = &source.xref {
            pending.push(PendingCitation {
                event_id: event_id.to_string(),
                xref: xref.clone(),
                page: source.page.clone(),
                quality: source.quality.clone(),
                note: source.note.clone(),
                media_type: source.media_type.clone(),
                data_date: source.data_date.clone(),
                data_text: source.data_text.clone(),
            });
        } else if let Some(title) = &source.text {
            let source_id = insert_inline_source(conn, inline_sources, title)?;
            insert_citation(
                conn,
                &source_id,
                source.page.as_deref(),
                source.quality.as_deref(),
                source.note.as_deref(),
                source.media_type.as_deref(),
                source.data_date.as_deref(),
                source.data_text.as_deref(),
                Some(event_id),
                report,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_citation(
    conn: &Connection,
    source_id: &str,
    page_or_reference: Option<&str>,
    quality: Option<&str>,
    note: Option<&str>,
    media_type: Option<&str>,
    data_date: Option<&str>,
    data_text: Option<&str>,
    event_id: Option<&str>,
    report: &mut ImportReport,
) -> rusqlite::Result<()> {
    let citation_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO citations (id, source_id, page_or_reference, notes,
             quality_certainty, media_type, data_date, data_text)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            citation_id,
            source_id,
            page_or_reference,
            note,
            quality,
            media_type,
            data_date,
            data_text
        ],
    )?;
    if let Some(event_id) = event_id {
        conn.execute(
            "INSERT INTO citation_links (citation_id, event_id) VALUES (?1, ?2)",
            params![citation_id, event_id],
        )?;
    }
    report.citations += 1;
    Ok(())
}

fn insert_source(
    conn: &Connection,
    source: &SourRecord,
    repositories: &HashMap<String, String>,
    report: &mut ImportReport,
) -> rusqlite::Result<String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sources (id, title, author, publication, abbreviation, notes,
             data_date, data_text)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            source.title,
            source.author,
            source.publication,
            source.abbreviation,
            source.note,
            source.data_date,
            source.data_text
        ],
    )?;
    for repository in &source.repositories {
        match repositories.get(&repository.xref) {
            Some(repository_id) => {
                conn.execute(
                    "INSERT OR IGNORE INTO source_repositories
                     (source_id, repository_id, call_number, media_type)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        id,
                        repository_id,
                        repository.call_number,
                        repository.media_type
                    ],
                )?;
            }
            None => report
                .warnings
                .push(format!("dangling REPO pointer {}", repository.xref)),
        }
    }
    Ok(id)
}

fn insert_inline_source(
    conn: &Connection,
    cache: &mut HashMap<String, String>,
    title: &str,
) -> rusqlite::Result<String> {
    if let Some(id) = cache.get(title) {
        return Ok(id.clone());
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sources (id, title) VALUES (?1, ?2)",
        params![id, title],
    )?;
    cache.insert(title.to_string(), id.clone());
    Ok(id)
}

fn insert_event(
    conn: &Connection,
    event: &ParsedEvent,
    person_id: Option<&str>,
    family_id: Option<&str>,
    report: &mut ImportReport,
    places: &mut HashMap<String, String>,
) -> rusqlite::Result<String> {
    let id = Uuid::new_v4().to_string();
    let place_id = resolve_place(
        conn,
        places,
        event.place.as_deref(),
        event.latitude,
        event.longitude,
    )?;
    conn.execute(
        "INSERT INTO events
         (id, event_type, date, place_id, person_id, family_id, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            event.event_type,
            event.date,
            place_id,
            person_id,
            family_id,
            event.description
        ],
    )?;
    report.events += 1;
    Ok(id)
}

/// Finds or creates a place for a trimmed non-empty name.
fn resolve_place(
    conn: &Connection,
    cache: &mut HashMap<String, String>,
    name: Option<&str>,
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> rusqlite::Result<Option<String>> {
    let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else {
        return Ok(None);
    };
    let id = if let Some(id) = cache.get(name) {
        id.clone()
    } else {
        let id: String = match conn
            .query_row(
                "SELECT id FROM places WHERE name = ?1",
                params![name],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO places (id, name, latitude, longitude)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, name, latitude, longitude],
                )?;
                cache.insert(name.to_string(), id.clone());
                return Ok(Some(id));
            }
        };
        cache.insert(name.to_string(), id.clone());
        id
    };
    if latitude.is_some() || longitude.is_some() {
        conn.execute(
            "UPDATE places
             SET latitude = COALESCE(latitude, ?1), longitude = COALESCE(longitude, ?2)
             WHERE id = ?3",
            params![latitude, longitude, id],
        )?;
    }
    Ok(Some(id))
}

fn parse_latitude(text: &str) -> Option<f64> {
    parse_hemisphere(text, 'N', 'S', 90.0)
}

fn parse_longitude(text: &str) -> Option<f64> {
    parse_hemisphere(text, 'E', 'W', 180.0)
}

fn parse_hemisphere(text: &str, positive: char, negative: char, limit: f64) -> Option<f64> {
    let mut chars = text.trim().chars();
    let prefix = chars.next()?;
    let sign = if prefix.to_ascii_uppercase() == positive {
        1.0
    } else if prefix.to_ascii_uppercase() == negative {
        -1.0
    } else {
        return None;
    };
    let value = chars.as_str().trim().parse::<f64>().ok()?;
    if !value.is_finite() || !(0.0..=limit).contains(&value) {
        return None;
    }
    Some(sign * value)
}
