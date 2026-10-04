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
    Exported { bytes: usize, path: PathBuf },
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
    Import,
    Export,
}

/// Application state shared between the UI shell and job workers.
pub struct StemmaApp {
    db_path: PathBuf,
    busy: bool,
    status: UiStatus,
    rx: Option<Receiver<JobEvent>>,
}

impl StemmaApp {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            busy: false,
            status: UiStatus::Idle,
            rx: None,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn status(&self) -> &UiStatus {
        &self.status
    }

    /// Opens the native open dialog and streams the chosen file into the
    /// database on a worker thread.
    pub fn begin_import(&mut self) {
        self.begin(Job::Import);
    }

    /// Opens the native save dialog and writes the database as GEDCOM on a
    /// worker thread.
    pub fn begin_export(&mut self) {
        self.begin(Job::Export);
    }

    fn begin(&mut self, job: Job) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = UiStatus::Running(match job {
            Job::Import => "Choosing file to import\u{2026}",
            Job::Export => "Choosing export destination\u{2026}",
        });
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let db_path = self.db_path.clone();
        thread::spawn(move || job.run(db_path, &tx));
    }

    /// Applies every job event that arrived since the last frame.
    pub fn poll(&mut self) {
        let events: Vec<JobEvent> = match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        for event in events {
            self.apply_event(event);
        }
    }

    /// Applies a single job event to the presentation state.
    pub fn apply_event(&mut self, event: JobEvent) {
        self.busy = false;
        self.status = match event {
            JobEvent::Imported(report) => UiStatus::Success(format!(
                "Imported {} people, {} families, {} events, {} citations",
                report.people, report.families, report.events, report.citations
            )),
            JobEvent::Exported { bytes, path } => {
                UiStatus::Success(format!("Exported {bytes} bytes to {}", path.display()))
            }
            JobEvent::Cancelled => UiStatus::Cancelled,
            JobEvent::Failed(message) => UiStatus::Error(message),
        };
    }
}

impl Job {
    fn run(self, db_path: PathBuf, tx: &Sender<JobEvent>) {
        let event = match self {
            Job::Import => match dialogs::pick_gedcom_import_path() {
                None => JobEvent::Cancelled,
                Some(source) => match perform_import(&db_path, &source) {
                    Ok(report) => JobEvent::Imported(report),
                    Err(message) => JobEvent::Failed(message),
                },
            },
            Job::Export => match dialogs::prompt_gedcom_export_path() {
                None => JobEvent::Cancelled,
                Some(dest) => match perform_export(&db_path, &dest) {
                    Ok(bytes) => JobEvent::Exported { bytes, path: dest },
                    Err(message) => JobEvent::Failed(message),
                },
            },
        };
        let _ = tx.send(event);
    }
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
    fn apply_event_drives_status_transitions() {
        let mut app = StemmaApp::new("stemma-flow.db");
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
