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

/// Marker stored in share-bundle manifests.
pub const SHARE_FORMAT: &str = "stemma-flow-share";
/// Current share-bundle manifest version.
pub const SHARE_VERSION: u32 = 2;

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

/// Summary of a portable folder share bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareReport {
    pub path: PathBuf,
    /// Size of the bundled database snapshot.
    pub database_bytes: u64,
    /// Size of the whole bundle: database, media, manifest and README.
    pub total_bytes: u64,
    pub media_included: usize,
    pub media_missing: Vec<String>,
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
struct ShareMediaEntry {
    id: String,
    /// Original file name only; never a filesystem path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bundle_path: Option<String>,
    missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShareManifest {
    format: String,
    version: u32,
    created_at: String,
    app_version: String,
    schema_version: i64,
    database: String,
    media: Vec<ShareMediaEntry>,
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

pub fn timestamp_text() -> Result<String, String> {
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
    snapshot_database(db_path, &snapshot_path)?;
    let schema_version = schema_version(&snapshot_path)?;

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
    crate::media_path::resolve(db_path.parent(), stored)
}

/// Writes a portable folder bundle containing a tree snapshot and its media.
///
/// Media paths in the bundled database are rewritten relative to the bundle,
/// so the folder can be copied to removable media or another machine.
pub fn share_folder(
    db_path: &Path,
    parent: &Path,
    name: &str,
    timestamp: &str,
) -> Result<ShareReport, String> {
    if !db_path.is_file() {
        return Err(format!("no tree database at {}", db_path.display()));
    }
    let safe_name = sanitize_filename(name);
    let safe_name = if safe_name.is_empty() {
        file_stem(db_path)
    } else {
        safe_name
    };
    let directory = parent.join(format!("{safe_name}-share-{timestamp}"));
    if directory.exists() {
        return Err(format!(
            "share bundle already exists: {}",
            directory.display()
        ));
    }
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let result = write_share_folder(db_path, &directory, &safe_name);
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

struct MediaCopy {
    entries: Vec<ShareMediaEntry>,
    rewritten_paths: Vec<(String, String)>,
    missing: Vec<String>,
    included: usize,
}

fn write_share_folder(db_path: &Path, directory: &Path, name: &str) -> Result<ShareReport, String> {
    let database_name = format!("{name}.db");
    let snapshot_path = directory.join(&database_name);
    snapshot_database(db_path, &snapshot_path)?;
    let schema_version = schema_version(&snapshot_path)?;
    let media = copy_media(db_path, directory, media_rows(&snapshot_path)?)?;
    rewrite_media_paths(&snapshot_path, &media.rewritten_paths)?;
    write_share_manifest(directory, &database_name, schema_version, media.entries)?;
    write_share_readme(
        directory,
        &database_name,
        media.included,
        media.missing.len(),
    )?;
    share_report(directory, &snapshot_path, media.included, media.missing)
}

fn schema_version(snapshot_path: &Path) -> Result<i64, String> {
    let conn = Connection::open(snapshot_path).map_err(|error| error.to_string())?;
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn copy_media(
    db_path: &Path,
    directory: &Path,
    media: Vec<(String, String)>,
) -> Result<MediaCopy, String> {
    let media_dir = directory.join("media");
    fs::create_dir_all(&media_dir).map_err(|error| error.to_string())?;
    let mut archive_paths: BTreeMap<PathBuf, Option<String>> = BTreeMap::new();
    let mut entries = Vec::with_capacity(media.len());
    let mut missing: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut rewritten_paths: Vec<(String, String)> = Vec::with_capacity(media.len());
    for (id, stored_path) in media {
        let resolved = resolve_media_path(db_path, &stored_path);
        let exists = resolved.is_file();
        let key = if exists {
            fs::canonicalize(&resolved).unwrap_or_else(|_| resolved.clone())
        } else {
            resolved.clone()
        };
        if !exists {
            missing.entry(resolved.clone()).or_insert_with(|| {
                file_name_of(&stored_path).unwrap_or_else(|| "unknown media".to_string())
            });
        }
        let bundle_path = match archive_paths.get(&key) {
            Some(path) => path.clone(),
            None => {
                if exists {
                    let file_name = resolved
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("media");
                    let stored = format!(
                        "media/{}_{}",
                        sanitize_filename(&id),
                        sanitize_filename(file_name)
                    );
                    fs::copy(&resolved, directory.join(&stored))
                        .map_err(|error| error.to_string())?;
                    archive_paths.insert(key, Some(stored.clone()));
                    Some(stored)
                } else {
                    archive_paths.insert(key, None);
                    None
                }
            }
        };
        let file_name = file_name_of(&stored_path);
        let written_path = bundle_path
            .clone()
            .unwrap_or_else(|| missing_media_path(&id, &stored_path));
        rewritten_paths.push((id.clone(), written_path));
        entries.push(ShareMediaEntry {
            id,
            file_name,
            bundle_path,
            missing: !exists,
        });
    }
    Ok(MediaCopy {
        included: archive_paths.values().filter(|path| path.is_some()).count(),
        entries,
        rewritten_paths,
        missing: missing.into_values().collect(),
    })
}

fn rewrite_media_paths(snapshot_path: &Path, updates: &[(String, String)]) -> Result<(), String> {
    if updates.is_empty() {
        return Ok(());
    }
    let mut conn = Connection::open(snapshot_path).map_err(|error| error.to_string())?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    for (id, path) in updates {
        tx.execute(
            "UPDATE media SET file_path = ?1 WHERE id = ?2",
            params![path, id],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| error.to_string())
}

fn write_share_manifest(
    directory: &Path,
    database_name: &str,
    schema_version: i64,
    media: Vec<ShareMediaEntry>,
) -> Result<(), String> {
    let manifest = ShareManifest {
        format: SHARE_FORMAT.to_string(),
        version: SHARE_VERSION,
        created_at: timestamp_text()?,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version,
        database: database_name.to_string(),
        media,
    };
    let manifest_text = toml::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    fs::write(directory.join("share-manifest.toml"), manifest_text)
        .map_err(|error| error.to_string())
}

fn write_share_readme(
    directory: &Path,
    database_name: &str,
    included: usize,
    missing: usize,
) -> Result<(), String> {
    let readme = format!(
        "Stemma Flow family tree share\n=============================\n\nOpen \"{database_name}\" in Stemma Flow.\nKeep the \"media\" folder next to the database; its paths are relative.\nMissing media stay in the database as relative \"missing/...\" placeholders.\n\nGenerated: {}\nMedia included: {included}\nMedia missing: {missing}\n",
        timestamp_text()?,
    );
    fs::write(directory.join("README.txt"), readme).map_err(|error| error.to_string())
}

fn share_report(
    directory: &Path,
    snapshot_path: &Path,
    included: usize,
    missing: Vec<String>,
) -> Result<ShareReport, String> {
    let database_bytes = fs::metadata(snapshot_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    Ok(ShareReport {
        path: directory.to_path_buf(),
        database_bytes,
        total_bytes: bundle_bytes(directory)?,
        media_included: included,
        media_missing: missing,
    })
}

fn bundle_bytes(path: &Path) -> Result<u64, String> {
    let mut total = 0;
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        total += if metadata.is_dir() {
            bundle_bytes(&entry.path())?
        } else {
            metadata.len()
        };
    }
    Ok(total)
}

fn file_name_of(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn missing_media_path(id: &str, stored: &str) -> String {
    let name = file_name_of(stored)
        .as_deref()
        .map(sanitize_filename)
        .unwrap_or_else(|| "file".to_string());
    format!("missing/{}_{}", sanitize_filename(id), name)
}

fn snapshot_database(db_path: &Path, dest: &Path) -> Result<(), String> {
    let source = db::open_connection(db_path).map_err(|error| error.to_string())?;
    let mut snapshot = Connection::open(dest).map_err(|error| error.to_string())?;
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
    Ok(())
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
        let stored = crate::media_path::store(dest_db.parent(), &path.to_string_lossy());
        for entry in &manifest.media {
            if entry.archive_path.as_deref() == Some(archive_path.as_str()) {
                restored_paths.insert(entry.id.clone(), stored.clone());
            }
        }
    }
    if !restored_paths.is_empty() {
        let mut conn = db::open_connection(&temp_db).map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        for (id, path) in &restored_paths {
            tx.execute(
                "UPDATE media SET file_path = ?1 WHERE id = ?2",
                params![path, id],
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
        assert!(
            Path::new(&stored).is_relative(),
            "restored media paths are portable"
        );
        let stored_path = crate::media_path::resolve(restored.parent(), &stored);
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
    fn share_bundle_copies_media_and_rewrites_relative_paths() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let media_path = seed(&db_path);
        let conn = Connection::open(&db_path).expect("db");
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m2', ?1)",
            params![media_path.to_string_lossy().to_string()],
        )
        .expect("shared media row");
        drop(conn);
        let parent = dir.join("usb");
        fs::create_dir_all(&parent).expect("usb dir");
        let report = share_folder(&db_path, &parent, "Family Tree", "20260101").expect("share");
        assert!(report.path.is_dir());
        assert_eq!(report.media_included, 1, "shared media copied once");
        assert!(report.media_missing.is_empty());
        assert!(report.database_bytes > 0);
        assert!(report.total_bytes > report.database_bytes);
        let bundled_db = report.path.join("Family_Tree.db");
        assert!(bundled_db.is_file());
        let conn = Connection::open(&bundled_db).expect("bundled db");
        let paths: Vec<String> = conn
            .prepare("SELECT file_path FROM media ORDER BY id")
            .expect("query")
            .query_map([], |row| row.get(0))
            .expect("rows")
            .collect::<rusqlite::Result<_>>()
            .expect("paths");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], paths[1]);
        assert!(Path::new(&paths[0]).is_relative());
        assert!(
            crate::media_path::resolve(Some(report.path.as_path()), &paths[0]).is_file(),
            "bundled media resolves next to the database"
        );
        let readme = fs::read_to_string(report.path.join("README.txt")).expect("readme");
        assert!(readme.contains("Family_Tree.db"));
        assert!(readme.contains("Media included: 1"));
        let manifest_text =
            fs::read_to_string(report.path.join("share-manifest.toml")).expect("manifest");
        assert!(manifest_text.contains(SHARE_FORMAT));
        assert!(
            !manifest_text.contains(dir.to_string_lossy().as_ref()),
            "manifest stores no source paths"
        );
        assert!(!manifest_text.contains("original_path"));
        let manifest: ShareManifest = toml::from_str(&manifest_text).expect("manifest parses");
        assert_eq!(manifest.version, SHARE_VERSION);
        assert_eq!(manifest.media.len(), 2);
        for entry in &manifest.media {
            assert!(!entry.missing);
            assert_eq!(entry.file_name.as_deref(), Some("portrait.jpg"));
            assert!(entry.bundle_path.is_some());
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn share_bundles_report_missing_media_and_refuse_overwrites() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let conn = connection(&db_path);
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m1', '/definitely/missing.jpg')",
            [],
        )
        .expect("media row");
        drop(conn);
        let parent = dir.join("usb");
        fs::create_dir_all(&parent).expect("usb dir");
        let report = share_folder(&db_path, &parent, "Tree", "20260101").expect("share");
        assert_eq!(report.media_included, 0);
        assert_eq!(report.media_missing, vec!["missing.jpg".to_string()]);
        assert!(report.total_bytes > report.database_bytes);
        let conn = Connection::open(report.path.join("Tree.db")).expect("db");
        let stored: String = conn
            .query_row("SELECT file_path FROM media WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .expect("media");
        assert_eq!(stored, "missing/m1_missing.jpg");
        assert!(Path::new(&stored).is_relative());
        let manifest_text =
            fs::read_to_string(report.path.join("share-manifest.toml")).expect("manifest");
        assert!(!manifest_text.contains("/definitely"));
        assert!(!manifest_text.contains("original_path"));
        let manifest: ShareManifest = toml::from_str(&manifest_text).expect("manifest parses");
        assert_eq!(manifest.version, SHARE_VERSION);
        let entry = manifest.media.first().expect("media entry");
        assert_eq!(entry.file_name.as_deref(), Some("missing.jpg"));
        assert!(entry.bundle_path.is_none());
        assert!(entry.missing);
        assert!(
            share_folder(&db_path, &parent, "Tree", "20260101").is_err(),
            "existing bundle is not overwritten"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn share_bundles_do_not_leak_source_paths() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let source_dir = dir.join("Alice Private").join("Photos");
        fs::create_dir_all(&source_dir).expect("source dir");
        let included = source_dir.join("portrait.jpg");
        fs::write(&included, b"portrait").expect("portrait");
        let missing_absolute = source_dir.join("secret-location.jpg");
        let conn = connection(&db_path);
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m1', ?1)",
            params![included.to_string_lossy().to_string()],
        )
        .expect("included row");
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m2', ?1)",
            params![missing_absolute.to_string_lossy().to_string()],
        )
        .expect("missing absolute row");
        conn.execute(
            "INSERT INTO media (id, file_path)
             VALUES ('m3', 'private/album/secret.jpg')",
            [],
        )
        .expect("missing relative row");
        drop(conn);
        let parent = dir.join("usb");
        fs::create_dir_all(&parent).expect("usb dir");
        let report = share_folder(&db_path, &parent, "Tree", "20260101").expect("share");
        assert_eq!(report.media_included, 1);
        assert_eq!(report.media_missing.len(), 2);
        for name in &report.media_missing {
            assert!(Path::new(name).is_relative(), "report holds display names");
        }

        let conn = Connection::open(report.path.join("Tree.db")).expect("bundled db");
        let paths: Vec<String> = conn
            .prepare("SELECT file_path FROM media ORDER BY id")
            .expect("query")
            .query_map([], |row| row.get(0))
            .expect("rows")
            .collect::<rusqlite::Result<_>>()
            .expect("paths");
        assert_eq!(paths.len(), 3);
        for path in &paths {
            assert!(Path::new(path).is_relative(), "{path} stays relative");
            assert!(
                !path.contains("Alice Private"),
                "{path} hides the source tree"
            );
            assert!(
                !path.contains("private/album"),
                "{path} hides source folders"
            );
            assert!(
                !Path::new(path)
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir)),
                "{path} has no parent traversal"
            );
        }
        assert_eq!(paths[1], "missing/m2_secret-location.jpg");
        assert_eq!(paths[2], "missing/m3_secret.jpg");

        let manifest_text =
            fs::read_to_string(report.path.join("share-manifest.toml")).expect("manifest");
        assert!(!manifest_text.contains("Alice Private"));
        assert!(!manifest_text.contains(source_dir.to_string_lossy().as_ref()));
        assert!(!manifest_text.contains("original_path"));
        let manifest: ShareManifest = toml::from_str(&manifest_text).expect("manifest parses");
        assert_eq!(manifest.version, SHARE_VERSION);
        assert_eq!(manifest.media.len(), 3);
        let included = manifest
            .media
            .iter()
            .find(|entry| entry.id == "m1")
            .expect("included entry");
        assert_eq!(included.file_name.as_deref(), Some("portrait.jpg"));
        assert!(
            included
                .bundle_path
                .as_deref()
                .is_some_and(|path| Path::new(path).is_relative())
        );
        let missing = manifest
            .media
            .iter()
            .find(|entry| entry.id == "m2")
            .expect("missing entry");
        assert_eq!(missing.file_name.as_deref(), Some("secret-location.jpg"));
        assert!(missing.bundle_path.is_none());
        assert!(missing.missing);
        let relative = manifest
            .media
            .iter()
            .find(|entry| entry.id == "m3")
            .expect("relative entry");
        assert_eq!(relative.file_name.as_deref(), Some("secret.jpg"));
        assert!(relative.bundle_path.is_none());
        assert!(relative.missing);

        let readme = fs::read_to_string(report.path.join("README.txt")).expect("readme");
        assert!(!readme.contains("Alice Private"));
        assert!(readme.contains("missing/..."));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_media_placeholders_are_relative_and_sanitized() {
        assert_eq!(
            missing_media_path("m1", "/home/alice/Photos/portrait.jpg"),
            "missing/m1_portrait.jpg"
        );
        assert_eq!(
            missing_media_path("m2", "private/album/secret.jpg"),
            "missing/m2_secret.jpg"
        );
        assert_eq!(
            missing_media_path("m3", "portrait.jpg"),
            "missing/m3_portrait.jpg"
        );
        assert_eq!(missing_media_path("m4", ""), "missing/m4_file");
        for path in [
            missing_media_path("m1", "/home/alice/Photos/portrait.jpg"),
            missing_media_path("m2", "private/album/secret.jpg"),
            missing_media_path("m4", ""),
        ] {
            assert!(Path::new(&path).is_relative());
            assert!(
                !Path::new(&path)
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            );
        }
    }

    #[test]
    fn copy_media_deduplicates_and_marks_missing() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let source = dir.join("photo.jpg");
        fs::write(&source, b"photo").expect("source media");
        let bundle = dir.join("bundle");
        let media = vec![
            ("m1".to_string(), source.to_string_lossy().to_string()),
            ("m2".to_string(), source.to_string_lossy().to_string()),
            ("m3".to_string(), "private/missing.jpg".to_string()),
        ];
        let copy = copy_media(&db_path, &bundle, media).expect("copy media");
        assert_eq!(copy.included, 1, "shared files are copied once");
        assert_eq!(copy.entries.len(), 3);
        assert_eq!(copy.rewritten_paths.len(), 3);
        assert_eq!(copy.missing, vec!["missing.jpg".to_string()]);
        assert_eq!(
            copy.entries[0].bundle_path, copy.entries[1].bundle_path,
            "shared rows point at one bundled file"
        );
        assert!(copy.entries[2].missing);
        assert!(copy.entries[2].bundle_path.is_none());
        assert_eq!(
            copy.rewritten_paths[2].1, "missing/m3_missing.jpg",
            "missing rows use a relative placeholder"
        );
        let bundle_path = copy.entries[0]
            .bundle_path
            .as_deref()
            .expect("bundled path");
        let copied = crate::media_path::resolve(Some(&bundle), bundle_path);
        assert!(copied.is_file());
        assert_eq!(fs::read(copied).expect("copied bytes"), b"photo");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rewrite_media_paths_ignores_empty_updates() {
        let dir = temp_dir();
        let missing = dir.join("does-not-exist.db");
        rewrite_media_paths(&missing, &[]).expect("empty update list");
        assert!(
            !missing.exists(),
            "an empty update does not open the database"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn share_bundle_total_includes_media_and_metadata() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let media_path = seed(&db_path);
        let media_bytes = fs::metadata(&media_path).expect("media metadata").len();
        let parent = dir.join("usb");
        fs::create_dir_all(&parent).expect("usb dir");
        let report = share_folder(&db_path, &parent, "Tree", "20260101").expect("share");
        let bundled_media: u64 = fs::read_dir(report.path.join("media"))
            .expect("media dir")
            .map(|entry| {
                entry
                    .expect("media entry")
                    .metadata()
                    .expect("media metadata")
                    .len()
            })
            .sum();
        assert_eq!(bundled_media, media_bytes, "media bytes are counted once");
        let manifest_bytes = fs::metadata(report.path.join("share-manifest.toml"))
            .expect("manifest")
            .len();
        let readme_bytes = fs::metadata(report.path.join("README.txt"))
            .expect("readme")
            .len();
        assert_eq!(
            report.total_bytes,
            report.database_bytes + bundled_media + manifest_bytes + readme_bytes
        );
        assert!(report.total_bytes > report.database_bytes);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn relative_media_paths_survive_backup_and_restore() {
        let dir = temp_dir();
        let db_path = dir.join("tree.db");
        let conn = connection(&db_path);
        let media_dir = dir.join("media");
        fs::create_dir_all(&media_dir).expect("media dir");
        fs::write(media_dir.join("photo.jpg"), b"photo").expect("media file");
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m1', 'media/photo.jpg')",
            [],
        )
        .expect("media row");
        drop(conn);
        let archive = dir.join("backup.zip");
        let report = backup_tree(&db_path, &archive).expect("backup");
        assert_eq!(report.media_included, 1);

        let restored = dir.join("restored.db");
        restore_backup(&archive, &restored).expect("restore");
        let conn = Connection::open(&restored).expect("db");
        let stored: String = conn
            .query_row("SELECT file_path FROM media WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .expect("media");
        assert!(Path::new(&stored).is_relative(), "{stored}");
        assert!(crate::media_path::resolve(restored.parent(), &stored).is_file());

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
                    .map(|entry| {
                        entry
                            .name()
                            .map(|name| name.starts_with("media/"))
                            .unwrap_or(false)
                    })
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
