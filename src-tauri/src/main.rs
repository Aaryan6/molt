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

/// Delete folders. The core re-validates every path against the rules.
#[tauri::command]
async fn delete_paths(paths: Vec<String>) -> Result<Vec<DeleteResult>, String> {
    spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| {
                let error = molt_core::delete_candidate(Path::new(&p))
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
