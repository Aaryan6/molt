use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use molt_core::{delete_candidate, recycle_candidate, scan, Candidate};

#[derive(Parser)]
#[command(name = "molt", about = "Find and clean regenerable dev folders")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List reclaimable folders, largest first.
    Scan {
        roots: Vec<PathBuf>,
        /// Only show folders at least this many MB.
        #[arg(long, default_value_t = 0)]
        min_mb: u64,
        #[arg(long)]
        json: bool,
    },
    /// Delete reclaimable folders. Dry run unless --yes is passed.
    Clean {
        roots: Vec<PathBuf>,
        #[arg(long, default_value_t = 0)]
        min_mb: u64,
        /// Only folders whose newest file is at least this many days old.
        #[arg(long, default_value_t = 0)]
        older_than_days: u64,
        /// Move folders to the Recycle Bin instead of deleting them.
        #[arg(long)]
        recycle: bool,
        #[arg(long)]
        yes: bool,
    },
}

fn collect(roots: &[PathBuf], min_mb: u64) -> Vec<Candidate> {
    for root in roots {
        if let Some(area) = molt_core::protected_area(root) {
            eprintln!("skipping {}: it is inside {area}, which holds installed apps and tools, not projects", root.display());
        }
    }
    let found = Mutex::new(Vec::new());
    scan(roots, |c| {
        if c.size >= min_mb * 1024 * 1024 {
            found.lock().unwrap().push(c);
        }
    });
    let mut v = found.into_inner().unwrap();
    v.sort_by(|a, b| b.size.cmp(&a.size));
    v
}

fn human(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

fn age_days(c: &Candidate) -> Option<u64> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    c.last_active.map(|m| now.saturating_sub(m) / 86_400)
}

fn main() {
    match Cli::parse().cmd {
        Cmd::Scan { roots, min_mb, json } => {
            let items = collect(&roots, min_mb);
            if json {
                println!("{}", serde_json::to_string_pretty(&items).unwrap());
                return;
            }
            for c in &items {
                println!(
                    "{:>10} {:>10}  {:>5}d  {:<13} {}",
                    human(c.size),
                    human(c.reclaimable()),
                    age_days(c).map_or("?".into(), |d| d.to_string()),
                    c.kind.label(),
                    c.path.display()
                );
            }
            let total: u64 = items.iter().map(|c| c.size).sum();
            let reclaim: u64 = items.iter().map(|c| c.reclaimable()).sum();
            println!("\n(columns: size, frees-now, days since project activity)");
            println!(
                "{} folders, {} total, {} freed immediately (rest is hardlinked to the pnpm store: run `pnpm store prune` after)",
                items.len(),
                human(total),
                human(reclaim)
            );
        }
        Cmd::Clean { roots, min_mb, older_than_days, recycle, yes } => {
            let items: Vec<_> = collect(&roots, min_mb)
                .into_iter()
                .filter(|c| age_days(c).is_some_and(|d| d >= older_than_days))
                .collect();
            let total: u64 = items.iter().map(|c| c.reclaimable()).sum();
            println!("{} folders, {} freed immediately", items.len(), human(total));
            if !yes {
                for c in &items {
                    println!("would delete {:>10}  {}", human(c.size), c.path.display());
                }
                println!("\nDry run. Re-run with --yes to delete.");
                return;
            }
            let mut freed = 0;
            for c in &items {
                let result = if recycle { recycle_candidate(&c.path) } else { delete_candidate(&c.path) };
                match result {
                    Ok(()) => {
                        freed += c.reclaimable();
                        println!("{} {}", if recycle { "recycled" } else { "deleted " }, c.path.display());
                    }
                    Err(e) => eprintln!("failed   {}: {e}", c.path.display()),
                }
            }
            if recycle {
                println!("\nMoved {} to the Recycle Bin. Empty it to free the space.", human(freed));
            } else {
                println!("\nFreed {}", human(freed));
            }
        }
    }
}
