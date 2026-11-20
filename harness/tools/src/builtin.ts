// The harness's four built-in tools keep pi's parameter shapes and call Infra.
// These targets are not exposed to the model as MCP tools a second time.
// harness 的四个内置工具沿用 pi 的参数形态，调用 Infra。
// 这些目标不会再以 MCP 工具的形式重复暴露给模型。
import type {
  ErrorCode,
  InfraToolName,
  InfraTools,
  RunExecOutput,
  WsApplyOutput,
  WsReadOutput,
} from "./generated/infra-tools.ts";

export type BuiltinName = "read" | "write" | "edit" | "bash";

/** pi coding-agent tool inputs (packages/coding-agent/src/core/tools).
 * pi coding-agent 的工具输入（packages/coding-agent/src/core/tools）。 */
export interface PiRead { path: string; offset?: number; limit?: number }
export interface PiWrite { path: string; content: string }
export interface PiEdit { path: string; edits: Array<{ oldText: string; newText: string }> }
export interface PiBash { command: string; timeout?: number }
export interface PiInputs { read: PiRead; write: PiWrite; edit: PiEdit; bash: PiBash }

export interface CallContext {
  sessionId: string;
  toolCallId: string;
  workspaceId: string;
  /** Where the workspace is mounted inside the container, e.g. /workspaces/ws_…
   * 工作区在容器内的挂载点，例如 /workspaces/ws_… */
  mountPoint: string;
}

export type InfraCall = { [K in InfraToolName]: { name: K; arguments: InfraTools[K]["input"] } }[InfraToolName];

export class PathOutsideWorkspace extends Error {
  readonly code: ErrorCode = "PATH_ESCAPE";
  readonly path: string;
  constructor(path: string) {
    super(`${path} is outside the workspace`);
    this.path = path;
  }
}

/** Same session + same tool call ⇒ same key, so a replayed call is naturally idempotent.
 * 同一会话 + 同一工具调用 ⇒ 同一个键，所以重放的调用天然幂等。 */
export function idempotencyKey(ctx: CallContext): string {
  return `${ctx.sessionId}:${ctx.toolCallId}`;
}

/**
 * pi accepts relative or absolute paths. Absolute paths are only valid inside
 * the workspace mount; everything else is refused before reaching Infra.
 *
 * pi 接受相对或绝对路径。绝对路径只在工作区挂载点内有效；其余的在到达 Infra 之前就被拒绝。
 */
export function workspacePath(path: string, ctx: CallContext): string {
  if (!path.startsWith("/")) return path === "" ? "." : path;
  const root = ctx.mountPoint.replace(/\/+$/, "");
  if (path === root) return ".";
  if (path.startsWith(root + "/")) return path.slice(root.length + 1);
  throw new PathOutsideWorkspace(path);
}

export function mapBuiltin<K extends BuiltinName>(tool: K, input: PiInputs[K], ctx: CallContext): InfraCall {
  const ws = ctx.workspaceId;
  switch (tool) {
    case "read": {
      const a = input as PiRead;
      return {
        name: "ws_read",
        arguments: {
          workspace_id: ws,
          path: workspacePath(a.path, ctx),
          ...(a.offset !== undefined ? { offset: a.offset } : {}),
          ...(a.limit !== undefined ? { limit: a.limit } : {}),
        },
      };
    }
    case "write": {
      const a = input as PiWrite;
      return {
        name: "ws_apply",
        arguments: {
          workspace_id: ws,
          idempotency_key: idempotencyKey(ctx),
          ops: [{ op: "write", path: workspacePath(a.path, ctx), content: a.content }],
        },
      };
    }
    case "edit": {
      const a = input as PiEdit;
      return {
        name: "ws_apply",
        arguments: {
          workspace_id: ws,
          idempotency_key: idempotencyKey(ctx),
          ops: [{ op: "edit", path: workspacePath(a.path, ctx), edits: a.edits.map((e) => ({ old: e.oldText, new: e.newText })) }],
        },
      };
    }
    case "bash": {
      const a = input as PiBash;
      return {
        name: "run_exec",
        arguments: {
          workspace_id: ws,
          idempotency_key: idempotencyKey(ctx),
          command: a.command,
          ...(a.timeout !== undefined ? { timeout_s: Math.max(1, Math.ceil(a.timeout)) } : {}),
        },
      };
    }
  }
  throw new Error(`unknown built-in tool ${String(tool)}`);
}

/** Text the model sees for a successful built-in call.
 * 内置工具调用成功时模型看到的文本。 */
export function renderResult(tool: BuiltinName, input: PiInputs[BuiltinName], output: unknown): string {
  switch (tool) {
    case "read": {
      const o = output as WsReadOutput;
      if (o.encoding === "base64") return `[binary file, ${o.size} bytes, sha256 ${o.sha256}]`;
      const more = o.next_offset !== null ? `\n\n[${o.total_lines ?? "?"} lines total; continue with offset=${o.next_offset}]` : "";
      return o.content + more;
    }
    case "write": {
      const o = output as WsApplyOutput;
      const a = input as PiWrite;
      return `Wrote ${Buffer.byteLength(a.content)} bytes to ${a.path}` + (o.up_to_date ? " (unchanged)" : "");
    }
    case "edit": {
      const a = input as PiEdit;
      return `Applied ${a.edits.length} edit${a.edits.length === 1 ? "" : "s"} to ${a.path}`;
    }
    case "bash": {
      const o = output as RunExecOutput;
      const parts = [o.stdout];
      if (o.stderr) parts.push(o.stderr);
      let text = parts.filter((p) => p !== "").join("\n");
      if (o.truncated) {
        const ids = [o.stdout_artifact_id, o.stderr_artifact_id].filter(Boolean).join(", ");
        text += `\n[output truncated; full output in artifact ${ids}]`;
      }
      if (o.state === "TIMED_OUT") text += "\n[timed out]";
      else if (o.exit_code !== 0) text += `\n[exit code ${o.exit_code ?? "unknown"}]`;
      return text;
    }
  }
}

/** Errors are reported to the model as text; retryable ones say so.
 * 错误以文本形式报告给模型；可重试的会注明。 */
export function renderError(err: { code: ErrorCode; message: string; retryable: boolean }): string {
  return `${err.code}: ${err.message}${err.retryable ? " (retryable)" : ""}`;
}
