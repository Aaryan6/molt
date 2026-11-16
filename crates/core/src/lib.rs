//! Core of Molt: find regenerable dev folders and delete them safely.

mod clean;
mod disk;
mod rules;
mod scan;

pub use clean::{delete_candidate, DeleteError};
pub use disk::{disk_space, DiskSpace};
pub use rules::Kind;
pub use scan::{scan, Candidate};
