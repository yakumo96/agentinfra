use std::collections::BTreeMap;

use infra_fileops::{Action, FileView, Kind, Node, NormPath, Op, normalize, plan};
use infra_proto::ErrorCode;

#[derive(Default)]
struct Mem {
    nodes: BTreeMap<String, (Node, Vec<u8>)>,
}

impl Mem {
    fn file(mut self, p: &str, content: &str) -> Self {
        let sha = format!("sha-of-{content}");
        self.nodes.insert(
            p.into(),
            (
                Node {
                    kind: Kind::File,
                    mode: 0o644,
                    sha256: Some(sha),
                    target: None,
                },
                content.into(),
            ),
        );
        self
    }
    fn dir(mut self, p: &str) -> Self {
        self.nodes.insert(
            p.into(),
            (
                Node {
                    kind: Kind::Dir,
                    mode: 0o755,
                    sha256: None,
                    target: None,
                },
                vec![],
            ),
        );
        self
    }
    fn link(mut self, p: &str, target: &str) -> Self {
        self.nodes.insert(
            p.into(),
            (
                Node {
                    kind: Kind::Symlink,
                    mode: 0o777,
                    sha256: None,
                    target: Some(target.into()),
                },
                vec![],
            ),
        );
        self
    }
}

impl FileView for Mem {
    fn node(&self, p: &NormPath) -> Option<Node> {
        if p.is_root() {
            return Some(Node {
                kind: Kind::Dir,
                mode: 0o755,
                sha256: None,
                target: None,
            });
        }
        self.nodes.get(p.as_str()).map(|(n, _)| n.clone())
    }
    fn read(&self, p: &NormPath) -> Option<Vec<u8>> {
        self.nodes.get(p.as_str()).map(|(_, c)| c.clone())
    }
}

fn p(s: &str) -> NormPath {
    normalize(s).unwrap()
}
fn write(path: &str, content: &str) -> Op {
    Op::Write {
        path: path.into(),
        content: content.into(),
        expected_sha256: None,
        mode: None,
    }
}
fn replace(path: &str, old: &str, new: &str) -> Op {
    Op::ReplaceText {
        path: path.into(),
        old: old.into(),
        new: new.into(),
        expected_count: 1,
    }
}
fn code(r: Result<infra_fileops::Plan, infra_proto::InfraError>) -> ErrorCode {
    r.unwrap_err().code
}

#[test]
fn normalizes_and_refuses_escapes() {
    assert_eq!(p("a//b/./c/").as_str(), "a/b/c");
    assert!(p("").is_root() && p("./").is_root());
    for bad in ["/etc/passwd", "../x", "a/../b", "a/\0b"] {
        assert_eq!(
            normalize(bad).unwrap_err().code,
            ErrorCode::PathEscape,
            "{bad:?}"
        );
    }
}

#[test]
fn write_creates_missing_parents() {
    let plan = plan(
        &Mem::default().dir("a"),
        &[write("a/b/c.txt", "hi")],
        None,
        0,
    )
    .unwrap();
    assert_eq!(
        plan.actions,
        vec![
            Action::MakeDirs { path: p("a/b") },
            Action::Put {
                path: p("a/b/c.txt"),
                bytes: b"hi".to_vec(),
                mode: 0o644
            }
        ]
    );
    assert_eq!(plan.changed_paths, vec![p("a/b/c.txt")]);
}

#[test]
fn write_under_a_file_is_refused() {
    let view = Mem::default().file("a", "x");
    assert_eq!(
        code(plan(&view, &[write("a/b", "y")], None, 0)),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn replace_text_counts_occurrences() {
    let view = Mem::default()
        .file("f.rs", "let a = 1; let a = 1;")
        .file("g.rs", "one");
    assert_eq!(
        code(plan(&view, &[replace("f.rs", "let a", "let b")], None, 0)),
        ErrorCode::TextNotUnique
    );
    assert_eq!(
        code(plan(&view, &[replace("g.rs", "two", "2")], None, 0)),
        ErrorCode::TextNotFound
    );
    assert_eq!(
        code(plan(&view, &[replace("nope.rs", "a", "b")], None, 0)),
        ErrorCode::NotFound
    );
    let both = Op::ReplaceText {
        path: "f.rs".into(),
        old: "= 1".into(),
        new: "= 2".into(),
        expected_count: 2,
    };
    let plan = plan(&view, &[both], None, 0).unwrap();
    assert_eq!(
        plan.actions,
        vec![Action::Put {
            path: p("f.rs"),
            bytes: b"let a = 2; let a = 2;".to_vec(),
            mode: 0o644
        }]
    );
}

#[test]
fn later_ops_see_earlier_ops_and_failure_is_all_or_nothing() {
    let view = Mem::default();
    let ok = plan(
        &view,
        &[
            write("n.txt", "alpha beta"),
            replace("n.txt", "beta", "gamma"),
        ],
        None,
        0,
    )
    .unwrap();
    assert_eq!(
        ok.actions.last(),
        Some(&Action::Put {
            path: p("n.txt"),
            bytes: b"alpha gamma".to_vec(),
            mode: 0o644
        })
    );

    let err = plan(
        &view,
        &[write("n.txt", "alpha"), replace("n.txt", "zeta", "x")],
        None,
        0,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::TextNotFound);
    assert!(err.message.starts_with("ops[1]:"), "{}", err.message);
}

#[test]
fn expected_sha_is_about_the_state_before_the_call() {
    let view = Mem::default().file("a", "v1");
    let stale = Op::Write {
        path: "a".into(),
        content: "v2".into(),
        expected_sha256: Some("sha-of-v0".into()),
        mode: None,
    };
    assert_eq!(
        code(plan(&view, &[stale], None, 0)),
        ErrorCode::RevisionMismatch
    );
    let fresh = Op::Write {
        path: "a".into(),
        content: "v2".into(),
        expected_sha256: Some("sha-of-v1".into()),
        mode: None,
    };
    assert!(plan(&view, std::slice::from_ref(&fresh), None, 0).is_ok());
    assert_eq!(
        code(plan(&view, &[write("a", "v0"), fresh], None, 0)),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn symlinks_resolve_inside_and_refuse_outside() {
    let view = Mem::default()
        .dir("real")
        .dir("real/docs")
        .link("docs", "real/docs")
        .link("out", "../../etc")
        .link("abs", "/etc");
    let plan = plan(&view, &[write("docs/a.md", "x")], None, 0).unwrap();
    assert_eq!(plan.changed_paths, vec![p("real/docs/a.md")]);
    assert_eq!(
        code(plan_of(&view, write("out/passwd", "x"))),
        ErrorCode::PathEscape
    );
    assert_eq!(
        code(plan_of(&view, write("abs/passwd", "x"))),
        ErrorCode::PathEscape
    );
    // deleting the link itself is fine: the final component is not followed
    assert_eq!(
        plan_of(
            &view,
            Op::Delete {
                path: "out".into(),
                expected_sha256: None
            }
        )
        .unwrap()
        .changed_paths,
        vec![p("out")]
    );
}

fn plan_of(view: &Mem, op: Op) -> Result<infra_fileops::Plan, infra_proto::InfraError> {
    plan(view, &[op], None, 0)
}

#[test]
fn delete_and_move_rules() {
    let view = Mem::default().dir("d").file("a", "1").file("b", "2");
    assert_eq!(
        code(plan_of(
            &view,
            Op::Delete {
                path: "d".into(),
                expected_sha256: None
            }
        )),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(plan_of(
            &view,
            Op::Delete {
                path: "zz".into(),
                expected_sha256: None
            }
        )),
        ErrorCode::NotFound
    );
    assert_eq!(
        code(plan_of(
            &view,
            Op::Move {
                from: "d".into(),
                to: "e".into()
            }
        )),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(plan_of(
            &view,
            Op::Move {
                from: "a".into(),
                to: "d".into()
            }
        )),
        ErrorCode::InvalidArgument
    );

    let mv = plan(
        &view,
        &[
            Op::Move {
                from: "a".into(),
                to: "x/y/a".into(),
            },
            replace("x/y/a", "1", "one"),
        ],
        None,
        0,
    )
    .unwrap();
    assert_eq!(mv.actions[0], Action::MakeDirs { path: p("x/y") });
    assert_eq!(
        mv.actions[1],
        Action::Rename {
            from: p("a"),
            to: p("x/y/a")
        }
    );
    assert_eq!(
        mv.actions[2],
        Action::Put {
            path: p("x/y/a"),
            bytes: b"one".to_vec(),
            mode: 0o644
        }
    );
    assert_eq!(mv.changed_paths, vec![p("a"), p("x/y/a")]);
    // the moved-away path no longer exists for later ops
    assert_eq!(
        code(plan(
            &view,
            &[
                Op::Move {
                    from: "a".into(),
                    to: "c".into()
                },
                replace("a", "1", "x")
            ],
            None,
            0
        )),
        ErrorCode::NotFound
    );
}

#[test]
fn chmod_and_generation() {
    let view = Mem::default().file("run.sh", "#!/bin/sh");
    assert_eq!(
        code(plan_of(
            &view,
            Op::Chmod {
                path: "run.sh".into(),
                mode: 0o17777
            }
        )),
        ErrorCode::InvalidArgument
    );
    let plan_ok = plan(
        &view,
        &[
            Op::Chmod {
                path: "run.sh".into(),
                mode: 0o755,
            },
            replace("run.sh", "sh", "bash"),
        ],
        None,
        0,
    )
    .unwrap();
    assert_eq!(
        plan_ok.actions[1],
        Action::Put {
            path: p("run.sh"),
            bytes: b"#!/bin/bash".to_vec(),
            mode: 0o755
        }
    );
    assert_eq!(
        code(plan(&view, &[write("x", "")], Some(3), 4)),
        ErrorCode::RevisionMismatch
    );
    assert_eq!(code(plan(&view, &[], None, 0)), ErrorCode::InvalidArgument);
    assert_eq!(
        code(plan(&view, &[write(".", "")], None, 0)),
        ErrorCode::InvalidArgument
    );
}

fn edit(path: &str, edits: &[(&str, &str)]) -> Op {
    Op::Edit {
        path: path.into(),
        edits: edits
            .iter()
            .map(|(a, b)| ((*a).into(), (*b).into()))
            .collect(),
    }
}

#[test]
fn pi_edit_matches_against_the_original_text() {
    let view = Mem::default().file("m.rs", "fn a() {}\nfn main() { a(); }\n");
    // the second edit's old text would not exist after the first one if applied incrementally
    let plan = plan_of(
        &view,
        edit("m.rs", &[("fn a()", "fn b()"), ("{ a(); }", "{ b(); }")]),
    )
    .unwrap();
    assert_eq!(
        plan.actions,
        vec![Action::Put {
            path: p("m.rs"),
            bytes: b"fn b() {}\nfn main() { b(); }\n".to_vec(),
            mode: 0o644
        }]
    );
    // newText that contains another edit's oldText is fine: matching uses the original
    assert!(
        plan_of(
            &view,
            edit(
                "m.rs",
                &[("fn a() {}", "fn a() {} fn main()"), ("a();", "x();")]
            )
        )
        .is_ok()
    );
    assert_eq!(
        code(plan_of(&view, edit("m.rs", &[("fn ", "pub fn ")]))),
        ErrorCode::TextNotUnique
    );
    assert_eq!(
        code(plan_of(&view, edit("m.rs", &[("zzz", "y")]))),
        ErrorCode::TextNotFound
    );
    assert_eq!(
        code(plan_of(
            &view,
            edit("m.rs", &[("fn a() {}", "x"), ("a() {", "y")])
        )),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        code(plan_of(&view, edit("m.rs", &[]))),
        ErrorCode::InvalidArgument
    );
}
