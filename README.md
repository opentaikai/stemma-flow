# stemma-flow

Local-first genealogy application in Rust. Family data lives in a single
SQLite database with an event-centric, Gramps-style model built for snappy
queries on trees with 10,000+ individuals.

## Build, test, run

```bash
cargo build   # compile
cargo test    # run the unit tests
cargo run     # create/open stemma-flow.db and initialise the schema
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
