import type { AnswerMode } from "@/service/answer-mode"

// GitCommitSettingsSaveRequest describes one Git commit settings submission.
// GitCommitSettingsSaveRequest 描述一次 Git 提交设置提交。
export interface GitCommitSettingsSaveRequest {
  // Default namespace prefix used when the current branch has no namespace.
  // 当前分支没有命名空间时使用的默认命名空间前缀。
  newBranchPrefix: string
  // Model selection used when commit messages are generated automatically.
  // 自动生成提交信息时使用的模型选择。
  summaryModel: GitCommitSummaryModelSelectionSaveRequest
  // User-authored extra prompt appended to commit-message generation.
  // 追加到提交信息生成中的用户额外提示词。
  summaryExtraPrompt: string
}

// ProviderRetrySettingsSaveRequest describes one complete Provider retry settings submission.
// ProviderRetrySettingsSaveRequest 描述一次完整 Provider 重试设置提交。
export interface ProviderRetrySettingsSaveRequest {
  // Whether automatic retry is enabled.
  // 是否启用自动重试。
  enabled: boolean
  // Maximum ordinary transient retry count.
  // 普通瞬态错误的最大重试次数。
  maxRetries: number
  // Initial ordinary retry delay in milliseconds.
  // 普通重试的初始等待毫秒数。
  initialDelayMs: number
  // Maximum ordinary retry delay in milliseconds.
  // 普通重试的最大等待毫秒数。
  maxDelayMs: number
  // Ordinary backoff multiplier represented in thousandths.
  // 以千分数表示的普通退避倍数。
  backoffFactorMilli: number
  // Deterministic jitter percentage.
  // 确定性抖动百分比。
  jitterPercent: number
  // Whether Retry-After hints may increase waits.
  // 是否允许 Retry-After 提示增加等待时间。
  respectRetryAfter: boolean
  // Whether HTTP 429 and structured rate-limit errors may be retried.
  // 是否允许重试 HTTP 429 与结构化限流错误。
  retry429: boolean
  // Complete retry keyword list; an empty list disables keyword retry.
  // 完整的重试关键词列表；空列表表示关闭关键词重试。
  retryKeywords: string[]
  // Dedicated rate-limit scheduling values.
  // 专用限流调度值。
  rateLimit: {
    // Maximum finite rate-limit retries.
    // 最大有限限流重试次数。
    maxRetries: number
    // Delay before the first rate-limit retry in milliseconds.
    // 第一次限流重试前的等待毫秒数。
    initialDelayMs: number
    // Maximum rate-limit delay in milliseconds.
    // 最大限流等待毫秒数。
    maxDelayMs: number
    // Rate-limit backoff multiplier represented in thousandths.
    // 以千分数表示的限流退避倍数。
    backoffFactorMilli: number
  }
}

// GitCommitSummaryModelSelectionSaveRequest describes the saved commit-summary model mode.
// GitCommitSummaryModelSelectionSaveRequest 描述已保存的提交总结模型模式。
export type GitCommitSummaryModelSelectionSaveRequest =
  | {
    // Selection mode that follows the owning conversation session.
    // 跟随所属会话的选择模式。
    mode: "session"
  }
  | {
    // Selection mode that uses a fixed provider/model pair.
    // 使用固定 Provider/模型组合的选择模式。
    mode: "fixed"
    // Stable provider identifier.
    // 稳定 Provider 标识。
    providerId: string
    // Stable model identifier.
    // 稳定模型标识。
    modelId: string
  }

// McpServerSaveRequest describes one MCP server form submission.
// McpServerSaveRequest 描述单次 MCP 服务器表单提交。
export interface McpServerSaveRequest {
  // Existing server id, or null when creating one.
  // 已有服务器标识，创建时为 null。
  serverId?: string | null
  // Server display name.
  // 服务器显示名称。
  name: string
  // Whether the server is enabled.
  // 服务器是否启用。
  enabled: boolean
  // Transport key.
  // 传输键。
  transport: string
  // Stdio command.
  // Stdio 命令。
  command: string
  // Stdio arguments.
  // Stdio 参数。
  args: string[]
  // Remote endpoint URL.
  // 远端端点 URL。
  url: string
  // Remote bearer token.
  // 远端 Bearer Token。
  bearerToken: string
  // Explicit request headers.
  // 显式请求头。
  headers: Array<{ key: string; value: string }>
  // Headers resolved from environment variables.
  // 从环境变量解析的请求头。
  headersFromEnv: Array<{ key: string; value: string }>
  // Whether the server signs in through OAuth, including dynamic client registration.
  // 服务器是否通过 OAuth 登录（包括动态客户端注册）。
  oauthEnabled: boolean
  // OAuth client id.
  // OAuth 客户端标识。
  oauthClientId: string
  // OAuth client secret.
  // OAuth 客户端密钥。
  oauthClientSecret: string
  // OAuth scopes.
  // OAuth scope 列表。
  oauthScopes: string[]
  // OAuth redirect URI.
  // OAuth 回调 URI。
  oauthRedirectUri: string
  // Stdio working directory.
  // Stdio 工作目录。
  cwd: string
  // Stdio environment variables.
  // Stdio 环境变量。
  env: Array<{ key: string; value: string }>
  // Stdio environment passthrough names.
  // Stdio 环境变量透传名称。
  envPassthrough: string[]
}

// SettingsActionProps groups service-backed actions used by the settings page.
// SettingsActionProps 分组设置页面使用的服务端动作。
export interface SettingsActionProps {
  // Saves the global default output style for new sessions and compaction boundaries.
  // 保存供新会话与压缩边界使用的全局默认输出风格。
  onSavePromptOutputStyle: (styleId: string) => Promise<void>
  // Saves the global answer mode copied into new conversations.
  // 保存复制到新会话的全局回答模式。
  onSavePromptAnswerMode: (mode: AnswerMode) => Promise<void>
  // Saves whether new sessions get a model-generated title.
  // 保存新会话是否获得模型生成的标题。
  onSaveSessionTitleGeneration: (enabled: boolean) => Promise<void>
  // Saves Git commit dialog settings.
  // 保存 Git 提交弹窗设置。
  onSaveGitCommitSettings: (request: GitCommitSettingsSaveRequest) => Promise<void>
  // Saves the default shell of the integrated terminal and the command tools; null picks automatically.
  // 保存集成终端与命令工具的默认 Shell；null 表示自动选择。
  onSaveDefaultCommandShell: (shellId: string | null) => Promise<void>
  // Saves Provider retry settings.
  // 保存 Provider 重试设置。
  onSaveProviderRetrySettings: (request: ProviderRetrySettingsSaveRequest) => Promise<void>
  // Saves the bounded Provider wire-request trace diagnostic switch.
  // 保存有界 Provider 线上请求追踪诊断开关。
  onSaveProviderRequestTraceSettings: (enabled: boolean, errorsEnabled?: boolean) => Promise<void>
  // Opens the exact selected session Provider API trace directory.
  // 打开当前选中会话的精确 Provider API 追踪目录。
  onOpenSelectedProviderApiTraceDirectory: () => Promise<void>
  // Clears only the exact selected session Provider API trace directory.
  // 仅清除当前选中会话的精确 Provider API 追踪目录。
  onClearSelectedProviderApiTraceDirectory: () => Promise<boolean>
  // Restores the selected archived conversations in one authoritative batch.
  // 一次权威批量恢复选中的归档会话。
  onRestoreArchivedConversations: (conversationIds: string[]) => Promise<void>
  // Permanently deletes the selected archived conversations in one authoritative batch.
  // 一次权威批量彻底删除选中的归档会话。
  onDeleteArchivedConversations: (conversationIds: string[]) => Promise<void>
}
