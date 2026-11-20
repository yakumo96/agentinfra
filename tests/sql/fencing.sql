-- Mechanism two: write authority, fencing tokens and per-call commits.
-- 机制二：写入权、fencing token 和每次调用的提交。
SET search_path = infra, public;
DO $$
DECLARE
  ws text := make_workspace(1);
  wa text := 'act_00000000000000000000000002';
  wb text := 'act_00000000000000000000000003';
  sys text := 'act_00000000000000000000000009';
  t1 bigint; t2 bigint; t3 bigint;
BEGIN
  t1 := acquire_writer(ws, 'agent', wa);
  ASSERT acquire_writer(ws, 'agent', wa) = t1, 'same actor keeps its token';
  PERFORM expect_error(format('SELECT infra.acquire_writer(%L, %L, %L)', ws, 'agent', wb), 'WORKSPACE_BUSY');

  ASSERT record_commit(ws, t1, cm(0), cm(1), rt(1), 'cs1', '{a.txt}', wa, 'call_1', NULL, 'filesystem-consistent') = 1;
  ASSERT (SELECT head_commit FROM branches WHERE id = uid('br', 1)) = cm(1);
  ASSERT (SELECT parent FROM commits WHERE id = cm(1)) = cm(0);
  PERFORM expect_error(format(
    'SELECT infra.record_commit(%L, %s, %L, %L, %L, %L, %L, %L, %L, NULL, %L)',
    ws, t1, cm(0), cm(2), rt(2), 'cs2', '{b}', wa, 'call_2', 'filesystem-consistent'), 'REVISION_MISMATCH');

  -- restore/rebase: system takes over and rotates the token; the old writer is fenced out
  -- restore/rebase：system 接管并换发 token；旧写入者被挡在外面
  t2 := acquire_writer(ws, 'system', sys);
  ASSERT t2 > t1;
  PERFORM expect_error(format(
    'SELECT infra.record_commit(%L, %s, %L, %L, %L, %L, %L, %L, %L, NULL, %L)',
    ws, t1, cm(1), cm(2), rt(2), 'cs2', '{b}', wa, 'call_2', 'filesystem-consistent'), 'FENCED_OUT');
  ASSERT record_commit(ws, t2, cm(1), cm(3), rt(1), 'restore', '{}', sys, NULL, NULL, 'quiesced') = 2;

  PERFORM release_writer(ws, t2);
  PERFORM expect_error(format('SELECT infra.release_writer(%L, %s)', ws, t2), 'FENCED_OUT');
  t3 := acquire_writer(ws, 'agent', wb);
  ASSERT t3 > t2 AND t3 <> t1;
  ASSERT (SELECT count(*) FROM leases WHERE resource_id = ws AND state = 'active') = 1;
  PERFORM expect_error(format('SELECT infra.acquire_writer(%L, %L, %L)', 'ws_99999999999999999999999999', 'agent', wa), 'NOT_FOUND');
END $$;
