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

/// Move a candidate folder to the Recycle Bin instead of deleting it. The
/// space is only freed once the bin is emptied. Same safety checks as
/// [`delete_candidate`].
pub fn recycle_candidate(path: &Path) -> Result<(), DeleteError> {
    if is_link(path) || !is_target(path) {
        return Err(DeleteError::NotATarget);
    }
    recycle(path).map_err(DeleteError::Io)
}

#[cfg(windows)]
fn recycle(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
        FOF_WANTNUKEWARNING, FO_DELETE, SHFILEOPSTRUCTW,
    };

    // The shell wants a full path with backslashes, ending in two NULs.
    let full = std::path::absolute(path)?;
    let mut from: Vec<u16> = full.as_os_str().encode_wide().collect();
    from.extend([0, 0]);

    // SAFETY: an all-zero SHFILEOPSTRUCTW is valid (null handles and
    // pointers); the fields the call reads are set below and `from` outlives it.
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = from.as_ptr();
    // FOF_WANTNUKEWARNING makes Windows ask before it permanently deletes a
    // folder that is too big for the Recycle Bin, instead of doing it silently.
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT | FOF_WANTNUKEWARNING) as u16;
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 {
        return Err(io::Error::other(format!(
            "couldn't move to the Recycle Bin (shell error {code:#x})"
        )));
    }
    if op.fAnyOperationsAborted != 0 {
        return Err(io::Error::other("cancelled"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn recycle(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Recycle Bin is only supported on Windows"))
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
    if rules::protected_area(path).is_some() {
        return false;
    }
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
