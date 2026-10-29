//! Timestamped ZIP backups containing a SQLite snapshot and referenced media.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::db;

/// Marker stored in every manifest.
pub const BACKUP_FORMAT: &str = "stemma-flow-backup";

/// Current manifest version.
pub const BACKUP_VERSION: u32 = 1;

const MANIFEST_ENTRY: &str = "manifest.toml";
const DATABASE_ENTRY: &str = "tree.db";

/// Summary of a completed backup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupReport {
    pub media_included: usize,
    pub media_missing: Vec<String>,
    pub database_bytes: u64,
}

/// Summary of a completed restore.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreReport {
    pub media_restored: usize,
    pub media_missing: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupManifest {
    format: String,
    version: u32,
    created_at: String,
    app_version: String,
    schema_version: i64,
    database: String,
    #[serde(default)]
    media: Vec<MediaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MediaEntry {
    id: String,
    original_path: String,
    #[serde(default)]
    archive_path: Option<String>,
    missing: bool,
}

/// Local timestamp suffix used in backup and restored file names.
pub fn timestamp_suffix() -> Result<String, String> {
    let conn = Connection::open_in_memory().map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT strftime('%Y%m%d-%H%M%S', 'now', 'localtime')",
        [],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn timestamp_text() -> Result<String, String> {
    let conn = Connection::open_in_memory().map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT strftime('%Y-%m-%d %H:%M:%S', 'now', 'localtime')",
        [],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

/// Default backup name for a tree database.
pub fn backup_file_name(db_path: &Path, timestamp: &str) -> String {
    format!("{}-backup-{timestamp}.zip", file_stem(db_path))
}

/// Default restored database name for an archive.
pub fn restore_file_name(archive: &Path, timestamp: &str) -> String {
    format!("{}-restored-{timestamp}.db", file_stem(archive))
}

/// Media folder created next to a restored database.
pub fn restored_media_dir(db_path: &Path) -> PathBuf {
    db_path.with_file_name(format!("{}_media", file_stem(db_path)))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitize_filename)
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "family_tree".to_string())
}

/// Creates a backup ZIP at `dest`.
pub fn backup_tree(db_path: &Path, dest: &Path) -> Result<BackupReport, String> {
    if !db_path.is_file() {
        return Err(format!("no tree database at {}", db_path.display()));
    }
    let parent = dest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let work = parent.join(format!(".stemma-flow-backup-{}", Uuid::new_v4()));
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    let result = write_backup(db_path, dest, &work);
    let _ = fs::remove_dir_all(&work);
    result
}

fn write_backup(db_path: &Path, dest: &Path, work: &Path) -> Result<BackupReport, String> {
    let snapshot_path = work.join(DATABASE_ENTRY);
    let source = db::open_connection(db_path).map_err(|error| error.to_string())?;
    let mut snapshot = Connection::open(&snapshot_path).map_err(|error| error.to_string())?;
    {
        let backup = rusqlite::backup::Backup::new(&source, &mut snapshot)
            .map_err(|error| error.to_string())?;
        backup
            .run_to_completion(128, Duration::from_millis(1), None)
            .map_err(|error| error.to_string())?;
    }
    snapshot
        .execute_batch("PRAGMA journal_mode = DELETE;")
        .map_err(|error| error.to_string())?;
    drop(snapshot);
    drop(source);

    let schema_version: i64 = {
        let conn = Connection::open(&snapshot_path).map_err(|error| error.to_string())?;
        conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| error.to_string())?
    };

    let media = media_rows(&snapshot_path)?;
    let mut archive_paths: BTreeMap<PathBuf, Option<String>> = BTreeMap::new();
    let mut entries = Vec::with_capacity(media.len());
    let mut missing = BTreeSet::new();
    for (id, stored_path) in media {
        let resolved = resolve_media_path(db_path, &stored_path);
        let exists = resolved.is_file();
        let key = if exists {
            fs::canonicalize(&resolved).unwrap_or_else(|_| resolved.clone())
        } else {
            resolved.clone()
        };
        if !exists {
            missing.insert(resolved.to_string_lossy().to_string());
        }
        let archive_path = match archive_paths.get(&key) {
            Some(path) => path.clone(),
            None => {
                if exists {
                    let name = resolved
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("media");
                    let archive_path = format!(
                        "media/{}_{}",
                        sanitize_filename(&id),
                        sanitize_filename(name)
                    );
                    archive_paths.insert(key, Some(archive_path.clone()));
                    Some(archive_path)
                } else {
                    archive_paths.insert(key, None);
                    None
                }
            }
        };
        entries.push(MediaEntry {
            id,
            original_path: stored_path,
            archive_path,
            missing: !exists,
        });
    }

    let manifest = BackupManifest {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        created_at: timestamp_text()?,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version,
        database: DATABASE_ENTRY.to_string(),
        media: entries,
    };
    let manifest_text = toml::to_string_pretty(&manifest).map_err(|error| error.to_string())?;

    let zip_path = work.join("backup.zip");
    let file = File::create(&zip_path).map_err(|error| error.to_string())?;
    let mut zip = ZipWriter::new(file);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    zip.start_file(DATABASE_ENTRY, deflated)
        .map_err(|error| error.to_string())?;
    let mut database = File::open(&snapshot_path).map_err(|error| error.to_string())?;
    io::copy(&mut database, &mut zip).map_err(|error| error.to_string())?;

    for (source, archive_path) in archive_paths
        .iter()
        .filter_map(|(source, path)| path.as_ref().map(|path| (source, path)))
    {
        zip.start_file(archive_path, stored)
            .map_err(|error| error.to_string())?;
        let mut media_file = File::open(source).map_err(|error| error.to_string())?;
        io::copy(&mut media_file, &mut zip).map_err(|error| error.to_string())?;
    }

    zip.start_file(MANIFEST_ENTRY, deflated)
        .map_err(|error| error.to_string())?;
    zip.write_all(manifest_text.as_bytes())
        .map_err(|error| error.to_string())?;
    zip.finish().map_err(|error| error.to_string())?;

    #[cfg(windows)]
    if dest.exists() {
        fs::remove_file(dest).map_err(|error| error.to_string())?;
    }
    fs::rename(&zip_path, dest).map_err(|error| error.to_string())?;
    let database_bytes = fs::metadata(&snapshot_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    Ok(BackupReport {
        media_included: archive_paths.values().filter(|path| path.is_some()).count(),
        media_missing: missing.into_iter().collect(),
        database_bytes,
    })
}

fn media_rows(db_path: &Path) -> Result<Vec<(String, String)>, String> {
    let conn = Connection::open(db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, file_path FROM media ORDER BY rowid")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn resolve_media_path(db_path: &Path, stored: &str) -> PathBuf {
    let path = Path::new(stored);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

/// Restores `archive` into a new database at `dest_db`.
pub fn restore_backup(archive: &Path, dest_db: &Path) -> Result<RestoreReport, String> {
    if dest_db.exists() {
        return Err(format!(
            "restore destination already exists: {}",
            dest_db.display()
        ));
    }
    let media_dir = restored_media_dir(dest_db);
    if media_dir.exists() {
        return Err(format!(
            "restore media folder already exists: {}",
            media_dir.display()
        ));
    }
    let parent = dest_db
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let work = parent.join(format!(".stemma-flow-restore-{}", Uuid::new_v4()));
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    let result = read_backup(archive, dest_db, &media_dir, &work);
    let _ = fs::remove_dir_all(&work);
    if result.is_err() {
        let _ = fs::remove_file(dest_db);
        let _ = fs::remove_dir_all(&media_dir);
    }
    result
}

fn read_backup(
    archive: &Path,
    dest_db: &Path,
    media_dir: &Path,
    work: &Path,
) -> Result<RestoreReport, String> {
    let file = File::open(archive).map_err(|error| error.to_string())?;
    let mut zip = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let manifest = read_manifest(&mut zip)?;
    if manifest.format != BACKUP_FORMAT {
        return Err("not a stemma-flow backup".to_string());
    }
    if manifest.version > BACKUP_VERSION {
        return Err(format!(
            "backup version {} is newer than this app supports",
            manifest.version
        ));
    }
    if manifest.schema_version > db::schema::SCHEMA_VERSION {
        return Err(format!(
            "backup schema {} is newer than this app supports",
            manifest.schema_version
        ));
    }
    if manifest.database != DATABASE_ENTRY {
        return Err("backup database entry is invalid".to_string());
    }

    let temp_db = work.join(DATABASE_ENTRY);
    copy_entry(&mut zip, DATABASE_ENTRY, &temp_db)?;
    {
        let conn = db::open_connection(&temp_db).map_err(|error| error.to_string())?;
        db::init_db(&conn).map_err(|error| error.to_string())?;
        let check: String = conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if check != "ok" {
            return Err("backup database failed its integrity check".to_string());
        }
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|error| error.to_string())?;
    }

    let mut targets = BTreeMap::new();
    let mut target_names = BTreeMap::new();
    for entry in &manifest.media {
        let Some(archive_path) = entry.archive_path.as_deref() else {
            continue;
        };
        if entry.missing {
            continue;
        }
        let Some(name) = safe_media_name(archive_path) else {
            return Err(format!("unsafe media path in backup: {archive_path}"));
        };
        if let Some(existing) = target_names.get(&name)
            && existing != archive_path
        {
            return Err(format!("duplicate media file name in backup: {name}"));
        }
        target_names.insert(name.clone(), archive_path.to_string());
        targets.insert(archive_path.to_string(), name);
    }

    let staging_media = work.join("media");
    if !targets.is_empty() {
        fs::create_dir_all(&staging_media).map_err(|error| error.to_string())?;
        for (archive_path, name) in &targets {
            let target = staging_media.join(name);
            copy_entry(&mut zip, archive_path, &target)?;
        }
        fs::rename(&staging_media, media_dir).map_err(|error| error.to_string())?;
    }

    let mut restored_paths = BTreeMap::new();
    for (archive_path, name) in &targets {
        let path = std::path::absolute(media_dir.join(name)).map_err(|error| error.to_string())?;
        for entry in &manifest.media {
            if entry.archive_path.as_deref() == Some(archive_path.as_str()) {
                restored_paths.insert(entry.id.clone(), path.clone());
            }
        }
    }
    if !restored_paths.is_empty() {
        let mut conn = db::open_connection(&temp_db).map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        for (id, path) in &restored_paths {
            tx.execute(
                "UPDATE media SET file_path = ?1 WHERE id = ?2",
                params![path.to_string_lossy().to_string(), id],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|error| error.to_string())?;
    }

    if dest_db.exists() {
        let _ = fs::remove_dir_all(media_dir);
        return Err(format!(
            "restore destination already exists: {}",
            dest_db.display()
        ));
    }
    fs::rename(&temp_db, dest_db).map_err(|error| error.to_string())?;
    Ok(RestoreReport {
        media_restored: restored_paths.len(),
        media_missing: manifest.media.iter().filter(|entry| entry.missing).count(),
    })
}

fn read_manifest(zip: &mut ZipArchive<File>) -> Result<BackupManifest, String> {
    let mut manifest = String::new();
    zip.by_name(MANIFEST_ENTRY)
        .map_err(|error| error.to_string())?
        .read_to_string(&mut manifest)
        .map_err(|error| error.to_string())?;
    toml::from_str(&manifest).map_err(|error| format!("invalid backup manifest: {error}"))
}

fn copy_entry(zip: &mut ZipArchive<File>, name: &str, target: &Path) -> Result<(), String> {
    let mut entry = zip.by_name(name).map_err(|error| error.to_string())?;
    let mut file = File::create(target).map_err(|error| error.to_string())?;
    io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
    Ok(())
}

fn safe_media_name(path: &str) -> Option<String> {
    if path.contains('\\') {
        return None;
    }
    let mut parts = path.split('/');
    if parts.next() != Some("media") {
        return None;
    }
    let name = parts.next()?;
    if parts.next().is_some() || name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let name = sanitize_filename(name);
    (!name.is_empty()).then_some(name)
}

fn sanitize_filename(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
            result.push(character);
        } else {
            result.push('_');
        }
    }
    let trimmed = result.trim_matches('.');
    if trimmed.is_empty() {
        "file".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stemma-flow-backup-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn connection(path: &Path) -> Connection {
        let conn = Connection::open(path).expect("database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma");
        init_db(&conn).expect("schema");
        conn
    }

    fn seed(db_path: &Path) -> PathBuf {
        let conn = connection(db_path);
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person");
        drop(conn);
        let media = db_path.with_file_name("portrait.jpg");
        fs::write(&media, b"portrait-bytes").expect("media file");
        let conn = Connection::open(db_path).expect("reopen");
        conn.execute(
            "INSERT INTO media (id, file_path, format) VALUES ('m1', ?1, 'jpg')",
            params![media.to_string_lossy().to_string()],
        )
        .expect("media row");
        media
    }

    #[test]
    fn names_are_timestamped_and_sanitized() {
        let db = Path::new("/tmp/Family Tree.db");
        assert_eq!(
            backup_file_name(db, "20261029-143500"),
            "Family_Tree-backup-20261029-143500.zip"
        );
        let archive = Path::new("/tmp/Family_Tree-backup-20261029-143500.zip");
        assert_eq!(
            restore_file_name(archive, "20261030-010000"),
            "Family_Tree-backup-20261029-143500-restored-20261030-010000.db"
        );
        assert!(timestamp_suffix().expect("timestamp").len() == 15);
    }

    #[test]
    fn backup_and_restore_roundtrip_tree_and_media() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let media_path = seed(&db_path);
        let archive = dir.join("tree-backup.zip");

        let report = backup_tree(&db_path, &archive).expect("backup");
        assert_eq!(report.media_included, 1);
        assert!(report.media_missing.is_empty());
        assert!(archive.is_file());

        let restored = dir.join("restored.db");
        let restore = restore_backup(&archive, &restored).expect("restore");
        assert_eq!(restore.media_restored, 1);
        assert!(restored.is_file());
        let conn = Connection::open(&restored).expect("restored db opens");
        let person: String = conn
            .query_row("SELECT given_name FROM people WHERE id = 'p1'", [], |row| {
                row.get(0)
            })
            .expect("person survives");
        assert_eq!(person, "Ada");
        let stored: String = conn
            .query_row("SELECT file_path FROM media WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .expect("media row survives");
        let stored_path = PathBuf::from(stored);
        assert!(stored_path.is_file(), "restored media file exists");
        assert_eq!(
            fs::read(stored_path).expect("media bytes"),
            fs::read(&media_path).expect("original bytes")
        );
        assert!(
            restored_media_dir(&restored).is_dir(),
            "media folder is next to the restored database"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_media_is_reported_but_does_not_fail() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let conn = connection(&db_path);
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m1', '/definitely/missing.jpg')",
            [],
        )
        .expect("media row");
        drop(conn);
        let archive = dir.join("backup.zip");
        let report = backup_tree(&db_path, &archive).expect("backup");
        assert_eq!(report.media_included, 0);
        assert_eq!(report.media_missing.len(), 1);

        let restored = dir.join("restored.db");
        let restore = restore_backup(&archive, &restored).expect("restore");
        assert_eq!(restore.media_restored, 0);
        assert_eq!(restore.media_missing, 1);
        let conn = Connection::open(&restored).expect("db");
        let stored: String = conn
            .query_row("SELECT file_path FROM media WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .expect("media");
        assert_eq!(stored, "/definitely/missing.jpg");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn shared_media_files_are_stored_once() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let media_path = seed(&db_path);
        let conn = Connection::open(&db_path).expect("db");
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m2', ?1)",
            params![media_path.to_string_lossy().to_string()],
        )
        .expect("second media row");
        drop(conn);
        let archive = dir.join("backup.zip");
        let report = backup_tree(&db_path, &archive).expect("backup");
        assert_eq!(report.media_included, 1);

        let file = File::open(&archive).expect("archive");
        let mut zip = ZipArchive::new(file).expect("zip");
        let media_entries = (0..zip.len())
            .filter(|index| {
                zip.by_index(*index)
                    .map(|entry| entry.name().starts_with("media/"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(media_entries, 1);

        let restored = dir.join("restored.db");
        restore_backup(&archive, &restored).expect("restore");
        let conn = Connection::open(&restored).expect("db");
        let paths: Vec<String> = conn
            .prepare("SELECT file_path FROM media ORDER BY id")
            .expect("query")
            .query_map([], |row| row.get(0))
            .expect("rows")
            .collect::<rusqlite::Result<_>>()
            .expect("paths");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], paths[1]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_and_unsafe_archives_are_rejected() {
        let dir = temp_dir();
        let bogus = dir.join("bogus.zip");
        fs::write(&bogus, b"not a zip").expect("bogus file");
        let dest = dir.join("restored.db");
        assert!(restore_backup(&bogus, &dest).is_err());

        let db_path = dir.join("tree.db");
        seed(&db_path);
        let archive = dir.join("backup.zip");
        backup_tree(&db_path, &archive).expect("backup");
        assert!(
            restore_backup(&archive, &dest).is_ok(),
            "first restore succeeds"
        );
        assert!(
            restore_backup(&archive, &dest).is_err(),
            "restore refuses to overwrite an existing database"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn newer_backup_or_schema_versions_are_rejected() {
        let dir = temp_dir();
        let archive = dir.join("newer.zip");
        let manifest = BackupManifest {
            format: BACKUP_FORMAT.to_string(),
            version: BACKUP_VERSION + 1,
            created_at: "now".to_string(),
            app_version: "999".to_string(),
            schema_version: db::schema::SCHEMA_VERSION,
            database: DATABASE_ENTRY.to_string(),
            media: Vec::new(),
        };
        write_test_archive(&archive, &manifest, b"not a database");
        let error = restore_backup(&archive, &dir.join("restored.db")).expect_err("newer version");
        assert!(error.contains("newer"), "{error}");

        let archive = dir.join("newer-schema.zip");
        let manifest = BackupManifest {
            format: BACKUP_FORMAT.to_string(),
            version: BACKUP_VERSION,
            created_at: "now".to_string(),
            app_version: "999".to_string(),
            schema_version: db::schema::SCHEMA_VERSION + 1,
            database: DATABASE_ENTRY.to_string(),
            media: Vec::new(),
        };
        write_test_archive(&archive, &manifest, b"not a database");
        let error = restore_backup(&archive, &dir.join("restored.db")).expect_err("newer schema");
        assert!(error.contains("schema"), "{error}");
        let _ = fs::remove_dir_all(dir);
    }

    fn write_test_archive(path: &Path, manifest: &BackupManifest, database: &[u8]) {
        let file = File::create(path).expect("archive");
        let mut zip = ZipWriter::new(file);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file(DATABASE_ENTRY, deflated).expect("db entry");
        zip.write_all(database).expect("db bytes");
        zip.start_file(MANIFEST_ENTRY, deflated)
            .expect("manifest entry");
        zip.write_all(
            toml::to_string_pretty(manifest)
                .expect("manifest toml")
                .as_bytes(),
        )
        .expect("manifest bytes");
        zip.finish().expect("zip finish");
    }

    #[test]
    fn unsafe_media_manifest_paths_are_rejected() {
        assert!(safe_media_name("media/portrait.jpg").is_some());
        assert!(safe_media_name("../portrait.jpg").is_none());
        assert!(safe_media_name("media/../portrait.jpg").is_none());
        assert!(safe_media_name("/media/portrait.jpg").is_none());
        assert!(safe_media_name("media/sub/portrait.jpg").is_none());
    }
}
