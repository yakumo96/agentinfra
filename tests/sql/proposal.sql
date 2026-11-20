-- Submit, supersede, block, publish; ref conditional update.
-- 提交、作废、阻塞、发布；ref 条件更新。
SET search_path = infra, public;
DO $$
DECLARE
  wa text := 'act_00000000000000000000000002';
  wb text := 'act_00000000000000000000000003';
  sys text := 'act_00000000000000000000000009';
  ws text; tok bigint; p proposals;
BEGIN
  INSERT INTO work_items (id, tenant_id, repo_id, title, spec, acceptance, created_by)
  VALUES (uid('wi', 1), 't1', 'demo', 'feature', 's', 'a', 'act_00000000000000000000000001');
  PERFORM claim_work_item('t1', 'demo', wa);
  ws := make_workspace(1, wa);
  tok := acquire_writer(ws, 'agent', wa);
  PERFORM record_commit(ws, tok, cm(0), cm(1), rt(1), 'cs1', '{src/a.rs}', wa, 'c1', NULL, 'filesystem-consistent');

  -- only the claimer may submit, and only the branch head
  -- 只有认领者能提交，且只能提交分支头
  PERFORM expect_error(format('SELECT infra.submit_proposal(%L, %L, %L, %L, %L, %L, %L, false)',
    uid('pr', 9), ws, uid('wi', 1), cm(1), 'main', 'm', wb), 'AUTHORIZATION_DENIED');
  PERFORM expect_error(format('SELECT infra.submit_proposal(%L, %L, %L, %L, %L, %L, %L, false)',
    uid('pr', 9), ws, uid('wi', 1), cm(0), 'main', 'm', wa), 'REVISION_MISMATCH');

  p := submit_proposal(uid('pr', 1), ws, uid('wi', 1), cm(1), 'main', 'first try', wa, false);
  ASSERT p.state = 'PROPOSED' AND p.base_revision = 'rev_00000000000000000000000001';
  ASSERT (SELECT state FROM work_items WHERE id = uid('wi', 1)) = 'in_review';
  ASSERT (SELECT count(*) FROM queue_entries WHERE state = 'queued') = 1;

  -- the queue blocks it on a conflict: work item goes back to its worker
  -- 队列因冲突阻塞它：工作项回到原 worker
  PERFORM block_proposal(uid('pr', 1), 'merge_conflict', '{src/a.rs}');
  ASSERT (SELECT state FROM work_items WHERE id = uid('wi', 1)) = 'claimed';
  ASSERT (SELECT conflicts FROM proposals WHERE id = uid('pr', 1)) = '{src/a.rs}';

  -- unresolved conflict markers stop a resubmit
  -- 未解决的冲突标记会阻止重新提交
  UPDATE workspaces SET unresolved_conflicts = '{src/a.rs}' WHERE id = ws;
  PERFORM expect_error(format('SELECT infra.submit_proposal(%L, %L, %L, %L, %L, %L, %L, false)',
    uid('pr', 2), ws, uid('wi', 1), cm(1), 'main', 'retry', wa), 'CONFLICT_MARKERS_PRESENT');
  UPDATE workspaces SET unresolved_conflicts = '{}' WHERE id = ws;
  PERFORM record_commit(ws, tok, cm(1), cm(2), rt(2), 'cs2', '{src/a.rs}', wa, 'c2', NULL, 'quiesced');

  -- resubmit supersedes the blocked proposal
  -- 重新提交会作废被阻塞的 Proposal
  p := submit_proposal(uid('pr', 2), ws, uid('wi', 1), cm(2), 'main', 'retry', wa, false);
  ASSERT (SELECT state || ':' || superseded_by FROM proposals WHERE id = uid('pr', 1)) = 'SUPERSEDED:' || uid('pr', 2);

  -- publish: ref moves only from the expected revision
  -- 发布：ref 只从预期的 revision 移动
  INSERT INTO revisions (id, tenant_id, repo_id, root_id, parents, changeset_digest, published_by)
  VALUES (uid('rev', 2), 't1', 'demo', rt(2), ARRAY['rev_00000000000000000000000001'], 'cs', sys);
  ASSERT advance_ref('demo', 'main', uid('rev', 9), uid('rev', 2), sys) IS NULL, 'PUBLICATION_CONFLICT';
  ASSERT advance_ref('demo', 'main', 'rev_00000000000000000000000001', uid('rev', 2), sys) = 2;
  ASSERT publish_proposals(ARRAY[uid('pr', 2)], uid('rev', 2), '{"input_root": "x", "validate_toml_sha256": "y"}') = 1;
  ASSERT (SELECT state FROM work_items WHERE id = uid('wi', 1)) = 'done';
  ASSERT (SELECT count(*) FROM leases WHERE resource_kind = 'work_item_claim' AND state = 'active') = 0;
  ASSERT (SELECT array_agg(kind ORDER BY id) FROM events_outbox WHERE aggregate_kind IN ('proposal', 'ref')) = ARRAY[
    'proposal.submitted', 'proposal.blocked', 'proposal.superseded', 'proposal.submitted', 'ref.advanced', 'proposal.published'];
  PERFORM expect_error(format('SELECT infra.publish_proposals(%L, %L, %L)', ARRAY[uid('pr', 2)], uid('rev', 2), '{}'), 'INVALID_ARGUMENT');
END $$;
