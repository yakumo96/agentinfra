use crate::NormPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
}

/// What the workspace holds at one path, without following a final symlink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: Kind,
    /// Permission bits (`0o7777` mask).
    pub mode: u32,
    /// For files: SHA-256 of the content, lowercase hex.
    pub sha256: Option<String>,
    /// For symlinks: the raw link target.
    pub target: Option<String>,
}

/// Read access to the state a `ws_apply` call starts from.
///
/// Paths passed in are already normalized; implementations must not follow a
/// symlink in the final component of `node`.
pub trait FileView {
    fn node(&self, path: &NormPath) -> Option<Node>;
    /// File content; only called for paths whose node is a file.
    fn read(&self, path: &NormPath) -> Option<Vec<u8>>;
}
