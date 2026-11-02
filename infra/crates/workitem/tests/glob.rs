use infra_workitem::{matches, may_overlap, overlapping, touched};

#[test]
fn concrete_matching() {
    assert!(matches("src/**", "src/a/b.rs"));
    assert!(matches("src/**", "src"));
    assert!(matches("**/*.md", "README.md"));
    assert!(matches("**/*.md", "docs/x/y.md"));
    assert!(matches("crates/*/Cargo.toml", "crates/fs/Cargo.toml"));
    assert!(!matches("crates/*/Cargo.toml", "crates/fs/sub/Cargo.toml"));
    assert!(matches("src/[a-c]?.rs", "src/b1.rs"));
    assert!(!matches("src/[!a-c]?.rs", "src/b1.rs"));
    assert!(!matches("docs/*.md", "src/a.md"));
}

#[test]
fn glob_overlap_is_conservative_but_tight_on_literals() {
    assert!(may_overlap("src/fs/**", "src/**/*.rs"));
    assert!(may_overlap("src/fs/mod.rs", "src/*/mod.rs"));
    assert!(!may_overlap("src/fs/**", "src/net/**"));
    assert!(!may_overlap("docs/*.md", "docs/*.txt"));
    assert!(may_overlap("docs/a*.md", "docs/*b.md"));
    assert!(!may_overlap("Cargo.toml", "Cargo.lock"));
    assert!(may_overlap("**", "anything/at/all"));
}

#[test]
fn report_helpers() {
    let mine = vec!["src/fs/**".to_owned()];
    let theirs = vec!["src/net/**".to_owned(), "src/**/mod.rs".to_owned()];
    assert_eq!(overlapping(&mine, &theirs), vec!["src/**/mod.rs"]);
    let changed = vec!["src/net/tcp.rs".to_owned()];
    assert_eq!(touched(&theirs, &changed), vec!["src/net/**"]);
}
