import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime"

// McpConnectionStatus mirrors the serialized Agent runtime state exactly.
// McpConnectionStatus 精确对应 Agent 运行时序列化状态。
export type McpConnectionStatus = "disabled" | "connecting" | "connected" | "disconnected" | "failed" | "stopping" | "stopped"

// McpToolInspection retains both original schemas without narrowing JSON Schema keywords.
// McpToolInspection 保留两份原始 Schema，不收窄 JSON Schema 关键字。
export interface McpToolInspection {
  // Original name, description, and full schemas from the server.
  // 服务器提供的原始名称、说明与完整 Schema。
  name: string; description: string; inputSchema: unknown; outputSchema: unknown
}

// McpServerInspection is one generation-consistent state and discovered tool catalog.
// McpServerInspection 是代次一致的状态与已发现工具目录。
export interface McpServerInspection {
  // Process-local catalog identity changes on reconnect or replacement.
  // 重连或替换时更新的进程内目录身份。
  inspectionGeneration: number
  // Runtime-owned connectivity and bounded server diagnostics.
  // 运行时拥有的连接状态与有界服务器诊断。
  state: {
    id: string; name: string; source: "user" | "plugin"; pluginId: string | null; generation: number | null
    transport: string; endpoint: string; status: McpConnectionStatus; retryCount: number
    protocolVersion: string | null; serverProduct: string | null; serverVersion: string | null
    toolCount: number; promptCount: number; resourceCount: number; lastError: string | null; recentLogs: string[]
  }
  // Discovery is independent of active AI tool permissions.
  // 目录发现独立于当前 AI 工具权限。
  tools: McpToolInspection[]
}

// McpCapabilities preserves optional metadata and independent discovery failures.
// McpCapabilities 保留可选元数据及各自的发现失败。
export interface McpCapabilities {
  // Wire metadata only; resource contents and prompt rendering are never requested.
  // 仅协议元数据；从不请求资源正文或提示词渲染。
  prompts: McpPromptMetadata[] | null; resources: McpResourceMetadata[] | null; promptError: string | null; resourceError: string | null
}

// McpPromptMetadata mirrors rmcp 3.1 Prompt while retaining optional wire extensions.
// McpPromptMetadata 对应 rmcp 3.1 Prompt，同时保留可选协议扩展。
export interface McpPromptMetadata {
  // Exact name and optional human-readable metadata from the prompt definition.
  // 提示词定义提供的精确名称与可选可读元数据。
  name: string; title?: string; description?: string
  // Declared prompt arguments, not argument values or rendered prompt contents.
  // 已声明的提示词参数，不包含参数值或渲染后的提示词正文。
  arguments?: Array<{ name: string; description?: string; required?: boolean }>
  // Preserve unrendered protocol fields for full metadata inspection.
  // 保留未单独渲染的协议字段，供完整元数据检查。
  [key: string]: unknown
}

// McpResourceMetadata mirrors rmcp 3.1 Resource without requesting its contents.
// McpResourceMetadata 对应 rmcp 3.1 Resource，不请求其正文。
export interface McpResourceMetadata {
  // Exact resource identity and optional description, MIME type and byte size.
  // 精确资源身份与可选描述、MIME 类型及字节大小。
  name: string; uri: string; title?: string; description?: string; mimeType?: string; size?: number
  // Preserve additional protocol fields for full metadata inspection.
  // 保留其余协议字段，供完整元数据检查。
  [key: string]: unknown
}

// loadMcpRuntimeSnapshot reads the published catalog without connecting any server.
// loadMcpRuntimeSnapshot 读取已发布目录，不连接任何服务器。
export async function loadMcpRuntimeSnapshot(): Promise<McpServerInspection[]> {
  if (!hasServiceRuntime()) throw new Error("MCP runtime is unavailable in preview mode.")
  return serviceClient.request<McpServerInspection[]>("service_mcp_runtime_snapshot")
}

// loadMcpServerCapabilities lists metadata only on the exact existing connection.
// loadMcpServerCapabilities 仅在精确现有连接上列出元数据。
export async function loadMcpServerCapabilities(serverId: string): Promise<McpCapabilities> {
  if (!hasServiceRuntime()) throw new Error("MCP runtime is unavailable in preview mode.")
  return serviceClient.request<McpCapabilities>("service_mcp_server_capabilities", { serverId })
}
