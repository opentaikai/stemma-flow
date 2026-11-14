//! Matching missing media records against files in a chosen folder.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::db::media::MediaSummary;

/// One unique filename match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRepairMatch {
    pub media_id: String,
    pub old_path: String,
    pub new_path: String,
}

/// A filename found in several places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguousMediaMatch {
    pub media_id: String,
    pub file_name: String,
    pub candidates: Vec<String>,
}

/// The result of scanning one folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaRepairPlan {
    pub matches: Vec<MediaRepairMatch>,
    pub ambiguous: Vec<AmbiguousMediaMatch>,
    pub unmatched: Vec<String>,
}

/// Scans `root` recursively and matches missing media by file name.
pub fn find_matches(root: &Path, missing: &[MediaSummary]) -> Result<MediaRepairPlan, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    let mut files: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    collect_files(root, &mut files).map_err(|error| error.to_string())?;

    let mut plan = MediaRepairPlan::default();
    for media in missing {
        let Some(file_name) = media
            .resolved_path
            .file_name()
            .and_then(|name| name.to_str())
        else {
            plan.unmatched.push(media.id.clone());
            continue;
        };
        let key = file_name.to_lowercase();
        match files.get(&key) {
            Some(candidates) if candidates.len() == 1 => {
                plan.matches.push(MediaRepairMatch {
                    media_id: media.id.clone(),
                    old_path: media.file_path.clone(),
                    new_path: portable(&candidates[0]),
                });
            }
            Some(candidates) => {
                let mut candidates = candidates
                    .iter()
                    .map(|path| portable(path))
                    .collect::<Vec<_>>();
                candidates.sort();
                plan.ambiguous.push(AmbiguousMediaMatch {
                    media_id: media.id.clone(),
                    file_name: file_name.to_string(),
                    candidates,
                });
            }
            None => plan.unmatched.push(media.id.clone()),
        }
    }
    plan.matches
        .sort_by(|left, right| left.old_path.cmp(&right.old_path));
    plan.ambiguous
        .sort_by(|left, right| left.file_name.cmp(&right.file_name));
    plan.unmatched.sort();
    Ok(plan)
}

fn collect_files(root: &Path, files: &mut BTreeMap<String, Vec<PathBuf>>) -> std::io::Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && let Some(name) = entry.file_name().to_str()
            {
                files
                    .entry(name.to_lowercase())
                    .or_default()
                    .push(entry.path());
            }
        }
    }
    Ok(())
}

fn portable(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(id: &str, stored: &str, resolved: &str) -> MediaSummary {
        MediaSummary {
            id: id.to_string(),
            file_path: stored.to_string(),
            resolved_path: PathBuf::from(resolved),
            format: None,
            caption: None,
            link_count: 0,
            missing: true,
        }
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stemma-media-repair-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn finds_unique_and_case_insensitive_files() {
        let root = temp_dir();
        fs::create_dir_all(root.join("photos")).expect("photos dir");
        fs::write(root.join("photos/photo.jpg"), b"photo").expect("file");
        let missing = [media("m1", "old/photo.jpg", "/old/PHOTO.JPG")];
        let plan = find_matches(&root, &missing).expect("scan");
        assert_eq!(plan.matches.len(), 1);
        assert_eq!(plan.matches[0].media_id, "m1");
        assert_eq!(
            plan.matches[0].new_path,
            root.join("photos/photo.jpg")
                .to_string_lossy()
                .replace('\\', "/")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reports_ambiguous_and_missing_names() {
        let root = temp_dir();
        fs::create_dir_all(root.join("a")).expect("a");
        fs::create_dir_all(root.join("b")).expect("b");
        fs::write(root.join("a/photo.jpg"), b"a").expect("a file");
        fs::write(root.join("b/photo.jpg"), b"b").expect("b file");
        let missing = [
            media("m1", "old/photo.jpg", "/old/photo.jpg"),
            media("m2", "old/absent.jpg", "/old/absent.jpg"),
        ];
        let plan = find_matches(&root, &missing).expect("scan");
        assert!(plan.matches.is_empty());
        assert_eq!(plan.ambiguous.len(), 1);
        assert_eq!(plan.ambiguous[0].media_id, "m1");
        assert_eq!(plan.ambiguous[0].candidates.len(), 2);
        assert_eq!(plan.unmatched, ["m2"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_non_folders() {
        let root = temp_dir();
        let file = root.join("file.txt");
        fs::write(&file, b"x").expect("file");
        assert!(find_matches(&file, &[]).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
