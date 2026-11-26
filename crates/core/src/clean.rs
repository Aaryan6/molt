use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::guard::{self, GitCheck};
use crate::rules;
use crate::scan::is_link;

#[derive(Debug)]
pub enum DeleteError {
    /// The path is not something the rules recognise as reclaimable.
    NotATarget,
    /// Git tracks files inside the folder.
    Tracked,
    /// Another Git repository sits inside the folder.
    NestedRepo,
    /// Git couldn't say whether it tracks files inside the folder.
    GitFailed(String),
    /// Programs are using the folder. Holds their names when Windows knows them.
    InUse(Vec<String>),
    Io(io::Error),
}

impl fmt::Display for DeleteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeleteError::NotATarget => write!(f, "refusing: not a recognised reclaimable folder"),
            DeleteError::Tracked => write!(f, "kept: Git tracks files in this folder"),
            DeleteError::NestedRepo => write!(f, "kept: it holds another Git repository"),
            DeleteError::GitFailed(msg) => write!(f, "kept: couldn't check it with Git ({msg})"),
            DeleteError::InUse(apps) if apps.is_empty() => write!(
                f,
                "in use: close dev servers, editors or terminals open in this project and try again"
            ),
            DeleteError::InUse(apps) => write!(f, "in use by {}: close it and try again", apps.join(", ")),
            DeleteError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Delete a candidate folder permanently. The rules are re-checked at delete
/// time so a stale or tampered path can never remove an arbitrary directory,
/// and the folder is renamed out of the way first, so a folder a program is
/// using fails up front instead of being left half deleted.
pub fn delete_candidate(path: &Path) -> Result<(), DeleteError> {
    check(path)?;
    let claimed = claim(path)?;
    match remove_all(&claimed) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Put what's left back under its own name, so it isn't hidden.
            if !path.exists() {
                let _ = fs::rename(&claimed, path);
            }
            Err(DeleteError::Io(e))
        }
    }
}

/// Move a candidate folder to the Recycle Bin instead of deleting it. The
/// space is only freed once the bin is emptied. Same safety checks as
/// [`delete_candidate`].
pub fn recycle_candidate(path: &Path) -> Result<(), DeleteError> {
    check(path)?;
    // The bin restores a folder under the name it went in with, so only test
    // that the folder can be moved, then put the name back.
    let claimed = claim(path)?;
    release(&claimed, path)?;
    recycle(path).map_err(DeleteError::Io)
}

/// Everything that must hold before Molt touches a folder.
fn check(path: &Path) -> Result<(), DeleteError> {
    if is_link(path) || !is_target(path) {
        return Err(DeleteError::NotATarget);
    }
    match guard::git_tracked(path) {
        GitCheck::Tracked => return Err(DeleteError::Tracked),
        GitCheck::Failed(msg) => return Err(DeleteError::GitFailed(msg)),
        // Without Git installed there is no index to protect.
        GitCheck::NotTracked | GitCheck::NoGit => {}
    }
    let contents = guard::inspect(path);
    if contents.nested_repo {
        return Err(DeleteError::NestedRepo);
    }
    // Renaming the folder can't see a running program inside it, so ask
    // Windows who has its programs and libraries loaded. If Restart Manager
    // itself fails, the rename in `claim` is still there.
    let users = guard::programs_using(&contents.binaries).unwrap_or_default();
    if !users.is_empty() {
        return Err(DeleteError::InUse(users));
    }
    Ok(())
}

/// Rename `path` to a hidden name next to it. Windows refuses while any
/// program has a file open in the folder or is running from inside it.
fn claim(path: &Path) -> Result<PathBuf, DeleteError> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(DeleteError::NotATarget);
    };
    let name = name.to_string_lossy();
    let pid = std::process::id();
    let claimed = (0..100)
        .map(|n| parent.join(format!(".{name}.molt-{pid}-{n}")))
        .find(|p| fs::symlink_metadata(p).is_err())
        .ok_or_else(|| DeleteError::Io(io::Error::other("couldn't find a free temporary name")))?;
    match fs::rename(path, &claimed) {
        Ok(()) => Ok(claimed),
        Err(e) if is_in_use(&e) => Err(DeleteError::InUse(Vec::new())),
        Err(e) => Err(DeleteError::Io(e)),
    }
}

/// Undo [`claim`]. Retries briefly in case something (a virus scanner, the
/// search indexer) opened the folder in between.
fn release(claimed: &Path, path: &Path) -> Result<(), DeleteError> {
    let mut last = None;
    for _ in 0..20 {
        match fs::rename(claimed, path) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let e = last.expect("at least one attempt");
    Err(DeleteError::Io(io::Error::other(format!(
        "couldn't rename it back from {}: {e}",
        claimed.display()
    ))))
}

/// Access denied, sharing violation or lock violation: what Windows says when
/// a folder can't be renamed because something is using it.
fn is_in_use(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

fn remove_all(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        // Read-only files (common in node_modules and .git packs) block
        // deletion on Windows. Clear the flag and try once more.
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            clear_readonly(path);
            fs::remove_dir_all(path)
        }
        other => other,
    }
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
    if rules::is_venv(&list_names(path)) || rules::has_cachedir_tag(path) {
        return true;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    rules::match_by_name(&name, parent, &list_names(parent)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    // Not under the system temp folder: that is in AppData, which the rules
    // protect, so nothing there could be deleted.
    fn project(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/clean-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join("package.json"), "{}").unwrap();
        fs::write(dir.join("node_modules/pkg/index.js"), "1").unwrap();
        dir
    }

    #[cfg(windows)]
    #[test]
    fn refuses_a_folder_with_an_open_file_and_leaves_it_whole() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = project("open");
        let nm = dir.join("node_modules");
        let held = fs::OpenOptions::new().read(true).share_mode(1).open(nm.join("pkg/index.js")).unwrap();
        assert!(matches!(delete_candidate(&nm), Err(DeleteError::InUse(_))));
        assert!(nm.join("pkg/index.js").is_file());
        drop(held);
        delete_candidate(&nm).unwrap();
        assert!(!nm.exists());
        // No temporary folder is left behind.
        assert_eq!(list_names(&dir), vec!["package.json".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn names_the_program_running_from_inside() {
        let dir = project("running");
        let nm = dir.join("node_modules");
        let exe = nm.join("pkg/pinger.exe");
        fs::copy(r"C:\Windows\System32\PING.EXE", &exe).unwrap();
        let mut child = Command::new(&exe)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let result = delete_candidate(&nm);
        let _ = child.kill();
        let _ = child.wait();
        match result {
            Err(DeleteError::InUse(apps)) => assert!(!apps.is_empty()),
            other => panic!("expected InUse, got {other:?}"),
        }
        assert!(exe.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_folders_git_tracks() {
        let dir = project("tracked");
        let git = |args: &[&str]| {
            assert!(Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success());
        };
        git(&["init", "-q"]);
        git(&["add", "-f", "node_modules"]);
        assert!(matches!(delete_candidate(&dir.join("node_modules")), Err(DeleteError::Tracked)));
        git(&["rm", "-q", "-r", "--cached", "node_modules"]);
        delete_candidate(&dir.join("node_modules")).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_folders_holding_another_repo() {
        let dir = project("nested");
        fs::create_dir_all(dir.join("node_modules/pkg/.git")).unwrap();
        assert!(matches!(delete_candidate(&dir.join("node_modules")), Err(DeleteError::NestedRepo)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn claim_and_release_keep_the_name() {
        let dir = project("claim");
        let nm = dir.join("node_modules");
        let claimed = claim(&nm).unwrap();
        assert!(!nm.exists() && claimed.is_dir());
        release(&claimed, &nm).unwrap();
        assert!(nm.join("pkg/index.js").is_file());
        let _ = fs::remove_dir_all(&dir);
    }
}
