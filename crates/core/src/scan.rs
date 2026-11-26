use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rayon::prelude::*;
use serde::Serialize;

use crate::guard;
use crate::rules::{self, Kind};

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
    /// Owning project directory (enclosing git repo or manifest root, else parent).
    pub project_root: PathBuf,
    pub kind: Kind,
    /// Sum of file sizes in bytes (logical size).
    pub size: u64,
    /// Bytes in files that are hardlinked elsewhere (pnpm store). Deleting the
    /// folder does not free these until every other link is gone.
    pub shared_size: u64,
    /// Last time the owning project was touched, seconds since the Unix epoch.
    pub last_active: Option<u64>,
}

impl Candidate {
    /// Bytes actually freed by deleting this folder right now.
    pub fn reclaimable(&self) -> u64 {
        self.size - self.shared_size
    }
}

/// Walk `roots` in parallel and call `on_found` for every reclaimable folder.
/// Results stream out as soon as each folder is sized.
pub fn scan<F>(roots: &[PathBuf], on_found: F)
where
    F: Fn(Candidate) + Sync,
{
    let on_found = &on_found;
    rayon::scope(|s| {
        for root in roots {
            // Callers should check `protected_area` first and explain; this
            // keeps a protected root from being walked regardless.
            if rules::protected_area(root).is_some() {
                continue;
            }
            let scan_root = root.as_path();
            s.spawn(move |s| walk(s, root.clone(), scan_root, on_found));
        }
    });
}

fn walk<'a, F>(s: &rayon::Scope<'a>, dir: PathBuf, scan_root: &'a Path, on_found: &'a F)
where
    F: Fn(Candidate) + Sync,
{
    let Ok(read) = fs::read_dir(&dir) else { return };
    let mut names = Vec::new();
    let mut subdirs: Vec<(String, PathBuf)> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() && !is_link(&entry.path()) {
                subdirs.push((name.clone(), entry.path()));
            }
        }
        names.push(name);
    }

    if rules::is_package_store(&names) {
        return;
    }
    if rules::is_venv(&names) {
        spawn_size(s, dir, scan_root, Kind::PythonVenv, on_found);
        return;
    }
    // A tool marked this folder as a disposable cache. The scan root itself is
    // never offered: the user picked it as the place their projects live.
    if dir != scan_root && rules::lists_cachedir_tag(&names) && rules::has_cachedir_tag(&dir) {
        spawn_size(s, dir, scan_root, Kind::Cache, on_found);
        return;
    }

    for (name, path) in subdirs {
        if rules::is_skipped_dir(&name) || rules::is_package_manager_dir(&dir, &name) {
            continue;
        }
        if let Some(kind) = rules::match_by_name(&name, &dir, &names) {
            spawn_size(s, path, scan_root, kind, on_found);
        } else if rules::is_hidden_dir(&name) {
            // Hidden folders are only taken when they say what they are:
            // a venv (`.venv`) or a tagged cache.
            if rules::is_venv_dir(&path) {
                spawn_size(s, path, scan_root, Kind::PythonVenv, on_found);
            } else if rules::has_cachedir_tag(&path) {
                spawn_size(s, path, scan_root, Kind::Cache, on_found);
            }
        } else {
            s.spawn(move |s| walk(s, path, scan_root, on_found));
        }
    }
}

fn spawn_size<'a, F>(
    s: &rayon::Scope<'a>,
    path: PathBuf,
    scan_root: &'a Path,
    kind: Kind,
    on_found: &'a F,
) where
    F: Fn(Candidate) + Sync,
{
    s.spawn(move |_| {
        // Only pnpm node_modules hold hardlinks into the global store.
        let check_links = kind == Kind::NodeModules && path.join(".pnpm").is_dir();
        let stats = dir_stats(&path, check_links);
        // Empty folders free nothing and only add noise to the list.
        if stats.size == 0 {
            return;
        }
        // Another repository inside may hold work that exists nowhere else,
        // and a folder Git tracks was committed on purpose. Leave both out;
        // the same checks run again before anything is deleted.
        if stats.nested_repo || guard::git_tracked(&path) == guard::GitCheck::Tracked {
            return;
        }
        let project_root = find_project_root(&path, scan_root, kind);
        let last_active = project_last_active(&path, &project_root);
        on_found(Candidate {
            path,
            project_root,
            kind,
            size: stats.size,
            shared_size: stats.shared,
            last_active,
        });
    });
}

#[derive(Default, Clone, Copy)]
struct Stats {
    size: u64,
    shared: u64,
    /// A `.git` was found somewhere inside.
    nested_repo: bool,
}

fn dir_stats(dir: &Path, check_links: bool) -> Stats {
    let Ok(read) = fs::read_dir(dir) else {
        return Stats::default();
    };
    let entries: Vec<_> = read.flatten().collect();
    entries
        .par_iter()
        .map(|e| {
            if e.file_name().eq_ignore_ascii_case(".git") {
                return Stats { nested_repo: true, ..Default::default() };
            }
            let Ok(ft) = e.file_type() else { return Stats::default() };
            if ft.is_dir() {
                if is_link(&e.path()) {
                    Stats::default()
                } else {
                    dir_stats(&e.path(), check_links)
                }
            } else if let Ok(md) = e.metadata() {
                let len = md.len();
                let shared = if check_links && link_count(&e.path()) > 1 { len } else { 0 };
                Stats { size: len, shared, nested_repo: false }
            } else {
                Stats::default()
            }
        })
        .reduce(Stats::default, |a, b| Stats {
            size: a.size + b.size,
            shared: a.shared + b.shared,
            nested_repo: a.nested_repo || b.nested_repo,
        })
}

/// Number of hardlinks to the file (1 if unknown or not on Windows).
#[cfg(windows)]
fn link_count(path: &Path) -> u32 {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    // Access 0 + full sharing: query metadata without touching the file.
    let Ok(file) = fs::OpenOptions::new().access_mode(0).share_mode(7).open(path) else {
        return 1;
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
    if ok == 0 {
        1
    } else {
        info.nNumberOfLinks
    }
}

#[cfg(not(windows))]
fn link_count(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).map(|m| m.nlink() as u32).unwrap_or(1)
}

/// Locate the owning project directory for `candidate` without climbing above
/// `scan_root`. Prefers the nearest ancestor containing `.git`; for
/// `__pycache__` without git, falls back to the nearest ancestor with a Python
/// project manifest (`pyproject.toml`, `setup.py`, `requirements.txt`).
fn find_project_root(candidate: &Path, scan_root: &Path, kind: Kind) -> PathBuf {
    let Some(parent) = candidate.parent() else {
        return candidate.to_path_buf();
    };
    let mut chain = Vec::new();
    let mut cur = Some(parent);
    while let Some(dir) = cur {
        if !dir.starts_with(scan_root) {
            break;
        }
        chain.push(dir);
        if dir == scan_root {
            break;
        }
        cur = dir.parent();
    }
    if let Some(git_root) = chain.iter().find(|d| d.join(".git").exists()) {
        return git_root.to_path_buf();
    }
    if kind == Kind::PyCache {
        const PY_MARKERS: [&str; 3] = ["pyproject.toml", "setup.py", "requirements.txt"];
        if let Some(py_root) = chain.iter().find(|&&d| {
            (d != scan_root || d == parent) && PY_MARKERS.iter().any(|m| d.join(m).is_file())
        }) {
            return py_root.to_path_buf();
        }
    }
    parent.to_path_buf()
}

/// Newest mtime among the owning project's own files (two levels deep, skipping
/// generated folders and the candidate itself) plus `.git/index` and
/// `.git/logs/HEAD` at the project root.
fn project_last_active(candidate: &Path, project_root: &Path) -> Option<u64> {
    let mut newest = None;
    let mut note = |md: &fs::Metadata| {
        newest = newest.max(md.modified().ok().and_then(epoch_secs));
    };

    let git = project_root.join(".git");
    for rel in ["index", "HEAD", "logs/HEAD"] {
        if let Ok(md) = fs::metadata(git.join(rel)) {
            note(&md);
        }
    }

    if let Some(parent) = candidate.parent() {
        scan_dir_mtimes(parent, candidate, 2, &mut note);
        if project_root != parent {
            scan_dir_mtimes(project_root, candidate, 2, &mut note);
        }
    }
    newest
}

fn scan_dir_mtimes(dir: &Path, skip: &Path, depth: u8, note: &mut impl FnMut(&fs::Metadata)) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for e in read.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path == skip || rules::is_generated_name(&name) || rules::is_venv_dir(&path) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        note(&md);
        if depth > 1 && md.is_dir() && !rules::is_hidden_dir(&name) && !is_link(&path) {
            scan_dir_mtimes(&path, skip, depth - 1, note);
        }
    }
}

fn epoch_secs(t: SystemTime) -> Option<u64> {
    t.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// True for symlinks and Windows junctions / any reparse point.
pub(crate) fn is_link(path: &Path) -> bool {
    let Ok(md) = fs::symlink_metadata(path) else {
        return true;
    };
    if md.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("molt-scan-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn skips_empty_folders_and_resolves_project_root() {
        let root = tmp("project-root");

        // Monorepo with .git at repo root, one non-empty and one empty node_modules.
        let mono = root.join("my-mono");
        fs::create_dir_all(mono.join(".git")).unwrap();
        fs::write(mono.join(".git").join("index"), b"idx").unwrap();

        let pkg_a = mono.join("packages").join("a");
        fs::create_dir_all(pkg_a.join("node_modules")).unwrap();
        fs::write(pkg_a.join("package.json"), b"{}").unwrap();
        fs::write(pkg_a.join("node_modules").join("dep.js"), b"hello").unwrap();

        let pkg_b = mono.join("packages").join("b");
        fs::create_dir_all(pkg_b.join("node_modules")).unwrap();
        fs::write(pkg_b.join("package.json"), b"{}").unwrap();

        // Python project without .git, with nested __pycache__.
        let py = root.join("py-app");
        fs::create_dir_all(py.join("pkg").join("sub").join("__pycache__")).unwrap();
        fs::write(py.join("pyproject.toml"), b"[project]").unwrap();
        fs::write(py.join("pkg").join("sub").join("__pycache__").join("a.pyc"), b"1234").unwrap();

        let found = Mutex::new(Vec::new());
        // `std::env::temp_dir()` lives under `AppData` on Windows, which `scan`
        // refuses as a protected area, so exercise `walk` directly here.
        let on_found = |c| found.lock().unwrap().push(c);
        rayon::scope(|s| {
            let scan_root = root.as_path();
            walk(s, root.clone(), scan_root, &on_found);
        });
        let found = found.into_inner().unwrap();

        // Empty pkg_b/node_modules must be skipped.
        assert_eq!(found.len(), 2);

        let nm = found.iter().find(|c| c.kind == Kind::NodeModules).unwrap();
        assert_eq!(nm.path, pkg_a.join("node_modules"));
        assert_eq!(nm.project_root, mono);
        assert!(nm.last_active.is_some());

        let pyc = found.iter().find(|c| c.kind == Kind::PyCache).unwrap();
        assert_eq!(pyc.path, py.join("pkg").join("sub").join("__pycache__"));
        assert_eq!(pyc.project_root, py);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn finds_tagged_caches_and_skips_nested_repos() {
        let root = tmp("tags");
        let tag = "Signature: 8a477f597d28d172789f06886806bc55
";

        // A tagged cache in a plain folder and one hidden folder.
        let cache = root.join("tool-cache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("CACHEDIR.TAG"), tag).unwrap();
        fs::write(cache.join("blob"), b"data").unwrap();
        let hidden = root.join("app").join(".hidden-cache");
        fs::create_dir_all(&hidden).unwrap();
        fs::write(hidden.join("CACHEDIR.TAG"), tag).unwrap();
        fs::write(hidden.join("blob"), b"data").unwrap();

        // A venv holding a package checked out from Git (`pip install -e git+...`).
        let venv = root.join("py").join(".venv");
        fs::create_dir_all(venv.join("src").join("dep").join(".git")).unwrap();
        fs::write(venv.join("pyvenv.cfg"), b"home = x").unwrap();
        fs::write(venv.join("src").join("dep").join("mod.py"), b"x = 1").unwrap();

        let found = Mutex::new(Vec::new());
        let on_found = |c| found.lock().unwrap().push(c);
        rayon::scope(|s| walk(s, root.clone(), root.as_path(), &on_found));
        let mut paths: Vec<_> = found.into_inner().unwrap().into_iter().map(|c: Candidate| (c.kind, c.path)).collect();
        paths.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(paths, vec![(Kind::Cache, hidden), (Kind::Cache, cache)]);
        let _ = fs::remove_dir_all(&root);
    }
}
