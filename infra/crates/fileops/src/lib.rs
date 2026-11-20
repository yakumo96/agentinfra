//! `ws_apply` semantics, independent of how a workspace is stored.
//!
//! All operations of one call are validated against a staged view first
//! (expectations, text matches, path safety); if any fails nothing is written.
//! The result is an ordered [`Plan`] that the workspace writer executes, after
//! which the call's changes are committed as one commit.
//!
//! `ws_apply` 的语义，与工作区如何存储无关。
//!
//! 一次调用的所有操作先对照暂存视图全部校验（期望值、文本匹配、路径安全）；任一失败则什么都不写。
//! 结果是一个有序的 [`Plan`]，由工作区写入方执行，之后这次调用的改动作为一个 commit 提交。

mod path;
mod plan;
mod view;

pub use path::{NormPath, normalize};
pub use plan::{Action, Op, Plan, plan};
pub use view::{FileView, Kind, Node};
