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
  sessions: McpSessionGrantEntry;
  mediaTargets: McpMediaTargetEntry[];
  // Usage since the service started; not persisted.
  // 本次服务启动以来的用量；不持久化。
  lastUsedAtMs?: number;
  callCount: number;
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
  limits: McpServiceLimits;
}

// McpServiceClientSaveRequest creates (no clientId) or updates one client.
// McpServiceClientSaveRequest 创建（无 clientId）或更新一个客户端。
export interface McpServiceClientSaveRequest {
  clientId?: string;
  name: string;
  enabled: boolean;
  sessions: McpSessionGrantEntry;
  mediaTargets: McpMediaTargetEntry[];
}

// McpServiceIssuedToken carries a token that is shown only once.
// McpServiceIssuedToken 携带只展示一次的令牌。
export interface McpServiceIssuedToken {
  snapshot: McpServiceSnapshot;
  clientId: string;
  token: string;
}
