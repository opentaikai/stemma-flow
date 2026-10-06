# stemma-flow

Local-first genealogy application in Rust. Family data lives in a single
SQLite database with an event-centric, Gramps-style model built for snappy
queries on trees with 10,000+ individuals.

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
- `db::init_db(&conn)` creates the schema (5 tables, 7 indexes) in a single
  batch transaction: `people`, `families`, `family_children`, `events`,
  `citations`. Databases from older builds are upgraded in place —
  `init_db` adds the `people.middle_name`/`people.nickname`/`people.notes`
  columns (PRAGMA + ALTER) when they are missing, so existing trees keep
  opening.
- `db::{Person, Family, Event, Citation}` are the serde-enabled entity
  structs; ids are UUID strings.
- `db::load_marriage_details(&conn)` maps each family id to its first
  `MARRIAGE` event's date and place, for the inspector's spouse rows.

Relationships are modelled through events and the `family_children` junction
table. Foreign keys cascade deletes from people/families down to events,
citations and child links, while spouses are detached with `ON DELETE SET NULL`.

`db::people` layers transactional person operations on top: `insert_person`,
`update_person`, `delete_person` (the FK cascade removes events, citations
and child links; emptied spouse slots detach), the one-step relational
creators `link_parent`, `link_spouse`, `link_child`, plus `load_vital_dates`
which groups `BIRTH`/`DEATH` event dates and places by person id.

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
  byte-identical). Every other tag the importer recognises but does not
  model — `REFN`, `NOTE`, `OBJE`, unknown level-0 records — is logged as a
  non-fatal `Line N: Tag recognized/unsupported or ignored [TAG value]`
  warning (payload truncated to 100 characters) and the import continues.
- Warnings never block an import: the status bar appends the count
  (`…, N warnings`) to every finished import, and an import that logged
  any warnings also opens the **Import Report** window
  (`gui::import_report`) — a virtualized one-row-per-message list with the
  full text on hover, sized for the thousands of warnings a large
  WikiTree export produces.
- `gedcom::export_to_gedcom(&conn)` serialises `people`, `families`,
  `family_children`, `events` and `citations` back into a deterministic
  GEDCOM 5.5.1 document: a `LINEAGE-LINKED` `HEAD`, `SOUR` records, `INDI`
  and `FAM` records, then `TRLR`.
- Family links are recovered from `HUSB`/`WIFE`/`CHIL`, backfilled from
  `FAMC`, and `FAMS` fills empty spouse slots when `SEX` makes it unambiguous.
- Citations serialise as `0 @S@ SOUR` records (`1 TITL`, `1 NOTE`) that
  citing events reference with `2 SOUR @S@` + `3 PAGE`; re-import restores
  them losslessly, warnings cover dangling source pointers.

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
  open), **Recent**, **Close Tree** (unloads the active database and
  returns to the dashboard), **Settings…** (the appearance dialog) and
  **Quit**. The menu bar keeps a muted
  **Active Tree: &lt;file&gt;** label on its right edge whenever a database
  is loaded.

## Graph View, Watch List & inspector

With a tree open the `eframe` window (1200x800) shows a tab bar with two
views (the app always launches on Graph View):

- **Graph View** — the interactive canvas: **Pan** with the middle mouse
  button or by dragging the background with the left button (drags that
  start on a person do not pan), **Zoom** with the mouse wheel,
  cursor-anchored, 0.3x - 3.0x, and **Select** with a left click; the
  selected node gets a blue accent border. Clicking empty space clears the
  selection.
- **Watch List** — an `egui_extras::TableBuilder` grid listing every
  person with Given Name, Surname, Gender, Birth Date, Death Date and an
  Actions cell. The filter box narrows rows by either name
  (case-insensitive), header clicks toggle ascending/descending sort, rows
  are virtualized for large trees, and clicking a row (or **Edit**)
  selects the person. **Delete** asks for confirmation before the cascading
  delete removes the person, their events, citations and child links.
  **+ Add Independent Person** inserts an unnamed person and focuses it.
- **Person Inspector** — a right-side panel shown whenever a person is
  selected: buffered profile fields (given name, middle name, surname,
  nickname, gender, birth, death) committed with **Save Changes**,
  connection counts, and **+ Add Parent**, **+ Add Spouse**,
  **+ Add Child** buttons whose pop-up creates and links the relative in
  one step (parents choose a Father/Mother role that presets the gender).

`src/gui` builds it from straight SQLite data each time an import or person
mutation finishes:

- `gui::canvas::ViewportState` owns pan/zoom with canvas<->screen
  transforms, cursor-anchored zooming and off-screen culling tests.
- `gui::layout::build_scene` ranks whole couples as one compound (Kahn
  ordering with cycle fallbacks), draws remarriage spouse chains with
  12px union gaps, interlocks sibling subtrees with depth-contour
  packing (24px node gaps), grid-packs disconnected family groups
  toward a 16:9 canvas target, and spaces ranks 120 apart; names are
  truncated to fit the nodes.
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
  `gui::inspector` draws the side panel, the profile editors shared with
  the relation pop-up, and the connection counts.
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

- **Theme** — Light, Dark or System; System follows the OS dark/light
  preference through egui's `ThemePreference`.
- **Font size** — a 10–24pt slider driving `ctx.set_zoom_factor` (14pt
  maps to 1.0), so every tab, panel and window rescales in real time.

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
