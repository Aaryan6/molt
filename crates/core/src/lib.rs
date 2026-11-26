//! Core of Molt: find regenerable dev folders and delete them safely.

mod clean;
mod disk;
mod guard;
mod rules;
mod scan;

pub use clean::{delete_candidate, recycle_candidate, DeleteError};
pub use disk::{disk_space, DiskSpace};
pub use rules::{protected_area, Kind};
pub use scan::{scan, Candidate};
