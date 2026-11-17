#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

use molt_core::{Candidate, DiskSpace};
use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Channel;

/// Scan `roots`, streaming each reclaimable folder to the UI as it is sized.
#[tauri::command]
async fn scan(roots: Vec<String>, on_found: Channel<Candidate>) -> Result<(), String> {
    let roots: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).collect();
    if let Some(area) = roots.iter().find_map(|r| molt_core::protected_area(r)) {
        return Err(format!(
            "This folder is inside {area}, which holds installed apps and tools, not projects. Pick the folder where you keep your code."
        ));
    }
    spawn_blocking(move || {
        molt_core::scan(&roots, |c| {
            let _ = on_found.send(c);
        })
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct DeleteResult {
    path: String,
    error: Option<String>,
}

/// Delete folders, or move them to the Recycle Bin when `recycle` is set. The
/// core re-validates every path against the rules.
#[tauri::command]
async fn delete_paths(paths: Vec<String>, recycle: bool) -> Result<Vec<DeleteResult>, String> {
    spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| {
                let path = Path::new(&p);
                let result = if recycle {
                    molt_core::recycle_candidate(path)
                } else {
                    molt_core::delete_candidate(path)
                };
                let error = result
                    .err()
                    .map(|e| e.to_string());
                DeleteResult { path: p, error }
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Size and free space of the drive that holds `path`.
#[tauri::command]
fn disk_info(path: String) -> Option<DiskSpace> {
    molt_core::disk_space(Path::new(&path))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![scan, delete_paths, disk_info])
        .run(tauri::generate_context!())
        .expect("error while running Molt");
}
