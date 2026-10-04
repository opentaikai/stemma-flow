//! Application state and background job orchestration for import/export.
//!
//! Native file dialogs and file I/O run on a spawned worker thread so the
//! egui render loop stays responsive; results travel back through an
//! `mpsc` channel that [`StemmaApp::poll`] drains every frame.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use crate::db;
use crate::gedcom::{ImportReport, export_to_gedcom, import_gedcom};
use crate::gui::dialogs;

/// Outcome of a background import/export job.
#[derive(Debug)]
pub enum JobEvent {
    Imported(ImportReport),
    /// A freshly created tree database finished importing; the UI should
    /// make it the active tree.
    TreeReady {
        report: ImportReport,
        path: PathBuf,
    },
    Exported {
        bytes: usize,
        path: PathBuf,
    },
    Cancelled,
    Failed(String),
}

/// Presentation state derived from the most recent job event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiStatus {
    Idle,
    Running(&'static str),
    Success(String),
    Cancelled,
    Error(String),
}

enum Job {
    /// Stream a GEDCOM file into the already active tree.
    Import,
    /// Pick a source file and a fresh target database, then import into it.
    ImportNew,
    Export,
}

/// Application state shared between the UI shell and job workers.
pub struct StemmaApp {
    active_db: Option<PathBuf>,
    busy: bool,
    status: UiStatus,
    rx: Option<Receiver<JobEvent>>,
    selected_person_id: Option<String>,
    import_generation: u64,
}

impl StemmaApp {
    /// Starts without an active tree; the welcome dashboard shows until
    /// [`Self::activate_tree`] or a background import opens one.
    pub fn new() -> Self {
        Self {
            active_db: None,
            busy: false,
            status: UiStatus::Idle,
            rx: None,
            selected_person_id: None,
            import_generation: 0,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn status(&self) -> &UiStatus {
        &self.status
    }

    /// The tree database currently open, if any.
    pub fn active_db(&self) -> Option<&Path> {
        self.active_db.as_deref()
    }

    /// Makes `path` the active tree and clears any stale selection.
    pub fn activate_tree(&mut self, path: impl Into<PathBuf>) {
        self.active_db = Some(path.into());
        self.selected_person_id = None;
    }

    /// Unloads the active tree so the welcome dashboard takes over.
    pub fn close_tree(&mut self) {
        self.active_db = None;
        self.selected_person_id = None;
    }

    /// Person under the most recent canvas click, if any.
    pub fn selected_person_id(&self) -> Option<&str> {
        self.selected_person_id.as_deref()
    }

    /// Records the canvas selection; `None` clears it.
    pub fn set_selected_person(&mut self, person_id: Option<String>) {
        self.selected_person_id = person_id;
    }

    /// Incremented on every successful import so the canvas knows when to
    /// rebuild its scene.
    pub fn import_generation(&self) -> u64 {
        self.import_generation
    }

    /// Streams the chosen file into the active tree on a worker thread.
    pub fn begin_import(&mut self) {
        if self.active_db.is_none() {
            return;
        }
        self.begin(Job::Import, self.active_db.clone());
    }

    /// Picks a source GEDCOM plus a brand-new target database and imports
    /// into it on a worker thread; the resulting tree becomes active.
    pub fn begin_import_new(&mut self) {
        self.begin(Job::ImportNew, None);
    }

    /// Writes the active database as GEDCOM on a worker thread.
    pub fn begin_export(&mut self) {
        if self.active_db.is_none() {
            return;
        }
        self.begin(Job::Export, self.active_db.clone());
    }

    fn begin(&mut self, job: Job, db_path: Option<PathBuf>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = UiStatus::Running(match job {
            Job::Import | Job::ImportNew => "Choosing file to import\u{2026}",
            Job::Export => "Choosing export destination\u{2026}",
        });
        let (tx, rx) = channel();
        self.rx = Some(rx);
        thread::spawn(move || job.run(db_path, &tx));
    }

    /// Applies every job event that arrived since the last frame.
    ///
    /// Returns `true` when at least one event was applied.
    pub fn poll(&mut self) -> bool {
        let events: Vec<JobEvent> = match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => return false,
        };
        if events.is_empty() {
            return false;
        }
        for event in events {
            self.apply_event(event);
        }
        true
    }

    /// Applies a single job event to the presentation state.
    pub fn apply_event(&mut self, event: JobEvent) {
        self.busy = false;
        self.status = match event {
            JobEvent::Imported(report) => {
                self.import_generation += 1;
                UiStatus::Success(format!(
                    "Imported {} people, {} families, {} events, {} citations",
                    report.people, report.families, report.events, report.citations
                ))
            }
            JobEvent::TreeReady { report, path } => {
                self.activate_tree(path);
                self.import_generation += 1;
                UiStatus::Success(format!(
                    "Imported {} people, {} families, {} events, {} citations",
                    report.people, report.families, report.events, report.citations
                ))
            }
            JobEvent::Exported { bytes, path } => {
                UiStatus::Success(format!("Exported {bytes} bytes to {}", path.display()))
            }
            JobEvent::Cancelled => UiStatus::Cancelled,
            JobEvent::Failed(message) => UiStatus::Error(message),
        };
    }
}

impl Default for StemmaApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Job {
    fn run(self, db_path: Option<PathBuf>, tx: &Sender<JobEvent>) {
        let event = self.run_with(db_path);
        let _ = tx.send(event);
    }

    fn run_with(self, db_path: Option<PathBuf>) -> JobEvent {
        match self {
            Job::Import => {
                let Some(db_path) = db_path else {
                    return JobEvent::Failed("No tree is open".to_string());
                };
                match dialogs::pick_gedcom_import_path() {
                    None => JobEvent::Cancelled,
                    Some(source) => match perform_import(&db_path, &source) {
                        Ok(report) => JobEvent::Imported(report),
                        Err(message) => JobEvent::Failed(message),
                    },
                }
            }
            Job::ImportNew => match dialogs::pick_gedcom_import_path() {
                None => JobEvent::Cancelled,
                Some(source) => match dialogs::prompt_new_tree_path() {
                    None => JobEvent::Cancelled,
                    Some(target) => match perform_import(&target, &source) {
                        Ok(report) => JobEvent::TreeReady {
                            report,
                            path: target,
                        },
                        Err(message) => JobEvent::Failed(message),
                    },
                },
            },
            Job::Export => {
                let Some(db_path) = db_path else {
                    return JobEvent::Failed("No tree is open".to_string());
                };
                match dialogs::prompt_gedcom_export_path() {
                    None => JobEvent::Cancelled,
                    Some(dest) => match perform_export(&db_path, &dest) {
                        Ok(bytes) => JobEvent::Exported { bytes, path: dest },
                        Err(message) => JobEvent::Failed(message),
                    },
                }
            }
        }
    }
}

/// Creates a fresh database file at `path` with the schema initialised.
///
/// This is the dialog-free half of the "Create New Family Tree" action.
pub fn create_tree(path: &Path) -> Result<(), String> {
    let conn = db::open_connection(path).map_err(|error| error.to_string())?;
    db::init_db(&conn).map_err(|error| error.to_string())
}

/// Opens `source` and streams it into the database at `db_path`.
pub fn perform_import(db_path: &Path, source: &Path) -> Result<ImportReport, String> {
    let mut conn = db::open_connection(db_path).map_err(|error| error.to_string())?;
    db::init_db(&conn).map_err(|error| error.to_string())?;
    let file = std::fs::File::open(source).map_err(|error| error.to_string())?;
    import_gedcom(&mut conn, std::io::BufReader::new(file)).map_err(|error| error.to_string())
}

/// Serializes the database at `db_path` into a UTF-8 GEDCOM file at `dest`.
///
/// Returns the number of bytes written.
pub fn perform_export(db_path: &Path, dest: &Path) -> Result<usize, String> {
    let conn = db::open_connection(db_path).map_err(|error| error.to_string())?;
    db::init_db(&conn).map_err(|error| error.to_string())?;
    let gedcom = export_to_gedcom(&conn).map_err(|error| error.to_string())?;
    std::fs::write(dest, gedcom.as_bytes()).map_err(|error| error.to_string())?;
    Ok(gedcom.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
0 HEAD
1 SOUR TEST
0 @I1@ INDI
1 NAME Johan /Ahlberg/
1 SEX M
1 FAMS @F1@
0 @I2@ INDI
1 NAME Maria /Ekdahl/
1 SEX F
1 FAMS @F1@
0 @I3@ INDI
1 NAME Elsa /Ahlberg/
1 SEX F
1 FAMC @F1@
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I2@
1 CHIL @I3@
0 TRLR
";

    fn temp_path(extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "stemma-flow-{}.{}",
            uuid::Uuid::new_v4(),
            extension
        ))
    }

    #[test]
    fn exported_file_imports_into_identical_connections() -> Result<(), String> {
        let source = temp_path("ged");
        let seed_db = temp_path("db");
        let export_db = temp_path("db");
        let export_file = temp_path("ged");

        std::fs::write(&source, SAMPLE).map_err(|error| error.to_string())?;
        let report = perform_import(&seed_db, &source)?;
        assert_eq!((report.people, report.families), (3, 1));

        perform_export(&seed_db, &export_file)?;
        let reimported = perform_import(&export_db, &export_file)?;
        assert_eq!(
            (
                reimported.people,
                reimported.families,
                reimported.child_links
            ),
            (3, 1, 1),
            "connections must be restored from the exported file"
        );

        let original = connections_of(&seed_db)?;
        let restored = connections_of(&export_db)?;
        assert_eq!(
            original, restored,
            "parent/child and spouse links must match exactly"
        );

        let second_export = temp_path("ged");
        perform_export(&export_db, &second_export)?;
        let first = std::fs::read(&export_file).map_err(|error| error.to_string())?;
        let again = std::fs::read(&second_export).map_err(|error| error.to_string())?;
        assert_eq!(first, again, "re-export must be byte identical");

        for path in [&source, &seed_db, &export_db, &export_file, &second_export] {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    #[test]
    fn failed_import_reports_error_without_panic() {
        let missing = temp_path("db");
        let result = perform_import(&missing, Path::new("/nonexistent/file.ged"));
        assert!(result.is_err(), "missing input file must fail cleanly");
        let _ = std::fs::remove_file(missing);
    }

    #[test]
    fn poll_applies_events_and_tracks_import_generation() {
        let mut app = StemmaApp::new();
        assert!(!app.poll(), "without a channel there is nothing to apply");
        assert_eq!(app.import_generation(), 0);
        assert_eq!(app.selected_person_id(), None);
        assert_eq!(app.active_db(), None, "starts without a tree");

        app.activate_tree("stemma-flow.db");
        assert_eq!(app.active_db(), Some(Path::new("stemma-flow.db")));
        assert_eq!(
            app.import_generation(),
            0,
            "opening a tree does not rewrite data"
        );

        let (tx, rx) = channel();
        app.rx = Some(rx);
        let _ = tx.send(JobEvent::Imported(ImportReport {
            people: 2,
            families: 1,
            child_links: 1,
            events: 0,
            citations: 0,
            warnings: Vec::new(),
        }));
        assert!(app.poll(), "queued events must be applied");
        assert_eq!(app.import_generation(), 1, "imports bump the generation");
        assert!(!app.poll(), "a drained channel applies nothing");

        app.set_selected_person(Some("i1".to_string()));
        assert_eq!(app.selected_person_id(), Some("i1"));
        app.close_tree();
        assert_eq!(app.active_db(), None, "closing returns to the dashboard");
        assert_eq!(app.selected_person_id(), None, "closing clears selection");
    }

    #[test]
    fn tree_ready_event_activates_the_new_database() {
        let mut app = StemmaApp::new();
        app.busy = true;
        app.apply_event(JobEvent::TreeReady {
            report: ImportReport {
                people: 4,
                families: 1,
                child_links: 2,
                events: 0,
                citations: 0,
                warnings: Vec::new(),
            },
            path: PathBuf::from("/tmp/fresh.db"),
        });
        assert_eq!(app.active_db(), Some(Path::new("/tmp/fresh.db")));
        assert_eq!(
            app.import_generation(),
            1,
            "the fresh import marks the scene stale"
        );
        assert!(matches!(app.status(), UiStatus::Success(_)));
    }

    #[test]
    fn begin_without_an_active_tree_is_a_noop() {
        let mut app = StemmaApp::new();
        app.begin_import();
        app.begin_export();
        assert!(!app.is_busy(), "jobs need an open tree");
        assert_eq!(app.status(), &UiStatus::Idle);
    }

    #[test]
    fn create_tree_initialises_an_empty_schema() -> Result<(), String> {
        let path = temp_path("db");
        create_tree(&path)?;
        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        let tree = db::load_family_tree(&conn).map_err(|error| error.to_string())?;
        assert!(tree.people.is_empty(), "a fresh tree has no people");
        assert!(tree.families.is_empty(), "a fresh tree has no families");
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn apply_event_drives_status_transitions() {
        let mut app = StemmaApp::new();
        assert_eq!(app.status(), &UiStatus::Idle);
        assert!(!app.is_busy());

        app.busy = true;
        app.apply_event(JobEvent::Imported(ImportReport {
            people: 3,
            families: 1,
            child_links: 1,
            events: 0,
            citations: 0,
            warnings: Vec::new(),
        }));
        assert!(!app.is_busy());
        assert!(matches!(app.status(), UiStatus::Success(message) if message.contains("3 people")));

        app.busy = true;
        app.apply_event(JobEvent::Cancelled);
        assert!(!app.is_busy());
        assert_eq!(app.status(), &UiStatus::Cancelled);

        app.busy = true;
        app.apply_event(JobEvent::Failed("disk full".to_string()));
        assert_eq!(app.status(), &UiStatus::Error("disk full".to_string()));
    }

    /// Renders the family links as names so two databases with freshly
    /// generated UUIDs can be compared structurally.
    fn connections_of(db_path: &Path) -> Result<String, String> {
        let conn = db::open_connection(db_path).map_err(|error| error.to_string())?;
        let spouses: String = conn
            .query_row(
                "SELECT GROUP_CONCAT(COALESCE(h.given_name, '-') || '<>' || COALESCE(w.given_name, '-'))
                 FROM families f
                 LEFT JOIN people h ON h.id = f.husband_id
                 LEFT JOIN people w ON w.id = f.wife_id",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let children: String = conn
            .query_row(
                "SELECT GROUP_CONCAT(p.given_name)
                 FROM family_children fc
                 JOIN people p ON p.id = fc.child_id",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(format!("{spouses}|{children}"))
    }
}
