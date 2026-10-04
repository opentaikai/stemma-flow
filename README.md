# stemma-flow

Local-first genealogy application in Rust. Family data lives in a single
SQLite database with an event-centric, Gramps-style model built for snappy
queries on trees with 10,000+ individuals.

## Build, test, run

```bash
cargo build   # compile
cargo test    # run the unit tests
cargo run     # open the desktop window (creates stemma-flow.db on first run)
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

- `gui::dialogs::{pick_gedcom_import_path, prompt_gedcom_export_path}` open
  the native open/save dialog through `rfd` filtered to `*.ged`/`*.gedcom`
  (save defaults to `family_tree.ged`); cancelling the dialog yields `None`
  rather than an error.
- `app::StemmaApp` runs the dialog, file I/O and SQLite work on a spawned
  worker thread and reports through an `mpsc` channel that `poll()` drains
  each frame, so the render loop never blocks: `begin_import()`,
  `begin_export()`, `status()`, `is_busy()`, plus the canvas-facing
  `selected_person_id()` and `import_generation()` (bumped on every
  successful import so the tree can reload itself).
- `app::{perform_import, perform_export}` expose the dialog-free worker path
  (used by the unit tests) for streaming imports and UTF-8 exports.

## Interactive tree canvas

`cargo run` opens an `eframe` window (1200x800) with a toolbar
(Import/Export GEDCOM, selection indicator), a status bar and the tree:

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
- `gui::window::TreeWindow` wires it into `eframe::App`: toolbar
  actions, background-job polling, scene reloads and input handling.
