# stemma-flow

Local-first genealogy application in Rust. Family data lives in a single
SQLite database with an event-centric, Gramps-style model built for snappy
queries on trees with 10,000+ individuals.

## Build, test, run

```bash
cargo build   # compile
cargo test    # run the unit tests
cargo run     # open the desktop window (welcome dashboard on first launch)
```

## Storage layer

`src/db` is the local storage layer:

- `db::open_connection(path)` opens a connection with the required pragmas:
  `journal_mode = WAL` and `foreign_keys = ON`.
- `db::init_db(&conn)` creates the schema (5 tables, 7 indexes) in a single
  batch transaction: `people`, `families`, `family_children`, `events`,
  `citations`.
- `db::{Person, Family, Event, Citation}` are the serde-enabled entity
  structs; ids are UUID strings.

Relationships are modelled through events and the `family_children` junction
table. Foreign keys cascade deletes from people/families down to events,
citations and child links, while spouses are detached with `ON DELETE SET NULL`.

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
  (dangling pointers, malformed lines) come back as warnings in the returned
  `ImportReport`.
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
  `activate_tree()`, `close_tree()`, `status()`, `is_busy()`, plus the
  canvas-facing `selected_person_id()` and `import_generation()` (bumped
  on every successful import so the tree can reload itself).
- `app::{create_tree, perform_import, perform_export}` expose the
  dialog-free paths (used by the unit tests) for schema creation, streaming
  imports and UTF-8 exports.

## Welcome dashboard & tree lifecycle

On launch the window starts **without** an active tree and shows a centered
welcome dashboard (`gui::welcome`) instead of an empty canvas. The view
switches the moment a tree becomes usable:

- **Create New Family Tree** — native save dialog (`*.db`/`*.sqlite`),
  schema initialisation (`app::create_tree`, milliseconds, runs inline) and
  the canvas appears within the same frame.
- **Import GEDCOM File** — background worker picks the source `*.ged`, then
  a fresh target database, runs the streaming importer and activates the
  result through the `TreeReady` job event; progress shows in the status
  bar while the UI stays responsive.
- **Recent trees** — opened or imported databases are remembered (capped
  at 8, deduplicated) through `eframe`'s built-in persistence and listed on
  the dashboard and under `File > Recent`; clicking one re-opens it, and
  entries whose file no longer loads are dropped automatically.
- The `File` menu also offers **Close Tree** (unloads the active database
  and returns to the dashboard) and **Quit**.

## Interactive tree canvas

With a tree open the `eframe` window (1200x800) shows a toolbar
(Import into tree/Export GEDCOM, selection indicator), a status bar and
the tree:

- **Pan** with the middle mouse button or by dragging the background with
  the left button (drags that start on a person do not pan).
- **Zoom** with the mouse wheel, cursor-anchored, 0.3x - 3.0x.
- **Select** with a left click; the selected node gets a blue accent
  border and the toolbar shows their name. Clicking empty space clears
  the selection.

`src/gui` builds it from straight SQLite data each time an import
finishes:

- `gui::canvas::ViewportState` owns pan/zoom with canvas<->screen
  transforms, cursor-anchored zooming and off-screen culling tests.
- `gui::layout::build_scene` layers generations (Kahn ordering with
  cycle fallbacks), places couples side by side and centres children
  under the family union point; names are truncated to fit the nodes.
- `gui::connectors::build_segments` routes spouse bars and three-segment
  orthogonal drops (union -> branch row -> children) as pure geometry.
- `gui::nodes` hit-tests clicks back-to-front and paints nodes, borders
  and zoom-scaled names, culling everything outside the viewport.
- `gui::welcome::show` renders the dashboard card (logo, title, tagline,
  actions, recent list) and returns a `WelcomeAction` for the shell to
  execute.
- `gui::window::TreeWindow` wires it into `eframe::App`: the menu bar and
  toolbar, welcome/canvas viewport switching, background-job polling,
  scene reloads, recent-tree persistence and input handling.
