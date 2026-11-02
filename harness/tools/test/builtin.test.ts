import assert from "node:assert/strict";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { test } from "node:test";

import {
  InfraClient,
  InfraToolError,
  PathOutsideWorkspace,
  TOOL_META,
  fromSse,
  mapBuiltin,
  renderError,
  renderResult,
  type CallContext,
} from "../src/index.ts";

const ctx: CallContext = {
  sessionId: "sess_1",
  toolCallId: "call_9",
  workspaceId: "ws_01JBQ3Z8Y4K6M7N8P9QRSTVWXY",
  mountPoint: "/workspaces/ws_01JBQ3Z8Y4K6M7N8P9QRSTVWXY",
};

test("built-ins map onto the Infra tools named in the schemas", () => {
  for (const [builtin, target] of [["read", "ws_read"], ["write", "ws_apply"], ["edit", "ws_apply"], ["bash", "run_exec"]] as const) {
    assert.ok((TOOL_META[target].harnessBuiltin as readonly string[]).includes(builtin), `${builtin} -> ${target}`);
  }
});

test("pi edit becomes one ws_apply edit op, keyed by session and tool call", () => {
  const call = mapBuiltin("edit", { path: "/workspaces/ws_01JBQ3Z8Y4K6M7N8P9QRSTVWXY/src/a.rs", edits: [{ oldText: "a", newText: "b" }] }, ctx);
  assert.deepEqual(call, {
    name: "ws_apply",
    arguments: {
      workspace_id: ctx.workspaceId,
      idempotency_key: "sess_1:call_9",
      ops: [{ op: "edit", path: "src/a.rs", edits: [{ old: "a", new: "b" }] }],
    },
  });
});

test("read keeps pi's line offsets; bash timeout becomes whole seconds", () => {
  assert.deepEqual(mapBuiltin("read", { path: "README.md", offset: 10, limit: 5 }, ctx).arguments, {
    workspace_id: ctx.workspaceId, path: "README.md", offset: 10, limit: 5,
  });
  const bash = mapBuiltin("bash", { command: "cargo test", timeout: 1.5 }, ctx);
  assert.equal(bash.name, "run_exec");
  assert.equal((bash.arguments as { timeout_s: number }).timeout_s, 2);
});

test("absolute paths outside the mount never reach Infra", () => {
  assert.throws(() => mapBuiltin("write", { path: "/etc/passwd", content: "" }, ctx), PathOutsideWorkspace);
  assert.equal((mapBuiltin("read", { path: ctx.mountPoint }, ctx).arguments as { path: string }).path, ".");
});

test("results render like pi's", () => {
  const out = { run_id: "run_x", state: "SUCCEEDED", exit_code: 1, stdout: "ok", stderr: "warn", truncated: true,
    stdout_artifact_id: "art_1", stderr_artifact_id: null, output_commit: null } as const;
  assert.equal(renderResult("bash", { command: "x" }, out), "ok\nwarn\n[output truncated; full output in artifact art_1]\n[exit code 1]");
  const read = { content: "line\n", encoding: "utf-8", sha256: "0".repeat(64), size: 5, truncated: true, next_offset: 201, total_lines: 900 } as const;
  assert.match(renderResult("read", { path: "f" }, read), /continue with offset=201/);
  assert.equal(renderError({ code: "WORKSPACE_BUSY", message: "held", retryable: true }), "WORKSPACE_BUSY: held (retryable)");
});

test("SSE responses are matched by id", () => {
  const body = 'event: message\ndata: {"jsonrpc":"2.0","method":"notifications/progress"}\n\ndata: {"jsonrpc":"2.0","id":7,"result":{"ok":true}}\n\n';
  assert.deepEqual(fromSse(body, 7).result, { ok: true });
});

test("client sends the bearer token and maps tool errors", async () => {
  const seen: Array<{ auth: string | undefined; method: string }> = [];
  const server = createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      const msg = JSON.parse(raw);
      seen.push({ auth: req.headers.authorization, method: msg.method });
      const reply = (result: unknown) => {
        res.setHeader("content-type", "application/json");
        res.end(JSON.stringify({ jsonrpc: "2.0", id: msg.id, result }));
      };
      if (msg.params.name === "ws_status") {
        reply({ isError: true, content: [{ type: "text", text: "busy" }],
          structuredContent: { code: "WORKSPACE_BUSY", message: "held by act_x", retryable: true, details: {} } });
      } else {
        reply({ content: [{ type: "text", text: "ok" }], structuredContent: { revision: "rev_1", root_id: "root_1", version: 3 } });
      }
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  try {
    const { port } = server.address() as AddressInfo;
    const client = new InfraClient(`http://127.0.0.1:${port}/mcp`, "secret-token");
    const rev = await client.call("repo_resolve", { repo: "demo", ref: "main" });
    assert.equal(rev.version, 3);
    await assert.rejects(client.call("ws_status", { workspace_id: ctx.workspaceId }), (e: unknown) =>
      e instanceof InfraToolError && e.code === "WORKSPACE_BUSY" && e.retryable);
    assert.deepEqual(seen.map((s) => s.auth), ["Bearer secret-token", "Bearer secret-token"]);
  } finally {
    server.close();
  }
});
