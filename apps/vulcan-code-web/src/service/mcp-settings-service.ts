import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime"

// McpConnectionStatus mirrors the serialized Agent runtime state exactly.
// McpConnectionStatus 精确对应 Agent 运行时序列化状态。
export type McpConnectionStatus = "disabled" | "connecting" | "awaiting_authorization" | "connected" | "disconnected" | "failed" | "stopping" | "stopped"

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

// McpAuthStatus mirrors the runtime credential status codes.
// McpAuthStatus 对应运行时凭据状态代码。
export type McpAuthStatus = "not_configured" | "not_authenticated" | "authenticated" | "expired"

// McpOAuthFlowPhase mirrors the host sign-in flow phases.
// McpOAuthFlowPhase 对应宿主登录流程阶段。
export type McpOAuthFlowPhase = "awaiting_callback" | "exchanging" | "succeeded" | "failed" | "expired"

// McpOAuthFlow is the public view of one sign-in flow; it never carries tokens or PKCE material.
// McpOAuthFlow 是单个登录流程的公开视图，从不携带令牌或 PKCE 材料。
export interface McpOAuthFlow {
  // Opaque identity echoed by finish and cancel.
  // 完成与取消时回传的不透明标识。
  flowId: string; phase: McpOAuthFlowPhase
  // Loopback flows finish by themselves; manual flows need the pasted callback URL.
  // 回环流程自动完成；手动流程需要粘贴回调 URL。
  callbackMode: "loopback" | "manual"
  // Authorization page and the redirect registered for this flow.
  // 授权页面与本流程登记的回调地址。
  authorizationUrl: string; redirectUri: string
  // Failure detail and wall-clock expiry in Unix milliseconds.
  // 失败详情与以 Unix 毫秒表示的墙钟过期时间。
  error: string | null; expiresAtUnixMs: number
}

// McpOAuthStatus is the OAuth view of one server for the settings page.
// McpOAuthStatus 是设置页使用的单个服务器 OAuth 视图。
export interface McpOAuthStatus {
  // authStatus is null when the host could not determine it; statusError says why.
  // 宿主无法确定时 authStatus 为 null，statusError 说明原因。
  serverId: string; authStatus: McpAuthStatus | null; statusError: string | null; flow: McpOAuthFlow | null
}

// requireServiceRuntime rejects OAuth operations in preview mode.
// requireServiceRuntime 在预览模式下拒绝 OAuth 操作。
function requireServiceRuntime() {
  if (!hasServiceRuntime()) throw new Error("MCP runtime is unavailable in preview mode.")
}

// loadMcpOAuthStatus reads the credential status and the latest sign-in flow of one server.
// loadMcpOAuthStatus 读取单个服务器的凭据状态与最近登录流程。
export async function loadMcpOAuthStatus(serverId: string): Promise<McpOAuthStatus> {
  requireServiceRuntime()
  return serviceClient.request<McpOAuthStatus>("service_mcp_oauth_status", { serverId })
}

// startMcpOAuth starts a sign-in flow, replacing any earlier one, and opens the authorization page.
// startMcpOAuth 启动登录流程（替换先前流程）并打开授权页面。
export async function startMcpOAuth(serverId: string): Promise<McpOAuthStatus> {
  requireServiceRuntime()
  return serviceClient.request<McpOAuthStatus>("service_start_mcp_oauth", { serverId })
}

// finishMcpOAuth completes a manual flow with the URL the browser was redirected to.
// finishMcpOAuth 使用浏览器被重定向到的 URL 完成手动流程。
export async function finishMcpOAuth(request: { serverId: string; flowId: string; callbackUrl: string }): Promise<McpOAuthStatus> {
  requireServiceRuntime()
  return serviceClient.request<McpOAuthStatus>("service_finish_mcp_oauth", { request })
}

// cancelMcpOAuth cancels the exact flow and releases its callback port.
// cancelMcpOAuth 取消精确流程并释放其回调端口。
export async function cancelMcpOAuth(request: { serverId: string; flowId: string }): Promise<McpOAuthStatus> {
  requireServiceRuntime()
  return serviceClient.request<McpOAuthStatus>("service_cancel_mcp_oauth", { request })
}

// removeMcpOAuth signs out by deleting the stored credentials of one server.
// removeMcpOAuth 删除单个服务器已存储的凭据以退出登录。
export async function removeMcpOAuth(serverId: string): Promise<McpOAuthStatus> {
  requireServiceRuntime()
  return serviceClient.request<McpOAuthStatus>("service_remove_mcp_oauth", { serverId })
}
