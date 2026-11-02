//! The merge queue's decision logic, one target ref at a time.
//!
//! The runtime (one task per target ref in `infrad`) owns scheduling and I/O;
//! this crate owns the rules: batch sizing, folding candidates onto the ref's
//! head with the merge engine, validating the exact result, binding evidence to
//! it, conditional publication and bisecting failed batches.

mod batch;
mod queue;

pub use batch::BatchSizer;
pub use queue::{
    BatchOutcomes, BlockReason, Engine, Evidence, MergeOutcome, Outcome, QueueError, Validation,
    run_batch,
};
