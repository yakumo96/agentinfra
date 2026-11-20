//! The merge queue's decision logic, one target ref at a time.
//!
//! The runtime (one task per target ref in `infrad`) owns scheduling and I/O;
//! this crate owns the rules: batch sizing, folding candidates onto the ref's
//! head with the merge engine, validating the exact result, binding evidence to
//! it, conditional publication and bisecting failed batches.
//!
//! 合并队列的决策逻辑，每次针对一个目标 ref。
//!
//! 运行时（`infrad` 里每个目标 ref 一个任务）负责调度和 I/O；本 crate 负责规则：批大小、用合并引擎把候选
//! 折叠到 ref 的头上、验证确切的结果、把证据绑定到结果、条件发布、二分失败的批。

mod batch;
mod queue;

pub use batch::BatchSizer;
pub use queue::{
    BatchOutcomes, BlockReason, Engine, Evidence, MergeOutcome, Outcome, QueueError, Validation,
    run_batch,
};
