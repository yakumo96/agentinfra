use infra_publish::{
    BatchSizer, BlockReason, Engine, MergeOutcome, Outcome, QueueError, Validation, run_batch,
};

#[derive(Debug, Clone)]
struct Entry {
    name: &'static str,
    path: &'static str,
    bad: bool,
}

fn e(name: &'static str, path: &'static str) -> Entry {
    Entry {
        name,
        path,
        bad: false,
    }
}
fn bad(name: &'static str, path: &'static str) -> Entry {
    Entry {
        name,
        path,
        bad: true,
    }
}

/// Roots are the list of entries folded in, in order. Two entries touching the
/// same path conflict. Validation fails if a bad entry is in the root.
#[derive(Default)]
struct Fake {
    rev: u32,
    root: Vec<Entry>,
    validations: usize,
    lie_about_input: bool,
    ref_moves_before_publish: bool,
}

type Root = Vec<&'static str>;

impl Fake {
    fn names(root: &[Entry]) -> Root {
        root.iter().map(|e| e.name).collect()
    }
}

impl Engine for Fake {
    type Rev = u32;
    type Root = Root;
    type Entry = Entry;

    fn head(&mut self) -> (u32, Root) {
        (self.rev, Self::names(&self.root))
    }
    fn merge(&mut self, onto: &Root, entry: &Entry) -> MergeOutcome<Root> {
        let all = all_entries();
        let clash = onto
            .iter()
            .any(|n| all.iter().any(|x| x.name == *n && x.path == entry.path));
        if clash {
            return MergeOutcome::Conflict(vec![entry.path.to_owned()]);
        }
        let mut r = onto.clone();
        r.push(entry.name);
        MergeOutcome::Clean(r)
    }
    fn validate_toml_digest(&mut self, _root: &Root) -> String {
        "d1".into()
    }
    fn validate(&mut self, root: &Root) -> Validation<Root> {
        self.validations += 1;
        let all = all_entries();
        let failed = root
            .iter()
            .any(|n| all.iter().any(|x| x.name == *n && x.bad));
        let input_root = if self.lie_about_input {
            vec!["other"]
        } else {
            root.clone()
        };
        Validation {
            input_root,
            validate_toml_digest: "d1".into(),
            passed: !failed,
            summary: format!("{} failed", root.join("+")),
        }
    }
    fn publish(&mut self, expected: &u32, root: &Root) -> Option<u32> {
        if self.ref_moves_before_publish || *expected != self.rev {
            return None;
        }
        let all = all_entries();
        self.root = root
            .iter()
            .map(|n| all.iter().find(|x| x.name == *n).unwrap().clone())
            .collect();
        self.rev += 1;
        Some(self.rev)
    }
}

fn all_entries() -> Vec<Entry> {
    vec![
        e("a", "x"),
        e("b", "y"),
        bad("c", "z"),
        e("d", "w"),
        e("a2", "x"),
        e("solo", "s"),
    ]
}
fn pick(names: &[&str]) -> Vec<Entry> {
    names
        .iter()
        .map(|n| all_entries().into_iter().find(|x| x.name == *n).unwrap())
        .collect()
}
fn published(o: &Outcome<u32, Root>) -> Option<u32> {
    match o {
        Outcome::Published { revision, .. } => Some(*revision),
        _ => None,
    }
}

#[test]
fn batch_size_doubles_to_eight_and_resets() {
    let mut s = BatchSizer::default();
    let seen: Vec<usize> = (0..5)
        .map(|_| {
            let n = s.size();
            s.success();
            n
        })
        .collect();
    assert_eq!(seen, vec![1, 2, 4, 8, 8]);
    s.failure();
    assert_eq!(s.size(), 1);
}

#[test]
fn clean_entry_publishes_with_evidence_bound_to_t() {
    let mut f = Fake::default();
    let mut s = BatchSizer::default();
    let out = run_batch(&mut f, &mut s, &pick(&["solo"])).unwrap();
    match &out[0] {
        Outcome::Published { revision, evidence } => {
            assert_eq!(*revision, 1);
            assert_eq!(evidence.input_root, vec!["solo"]);
            assert_eq!(evidence.validate_toml_digest, "d1");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(s.size(), 2);
}

#[test]
fn failed_batch_is_bisected_to_the_bad_entry() {
    let mut f = Fake::default();
    let mut s = BatchSizer::new(8);
    s.success();
    s.success(); // size 4
    let out = run_batch(&mut f, &mut s, &pick(&["a", "b", "c", "d"])).unwrap();
    assert_eq!(published(&out[0]), Some(1));
    assert_eq!(published(&out[1]), Some(1));
    assert!(
        matches!(
            &out[2],
            Outcome::Blocked(BlockReason::ValidationFailed { .. })
        ),
        "{:?}",
        out[2]
    );
    assert_eq!(published(&out[3]), Some(2));
    // abcd fails, ab passes, cd fails, c fails, d passes
    assert_eq!(f.validations, 5);
    assert_eq!(Fake::names(&f.root), vec!["a", "b", "d"]);
    assert_eq!(s.size(), 1);
}

#[test]
fn conflicts_leave_the_batch_without_shrinking_it() {
    let mut f = Fake::default();
    let mut s = BatchSizer::default();
    s.success(); // size 2
    let out = run_batch(&mut f, &mut s, &pick(&["a", "a2"])).unwrap();
    assert_eq!(published(&out[0]), Some(1));
    assert_eq!(
        out[1],
        Outcome::Blocked(BlockReason::MergeConflict {
            paths: vec!["x".into()]
        })
    );
    assert_eq!(f.validations, 1);
    assert_eq!(s.size(), 4);
}

#[test]
fn receipt_not_about_t_stops_the_queue() {
    let mut f = Fake {
        lie_about_input: true,
        ..Fake::default()
    };
    let err = run_batch(&mut f, &mut BatchSizer::default(), &pick(&["solo"])).unwrap_err();
    assert!(matches!(err, QueueError::EvidenceMismatch { .. }));
    assert_eq!(f.rev, 0, "nothing published");
}

#[test]
fn moved_ref_requeues_without_blame() {
    let mut f = Fake {
        ref_moves_before_publish: true,
        ..Fake::default()
    };
    let mut s = BatchSizer::default();
    let out = run_batch(&mut f, &mut s, &pick(&["a", "b"])).unwrap();
    assert_eq!(out, vec![Outcome::Requeued, Outcome::Requeued]);
    assert_eq!(s.size(), 1);
}
