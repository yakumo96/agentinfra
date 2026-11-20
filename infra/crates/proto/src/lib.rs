//! Shared Infra protocol definitions: tool table, error codes, state enums and IDs.
//!
//! Everything in `generated` comes from `schemas/` via `tools/schema/codegen.py`.
//!
//! Infra 共享协议定义：工具表、错误码、状态枚举和 ID。
//!
//! `generated` 里的一切都由 `tools/schema/codegen.py` 从 `schemas/` 生成。

mod error;
#[rustfmt::skip]
mod generated;
pub mod ids;

pub use error::{InfraError, InfraResult};
pub use generated::*;

/// Look up a tool definition by name.
/// 按名字查找工具定义。
pub fn tool(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The tools a token with `role` may see in `tools/list`.
/// 某角色的令牌在 `tools/list` 里能看到的工具。
pub fn tools_for(role: Role) -> impl Iterator<Item = &'static ToolDef> {
    TOOLS
        .iter()
        .filter(move |t| t.roles.contains(&role) || role == Role::Admin)
}
