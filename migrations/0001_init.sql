-- 0001_init: control plane of the single-machine Infra.
--
-- Content is immutable and lives in the content store; every mutable pointer
-- and all control state live here. Order is always: content committed and
-- fsynced in the store first, then the pointer advances here.
--
-- Atomic mechanisms are SQL functions so that each one is a single statement
-- for infrad and is testable without it (tests/sql/). Infra errors are raised
-- with SQLSTATE 'IA000' and the error code (schemas/common/errors.json) as the
-- message; infrad maps them 1:1 onto tool errors.

BEGIN;

CREATE SCHEMA IF NOT EXISTS infra;
SET LOCAL search_path = infra;

CREATE TABLE schema_migrations (
  version    integer PRIMARY KEY,
  name       text NOT NULL,
  applied_at timestamptz NOT NULL DEFAULT now()
);

CREATE SEQUENCE fencing_seq;

-- ------------------------------------------------------------------ identity
CREATE TABLE actors (
  id           text PRIMARY KEY CHECK (id ~ '^act_[0-9A-HJKMNP-TV-Z]{26}$'),
  tenant_id    text NOT NULL,
  kind         text NOT NULL CHECK (kind IN ('agent', 'human', 'system')),
  role         text NOT NULL CHECK (role IN ('coordinator', 'worker', 'human', 'admin')),
  display_name text NOT NULL,
  created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE repositories (
  id          text PRIMARY KEY CHECK (id ~ '^[a-z0-9][a-z0-9._-]{0,63}$'),
  tenant_id   text NOT NULL,
  store_path  text NOT NULL,
  default_ref text NOT NULL DEFAULT 'main',
  created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE tokens (
  id          bigserial PRIMARY KEY,
  tenant_id   text NOT NULL,
  actor_id    text NOT NULL REFERENCES actors (id),
  token_hash  bytea NOT NULL UNIQUE,          -- sha256 of the 32-byte token; the token itself is never stored
  role        text NOT NULL CHECK (role IN ('coordinator', 'worker', 'human', 'admin')),
  repo_id     text REFERENCES repositories (id),
  scopes      text[] NOT NULL DEFAULT '{}',
  expires_at  timestamptz NOT NULL,
  revoked_at  timestamptz
);

-- ------------------------------------------------------------------ versions
CREATE TABLE revisions (
  id               text PRIMARY KEY CHECK (id ~ '^rev_[0-9A-HJKMNP-TV-Z]{26}$'),
  tenant_id        text NOT NULL,
  repo_id          text NOT NULL REFERENCES repositories (id),
  root_id          text NOT NULL CHECK (root_id ~ '^root_[0-9a-f]{64}$'),
  parents          text[] NOT NULL DEFAULT '{}',
  changeset_digest text NOT NULL,
  published_by     text NOT NULL,
  published_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX revisions_repo ON revisions (repo_id, published_at);

CREATE TABLE refs (
  tenant_id  text   NOT NULL,
  repo_id    text   NOT NULL REFERENCES repositories (id),
  name       text   NOT NULL,
  revision   text   NOT NULL REFERENCES revisions (id),
  version    bigint NOT NULL DEFAULT 1,
  updated_by text   NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (repo_id, name)
);

CREATE TABLE branches (
  id             text PRIMARY KEY CHECK (id ~ '^br_[0-9A-HJKMNP-TV-Z]{26}$'),
  tenant_id      text NOT NULL,
  repo_id        text NOT NULL REFERENCES repositories (id),
  base_revision  text NOT NULL REFERENCES revisions (id),
  head_commit    text NOT NULL CHECK (head_commit ~ '^cm_[0-9a-f]{64}$'),
  owner_actor    text NOT NULL REFERENCES actors (id),
  created_at     timestamptz NOT NULL DEFAULT now(),
  retained_until timestamptz            -- set when the last workspace is released and nothing published it
);

-- One per file-changing tool call. Content addressed: the id covers parent,
-- root, changeset and metadata, so inserting the same id twice is a no-op.
CREATE TABLE commits (
  id               text PRIMARY KEY CHECK (id ~ '^cm_[0-9a-f]{64}$'),
  tenant_id        text NOT NULL,
  branch_id        text NOT NULL REFERENCES branches (id),
  parent           text,
  root_id          text NOT NULL CHECK (root_id ~ '^root_[0-9a-f]{64}$'),
  changeset_digest text NOT NULL,
  changed_paths    text[] NOT NULL DEFAULT '{}',
  actor_id         text NOT NULL REFERENCES actors (id),
  tool_call_id     text,
  run_id           text,
  consistency      text NOT NULL CHECK (consistency IN ('filesystem-consistent', 'quiesced', 'best-effort')),
  created_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX commits_branch ON commits (branch_id, created_at);

CREATE TABLE git_map (
  tenant_id text NOT NULL,
  repo_id   text NOT NULL REFERENCES repositories (id),
  git_sha   text NOT NULL CHECK (git_sha ~ '^[0-9a-f]{40}([0-9a-f]{24})?$'),
  revision  text NOT NULL REFERENCES revisions (id),
  direction text NOT NULL CHECK (direction IN ('import', 'export')),
  PRIMARY KEY (repo_id, git_sha)
);

-- --------------------------------------------------------------- execution
CREATE TABLE environments (
  id           text PRIMARY KEY,            -- digest of root id, platform and capability list
  tenant_id    text NOT NULL,
  root_id      text NOT NULL CHECK (root_id ~ '^root_[0-9a-f]{64}$'),
  platform     text NOT NULL,
  capabilities jsonb NOT NULL DEFAULT '{}',
  created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sandboxes (
  id              text PRIMARY KEY,
  tenant_id       text NOT NULL,
  actor_id        text NOT NULL REFERENCES actors (id),
  environment_id  text NOT NULL REFERENCES environments (id),
  lxc_name        text NOT NULL UNIQUE,
  sandboxd_unit   text NOT NULL,
  ns_handles      jsonb NOT NULL DEFAULT '{}',  -- namespace file handles (name_to_handle_at), for reconciliation
  workspace_count integer NOT NULL DEFAULT 0 CHECK (workspace_count BETWEEN 0 AND 16),
  state           text NOT NULL CHECK (state IN ('starting', 'running', 'idle', 'stopping', 'stopped', 'lost')),
  created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE workspaces (
  id                   text PRIMARY KEY CHECK (id ~ '^ws_[0-9A-HJKMNP-TV-Z]{26}$'),
  tenant_id            text NOT NULL,
  branch_id            text NOT NULL REFERENCES branches (id),
  actor_id             text NOT NULL REFERENCES actors (id),
  projection           text NOT NULL CHECK (projection IN ('fuse', 'materialized')),
  sandbox_id           text REFERENCES sandboxes (id),
  generation           bigint NOT NULL DEFAULT 0,
  writer_kind          text NOT NULL DEFAULT 'none' CHECK (writer_kind IN ('none', 'agent', 'system')),
  writer_id            text,
  fencing_token        bigint NOT NULL DEFAULT nextval('fencing_seq'),
  state                text NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'released', 'recovered')),
  consistency          text NOT NULL DEFAULT 'filesystem-consistent'
                         CHECK (consistency IN ('filesystem-consistent', 'quiesced', 'best-effort')),
  dirty_after_crash    boolean NOT NULL DEFAULT false,
  unresolved_conflicts text[] NOT NULL DEFAULT '{}',
  created_at           timestamptz NOT NULL DEFAULT now(),
  released_at          timestamptz,
  CHECK ((writer_kind = 'none') = (writer_id IS NULL))
);
CREATE INDEX workspaces_actor ON workspaces (actor_id) WHERE state = 'active';

CREATE TABLE checkpoints (
  id           text PRIMARY KEY CHECK (id ~ '^ck_[0-9A-HJKMNP-TV-Z]{26}$'),
  tenant_id    text NOT NULL,
  workspace_id text NOT NULL REFERENCES workspaces (id),
  branch_id    text NOT NULL REFERENCES branches (id),
  commit_id    text NOT NULL REFERENCES commits (id),
  name         text,
  created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE leases (
  id            bigserial PRIMARY KEY,
  tenant_id     text NOT NULL,
  resource_kind text NOT NULL CHECK (resource_kind IN ('workspace_writer', 'work_item_claim')),
  resource_id   text NOT NULL,
  holder        text NOT NULL,
  fencing_token bigint,
  expires_at    timestamptz NOT NULL,
  state         text NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'released', 'expired'))
);
CREATE UNIQUE INDEX leases_one_active ON leases (resource_kind, resource_id) WHERE state = 'active';
CREATE INDEX leases_expiry ON leases (expires_at) WHERE state = 'active';
CREATE INDEX leases_holder ON leases (holder) WHERE state = 'active';

CREATE TABLE runs (
  id             text PRIMARY KEY CHECK (id ~ '^run_[0-9A-HJKMNP-TV-Z]{26}$'),
  tenant_id      text NOT NULL,
  kind           text NOT NULL CHECK (kind IN ('sync', 'background', 'validation')),
  workspace_id   text REFERENCES workspaces (id),      -- NULL for validation runs
  actor_id       text NOT NULL REFERENCES actors (id),
  environment_id text REFERENCES environments (id),
  command        text NOT NULL,
  cwd            text NOT NULL DEFAULT '.',
  state          text NOT NULL DEFAULT 'CREATED' CHECK (state IN
                   ('CREATED', 'STARTING', 'RUNNING', 'FINALIZING', 'SUCCEEDED', 'FAILED', 'CANCELLED', 'TIMED_OUT', 'LOST')),
  exit_code      integer,
  fencing_token  bigint,
  input_commit   text,
  input_root     text,
  output_commit  text,
  timeout_s      integer,
  cancel_requested boolean NOT NULL DEFAULT false,
  created_at     timestamptz NOT NULL DEFAULT now(),
  started_at     timestamptz,
  finished_at    timestamptz,
  CHECK ((kind = 'validation') = (workspace_id IS NULL))
);
CREATE INDEX runs_workspace ON runs (workspace_id) WHERE state NOT IN ('SUCCEEDED', 'FAILED', 'CANCELLED', 'TIMED_OUT', 'LOST');

CREATE TABLE artifacts (
  id         text PRIMARY KEY CHECK (id ~ '^art_[0-9a-f]{64}$'),
  tenant_id  text NOT NULL,
  object_id  text NOT NULL,
  size       bigint NOT NULL CHECK (size >= 0),
  kind       text NOT NULL CHECK (kind IN ('stdout', 'stderr', 'patch', 'file', 'report')),
  run_id     text REFERENCES runs (id),
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE run_receipts (
  run_id          text PRIMARY KEY REFERENCES runs (id),
  tenant_id       text NOT NULL,
  input_commit    text,
  input_root      text NOT NULL,
  output_commit   text,
  stdout_artifact text REFERENCES artifacts (id),
  stderr_artifact text REFERENCES artifacts (id),
  evidence        jsonb NOT NULL DEFAULT '{}',   -- validation: {validate_toml_sha256, passed, summary}
  sealed_at       timestamptz NOT NULL DEFAULT now()
);

-- ------------------------------------------------------------ collaboration
CREATE TABLE work_items (
  id           text PRIMARY KEY CHECK (id ~ '^wi_[0-9A-HJKMNP-TV-Z]{26}$'),
  tenant_id    text NOT NULL,
  repo_id      text NOT NULL REFERENCES repositories (id),
  title        text NOT NULL CHECK (length(title) BETWEEN 1 AND 300),
  spec         text NOT NULL,
  acceptance   text NOT NULL,
  scope_paths  text[] NOT NULL DEFAULT '{}',
  depends_on   text[] NOT NULL DEFAULT '{}',
  priority     integer NOT NULL DEFAULT 0,
  state        text NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'claimed', 'in_review', 'done', 'blocked', 'cancelled')),
  claimed_by   text REFERENCES actors (id),
  claim_lease  bigint REFERENCES leases (id),
  workspace_id text REFERENCES workspaces (id),
  proposal_id  text,
  created_by   text NOT NULL REFERENCES actors (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  CHECK ((state IN ('claimed', 'in_review')) = (claimed_by IS NOT NULL) OR state = 'done')
);
CREATE INDEX work_items_open ON work_items (repo_id, priority DESC, created_at) WHERE state = 'open';

CREATE TABLE proposals (
  id               text PRIMARY KEY CHECK (id ~ '^pr_[0-9A-HJKMNP-TV-Z]{26}$'),
  tenant_id        text NOT NULL,
  repo_id          text NOT NULL REFERENCES repositories (id),
  work_item_id     text NOT NULL REFERENCES work_items (id),
  workspace_id     text NOT NULL REFERENCES workspaces (id),
  base_revision    text NOT NULL REFERENCES revisions (id),
  candidate_commit text NOT NULL REFERENCES commits (id),
  target_ref       text NOT NULL,
  message          text NOT NULL,
  state            text NOT NULL CHECK (state IN ('PROPOSED', 'VALIDATING', 'READY', 'PUBLISHED', 'BLOCKED', 'REJECTED', 'SUPERSEDED')),
  blocked_reason   text CHECK (blocked_reason IN ('merge_conflict', 'validation_failed', 'approval_required')),
  conflicts        text[] NOT NULL DEFAULT '{}',
  evidence         jsonb,
  published_revision text REFERENCES revisions (id),
  superseded_by    text REFERENCES proposals (id),
  submitted_by     text NOT NULL REFERENCES actors (id),
  created_at       timestamptz NOT NULL DEFAULT now(),
  updated_at       timestamptz NOT NULL DEFAULT now(),
  CHECK ((state = 'BLOCKED') = (blocked_reason IS NOT NULL)),
  CHECK ((state = 'PUBLISHED') = (published_revision IS NOT NULL AND evidence IS NOT NULL))
);
ALTER TABLE work_items ADD FOREIGN KEY (proposal_id) REFERENCES proposals (id);

CREATE TABLE queue_entries (
  id              bigserial PRIMARY KEY,
  tenant_id       text NOT NULL,
  repo_id         text NOT NULL REFERENCES repositories (id),
  target_ref      text NOT NULL,
  proposal_id     text NOT NULL REFERENCES proposals (id),
  batch_id        bigint,
  attempt         integer NOT NULL DEFAULT 0,
  state           text NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'in_batch', 'published', 'blocked', 'removed')),
  merged_revision text REFERENCES revisions (id),
  enqueued_at     timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX queue_one_live_entry ON queue_entries (proposal_id) WHERE state IN ('queued', 'in_batch');
CREATE INDEX queue_order ON queue_entries (repo_id, target_ref, id) WHERE state = 'queued';

-- -------------------------------------------------------- idempotency, events
CREATE TABLE operations (
  tenant_id       text  NOT NULL,
  actor_id        text  NOT NULL,
  kind            text  NOT NULL,
  idempotency_key text  NOT NULL,
  request_digest  bytea NOT NULL,
  operation_id    text  NOT NULL UNIQUE CHECK (operation_id ~ '^op_[0-9A-HJKMNP-TV-Z]{26}$'),
  state           text  NOT NULL CHECK (state IN ('running', 'succeeded', 'failed')),
  result          jsonb,
  created_at      timestamptz NOT NULL DEFAULT now(),
  expires_at      timestamptz NOT NULL,
  PRIMARY KEY (tenant_id, actor_id, kind, idempotency_key)
);
CREATE INDEX operations_expiry ON operations (expires_at);

CREATE TABLE events_outbox (
  id             bigserial PRIMARY KEY,
  tenant_id      text NOT NULL,
  aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('work_item', 'proposal', 'ref', 'run', 'workspace', 'lease')),
  aggregate_id   text NOT NULL,
  kind           text NOT NULL,
  payload        jsonb NOT NULL DEFAULT '{}',
  created_at     timestamptz NOT NULL DEFAULT now(),
  delivered_at   timestamptz
);

CREATE TABLE audit_log (
  id       bigserial PRIMARY KEY,
  at       timestamptz NOT NULL DEFAULT now(),
  tenant_id text NOT NULL,
  actor_id text NOT NULL,
  tool     text NOT NULL,
  resource text,
  decision text NOT NULL CHECK (decision IN ('allowed', 'denied')),
  detail   jsonb NOT NULL DEFAULT '{}'
);

CREATE FUNCTION refuse_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION '% is append-only', TG_TABLE_NAME;
END $$;
CREATE TRIGGER audit_log_append_only BEFORE UPDATE OR DELETE ON audit_log
  FOR EACH ROW EXECUTE FUNCTION refuse_change();

-- ================================================================ functions
CREATE FUNCTION fail(code text, detail text DEFAULT NULL) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION USING ERRCODE = 'IA000', MESSAGE = code, DETAIL = coalesce(detail, '');
END $$;

-- Events are written in the same transaction as the state change they describe.
CREATE FUNCTION emit(p_tenant text, p_aggregate_kind text, p_aggregate_id text, p_kind text, p_payload jsonb DEFAULT '{}')
RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE v_id bigint;
BEGIN
  INSERT INTO infra.events_outbox (tenant_id, aggregate_kind, aggregate_id, kind, payload)
  VALUES (p_tenant, p_aggregate_kind, p_aggregate_id, p_kind, p_payload)
  RETURNING id INTO v_id;
  PERFORM pg_notify('infra_events', v_id::text);
  RETURN v_id;
END $$;

-- Mechanism one: conditional ref update. NULL means the ref moved (PUBLICATION_CONFLICT).
CREATE FUNCTION advance_ref(p_repo text, p_ref text, p_expected text, p_new text, p_actor text)
RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE v_version bigint; v_tenant text;
BEGIN
  UPDATE infra.refs SET revision = p_new, version = version + 1, updated_by = p_actor, updated_at = now()
  WHERE repo_id = p_repo AND name = p_ref AND revision = p_expected
  RETURNING version, tenant_id INTO v_version, v_tenant;
  IF FOUND THEN
    PERFORM infra.emit(v_tenant, 'ref', p_repo || ':' || p_ref, 'ref.advanced',
                       jsonb_build_object('from', p_expected, 'to', p_new, 'version', v_version));
  END IF;
  RETURN v_version;
END $$;

-- Mechanism two: write authority with fencing tokens.
--   agent: takes a free workspace, or re-acquires its own (token unchanged);
--   system: always takes over and rotates the token (restore, rebase, reaper).
CREATE FUNCTION acquire_writer(p_ws text, p_kind text, p_actor text, p_lease interval DEFAULT '60 seconds')
RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE w infra.workspaces; v_token bigint;
BEGIN
  SELECT * INTO w FROM infra.workspaces WHERE id = p_ws FOR UPDATE;
  IF NOT FOUND THEN PERFORM infra.fail('NOT_FOUND', p_ws); END IF;
  IF w.state <> 'active' THEN PERFORM infra.fail('NOT_FOUND', p_ws || ' is ' || w.state); END IF;
  IF p_kind NOT IN ('agent', 'system') THEN PERFORM infra.fail('INVALID_ARGUMENT', 'writer kind ' || p_kind); END IF;

  IF w.writer_kind = p_kind AND w.writer_id = p_actor AND p_kind = 'agent' THEN
    v_token := w.fencing_token;
  ELSIF w.writer_kind = 'none' OR p_kind = 'system' THEN
    v_token := nextval('infra.fencing_seq');
    UPDATE infra.workspaces SET writer_kind = p_kind, writer_id = p_actor, fencing_token = v_token WHERE id = p_ws;
    UPDATE infra.leases SET state = 'released'
      WHERE resource_kind = 'workspace_writer' AND resource_id = p_ws AND state = 'active';
  ELSE
    PERFORM infra.fail('WORKSPACE_BUSY', p_ws || ' is held by ' || w.writer_kind || ' ' || w.writer_id);
  END IF;

  INSERT INTO infra.leases (tenant_id, resource_kind, resource_id, holder, fencing_token, expires_at)
  VALUES (w.tenant_id, 'workspace_writer', p_ws, p_actor, v_token, now() + p_lease)
  ON CONFLICT (resource_kind, resource_id) WHERE state = 'active'
  DO UPDATE SET expires_at = EXCLUDED.expires_at;
  RETURN v_token;
END $$;

-- Give up write authority. The token rotates so a stale holder cannot write.
CREATE FUNCTION release_writer(p_ws text, p_token bigint) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  UPDATE infra.workspaces SET writer_kind = 'none', writer_id = NULL, fencing_token = nextval('infra.fencing_seq')
  WHERE id = p_ws AND fencing_token = p_token;
  IF NOT FOUND THEN PERFORM infra.fail('FENCED_OUT', p_ws); END IF;
  UPDATE infra.leases SET state = 'released'
  WHERE resource_kind = 'workspace_writer' AND resource_id = p_ws AND state = 'active';
END $$;

-- Record a commit and advance the branch head at a tool-call boundary.
-- The commit's content is already durable in the store. Returns the new generation.
CREATE FUNCTION record_commit(
  p_ws text, p_token bigint, p_expected_head text,
  p_commit text, p_root text, p_changeset_digest text, p_changed_paths text[],
  p_actor text, p_tool_call text, p_run text, p_consistency text)
RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE w infra.workspaces; v_gen bigint;
BEGIN
  SELECT * INTO w FROM infra.workspaces WHERE id = p_ws FOR UPDATE;
  IF NOT FOUND THEN PERFORM infra.fail('NOT_FOUND', p_ws); END IF;
  IF w.writer_kind = 'none' OR w.fencing_token <> p_token THEN
    PERFORM infra.fail('FENCED_OUT', format('%s: token %s, current %s', p_ws, p_token, w.fencing_token));
  END IF;

  INSERT INTO infra.commits (id, tenant_id, branch_id, parent, root_id, changeset_digest, changed_paths,
                             actor_id, tool_call_id, run_id, consistency)
  VALUES (p_commit, w.tenant_id, w.branch_id, p_expected_head, p_root, p_changeset_digest, p_changed_paths,
          p_actor, p_tool_call, p_run, p_consistency)
  ON CONFLICT (id) DO NOTHING;

  UPDATE infra.branches SET head_commit = p_commit WHERE id = w.branch_id AND head_commit = p_expected_head;
  IF NOT FOUND THEN PERFORM infra.fail('REVISION_MISMATCH', 'branch head moved'); END IF;

  UPDATE infra.workspaces SET generation = generation + 1, consistency = p_consistency, dirty_after_crash = false
  WHERE id = p_ws RETURNING generation INTO v_gen;
  RETURN v_gen;
END $$;

-- Mechanism three: leases. Any call by a holder renews what it holds.
CREATE FUNCTION renew_leases(p_holder text, p_lease interval DEFAULT '60 seconds') RETURNS integer
LANGUAGE sql AS $$
  WITH r AS (
    UPDATE infra.leases SET expires_at = greatest(expires_at, now() + p_lease)
    WHERE holder = p_holder AND state = 'active' AND expires_at > now()
    RETURNING 1)
  SELECT count(*)::integer FROM r;
$$;

-- The reaper: expire leases, take back write authority (new token), return
-- claimed work items to `open` while keeping their workspace for the next claimer.
CREATE FUNCTION reap_expired_leases() RETURNS integer LANGUAGE plpgsql AS $$
DECLARE l infra.leases; n integer := 0;
BEGIN
  FOR l IN SELECT * FROM infra.leases WHERE state = 'active' AND expires_at <= now()
           ORDER BY id FOR UPDATE SKIP LOCKED
  LOOP
    UPDATE infra.leases SET state = 'expired' WHERE id = l.id;
    IF l.resource_kind = 'workspace_writer' THEN
      UPDATE infra.workspaces SET writer_kind = 'none', writer_id = NULL, fencing_token = nextval('infra.fencing_seq')
      WHERE id = l.resource_id AND fencing_token = l.fencing_token;
    ELSE
      UPDATE infra.work_items SET state = 'open', claimed_by = NULL, claim_lease = NULL, updated_at = now()
      WHERE id = l.resource_id AND claim_lease = l.id AND state = 'claimed';
      IF FOUND THEN
        PERFORM infra.emit(l.tenant_id, 'work_item', l.resource_id, 'workitem.released',
                           jsonb_build_object('reason', 'lease_expired', 'holder', l.holder));
      END IF;
    END IF;
    PERFORM infra.emit(l.tenant_id, 'lease', l.id::text, 'lease.expired',
                       jsonb_build_object('resource_kind', l.resource_kind, 'resource_id', l.resource_id, 'holder', l.holder));
    n := n + 1;
  END LOOP;
  RETURN n;
END $$;

-- Atomic claim; items with unfinished dependencies are skipped; concurrent
-- claimers do not block each other. NULL means NO_WORK_AVAILABLE.
CREATE FUNCTION claim_work_item(p_tenant text, p_repo text, p_actor text, p_lease interval DEFAULT '60 seconds')
RETURNS infra.work_items LANGUAGE plpgsql AS $$
DECLARE v_id text; v_lease bigint; v_item infra.work_items;
BEGIN
  SELECT w.id INTO v_id FROM infra.work_items w
  WHERE w.tenant_id = p_tenant AND w.repo_id = p_repo AND w.state = 'open'
    AND NOT EXISTS (SELECT 1 FROM infra.work_items d WHERE d.id = ANY (w.depends_on) AND d.state <> 'done')
  ORDER BY w.priority DESC, w.created_at, w.id
  FOR UPDATE SKIP LOCKED
  LIMIT 1;
  IF v_id IS NULL THEN RETURN NULL; END IF;

  INSERT INTO infra.leases (tenant_id, resource_kind, resource_id, holder, expires_at)
  VALUES (p_tenant, 'work_item_claim', v_id, p_actor, now() + p_lease)
  RETURNING id INTO v_lease;

  UPDATE infra.work_items SET state = 'claimed', claimed_by = p_actor, claim_lease = v_lease, updated_at = now()
  WHERE id = v_id RETURNING * INTO v_item;
  PERFORM infra.emit(p_tenant, 'work_item', v_id, 'workitem.claimed', jsonb_build_object('actor', p_actor));
  RETURN v_item;
END $$;

-- Mechanism four: idempotency. Outcome is one of
--   new         first time: run the operation, then op_finish
--   replay      same request, finished: return `result`
--   in_progress same request, still running: OPERATION_IN_PROGRESS with operation_id
--   conflict    same key, different request: IDEMPOTENCY_CONFLICT
CREATE FUNCTION op_begin(p_tenant text, p_actor text, p_kind text, p_key text, p_digest bytea, p_new_op text,
                         p_ttl interval DEFAULT '24 hours',
                         OUT outcome text, OUT operation_id text, OUT state text, OUT result jsonb)
LANGUAGE plpgsql AS $$
DECLARE o infra.operations;
BEGIN
  DELETE FROM infra.operations
  WHERE tenant_id = p_tenant AND actor_id = p_actor AND kind = p_kind AND idempotency_key = p_key AND expires_at <= now();

  INSERT INTO infra.operations (tenant_id, actor_id, kind, idempotency_key, request_digest, operation_id, state, expires_at)
  VALUES (p_tenant, p_actor, p_kind, p_key, p_digest, p_new_op, 'running', now() + p_ttl)
  ON CONFLICT DO NOTHING;
  IF FOUND THEN
    outcome := 'new'; operation_id := p_new_op; state := 'running'; result := NULL;
    RETURN;
  END IF;

  SELECT * INTO o FROM infra.operations
  WHERE tenant_id = p_tenant AND actor_id = p_actor AND kind = p_kind AND idempotency_key = p_key;
  operation_id := o.operation_id; state := o.state; result := o.result;
  IF o.request_digest <> p_digest THEN outcome := 'conflict';
  ELSIF o.state = 'running' THEN outcome := 'in_progress';
  ELSE outcome := 'replay';
  END IF;
END $$;

CREATE FUNCTION op_finish(p_op text, p_state text, p_result jsonb) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  IF p_state NOT IN ('succeeded', 'failed') THEN PERFORM infra.fail('INVALID_ARGUMENT', 'final state ' || p_state); END IF;
  UPDATE infra.operations SET state = p_state, result = p_result WHERE operation_id = p_op AND state = 'running';
  IF NOT FOUND THEN PERFORM infra.fail('NOT_FOUND', p_op || ' is not running'); END IF;
END $$;

-- Submit: supersede the work item's live proposal, enqueue (unless blocked for
-- approval), move the work item to in_review. Protected-path policy is decided
-- by infrad from the changeset and passed in as p_needs_approval.
CREATE FUNCTION submit_proposal(p_id text, p_ws text, p_work_item text, p_candidate text, p_target_ref text,
                                p_message text, p_actor text, p_needs_approval boolean)
RETURNS infra.proposals LANGUAGE plpgsql AS $$
DECLARE w infra.workspaces; b infra.branches; wi infra.work_items; prev text; p infra.proposals;
BEGIN
  SELECT * INTO wi FROM infra.work_items WHERE id = p_work_item FOR UPDATE;
  IF NOT FOUND THEN PERFORM infra.fail('NOT_FOUND', p_work_item); END IF;
  IF wi.claimed_by IS DISTINCT FROM p_actor OR wi.state NOT IN ('claimed', 'in_review') THEN
    PERFORM infra.fail('AUTHORIZATION_DENIED', p_work_item || ' is not claimed by the caller');
  END IF;
  SELECT * INTO w FROM infra.workspaces WHERE id = p_ws;
  IF NOT FOUND OR w.actor_id <> p_actor THEN PERFORM infra.fail('AUTHORIZATION_DENIED', p_ws); END IF;
  IF cardinality(w.unresolved_conflicts) > 0 THEN
    PERFORM infra.fail('CONFLICT_MARKERS_PRESENT', array_to_string(w.unresolved_conflicts, ', '));
  END IF;
  SELECT * INTO b FROM infra.branches WHERE id = w.branch_id;
  IF b.head_commit <> p_candidate THEN PERFORM infra.fail('REVISION_MISMATCH', 'candidate is not the branch head'); END IF;

  SELECT id INTO prev FROM infra.proposals
  WHERE work_item_id = p_work_item AND state IN ('PROPOSED', 'VALIDATING', 'READY', 'BLOCKED')
  FOR UPDATE;

  INSERT INTO infra.proposals (id, tenant_id, repo_id, work_item_id, workspace_id, base_revision, candidate_commit,
                               target_ref, message, state, blocked_reason, submitted_by)
  VALUES (p_id, w.tenant_id, b.repo_id, p_work_item, p_ws, b.base_revision, p_candidate, p_target_ref, p_message,
          CASE WHEN p_needs_approval THEN 'BLOCKED' ELSE 'PROPOSED' END,
          CASE WHEN p_needs_approval THEN 'approval_required' END,
          p_actor)
  RETURNING * INTO p;

  IF prev IS NOT NULL THEN
    UPDATE infra.proposals SET state = 'SUPERSEDED', blocked_reason = NULL, superseded_by = p_id, updated_at = now()
    WHERE id = prev;
    UPDATE infra.queue_entries SET state = 'removed' WHERE proposal_id = prev AND state IN ('queued', 'in_batch');
    PERFORM infra.emit(w.tenant_id, 'proposal', prev, 'proposal.superseded', jsonb_build_object('by', p_id));
  END IF;

  IF NOT p_needs_approval THEN
    INSERT INTO infra.queue_entries (tenant_id, repo_id, target_ref, proposal_id)
    VALUES (w.tenant_id, b.repo_id, p_target_ref, p_id);
  END IF;
  UPDATE infra.work_items SET state = 'in_review', proposal_id = p_id, workspace_id = p_ws, updated_at = now()
  WHERE id = p_work_item;
  PERFORM infra.emit(w.tenant_id, 'proposal', p_id, CASE WHEN p_needs_approval THEN 'proposal.blocked' ELSE 'proposal.submitted' END,
                     jsonb_build_object('work_item_id', p_work_item, 'supersedes', prev));
  RETURN p;
END $$;

-- Merge queue results. Published proposals mark their work items done;
-- blocked ones send the work item back to `claimed` for the same worker.
CREATE FUNCTION publish_proposals(p_ids text[], p_revision text, p_evidence jsonb) RETURNS integer
LANGUAGE plpgsql AS $$
DECLARE p infra.proposals; n integer := 0;
BEGIN
  FOR p IN SELECT * FROM infra.proposals WHERE id = ANY (p_ids) ORDER BY id FOR UPDATE LOOP
    IF p.state NOT IN ('PROPOSED', 'VALIDATING', 'READY') THEN
      PERFORM infra.fail('INVALID_ARGUMENT', p.id || ' is ' || p.state);
    END IF;
    UPDATE infra.proposals SET state = 'PUBLISHED', published_revision = p_revision, evidence = p_evidence, updated_at = now()
    WHERE id = p.id;
    UPDATE infra.queue_entries SET state = 'published', merged_revision = p_revision
    WHERE proposal_id = p.id AND state IN ('queued', 'in_batch');
    UPDATE infra.work_items SET state = 'done', updated_at = now() WHERE id = p.work_item_id;
    UPDATE infra.leases SET state = 'released'
    WHERE resource_kind = 'work_item_claim' AND resource_id = p.work_item_id AND state = 'active';
    PERFORM infra.emit(p.tenant_id, 'proposal', p.id, 'proposal.published', jsonb_build_object('revision', p_revision));
    PERFORM infra.emit(p.tenant_id, 'work_item', p.work_item_id, 'workitem.done', jsonb_build_object('proposal_id', p.id));
    n := n + 1;
  END LOOP;
  IF n <> cardinality(p_ids) THEN PERFORM infra.fail('NOT_FOUND', 'unknown proposal in batch'); END IF;
  RETURN n;
END $$;

CREATE FUNCTION block_proposal(p_id text, p_reason text, p_conflicts text[], p_evidence jsonb DEFAULT NULL)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE p infra.proposals;
BEGIN
  UPDATE infra.proposals SET state = 'BLOCKED', blocked_reason = p_reason, conflicts = coalesce(p_conflicts, '{}'),
         evidence = coalesce(p_evidence, evidence), updated_at = now()
  WHERE id = p_id AND state IN ('PROPOSED', 'VALIDATING', 'READY')
  RETURNING * INTO p;
  IF NOT FOUND THEN PERFORM infra.fail('NOT_FOUND', p_id || ' is not in the queue'); END IF;
  UPDATE infra.queue_entries SET state = 'blocked' WHERE proposal_id = p_id AND state IN ('queued', 'in_batch');
  UPDATE infra.work_items SET state = 'claimed', updated_at = now() WHERE id = p.work_item_id AND state = 'in_review';
  PERFORM infra.emit(p.tenant_id, 'proposal', p_id, 'proposal.blocked',
                     jsonb_build_object('reason', p_reason, 'conflicts', coalesce(p_conflicts, '{}')));
END $$;

INSERT INTO schema_migrations (version, name) VALUES (1, 'init');

COMMIT;
