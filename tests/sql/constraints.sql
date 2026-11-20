-- Table constraints that back the schemas' enums and invariants.
-- 支撑 schema 枚举和不变式的表约束。
SET search_path = infra, public;
DO $$
BEGIN
  BEGIN
    INSERT INTO runs (id, tenant_id, kind, workspace_id, actor_id, command, state)
    VALUES (uid('run', 1), 't1', 'validation', NULL, 'act_00000000000000000000000009', 'make test', 'DONE');
    RAISE EXCEPTION 'bad run state accepted' USING ERRCODE = 'XX000';
  EXCEPTION WHEN check_violation THEN NULL;
  END;
  BEGIN
    INSERT INTO runs (id, tenant_id, kind, workspace_id, actor_id, command)
    VALUES (uid('run', 2), 't1', 'sync', NULL, 'act_00000000000000000000000002', 'ls');
    RAISE EXCEPTION 'sync run without workspace accepted' USING ERRCODE = 'XX000';
  EXCEPTION WHEN check_violation THEN NULL;
  END;
  BEGIN
    INSERT INTO workspaces (id, tenant_id, branch_id, actor_id, projection, writer_kind)
    VALUES (uid('ws', 5), 't1', 'br_x', 'act_00000000000000000000000002', 'fuse', 'agent');
    RAISE EXCEPTION 'writer without id accepted' USING ERRCODE = 'XX000';
  EXCEPTION WHEN check_violation OR foreign_key_violation THEN NULL;
  END;
  INSERT INTO audit_log (tenant_id, actor_id, tool, decision) VALUES ('t1', 'a', 'ws_read', 'allowed');
  BEGIN
    UPDATE audit_log SET decision = 'denied';
    RAISE EXCEPTION 'audit log was updated' USING ERRCODE = 'XX000';
  EXCEPTION WHEN raise_exception THEN NULL;
  END;
END $$;
