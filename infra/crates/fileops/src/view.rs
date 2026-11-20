use crate::NormPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
}

/// What the workspace holds at one path, without following a final symlink.
/// 工作区在某个路径上的内容，不跟随最后一级符号链接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: Kind,
    /// Permission bits (`0o7777` mask).
    /// 权限位（`0o7777` 掩码）。
    pub mode: u32,
    /// For files: SHA-256 of the content, lowercase hex.
    /// 文件：内容的 SHA-256，小写十六进制。
    pub sha256: Option<String>,
    /// For symlinks: the raw link target.
    /// 符号链接：原始链接目标。
    pub target: Option<String>,
}

/// Read access to the state a `ws_apply` call starts from.
///
/// Paths passed in are already normalized; implementations must not follow a
/// symlink in the final component of `node`.
///
/// 读取一次 `ws_apply` 调用起始状态的接口。
///
/// 传入的路径已经规范化；实现不得跟随 `node` 最后一级的符号链接。
pub trait FileView {
    fn node(&self, path: &NormPath) -> Option<Node>;
    /// File content; only called for paths whose node is a file.
    /// 文件内容；只对节点是文件的路径调用。
    fn read(&self, path: &NormPath) -> Option<Vec<u8>>;
}
