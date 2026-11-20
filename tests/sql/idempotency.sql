-- Mechanism four: idempotency records.
-- 机制四：幂等记录。
SET search_path = infra, public;
DO $$
DECLARE r record; a text := 'act_00000000000000000000000002';
BEGIN
  r := op_begin('t1', a, 'ws_apply', 'sess:call1', '\x01', uid('op', 1));
  ASSERT r.outcome = 'new' AND r.operation_id = uid('op', 1);
  r := op_begin('t1', a, 'ws_apply', 'sess:call1', '\x01', uid('op', 2));
  ASSERT r.outcome = 'in_progress' AND r.operation_id = uid('op', 1), 'replay while running';
  PERFORM op_finish(uid('op', 1), 'succeeded', '{"generation": 3}');
  r := op_begin('t1', a, 'ws_apply', 'sess:call1', '\x01', uid('op', 3));
  ASSERT r.outcome = 'replay' AND r.result = '{"generation": 3}'::jsonb;
  r := op_begin('t1', a, 'ws_apply', 'sess:call1', '\x02', uid('op', 4));
  ASSERT r.outcome = 'conflict';
  -- keys are scoped by actor and tool
  -- 键按 actor 和工具区分
  ASSERT (op_begin('t1', a, 'run_exec', 'sess:call1', '\x02', uid('op', 5))).outcome = 'new';
  ASSERT (op_begin('t1', 'act_00000000000000000000000003', 'ws_apply', 'sess:call1', '\x02', uid('op', 6))).outcome = 'new';
  -- expired records are replaced
  -- 过期记录会被替换
  UPDATE operations SET expires_at = now() - interval '1 second' WHERE operation_id = uid('op', 1);
  r := op_begin('t1', a, 'ws_apply', 'sess:call1', '\x02', uid('op', 7));
  ASSERT r.outcome = 'new' AND r.operation_id = uid('op', 7);
  PERFORM expect_error(format('SELECT infra.op_finish(%L, %L, NULL)', uid('op', 1), 'succeeded'), 'NOT_FOUND');
  PERFORM expect_error(format('SELECT infra.op_finish(%L, %L, NULL)', uid('op', 7), 'running'), 'INVALID_ARGUMENT');
END $$;
