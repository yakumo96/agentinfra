use std::fmt::Debug;

use crate::BatchSizer;

/// Result of folding one candidate onto a root with the merge engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome<Root> {
    Clean(Root),
    Conflict(Vec<String>),
}

/// A validation receipt. Only a receipt whose input is exactly the batch result
/// T, and whose `validate.toml` digest equals the one inside T, is evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validation<Root> {
    pub input_root: Root,
    pub validate_toml_digest: String,
    pub passed: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence<Root> {
    pub input_root: Root,
    pub validate_toml_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockReason {
    MergeConflict { paths: Vec<String> },
    ValidationFailed { summary: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<Rev, Root> {
    Published {
        revision: Rev,
        evidence: Evidence<Root>,
    },
    Blocked(BlockReason),
    /// The ref moved underneath the batch (only possible if something other
    /// than this queue wrote it); the entry goes back to the queue unchanged.
    Requeued,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError {
    /// The validator returned a receipt that is not about T. This is an Infra
    /// defect, not a property of any proposal: the queue stops and alerts.
    EvidenceMismatch {
        expected_root: String,
        receipt_root: String,
        expected_digest: String,
        receipt_digest: String,
    },
}

/// What the queue needs from the version engine, the validator and the control plane.
pub trait Engine {
    type Rev: Clone + Debug + PartialEq;
    type Root: Clone + Debug + PartialEq;
    type Entry: Debug;

    /// Current revision M of the target ref and its root.
    fn head(&mut self) -> (Self::Rev, Self::Root);
    /// Three-way merge of `entry`'s candidate onto `onto`, merge base = the entry's base revision.
    fn merge(&mut self, onto: &Self::Root, entry: &Self::Entry) -> MergeOutcome<Self::Root>;
    /// Digest of `.agentinfra/validate.toml` inside `root`.
    fn validate_toml_digest(&mut self, root: &Self::Root) -> String;
    /// Run validation in a throwaway container whose input is exactly `root`.
    fn validate(&mut self, root: &Self::Root) -> Validation<Self::Root>;
    /// Register `root` as the next revision and move the ref from `expected` to it.
    /// `None` means the conditional update affected no row.
    fn publish(&mut self, expected: &Self::Rev, root: &Self::Root) -> Option<Self::Rev>;
}

/// Per-entry outcomes of a batch, in input order.
pub type BatchOutcomes<E> = Vec<Outcome<<E as Engine>::Rev, <E as Engine>::Root>>;

/// Process one batch (the first `sizer.size()` queued entries, in order).
/// Returns one outcome per entry, in input order.
pub fn run_batch<E: Engine>(
    engine: &mut E,
    sizer: &mut BatchSizer,
    entries: &[E::Entry],
) -> Result<BatchOutcomes<E>, QueueError> {
    let mut outcomes: Vec<Option<Outcome<E::Rev, E::Root>>> =
        entries.iter().map(|_| None).collect();
    let all: Vec<usize> = (0..entries.len()).collect();
    let clean = attempt(engine, entries, &all, &mut outcomes)?;
    if clean {
        sizer.success();
    } else {
        sizer.failure();
    }
    Ok(outcomes
        .into_iter()
        .map(|o| o.unwrap_or(Outcome::Requeued))
        .collect())
}

/// Fold, validate and publish `idx`; bisect on validation failure.
/// Returns false when validation failed or the ref moved. Merge conflicts only
/// remove their entry from the batch; they do not count as a batch failure.
fn attempt<E: Engine>(
    engine: &mut E,
    entries: &[E::Entry],
    idx: &[usize],
    outcomes: &mut [Option<Outcome<E::Rev, E::Root>>],
) -> Result<bool, QueueError> {
    if idx.is_empty() {
        return Ok(true);
    }
    let (m_rev, m_root) = engine.head();
    let mut t = m_root;
    let mut included = Vec::new();
    for &i in idx {
        match engine.merge(&t, &entries[i]) {
            MergeOutcome::Clean(root) => {
                t = root;
                included.push(i);
            }
            MergeOutcome::Conflict(paths) => {
                outcomes[i] = Some(Outcome::Blocked(BlockReason::MergeConflict { paths }));
            }
        }
    }
    if included.is_empty() {
        return Ok(true);
    }

    let digest = engine.validate_toml_digest(&t);
    let v = engine.validate(&t);
    if v.input_root != t || v.validate_toml_digest != digest {
        return Err(QueueError::EvidenceMismatch {
            expected_root: format!("{t:?}"),
            receipt_root: format!("{:?}", v.input_root),
            expected_digest: digest,
            receipt_digest: v.validate_toml_digest,
        });
    }

    if v.passed {
        match engine.publish(&m_rev, &t) {
            Some(rev) => {
                let evidence = Evidence {
                    input_root: t,
                    validate_toml_digest: digest,
                };
                for &i in &included {
                    outcomes[i] = Some(Outcome::Published {
                        revision: rev.clone(),
                        evidence: evidence.clone(),
                    });
                }
                Ok(true)
            }
            None => {
                for &i in &included {
                    outcomes[i] = Some(Outcome::Requeued);
                }
                Ok(false)
            }
        }
    } else if included.len() == 1 {
        outcomes[included[0]] = Some(Outcome::Blocked(BlockReason::ValidationFailed {
            summary: v.summary,
        }));
        Ok(false)
    } else {
        let (left, right) = included.split_at(included.len() / 2);
        attempt(engine, entries, left, outcomes)?;
        attempt(engine, entries, right, outcomes)?;
        Ok(false)
    }
}
