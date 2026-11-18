// McpProjectScopeEntry limits which projects' sessions one MCP client can reach.
// McpProjectScopeEntry 限定一个 MCP 客户端可以触达哪些项目的会话。
export type McpProjectScopeEntry =
  | { mode: "all" }
  | { mode: "selected"; projectIds: string[] };

// McpSessionGrantEntry is what one MCP client may do with sessions; every write grant needs read.
// McpSessionGrantEntry 是一个 MCP 客户端可以对会话执行的操作；每项写权限都需要读取权限。
export interface McpSessionGrantEntry {
  read: boolean;
  send: boolean;
  create: boolean;
  stop: boolean;
  answerQuestions: boolean;
  projects: McpProjectScopeEntry;
}

// McpMediaTargetEntry grants one image or video model through one account.
// McpMediaTargetEntry 通过一个账号授权一个图片或视频模型。
export interface McpMediaTargetEntry {
  connectionId: string;
  targetId: string;
}

// McpMediaTargetCandidate is one grantable media model, one row per account.
// McpMediaTargetCandidate 是一个可授权的媒体模型，每个账号一行。
export interface McpMediaTargetCandidate {
  connectionId: string;
  targetId: string;
  // Capability wire name such as image_generate or video_generate.
  // 能力线上名称，如 image_generate 或 video_generate。
  capability: string;
  // Public model id the MCP client uses.
  // MCP 客户端使用的公开模型标识。
  model: string;
  displayName: string;
  accountName: string;
}

// McpServiceClientSummary is one external AI client; its token is never returned after creation.
// McpServiceClientSummary 是一个外部 AI 客户端；令牌在创建后不会再返回。
export interface McpServiceClientSummary {
  id: string;
  name: string;
  // Last characters of the token, for recognition.
  // 令牌末尾几个字符，用于辨认。
  tokenHint: string;
  createdAtMs: number;
  enabled: boolean;
  // Opt-in capture of tool arguments and results.
  // 显式启用后记录工具参数和结果。
  recordCalls?: boolean;
  sessions: McpSessionGrantEntry;
  mediaTargets: McpMediaTargetEntry[];
  // Usage since the service started; not persisted.
  // 本次服务启动以来的用量；不持久化。
  lastUsedAtMs?: number;
  callCount: number;
  // Individual tools and their independent scopes.
  // 独立授权的工具及其各自的范围。
  toolIds?: string[];
  toolProjectIds?: string[];
  knowledgeLibraries?: Record<string, boolean>;
}

// McpServiceLimits are the bounds the service enforces.
// McpServiceLimits 是服务强制执行的边界。
export interface McpServiceLimits {
  maxClients: number;
  maxNameChars: number;
  maxProjects: number;
  maxMediaTargets: number;
}

// McpServiceSnapshot is the whole MCP settings page state.
// McpServiceSnapshot 是 MCP 设置页的完整状态。
export interface McpServiceSnapshot {
  enabled: boolean;
  // Absent while the local HTTP service is not listening yet.
  // 本地 HTTP 服务尚未监听时缺失。
  endpointUrl?: string;
  clients: McpServiceClientSummary[];
  mediaCandidates: McpMediaTargetCandidate[];
  // Grantable names and selectable knowledge library labels.
  // 可授权的名称和可选知识库标签。
  toolCandidates?: string[];
  knowledgeCandidates?: Record<string, string>;
  limits: McpServiceLimits;
}

// McpServiceClientSaveRequest creates (no clientId) or updates one client.
// McpServiceClientSaveRequest 创建（无 clientId）或更新一个客户端。
export interface McpServiceClientSaveRequest {
  clientId?: string;
  name: string;
  enabled: boolean;
  // Omitting this preference preserves the existing setting.
  // 省略此偏好时保留已有设置。
  recordCalls?: boolean;
  sessions: McpSessionGrantEntry;
  mediaTargets: McpMediaTargetEntry[];
  // An explicit empty selection revokes the corresponding grants.
  // 显式空选择撤销对应授权。
  toolIds?: string[];
  toolProjectIds?: string[];
  knowledgeLibraries?: Record<string, boolean>;
}

// McpServiceIssuedToken carries a token that is shown only once.
// McpServiceIssuedToken 携带只展示一次的令牌。
export interface McpServiceIssuedToken {
  snapshot: McpServiceSnapshot;
  clientId: string;
  token: string;
}

// Query filters scoped to the MCP audit database.
// MCP 审计数据库查询筛选条件。
export interface McpCallLogQuery {
  // Stable client ID to filter by, when selected.
  // 选择客户端时用于筛选的稳定标识。
  clientId?: string;
  // Public MCP project and session identifiers used by the external client.
  // 外部客户端使用的公开 MCP 项目和会话标识。
  projectId?: string;
  sid?: string;
  // Orders older records first when true.
  // 为真时按时间从早到晚排列记录。
  ascending: boolean;
  // Continues from the last call on the previous page.
  // 从上一页的最后一条调用继续读取。
  cursor?: number;
}

// Metadata returned for one tool call without the request or response payload.
// 一次工具调用的元数据，不包含请求及响应载荷。
export interface McpCallLogRow {
  // Stored call identity, also used for fetching detail and seeking pages.
  // 已保存的调用标识，也用于加载详情与定位分页。
  id: number;
  // The display name is captured at dispatch and survives changes to the client.
  // 显示名称在调度时保存，客户端变化后仍保留原值。
  clientId: string;
  clientName: string;
  // Short context identifiers are present only when the caller owns the context.
  // 仅在上下文归调用者所有时提供短标识。
  projectId: string | null;
  sid: string | null;
  // Tool name and timing projected for the call list.
  // 供调用列表展示的工具名称与时间。
  tool: string;
  startedAt: number;
  finishedAt: number | null;
  // Lifecycle of the recorded call.
  // 已记录调用的生命周期状态。
  status: "running" | "completed" | "failed" | "interrupted";
}

// Page of call metadata with a seek cursor when additional calls exist.
// 调用元数据分页，有更多调用时携带定位游标。
export interface McpCallLogPage {
  // Rows in the order the user requested.
  // 按用户指定顺序排列的记录。
  rows: McpCallLogRow[];
  // Null means the query has reached its last page.
  // 为 null 表示查询已经到达最后一页。
  nextCursor: number | null;
}

// Request parameters and final projected response of the selected call.
// 所选调用的请求参数和最终投影响应。
export interface McpCallDetail {
  // Arguments and returned content are opaque JSON values.
  // 参数与返回内容是结构不固定的 JSON 值。
  arguments: unknown;
  result: unknown;
}
