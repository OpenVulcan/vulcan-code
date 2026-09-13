import type { McpServerSummary } from "../../src/types";
import type { McpCapabilities, McpServerInspection } from "../../src/service/mcp-settings-service";
import type { AuditScenario } from "./fixtures";

// auditMcpServers supplies fictional persisted entries; reserved endpoints are never contacted.
// auditMcpServers 提供虚构持久配置条目；保留域名端点从不被连接。
// The selected scenario controls emptiness; returned values use the production summary contract.
// 所选场景控制空态；返回值遵循生产摘要契约。
export function auditMcpServers(selected: AuditScenario): McpServerSummary[] {
  if (selected === "empty") return [];
  return ["connected", "failed", "connecting", "disabled"].map((status, index) => ({
    id: `audit-mcp-${status}`, name: ["合成文档索引 · 已连接", "合成远程服务 · 连接失败", "合成事件服务 · 连接中", "合成本地服务 · 已禁用"][index],
    enabled: status !== "disabled", transport: index === 0 || index === 3 ? "stdio" : "http",
    command: index === 0 || index === 3 ? "node" : "", args: index === 0 ? ["fictional-server.js", "--label", "a b"] : [],
    url: index === 1 || index === 2 ? "https://audit.invalid/mcp" : "", bearerToken: "", headers: [], headersFromEnv: [],
    oauthClientId: "", oauthClientSecret: "", oauthScopes: [], oauthRedirectUri: "", cwd: "", env: [], envPassthrough: [],
  }));
}

// auditMcpInspections projects actual runtime wire fields from exact fictional server identities.
// auditMcpInspections 按准确虚构服务器身份投影真实运行时协议字段。
// It returns metadata only and cannot create transports or execute tools.
// 仅返回元数据，无法创建传输或执行工具。
export function auditMcpInspections(selected: AuditScenario): McpServerInspection[] {
  return auditMcpServers(selected).map((server, index) => ({
    inspectionGeneration: index + 1,
    state: { id: server.id, name: server.name, source: "user", pluginId: null, generation: null,
      transport: server.transport, endpoint: server.url || server.command,
      status: (["connected", "failed", "connecting", "disabled"] as const)[index], retryCount: index === 1 ? 2 : 0,
      protocolVersion: index === 0 ? "2025-11-25" : null, serverProduct: index === 0 ? "Synthetic document catalog" : null,
      serverVersion: index === 0 ? "0.0.0-audit" : null, toolCount: index === 0 ? 1 : 0, promptCount: index === 0 ? 1 : 0, resourceCount: index === 0 ? 1 : 0,
      lastError: index === 1 ? "合成诊断：连接请求超时（未进行网络请求）" : null, recentLogs: index === 1 ? ["合成服务连接尝试未完成。"] : [],
    },
    tools: index === 0 ? [{ name: "search_documents", description: "Search the synthetic document catalog. Metadata fixture only; execution is unavailable.",
      inputSchema: { type: "object", required: ["query"], properties: { query: { type: "string", description: "Text to search", minLength: 1 },
        filters: { type: "object", properties: { tags: { type: "array", items: { type: "string", enum: ["guide", "reference"] } } }, additionalProperties: false },
        limit: { type: "integer", minimum: 1, maximum: 100, default: 10 } }, additionalProperties: false, "x-audit-extension": { preserved: true } },
      outputSchema: { oneOf: [{ type: "object", properties: { results: { type: "array", items: { type: "object", properties: { title: { type: "string" }, uri: { type: "string", format: "uri" } } } } } }, { type: "null" }] },
    }] : [],
  }));
}

// auditMcpCapabilities accepts only the exact connected fixture and never renders prompts or reads resources.
// auditMcpCapabilities 仅接纳准确已连接夹具，从不渲染提示词或读取资源正文。
// The server ID and scenario select optional metadata; unknown or disconnected identities reject.
// 服务器标识与场景选择可选元数据；未知或断开身份明确拒绝。
export function auditMcpCapabilities(serverId: string, selected: AuditScenario): McpCapabilities {
  if (!auditMcpInspections(selected).some(server => server.state.id === serverId && server.state.status === "connected")) {
    throw new Error("UI_AUDIT_MCP_NOT_CONNECTED");
  }
  return { prompts: [{ name: "summarize_document", title: "合成文档摘要", description: "元数据示例，不渲染或发送提示词。", arguments: [{ name: "document_uri", description: "Exact document URI", required: true }] }],
    resources: [{ name: "audit-handbook", title: "合成手册", uri: "audit://handbook", description: "仅展示资源元数据，不读取正文。", mimeType: "text/markdown", size: 2048 }], promptError: null, resourceError: null };
}
