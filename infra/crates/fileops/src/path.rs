use std::fmt;

use infra_proto::{ErrorCode, InfraError, InfraResult};

/// A normalized path relative to the workspace root: no leading `/`, no `.`
/// or `..` components, no empty components. The root itself is `"."`.
/// 相对工作区根的规范化路径：没有开头的 `/`，没有 `.` 或 `..` 组成部分，没有空组成部分。根本身是 `"."`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormPath(String);

impl NormPath {
    pub fn root() -> Self {
        NormPath(".".to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0 == "."
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|c| *c != ".")
    }

    /// The parent directory; the root's parent is `None`.
    /// 父目录；根的父目录是 `None`。
    pub fn parent(&self) -> Option<NormPath> {
        if self.is_root() {
            return None;
        }
        Some(match self.0.rsplit_once('/') {
            Some((p, _)) => NormPath(p.to_owned()),
            None => NormPath::root(),
        })
    }

    pub fn join(&self, name: &str) -> NormPath {
        if self.is_root() {
            NormPath(name.to_owned())
        } else {
            NormPath(format!("{}/{}", self.0, name))
        }
    }

    pub fn file_name(&self) -> Option<&str> {
        if self.is_root() {
            None
        } else {
            self.0.rsplit('/').next()
        }
    }

    /// Whether `self` is `other` or lies beneath it.
    /// `self` 是否等于 `other` 或位于其下。
    pub fn starts_with(&self, other: &NormPath) -> bool {
        other.is_root()
            || self.0 == other.0
            || self
                .0
                .strip_prefix(&other.0)
                .is_some_and(|r| r.starts_with('/'))
    }
}

impl fmt::Display for NormPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn escape(raw: &str, why: &str) -> InfraError {
    InfraError::new(ErrorCode::PathEscape, format!("{why}: {raw:?}")).at(raw)
}

/// Normalize an API path. Absolute paths, `..` and NUL bytes are `PATH_ESCAPE`.
/// `a//b/./c/` becomes `a/b/c`; `""`, `"."` and `"./"` are the root.
/// 规范化 API 路径。绝对路径、`..` 和 NUL 字节都是 `PATH_ESCAPE`。
/// `a//b/./c/` 变成 `a/b/c`；`""`、`"."` 和 `"./"` 都表示根。
pub fn normalize(raw: &str) -> InfraResult<NormPath> {
    if raw.starts_with('/') {
        return Err(escape(raw, "absolute path"));
    }
    if raw.contains('\0') {
        return Err(escape(raw, "NUL byte in path"));
    }
    let mut parts: Vec<&str> = Vec::new();
    for c in raw.split('/') {
        match c {
            "" | "." => {}
            ".." => return Err(escape(raw, "`..` component")),
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        Ok(NormPath::root())
    } else {
        Ok(NormPath(parts.join("/")))
    }
}

/// Resolve a symlink target found at `link` (itself a normalized path) to a
/// normalized path, or `PATH_ESCAPE` if it leaves the workspace.
/// 把在 `link`（本身是规范化路径）处找到的符号链接目标解析为规范化路径；离开工作区则返回 `PATH_ESCAPE`。
pub(crate) fn resolve_link(link: &NormPath, target: &str) -> InfraResult<NormPath> {
    if target.starts_with('/') {
        return Err(escape(link.as_str(), "symlink points to an absolute path"));
    }
    let mut stack: Vec<String> = link
        .parent()
        .map_or_else(Vec::new, |p| p.components().map(str::to_owned).collect());
    for c in target.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(escape(
                        link.as_str(),
                        "symlink points outside the workspace",
                    ));
                }
            }
            other => stack.push(other.to_owned()),
        }
    }
    if stack.is_empty() {
        Ok(NormPath::root())
    } else {
        Ok(NormPath(stack.join("/")))
    }
}
