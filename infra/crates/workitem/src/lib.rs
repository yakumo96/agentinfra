//! Work-item board rules that do not need the database.
//!
//! Claiming itself is one SQL statement (`migrations/`); this crate computes
//! the path overlaps Infra reports at claim and submit time. Overlap is
//! information for the coordinator, never a lock, so the glob-vs-glob test is
//! conservative: it may report an overlap that no real path hits, but never
//! misses one.
//!
//! 不需要数据库的工作项看板规则。
//!
//! 认领本身是一条 SQL 语句（`migrations/`）；本 crate 计算 Infra 在认领和提交时报告的路径重叠。
//! 重叠只是给协调者的信息，从不是锁，所以 glob 与 glob 的比较是保守的：可能报出实际没有路径命中的重叠，但不会漏掉。

mod glob;

pub use glob::{matches, may_overlap};

/// Patterns of `theirs` that may share a path with any pattern of `mine`.
/// `theirs` 中可能与 `mine` 任一模式共享路径的模式。
pub fn overlapping<'a>(mine: &[String], theirs: &'a [String]) -> Vec<&'a str> {
    theirs
        .iter()
        .filter(|t| mine.iter().any(|m| may_overlap(m, t)))
        .map(String::as_str)
        .collect()
}

/// Patterns of `scope` that match at least one of the concrete `paths`.
/// `scope` 中至少匹配 `paths` 里一个具体路径的模式。
pub fn touched<'a>(scope: &'a [String], paths: &[String]) -> Vec<&'a str> {
    scope
        .iter()
        .filter(|g| paths.iter().any(|p| matches(g, p)))
        .map(String::as_str)
        .collect()
}
