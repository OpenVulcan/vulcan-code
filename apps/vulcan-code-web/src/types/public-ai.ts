// CredentialMode identifies one supported credential lifecycle.
// CredentialMode 标识一种受支持的凭据生命周期。
export type CredentialMode = "auth" | "token";

// OAuthProviderKind identifies one exact renewable provider flow.
// OAuthProviderKind 标识一个精确的可续期供应商流程。
export type OAuthProviderKind =
  | "open_ai_codex"
  | "claude_code"
  | "google_code_assist"
  | "google_gemini"
  | "mini_max_global_token_plan"
  | "mini_max_china_token_plan"
  | "kimi_coding_plan";

// CredentialBillingKind identifies the exact billing semantics of one credential profile.
// CredentialBillingKind 标识一个凭据配置档的精确计费语义。
export type CredentialBillingKind = "pay_as_you_go" | "token_plan" | "provider_managed";

// PublicAiCredentialProfile declares one exact credential acquisition and billing combination.
// PublicAiCredentialProfile 声明一种精确的凭据获取与计费组合。
export interface PublicAiCredentialProfile {
  id: string;
  display_name: string;
  credential_mode: CredentialMode;
  oauth_provider?: OAuthProviderKind;
  billing_kind: CredentialBillingKind;
  account_usage_query: "unsupported" | "mini_max_token_plan" | "open_router_credits" | "tavily_credits";
  authentication?: string;
  key_prefix?: string;
}

// PublicAiProtocolRoute describes one verified wire route.
// PublicAiProtocolRoute 描述一条已验证 Wire 路由。
export interface PublicAiProtocolRoute {
  protocol_profile_id: string;
  endpoint_path: string;
  verified: boolean;
}

// PublicAiServiceEntry describes one selectable provider entry.
// PublicAiServiceEntry 描述一个可选供应商入口。
export interface PublicAiServiceEntry {
  id: string;
  display_name: string;
  kind: "conversation" | "provider_tools";
  base_url: string;
  credential_profiles: PublicAiCredentialProfile[];
  authentication: string;
  protocol_routes: PublicAiProtocolRoute[];
}

// PublicAiProviderDescriptor describes one registered provider adapter.
// PublicAiProviderDescriptor 描述一个已注册供应商适配器。
export interface PublicAiProviderDescriptor {
  id: string;
  display_name: string;
  kind: "system" | "custom";
  service_entries: PublicAiServiceEntry[];
}

// PublicAiCustomProviderProtocol identifies one user-selected compatible wire protocol.
// PublicAiCustomProviderProtocol 标识用户选择的一种兼容 Wire 协议。
export type PublicAiCustomProviderProtocol =
  | "anthropic_messages"
  | "open_ai_chat"
  | "open_ai_responses";

// PublicAiCustomProviderSaveRequest creates or replaces one user-defined provider.
// PublicAiCustomProviderSaveRequest 创建或替换一个用户自定义供应商。
export interface PublicAiCustomProviderSaveRequest {
  providerId?: string;
  displayName: string;
  baseUrl: string;
  protocol: PublicAiCustomProviderProtocol;
  authentication: "bearer_authorization" | "x_api_key";
  credentialDisplayName: string;
  secret: string;
}

// PublicAiConnection carries redacted credential connection metadata.
// PublicAiConnection 承载脱敏凭据连接元数据。
export interface PublicAiConnection {
  id: string;
  provider_id: string;
  service_entry_id: string;
  credential_profile_id: string;
  billing_kind: CredentialBillingKind;
  usage_capability: "unsupported" | "mini_max_token_plan" | "open_router_credits" | "tavily_credits";
  has_management_key: boolean;
  credential_mode: CredentialMode;
  credential_id: string;
  display_name: string;
  verified_protocols: string[];
  selected_protocol: string;
  prompt_cache_by_protocol: Record<string, unknown>;
}

// PublicAiModel describes one runtime-discovered model.
// PublicAiModel 描述一个运行时发现模型。
export interface PublicAiModel {
  id: string;
  display_name: string;
  capabilities: {
    context_limit?: number;
    output_limit?: number;
    reasoning: string;
    tool_calling: string;
    prompt_cache: PublicAiPromptCacheCapability;
    model_parameters: PublicAiModelParameterDefinition[];
    [key: string]: unknown;
  };
}

// PublicAiPromptCacheCapability describes route-specific cache controls.
// PublicAiPromptCacheCapability 描述路由专属缓存控制。
export interface PublicAiPromptCacheCapability {
  provider_default_behavior: string;
  supports_managed_explicit: boolean;
  supported_ttl_options: Array<{
    id: string;
    display_name: string;
    description?: string;
  }>;
  profile_default_ttl_option_id?: string;
  minimum_cacheable_tokens?: number;
  maximum_breakpoints?: number;
}

// PublicAiModelParameterDefinition describes one adapter-registered field.
// PublicAiModelParameterDefinition 描述一个适配器注册字段。
export interface PublicAiModelParameterDefinition {
  id: string;
  display_name: string;
  description: string;
  schema: { type: string; [key: string]: unknown };
  default: { type: string; [key: string]: unknown };
  protocol_ids: string[];
  model_ids: string[];
}

// PublicAiConnectionCatalog carries one memory-only model catalog.
// PublicAiConnectionCatalog 承载一个仅内存模型目录。
export interface PublicAiConnectionCatalog {
  connection_id: string;
  models: PublicAiModel[];
  generation: number;
}

// PublicAiActiveSelection identifies one exact model and account.
// PublicAiActiveSelection 标识一个精确模型与账号。
export interface PublicAiActiveSelection {
  provider_id: string;
  model_id: string;
  connection_id: string;
}

// PublicAiModelOverrides stores local narrowing and registered parameter values.
// PublicAiModelOverrides 保存本地收窄与注册参数值。
export interface PublicAiModelOverrides {
  context_limit?: number;
  output_limit?: number;
  prompt_cache?: unknown;
  model_parameters?: Record<string, unknown>;
}

// PublicAiModelOverrideRecord identifies one override without composite-key parsing.
// PublicAiModelOverrideRecord 无需解析组合键即可标识一个覆盖。
export interface PublicAiModelOverrideRecord {
  providerId: string;
  modelId: string;
  protocolId: string;
  overrides: PublicAiModelOverrides;
}

// PublicAiModelAccessRecord carries one provider-model effective and default access state.
// PublicAiModelAccessRecord 承载一个供应商模型的有效及默认访问状态。
export interface PublicAiModelAccessRecord {
  providerId: string;
  modelId: string;
  enabled: boolean;
  defaultEnabled: boolean;
}

// PublicAiProviderToolCapabilityKind identifies one single-select semantic provider tool.
// PublicAiProviderToolCapabilityKind 标识一种单选语义供应商工具。
export type PublicAiProviderToolCapabilityKind =
  | "web_search"
  | "web_extract"
  | "media_analyze"
  | "image_generate"
  | "image_edit"
  | "video_generate"
  | "speech_synthesize"
  | "speech_transcribe"
  | "music_generate"
  | "music_cover"
  | "embedding_create"
  | "rerank_documents";

// PublicAiProviderToolContract carries one selected-model-native schema and unified output contract.
// PublicAiProviderToolContract 承载一个选定模型原生 Schema 与统一输出契约。
export interface PublicAiProviderToolContract {
  tool_id: string;
  description: string;
  capability: PublicAiProviderToolCapabilityKind;
  input_schema: Record<string, unknown>;
  output: {
    kind: "text" | "resources" | "embedding" | "rerank" | "search" | "extract" | "composite";
    source_path: string;
    normalized_result_path: string;
    resource_kind?: string;
    mime_types?: string[];
  };
}

// PublicAiProviderToolTarget describes one exact model or service target.
// PublicAiProviderToolTarget 描述一个精确模型或服务目标。
export interface PublicAiProviderToolTarget {
  target_id: string;
  provider_id: string;
  service_entry_id: string;
  credential_profile_ids: string[];
  upstream_target_id: string;
  display_name: string;
  contract: PublicAiProviderToolContract;
}

// PublicAiProviderToolTargetSnapshot pairs one target with one configured credential.
// PublicAiProviderToolTargetSnapshot 将一个目标与一个已配置凭据配对。
export interface PublicAiProviderToolTargetSnapshot {
  connectionId: string;
  connectionDisplayName: string;
  target: PublicAiProviderToolTarget;
  enabled: boolean;
}

// PublicAiProviderToolTargetSelection stores the one selected target for one capability.
// PublicAiProviderToolTargetSelection 保存某项能力唯一选中的目标。
export interface PublicAiProviderToolTargetSelection {
  connection_id: string;
  target_id: string;
}

// PublicAiSettingsSnapshot combines persisted and process-local settings state.
// PublicAiSettingsSnapshot 组合持久化设置状态与进程内设置状态。
export interface PublicAiSettingsSnapshot {
  usageRefreshIntervalSeconds: number;
  usageCache: Record<string, PublicAiAccountUsage>;
  providers: PublicAiProviderDescriptor[];
  connections: PublicAiConnection[];
  providerDefaults: Record<string, string>;
  active?: PublicAiActiveSelection;
  catalogs: Record<string, PublicAiConnectionCatalog>;
  catalogErrors: Record<string, string>;
  modelOverrides: Record<string, PublicAiModelOverrides>;
  modelOverrideRecords: PublicAiModelOverrideRecord[];
  modelAccessRecords: PublicAiModelAccessRecord[];
  providerToolTargets: PublicAiProviderToolTargetSnapshot[];
  providerToolSelections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >;
}

// PublicAiAccountUsage carries one credential-scoped observation.
// PublicAiAccountUsage 承载一个凭据作用域观测。
export interface PublicAiAccountUsage {
  credential_id: string;
  availability: "unsupported" | "supported" | "temporarily_unavailable";
  plan_name?: string;
  used_amount?: string;
  remaining_amount?: string;
  currency?: string;
  balance_amount?: string;
  quota_windows: PublicAiAccountUsageWindow[];
  observed_at?: string;
  refresh_message?: string;
}

// PublicAiAccountUsageWindow carries one exact provider quota window.
// PublicAiAccountUsageWindow 承载一个精确供应商额度窗口。
export interface PublicAiAccountUsageWindow {
  id: string;
  display_name: string;
  bucket_name?: string;
  used?: string;
  remaining?: string;
  total?: string;
  remaining_percent?: string;
  unit: string;
  starts_at?: string;
  resets_at?: string;
  unlimited: boolean;
  available: boolean;
}

// PublicAiConnectionAddRequest describes one new credential submission.
// PublicAiConnectionAddRequest 描述一次新凭据提交。
export interface PublicAiConnectionAddRequest {
  providerId: string;
  serviceEntryId: string;
  credentialProfileId: string;
  credentialMode: CredentialMode;
  displayName: string;
  selectedProtocol: string;
  secret: string;
  managementKey?: string;
}

// PublicAiConnectionUpdateRequest describes one credential edit.
// PublicAiConnectionUpdateRequest 描述一次凭据编辑。
export interface PublicAiConnectionUpdateRequest {
  connectionId: string;
  displayName: string;
  serviceEntryId: string;
  credentialProfileId: string;
  credentialMode: CredentialMode;
  selectedProtocol: string;
  secret?: string;
  managementKey?: string;
  removeManagementKey?: boolean;
}

// PublicAiOAuthClientConfiguration carries user-owned client settings for declared flows.
// PublicAiOAuthClientConfiguration 承载声明流程使用的用户自有客户端设置。
export interface PublicAiOAuthClientConfiguration {
  // User-owned OAuth client identifier.
  // 用户自有 OAuth 客户端标识。
  clientId?: string;
  // User-owned OAuth client secret.
  // 用户自有 OAuth 客户端密钥。
  clientSecret?: string;
  // Google Cloud project used by public Gemini OAuth billing and quota.
  // 公共 Gemini OAuth 计费与配额使用的 Google Cloud 项目。
  projectId?: string;
}

// PublicAiOAuthStartRequest starts one provider-declared authorization flow.
// PublicAiOAuthStartRequest 启动一个供应商声明的授权流程。
export interface PublicAiOAuthStartRequest {
  providerId: string;
  serviceEntryId: string;
  credentialProfileId: string;
  client: PublicAiOAuthClientConfiguration;
}

// PublicAiOAuthTransaction describes one browser-safe in-memory authorization state.
// PublicAiOAuthTransaction 描述一个浏览器安全的内存授权状态。
export interface PublicAiOAuthTransaction {
  transactionId: string;
  providerKind: OAuthProviderKind;
  phase:
    | "waiting_for_user_authorization"
    | "waiting_for_callback"
    | "exchanging_token"
    | "succeeded"
    | "failed"
    | "cancelled";
  expiresAtUnixMs: number;
  message?: string;
}

// PublicAiOAuthChallenge carries the external authorization URL and transaction state.
// PublicAiOAuthChallenge 承载外部授权 URL 与事务状态。
export interface PublicAiOAuthChallenge {
  transaction: PublicAiOAuthTransaction;
  authorizationUrl: string;
  flowKind: "loopback_callback" | "device_code";
  userCode?: string;
  pollIntervalMs?: number;
}

// PublicAiOAuthCommitRequest commits a successful authorization as one connection.
// PublicAiOAuthCommitRequest 将成功授权提交为一个连接。
export interface PublicAiOAuthCommitRequest {
  transactionId: string;
  providerId: string;
  serviceEntryId: string;
  credentialProfileId: string;
  displayName: string;
  selectedProtocol: string;
  // Existing connection replaced by this authorization.
  // 由此次授权替换的现有连接。
  connectionId?: string;
}
