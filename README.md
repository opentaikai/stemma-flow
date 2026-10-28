# stemma-flow

Local-first genealogy application in Rust. Family data lives in a single
SQLite database with an event-centric, Gramps-style model built for snappy
queries on trees with 10,000+ individuals.

See [docs/roadmap.md](docs/roadmap.md) for the planned feature roadmap.

## Build, test, run

```bash
cargo build   # compile
cargo test    # run the unit tests
cargo run     # open the desktop window (reopens the last tree, welcome dashboard otherwise)
```

## Storage layer

`src/db` is the local storage layer:

- `db::open_connection(path)` opens a connection with the required pragmas:
  `journal_mode = WAL` and `foreign_keys = ON`.
- `db::init_db(&conn)` creates or upgrades the schema (27 tables, 46
  indexes) and records its version in `PRAGMA user_version` (currently 15):
  `people`, `alternate_names`, `tree_settings`, `notes`, `associations`, `families`, `family_children`, `repositories`, `sources`,
  `source_fields`, `source_repositories`, `source_templates`,
  `source_template_fields`, `places`, `fact_types`, `fact_type_roles`,
  `events`, `event_participants`, `citations`, `citation_links`, `media`,
  `media_links`, `tasks`, `task_links`, `groups` and `group_members`. A v1 database is migrated in place: citation titles
  become `sources`, event places become `places`, and the new links are
  backfilled. v2 databases gain the v3 source/citation metadata, v4 the
  source template catalogue, v5 the free-form citation outputs, v6 the
  reusable citation links, v7 fact scope/kind with the 61-type built-in
  catalogue, v8 per-fact-type roles plus unique GEDCOM tags, v9 shared
  event participants, v10 person/event privacy with living overrides,
  v11 primary name pieces plus alternate names, v12 per-tree display
  settings, v13 first-class notes migrated out of the person, place and
  citation columns, v14 directed person associations, and v15 event
  associations.
  Databases from a newer build are rejected. The upgrade runs on every open (the `gui::window` loader
  validates and migrates before loading), so existing trees keep opening.
- `db::{Person, Family, Event, Citation}` are the serde-enabled entity
  structs; ids are UUID strings. `Event` references a `place_id` and
  `Citation` references a `source_id`.
- `db::load_marriage_details(&conn)` maps each family id to its first
  `MARRIAGE` event's date and place name, for the inspector's spouse rows.
- Sources and places are normalised: `sources` carry title, author,
  publication, abbreviation, notes, data date/text, free-form
  `footnote`/`short_footnote`/`bibliography` and an optional
  `template_id`; `source_repositories` links a source to one or more
  repositories with a call number and media type; `source_fields` stores
  template values, `source_templates` and `source_template_fields` hold the
  built-in Evidence Explained style catalogue. Places carry latitude and
  longitude and are shared by every event that mentions them. `notes` stores
  ordered plain-text notes for people, families, events/facts, citations and
  places; each note has exactly one owner and cascades with it.
  `associations` stores directed person-to-person links with a free-text
  relationship label such as Friend, Neighbour or Godparent;
  `event_associations` stores the same labels on a fact/event.
  `fact_types` carries the 61 built-in person/family event and attribute
  definitions, including their GEDCOM tags and scope; `fact_type_roles`
  holds user-defined roles for each type. `event_participants` links
  additional people to an event with one of those roles. `people` and
  `events` carry private flags, and `people.living_override` stores the
  Auto/Living/Deceased override. `people.name_prefix`/`name_suffix` hold the
  primary GEDCOM name pieces; `alternate_names` stores additional typed
  names, including married names. Custom types use the same tables with
  `is_builtin = 0`. Citations keep page, certainty, media type and data
  date/text; `citation_links` attaches one
  canonical citation to any number of person or family events.
  `load_vital_dates` and
  `load_marriage_details` join the place name for the existing inspector
  and watch-list views. `media` stores external file paths, format,
  caption and notes; typed `media_links` rows attach one media record to a
  person, family, event/fact, source, citation or place and cascade with
  either side. `tasks` stores research goals, logs, status, priority, folder
  and due/completion dates; `task_links` attaches a task to people, families,
  events, sources, citations, places or media. `groups` and `group_members`
  hold static named groups of people. Media and task links are typed,
  cascading rows; tasks and groups are app-only.

Relationships are modelled through events and the `family_children` junction
table. Foreign keys cascade deletes from people/families down to events,
citations and child links, while spouses are detached with `ON DELETE SET NULL`.

`db::people` layers transactional person operations on top: `insert_person`,
`update_person`, `delete_person` (the FK cascade removes events, citations
and child links; emptied spouse slots detach), the one-step relational
creators `link_parent`, `link_spouse`, `link_child`, plus `load_vital_dates`
which groups `BIRTH`/`DEATH` event dates and places by person id.

Manual saves require at least a given name or surname; nameless drafts and
imported nameless individuals are still allowed to exist and display as
`(unnamed)` until named.

Vital dates are GEDCOM date values kept in canonical form. Writes accept
exact dates (`12 JAN 1850`, `JAN 1850`, `1850`), ISO convenience input
(`1850-01-12`, `1850-01`), qualifiers (`ABT`/`CAL`/`EST`/`BEF`/`AFT`),
ranges (`BET … AND …`), periods (`FROM … TO …`, `FROM …`, `TO …`) and
phrases (`(free text)`), normalise them through `model::date`, and reject
impossible calendar days (`31 FEB 1850`) with a field-named error. Ages are
derived from birth and event dates: full dates are exact, partial or
qualified dates show “about”, bounds show “at most”/“at least” and ranges
show a span; phrases, BC/dual years and unusable dates produce no age.

## Graph layer

`src/model` holds the in-memory graph state the `egui` canvas renders:

- `model::GenealogyDb` wraps a `petgraph::stable_graph::StableGraph` plus a
  UUID -> `NodeIndex` map, so removing a person never invalidates the indices
  of everyone else.
- Edges carry `Relationship::ParentChild` (directional) or
  `Relationship::Spouse` (reciprocal); `add_parent_child` rejects self-links
  and cycles so the family graph stays acyclic.
- Traversals: `get_parents`, `get_children`, `get_spouses`, `get_ancestors`.

## GEDCOM 5.5.1

`src/gedcom` streams GEDCOM files in and out of the database:

- `gedcom::import_gedcom(&mut conn, reader)` parses line-by-line from any
  `BufRead` (low memory even for 50MB+ files), translates `@I1@`/`@F1@`
  pointers to UUIDs and writes everything inside a single transaction — I/O
  or database errors roll back the whole import. Recoverable problems
  (dangling pointers, malformed lines, unmapped tags) come back as warnings
  in the returned `ImportReport`.
- Real-world exports are tolerated instead of failing: physical
  `CONC`/`CONT` lines are reassembled byte-wise before UTF-8 decoding
  (wrappers break at a fixed byte width and can split multi-byte
  characters), the `1 CHAR` header selects UTF-8, Windows-1251 or Latin-1,
  and a file that still fails UTF-8 decoding falls back to Windows-1251
  once for the rest of the stream.
- Vendor name tags map onto the person: `2 GIVN`/`2 SURN` refresh the
  given/surname split of `1 NAME`, `2 _MIDN` fills `people.middle_name`
  and `2 _PGVN`/`2 NICK` fill `people.nickname` (export writes those
  children back only when the columns are populated, so plain exports stay
  byte-identical). Every inline `NOTE` on an `INDI`, `FAM`, event/fact or
  citation — including its `CONT`/`CONC` continuations — becomes an ordered
  row in `notes`; multiple notes are kept. Place notes remain app-only.
  Person-level and event-level `ASSO` links keep their `RELA` label,
  including `CONT`/`CONC` continuations; missing labels default to
  `Associate`. Shared `0 @N@ NOTE` records are resolved into each owner's
  notes. Every other tag the importer recognises but does not model —
  `REFN`, unknown level-0 records — is logged as a
  non-fatal `Line N: Tag recognized/unsupported or ignored [TAG value]`
  warning (payload truncated to 100 characters) and the import continues.
- Source records are first class: every `0 @S@ SOUR` keeps its own
  identity (same-titled records are not merged), `AUTH`/`PUBL`/`ABBR`/
  `NOTE`/`DATA` and `REPO`/`CALN` links are stored, and level-0
  `0 @R@ REPO` records become repositories. Citations keep `PAGE`,
  `QUAY`, `NOTE`, `MEDI` and `DATA` (`DATE`/`TEXT`), plus a link to its
  event; a citation data date never overwrites the event date. Inline
  `2 SOUR text` sources are deduplicated only within one import, while
  repeated event citations stay separate records. Shared `0 @M@ OBJE`
  records and inline `OBJE` blocks on people, families, facts, sources,
  citations and places import their `FILE`, `FORM`, `TITL` and `NOTE`
  values; each `FILE` becomes its own media row and relative paths resolve
  against the GEDCOM directory. Custom `0 @P@ _PLACE` records carry place
  coordinates, notes, media and task links even when no event uses the
  place.
- Warnings never block an import: the status bar appends the count
  (`…, N warnings`) to every finished import, and an import that logged
  any warnings also opens the **Import Report** window
  (`gui::import_report`) — a virtualized one-row-per-message list with the
  full text on hover, sized for the thousands of warnings a large
  WikiTree export produces.
- `gedcom::export_to_gedcom(&conn)` serialises `people`, `families`,
  `family_children`, `events`, `sources`, `citations` and linked media back
  into a deterministic GEDCOM 5.5.1 document. Every citation is written for each
  event it supports, including family marriages; the document starts with a
  `LINEAGE-LINKED` `HEAD`, one
  `REPO` record per repository, one `SOUR` record per source with `AUTH`,
  `PUBL`, `ABBR`, `REPO`/`CALN`, `NOTE` and `DATA` (templated sources render
  those columns from their fields when saved), then `INDI` and `FAM`
  records and finally `TRLR`. Events read their `2 PLAC` from the linked
  place record; stored coordinates are written as `3 MAP` with `4 LATI` and
  `4 LONG`. Fact types and attributes such
  as `OCCU`, `CENS`, `BURI` and `DIV` round-trip, including custom types with
  their stored GEDCOM tags; attribute values are stored in
  `events.description`. Multiple `NAME` records round-trip with `NPFX`/
  `NSFX` and `TYPE`, while the first NAME is the primary name. Shared-event
  witnesses use custom `2 _WITN @I@` and
  `3 _ROLE <label>` lines; imported role labels are added to the fact type
  when missing. Privacy filtering happens before serialisation: hidden
  people and private facts are omitted, `RESN`/`_PRIV` markers are written
  when included, and family links are limited to visible people. Stored
  notes travel as inline `NOTE` lines with `CONT` continuations on
  people, families, facts and citations, so notes round-trip losslessly;
  Places also get `_PLACE` records with coordinates, notes, media and task
  links. Associations export as `1 ASSO` + `2 RELA` on people and events and
  are omitted when either person is filtered out. Visible media export as
  shared `0 @M@ OBJE` records with `FILE`/`FORM`/`TITL`/`NOTE`, linked from
  people, families, facts, sources, citations and places; media with no
  visible owner is omitted. Tasks and groups use documented custom records:
  `_TASK` with `TITL`/`NOTE`/`_STATUS`/`_PRIO`/`_FOLDER`/`_DUE`/`_DONE`,
  `_TASK @T@` links on owners, and `_GROUP` records with `_MEMBER @I@`.
  Custom records re-import without warnings but are ignored by other apps. Dates are canonicalised on the way out
  (`12 jan 1850` → `12 JAN 1850`), and stored text that does not parse is
  written as a single-line `(date phrase)` instead, so a dirty date can
  never corrupt a record.
- Family links are recovered from `HUSB`/`WIFE`/`CHIL`, backfilled from
  `FAMC`, and `FAMS` fills empty spouse slots when `SEX` makes it unambiguous.
- `SOUR` records become `sources`; repeated `2 SOUR @S@` pointers share one
  source row, and every citation keeps its own `PAGE`, `QUAY`, `NOTE`,
  `MEDI` and `DATA`. Unreferenced sources are still exported. Re-import
  restores them losslessly, warnings cover dangling source and repository
  pointers and duplicate IDs.

## Native file dialogs & background jobs

- `gui::dialogs::{pick_gedcom_import_path, prompt_gedcom_export_path,
  prompt_new_tree_path}` open the native open/save dialog through `rfd`
  filtered to `*.ged`/`*.gedcom` (save defaults to `family_tree.ged`) or
  `*.db`/`*.sqlite` for new trees; cancelling the dialog yields `None`
  rather than an error.
- `app::StemmaApp` tracks the active tree as `Option<PathBuf>` and runs the
  dialogs, file I/O and SQLite work on a spawned worker thread, reporting
  through an `mpsc` channel that `poll()` drains each frame, so the render
  loop never blocks: `begin_import()` (into the active tree),
  `begin_import_new()` (fresh database), `begin_export()`,
  `activate_tree()`, `close_tree()`, `status()`, `is_busy()`. Person edits
  go through the synchronous `add_person`, `delete_person`, `edit_person`,
  `add_parent`, `add_spouse` and `add_child`, each opening its own
  connection, writing through `db::people` and bumping
  `import_generation()` (also bumped by imports) so every view reloads; the
  canvas-facing `selected_person_id()` tracks the focused person.
- `app::{create_tree, perform_import, perform_export}` expose the
  dialog-free paths (used by the unit tests) for schema creation, streaming
  imports and UTF-8 exports.

## Welcome dashboard & tree lifecycle

On launch the window reopens the last tree it used whenever that database
still loads; otherwise it shows a centered welcome dashboard
(`gui::welcome`) instead of an empty canvas. The view switches the moment
a tree becomes usable:

- **Create New Family Tree** — native save dialog (`*.db`/`*.sqlite`),
  schema initialisation (`app::create_tree`, milliseconds, runs inline) and
  the canvas appears within the same frame.
- **Open Tree** — native open dialog (`*.db`/`*.sqlite`) for an existing
  database; the file is validated first and switches the view on success
  (also available as `File > Open Tree…`).
- **Import GEDCOM File** — background worker picks the source `*.ged`, then
  a fresh target database, runs the streaming importer and activates the
  result through the `TreeReady` job event; progress shows in the status
  bar while the UI stays responsive.
- **Recent trees** — opened or imported databases are remembered (capped
  at 8, deduplicated) through `eframe`'s built-in persistence and listed on
  the dashboard and under `File > Recent`; clicking one re-opens it, and
  entries whose file no longer loads are dropped automatically. The most
  recent entry is what the app reopens on launch.
- The `File` menu lists **New Family Tree…**, **Open Tree…**,
  **Import GEDCOM…** (into a fresh database from the dashboard; into the
  open tree once one is active), **Export GEDCOM…** (enabled once a tree is
  open; opens a privacy options dialog before the file picker), **Recent**,
  **Close Tree** (unloads the active database and
  returns to the dashboard), **Settings…** (the appearance dialog) and
  **Quit**. The menu bar keeps a muted
  **Active Tree: &lt;file&gt;** label on its right edge whenever a database
  is loaded.

GEDCOM export excludes living people and private people/facts by default.
The export dialog can include either category and adjusts the automatic
living-age threshold (default 110 years); manual living overrides always
win. Included private or living people carry `1 RESN privacy`, and
included private events carry `2 _PRIV Y`. Family links to hidden people
are removed, and families with no visible member are not exported.

## Graph View, Watch List, Places, Media, Research, Fact Types, Sources & inspector

With a tree open the `eframe` window (1200x800) shows a tab bar with eight
views (the app always launches on Graph View):

- **Graph View** — the interactive canvas: opening (or reloading) a tree
  **fits the whole scene** inside the viewport, then **Pan** with the
  middle mouse button or by dragging the background with the left button
  (drags that start on a person do not pan), **Zoom** with the mouse
  wheel, cursor-anchored, 0.02x - 3.0x, and **Select** with a left click;
  the selected node gets a blue accent border. Clicking empty space clears
  the selection.
- **Watch List** — an `egui_extras::TableBuilder` grid listing every
  person with Given Name, Surname, Gender, Birth Date, Death Date and an
  Actions cell. The filter box narrows rows by either name
  (case-insensitive), header clicks toggle ascending/descending sort, rows
  are virtualized for large trees, and clicking a row (or **Edit**)
  selects the person. **Delete** asks for confirmation before the cascading
  delete removes the person, their events, citations and child links.
  **+ Add Independent Person** inserts an unnamed draft and focuses it; a
  first or last name is required before it can be saved.
- **View > Surname-first names** — a per-tree toggle that renders composed
  names as `Surname, Prefix Given Middle Suffix` (for example
  `King, Dr. Ada Lynn Jr.`) in graph labels, the watch list's composed name,
  the inspector, relation and witness pickers, place usage and alternate
  names. The separate Given Name and Surname columns keep their normal
  order, and GEDCOM output stays canonical. The preference lives in the
  tree database, so reopening the file restores it.
- **Notes** — one reusable dialog lists a record's notes and supports
  adding, editing and deleting them. It opens from the person **Notes**
  card, the **Families** card, vital and other fact rows, each citation and
  the place editor. Notes are plain text and save independently of the
  person form.
- **Places** — a filterable, sortable grid of the tree's shared place
  records with event counts, notes, latitude and longitude. The editor
  changes the canonical name and decimal-degree coordinates; renaming
  a place updates every event that references it. Deleting a place asks for
  confirmation and clears the place from its events. The editor also lists
  every event that uses the place; clicking a person or either marriage
  spouse opens their inspector without leaving the Places tab. Read-only
  place displays can show comma components in reverse order; the stored
  name and GEDCOM output keep the canonical order. Coordinates round-trip through
  `MAP`/`LATI`/`LONG`; place notes open in the shared notes dialog and
  remain local to the tree.
- **Media** — a filterable media list with cached image thumbnails and
  format/path fallbacks, a missing-file marker and link counts. The editor
  stores an external file path, format, caption and notes; **Browse…**
  fills path/format/caption from a chosen file. Deleting a record cascades
  its attachments. **Media…** dialogs on person, family, fact, source and
  place records list attached media, attach existing records, add a file
  directly and detach links. Citation/place media now round-trip through
  GEDCOM; relative paths, copying files into the tree and broken-link repair
  remain deferred.
- **Research** — sub-tabs for **Tasks** and **Groups**. Tasks form a
  research log with title/goal, description/results, open/done status,
  priority, folder, due date, completion time and linked-record counts;
  filters cover text, status and folder. Groups are static named people
  groups with an editor, member search and add/remove actions. **Tasks…**
  dialogs on people, families, facts, citations, sources, places and media
  attach existing tasks and detach links. Tasks and groups remain app-only.
- **Fact Types** — a filterable, sortable grid of the built-in and custom
  fact types with category, scope, kind, GEDCOM tag, role count and event
  usage, plus an editor. Custom types get an immutable generated key and a
  unique GEDCOM tag; built-in metadata is read-only. Roles are defined per
  fact type (for example Owner or Witness) and feed shared-event witnesses;
  deleting a custom type is blocked while events use it. Custom tags
  round-trip through GEDCOM.
- **Sources** — a searchable list of the tree's sources with citation
  counts, plus an editor beside it. A source is either free-form or uses one
  of five built-in Evidence Explained style templates (book, census,
  certificate, website, cemetery memorial). Free-form sources store their
  own footnote, short footnote and bibliography; the per-citation page is
  appended (or substituted for `{page}`) in the live preview. Templated
  sources show the same three forms from their fields and cache their mapped
  fields into the GEDCOM-compatible source columns on save; required fields
  disable saving until filled. Sources can be merged: the survivor keeps its
  template and non-empty values, while citations, repositories, fields and
  media/task links move from the absorbed source. Switching templates carries
  matching values, seeds title and author, and replaces free-form citation
  text with the template rendering; the live preview shows the result before
  saving. Citations are attached, reused and rated from the inspector's
  Citations card.
- **Repositories** — a filterable, sortable grid of repositories with linked
  source counts, plus an editor for name, multiline address and notes and the
  linked sources (call number and media type). Links are added through a
  source picker and removed from the editor; deleting a repository asks for
  confirmation and removes its source links. Repository metadata and links
  round-trip through GEDCOM.
- **Person Inspector** — a right-side panel shown whenever a person is
  selected, with a pinned **×** button that closes it by clearing the
  selection. It is laid out as framed cards in a scrolling body with the
  accent **Save Changes** footer pinned below, enabled only once a first
  or last name is present: a **Header** card with the
  person's name and gender, **Profile** (prefix, given name, middle name,
  surname, suffix, nickname, gender), a **Names** card for typed alternate
  names (including Married) and **Vital Facts** (birth/death dates *and places*)
  editors — the date fields accept GEDCOM or ISO input, and an invalid
  date shows an inline message and disables saving — an **Other Facts**
  card listing imported non-vital facts with their labels, dates, places,
  values and derived age when the birth date is usable; facts the person
  witnessed also appear with their role and principal, and **Add/Edit
  witnesses** opens a participant editor with people search and the fact
  type's roles. Birth/death facts stay in Vital Facts unless they have
  witnesses; Vital Facts also shows the age at death. Spouse rows carry a
  **Witnesses** action for the marriage event and the selected person's age
  at that marriage. A **Privacy** card marks the person private, overrides
  auto living status (Auto/Living/Deceased) and lists every own or family
  event with a private checkbox. A **Citations** card
  that attaches a source to the birth, death or a family marriage event and
  records the page, the Evidence Explained source/information/evidence
  quality and the GEDCOM certainty (each optionally "Don't know"). It can
  reuse an existing citation of the source; shared citations show how many
  facts use them, detach from one fact or delete everywhere after a
  confirmation, and edits reach every use. A
  **Relationships** card listing parents, siblings, spouses and
  children —
  rows select that relative on the graph and re-centre the view, spouse
  rows carry the marriage date and place when a `MARRIAGE` event exists —
  with **+ Add Parent**, **+ Add Spouse**,
  **+ Add Child** buttons whose pop-up can **search and link an existing
  person** (filtered by role; duplicates, self links and cycles are
  rejected) or create a new one in one step (parents choose a
  Father/Mother role that presets the gender). A **Families** card lists
  every family the person belongs to, an **Associations** card lists
  directed links with relationship labels and opens a dialog to search
  people and add, edit or delete them, **Vital Facts** and **Other Facts**
  expose their event notes and media, a **Media** card opens the attachment dialog and a **Tasks** card opens
  the task linker; the **Notes** card opens a shared dialog for adding,
  editing and deleting notes. Profile and vital fields still
  commit together on save; notes save independently.

`src/gui` builds it from straight SQLite data each time an import or person
mutation finishes; the three largest areas — `gui::layout`, `gui::window`
and `gui::inspector` — are split into focused submodules (scene, graph,
packing, compounds, placement / loading, lifecycle, panels, actions /
form, connections, panel, relation), with the public API and tests kept in
each module's `mod.rs`:

- `gui::canvas::ViewportState` owns pan/zoom with canvas<->screen
  transforms, cursor-anchored zooming (`fit_to` scales and centres a
  bounding box on open) and off-screen culling tests.
- `gui::layout::build_scene` ranks whole couples as one compound (Kahn
  ordering with cycle fallbacks), draws remarriage spouse chains with
  12px union gaps, interlocks sibling subtrees with depth-contour
  packing (24px node gaps), grid-packs disconnected family groups
  toward a 16:9 canvas target (each group box is *measured* from its
  placed nodes, not summed), and spaces ranks 120 apart; names are
  truncated to fit the nodes. A single large connected group still lays
  out as one wide, shallow chart, which the open-time fit then scales to
  the window.
- `gui::connectors::build_segments` routes spouse bars and three-segment
  orthogonal drops (union -> trunk row inside the rank gap -> children)
  as pure geometry.
- `gui::nodes` hit-tests clicks back-to-front and paints nodes, borders
  and zoom-scaled names, culling everything outside the viewport.
- `gui::welcome::show` renders the dashboard card (logo, title, tagline,
  actions, recent list) and returns a `WelcomeAction` for the shell to
  execute.
- `gui::tabs` renders the tab row, `gui::watchlist` owns the grid's
  filter/sort state and emits selection/add/delete events, and
  `gui::inspector` draws the side panel as framed, titled cards (header,
  profile, vitals, relationships, notes) around the profile editors
  shared with the relation pop-up, and the relationship lists with
  click-to-focus navigation (`InspectorAction::Focus` re-selects the
  person, switches to the graph tab and centres the viewport on the
  node).
- `gui::import_report` renders the **Import Report** window opened after
  a warning-bearing import (virtualized rows, close button, hover for the
  full message), and `gui::settings` renders the `File > Settings…` dialog
  with `gui::theme::apply_config_to_ctx` pushing the chosen theme preference and
  zoom factor into the `egui::Context`; both run live, no restart needed.
- `gui::window::TreeWindow` wires it into `eframe::App`: the `File`
  menu, tab switching, background-job polling, scene reloads (a quiet
  variant keeps pan/zoom through profile edits), watch-list events, the
  inspector panel and relation pop-up, recent-tree persistence, the
  startup settings apply and input handling.

## Settings & appearance

`File > Settings…` opens a modal dialog with an **Appearance** section:

- **Language** — English (default) or Deutsch, applied live and persisted
  in `config.toml`.
- **Theme** — Light, Dark or System; System follows the OS dark/light
  preference through egui's `ThemePreference`.
- **Font size** — a 10–24pt slider driving `ctx.set_zoom_factor` (14pt
  maps to 1.0), so every tab, panel and window rescales in real time.
- **Places** — **Reverse place-name order** shows comma-separated place
  components largest-first in read-only displays; editors and GEDCOM keep
  the stored order.

## Translations

Interface strings live in external `locales/*.toml` catalogs. `build.rs`
discovers every catalog and embeds it in the binary, so adding a language
only requires a new file with the same key set as `locales/en.toml`.
Catalogs use nested tables flattened to namespaced keys, `{name}`
placeholders and `_one`/`_other` plural variants. The active catalog is
process-wide and read every frame, which makes the Settings language
selector update all windows without a restart. Missing keys and unsupported
config codes fall back to English. Tests enforce key and placeholder parity
between English and German.

Theme changes apply to the context immediately and persist to
`<config_dir>/stemma-flow/config.toml` resolved with `dirs::config_dir()`
(`~/.config/stemma-flow/config.toml` on Linux). The font-size slider stays
live in the dialog but only rescales the app once the mouse button is
released, and its number input commits only on Enter or when focus moves
away (Escape cancels) — so neither dragging nor typing makes the
interface jump (one disk write per commit instead of one per drag frame).
`src/config.rs` creates
the file on first launch, rewrites it when missing or corrupt, resets
insane font sizes to the 14pt baseline, and writes atomically through a
temporary file plus rename. Read/write failures fall back to in-memory
defaults — a read-only filesystem only surfaces a status-bar message and
never crashes the app. The stored config is loaded when the window starts
and applied before the first frame paints, so preferences survive
restarts without any manual step.
