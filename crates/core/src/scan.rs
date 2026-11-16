use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rayon::prelude::*;
use serde::Serialize;

use crate::rules::{self, Kind};

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
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
            s.spawn(move |s| walk(s, root.clone(), on_found));
        }
    });
}

fn walk<'a, F>(s: &rayon::Scope<'a>, dir: PathBuf, on_found: &'a F)
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

    if rules::is_venv(&names) {
        spawn_size(s, dir, Kind::PythonVenv, on_found);
        return;
    }

    for (name, path) in subdirs {
        if rules::is_skipped_dir(&name) {
            continue;
        }
        if let Some(kind) = rules::match_by_name(&name, &dir, &names) {
            spawn_size(s, path, kind, on_found);
        } else {
            s.spawn(move |s| walk(s, path, on_found));
        }
    }
}

fn spawn_size<'a, F>(s: &rayon::Scope<'a>, path: PathBuf, kind: Kind, on_found: &'a F)
where
    F: Fn(Candidate) + Sync,
{
    s.spawn(move |_| {
        // Only pnpm node_modules hold hardlinks into the global store.
        let check_links = kind == Kind::NodeModules && path.join(".pnpm").is_dir();
        let stats = dir_stats(&path, check_links);
        let last_active = project_last_active(&path);
        on_found(Candidate {
            path,
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
}

fn dir_stats(dir: &Path, check_links: bool) -> Stats {
    let Ok(read) = fs::read_dir(dir) else {
        return Stats::default();
    };
    let entries: Vec<_> = read.flatten().collect();
    entries
        .par_iter()
        .map(|e| {
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
                Stats { size: len, shared }
            } else {
                Stats::default()
            }
        })
        .reduce(Stats::default, |a, b| Stats {
            size: a.size + b.size,
            shared: a.shared + b.shared,
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

/// Newest mtime among the project's own files (two levels deep, skipping
/// generated folders and the candidate itself) plus `.git/index`.
fn project_last_active(candidate: &Path) -> Option<u64> {
    let project = candidate.parent()?;
    let mut newest = None;
    let mut note = |md: &fs::Metadata| {
        newest = newest.max(md.modified().ok().and_then(epoch_secs));
    };

    if let Ok(md) = fs::metadata(project.join(".git").join("index")) {
        note(&md);
    }
    for e in fs::read_dir(project).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if e.path() == candidate || rules::is_generated_name(&name) || rules::is_venv_dir(&e.path()) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        note(&md);
        if md.is_dir() && !is_link(&e.path()) {
            if let Ok(inner) = fs::read_dir(e.path()) {
                for i in inner.flatten() {
                    if let Ok(imd) = i.metadata() {
                        note(&imd);
                    }
                }
            }
        }
    }
    newest
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
