//! Checks that keep Molt from deleting a folder it can't prove is safe to
//! remove: files Git tracks, another repository nested inside, and programs
//! that are using it right now.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rayon::prelude::*;

use crate::scan::is_link;

/// What Git says about the files inside a folder.
#[derive(Debug, PartialEq, Eq)]
pub enum GitCheck {
    /// Not inside a repository, or Git tracks nothing in it.
    NotTracked,
    /// Git tracks at least one file inside the folder.
    Tracked,
    /// The folder is in a repository but Git isn't installed.
    NoGit,
    /// Git ran and failed, so nothing is known.
    Failed(String),
}

/// Ask Git whether it tracks any file inside `path`. Build folders are almost
/// always ignored, so a tracked file means the folder holds something the
/// project committed on purpose (a vendored dependency, a checked-in build).
pub fn git_tracked(path: &Path) -> GitCheck {
    let Some(repo) = path.parent().and_then(|p| p.ancestors().find(|d| d.join(".git").exists())) else {
        return GitCheck::NotTracked;
    };
    let Ok(rel) = path.strip_prefix(repo) else {
        return GitCheck::NotTracked;
    };
    let rel = rel.to_string_lossy().replace('\\', "/");
    let mut cmd = Command::new("git");
    // safe.directory: repos on other drives often belong to another Windows
    // account, which makes Git refuse to read them. fsmonitor: never start a
    // hook or daemon just to answer this.
    cmd.args(["-c", "safe.directory=*", "-c", "core.fsmonitor=false", "-C"])
        .arg(repo)
        .args(["ls-files", "--cached", "-z", "--"])
        // literal: the name is not a glob. icase: Windows paths ignore case,
        // and the index may spell a renamed folder differently.
        .arg(format!(":(top,literal,icase){rel}"));
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_LITERAL_PATHSPECS",
        "GIT_GLOB_PATHSPECS",
        "GIT_NOGLOB_PATHSPECS",
        "GIT_ICASE_PATHSPECS",
    ] {
        cmd.env_remove(var);
    }
    hide_window(&mut cmd);
    match cmd.output() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => GitCheck::NoGit,
        Err(e) => GitCheck::Failed(e.to_string()),
        Ok(out) if !out.status.success() => {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            GitCheck::Failed(if msg.is_empty() { format!("git exited with {}", out.status) } else { msg })
        }
        Ok(out) if out.stdout.is_empty() => GitCheck::NotTracked,
        Ok(_) => GitCheck::Tracked,
    }
}

#[cfg(windows)]
fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_cmd: &mut Command) {}

/// What a walk through a folder found.
#[derive(Debug, Default)]
pub struct Contents {
    /// Programs and libraries (`.exe`, `.dll`, `.node`, `.pyd`). A running
    /// one can't be deleted, and renaming its folder doesn't notice it.
    pub binaries: Vec<PathBuf>,
    /// A `.git` somewhere inside: another repository, maybe with work that
    /// exists nowhere else.
    pub nested_repo: bool,
}

/// Walk `dir` (without following links) for binaries and nested repositories.
pub fn inspect(dir: &Path) -> Contents {
    let Ok(read) = fs::read_dir(dir) else {
        return Contents::default();
    };
    let entries: Vec<_> = read.flatten().collect();
    entries
        .par_iter()
        .map(|e| {
            let path = e.path();
            let name = e.file_name();
            if name.eq_ignore_ascii_case(".git") {
                return Contents { nested_repo: true, ..Default::default() };
            }
            let Ok(ft) = e.file_type() else { return Contents::default() };
            if ft.is_dir() {
                return if is_link(&path) { Contents::default() } else { inspect(&path) };
            }
            let ext = path.extension().map(|x| x.to_string_lossy().to_ascii_lowercase());
            let binary = matches!(ext.as_deref(), Some("exe" | "dll" | "node" | "pyd"));
            Contents { binaries: if binary { vec![path] } else { Vec::new() }, nested_repo: false }
        })
        .reduce(Contents::default, |mut a, mut b| {
            a.binaries.append(&mut b.binaries);
            a.nested_repo |= b.nested_repo;
            a
        })
}

/// Names of the programs that have any of `files` open or loaded, using the
/// Windows Restart Manager. Molt itself is left out.
#[cfg(windows)]
pub fn programs_using(files: &[PathBuf]) -> std::io::Result<Vec<String>> {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::ERROR_MORE_DATA;
    use windows_sys::Win32::System::RestartManager::{
        RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY, RM_PROCESS_INFO,
    };

    if files.is_empty() {
        return Ok(Vec::new());
    }
    let fail = |code: u32| Err(io::Error::from_raw_os_error(code as i32));

    let mut session = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: `key` has room for the session key and its NUL.
    let rc = unsafe { RmStartSession(&mut session, 0, key.as_mut_ptr()) };
    if rc != 0 {
        return fail(rc);
    }
    struct Session(u32);
    impl Drop for Session {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful RmStartSession.
            unsafe { RmEndSession(self.0) };
        }
    }
    let _session = Session(session);

    let wide: Vec<Vec<u16>> =
        files.iter().map(|f| f.as_os_str().encode_wide().chain(std::iter::once(0)).collect()).collect();
    let ptrs: Vec<*const u16> = wide.iter().map(|w| w.as_ptr()).collect();
    for chunk in ptrs.chunks(1000) {
        // SAFETY: every pointer is a NUL-terminated string in `wide`, which
        // outlives the call.
        let rc = unsafe {
            RmRegisterResources(session, chunk.len() as u32, chunk.as_ptr(), 0, std::ptr::null(), 0, std::ptr::null())
        };
        if rc != 0 {
            return fail(rc);
        }
    }

    let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
    loop {
        let mut needed = 0u32;
        let mut count = infos.len() as u32;
        let mut reasons = 0u32;
        let buf = if infos.is_empty() { std::ptr::null_mut() } else { infos.as_mut_ptr() };
        // SAFETY: `buf` is null with a count of 0, or points at `count` entries.
        let rc = unsafe { RmGetList(session, &mut needed, &mut count, buf, &mut reasons) };
        if rc == ERROR_MORE_DATA {
            // SAFETY: RM_PROCESS_INFO is plain data; all zeroes is valid.
            infos = vec![unsafe { std::mem::zeroed() }; needed as usize];
            continue;
        }
        if rc != 0 {
            return fail(rc);
        }
        infos.truncate(count as usize);
        break;
    }

    let me = std::process::id();
    let mut names: Vec<String> = infos
        .iter()
        .filter(|p| p.Process.dwProcessId != me)
        .map(|p| {
            let len = p.strAppName.iter().position(|&c| c == 0).unwrap_or(p.strAppName.len());
            String::from_utf16_lossy(&p.strAppName[..len])
        })
        .filter(|n| !n.is_empty())
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(not(windows))]
pub fn programs_using(_files: &[PathBuf]) -> std::io::Result<Vec<String>> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Not under the system temp folder: that is in AppData, which the
    // rules protect.
    fn tmp(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/guard-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap().status.success();
        assert!(ok, "git {args:?} failed");
    }

    #[test]
    fn finds_tracked_files_inside_a_folder() {
        let repo = tmp("git");
        git(&repo, &["init", "-q"]);
        fs::create_dir_all(repo.join("vendor/pkg")).unwrap();
        fs::write(repo.join("vendor/pkg/a.php"), "<?php").unwrap();
        fs::create_dir_all(repo.join("Node_Modules")).unwrap();
        fs::write(repo.join("Node_Modules/x.js"), "1").unwrap();
        git(&repo, &["add", "vendor"]);
        assert_eq!(git_tracked(&repo.join("vendor")), GitCheck::Tracked);
        assert_eq!(git_tracked(&repo.join("Node_Modules")), GitCheck::NotTracked);
        // Case differs from the index, as after a rename on Windows.
        assert_eq!(git_tracked(&repo.join("VENDOR")), GitCheck::Tracked);
        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn inspect_finds_binaries_and_nested_repos() {
        let dir = tmp("inspect");
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/addon.NODE"), "").unwrap();
        fs::write(dir.join("a/readme.md"), "").unwrap();
        let c = inspect(&dir);
        assert_eq!(c.binaries, vec![dir.join("a/b/addon.NODE")]);
        assert!(!c.nested_repo);
        fs::create_dir_all(dir.join("a/dep/.git")).unwrap();
        assert!(inspect(&dir).nested_repo);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn sees_a_running_program() {
        let dir = tmp("inuse");
        let exe = dir.join("pinger.exe");
        fs::copy(r"C:\Windows\System32\PING.EXE", &exe).unwrap();
        assert!(programs_using(std::slice::from_ref(&exe)).unwrap().is_empty());
        let mut child = Command::new(&exe)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let users = programs_using(std::slice::from_ref(&exe));
        let _ = child.kill();
        let _ = child.wait();
        assert!(!users.unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
