# Stemma Flow Roadmap

This roadmap describes the path from the current local-first genealogy
application. It is a planning document: items are ordered by dependency and user
value, not by release date. Nothing here is a commitment to a specific schedule.

## Guiding principles

- Local-first: the tree lives in one SQLite file on the user's machine.
- Offline by default: every core feature works without a network.
- GEDCOM-first: import and export stay lossless and deterministic.
- No subscription: features are not gated behind an account.
- Network services (Ancestry, FamilySearch, MyHeritage, Findmypast, DNA,
  AI) are opt-in and deferred.
- Data safety first: transactions, backups and migrations are part of the
  feature, not an afterthought.

## Notation

Status:

- `done` - implemented and covered by tests.
- `partial` - data model or GEDCOM support exists, but the UI or scope is
  incomplete.
- `planned` - not implemented yet.
- `deferred` - intentionally scheduled later or optional.

Priority:

- `P0` - required for a usable research application.
- `P1` - important for parity and daily workflows.
- `P2` - refinement, scale, or optional integrations.

## Current baseline

| Capability | Status | Priority | Notes |
|---|---|---|---|
| Single-file SQLite tree with WAL and in-place column migrations | done | P0 | `db::init_db`, `ensure_column` |
| Create, open, close and recent trees | done | P0 | Welcome dashboard, File menu |
| GEDCOM 5.5.1 import | done | P0 | Tolerant parser, encodings, CONC/CONT, warnings |
| Deterministic GEDCOM 5.5.1 export | done | P0 | Stable row order, byte-identical repeat exports |
| Import report with recoverable warnings | done | P1 | Status bar count plus report window |
| Graph view with fit, pan, zoom and selection | done | P0 | Layered layout, long connector styling |
| Watch list with filter and sort | done | P0 | Virtualized table |
| Person inspector: profile, dates, places, notes | done | P0 | Card layout, sticky save |
| Add or link existing parent, spouse or child | done | P0 | Search picker, duplicate/self/cycle checks |
| Date validation and canonical GEDCOM dates | done | P0 | GEDCOM plus ISO input |
| First-or-last-name requirement on manual saves | done | P0 | Drafts may exist until named |
| Theme and font scaling settings | done | P2 | Persisted config |
| Background import and export jobs | done | P1 | UI stays responsive |
| Sources and citations | partial | P0 | Normalised in schema v2; no editor yet |
| Facts and events | partial | P0 | BIRT, DEAT and MARR only |
| Places | partial | P0 | Place records in schema v2; no place page yet |
| Names | partial | P0 | Given, middle, surname, nickname only |
| Notes | partial | P1 | Person level only, no rich text or spell check |
| Search | partial | P1 | Watch-list name filter and relation picker only |
| Media, maps, tasks, groups, merge, reports, backups | planned | P1 | See milestones |
| Online integrations, DNA, AI | deferred | P2 | Opt-in, later phases |

## Milestone map

| --- | Milestone |
|---|---|
| People and editing | M1, M2 |
| Places and maps | M1, M4 |
| Sources and evidence | M1 |
| Media | M1, M4 |
| Research workflow | M3 |
| Publishing | M5 |
| Interoperability and integrations | M6 |
| Platform and polish | M7 |

## M1 - Research-grade data model (P0)

Goal: model genealogy the way research actually works, with sources,
places, facts, alternate names and notes as first-class records.

| Feature | Status | Priority | Notes |
|---|---|---|---|
| Schema v2/v3: sources, repositories, places, media, fact catalogue, tasks, groups | done | P0 | v3 adds source/citation metadata and repository links |
| Citations linked to sources instead of embedded source titles | done | P0 | Full import/export fidelity; editor pending |
| Source templates (Evidence Explained style, start with a small set) | planned | P0 | Extensible template definitions |
| Free-form sources with footnote, short footnote and bibliography | planned | P0 | Fallback when no template fits |
| Citation quality fields (source, information, evidence) | partial | P1 | GEDCOM QUAY imported/exported; Evidence Explained editor pending |
| Reusable citations across people, facts and families | planned | P1 | Copy or reference model |
| Source list with edit, merge and template conversion | planned | P1 | One place to manage documentation |
| Repositories and repository list | partial | P1 | Schema and GEDCOM round-trip; grid pending |
| Place authority: list, reverse ordering, notes, latitude and longitude | planned | P0 | Replaces free-text place duplication |
| Place usage lists and editing people from a place | planned | P1 | See M4 for maps |
| Fact catalogue: 60+ predefined fact types | planned | P0 | Birth, death, marriage first |
| Custom fact types and user-defined roles | planned | P1 | Roles feed shared events |
| Shared events and witnesses | planned | P2 | Associations between participants |
| Age at event | planned | P1 | Derived from dates |
| Private facts and privacy suppression on export | planned | P1 | Living-person handling |
| Alternate names and name pieces (prefix, suffix) | planned | P1 | Also married names |
| Surname-first display option | planned | P2 | Per-file preference |
| Notes on person, family, fact, citation and place | planned | P0 | Current person notes migrate |
| Associations (friends, associates, neighbours) | planned | P2 | With relationship labels |
| Media records and links to people, families, facts, sources and places | partial | P1 | Schema and cascading link tables; UI in M4 |
| Task and group tables | done | P1 | Schema and link tables; UI in M3 |
| Migrations and GEDCOM mapping for every new entity | planned | P0 | Fidelity tests required |

## M2 - Editing and navigation (P0)

| Feature | Status | Priority | Notes |
|---|---|---|---|
| Modeless Edit Person screen with all information in one place | planned | P0 | Can stay open while browsing |
| Add relatives from empty parent, spouse and child slots | planned | P0 | Reuses the link-existing picker |
| Double-click a person to edit | planned | P1 | Graph and list views |
| Switch between family members inside the edit screen | planned | P2 | Spouse and parent switching |
| Pedigree view (5 or 6 generations) | planned | P0 | Ancestor-focused layout |
| Family view | planned | P0 | Couple plus children |
| Descendant view (2 to 7 generations) | planned | P1 | Reuses the graph layout |
| People list view with configurable columns | planned | P1 | Watch list evolves into this |
| Couple list view | planned | P2 | Sortable pairs |
| Association view | planned | P2 | Requires associations from M1 |
| Root person and relationship-to-root display | planned | P1 | Shown in list and info panel |
| Relationship calculator between any two people | planned | P1 | Path search over the tree |
| Duplicate check while adding a person | planned | P1 | Suggest matches before insert |
| Merge duplicates with side-by-side comparison | planned | P0 | Copy fields, merge families, SmartMerge-style pass |
| Backup and restore with timestamped file names | planned | P0 | Media included once M4 lands |
| Database tools: integrity check, rebuild indexes, compress | planned | P1 | Safe, on-demand maintenance |
| Living and privacy flags | planned | P1 | Suppress in exports and reports |

## M3 - Research workflow (P1)

Goal: keep research organised beside the people it concerns.

| Feature | Status | Priority | Notes |
|---|---|---|---|
| Tasks and research logs | planned | P1 | Goals, results, status, priority |
| Link tasks to people, families, events, sources, citations and places | planned | P1 | Filters by linked record |
| Task folders and research-log table view | planned | P1 | Printable |
| Problem alerts (missing or contradictory data) | planned | P1 | Interactive, mark as not a problem |
| DataClean for names and places | planned | P1 | Batch normalisation |
| Advanced person search (facts, dates, places, notes) | planned | P1 | Multiple criteria |
| Saved search criteria | planned | P1 | Reusable rules |
| Find Everywhere across all record types | planned | P2 | Global text search |
| Search and replace for names, places, notes and filenames | planned | P2 | Explicit confirmation |
| Named groups (static and rule-based) | planned | P1 | Used by reports and charts |
| Colour-code sets and labels | planned | P2 | Visual grouping |
| Bookmarks, history and back/forward navigation | planned | P2 | Last N people visited |
| Date calculator and pop-up calendar | planned | P2 | For date entry |
| Soundex calculator | planned | P2 | Several methods |

## M4 - Places and media (P1)

Goal: connect people to locations and sources of evidence beyond text.

| Feature | Status | Priority | Notes |
|---|---|---|---|
| Place list with reverse order and usage counts | planned | P1 | Uses the M1 place authority |
| Edit people and events directly from the place list | planned | P1 | Reduces navigation |
| PlaceClean and place-name standardisation | planned | P2 | Batch workflow |
| Geocoding of places (latitude and longitude) | planned | P2 | Local cache, opt-in service |
| Map view of places and groups | planned | P2 | Requires geocoding |
| Gazetteer search | planned | P2 | Place lookup |
| Media attachments with thumbnails and list modes | planned | P1 | Photos, audio, video, documents |
| Media albums and drag-and-drop organisation | planned | P2 | Per person and general albums |
| Relative media links so trees move between computers | planned | P2 | Portable paths |
| Broken media link repair | planned | P2 | Locate or relink |
| Photo tree and scrapbook | planned | P2 | Publishing-oriented |

## M5 - Publishing (P1/P2)

Goal: turn research into shareable output.

| Feature | Status | Priority | Notes |
|---|---|---|---|
| Pedigree charts and family group sheets | planned | P1 | Core parity items |
| Individual summary and individual list reports | planned | P1 | Facts, parents, spouses, children |
| Ahnentafel and descendant lists (indented and direct) | planned | P1 | Multiple numbering schemes |
| Kinship and relationship reports | planned | P2 | Uses the relationship calculator |
| Birthday, anniversary and calendar reports | planned | P2 | Home page can reuse the data |
| Timeline reports | planned | P2 | Per person or group |
| Place, source, repository and statistics lists | planned | P2 | Administrative reports |
| Duplicate and missing-information reports | planned | P2 | Supports cleanup |
| Fan charts | planned | P1 | Any orientation |
| Hourglass and bow-tie charts | planned | P2 | Combined ancestor and descendant |
| Ancestor and descendant wall charts | planned | P2 | Large format and backgrounds |
| Print customisation: headers, footers, margins, fonts | planned | P2 | Per report |
| Save reports as PDF, DOCX, XLSX, RTF, HTML, EPUB, Markdown | planned | P1 | Start with PDF and HTML |
| Save charts as PDF or SVG | planned | P2 | Vector output |
| Family history books | planned | P2 | Chapter-based composition |
| Static family website export | planned | P2 | Hosted publishing is a non-goal |
| USB or folder share bundle | planned | P2 | Tree plus media |
| AI prompt builder | deferred | P2 | Optional, local prompt generation |

## M6 - Interoperability and integrations (P2, deferred)

Goal: move data between applications and online trees. All network
features are opt-in and can be disabled.

| Feature | Status | Priority | Notes |
|---|---|---|---|
| GEDCOM 7.0 round-trip | deferred | P2 | Keep 5.5.1 support |
| LDS ordinance support | deferred | P2 | Import, export and UI |
| Direct importers: Family Tree Maker, Legacy, PAF, TMG, Family Origins | deferred | P2 | Format-specific parsers |
| FamilySearch Family Tree sync | deferred | P2 | OAuth, review before write |
| Ancestry TreeShare (upload and download) | deferred | P2 | Credentials and conflict review |
| Ancestry WebHints | deferred | P2 | Record hints, user accepts |
| MyHeritage and Findmypast hints | deferred | P2 | Provider-specific |
| DNA matches with Leeds Method clustering | deferred | P2 | Manual match entry first |
| Health conditions and family health history | deferred | P2 | Sensitive data handling |
| File compare and ShareMerge between databases | deferred | P2 | Side-by-side copy |
| Drag and drop people between databases | deferred | P2 | Auto-link option |

## M7 - Platform and polish (P2)

Goal: scale to large trees and make the application comfortable for
long research sessions.

| Feature | Status | Priority | Notes |
|---|---|---|---|
| Multiple tree windows | deferred | P2 | Up to four, multi-monitor |
| Large-tree performance (10,000+ people) | planned | P1 | Layout, search and rendering budgets |
| Virtualized lists and charts | planned | P1 | Avoid rendering off-screen rows and nodes |
| Internationalisation | deferred | P2 | Externalised strings |
| Accessibility and keyboard navigation | deferred | P2 | Focus order, labels, KeyTips |
| High-contrast themes and text scaling | partial | P2 | Font scaling exists, themes extend |
| WebTag and URL links on records | deferred | P2 | External references |
| Rich-text notes and live spell check | deferred | P2 | After notes are first-class |

## Cross-cutting work

- Schema versioning with forward-only migrations and migration tests.
- GEDCOM fidelity harness: import -> export -> import comparisons for
  every new entity.
- Privacy by default: living people and private facts excluded from
  exports and reports unless the user opts in.
- No network access unless a feature is explicitly enabled.
- Performance budgets for 10,000+ person trees in layout, search and
  rendering.
- Data safety: transactions, backups and recoverable failures throughout.

## Non-goals

- Mobile applications.
- Cloud accounts, subscriptions or mandatory sign-in.
- Hosted web publishing as a service; static HTML export is planned.
- Unattended or automatic record matching without user review.
- Storing secrets or provider credentials in the tree file.

## Recommended sequence

1. M1 research-grade data model.
2. M2 editing and navigation.
3. M3 research workflow.
4. M4 places and media.
5. M5 publishing.
6. M6 integrations, only when the local application is complete.
7. M7 platform polish throughout, with performance work starting in M1.
