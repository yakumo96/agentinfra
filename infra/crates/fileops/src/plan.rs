use std::collections::{BTreeMap, BTreeSet, VecDeque};

use infra_proto::{ErrorCode, InfraError, InfraResult};

use crate::path::resolve_link;
use crate::{FileView, Kind, NormPath, normalize};

const DEFAULT_FILE_MODE: u32 = 0o644;
const DEFAULT_DIR_MODE: u32 = 0o755;
const MAX_SYMLINK_HOPS: usize = 40;

/// One `ws_apply` operation, already decoded from JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Write {
        path: String,
        content: Vec<u8>,
        expected_sha256: Option<String>,
        mode: Option<u32>,
    },
    ReplaceText {
        path: String,
        old: String,
        new: String,
        expected_count: u32,
    },
    /// pi's `edit`: every `(old, new)` is matched against the file as it was
    /// before this op; each `old` must occur exactly once and matches must not overlap.
    Edit {
        path: String,
        edits: Vec<(String, String)>,
    },
    Delete {
        path: String,
        expected_sha256: Option<String>,
    },
    Move {
        from: String,
        to: String,
    },
    Chmod {
        path: String,
        mode: u32,
    },
}

/// What the workspace writer does, in order, once the whole call validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// `mkdir -p` with default directory mode.
    MakeDirs {
        path: NormPath,
    },
    Put {
        path: NormPath,
        bytes: Vec<u8>,
        mode: u32,
    },
    Remove {
        path: NormPath,
    },
    Rename {
        from: NormPath,
        to: NormPath,
    },
    SetMode {
        path: NormPath,
        mode: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub actions: Vec<Action>,
    /// Sorted, unique. Empty means the call changes nothing.
    pub changed_paths: Vec<NormPath>,
}

#[derive(Debug, Clone)]
enum Src {
    Bytes(Vec<u8>),
    View(NormPath),
}

#[derive(Debug, Clone)]
enum Staged {
    File { src: Src, mode: u32 },
    Dir { mode: u32 },
    Symlink { target: String },
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SNode {
    kind: Kind,
    mode: u32,
    target: Option<String>,
}

struct Stage<'a> {
    view: &'a dyn FileView,
    over: BTreeMap<NormPath, Staged>,
    touched: BTreeSet<NormPath>,
}

impl<'a> Stage<'a> {
    fn node(&self, p: &NormPath) -> Option<SNode> {
        match self.over.get(p) {
            Some(Staged::File { mode, .. }) => Some(SNode {
                kind: Kind::File,
                mode: *mode,
                target: None,
            }),
            Some(Staged::Dir { mode }) => Some(SNode {
                kind: Kind::Dir,
                mode: *mode,
                target: None,
            }),
            Some(Staged::Symlink { target }) => Some(SNode {
                kind: Kind::Symlink,
                mode: 0o777,
                target: Some(target.clone()),
            }),
            Some(Staged::Absent) => None,
            None => self.view.node(p).map(|n| SNode {
                kind: n.kind,
                mode: n.mode,
                target: n.target,
            }),
        }
    }

    fn content(&self, p: &NormPath) -> Option<Vec<u8>> {
        match self.over.get(p) {
            Some(Staged::File {
                src: Src::Bytes(b), ..
            }) => Some(b.clone()),
            Some(Staged::File {
                src: Src::View(orig),
                ..
            }) => self.view.read(orig),
            Some(_) => None,
            None => self.view.read(p),
        }
    }

    fn staged_entry(&self, p: &NormPath) -> Option<Staged> {
        match self.over.get(p) {
            Some(Staged::Absent) => None,
            Some(s) => Some(s.clone()),
            None => self.view.node(p).map(|n| match n.kind {
                Kind::File => Staged::File {
                    src: Src::View(p.clone()),
                    mode: n.mode,
                },
                Kind::Dir => Staged::Dir { mode: n.mode },
                Kind::Symlink => Staged::Symlink {
                    target: n.target.unwrap_or_default(),
                },
            }),
        }
    }

    /// Resolve symlinks in every directory component (and the final one when
    /// `follow_final`), refusing anything that leaves the workspace.
    fn resolve(&self, p: &NormPath, follow_final: bool) -> InfraResult<NormPath> {
        let mut cur = NormPath::root();
        let mut queue: VecDeque<String> = p.components().map(str::to_owned).collect();
        let mut hops = 0;
        while let Some(c) = queue.pop_front() {
            let next = cur.join(&c);
            let last = queue.is_empty();
            match self.node(&next) {
                Some(n) if n.kind == Kind::Symlink && (!last || follow_final) => {
                    hops += 1;
                    if hops > MAX_SYMLINK_HOPS {
                        return Err(invalid(p, "too many levels of symbolic links"));
                    }
                    let target = resolve_link(&next, n.target.as_deref().unwrap_or(""))?;
                    let mut rest: VecDeque<String> =
                        target.components().map(str::to_owned).collect();
                    rest.extend(queue);
                    queue = rest;
                    cur = NormPath::root();
                }
                Some(n) if n.kind != Kind::Dir && !last => {
                    return Err(invalid(p, &format!("{next} is not a directory")));
                }
                _ => cur = next,
            }
        }
        Ok(cur)
    }

    /// Stage missing parent directories of `p`; error if one is not a directory.
    fn ensure_parents(&mut self, p: &NormPath, actions: &mut Vec<Action>) -> InfraResult<()> {
        let Some(parent) = p.parent() else {
            return Ok(());
        };
        let mut missing = Vec::new();
        let mut cur = Some(parent.clone());
        while let Some(dir) = cur {
            if dir.is_root() {
                break;
            }
            match self.node(&dir) {
                Some(n) if n.kind == Kind::Dir => break,
                Some(_) => return Err(invalid(p, &format!("{dir} is not a directory"))),
                None => missing.push(dir.clone()),
            }
            cur = dir.parent();
        }
        if !missing.is_empty() {
            for dir in missing {
                self.over.insert(
                    dir,
                    Staged::Dir {
                        mode: DEFAULT_DIR_MODE,
                    },
                );
            }
            actions.push(Action::MakeDirs { path: parent });
        }
        Ok(())
    }

    /// `expected_sha256` speaks about the state before this call, so it is only
    /// meaningful on the first operation that touches a path.
    fn check_expected(&self, p: &NormPath, expected: Option<&str>) -> InfraResult<()> {
        let Some(expected) = expected else {
            return Ok(());
        };
        if self.touched.contains(p) {
            return Err(invalid(
                p,
                "expected_sha256 is only allowed on the first operation touching a path",
            ));
        }
        let actual = self.view.node(p).and_then(|n| n.sha256);
        if actual.as_deref() != Some(expected) {
            let got = actual.unwrap_or_else(|| "<absent>".to_owned());
            return Err(InfraError::new(
                ErrorCode::RevisionMismatch,
                format!("{p}: expected sha256 {expected}, found {got}"),
            )
            .at(p.as_str()));
        }
        Ok(())
    }
}

fn invalid(p: &NormPath, why: &str) -> InfraError {
    InfraError::new(ErrorCode::InvalidArgument, format!("{p}: {why}")).at(p.as_str())
}

fn not_found(p: &NormPath) -> InfraError {
    InfraError::new(ErrorCode::NotFound, format!("{p}: no such file")).at(p.as_str())
}

fn non_root(raw: &str) -> InfraResult<NormPath> {
    let p = normalize(raw)?;
    if p.is_root() {
        return Err(invalid(
            &p,
            "operation needs a path below the workspace root",
        ));
    }
    Ok(p)
}

fn check_mode(p: &NormPath, mode: u32) -> InfraResult<()> {
    if mode > 0o7777 {
        Err(invalid(p, &format!("mode {mode:o} has bits outside 7777")))
    } else {
        Ok(())
    }
}

fn count_matches(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

/// Validate every op against `view` and return the writes to perform.
///
/// Nothing is written when this returns an error, and the error names the
/// failing op. `generation` is the workspace's current generation.
pub fn plan(
    view: &dyn FileView,
    ops: &[Op],
    expected_generation: Option<u64>,
    generation: u64,
) -> InfraResult<Plan> {
    if let Some(expected) = expected_generation {
        if expected != generation {
            return Err(InfraError::new(
                ErrorCode::RevisionMismatch,
                format!("expected generation {expected}, workspace is at {generation}"),
            ));
        }
    }
    if ops.is_empty() {
        return Err(InfraError::new(
            ErrorCode::InvalidArgument,
            "ops must not be empty",
        ));
    }
    let mut st = Stage {
        view,
        over: BTreeMap::new(),
        touched: BTreeSet::new(),
    };
    let mut actions = Vec::new();
    let mut changed = BTreeSet::new();
    for (i, op) in ops.iter().enumerate() {
        apply_one(&mut st, op, &mut actions, &mut changed).map_err(|mut e| {
            e.message = format!("ops[{i}]: {}", e.message);
            e
        })?;
    }
    Ok(Plan {
        actions,
        changed_paths: changed.into_iter().collect(),
    })
}

fn apply_one(
    st: &mut Stage<'_>,
    op: &Op,
    actions: &mut Vec<Action>,
    changed: &mut BTreeSet<NormPath>,
) -> InfraResult<()> {
    match op {
        Op::Write {
            path,
            content,
            expected_sha256,
            mode,
        } => {
            let p = st.resolve(&non_root(path)?, true)?;
            if let Some(m) = mode {
                check_mode(&p, *m)?;
            }
            let existing = st.node(&p);
            if existing.as_ref().is_some_and(|n| n.kind == Kind::Dir) {
                return Err(invalid(&p, "is a directory"));
            }
            st.check_expected(&p, expected_sha256.as_deref())?;
            st.ensure_parents(&p, actions)?;
            let mode = mode
                .or(existing.map(|n| n.mode))
                .unwrap_or(DEFAULT_FILE_MODE);
            st.over.insert(
                p.clone(),
                Staged::File {
                    src: Src::Bytes(content.clone()),
                    mode,
                },
            );
            actions.push(Action::Put {
                path: p.clone(),
                bytes: content.clone(),
                mode,
            });
            st.touched.insert(p.clone());
            changed.insert(p);
        }
        Op::ReplaceText {
            path,
            old,
            new,
            expected_count,
        } => {
            let p = st.resolve(&non_root(path)?, true)?;
            let node = st.node(&p).ok_or_else(|| not_found(&p))?;
            if node.kind != Kind::File {
                return Err(invalid(&p, "is not a regular file"));
            }
            if old.is_empty() {
                return Err(invalid(&p, "old text must not be empty"));
            }
            let bytes = st.content(&p).ok_or_else(|| not_found(&p))?;
            let text =
                String::from_utf8(bytes).map_err(|_| invalid(&p, "file is not UTF-8 text"))?;
            let n = count_matches(&text, old);
            if n == 0 {
                return Err(InfraError::new(
                    ErrorCode::TextNotFound,
                    format!("{p}: old text not found"),
                )
                .at(p.as_str()));
            }
            if n != *expected_count as usize {
                return Err(InfraError::new(
                    ErrorCode::TextNotUnique,
                    format!("{p}: old text occurs {n} times, expected {expected_count}"),
                )
                .at(p.as_str()));
            }
            let bytes = text.replace(old.as_str(), new).into_bytes();
            st.over.insert(
                p.clone(),
                Staged::File {
                    src: Src::Bytes(bytes.clone()),
                    mode: node.mode,
                },
            );
            actions.push(Action::Put {
                path: p.clone(),
                bytes,
                mode: node.mode,
            });
            st.touched.insert(p.clone());
            changed.insert(p);
        }
        Op::Edit { path, edits } => {
            let p = st.resolve(&non_root(path)?, true)?;
            let node = st.node(&p).ok_or_else(|| not_found(&p))?;
            if node.kind != Kind::File {
                return Err(invalid(&p, "is not a regular file"));
            }
            if edits.is_empty() {
                return Err(invalid(&p, "edits must not be empty"));
            }
            let bytes = st.content(&p).ok_or_else(|| not_found(&p))?;
            let text =
                String::from_utf8(bytes).map_err(|_| invalid(&p, "file is not UTF-8 text"))?;
            let mut spans = Vec::with_capacity(edits.len());
            for (j, (old, new)) in edits.iter().enumerate() {
                if old.is_empty() {
                    return Err(invalid(&p, &format!("edits[{j}].old must not be empty")));
                }
                let mut hits = text.match_indices(old.as_str());
                let Some((at, _)) = hits.next() else {
                    return Err(InfraError::new(
                        ErrorCode::TextNotFound,
                        format!("{p}: edits[{j}].old not found"),
                    )
                    .at(p.as_str()));
                };
                if hits.next().is_some() {
                    let n = count_matches(&text, old);
                    return Err(InfraError::new(
                        ErrorCode::TextNotUnique,
                        format!("{p}: edits[{j}].old occurs {n} times"),
                    )
                    .at(p.as_str()));
                }
                spans.push((at, at + old.len(), new.as_str(), j));
            }
            spans.sort_by_key(|s| s.0);
            for w in spans.windows(2) {
                if w[1].0 < w[0].1 {
                    return Err(invalid(
                        &p,
                        &format!("edits[{}] and edits[{}] overlap", w[0].3, w[1].3),
                    ));
                }
            }
            let mut out = String::with_capacity(text.len());
            let mut cursor = 0;
            for (start, end, new, _) in &spans {
                out.push_str(&text[cursor..*start]);
                out.push_str(new);
                cursor = *end;
            }
            out.push_str(&text[cursor..]);
            let bytes = out.into_bytes();
            st.over.insert(
                p.clone(),
                Staged::File {
                    src: Src::Bytes(bytes.clone()),
                    mode: node.mode,
                },
            );
            actions.push(Action::Put {
                path: p.clone(),
                bytes,
                mode: node.mode,
            });
            st.touched.insert(p.clone());
            changed.insert(p);
        }
        Op::Delete {
            path,
            expected_sha256,
        } => {
            let p = st.resolve(&non_root(path)?, false)?;
            let node = st.node(&p).ok_or_else(|| not_found(&p))?;
            if node.kind == Kind::Dir {
                return Err(invalid(
                    &p,
                    "is a directory; remove directories with run_exec",
                ));
            }
            st.check_expected(&p, expected_sha256.as_deref())?;
            st.over.insert(p.clone(), Staged::Absent);
            actions.push(Action::Remove { path: p.clone() });
            st.touched.insert(p.clone());
            changed.insert(p);
        }
        Op::Move { from, to } => {
            let src = st.resolve(&non_root(from)?, false)?;
            let dst = st.resolve(&non_root(to)?, false)?;
            if src == dst {
                return Err(invalid(&src, "source and destination are the same"));
            }
            let entry = st.staged_entry(&src).ok_or_else(|| not_found(&src))?;
            if matches!(entry, Staged::Dir { .. }) {
                return Err(invalid(
                    &src,
                    "is a directory; move directories with run_exec",
                ));
            }
            if st.node(&dst).is_some_and(|n| n.kind == Kind::Dir) {
                return Err(invalid(&dst, "destination is a directory"));
            }
            st.ensure_parents(&dst, actions)?;
            st.over.insert(dst.clone(), entry);
            st.over.insert(src.clone(), Staged::Absent);
            actions.push(Action::Rename {
                from: src.clone(),
                to: dst.clone(),
            });
            st.touched.insert(src.clone());
            st.touched.insert(dst.clone());
            changed.insert(src);
            changed.insert(dst);
        }
        Op::Chmod { path, mode } => {
            let p = st.resolve(&non_root(path)?, true)?;
            check_mode(&p, *mode)?;
            let entry = st.staged_entry(&p).ok_or_else(|| not_found(&p))?;
            let updated = match entry {
                Staged::File { src, .. } => Staged::File { src, mode: *mode },
                Staged::Dir { .. } => Staged::Dir { mode: *mode },
                other => other,
            };
            st.over.insert(p.clone(), updated);
            actions.push(Action::SetMode {
                path: p.clone(),
                mode: *mode,
            });
            st.touched.insert(p.clone());
            changed.insert(p);
        }
    }
    Ok(())
}
