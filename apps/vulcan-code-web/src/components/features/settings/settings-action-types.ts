// ProviderConfigurationSaveRequest describes one provider configuration submission.
// ProviderConfigurationSaveRequest 描述单次 Provider 配置提交。
export interface ProviderConfigurationSaveRequest {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  providerId: string
  // Field values keyed by provider field key.
  // 按 Provider 字段键保存的字段值。
  fields: Record<string, string>
}

// ProviderEnabledModelsSaveRequest describes one provider model enablement submission.
// ProviderEnabledModelsSaveRequest 描述单次 Provider 模型启用提交。
export interface ProviderEnabledModelsSaveRequest {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  providerId: string
  // Enabled model identifiers.
  // 启用模型标识集合。
  enabledModelIds: string[]
}

// ProviderModelOverrideSaveRequest describes one model capability override submission.
// ProviderModelOverrideSaveRequest 描述单个模型能力覆盖提交。
export interface ProviderModelOverrideSaveRequest {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  providerId: string
  // Stable model identifier.
  // 稳定模型标识。
  modelId: string
  // Optional reasoning capability override.
  // 可选推理能力覆盖值。
  reasoning: boolean | null
  // Optional attachment marker override.
  // 可选附件标记覆盖值。
  attachment: boolean | null
  // Optional context-window limit override.
  // 可选上下文窗口上限覆盖值。
  contextLimit: number | null
  // Optional input-token limit override.
  // 可选输入 Token 上限覆盖值。
  inputLimit: number | null
  // Optional output-token limit override.
  // 可选输出 Token 上限覆盖值。
  outputLimit: number | null
  // Optional input-modality override list.
  // 可选输入模态覆盖列表。
  inputModalities: string[] | null
  // Optional output-modality override list.
  // 可选输出模态覆盖列表。
  outputModalities: string[] | null
  // Optional reasoning profile JSON override.
  // 可选推理档位 JSON 覆盖值。
  reasoningProfileJson: string | null
}

// AiCapabilityVendorSaveRequest describes one shared AI capability vendor submission.
// AiCapabilityVendorSaveRequest 描述单次共享 AI 能力供应商提交。
export interface AiCapabilityVendorSaveRequest {
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string
  // Whether the vendor is enabled.
  // 供应商是否启用。
  enabled: boolean
  // Shared API key or environment placeholder.
  // 共享 API Key 或环境变量占位符。
  apiKey: string
  // Shared base URL override.
  // 共享 Base URL 覆盖值。
  baseUrl: string
  // Shared region override.
  // 共享区域覆盖值。
  region: string
  // Optional connection mode.
  // 可选连接模式。
  connectionMode?: string | null
}

// AiCapabilityTargetEnabledRequest describes one target enabled-state submission.
// AiCapabilityTargetEnabledRequest 描述单次目标启用状态提交。
export interface AiCapabilityTargetEnabledRequest {
  // Stable capability kind.
  // 稳定能力类型。
  capabilityKind: string
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string
  // Stable target alias.
  // 稳定目标别名。
  targetAlias: string
  // Next enabled state.
  // 下一启用状态。
  enabled: boolean
}

// AiCapabilityTargetSaveInput describes one saved capability target binding.
// AiCapabilityTargetSaveInput 描述单个待保存能力目标绑定。
export interface AiCapabilityTargetSaveInput {
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string
  // Stable target alias under the vendor.
  // 供应商下的稳定目标别名。
  targetAlias: string
  // Provider request model id.
  // Provider 请求模型标识。
  modelId: string
  // Whether the binding is enabled.
  // 该绑定是否启用。
  enabled: boolean
}

// AiCapabilityTargetsSaveRequest describes one ordered capability target-list submission.
// AiCapabilityTargetsSaveRequest 描述单次有序能力目标列表提交。
export interface AiCapabilityTargetsSaveRequest {
  // Stable semantic capability kind.
  // 稳定语义能力类型。
  capabilityKind: string
  // Ordered target bindings to keep for this capability.
  // 当前能力需要保留的有序目标绑定集合。
  targets: AiCapabilityTargetSaveInput[]
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
  // Saves one provider configuration.
  // 保存单个 Provider 配置。
  onSaveProvider: (request: ProviderConfigurationSaveRequest) => Promise<void>
  // Removes one provider configuration.
  // 移除单个 Provider 配置。
  onRemoveProvider: (providerId: string) => Promise<void>
  // Saves enabled model identifiers for one provider.
  // 保存单个 Provider 的启用模型标识集合。
  onSaveProviderEnabledModels: (request: ProviderEnabledModelsSaveRequest) => Promise<void>
  // Saves one provider model capability override.
  // 保存单个 Provider 模型能力覆盖。
  onSaveProviderModelOverride: (request: ProviderModelOverrideSaveRequest) => Promise<void>
  // Saves one shared AI capability vendor.
  // 保存单个共享 AI 能力供应商。
  onSaveAiCapabilityVendor: (request: AiCapabilityVendorSaveRequest) => Promise<void>
  // Removes one shared AI capability vendor.
  // 移除单个共享 AI 能力供应商。
  onRemoveAiCapabilityVendor: (vendorId: string) => Promise<void>
  // Toggles one AI capability target.
  // 切换单个 AI 能力目标。
  onSetAiCapabilityTargetEnabled: (request: AiCapabilityTargetEnabledRequest) => Promise<void>
  // Saves one ordered AI capability target list.
  // 保存单个有序 AI 能力目标列表。
  onSaveAiCapabilityTargets: (request: AiCapabilityTargetsSaveRequest) => Promise<void>
  // Saves one MCP server configuration.
  // 保存单个 MCP 服务器配置。
  onSaveMcpServer: (request: McpServerSaveRequest) => Promise<void>
  // Removes one MCP server configuration.
  // 移除单个 MCP 服务器配置。
  onRemoveMcpServer: (serverId: string) => Promise<void>
}
