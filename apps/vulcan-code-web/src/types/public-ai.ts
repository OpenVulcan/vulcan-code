import type { BrowserServiceLocalizedMessage } from "@/service/browserTransport";
import type { PublicAiMediaDeliverySettingsSnapshot } from "./media-delivery";
export type * from "./media-delivery";

// CredentialMode identifies one supported credential lifecycle.
// CredentialMode 标识一种受支持的凭据生命周期。
export type CredentialMode = "auth" | "token";

// OAuthProviderKind identifies one registered account authorization protocol.
// OAuthProviderKind 标识一个已注册账户授权协议。
export type OAuthProviderKind =
  | "open_vulcan_china"
  | "open_vulcan_global"
  | "open_ai_codex"
  | "claude_code"
  | "google_code_assist"
  | "google_gemini"
  | "mini_max_global_token_plan"
  | "mini_max_china_token_plan"
  | "kimi_coding_plan"
  | "open_design";

// PublicAiCredentialAuthorization declares a registered OAuth primitive or a Manifest browser callback.
// PublicAiCredentialAuthorization 声明已注册 OAuth 原语或 Manifest 浏览器回调。
export type PublicAiCredentialAuthorization =
  | {
      kind: "registered_oauth";
      provider: OAuthProviderKind;
    }
  | {
      kind: "browser_callback";
      profile: {
        authorization_url: string;
        callback_query_parameter: string;
        state_query_parameter: string;
        listener: {
          url_host: "localhost" | "loopback_ipv4";
          path: string;
          port_start: number;
          port_end: number;
          timeout_seconds: number;
          max_body_bytes: number;
        };
        response: {
          secret_json_pointer: string;
          state_json_pointer: string;
          subject_json_pointer: string;
          username_json_pointer: string;
          credential_label_json_pointer: string;
          error_json_pointer?: string;
          error_description_json_pointer?: string;
          secret_projection: "api_key" | "bearer";
        };
      };
    };

// CredentialBillingKind identifies the exact billing semantics of one credential profile.
// CredentialBillingKind 标识一个凭据配置档的精确计费语义。
export type CredentialBillingKind = "pay_as_you_go" | "token_plan" | "provider_managed";

// PublicAiCredentialProfile declares one exact credential acquisition and billing combination.
// PublicAiCredentialProfile 声明一种精确的凭据获取与计费组合。
export interface PublicAiCredentialProfile {
  id: string;
  display_name: string;
  credential_mode: CredentialMode;
  authorization?: PublicAiCredentialAuthorization;
  billing_kind: CredentialBillingKind;
  account_usage_query:
    | "open_vulcan"
    | "unsupported"
    | "mini_max_token_plan"
    | "open_router_credits"
    | "tavily_credits"
    | "deep_seek_balance"
    | "kimi_coding_plan"
    | "command_code_usage"
    | "open_design_wallet";
  authentication?: string;
  key_prefix?: string;
}

// PublicAiProtocolRoute describes one verified wire route.
// PublicAiProtocolRoute 描述一条已验证 Wire 路由。
export interface PublicAiProtocolRoute {
  protocol_profile_id: string;
  endpoint_path?: string;
  // Optional protocol-specific base URL; absent inherits the entry base URL.
  // 可选协议专属基础 URL；缺省时继承入口基础 URL。
  base_url_override?: string;
  // Optional route-specific API-key projection; absent inherits the entry projection.
  // 可选路由专属 API Key 投影；缺省时继承入口投影。
  authentication?: string;
  // Bounded non-sensitive fixed request headers scoped only to this route.
  // 仅作用于当前路由的有界非敏感固定请求头。
  request_headers?: Record<string, string>;
  model_discovery?: PublicAiModelDiscoveryKind;
  model_discovery_path?: string;
  responses_features?: {
    remote_compaction: PublicAiResponsesRemoteCompactionPolicy;
    websocket: PublicAiResponsesWebsocketPolicy;
  };
  // Route-owned input declarations determine whether user-added models may enable video input.
  // 路由自有输入声明决定用户新增模型能否启用视频输入。
  input_resource_constraints?: { resource_kind: PublicAiResourceKind }[];
  verified: boolean;
}

// PublicAiModelDiscoveryKind identifies one registered catalog acquisition contract.
// PublicAiModelDiscoveryKind 标识一种已注册的目录获取契约。
export type PublicAiModelDiscoveryKind =
  | "manual"
  | "provider_tools_only"
  | "open_router"
  | "manifest_static"
  | "open_ai"
  | "open_ai_codex"
  | "open_vulcan"
  | "anthropic"
  | "anthropic_compatible"
  | "gemini"
  | "google_code_assist";

// PublicAiServiceEntry describes one selectable provider entry.
// PublicAiServiceEntry 描述一个可选供应商入口。
export interface PublicAiServiceEntry {
  id: string;
  display_name: string;
  kind: "conversation" | "provider_tools";
  base_url: string;
  credential_profiles: PublicAiCredentialProfile[];
  authentication: string;
  // Optional header name that receives the current public session identifier.
  // 可选的请求头名称；配置后接收当前公共会话标识。
  session_header?: string;
  service_origin_override_policy?: PublicAiServiceOriginOverridePolicy;
  model_discovery: PublicAiModelDiscoveryKind;
  protocol_routes: PublicAiProtocolRoute[];
}

// PublicAiServiceOriginOverridePolicy projects one manifest-owned connection-origin contract.
// PublicAiServiceOriginOverridePolicy 投影一个由 Manifest 持有的连接源地址契约。
export interface PublicAiServiceOriginOverridePolicy {
  host_suffix: string;
  allowed_input_paths: string[];
  protocol_base_paths: Record<string, string>;
  example_origin: string;
}

// PublicAiProviderCategory identifies the provider's user-facing business relationship.
// PublicAiProviderCategory 标识供应商面向用户的业务关系类别。
export type PublicAiProviderCategory = "unclassified" | "official" | "cloud_maas" | "relay";

// PublicAiCredentialDirectoryVisibility controls zero-credential onboarding visibility.
// PublicAiCredentialDirectoryVisibility 控制零凭据引导入口的可见性。
export type PublicAiCredentialDirectoryVisibility = "configured_only" | "pinned_onboarding" | "core_account";

// PublicAiProviderPartnershipKind identifies the disclosed commercial relationship.
// PublicAiProviderPartnershipKind 标识已披露的商业合作关系。
export type PublicAiProviderPartnershipKind = "none" | "referral" | "sponsored";

// PublicAiProviderPromotionDisclosureKey is the closed set of localized disclosure messages.
// PublicAiProviderPromotionDisclosureKey 是本地化披露文案的封闭集合。
export type PublicAiProviderPromotionDisclosureKey =
  | "settings.providerPromotion.neutralDisclosure"
  | "settings.providerPromotion.referralDisclosure"
  | "settings.providerPromotion.sponsoredDisclosure";

// PublicAiProviderPromotion describes public registration and referral metadata.
// PublicAiProviderPromotion 描述公开注册与推广元数据。
export interface PublicAiProviderPromotion {
  signup_url: string;
  referral_url?: string;
  campaign_id?: string;
  partnership_kind: PublicAiProviderPartnershipKind;
  disclosure_key: PublicAiProviderPromotionDisclosureKey;
}

// PublicAiProviderDescriptor describes one registered provider adapter.
// PublicAiProviderDescriptor 描述一个已注册供应商适配器。
export interface PublicAiProviderDescriptor {
  id: string;
  display_name: string;
  kind: "system" | "custom";
  category: PublicAiProviderCategory;
  credential_directory_visibility: PublicAiCredentialDirectoryVisibility;
  // Closed credential onboarding primitive selected by the provider manifest.
  // 由供应商清单选择的封闭凭据接入原语。
  credential_registration: "direct" | "mini_max_regional_api_key";
  promotion?: PublicAiProviderPromotion;
  service_entries: PublicAiServiceEntry[];
}

// PublicAiCustomProviderProtocol identifies one user-selected compatible wire protocol.
// PublicAiCustomProviderProtocol 标识用户选择的一种兼容 Wire 协议。
export type PublicAiCustomProviderProtocol =
  | "anthropic_messages"
  | "gemini_generate_content"
  | "open_ai_chat"
  | "open_ai_responses"
  | "open_ai_codex_responses";
// PublicAiResponsesRemoteCompactionPolicy selects the closed remote compaction dialect.
// PublicAiResponsesRemoteCompactionPolicy 选择封闭的远端压缩方言。
export type PublicAiResponsesRemoteCompactionPolicy =
  | "disabled"
  | "enabled_with_local_fallback"
  | "inline_v2";

// PublicAiCustomProviderModelDiscoveryMode selects automatic, custom-path, or manual discovery.
// PublicAiCustomProviderModelDiscoveryMode 选择自动、自定义路径或手工发现。
export type PublicAiCustomProviderModelDiscoveryMode = "standard" | "custom_path" | "manual";

// PublicAiCustomProviderProtocolPreference selects Auto or an explicit protocol for one custom model.
// PublicAiCustomModelProtocolPreference 为一个自定义模型选择 Auto 或显式协议。
export type PublicAiCustomModelProtocolPreference =
  | { mode: "auto" }
  | { mode: "explicit"; protocol: PublicAiCustomProviderProtocol };

// PublicAiCustomProviderProtocolRoute overrides one protocol route with its own base URL,
// endpoint path, authentication, discovery, and fixed headers.
// PublicAiCustomProviderProtocolRoute 以专属基础 URL、端点路径、鉴权、发现与固定请求头
// 覆盖一条协议路由。
export interface PublicAiCustomProviderProtocolRoute {
  protocol: PublicAiCustomProviderProtocol;
  // Nested route fields use the Rust RPC wire names; the outer save request remains camelCase.
  // 嵌套路由字段使用 Rust RPC 线协议名称；外层保存请求仍保持 camelCase。
  base_url?: string;
  endpoint_path?: string;
  // Route-level API-key projection; omitted inherits the shared provider projection.
  // 路由级 API Key 投影；省略时继承供应商共享投影。
  authentication?: "bearer_authorization" | "google_api_key" | "x_api_key";
  model_discovery_mode?: PublicAiCustomProviderModelDiscoveryMode;
  model_discovery_path?: string;
  request_headers?: Record<string, string>;
}

// PublicAiCustomModelProtocolPreferenceRecord exposes one persisted choice without composite keys.
// PublicAiCustomModelProtocolPreferenceRecord 以无组合键形式暴露一条持久化选择。
export interface PublicAiCustomModelProtocolPreferenceRecord {
  providerId: string;
  modelId: string;
  preference: PublicAiCustomModelProtocolPreference;
}

// PublicAiResponsesWebsocketPolicy controls one explicit Codex Responses WebSocket route.
// PublicAiResponsesWebsocketPolicy 控制一条显式 Codex Responses WebSocket 路由。
export type PublicAiResponsesWebsocketPolicy = "disabled" | "verified" | "unverified_override";

// PublicAiCustomProviderSaveRequest creates or replaces one user-defined provider.
// PublicAiCustomProviderSaveRequest 创建或替换一个用户自定义供应商。
export interface PublicAiCustomProviderSaveRequest {
  providerId?: string;
  displayName: string;
  baseUrl: string;
  // The authoritative non-empty enabled protocol collection.
  // 权威的非空启用协议集合。
  protocols: PublicAiCustomProviderProtocol[];
  // Per-protocol route overrides; each protocol may appear at most once.
  // 逐协议路由覆盖；每个协议最多出现一次。
  protocolRoutes?: PublicAiCustomProviderProtocolRoute[];
  // Bounded non-sensitive fixed request headers shared by every protocol route.
  // 全部协议路由共享的有界非敏感固定请求头。
  requestHeaders?: Record<string, string>;
  // Optional regular-conversation session-header name for this provider.
  // 此供应商可选的常规会话请求头名称。
  sessionHeader?: string;
  remoteCompactionPolicy: PublicAiResponsesRemoteCompactionPolicy;
  websocketPolicy: PublicAiResponsesWebsocketPolicy;
  authentication: "bearer_authorization" | "google_api_key" | "x_api_key";
  credentialDisplayName?: string;
  secret?: string;
  validationConnectionId?: string;
  modelDiscoveryMode: PublicAiCustomProviderModelDiscoveryMode;
  modelDiscoveryPath?: string;
  allowManualFallback?: boolean;
  allowPendingValidation?: boolean;
  // Confirms deletion of model settings bound to protocols other than the selected target.
  // 确认删除绑定到所选目标之外协议的模型设置。
  confirmInactiveProtocolOverrideDeletion?: boolean;
}

// PublicAiCustomProviderProbeResult is one typed non-persistent credential observation.
// PublicAiCustomProviderProbeResult 是一项类型化且不持久化的凭据观测。
export interface PublicAiCustomProviderProbeResult {
  status: PublicAiCatalogDiscoveryStatus;
  catalog?: PublicAiConnectionCatalog;
  manualFallbackAvailable: boolean;
  pendingSaveAvailable: boolean;
}

// PublicAiCustomRequestMutation describes one provider-owned request-body mutation.
// PublicAiCustomRequestMutation 描述一项供应商自有的请求体变更。
export interface PublicAiCustomRequestMutation {
  set: Record<string, unknown>;
  delete: string[];
}

// PublicAiCustomModelRequestProjection maps model controls onto one custom provider protocol.
// PublicAiCustomModelRequestProjection 将模型控制映射到一个自定义供应商协议。
export interface PublicAiCustomModelRequestProjection {
  reasoning: Record<string, PublicAiCustomRequestMutation>;
  model_parameter_paths: Record<string, string>;
  defaults: Record<string, unknown>;
  overrides: Record<string, unknown>;
  delete: string[];
}

// PublicAiToolSchemaProjectionMode selects the provider-facing function-tool schema contract.
// PublicAiToolSchemaProjectionMode 选择 Provider 可见的函数工具 Schema 契约。
export type PublicAiToolSchemaProjectionMode = "lightweight" | "passthrough" | "strict";

// PublicAiStructuredOutputCapability identifies one model's strongest stable JSON-output contract.
// PublicAiStructuredOutputCapability 标识一个模型最强的稳定 JSON 输出契约。
export type PublicAiStructuredOutputCapability = "json_object" | "json_schema" | "unsupported";

// PublicAiReasoningEffort identifies one provider-neutral user-selectable reasoning level.
// PublicAiReasoningEffort 标识一个供应商无关且用户可选的思考档位。
export type PublicAiReasoningEffort =
  | "none"
  | "adaptive"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh"
  | "max"
  | "ultra";

// PublicAiCustomModelDefinition is one credential-independent user model overlay.
// PublicAiCustomModelDefinition 是一个与凭据无关的用户模型覆盖定义。
export interface PublicAiCustomModelDefinition {
  id: string;
  display_name: string;
  enabled: boolean;
  capabilities: PublicAiModelCapabilities;
  request_projection: PublicAiCustomModelRequestProjection;
  tool_schema_projection: PublicAiToolSchemaProjectionMode;
  // system_binding scopes a user-added system model to one verified entry and protocol.
  // system_binding 将用户新增系统模型限制到一个已验证入口和协议。
  system_binding?: { service_entry_id: string; protocol_profile_id: string };
}

// PublicAiModelCapabilityOverrides stores only user differences from immutable capability data.
// PublicAiModelCapabilityOverrides 仅保存用户相对于不可变能力数据的差异。
export interface PublicAiModelCapabilityOverrides {
  context_limit?: number;
  output_limit?: number;
  input_resources?: PublicAiResourceKind[];
  structured_output?: PublicAiStructuredOutputCapability;
  reasoning?: string;
  reasoning_control?: string;
  reasoning_efforts?: string[];
  // Optional exact request projection shared by custom and native provider models.
  // 自定义供应商与系统供应商模型共同使用的可选精确请求投影。
  request_projection?: PublicAiCustomModelRequestProjection;
}

// PublicAiContextWindowPolicyOverrides stores sparse Codex window defaults.
// PublicAiContextWindowPolicyOverrides 保存稀疏 Codex 窗口默认值。
export interface PublicAiContextWindowPolicyOverrides {
  effective_context_window_percent?: number;
  auto_compact_percent?: number;
  auto_compact_token_limit?: number;
  auto_compact_scope?: "total" | "body_after_prefix";
  fallback_buffer_tokens?: number;
}

// ContextCompactionWindowNodeId is one immutable backend-owned node identity.
// ContextCompactionWindowNodeId 是一个由后台持有的不可变节点身份。
export type ContextCompactionWindowNodeId =
  | "compression-window-1"
  | "compression-window-2"
  | "compression-window-3"
  | "compression-window-4"
  | "compression-window-5";

// PublicAiContextCompactionWindowNode exposes one fixed node and its editable token value.
// PublicAiContextCompactionWindowNode 公开一个固定节点及其可编辑 Token 值。
export interface PublicAiContextCompactionWindowNode {
  // id is immutable and is the only value persisted by sessions.
  // id 不可变，且是会话唯一持久化的值。
  id: ContextCompactionWindowNodeId;
  // tokens is the current backend-authoritative compaction-window value.
  // tokens 是当前由后台权威持有的压缩窗口值。
  tokens: number;
}

// PublicAiContextCompactionWindowConfiguration contains exactly five ordered nodes.
// PublicAiContextCompactionWindowConfiguration 包含恰好五个有序节点。
export interface PublicAiContextCompactionWindowConfiguration {
  // nodes remain in immutable low-to-high product order.
  // nodes 保持不可变的产品从低到高顺序。
  nodes: [
    PublicAiContextCompactionWindowNode,
    PublicAiContextCompactionWindowNode,
    PublicAiContextCompactionWindowNode,
    PublicAiContextCompactionWindowNode,
    PublicAiContextCompactionWindowNode,
  ];
}

// PublicAiCustomProviderModelDefaults is the safe provider-wide fallback capability contract.
// PublicAiCustomProviderModelDefaults 是安全的供应商级回退能力契约。
export interface PublicAiCustomProviderModelDefaults {
  context_limit: number;
  output_limit: number;
  input_resources?: PublicAiResourceKind[];
  structured_output?: PublicAiStructuredOutputCapability;
  reasoning: string;
  reasoning_control: string;
  reasoning_efforts: string[];
  // Optional provider-wide request projection inherited by compatible models.
  // 可选的兼容模型继承供应商级请求投影。
  request_projection?: PublicAiCustomModelRequestProjection;
}

// PublicAiCustomModelTemplateSelection is one immutable user-reviewed template snapshot.
// PublicAiCustomModelTemplateSelection 是一份不可变的用户审查模板快照。
export interface PublicAiCustomModelTemplateSelection {
  template_key: string;
  template_revision: string;
  context_limit: number;
  supports_text_input: boolean;
  supports_text_output: boolean;
  input_resources?: PublicAiResourceKind[];
  output_resources?: PublicAiResourceKind[];
}

// PublicAiCustomModelTemplateDecision records an applied or rejected local template revision.
// PublicAiCustomModelTemplateDecision 记录已应用或已拒绝的本地模板修订。
export type PublicAiCustomModelTemplateDecision =
  | { decision: "applied"; selection: PublicAiCustomModelTemplateSelection }
  | { decision: "rejected"; template_key: string; template_revision: string };

// PublicAiCommonModelTemplateSuggestion describes one offline capability-template recommendation.
// PublicAiCommonModelTemplateSuggestion 描述一项离线能力模板建议。
export interface PublicAiCommonModelTemplateSuggestion {
  template_key: string;
  template_revision: string;
  match_kind: "exact" | "alias" | "provider_agnostic";
  context_limit: number;
  supports_text_input: boolean;
  supports_text_output: boolean;
  input_resources: PublicAiResourceKind[];
  output_resources: PublicAiResourceKind[];
}

// PublicAiCustomModelTemplateSuggestionsRecord carries local suggestions for one exact model.
// PublicAiCustomModelTemplateSuggestionsRecord 承载一个精确模型的本地模板建议。
export interface PublicAiCustomModelTemplateSuggestionsRecord {
  modelId: string;
  suggestions: PublicAiCommonModelTemplateSuggestion[];
}

// PublicAiCustomModelTemplateDecisionBatchItem carries one reviewed batch decision.
// PublicAiCustomModelTemplateDecisionBatchItem 承载一项已审查批量决策。
export interface PublicAiCustomModelTemplateDecisionBatchItem {
  modelId: string;
  decision: PublicAiCustomModelTemplateDecision;
}

// PublicAiCatalogDiscoveryStatus carries one closed process-local discovery observation.
// PublicAiCatalogDiscoveryStatus 承载一项封闭的进程内发现观测。
export interface PublicAiCatalogDiscoveryStatus {
  state:
    | "not_checked"
    | "ready"
    | "empty"
    | "unsupported"
    | "authentication_failed"
    | "rate_limited"
    | "temporarily_unavailable"
    | "invalid_response"
    | "stale"
    | "failed";
  failure_kind?:
    | "unsupported"
    | "authentication_failed"
    | "rate_limited"
    | "temporarily_unavailable"
    | "invalid_response"
    | "failed";
  message?: string;
  model_count: number;
  generation: number;
  observed_at_ms: number;
}

// PublicAiCustomModelTemplateDecisionRecord identifies one decision without composite-key parsing.
// PublicAiCustomModelTemplateDecisionRecord 无需解析组合键即可标识一项决策。
export interface PublicAiCustomModelTemplateDecisionRecord {
  providerId: string;
  modelId: string;
  decision: PublicAiCustomModelTemplateDecision;
}

// PublicAiCustomModelConnectionAvailabilityRecord identifies one explicit account-model choice.
// PublicAiCustomModelConnectionAvailabilityRecord 标识一项显式账号模型选择。
export interface PublicAiCustomModelConnectionAvailabilityRecord {
  connectionId: string;
  modelId: string;
  enabled: boolean;
}

// PublicAiModelDirectoryKind identifies whether one account model list is fixed, dynamic, manual, or unavailable.
// PublicAiModelDirectoryKind 标识一个账号模型列表是固定、动态、手工还是不可用。
export type PublicAiModelDirectoryKind = "fixed" | "dynamic" | "manual" | "unavailable";

// PublicAiConnectionModelAccessPolicyRecord carries one account's inheritance mode and explicit model decisions.
// PublicAiConnectionModelAccessPolicyRecord 承载一个账号的继承模式与显式模型决定。
export interface PublicAiConnectionModelAccessPolicyRecord {
  connectionId: string;
  defaultEnabled: boolean;
  inheritModelDefaults: boolean;
  modelEnabledOverrides: Record<string, boolean>;
  directoryKind: PublicAiModelDirectoryKind;
}

// PublicAiConnection carries redacted credential connection metadata.
// PublicAiConnection 承载脱敏凭据连接元数据。
export interface PublicAiConnection {
  id: string;
  provider_id: string;
  service_entry_id: string;
  credential_profile_id: string;
  billing_kind: CredentialBillingKind;
  usage_capability:
    | "unsupported"
    | "mini_max_token_plan"
    | "open_router_credits"
    | "tavily_credits"
    | "deep_seek_balance"
    | "kimi_coding_plan"
    | "command_code_usage"
    | "open_design_wallet";
  // Whether usage refresh needs a separate provider-management credential.
  // 用量刷新是否需要独立的供应商管理凭据。
  usage_requires_management_key: boolean;
  has_management_key: boolean;
  service_origin_override?: string;
  credential_mode: CredentialMode;
  credential_id: string;
  display_name: string;
  // Whether this stored credential currently participates in model and tool routing.
  // 此已保存凭据当前是否参与模型与工具路由。
  enabled: boolean;
  prompt_cache_by_protocol: Record<string, unknown>;
  // Whether normal routing is blocked until one explicit model validation succeeds.
  // 在一次显式模型验证成功前是否阻止常规路由。
  pending_validation: boolean;
}

// PublicAiCustomModelValidationResult carries compact facts from an explicit paid model test.
// PublicAiCustomModelValidationResult 承载一次显式付费模型测试的紧凑事实。
export interface PublicAiCustomModelValidationResult {
  providerId: string;
  connectionId: string;
  modelId: string;
  responseText: string;
  responseCharacters: number;
}

// PublicAiModel describes one runtime-discovered model.
// PublicAiModel 描述一个运行时发现模型。
export interface PublicAiModel {
  id: string;
  display_name: string;
  capabilities: PublicAiModelCapabilities;
  capability_evidence?: PublicAiModelCapabilityEvidence;
}

// PublicAiModelCapabilityEvidence preserves exact field-presence facts from one credential directory.
// PublicAiModelCapabilityEvidence 保留一个凭据目录返回的精确字段存在性事实。
export interface PublicAiModelCapabilityEvidence {
  context_limit_declared: boolean;
  output_limit_declared: boolean;
  input_modalities_declared: boolean;
  output_modalities_declared: boolean;
  structured_output_declared?: boolean;
  supports_text_input?: boolean;
  supports_text_output?: boolean;
}

// PublicAiNativeToolKind identifies one provider-executed capability declared by an exact model route.
// PublicAiNativeToolKind 标识由精确模型路由声明的一项供应商执行能力。
export type PublicAiNativeToolKind = "web_search" | "code_interpreter" | "image_generation";

// PublicAiModelCapabilities carries exact route controls used by workbench and internal inference.
// PublicAiModelCapabilities 承载工作台与内部推理使用的精确路由控件。
export interface PublicAiModelCapabilities {
  operations?: PublicAiOperation[];
  context_limit?: number;
  output_limit?: number;
  // Resource modalities accepted by this exact model route.
  // 此精确模型路由接受的资源模态。
  input_resources?: PublicAiResourceKind[];
  // Resource modalities emitted by this exact model route.
  // 此精确模型路由输出的资源模态。
  output_resources?: PublicAiResourceKind[];
  // Stable structured-output capability for this exact model route.
  // 此精确模型路由的稳定结构化输出能力。
  structured_output?: PublicAiStructuredOutputCapability;
  // Provider-native tools available on this exact protocol and model route.
  // 此精确协议与模型路由可用的供应商原生工具。
  provider_native_tools?: PublicAiNativeToolKind[];
  // Evidence revision proving the exact capability projection.
  // 证明精确能力投影的证据修订号。
  evidence_revision: string;
  reasoning: string;
  reasoning_control?: string;
  reasoning_efforts?: string[];
  prompt_cache: PublicAiPromptCacheCapability;
  model_parameters: PublicAiModelParameterDefinition[];
  embedding?: PublicAiEmbeddingCapabilities;
  rerank?: PublicAiRerankCapabilities;
  [key: string]: unknown;
}

// PublicAiResourceKind identifies one provider-neutral non-text modality.
// PublicAiResourceKind 标识一种供应商无关的非文本模态。
export type PublicAiResourceKind = "image" | "audio" | "video" | "file";

// PublicAiEmbeddingCapabilities describes exact embedding defaults and request limits.
// PublicAiEmbeddingCapabilities 描述精确向量默认值与请求限制。
export interface PublicAiEmbeddingCapabilities {
  tasks: string[];
  output_kinds: string[];
  encodings: string[];
  dimensions: number[];
  minimum_dimensions?: number;
  maximum_dimensions?: number;
  default_dimensions?: number;
  maximum_inputs?: number;
  maximum_tokens_per_input?: number;
  normalization: string;
  distance_metric: string;
}

// PublicAiRerankCapabilities describes exact rerank defaults and request limits.
// PublicAiRerankCapabilities 描述精确重排默认值与请求限制。
export interface PublicAiRerankCapabilities {
  maximum_candidates?: number;
  maximum_tokens_per_item?: number;
  maximum_request_tokens?: number;
  truncation: string[];
  supports_return_content: boolean;
  score_semantics: string;
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
  // Required sibling parameter identifiers declared by the adapter.
  // 适配器声明的必需同级参数标识。
  dependent_required: string[];
  // Conflicting sibling parameter identifiers declared by the adapter.
  // 适配器声明的冲突同级参数标识。
  conflicts: string[];
  // Evidence revision proving the exact parameter contract.
  // 证明精确参数契约的证据修订号。
  evidence_revision: string;
}

// PublicAiConnectionCatalog carries one memory-only model catalog.
// PublicAiConnectionCatalog 承载一个仅内存模型目录。
export interface PublicAiConnectionCatalog {
  connection_id: string;
  models: PublicAiModel[];
  generation: number;
  // Number of upstream pages consumed for this catalog snapshot.
  // 此目录快照已读取的上游分页数量。
  page_count: number;
  // Revision of the parser contract that produced this catalog snapshot.
  // 生成此目录快照的解析器契约修订号。
  discovery_schema_revision: string;
}

// PublicAiCodexModelBehavior carries the exact closed model behavior frozen by the backend.
// PublicAiCodexModelBehavior 承载后端冻结的精确封闭模型行为。
export interface PublicAiCodexModelBehavior {
  wire_dialect: "standard" | "lite";
  tool_mode: "direct" | "code_mode" | "code_mode_only";
  apply_patch_tool_type: "function" | "freeform";
  shell_type: "shell_command" | "unified_exec";
  web_search_tool_type: "text" | "text_and_image";
  supports_search_tool: boolean;
  experimental_supported_tools: string[];
  supports_parallel_tool_calls: boolean;
  supports_image_detail_original: boolean;
  supports_verbosity: boolean;
  default_verbosity?: "low" | "medium" | "high";
  multi_agent_version?: "v1" | "v2";
  multi_agent_reasoning_effort?: string;
  include_skills_usage_instructions: boolean;
  include_apps_usage_instructions: boolean;
  include_plugin_usage_instructions: boolean;
  node_repl_disabled: boolean;
  node_repl_auto_review_required: boolean;
  requires_sandboxed_review: boolean;
  auto_review_model_override?: string;
  comp_hash?: string;
  tool_output_truncation?: { mode: "tokens"; limit: number };
  prefer_websockets: boolean;
  supports_experimental_context: boolean;
}

// PublicAiModelRouteRecord carries one backend-resolved model protocol route status.
// PublicAiModelRouteRecord 承载一条由后端解析的模型协议路由状态。
export interface PublicAiModelRouteRecord {
  // Exact provider identifier.
  // 精确供应商标识。
  providerId: string;
  // Exact credential connection identifier.
  // 精确凭据连接标识。
  connectionId: string;
  // Exact service-entry identifier.
  // 精确服务入口标识。
  serviceEntryId: string;
  // Exact upstream model identifier.
  // 精确上游模型标识。
  modelId: string;
  // Backend-authoritative effective protocol identifier when a binding exists.
  // 存在绑定时由后端权威给出的生效协议标识。
  protocolId?: string;
  // Protocol variants available in this exact account catalog.
  // 此精确账号目录中可用的协议变体。
  availableProtocols: string[];
  // Stable backend binding-source identifier when a binding exists.
  // 存在绑定时的稳定后端绑定来源标识。
  bindingSource?: "system_manifest" | "system_user" | "custom_user" | "custom_auto_default";
  // Stable manifest rule identifier when present.
  // 存在时的稳定 Manifest 规则标识。
  bindingRuleId?: string;
  // Exact closed Codex behavior frozen by the matched model rule.
  // 由匹配模型规则冻结的精确封闭 Codex 行为。
  codexBehavior?: PublicAiCodexModelBehavior;
  // Stable runtime primitive required before this route can execute.
  // 当前路由可执行前所需的稳定运行时原语。
  requiredRuntimePrimitive?: string;
  // Activated provider-manifest revision owning the binding.
  // 持有绑定的已激活供应商 Manifest 修订。
  manifestRevision: number;
  // Whether request execution can consume this exact account/model route.
  // 请求执行是否可消费此精确账号/模型路由。
  executable: boolean;
  // Stable backend-owned diagnostic when the route is not executable.
  // 路由不可执行时由后端持有的稳定诊断。
  unavailableReason?: string;
}

// PublicAiActiveSelection identifies one exact model and account.
// PublicAiActiveSelection 标识一个精确模型与账号。
export interface PublicAiActiveSelection {
  provider_id: string;
  model_id: string;
  connection_id: string;
}

// PublicAiModelOverrides stores protocol-scoped cache choices and registered parameter values.
// PublicAiModelOverrides 保存协议作用域缓存选择与注册参数值。
export interface PublicAiModelOverrides {
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

// PublicAiModelAccessRecord carries one provider-model effective and default conversation state.
// PublicAiModelAccessRecord 承载一个供应商模型的有效及默认会话状态。
export interface PublicAiModelAccessRecord {
  providerId: string;
  modelId: string;
  enabled: boolean;
  defaultEnabled: boolean;
  customized?: boolean;
}

// PublicAiModelCapabilityOverrideRecord identifies one capability difference without key parsing.
// PublicAiModelCapabilityOverrideRecord 无需解析键即可标识一项能力差异。
export interface PublicAiModelCapabilityOverrideRecord {
  providerId: string;
  modelId: string;
  overrides: PublicAiModelCapabilityOverrides;
}

// PublicAiProviderToolCapabilityKind identifies one direct or model-extension provider capability.
// PublicAiProviderToolCapabilityKind 标识一种直连或模型扩展供应商能力。
export type PublicAiProviderToolCapabilityKind =
  | "web_search"
  | "web_extract"
  | "image_understand"
  | "video_understand"
  | "audio_understand"
  | "image_generate"
  | "image_edit"
  | "video_generate"
  | "video_edit"
  | "video_extend"
  | "speech_synthesize"
  | "speech_transcribe"
  | "music_generate"
  | "music_cover";

// PublicAiProviderToolExtensionProfile describes one reviewed native extension model.
// PublicAiProviderToolExtensionProfile 描述一个经过审核的原生扩展模型。
export interface PublicAiProviderToolExtensionProfile {
  summary: string;
  characteristics?: string[];
  input_modes?: string[];
  resolution_policy?: {
    mode: "fixed_presets" | "bounded_custom" | "provider_managed";
    presets?: Array<{ value: string; width?: number; height?: number }>;
    aspect_ratios?: string[];
    custom_bounds?: {
      minimum_width: number;
      maximum_width: number;
      minimum_height: number;
      maximum_height: number;
      step: number;
      minimum_pixels?: number;
      maximum_pixels?: number;
    };
  };
  duration_policy?: {
    minimum_seconds: number;
    maximum_seconds: number;
    default_seconds?: number;
    presets_seconds?: number[];
  };
  format_policy?: {
    input_formats?: string[];
    output_formats?: string[];
    sample_rates_hz?: number[];
    channel_counts?: number[];
  };
  constraints?: string[];
  limitations?: string[];
  evidence_refs?: string[];
}

// PublicAiOperation identifies one provider-neutral AI operation.
// PublicAiOperation 标识一项供应商无关 AI 操作。
export type PublicAiOperation =
  | "conversation_respond"
  | "embedding_create"
  | "rerank_documents"
  | "decision_evaluate";

// PublicAiInferenceUsageScope identifies one host-authorized usage context.
// PublicAiInferenceUsageScope 标识一种由宿主授权的用途上下文。
export type PublicAiInferenceUsageScope =
  | "interactive_agent"
  | "internal_background"
  | "external_service";

// PublicAiInferenceRouteBinding selects one exact protocol or native action.
// PublicAiInferenceRouteBinding 选择一个精确协议或原生动作。
export type PublicAiInferenceRouteBinding =
  | {
      kind: "protocol";
      protocol_profile_id: string;
    }
  | {
      kind: "action";
      action_binding_id: string;
    };

// PublicAiInferenceRoute persists one exact immutable internal route.
// PublicAiInferenceRoute 持久化一条精确不可变内部路由。
export interface PublicAiInferenceRoute {
  provider_id: string;
  service_entry_id: string;
  connection_id: string;
  model_id: string;
  binding: PublicAiInferenceRouteBinding;
  operation: PublicAiOperation;
  usage_scope: PublicAiInferenceUsageScope;
}

// PublicAiInternalInferenceCandidate describes one currently executable route choice.
// PublicAiInternalInferenceCandidate 描述一项当前可执行路由选择。
export interface PublicAiInternalInferenceCandidate {
  providerDisplayName: string;
  serviceEntryDisplayName: string;
  connectionDisplayName: string;
  modelDisplayName: string;
  route: PublicAiInferenceRoute;
  catalogGeneration: number;
  enabled: boolean;
  usageScopes: PublicAiInferenceUsageScope[];
  capabilities: PublicAiModelCapabilities;
  overrides: PublicAiInternalModelOverrides;
}

// PublicAiInferenceProfile stores consumer-isolated routes.
// PublicAiInferenceProfile 保存消费者隔离路由。
export interface PublicAiInferenceProfile {
  // Optional vector dimension owned by this exact consumer profile.
  // 由此精确消费者配置档持有的可选向量维度。
  embedding_dimensions?: number;
  llm_routes?: Record<string, PublicAiInferenceRoute>;
  // Exact reasoning effort selected independently for each restricted LLM purpose.
  // 为每个受限 LLM 用途独立选择的准确思考档位。
  llm_reasoning_efforts?: Record<string, PublicAiReasoningEffort>;
  embedding?: PublicAiInferenceRoute;
  rerank?: PublicAiInferenceRoute;
  decision?: PublicAiInferenceRoute;
}

// PublicAiInternalModelOverrides stores operation-scoped defaults.
// PublicAiInternalModelOverrides 保存操作作用域默认值。
export interface PublicAiInternalModelOverrides {
  context_limit?: number;
  output_limit?: number;
  reasoning_effort?: string;
  model_parameters?: Record<string, unknown>;
  embedding_task?: string;
  embedding_dimensions?: number;
  embedding_output_kind?: string;
  embedding_encoding?: string;
  rerank_top_n?: number;
  rerank_truncation?: string;
  rerank_return_content?: boolean;
}

// PublicAiProviderToolContract carries one selected-model-native schema and unified output contract.
// PublicAiProviderToolContract 承载一个选定模型原生 Schema 与统一输出契约。
export interface PublicAiProviderToolContract {
  tool_id: string;
  description: string;
  capability: PublicAiProviderToolCapabilityKind;
  input_schema: Record<string, unknown>;
  extension_profile?: PublicAiProviderToolExtensionProfile;
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
  availability_model_ids_any?: string[];
  display_name: string;
  contract: PublicAiProviderToolContract;
}

// PublicAiProviderToolTargetSnapshot pairs one target with one configured credential.
// PublicAiProviderToolTargetSnapshot 将一个目标与一个已配置凭据配对。
export interface PublicAiProviderToolTargetSnapshot {
  connectionId: string;
  connectionDisplayName: string;
  manifestSourceLayer: "bundled" | "active" | "custom" | "previous";
  target: PublicAiProviderToolTarget;
}

// PublicAiProviderToolTemplateModel keeps one signed media/tool model manageable while disabled.
// PublicAiProviderToolTemplateModel 让一个签名媒体/工具模型在关闭时仍可管理。
export interface PublicAiProviderToolTemplateModel {
  // Exact owning provider identifier.
  // 精确所属供应商标识。
  providerId: string;
  // Exact configured credential connection identifier.
  // 精确已配置凭据连接标识。
  connectionId: string;
  // Exact regional or product service entry identifier.
  // 精确区域或产品服务入口标识。
  serviceEntryId: string;
  // Exact upstream media or provider-tool model identifier.
  // 精确上游媒体或供应商工具模型标识。
  modelId: string;
  // User-visible signed catalog name.
  // 用户可见的签名目录名称。
  displayName: string;
  // Exact signed capabilities, absent only while an older running service supplies its previous DTO.
  // 精确签名能力；仅旧版运行中服务返回先前 DTO 时缺失。
  capabilities?: PublicAiProviderToolCapabilityKind[];
}

// PublicAiProviderToolTargetSelection stores the one selected target for one capability.
// PublicAiProviderToolTargetSelection 保存某项能力唯一选中的目标。
export interface PublicAiProviderToolTargetSelection {
  connection_id: string;
  target_id: string;
}

// PublicAiMediaUnderstandingBackendSelection chooses one closed backend for one media tool.
// PublicAiMediaUnderstandingBackendSelection 为一项媒体工具选择一个封闭后端。
export type PublicAiMediaUnderstandingBackendSelection =
  | {
      mode: "provider_tool";
      selection: PublicAiProviderToolTargetSelection;
    }
  | {
      mode: "model_replacement";
      selection: PublicAiActiveSelection;
      reasoning_effort?: PublicAiReasoningEffort;
    };

// PublicAiMediaUnderstandingBackendMode is the session-selectable backend family.
// PublicAiMediaUnderstandingBackendMode 是会话可选择的后端类型。
export type PublicAiMediaUnderstandingBackendMode =
  | "provider_tool"
  | "model_replacement";

// PublicAiMediaUnderstandingBackendOptions keeps exact global routes for both backend families.
// PublicAiMediaUnderstandingBackendOptions 分别保存两种后端类型的全局精确路由。
export interface PublicAiMediaUnderstandingBackendOptions {
  // Exact provider-specific target configured for this capability.
  // 为该能力配置的精确供应商专用目标。
  provider_tool?: PublicAiProviderToolTargetSelection;
  // Exact multimodal model route configured as the system default for this capability.
  // 为该能力配置为系统默认值的精确多模态模型路由。
  model_replacement?: {
    selection: PublicAiActiveSelection;
    reasoning_effort?: PublicAiReasoningEffort;
  };
}

// PublicAiMultimodalUnderstandingCandidateRecord exposes one shared system-default candidate switch.
// PublicAiMultimodalUnderstandingCandidateRecord 公开一个共享的系统默认候选开关。
export interface PublicAiMultimodalUnderstandingCandidateRecord {
  providerId: string;
  modelId: string;
}

// PublicAiMediaUnderstandingModelCandidate exposes one exact executable replacement route.
// PublicAiMediaUnderstandingModelCandidate 公开一条精确可执行的替代理解路由。
export interface PublicAiMediaUnderstandingModelCandidate {
  providerDisplayName: string;
  serviceEntryDisplayName: string;
  connectionDisplayName: string;
  modelDisplayName: string;
  selection: PublicAiActiveSelection;
  catalogGeneration: number;
  capabilities: PublicAiModelCapabilities;
}

// PublicAiSessionProviderToolConflict describes one retained choice that requires confirmation.
// PublicAiSessionProviderToolConflict 描述一项需要确认的已保留选择。
export interface PublicAiSessionProviderToolConflict {
  capability: PublicAiProviderToolCapabilityKind;
  kind:
    | "missing_selection"
    | "target_unavailable"
    | "invalid_direct_capability"
    | "invalid_extension_target_set";
  message: string;
}

// PublicAiSessionProviderToolSettings owns one complete session capability snapshot.
// PublicAiSessionProviderToolSettings 持有一份完整会话能力快照。
export interface PublicAiSessionProviderToolSettings {
  enabledCapabilities: PublicAiProviderToolCapabilityKind[];
  selections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >;
  extensionTargetSets: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection[]>
  >;
  // Exact session-owned route for each retained media-understanding capability.
  // 当前会话为每项保留的媒体理解能力持有的精确路由。
  mediaUnderstandingSelections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiMediaUnderstandingBackendSelection>
  >;
  conflicts: PublicAiSessionProviderToolConflict[];
}

// PublicAiProviderToolSearchResult carries one normalized ranked search item.
// PublicAiProviderToolSearchResult 承载一项规范化排序搜索结果。
export interface PublicAiProviderToolSearchResult {
  // rank is the one-based provider result position.
  // rank 是从一开始的供应商结果位置。
  rank: number;
  // title is the provider-returned result title.
  // title 是供应商返回的结果标题。
  title: string;
  // url is the validated absolute result address.
  // url 是经过校验的绝对结果地址。
  url: string;
  // source_domain is the normalized result hostname.
  // source_domain 是规范化结果主机名。
  source_domain: string;
  // snippet is the provider-returned result excerpt.
  // snippet 是供应商返回的结果摘要。
  snippet: string;
}

// PublicAiProviderToolSearchOutput carries one unified provider search response.
// PublicAiProviderToolSearchOutput 承载一份统一供应商搜索响应。
export interface PublicAiProviderToolSearchOutput {
  // query is the exact submitted search text.
  // query 是精确提交的搜索文本。
  query: string;
  // results contains ordered normalized search evidence.
  // results 包含有序的规范化搜索证据。
  results: PublicAiProviderToolSearchResult[];
}

// PublicAiProviderToolExtractResult carries one successfully extracted page.
// PublicAiProviderToolExtractResult 承载一个成功抓取的页面。
export interface PublicAiProviderToolExtractResult {
  // url is the validated source address.
  // url 是经过校验的来源地址。
  url: string;
  // content is provider-returned Markdown or plain text.
  // content 是供应商返回的 Markdown 或纯文本。
  content: string;
  // images contains validated image addresses returned for this page.
  // images 包含当前页面返回的已验证图片地址。
  images: string[];
  // favicon is the optional validated site icon address.
  // favicon 是可选的已验证站点图标地址。
  favicon?: string | null;
}

// PublicAiProviderToolExtractFailure carries one provider-reported URL failure.
// PublicAiProviderToolExtractFailure 承载一项供应商报告的 URL 失败。
export interface PublicAiProviderToolExtractFailure {
  // url is the failed source address.
  // url 是失败的来源地址。
  url: string;
  // error is the controlled provider failure message.
  // error 是受控的供应商失败消息。
  error: string;
}

// PublicAiProviderToolExtractOutput carries unified extraction successes and failures.
// PublicAiProviderToolExtractOutput 承载统一网页抓取成功项与失败项。
export interface PublicAiProviderToolExtractOutput {
  // results contains ordered successful pages.
  // results 包含有序的成功页面。
  results: PublicAiProviderToolExtractResult[];
  // failures contains ordered per-URL failures.
  // failures 包含有序的逐 URL 失败。
  failures: PublicAiProviderToolExtractFailure[];
}

// PublicAiProviderToolTestResult is the closed settings-diagnostic result union.
// PublicAiProviderToolTestResult 是封闭的设置诊断结果联合。
export type PublicAiProviderToolTestResult = {
  // upstream_response_id is the optional provider trace identifier.
  // upstream_response_id 是可选的供应商追踪标识。
  upstream_response_id?: string | null;
  // output contains the normalized semantic result selected by kind.
  // output 包含由 kind 选择的规范化语义结果。
  output:
    | {
        kind: "search";
        value: PublicAiProviderToolSearchOutput;
      }
    | {
        kind: "extract";
        value: PublicAiProviderToolExtractOutput;
      };
};

// ProviderUsageDailySummary carries one native daily provider accounting row.
// ProviderUsageDailySummary 承载一条原生供应商每日计量行。
export interface ProviderUsageDailySummary {
  calendarDate: string;
  providerId: string;
  serviceEntryId: string;
  consumerId: string;
  operation: string;
  modelId: string;
  currency: string;
  attemptCount: number;
  successfulAttemptCount: number;
  reportedUsageAttemptCount: number;
  inputTokens?: number | null;
  uncachedInputTokens?: number | null;
  cacheReadTokens?: number | null;
  cacheWriteTokens?: number | null;
  outputTokens?: number | null;
  reasoningTokens?: number | null;
  totalTokens?: number | null;
  reportedCost?: string | null;
  estimatedCost?: string | null;
  aggregateRevision: number;
}

// ProviderUsageBillingDetail preserves one provider-defined billing partition.
// ProviderUsageBillingDetail 保留一项供应商定义的计费分区。
export interface ProviderUsageBillingDetail {
  billing_type: string;
  input_tokens?: number;
  output_tokens?: number;
  total_tokens?: number;
  text_tokens?: number;
  image_tokens?: number;
  reasoning_tokens?: number;
}

// ProviderUsageAttemptSummary exposes one recent physical attempt and its frozen pricing source.
// ProviderUsageAttemptSummary 公开一条近期物理 attempt 及其冻结价格来源。
export interface ProviderUsageAttemptSummary {
  /** Exact physical dispatch timestamp; null when unobserved.
   * 精确物理分派时间；未观测时为空。 */
  dispatchedAtMs: number | null;
  /** Exact first text/reasoning token timestamp, independent of transport headers.
   * 精确首文本或推理 Token 时间，与传输响应头独立。 */
  firstTokenAtMs: number | null;
  attemptId: string;
  consumerId: string;
  operation: string;
  providerId: string;
  serviceEntryId: string;
  modelId: string;
  transport?: string | null;
  httpStatus?: number | null;
  websocketCloseCode?: number | null;
  errorCategory?: string | null;
  errorCode?: string | null;
  status: string;
  usageAvailability: "missing" | "partial" | "final";
  totalTokens?: number | null;
  billingDetails: ProviderUsageBillingDetail[];
  reportedCost?: string | null;
  estimatedCost?: string | null;
  currency?: string | null;
  pricingRevision?: string | null;
  pricingSourceKind?: string | null;
  pricingSourceId?: string | null;
  terminalAtMs: number;
}

// ProviderAccountUsageSummary carries one latest native account observation.
// ProviderAccountUsageSummary 承载一条最新原生账户观测。
export interface ProviderAccountUsageSummary {
  providerId: string;
  connectionId: string;
  snapshotKind: string;
  endpointProfileRevision: string;
  payload: PublicAiAccountUsage;
  observedAtMs: number;
}

// ProviderUsageDashboard exposes native usage, cost, and account facts to users.
// ProviderUsageDashboard 向用户公开原生用量、成本与账户事实。
export interface ProviderUsageDashboard {
  generatedAtMs: number;
  schemaRevision: number;
  aggregateRevision: number;
  healthStatus: string;
  daily: ProviderUsageDailySummary[];
  recentAttempts: ProviderUsageAttemptSummary[];
  accounts: ProviderAccountUsageSummary[];
}

// PublicAiSettingsSnapshot combines persisted and process-local settings state.
// PublicAiSettingsSnapshot 组合持久化设置状态与进程内设置状态。
export interface PublicAiSettingsSnapshot {
  // manualModelDefaults uses the backend's one default pair for new conversation models.
  // manualModelDefaults 为新增会话模型使用后端的唯一默认值。
  manualModelDefaults: { contextLimit: number; outputLimit: number };
  // mediaDelivery exposes the backend's effective policy, revision and configurable ceilings.
  // mediaDelivery 公开后端生效策略、修订及可配置上限。
  mediaDelivery: PublicAiMediaDeliverySettingsSnapshot;
  // contextWindowPolicyOverrides is the system-level sparse Codex window policy.
  // contextWindowPolicyOverrides 是系统级稀疏 Codex 窗口策略。
  contextWindowPolicyOverrides: PublicAiContextWindowPolicyOverrides;
  // contextCompactionWindowNodes exposes the complete fixed system node configuration.
  // contextCompactionWindowNodes 公开完整的固定系统节点配置。
  contextCompactionWindowNodes: PublicAiContextCompactionWindowConfiguration;
  // newSessionContextCompactionWindowNodeId applies only to newly created conversations.
  // newSessionContextCompactionWindowNodeId 仅应用于新建会话。
  newSessionContextCompactionWindowNodeId: ContextCompactionWindowNodeId | null;
  usageRefreshIntervalSeconds: number;
  // providerPriorities stores sparse provider ordering values; missing entries inherit zero.
  // providerPriorities 保存稀疏供应商排序值；缺失项继承零。
  providerPriorities: Record<string, number>;
  usageCache: Record<string, PublicAiAccountUsage>;
  providers: PublicAiProviderDescriptor[];
  connections: PublicAiConnection[];
  providerDefaults: Record<string, string>;
  active?: PublicAiActiveSelection;
  catalogs: Record<string, PublicAiConnectionCatalog>;
  sourceCatalogs: Record<string, PublicAiConnectionCatalog>;
  modelSupportCatalogs: Record<string, PublicAiConnectionCatalog>;
  // modelRouteRecords prevents browser-side model protocol resolution.
  // modelRouteRecords 防止浏览器侧自行解析模型协议。
  modelRouteRecords?: PublicAiModelRouteRecord[];
  catalogStatuses: Record<string, PublicAiCatalogDiscoveryStatus>;
  catalogErrors: Record<string, string>;
  modelOverrides: Record<string, PublicAiModelOverrides>;
  modelOverrideRecords: PublicAiModelOverrideRecord[];
  modelAccessRecords: PublicAiModelAccessRecord[];
  // providerToolTemplateModels is the enablement-independent signed media/tool inventory.
  // providerToolTemplateModels 是独立于启用状态的签名媒体/工具目录。
  providerToolTemplateModels: PublicAiProviderToolTemplateModel[];
  customModels?: Record<string, Record<string, PublicAiCustomModelDefinition>>;
  // customProviderManifestFailures exposes bounded startup-isolation diagnostics without source bytes.
  // customProviderManifestFailures 暴露有界启动隔离诊断，不包含源字节。
  customProviderManifestFailures?: Array<{
    providerId: string;
    message: string;
  }>;
  // customProviderManifestFailureTotalCount preserves the true count before list truncation.
  // customProviderManifestFailureTotalCount 保留列表截断前的真实数量。
  customProviderManifestFailureTotalCount?: number;
  // customProviderManifestFailuresTruncated reports omitted isolation diagnostics.
  // customProviderManifestFailuresTruncated 报告被省略的隔离诊断。
  customProviderManifestFailuresTruncated?: boolean;
  customProviderModelDefaults?: Record<string, PublicAiCustomProviderModelDefaults>;
  // customProviderSystemModelDefaults is the backend-owned baseline shown before any provider override.
  // customProviderSystemModelDefaults 是尚无供应商覆盖时展示的后端持有基线。
  customProviderSystemModelDefaults: PublicAiCustomProviderModelDefaults;
  // providersWithModelCustomizations identifies providers whose removal warning must include owned settings.
  // providersWithModelCustomizations 标识移除提示必须包含专属设置的供应商。
  providersWithModelCustomizations?: string[];
  // customModelProtocolPreferenceRecords stores persisted per-model protocol choices explicitly.
  // customModelProtocolPreferenceRecords 以显式记录保存持久化逐模型协议选择。
  customModelProtocolPreferenceRecords?: PublicAiCustomModelProtocolPreferenceRecord[];
  // providerManifestRevisions exposes process-local system manifest revisions for UI markers.
  // providerManifestRevisions 暴露进程内系统清单修订号供 UI 标记使用。
  providerManifestRevisions?: Record<string, number>;
  customModelTemplateDecisionRecords?: PublicAiCustomModelTemplateDecisionRecord[];
  customModelConnectionAvailabilityRecords?: PublicAiCustomModelConnectionAvailabilityRecord[];
  connectionModelAccessPolicyRecords: PublicAiConnectionModelAccessPolicyRecord[];
  modelCapabilityOverrides?: Record<string, PublicAiModelCapabilityOverrides>;
  modelCapabilityOverrideRecords?: PublicAiModelCapabilityOverrideRecord[];
  // providerCapabilityOverrides stores explicit supplier-level layers by provider id.
  // providerCapabilityOverrides 按供应商标识保存显式供应商级能力层。
  providerCapabilityOverrides?: Record<string, PublicAiModelCapabilityOverrides>;
  providerToolTargets: PublicAiProviderToolTargetSnapshot[];
  providerToolSelections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >;
  multimodalUnderstandingCandidateModels?: PublicAiMultimodalUnderstandingCandidateRecord[];
  mediaUnderstandingSelections?: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiMediaUnderstandingBackendSelection>
  >;
  // Exact globally configured routes for both media-understanding backend families.
  // 两种媒体理解后端类型的全局精确配置路由。
  mediaUnderstandingBackendOptions?: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiMediaUnderstandingBackendOptions>
  >;
  mediaUnderstandingModelCandidates?: PublicAiMediaUnderstandingModelCandidate[];
  providerToolEnabledCapabilities: PublicAiProviderToolCapabilityKind[];
  providerExtensionTargetSets: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection[]>
  >;
  internalInferenceCandidates: PublicAiInternalInferenceCandidate[];
  inferenceProfiles: Record<string, PublicAiInferenceProfile>;
  inferenceRouteDiagnostics: PublicAiInferenceRouteDiagnostic[];
  internalModelOverrides: Record<string, PublicAiInternalModelOverrides>;
}

// PublicAiInferenceRouteDiagnostic explains one retained route that cannot execute now.
// PublicAiInferenceRouteDiagnostic 解释一条当前无法执行但仍保留的路由。
export interface PublicAiInferenceRouteDiagnostic {
  consumerProfileId: string;
  purposeId?: string;
  operation: PublicAiOperation;
  route: PublicAiInferenceRoute;
  reason: string;
  // reasonMessage is the catalog form of reason, rendered in the active language when present.
  // reasonMessage 是 reason 的目录形式，存在时按当前语言渲染。
  reasonMessage?: BrowserServiceLocalizedMessage | null;
}

// PublicAiInferenceTestResult carries compact validated facts from a real provider test.
// PublicAiInferenceTestResult 承载一次真实供应商测试的紧凑已校验事实。
export interface PublicAiInferenceTestResult {
  operation: PublicAiOperation;
  providerId: string;
  serviceEntryId: string;
  connectionId: string;
  modelId: string;
  resultCount: number;
  dimensions?: number;
  durationMs: number;
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
  // Provider-reported whole-second period used to localize quota window labels.
  // 供应商报告的整秒周期，用于本地化额度窗口名称。
  duration_seconds?: number;
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
  secret: string;
  managementKey?: string;
  serviceOriginOverride?: string;
}

// PublicAiConnectionUpdateRequest describes one credential edit.
// PublicAiConnectionUpdateRequest 描述一次凭据编辑。
export interface PublicAiConnectionUpdateRequest {
  connectionId: string;
  displayName: string;
  serviceEntryId: string;
  credentialProfileId: string;
  credentialMode: CredentialMode;
  secret?: string;
  managementKey?: string;
  removeManagementKey?: boolean;
  serviceOriginOverride?: string;
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
  authorizationBinding: string;
  phase:
    | "waiting_for_user_authorization"
    | "waiting_for_callback"
    | "finalizing_credential"
    | "succeeded"
    | "failed"
    | "cancelled";
  expiresAtUnixMs: number;
  message?: string;
}

// ProviderTaskPhase mirrors the durable lifecycle phase of one provider asynchronous task.
// ProviderTaskPhase 镜像单个供应商异步任务的持久生命周期阶段。
export type ProviderTaskPhase =
  | "dispatching"
  | "queued"
  | "running"
  | "cancellation_requested"
  | "completed"
  | "failed"
  | "cancelled"
  | "dispatch_outcome_unknown";

// ProviderTaskSummary is the non-secret status of one retained provider asynchronous task.
// ProviderTaskSummary 是单个保留供应商异步任务的非秘密状态。
export interface ProviderTaskSummary {
  // Stable task identity used by status and cancellation calls.
  // 状态查询与取消调用使用的稳定任务标识。
  executionId: string;
  ownerSessionId: string | null;
  capability: PublicAiProviderToolCapabilityKind | null;
  targetDisplayName: string | null;
  providerId: string;
  connectionId: string;
  targetId: string;
  phase: ProviderTaskPhase;
  // Whether the phase can no longer change.
  // 该阶段是否已不会再变化。
  terminal: boolean;
  cancellationSupported: boolean;
  createdAtUnixMs: number;
  updatedAtUnixMs: number;
  deadlineUnixMs: number;
  consecutiveObservationFailures: number;
  error: string | null;
  // Whether the service process still holds the completed provider result.
  // 服务进程是否仍持有已完成的供应商结果。
  resultAvailable: boolean;
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
  // Existing connection replaced by this authorization.
  // 由此次授权替换的现有连接。
  connectionId?: string;
}

// ManifestActiveLayer identifies which layer the runtime currently binds for one provider.
// ManifestActiveLayer 标识一个供应商运行时当前绑定的层。
export type ManifestActiveLayer = "bundled" | "active" | "custom" | "previous";

// ManifestProvenanceStatus is the closed verification state declared by one manifest.
// ManifestProvenanceStatus 是一份 Manifest 声明的封闭验证状态。
export type ManifestProvenanceStatus = "example_only" | "verified" | "migrated";

// ManifestLayerStatus exposes one immutable layer revision and content hash.
// ManifestLayerStatus 暴露一个不可变层的修订与内容摘要。
export interface ManifestLayerStatus {
  revision: number;
  sha256: string;
}

// ManifestModelBindingChangeBaselineStatus identifies the authenticated layer used for active binding deltas.
// ManifestModelBindingChangeBaselineStatus 标识 active 绑定差异使用的已认证层。
export interface ManifestModelBindingChangeBaselineStatus {
  layer: ManifestActiveLayer;
  revision: number;
  sha256: string;
}

// ManifestActiveStatus exposes the active layer identity currently bound in the runtime.
// ManifestActiveStatus 暴露运行时当前绑定的 active 层身份。
export interface ManifestActiveStatus {
  revision: number;
  layer: ManifestActiveLayer;
  sha256?: string;
  minimum_manifest_engine: number;
  provenance_status: ManifestProvenanceStatus;
  evidence_source_count: number;
}

// ManifestSyncStateView exposes bounded sync bookkeeping for one provider.
// ManifestSyncStateView 暴露一个供应商的有界同步簿记。
export interface ManifestSyncStateView {
  revision: number;
  sha256: string;
  etag?: string;
  index_etag?: string;
  last_modified?: string;
  synced_at_unix_ms: number;
  last_error_kind?: string;
}

// ManifestCheckOutcome is the closed result of one configured remote check.
// ManifestCheckOutcome 是一次已配置远端检查的封闭结果。
export type ManifestCheckOutcome = "not_pinned" | "up_to_date" | "activated" | "rejected" | "failed";

// ManifestCheckStateView separates the latest check from the latest full activation.
// ManifestCheckStateView 区分最近检查与最近完整激活。
export interface ManifestCheckStateView {
  checked_at_unix_ms: number;
  last_successful_at_unix_ms?: number;
  outcome: ManifestCheckOutcome;
  last_error_kind?: string;
}

// ProviderOperationalTelemetryEventName is the closed lifecycle vocabulary for non-secret provider diagnostics.
// ProviderOperationalTelemetryEventName 是非秘密供应商诊断使用的封闭生命周期词汇。
export type ProviderOperationalTelemetryEventName =
  | "manifest_index_check_started"
  | "manifest_index_check_not_modified"
  | "manifest_index_check_failed"
  | "manifest_index_check_completed"
  | "provider_candidate_downloaded"
  | "provider_candidate_rejected"
  | "provider_candidate_staged"
  | "provider_manifest_activated"
  | "provider_manifest_post_activation_rolled_back"
  | "provider_manifest_fallback_selected"
  | "provider_manifest_isolated"
  | "provider_upload_started"
  | "provider_upload_completed"
  | "provider_upload_failed"
  | "provider_upload_cancelled"
  | "provider_async_task_state_changed";

// ProviderOperationalProfileRevision is one immutable profile identity without payload bodies.
// ProviderOperationalProfileRevision 是一条不含载荷正文的不可变 Profile 身份。
export interface ProviderOperationalProfileRevision {
  profile_id: string;
  revision: number;
}

// ProviderOperationalTelemetryEvent is one bounded, non-secret structured lifecycle event.
// ProviderOperationalTelemetryEvent 是一条有界、非秘密的结构化生命周期事件。
export interface ProviderOperationalTelemetryEvent {
  schema_version: number;
  event_id: string;
  event_name: ProviderOperationalTelemetryEventName;
  occurred_at_unix_ms: number;
  provider_id?: string;
  service_entry_id?: string;
  connection_id?: string;
  model_id?: string;
  protocol_profile_id?: string;
  route_revision?: string;
  manifest_revision?: number;
  manifest_generation?: number;
  manifest_sha256?: string;
  manifest_source?: string;
  binding_source?: string;
  binding_rule_id?: string;
  request_profiles?: ProviderOperationalProfileRevision[];
  response_profiles?: ProviderOperationalProfileRevision[];
  operation_id?: string;
  upload_channel_id?: string;
  tool_target_id?: string;
  task_execution_id?: string;
  task_phase?: string;
  failure_kind?: string;
  failure_path?: string;
}

// QuarantineSummary exposes one classified quarantined candidate.
// QuarantineSummary 暴露一条分类隔离候选。
export interface QuarantineSummary {
  revision: number;
  failure_kind: string;
  quarantined_at_unix_ms: number;
}

// ManifestFieldSourceKind identifies the exact inheritance layer that produced one value.
// ManifestFieldSourceKind 标识产生一个值的精确继承层。
export type ManifestFieldSourceKind =
  | "provider_defaults"
  | "service_entry_override"
  | "model_rule_exact"
  | "model_rule_prefix";

// ManifestFieldOrigin preserves one lower-precedence value replaced by the winner.
// ManifestFieldOrigin 保留一个被最终胜者替换的低优先级值来源。
export interface ManifestFieldOrigin {
  source: ManifestFieldSourceKind;
  rule_id?: string;
  manifest_revision: number;
  evidence_revision?: string;
}

// ManifestFieldProvenance explains the winner and every deterministic overridden origin.
// ManifestFieldProvenance 解释最终胜者及每个确定性被覆盖来源。
export interface ManifestFieldProvenance extends ManifestFieldOrigin {
  overridden_sources?: ManifestFieldOrigin[];
}

// ManifestRuleProvenance exposes one rule's exact evidence reference and verification state.
// ManifestRuleProvenance 暴露一条规则的精确证据引用与验证状态。
export interface ManifestRuleProvenance {
  source: "model_rule_exact" | "model_rule_prefix";
  rule_id: string;
  manifest_revision: number;
  provenance_ref?: string;
  provenance_status: ManifestProvenanceStatus;
}

// ManifestModelRuleStatus exposes one effective model rule without secrets.
// ManifestModelRuleStatus 公开一条不含秘密的有效模型规则。
export interface ManifestModelRuleStatus {
  rule_id: string;
  service_entry_id: string;
  credential_profile_ids: string[];
  matcher: { kind: "exact" | "prefix"; value: string };
  supported_protocols: string[];
  selected_protocol?: string;
  default_enabled: boolean;
  model_parameter_ids: string[];
  usage_profile_refs: string[];
  request_profile_refs: string[];
  response_profile_refs: string[];
  operation_refs: string[];
  input_resource_constraint_count: number;
  field_provenance: Record<string, ManifestFieldProvenance>;
  capability_provenance: Record<string, ManifestFieldProvenance>;
  provenance: ManifestRuleProvenance;
}

// ManifestModelSourceStatus is the closed backend-owned model inventory source.
// ManifestModelSourceStatus 是由后端持有的封闭模型目录来源。
export type ManifestModelSourceStatus = "manifest" | "discovery" | "manual" | "merged";

// ManifestModelBindingStatus is one concrete backend-resolved connection/model/profile binding.
// ManifestModelBindingStatus 是一条具体的后端解析连接/模型/配置档绑定。
export interface ManifestModelBindingStatus {
  connection_id?: string;
  service_entry_id: string;
  credential_profile_id?: string;
  model_id: string;
  display_name: string;
  model_source: ManifestModelSourceStatus;
  model_source_layers: ManifestModelSourceStatus[];
  supported_protocols: string[];
  available_protocols: string[];
  effective_protocol?: string;
  binding_source?: string;
  binding_rule_id?: string;
  manifest_revision: number;
  operation_refs: string[];
  capabilities?: PublicAiModelCapabilities;
  capability_evidence?: PublicAiModelCapabilityEvidence;
  capability_provenance: Record<string, ManifestFieldProvenance>;
  catalog_observed: boolean;
  catalog_enabled: boolean;
  executable: boolean;
  unavailable_reason?: string;
}

// ManifestOperationCarrier is the closed execution carrier selected by a manifest operation.
// ManifestOperationCarrier 是 Manifest operation 选择的封闭执行载体。
export type ManifestOperationCarrier = "conversation_protocol" | "provider_tool" | "native_action" | "unsupported";

// ManifestOperationMatchingModelStatus identifies one concrete model matched by an operation.
// ManifestOperationMatchingModelStatus 标识一条 operation 匹配的具体模型。
export interface ManifestOperationMatchingModelStatus {
  service_entry_id: string;
  model_id: string;
}

// ManifestOperationStatus exposes one operation's closed profile, upload, and tool bindings.
// ManifestOperationStatus 公开一个 operation 的封闭 Profile、上传与工具绑定。
export interface ManifestOperationStatus {
  operation_id: string;
  operation: string;
  carrier: ManifestOperationCarrier;
  input_resources: PublicAiResourceKind[];
  output_resources: PublicAiResourceKind[];
  input_mime_types: string[];
  output_mime_types: string[];
  request_profile_refs: string[];
  response_profile_refs: string[];
  upload_channel_refs: string[];
  tool_target_refs: string[];
  tool_target_template_refs: string[];
  asynchronous: boolean;
  poll_interval_ms?: number;
  max_backoff_ms?: number;
  cancellation_supported: boolean;
  timeout_ms?: number;
  matching_models: ManifestOperationMatchingModelStatus[];
  matching_model_total_count: number;
  matching_models_truncated: boolean;
}

// ManifestToolTargetTemplateInstanceStatus exposes one data-only model binding instantiated from immutable authority.
// ManifestToolTargetTemplateInstanceStatus 公开一条从不可变权限实例化的纯数据模型绑定。
export interface ManifestToolTargetTemplateInstanceStatus {
  target_id: string;
  template_id: string;
  service_entry_id: string;
  model_id: string;
  model_display_name: string;
}

// ManifestDeclarationProvenance identifies one active executable declaration's source.
// ManifestDeclarationProvenance 标识一项生效可执行声明的来源。
export interface ManifestDeclarationProvenance {
  provider_id: string;
  declaration_kind: string;
  declaration_id: string;
  source_layer: ManifestActiveLayer;
  manifest_revision: number;
  sha256?: string;
}

// ProviderManifestCriticalErrorStatus is one bounded provider-local operator diagnostic.
// ProviderManifestCriticalErrorStatus 是一条有界的逐供应商运维诊断。
export interface ProviderManifestCriticalErrorStatus {
  failure_kind: string;
  message: string;
}

// ProviderManifestStatus is the typed per-provider manifest status snapshot (plan §R10).
// ProviderManifestStatus 是 typed 逐供应商清单状态快照（方案 §R10）。
export interface ProviderManifestStatus {
  provider_id: string;
  status_error?: ProviderManifestCriticalErrorStatus;
  critical_error?: ProviderManifestCriticalErrorStatus;
  bundled_failure?: ProviderManifestCriticalErrorStatus;
  bundled?: ManifestLayerStatus;
  active?: ManifestActiveStatus;
  previous?: ManifestLayerStatus;
  sync_state?: ManifestSyncStateView;
  sync_check_state?: ManifestCheckStateView;
  recent_telemetry_events: ProviderOperationalTelemetryEvent[];
  recent_telemetry_total_count: number;
  recent_telemetry_events_truncated: boolean;
  quarantine: QuarantineSummary[];
  sync_configured: boolean;
  model_rules: ManifestModelRuleStatus[];
  model_bindings: ManifestModelBindingStatus[];
  model_binding_total_count: number;
  model_bindings_truncated: boolean;
  active_model_binding_change_baseline?: ManifestModelBindingChangeBaselineStatus;
  active_model_binding_changes: ProviderManifestModelBindingDelta[];
  active_model_binding_change_total_count: number;
  active_model_binding_changes_truncated: boolean;
  operations: ManifestOperationStatus[];
  tool_target_template_instances: ManifestToolTargetTemplateInstanceStatus[];
  declaration_provenance: Record<string, ManifestDeclarationProvenance>;
}

// ProviderManifestPreviewOutcome is the closed result of one read-only candidate preview.
// ProviderManifestPreviewOutcome 是一次只读候选预览的封闭结果。
export type ProviderManifestPreviewOutcome = "not_configured" | "not_pinned" | "up_to_date" | "candidate" | "rejected";

// ProviderManifestChangeKind classifies one semantic candidate delta.
// ProviderManifestChangeKind 分类一条语义候选差异。
export type ProviderManifestChangeKind = "added" | "removed" | "changed";

// ProviderManifestModelBindingDelta compares one exact model/profile binding.
// ProviderManifestModelBindingDelta 对比一条精确模型/配置档绑定。
export interface ProviderManifestModelBindingDelta {
  change: ProviderManifestChangeKind;
  current?: ManifestModelBindingStatus;
  candidate?: ManifestModelBindingStatus;
}

// ProviderManifestOperationDelta compares one semantic operation contract.
// ProviderManifestOperationDelta 对比一份语义 operation 契约。
export interface ProviderManifestOperationDelta {
  change: ProviderManifestChangeKind;
  current?: ManifestOperationStatus;
  candidate?: ManifestOperationStatus;
}

// ProviderManifestCandidatePreview is a verified read-only semantic diff with bounded details.
// ProviderManifestCandidatePreview 是带有界详情的已验证只读语义差异。
export interface ProviderManifestCandidatePreview {
  provider_id: string;
  outcome: ProviderManifestPreviewOutcome;
  current_revision?: number;
  candidate_revision?: number;
  candidate_sha256?: string;
  executable: boolean;
  failure_kind?: string;
  failure_message?: string;
  model_binding_changes: ProviderManifestModelBindingDelta[];
  model_binding_change_total_count: number;
  model_binding_changes_truncated: boolean;
  operation_changes: ProviderManifestOperationDelta[];
  operation_change_total_count: number;
  operation_changes_truncated: boolean;
}

// ProviderSyncReport is the typed outcome of one provider manifest sync (plan §R10).
// ProviderSyncReport 是一次供应商清单同步的 typed 结果（方案 §R10）。
export interface ProviderSyncReport {
  provider_id: string;
  outcome: "not_configured" | "up_to_date" | "activated" | "rejected" | "not_pinned";
  revision: number;
  failure_kind?: string;
  // failure_message retains one bounded and redacted provider-scoped public diagnostic.
  // failure_message 保留一条有界脱敏的逐供应商公开诊断。
  failure_message?: string;
}

// ProviderRollbackReport is the typed outcome of one explicit local rollback (plan §R10).
// ProviderRollbackReport 是一次显式本地回滚的 typed 结果（方案 §R10）。
export interface ProviderRollbackReport {
  provider_id: string;
  restored_revision: number;
}
