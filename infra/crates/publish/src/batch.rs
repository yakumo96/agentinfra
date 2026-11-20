/// Batch size B: starts at 1, doubles after each fully published batch up to
/// `max` (8 by default), and drops back to 1 after any failure.
/// 批大小 B：从 1 开始，每完整发布一批就翻倍，上限 `max`（默认 8）；任何失败后退回 1。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchSizer {
    size: usize,
    max: usize,
}

impl BatchSizer {
    pub const DEFAULT_MAX: usize = 8;

    pub fn new(max: usize) -> Self {
        Self {
            size: 1,
            max: max.max(1),
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn success(&mut self) {
        self.size = (self.size * 2).min(self.max);
    }

    pub fn failure(&mut self) {
        self.size = 1;
    }
}

impl Default for BatchSizer {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX)
    }
}
