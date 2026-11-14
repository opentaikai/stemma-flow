//! Portable media paths relative to the tree database directory.

use std::path::{Component, Path, PathBuf};

/// Resolves a stored media path against the tree directory.
pub fn resolve(base_dir: Option<&Path>, stored: &str) -> PathBuf {
    let path = Path::new(stored);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        match base_dir {
            Some(base) => base.join(path),
            None => path.to_path_buf(),
        }
    }
}

/// Converts an input path into its portable stored form.
pub fn store(base_dir: Option<&Path>, input: &str) -> String {
    let input = input.trim();
    if input.is_empty() {
        return String::new();
    }
    let Some(base_dir) = base_dir else {
        return input.to_string();
    };
    let base = normalize(&absolute(base_dir));
    let path = Path::new(input);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let path = normalize(&absolute(&path));
    match path.strip_prefix(&base) {
        Ok(relative) if !relative.as_os_str().is_empty() => portable(relative),
        _ => portable(&path),
    }
}

/// Returns the path written to portable formats such as GEDCOM.
pub fn export(base_dir: Option<&Path>, stored: &str) -> String {
    store(base_dir, stored)
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(component.as_os_str()),
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

fn portable(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PathBuf {
        std::env::temp_dir().join("stemma-media-paths")
    }

    #[test]
    fn stores_files_under_the_tree_relative() {
        let base = base();
        let input = base.join("media").join("photo.jpg");
        assert_eq!(
            store(Some(&base), &input.to_string_lossy()),
            "media/photo.jpg"
        );
        assert_eq!(store(Some(&base), "media/photo.jpg"), "media/photo.jpg");
        assert_eq!(
            resolve(Some(&base), "media/photo.jpg"),
            base.join("media/photo.jpg")
        );
    }

    #[test]
    fn keeps_external_absolute_paths() {
        let base = base();
        let outside = if cfg!(windows) {
            PathBuf::from("C:\\Photos\\photo.jpg")
        } else {
            PathBuf::from("/Photos/photo.jpg")
        };
        assert_eq!(
            store(Some(&base), &outside.to_string_lossy()),
            portable(&outside)
        );
        assert_eq!(resolve(Some(&base), &outside.to_string_lossy()), outside);
    }

    #[test]
    fn normalizes_parent_components() {
        let base = base();
        let parent = base.parent().expect("parent");
        let outside = parent.join("Photos").join("photo.jpg");
        assert_eq!(
            store(Some(&base), "../Photos/photo.jpg"),
            portable(&outside)
        );
        assert_eq!(
            store(
                Some(&base),
                &format!("{}/../Photos/photo.jpg", base.display())
            ),
            portable(&outside)
        );
        assert_eq!(
            store(
                Some(&base),
                &format!("{}/./media/photo.jpg", base.display())
            ),
            "media/photo.jpg"
        );
    }

    #[test]
    fn export_converts_absolute_tree_paths() {
        let base = base();
        let absolute = base.join("media").join("photo.jpg");
        assert_eq!(
            export(Some(&base), &absolute.to_string_lossy()),
            "media/photo.jpg"
        );
        assert_eq!(export(Some(&base), "media/photo.jpg"), "media/photo.jpg");
    }

    #[test]
    fn missing_base_keeps_input_verbatim() {
        assert_eq!(store(None, "  media/photo.jpg  "), "media/photo.jpg");
        assert_eq!(store(None, "/tmp/photo.jpg"), "/tmp/photo.jpg");
        assert_eq!(
            resolve(None, "media/photo.jpg"),
            Path::new("media/photo.jpg")
        );
    }
}
