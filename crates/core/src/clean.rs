use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use crate::rules;
use crate::scan::is_link;

#[derive(Debug)]
pub enum DeleteError {
    /// The path is not something the rules recognise as reclaimable.
    NotATarget,
    Io(io::Error),
}

impl fmt::Display for DeleteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeleteError::NotATarget => write!(f, "refusing: not a recognised reclaimable folder"),
            DeleteError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Delete a candidate folder permanently. The rules are re-checked at delete
/// time so a stale or tampered path can never remove an arbitrary directory.
pub fn delete_candidate(path: &Path) -> Result<(), DeleteError> {
    if is_link(path) || !is_target(path) {
        return Err(DeleteError::NotATarget);
    }
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        // Read-only files (common in node_modules and .git packs) block
        // deletion on Windows. Clear the flag and try once more.
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            clear_readonly(path);
            fs::remove_dir_all(path).map_err(DeleteError::Io)
        }
        Err(e) => Err(DeleteError::Io(e)),
    }
}

fn clear_readonly(dir: &Path) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if is_link(&path) {
            continue;
        }
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            clear_readonly(&path);
        }
        if let Ok(md) = fs::metadata(&path) {
            let mut perms = md.permissions();
            if perms.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                let _ = fs::set_permissions(&path, perms);
            }
        }
    }
}

fn list_names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

fn is_target(path: &Path) -> bool {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return false;
    };
    if rules::is_venv(&list_names(path)) {
        return true;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    rules::match_by_name(&name, parent, &list_names(parent)).is_some()
}
