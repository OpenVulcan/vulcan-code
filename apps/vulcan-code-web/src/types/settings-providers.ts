import type { MainAreaId } from "./startup";
import type { ComposerAttachmentSummary } from "./composer-skills";
import type { PublicAiNativeToolKind, PublicAiResourceKind } from "./public-ai";

export interface FeatureAreaSummary {
  // Stable feature area identifier.
  // 稳定功能区域标识。
  id: MainAreaId;
  // Feature area display label.
  // 功能区域显示名称。
  label: string;
  // Feature area description.
  // 功能区域描述。
  description: string;
  // Feature area status.
  // 功能区域状态。
  status: string;
}

// SettingSectionSummary describes one settings group.
// SettingSectionSummary 描述单个设置分组。
export interface SettingSectionSummary {
  // Stable settings section identifier.
  // 稳定设置分组标识。
  id: string;
  // Settings section display label.
  // 设置分组显示名称。
  label: string;
  // Settings section description.
  // 设置分组描述。
  description: string;
  // Representative setting item names.
  // 代表性设置项名称。
  items: string[];
}

// HostToolsSettingsSummary describes shared host-owned tool settings.
// HostToolsSettingsSummary 描述共享宿主自有工具设置。
export interface HostToolsSettingsSummary {
  // Runtime-registered native tool rows available for agent policy editing.
  // 可用于智能体策略编辑的运行时已注册原生工具行。
  nativeTools: NativeToolSummary[];
}

// NativeToolSummary describes one runtime-registered native host tool.
// NativeToolSummary 描述单个运行时已注册原生宿主工具。
export interface NativeToolSummary {
  // Stable native tool identifier.
  // 稳定原生工具标识。
  id: string;
  // Human-readable native tool name.
  // 人类可读原生工具名称。
  name: string;
  // Human-readable native tool description.
  // 人类可读原生工具描述。
  description: string;
  // Whether this native tool is normally visible to AI agents.
  // 该原生工具通常是否对 AI 代理可见。
  aiVisible: boolean;
  // Whether this native tool may run while Plan mode is active.
  // 该原生工具是否允许在计划模式激活时运行。
  availableInPlan: boolean;
}

// ProviderSummary describes one provider card.
// ProviderSummary 描述单个 Provider 卡片。
export interface ProviderSummary {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  id: string;
  // Exact provider adapter identifier for credential-scoped rows.
  // 凭据作用域行的精确供应商适配器标识。
  sourceProviderId?: string;
  // Exact credential connection identifier.
  // 精确凭据连接标识。
  connectionId?: string;
  // User-visible account label.
  // 用户可见账号标签。
  accountName?: string;
  // Provider display name.
  // Provider 显示名称。
  name: string;
  // Provider display name without the credential account suffix.
  // 不包含凭据账号后缀的 Provider 显示名称。
  providerDisplayName?: string;
  // Provider status.
  // Provider 状态。
  status: string;
  // Whether this provider has a saved local configuration.
  // 该 Provider 是否已保存本地配置。
  configured?: boolean;
  // Enabled model count text.
  // 已启用模型数量文本。
  // Whether this provider owns the active workbench model.
  // 该 Provider 是否拥有当前激活的工作台模型。
  active?: boolean;
  // Enabled models exposed to the model picker.
  // 暴露给模型选择器的已启用模型。
  models?: ProviderModelSummary[];
  // Full catalog model list before enabled filtering.
  // 启用过滤前的完整目录模型列表。
  catalogModels?: ProviderModelSummary[];
  // Editable provider fields.
  // 可编辑 Provider 字段。
  fields?: ProviderFieldSummary[];
}

// ProviderModelSummary describes one enabled model under a provider.
// ProviderModelSummary 描述 Provider 下的单个已启用模型。
export interface ProviderModelSummary {
  // Stable model identifier persisted with provider id.
  // 与 Provider 标识一起持久化的稳定模型标识。
  id: string;
  // Model display name.
  // 模型显示名称。
  name: string;
  // Whether this model is currently active.
  // 该模型当前是否处于激活状态。
  active?: boolean;
  // Whether this credential connection is the backend-resolved preferred account for this model.
  // 该凭据连接是否为后端针对当前模型解析出的首选账号。
  preferredConnection?: boolean;
  // Whether this model is enabled.
  // 该模型是否启用。
  enabled?: boolean;
  // Optional model status.
  // 可选模型状态。
  status?: string | null;
  // Optional model family.
  // 可选模型家族。
  family?: string | null;
  // Optional context window limit.
  // 可选上下文窗口上限。
  contextLimit?: number | null;
  // Optional input token limit.
  // 可选输入 Token 上限。
  inputLimit?: number | null;
  // Optional output token limit.
  // 可选输出 Token 上限。
  outputLimit?: number | null;
  // Optional reasoning capability flag.
  // 可选推理能力标记。
  reasoning?: boolean | null;
  // Optional attachment capability flag.
  // 可选附件能力标记。
  attachment?: boolean | null;
  // Input modalities accepted by the model.
  // 模型接受的输入模态集合。
  inputModalities?: PublicAiResourceKind[];
  // Output modalities emitted by the model.
  // 模型输出的模态集合。
  outputModalities?: PublicAiResourceKind[];
  // Provider-native tools available on this exact credential and model route.
  // 此精确凭据与模型路由可用的供应商原生工具。
  providerNativeTools?: PublicAiNativeToolKind[];
  // Optional reasoning profile JSON text.
  // 可选推理档位配置 JSON 文本。
  reasoningProfileJson?: string | null;
  // Exact reasoning selector projected from this model's discovered capabilities.
  // 从该模型已发现能力投影出的精确思考选择器。
  reasoningSelection?: ReasoningSelectionSummary | null;
  // Whether this provider/model pair supports backend-managed secondary routes.
  // 该 Provider/模型组合是否支持后端管理的二级路由。
  supportsSecondaryRoutes?: boolean;
  // Active secondary route when one has been selected.
  // 已选择时的当前二级路由。
  secondaryRoute?: ProviderModelSecondaryRouteSummary | null;
}

// ProviderModelSecondaryRouteSummary describes one provider/model secondary route.
// ProviderModelSecondaryRouteSummary 描述单个 Provider/模型二级路由。
export interface ProviderModelSecondaryRouteSummary {
  // Stable owning provider identifier.
  // 稳定所属 Provider 标识。
  providerId: string;
  // Stable owning model identifier.
  // 稳定所属模型标识。
  modelId: string;
  // Stable route identifier.
  // 稳定路由标识。
  routeId: string;
  // Provider-specific tag alias.
  // Provider 专属 tag 别名。
  tag: string;
  // Upstream provider name.
  // 上游供应商名称。
  providerName: string;
  // Endpoint display name.
  // 端点显示名称。
  endpointName: string;
  // Compact display label.
  // 紧凑显示标签。
  label: string;
  // Secondary detail text.
  // 二级详情文本。
  subtitle: string;
  // Whether this route is active.
  // 该路由是否激活。
  active?: boolean;
}

// ProviderModelSecondaryRouteSelection selects one provider/model secondary route.
// ProviderModelSecondaryRouteSelection 选择单个 Provider/模型二级路由。
export interface ProviderModelSecondaryRouteSelection {
  // Stable route identifier.
  // 稳定路由标识。
  routeId: string;
  // Provider-specific tag alias.
  // Provider 专属 tag 别名。
  tag?: string;
  // Upstream provider name.
  // 上游供应商名称。
  providerName?: string;
  // Endpoint display name.
  // 端点显示名称。
  endpointName?: string;
}

// UserPromptEditDraftSummary carries one backend-validated edit draft.
// UserPromptEditDraftSummary 承载单个后端校验后的编辑草稿。
export interface UserPromptEditDraftSummary {
  // Stable conversation identifier.
  // 稳定会话标识。
  conversationId: string;
  // Stable user prompt message identifier.
  // 稳定用户提示词消息标识。
  messageId: string;
  // Editable prompt text.
  // 可编辑提示词文本。
  prompt: string;
  // Attachments that will be replayed with the edited prompt.
  // 将随编辑后提示词重新执行的附件。
  attachments?: ComposerAttachmentSummary[];
  // Whether the owning conversation is currently running.
  // 所属会话是否正在运行。
  running?: boolean;
}

// ProviderFieldSummary describes one editable provider field.
// ProviderFieldSummary 描述单个可编辑 Provider 字段。
export interface ProviderFieldSummary {
  // Stable field key.
  // 稳定字段键。
  key: string;
  // Field display label.
  // 字段显示名称。
  label: string;
  // Field placeholder text.
  // 字段占位提示。
  placeholder: string;
  // Stored field value.
  // 已保存字段值。
  value: string;
  // Whether this field should use password input.
  // 该字段是否使用密码输入。
  secret: boolean;
  // Whether this field is required.
  // 该字段是否必填。
  required: boolean;
}

// KeyValueSummary describes one string key/value pair.
// KeyValueSummary 描述单个字符串键值对。
export interface KeyValueSummary {
  // Stable key.
  // 稳定键。
  key: string;
  // Stored value.
  // 已保存值。
  value: string;
}

// McpServerSummary describes one persisted MCP server.
// McpServerSummary 描述单个持久化 MCP 服务器。
export interface McpServerSummary {
  // Stable server identifier.
  // 稳定服务器标识。
  id: string;
  // Server display name.
  // 服务器显示名称。
  name: string;
  // Whether this server is enabled.
  // 该服务器是否启用。
  enabled: boolean;
  // Transport key.
  // 传输键。
  transport: string;
  // Stdio command.
  // Stdio 启动命令。
  command: string;
  // Stdio arguments.
  // Stdio 参数。
  args: string[];
  // Remote endpoint URL.
  // 远端端点 URL。
  url: string;
  // Remote bearer token.
  // 远端 Bearer Token。
  bearerToken: string;
  // Explicit request headers.
  // 显式请求头。
  headers: KeyValueSummary[];
  // Request headers resolved from environment variables.
  // 从环境变量解析的请求头。
  headersFromEnv: KeyValueSummary[];
  // OAuth client id.
  // OAuth 客户端标识。
  oauthClientId: string;
  // OAuth client secret.
  // OAuth 客户端密钥。
  oauthClientSecret: string;
  // OAuth scopes.
  // OAuth scope 列表。
  oauthScopes: string[];
  // OAuth redirect URI.
  // OAuth 回调 URI。
  oauthRedirectUri: string;
  // Stdio working directory.
  // Stdio 工作目录。
  cwd: string;
  // Stdio environment variables.
  // Stdio 环境变量。
  env: KeyValueSummary[];
  // Stdio environment passthrough names.
  // Stdio 环境变量透传名称。
  envPassthrough: string[];
}

// McpToolSummary describes one discovered MCP runtime tool.
// McpToolSummary 描述单个已发现的 MCP 运行时工具。
export interface McpToolSummary {
  // Stable selector id accepted by runtime tool exposure policies.
  // 运行时工具暴露策略可接受的稳定选择器标识。
  id: string;
  // Owning MCP server id.
  // 所属 MCP 服务器标识。
  serverId: string;
  // Remote tool name.
  // 远端工具名称。
  name: string;
  // Remote tool description.
  // 远端工具描述。
  description: string;
  // Original qualified MCP tool id.
  // 原始限定 MCP 工具标识。
  originToolId: string;
}

// ReasoningSelectionOptionSummary describes one backend-resolved reasoning selector option.
// ReasoningSelectionOptionSummary 描述单个后端解析出的思考选择器选项。
export interface ReasoningSelectionOptionSummary {
  // Stable reasoning effort value sent through the service router.
  // 通过服务路由发送的稳定思考档位值。
  value: string;
  // Provider-resolved user-facing option label.
  // Provider 解析出的面向用户选项标签。
  label: string;
}

// ReasoningSelectionSummary describes the active backend reasoning selector view.
// ReasoningSelectionSummary 描述当前激活的后端思考选择器视图。
export interface ReasoningSelectionSummary {
  // Dynamic options supported by the active provider and model.
  // 当前激活 Provider 与模型支持的动态选项。
  options: ReasoningSelectionOptionSummary[];
  // Effective selected reasoning effort value.
  // 当前生效的已选思考档位值。
  selectedEffort: string;
  // Optional backend reconciliation note.
  // 可选后端档位协调说明。
  note?: string | null;
}
