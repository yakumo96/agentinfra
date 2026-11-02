//! Shared Infra protocol definitions: tool table, error codes, state enums and IDs.
//!
//! Everything in `generated` comes from `schemas/` via `tools/schema/codegen.py`.

mod error;
#[rustfmt::skip]
mod generated;
pub mod ids;

pub use error::{InfraError, InfraResult};
pub use generated::*;

/// Look up a tool definition by name.
pub fn tool(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The tools a token with `role` may see in `tools/list`.
pub fn tools_for(role: Role) -> impl Iterator<Item = &'static ToolDef> {
    TOOLS
        .iter()
        .filter(move |t| t.roles.contains(&role) || role == Role::Admin)
}
