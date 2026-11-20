use infra_proto::ids::{self, DigestKind, UlidKind};
use infra_proto::{ErrorCode, Role, RunState, TOOLS, names, tool, tools_for};

#[test]
fn tool_table_matches_schemas() {
    assert_eq!(TOOLS.len(), 33);
    let apply = tool(names::WS_APPLY).unwrap();
    assert!(apply.mutates);
    assert_eq!(apply.harness_builtin, &["write", "edit"]);
    assert!(apply.input_schema.contains("\"idempotency_key\""));
    assert!(
        !apply.input_schema.contains("$ref"),
        "published schemas are self-contained"
    );
    for t in TOOLS {
        assert!(
            t.input_schema.starts_with('{') && t.output_schema.starts_with('{'),
            "{}",
            t.name
        );
        assert_eq!(
            t.mutates,
            t.input_schema.contains("\"idempotency_key\""),
            "{}",
            t.name
        );
    }
}

#[test]
fn roles_filter_tools() {
    let coordinator: Vec<_> = tools_for(Role::Coordinator).map(|t| t.name).collect();
    assert!(coordinator.contains(&"workitem_create"));
    assert!(!coordinator.contains(&"ws_apply"));
    assert!(!coordinator.contains(&"proposal_submit"));
    let worker: Vec<_> = tools_for(Role::Worker).map(|t| t.name).collect();
    assert!(worker.contains(&"workitem_claim"));
    assert!(!worker.contains(&"workitem_create"));
    assert_eq!(tools_for(Role::Admin).count(), TOOLS.len());
}

#[test]
fn error_codes_round_trip() {
    for c in ErrorCode::ALL {
        assert_eq!(ErrorCode::parse(c.as_str()), Some(*c));
    }
    assert!(ErrorCode::WorkspaceBusy.retryable());
    assert!(!ErrorCode::FencedOut.retryable());
    assert_eq!(RunState::parse("TIMED_OUT"), Some(RunState::TimedOut));
}

#[test]
fn ulid_ids() {
    let a = ids::mint(UlidKind::Workspace).unwrap();
    let b = ids::mint(UlidKind::Workspace).unwrap();
    assert_ne!(a, b);
    assert!(ids::is_ulid_id(UlidKind::Workspace, &a), "{a}");
    assert!(!ids::is_ulid_id(UlidKind::Run, &a));
    assert_eq!(ids::encode_ulid(0, [0; 10]), "00000000000000000000000000");
    assert_eq!(
        ids::encode_ulid(0xFFFF_FFFF_FFFF, [0xFF; 10]),
        "7ZZZZZZZZZZZZZZZZZZZZZZZZZ"
    );
    // ordering follows time
    // 顺序跟随时间
    let early = ids::encode_ulid(1, [0xFF; 10]);
    let late = ids::encode_ulid(2, [0; 10]);
    assert!(early < late);
}

#[test]
fn digest_ids() {
    let hex = "a".repeat(64);
    let id = ids::digest_id(DigestKind::Commit, &hex).unwrap();
    assert!(ids::is_digest_id(DigestKind::Commit, &id));
    assert!(ids::digest_id(DigestKind::Commit, "ABC").is_none());
    assert!(!ids::is_digest_id(DigestKind::Root, &id));
}
