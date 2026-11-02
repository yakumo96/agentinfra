//! Work-item board rules that do not need the database.
//!
//! Claiming itself is one SQL statement (`migrations/`); this crate computes
//! the path overlaps Infra reports at claim and submit time. Overlap is
//! information for the coordinator, never a lock, so the glob-vs-glob test is
//! conservative: it may report an overlap that no real path hits, but never
//! misses one.

mod glob;

pub use glob::{matches, may_overlap};

/// Patterns of `theirs` that may share a path with any pattern of `mine`.
pub fn overlapping<'a>(mine: &[String], theirs: &'a [String]) -> Vec<&'a str> {
    theirs
        .iter()
        .filter(|t| mine.iter().any(|m| may_overlap(m, t)))
        .map(String::as_str)
        .collect()
}

/// Patterns of `scope` that match at least one of the concrete `paths`.
pub fn touched<'a>(scope: &'a [String], paths: &[String]) -> Vec<&'a str> {
    scope
        .iter()
        .filter(|g| paths.iter().any(|p| matches(g, p)))
        .map(String::as_str)
        .collect()
}
