-- Shared fixture: one tenant, one repo with main at a genesis revision, a
-- coordinator, two workers and a system actor, and helpers for assertions.
-- 共享 fixture：一个租户、一个 main 指向创世 revision 的仓库、一个协调者、两个 worker 和一个 system actor，以及断言用的辅助函数。
SET search_path = infra, public;

INSERT INTO actors (id, tenant_id, kind, role, display_name) VALUES
  ('act_00000000000000000000000001', 't1', 'agent',  'coordinator', 'coordinator'),
  ('act_00000000000000000000000002', 't1', 'agent',  'worker',      'worker-a'),
  ('act_00000000000000000000000003', 't1', 'agent',  'worker',      'worker-b'),
  ('act_00000000000000000000000009', 't1', 'system', 'admin',       'infra');

INSERT INTO repositories (id, tenant_id, store_path) VALUES ('demo', 't1', '/var/lib/agentinfra/store/demo');
INSERT INTO revisions (id, tenant_id, repo_id, root_id, changeset_digest, published_by)
VALUES ('rev_00000000000000000000000001', 't1', 'demo', 'root_' || repeat('0', 64), 'genesis', 'act_00000000000000000000000009');
INSERT INTO refs (tenant_id, repo_id, name, revision, updated_by)
VALUES ('t1', 'demo', 'main', 'rev_00000000000000000000000001', 'act_00000000000000000000000009');

-- digest-shaped ids for tests: cm(1) = cm_000...001
-- 测试用的摘要形 id：cm(1) = cm_000...001
CREATE FUNCTION public.cm(n integer) RETURNS text LANGUAGE sql AS $$ SELECT 'cm_' || lpad(n::text, 64, '0') $$;
CREATE FUNCTION public.rt(n integer) RETURNS text LANGUAGE sql AS $$ SELECT 'root_' || lpad(n::text, 64, '0') $$;
CREATE FUNCTION public.uid(prefix text, n integer) RETURNS text LANGUAGE sql AS $$ SELECT prefix || '_' || lpad(n::text, 26, '0') $$;

-- Run `stmt` and require it to fail with Infra error `code`.
-- 执行 `stmt`，要求它以 Infra 错误 `code` 失败。
CREATE FUNCTION public.expect_error(stmt text, code text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  EXECUTE stmt;
  RAISE EXCEPTION 'expected % from: %', code, stmt USING ERRCODE = 'XX000';
EXCEPTION WHEN SQLSTATE 'IA000' THEN
  IF SQLERRM <> code THEN
    RAISE EXCEPTION 'expected %, got % from: %', code, SQLERRM, stmt USING ERRCODE = 'XX000';
  END IF;
END $$;

-- A branch at genesis plus an active workspace owned by worker-a.
-- 一条位于创世的分支，加一个属于 worker-a 的活跃工作区。
CREATE FUNCTION public.make_workspace(n integer, owner text DEFAULT 'act_00000000000000000000000002') RETURNS text
LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO infra.branches (id, tenant_id, repo_id, base_revision, head_commit, owner_actor)
  VALUES (uid('br', n), 't1', 'demo', 'rev_00000000000000000000000001', cm(0), owner);
  INSERT INTO infra.workspaces (id, tenant_id, branch_id, actor_id, projection)
  VALUES (uid('ws', n), 't1', uid('br', n), owner, 'fuse');
  RETURN uid('ws', n);
END $$;
