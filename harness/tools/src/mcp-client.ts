// Minimal MCP Streamable HTTP client for Infra (tools only, stateless requests).
// The token rides in the Authorization header; it never enters model context,
// tool arguments or logs.
// Infra 的最小 MCP Streamable HTTP 客户端（只用工具，请求无状态）。
// 令牌放在 Authorization 头里，从不进入模型上下文、工具参数或日志。
import type { ErrorCode, InfraToolName, InfraTools } from "./generated/infra-tools.ts";

/** Must match the protocol revision infrad is pinned to.
 * 必须与 infrad 固定的协议修订一致。 */
export const MCP_PROTOCOL_VERSION = "2025-11-25";

export class InfraToolError extends Error {
  readonly code: ErrorCode;
  readonly retryable: boolean;
  readonly details: Record<string, unknown>;
  constructor(code: ErrorCode, message: string, retryable: boolean, details: Record<string, unknown> = {}) {
    super(message);
    this.code = code;
    this.retryable = retryable;
    this.details = details;
  }
}

export class McpProtocolError extends Error {}

interface JsonRpcResponse {
  jsonrpc: "2.0";
  id: number | string | null;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

interface CallToolResult {
  content?: Array<{ type: string; text?: string }>;
  structuredContent?: unknown;
  isError?: boolean;
}

export interface McpTool {
  name: string;
  description?: string;
  inputSchema: unknown;
  outputSchema?: unknown;
}

export class InfraClient {
  private nextId = 1;
  private readonly url: string;
  private readonly token: string;
  private readonly fetchImpl: typeof fetch;

  constructor(url: string, token: string, fetchImpl: typeof fetch = fetch) {
    this.url = url;
    this.token = token;
    this.fetchImpl = fetchImpl;
  }

  async initialize(clientName = "agentinfra-harness", clientVersion = "0.1.0"): Promise<unknown> {
    return this.request("initialize", {
      protocolVersion: MCP_PROTOCOL_VERSION,
      capabilities: {},
      clientInfo: { name: clientName, version: clientVersion },
    });
  }

  async listTools(): Promise<McpTool[]> {
    const tools: McpTool[] = [];
    let cursor: string | undefined;
    do {
      const page = (await this.request("tools/list", cursor ? { cursor } : {})) as { tools: McpTool[]; nextCursor?: string };
      tools.push(...page.tools);
      cursor = page.nextCursor;
    } while (cursor);
    return tools;
  }

  /** Call an Infra tool; tool errors are thrown as InfraToolError.
   * 调用 Infra 工具；工具错误以 InfraToolError 抛出。 */
  async call<K extends InfraToolName>(name: K, args: InfraTools[K]["input"]): Promise<InfraTools[K]["output"]> {
    const result = (await this.request("tools/call", { name, arguments: args })) as CallToolResult;
    if (result.isError) {
      const e = (result.structuredContent ?? {}) as { code?: ErrorCode; message?: string; retryable?: boolean; details?: Record<string, unknown> };
      if (!e.code) throw new McpProtocolError(`tool ${name} failed without a structured error: ${textOf(result)}`);
      throw new InfraToolError(e.code, e.message ?? textOf(result), e.retryable ?? false, e.details ?? {});
    }
    if (result.structuredContent === undefined) throw new McpProtocolError(`tool ${name} returned no structuredContent`);
    return result.structuredContent as InfraTools[K]["output"];
  }

  private async request(method: string, params: unknown): Promise<unknown> {
    const id = this.nextId++;
    const res = await this.fetchImpl(this.url, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
        authorization: `Bearer ${this.token}`,
        "mcp-protocol-version": MCP_PROTOCOL_VERSION,
      },
      body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
    });
    if (!res.ok) throw new McpProtocolError(`${method}: HTTP ${res.status}`);
    const type = res.headers.get("content-type") ?? "";
    const body = await res.text();
    const msg = type.includes("text/event-stream") ? fromSse(body, id) : (JSON.parse(body) as JsonRpcResponse);
    if (msg.error) throw new McpProtocolError(`${method}: ${msg.error.code} ${msg.error.message}`);
    return msg.result;
  }
}

function textOf(r: CallToolResult): string {
  return (r.content ?? []).map((c) => c.text ?? "").join("\n");
}

/** Pick the JSON-RPC response with our id out of an SSE stream body.
 * 从 SSE 流的正文中挑出与我们的 id 对应的 JSON-RPC 响应。 */
export function fromSse(body: string, id: number): JsonRpcResponse {
  for (const event of body.split(/\r?\n\r?\n/)) {
    const data = event
      .split(/\r?\n/)
      .filter((l) => l.startsWith("data:"))
      .map((l) => l.slice(5).replace(/^ /, ""))
      .join("\n");
    if (!data) continue;
    const msg = JSON.parse(data) as JsonRpcResponse;
    if (msg.id === id) return msg;
  }
  throw new McpProtocolError(`no response with id ${id} in event stream`);
}
