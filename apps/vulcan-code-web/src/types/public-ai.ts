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
  account_usage_query:
    | "unsupported"
    | "mini_max_token_plan"
    | "open_router_credits"
    | "tavily_credits"
    | "deep_seek_balance"
    | "kimi_coding_plan";
  authentication?: string;
  key_prefix?: string;
}

// PublicAiProtocolRoute describes one verified wire route.
// PublicAiProtocolRoute 描述一条已验证 Wire 路由。
export interface PublicAiProtocolRoute {
  protocol_profile_id: string;
  endpoint_path?: string;
  model_discovery?: PublicAiModelDiscoveryKind;
  model_discovery_path?: string;
  responses_features?: {
    websocket: PublicAiResponsesWebSocketPolicy;
    remote_compaction: PublicAiResponsesRemoteCompactionPolicy;
  };
  verified: boolean;
}

// PublicAiModelDiscoveryKind identifies one registered catalog acquisition contract.
// PublicAiModelDiscoveryKind 标识一种已注册的目录获取契约。
export type PublicAiModelDiscoveryKind =
  | "manual"
  | "provider_tools_only"
  | "mini_max_anthropic"
  | "mini_max_open_ai"
  | "open_router"
  | "alibaba_coding_plan_static"
  | "alibaba_token_plan_static"
  | "alibaba_model_studio_static"
  | "open_ai"
  | "open_ai_codex"
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
  model_discovery: PublicAiModelDiscoveryKind;
  protocol_routes: PublicAiProtocolRoute[];
}

// PublicAiProviderCategory identifies the provider's user-facing business relationship.
// PublicAiProviderCategory 标识供应商面向用户的业务关系类别。
export type PublicAiProviderCategory = "unclassified" | "official" | "cloud_maas" | "relay";

// PublicAiCredentialDirectoryVisibility controls zero-credential onboarding visibility.
// PublicAiCredentialDirectoryVisibility 控制零凭据引导入口的可见性。
export type PublicAiCredentialDirectoryVisibility = "configured_only" | "pinned_onboarding";

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
  promotion?: PublicAiProviderPromotion;
  service_entries: PublicAiServiceEntry[];
}

// PublicAiCustomProviderProtocol identifies one user-selected compatible wire protocol.
// PublicAiCustomProviderProtocol 标识用户选择的一种兼容 Wire 协议。
export type PublicAiCustomProviderProtocol =
  | "anthropic_messages"
  | "open_ai_chat"
  | "open_ai_responses"
  | "open_ai_codex_responses";
// PublicAiResponsesWebSocketPolicy selects the explicit Codex Responses transport preference.
// PublicAiResponsesWebSocketPolicy 选择显式的 Codex Responses 传输偏好。
export type PublicAiResponsesWebSocketPolicy =
  | "disabled"
  | "preferred_with_http_fallback";
// PublicAiResponsesRemoteCompactionPolicy selects the explicit standalone compact behavior.
// PublicAiResponsesRemoteCompactionPolicy 选择显式的独立远端压缩行为。
export type PublicAiResponsesRemoteCompactionPolicy =
  | "disabled"
  | "enabled_with_local_fallback";

// PublicAiCustomProviderModelDiscoveryMode selects automatic, custom-path, or manual discovery.
// PublicAiCustomProviderModelDiscoveryMode 选择自动、自定义路径或手工发现。
export type PublicAiCustomProviderModelDiscoveryMode = "standard" | "custom_path" | "manual";

// PublicAiCustomProviderSaveRequest creates or replaces one user-defined provider.
// PublicAiCustomProviderSaveRequest 创建或替换一个用户自定义供应商。
export interface PublicAiCustomProviderSaveRequest {
  providerId?: string;
  displayName: string;
  baseUrl: string;
  protocol: PublicAiCustomProviderProtocol;
  websocketPolicy: PublicAiResponsesWebSocketPolicy;
  remoteCompactionPolicy: PublicAiResponsesRemoteCompactionPolicy;
  authentication: "bearer_authorization" | "x_api_key";
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

// PublicAiCustomModelDefinition is one credential-independent user model overlay.
// PublicAiCustomModelDefinition 是一个与凭据无关的用户模型覆盖定义。
export interface PublicAiCustomModelDefinition {
  id: string;
  display_name: string;
  enabled: boolean;
  capabilities: PublicAiModelCapabilities;
  request_projection: PublicAiCustomModelRequestProjection;
}

// PublicAiModelCapabilityOverrides stores only user differences from immutable capability data.
// PublicAiModelCapabilityOverrides 仅保存用户相对于不可变能力数据的差异。
export interface PublicAiModelCapabilityOverrides {
  context_limit?: number;
  output_limit?: number;
  input_resources?: PublicAiResourceKind[];
  reasoning?: string;
  reasoning_control?: string;
  reasoning_efforts?: string[];
  tool_calling?: string;
  // Optional sparse Codex context-window policy for this provider or model layer.
  // 此供应商或模型层的可选稀疏 Codex 上下文窗口策略。
  // Null explicitly removes a previously persisted sparse policy from this layer.
  // null 用于显式移除该层之前持久化的稀疏策略。
  context_window_policy?: PublicAiContextWindowPolicyOverrides | null;
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

// PublicAiCustomProviderModelDefaults is the safe provider-wide fallback capability contract.
// PublicAiCustomProviderModelDefaults 是安全的供应商级回退能力契约。
export interface PublicAiCustomProviderModelDefaults {
  context_limit: number;
  output_limit: number;
  input_resources?: PublicAiResourceKind[];
  reasoning: string;
  reasoning_control: string;
  reasoning_efforts: string[];
  tool_calling: string;
  // Provider-wide sparse Codex context-window defaults.
  // 供应商级稀疏 Codex 上下文窗口默认值。
  context_window_policy?: PublicAiContextWindowPolicyOverrides;
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

// PublicAiConnectionModelAccessPolicyRecord carries one account's inherited and sparse model decisions.
// PublicAiConnectionModelAccessPolicyRecord 承载一个账号的继承状态与稀疏模型决定。
export interface PublicAiConnectionModelAccessPolicyRecord {
  connectionId: string;
  defaultEnabled: boolean;
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
    | "kimi_coding_plan";
  has_management_key: boolean;
  service_origin_override?: string;
  credential_mode: CredentialMode;
  credential_id: string;
  display_name: string;
  verified_protocols: string[];
  selected_protocol: string;
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
  supports_text_input?: boolean;
  supports_text_output?: boolean;
}

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
  // Evidence revision proving the exact capability projection.
  // 证明精确能力投影的证据修订号。
  evidence_revision: string;
  reasoning: string;
  reasoning_control?: string;
  reasoning_efforts?: string[];
  tool_calling: string;
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

// PublicAiActiveSelection identifies one exact model and account.
// PublicAiActiveSelection 标识一个精确模型与账号。
export interface PublicAiActiveSelection {
  provider_id: string;
  model_id: string;
  connection_id: string;
}

// PublicAiModelOverrides stores local limit overrides and registered parameter values.
// PublicAiModelOverrides 保存本地限制覆盖与注册参数值。
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
  customized?: boolean;
}

// PublicAiModelCapabilityOverrideRecord identifies one capability difference without key parsing.
// PublicAiModelCapabilityOverrideRecord 无需解析键即可标识一项能力差异。
export interface PublicAiModelCapabilityOverrideRecord {
  providerId: string;
  modelId: string;
  overrides: PublicAiModelCapabilityOverrides;
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
  | "video_edit"
  | "speech_synthesize"
  | "speech_transcribe"
  | "music_generate"
  | "music_cover";

// PublicAiOperation identifies one provider-neutral AI operation.
// PublicAiOperation 标识一项供应商无关 AI 操作。
export type PublicAiOperation =
  | "conversation_respond"
  | "embedding_create"
  | "rerank_documents";

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
  llm_routes?: Record<string, PublicAiInferenceRoute>;
  embedding?: PublicAiInferenceRoute;
  rerank?: PublicAiInferenceRoute;
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
}

// PublicAiProviderToolTargetSelection stores the one selected target for one capability.
// PublicAiProviderToolTargetSelection 保存某项能力唯一选中的目标。
export interface PublicAiProviderToolTargetSelection {
  connection_id: string;
  target_id: string;
}

// PublicAiSessionProviderToolConflict describes one retained choice that requires confirmation.
// PublicAiSessionProviderToolConflict 描述一项需要确认的已保留选择。
export interface PublicAiSessionProviderToolConflict {
  capability: PublicAiProviderToolCapabilityKind;
  kind: "missing_selection" | "target_unavailable";
  message: string;
}

// PublicAiSessionProviderToolSettings owns one complete session capability snapshot.
// PublicAiSessionProviderToolSettings 持有一份完整会话能力快照。
export interface PublicAiSessionProviderToolSettings {
  enabledCapabilities: PublicAiProviderToolCapabilityKind[];
  selections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
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

// PublicAiSettingsSnapshot combines persisted and process-local settings state.
// PublicAiSettingsSnapshot 组合持久化设置状态与进程内设置状态。
export interface PublicAiSettingsSnapshot {
  // contextWindowPolicyOverrides is the system-level sparse Codex window policy.
  // contextWindowPolicyOverrides 是系统级稀疏 Codex 窗口策略。
  contextWindowPolicyOverrides: PublicAiContextWindowPolicyOverrides;
  // providerContextWindowPolicyOverrides stores explicit provider-level window policies.
  // providerContextWindowPolicyOverrides 保存显式供应商级窗口策略。
  providerContextWindowPolicyOverrides?: Record<string, PublicAiContextWindowPolicyOverrides>;
  usageRefreshIntervalSeconds: number;
  usageCache: Record<string, PublicAiAccountUsage>;
  providers: PublicAiProviderDescriptor[];
  connections: PublicAiConnection[];
  providerDefaults: Record<string, string>;
  active?: PublicAiActiveSelection;
  catalogs: Record<string, PublicAiConnectionCatalog>;
  sourceCatalogs: Record<string, PublicAiConnectionCatalog>;
  modelSupportCatalogs: Record<string, PublicAiConnectionCatalog>;
  catalogStatuses: Record<string, PublicAiCatalogDiscoveryStatus>;
  catalogErrors: Record<string, string>;
  modelOverrides: Record<string, PublicAiModelOverrides>;
  modelOverrideRecords: PublicAiModelOverrideRecord[];
  modelAccessRecords: PublicAiModelAccessRecord[];
  customModels?: Record<string, Record<string, PublicAiCustomModelDefinition>>;
  customProviderModelDefaults?: Record<string, PublicAiCustomProviderModelDefaults>;
  // customProviderSystemModelDefaults is the backend-owned baseline shown before any provider override.
  // customProviderSystemModelDefaults 是尚无供应商覆盖时展示的后端持有基线。
  customProviderSystemModelDefaults: PublicAiCustomProviderModelDefaults;
  // providersWithModelCustomizations identifies providers whose removal warning must include owned settings.
  // providersWithModelCustomizations 标识移除提示必须包含专属设置的供应商。
  providersWithModelCustomizations?: string[];
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
  providerToolEnabledCapabilities: PublicAiProviderToolCapabilityKind[];
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
  selectedProtocol: string;
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
