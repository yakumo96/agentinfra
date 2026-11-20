-- Work-item claim: priority order, dependencies, leases and the reaper.
-- 工作项认领：优先级顺序、依赖、租约和回收器。
SET search_path = infra, public;
DO $$
DECLARE
  c text := 'act_00000000000000000000000001';
  wa text := 'act_00000000000000000000000002';
  wb text := 'act_00000000000000000000000003';
  item work_items;
BEGIN
  INSERT INTO work_items (id, tenant_id, repo_id, title, spec, acceptance, priority, depends_on, created_by) VALUES
    (uid('wi', 1), 't1', 'demo', 'low',     's', 'a', 0, '{}', c),
    (uid('wi', 2), 't1', 'demo', 'high',    's', 'a', 5, '{}', c),
    (uid('wi', 3), 't1', 'demo', 'blocked', 's', 'a', 9, ARRAY[uid('wi', 1)], c);

  item := claim_work_item('t1', 'demo', wa);
  ASSERT item.id = uid('wi', 2), 'highest priority without open dependencies first';
  ASSERT item.state = 'claimed' AND item.claimed_by = wa AND item.claim_lease IS NOT NULL;
  item := claim_work_item('t1', 'demo', wb);
  ASSERT item.id = uid('wi', 1), 'wi3 depends on wi1, which is not done';
  ASSERT claim_work_item('t1', 'demo', wb) IS NULL, 'NO_WORK_AVAILABLE';

  -- leases renew on any call by the holder
  -- 持有者的任何调用都会续期租约
  ASSERT renew_leases(wa) = 1;
  -- expiry: the claim goes back to open, the workspace link is kept
  -- 过期：认领回到 open，保留工作区关联
  UPDATE work_items SET workspace_id = make_workspace(7, wb) WHERE id = uid('wi', 1);
  UPDATE leases SET expires_at = now() - interval '1 second' WHERE holder = wb;
  ASSERT reap_expired_leases() = 1;
  SELECT * INTO item FROM work_items WHERE id = uid('wi', 1);
  ASSERT item.state = 'open' AND item.claimed_by IS NULL AND item.workspace_id = uid('ws', 7);
  ASSERT (SELECT array_agg(kind ORDER BY id) FROM events_outbox WHERE kind <> 'workitem.claimed')
         = ARRAY['workitem.released', 'lease.expired'];
  ASSERT renew_leases(wb) = 0, 'expired leases are not revived';

  -- once wi1 is done, wi3 becomes claimable
  -- wi1 完成后，wi3 可以被认领
  UPDATE work_items SET state = 'done', claimed_by = wb WHERE id = uid('wi', 1);
  ASSERT (claim_work_item('t1', 'demo', wb)).id = uid('wi', 3);
END $$;
