//! Public AI connection settings and runtime metadata services.
//! 公共 AI 连接设置与运行时元数据服务。

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use vulcan_ai_contract::{
    AccountUsageAvailability, AccountUsageSnapshot, AiExecutionRequest, AiMessage, AiOperation,
    ContentItem, EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent, EmbeddingOutputKind,
    EmbeddingPayload, EmbeddingRequest, EmbeddingTask, INFERENCE_CONTRACT_VERSION,
    InferenceCallerBudget, InferenceCallerContext, InferencePermission, InferenceUsageScope,
    MessageRole, ModelCapabilities, ParallelToolCallPolicy, PromptCachePolicy,
    ReasoningControlMode, ReasoningEffort, ReasoningOutputPolicy, RerankRequest,
    RerankScoreSemantics, RerankText, RerankTruncation, ResourceKind, StructuredOutputCapability,
    StructuredOutputPolicy, ToolChoicePolicy,
};
use vulcan_context::{
    ContextWindowPolicyOverrides, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
};
use vulcan_core::types::ConversationAttachmentKind;
use vulcan_provider_adapters::{
    AccountUsageQueryKind, ActiveAiModelSelection, AvailableProviderToolTarget,
    CatalogDiscoveryState, CatalogDiscoveryStatus, CommonModelTemplateSuggestion,
    ConnectionModelAccessPolicy, ConnectionModelCatalog, ContextCompactionWindowConfiguration,
    CredentialBillingKind, CredentialConnection, CredentialMode, CredentialSecretEnvelope,
    CredentialSecretResolver, CredentialSecretStore, CustomModelDefinition,
    CustomModelProtocolPreference, CustomModelTemplateDecision, CustomProtocolRoute,
    CustomProviderAuthentication, CustomProviderDefinition, CustomProviderManifestFailure,
    CustomProviderModelDefaults, CustomProviderModelDiscoveryMode, CustomProviderProtocol,
    DeclaredProviderToolTemplateModel, InferenceProfile, InferenceRouteBinding,
    InferenceRouteConfiguration, InternalModelOverrides, ManifestSourceLayer,
    MediaUnderstandingBackendOptions, MediaUnderstandingBackendSelection, ModelCapabilityOverrides,
    ModelDiscoveryKind, ModelExecutionOverrides, OAuthAuthorizationChallenge,
    OAuthClientConfiguration, OAuthTransactionPhase, OAuthTransactionSnapshot,
    PersistedAiConfiguration, ProviderDescriptor, ProviderKind, ProviderRegistry,
    ProviderToolCapabilityKind, ProviderToolExecutionResult, ProviderToolTarget,
    ProviderToolTargetSelection, PublicAiRuntime, PublicAiRuntimeSettingsCatalogSnapshot,
    ResponsesRemoteCompactionPolicy, ResponsesRouteFeatures, ResponsesWebsocketPolicy,
    ServiceEntry, ServiceEntryKind, StagedConnectionCatalogObservation,
    credential_authorization_binding, registered_native_action_binding_id,
    validate_custom_provider_identifier,
};
use vulcan_service_api::{
    ContextCompactionWindowNodeSummary, ServiceEventEmitter, WorkbenchSnapshot,
};
use vulcan_workbench_model::SessionPendingModelSelection;

use crate::{
    SharedServiceBridgeState, begin_public_ai_configuration_transaction,
    configured_inference::{VMM_CONSUMER_PROFILE_ID, model_supports_disabled_reasoning},
    emit_workbench_event, mutate_public_ai_configuration_in_transaction,
    public_ai_provider_rows_for_selection, public_ai_workbench_selection,
    public_ai_workbench_selection_after_route_revocation, shell_host_from_state,
};

/// Stable LLM purpose key used by the library-owned configured-inference profile.
/// 资料库独立配置推理档案使用的稳定 LLM 用途键。
pub(super) const KNOWLEDGE_CURATION_PURPOSE_ID: &str = "knowledge_display_curation";

/// Browser-facing public AI settings snapshot.
/// 面向浏览器的公共 AI 设置快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiSettingsSnapshot {
    /// System-level sparse Codex context-window policy inherited by every route.
    /// 所有路由继承的系统级稀疏 Codex 上下文窗口策略。
    context_window_policy_overrides: ContextWindowPolicyOverrides,
    /// Fixed system context-compaction window nodes in immutable display order.
    /// 按不可变展示顺序排列的固定系统上下文压缩窗口节点。
    context_compaction_window_nodes: ContextCompactionWindowConfiguration,
    /// Explicit provider-level sparse Codex context-window policies.
    /// 显式供应商级稀疏 Codex 上下文窗口策略。
    provider_context_window_policy_overrides: BTreeMap<String, ContextWindowPolicyOverrides>,
    /// Persisted background account-usage refresh interval.
    /// 持久化的后台账号用量刷新间隔。
    usage_refresh_interval_seconds: u64,
    /// Persisted sparse provider priorities; missing providers inherit zero.
    /// 持久化的稀疏供应商优先级；缺失供应商继承零。
    provider_priorities: BTreeMap<String, i32>,
    /// Process-local account-usage observations.
    /// 进程内账号用量观测。
    usage_cache: BTreeMap<String, AccountUsageSnapshot>,
    /// Registered provider and service-entry descriptors.
    /// 已注册的供应商与服务入口描述。
    providers: Vec<ProviderDescriptor>,
    /// Persisted connection metadata without secrets.
    /// 不含秘密的持久化连接元数据。
    connections: Vec<PublicAiConnectionSnapshot>,
    /// Explicit default connection per provider.
    /// 每个供应商的显式默认连接。
    provider_defaults: BTreeMap<String, String>,
    /// Exact active model selection.
    /// 精确的当前模型选择。
    active: Option<ActiveAiModelSelection>,
    /// Process-local connection model catalogs.
    /// 进程内的连接模型目录。
    catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Immutable provider-discovery catalogs before user overlays.
    /// 应用用户覆盖前的不可变供应商发现目录。
    source_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Account support catalogs after membership and capability resolution but before access switches.
    /// 解析成员关系与能力后、应用访问开关前的账号支持目录。
    model_support_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Backend-resolved effective protocol routes for browser model settings.
    /// 后端为浏览器模型设置解析出的生效协议路由。
    model_route_records: Vec<PublicAiModelRouteRecord>,
    /// Closed process-local discovery status per credential connection.
    /// 每个凭据连接的封闭进程内发现状态。
    catalog_statuses: BTreeMap<String, CatalogDiscoveryStatus>,
    /// Latest process-local model synchronization errors.
    /// 最近的进程内模型同步错误。
    catalog_errors: BTreeMap<String, String>,
    /// Provider-model-protocol local overrides.
    /// 供应商、模型与协议作用域的本地覆盖。
    model_overrides: BTreeMap<String, ModelExecutionOverrides>,
    /// Explicit override records that avoid exposing composite-key parsing to the browser.
    /// 避免浏览器解析组合键的显式覆盖记录。
    model_override_records: Vec<PublicAiModelOverrideRecord>,
    /// Conversation publication records derived from runtime catalogs and persisted choices.
    /// 从运行时目录与持久化选择派生的会话发布记录。
    model_access_records: Vec<PublicAiModelAccessRecord>,
    /// Signed provider-tool template models kept visible even while provider-disabled.
    /// 即使在供应商级关闭时仍保持可见的签名供应商工具模板模型。
    provider_tool_template_models: Vec<DeclaredProviderToolTemplateModel>,
    /// User-owned custom models grouped by provider without credential duplication.
    /// 按供应商分组且不复制凭据的用户自有自定义模型。
    custom_models: BTreeMap<String, BTreeMap<String, CustomModelDefinition>>,
    /// Bounded standalone custom-provider failures isolated during startup hydration.
    /// 启动补水期间隔离的有界独立自定义供应商失败。
    custom_provider_manifest_failures: Vec<CustomProviderManifestFailure>,
    /// Total isolated custom-provider count before the snapshot list bound.
    /// 快照列表上限前的隔离自定义供应商总数。
    custom_provider_manifest_failure_total_count: u64,
    /// Whether additional isolated-provider diagnostics were omitted.
    /// 是否省略了额外隔离供应商诊断。
    custom_provider_manifest_failures_truncated: bool,
    /// Capability defaults grouped by custom provider identifier.
    /// 按自定义供应商标识分组的能力默认值。
    custom_provider_model_defaults: BTreeMap<String, CustomProviderModelDefaults>,
    /// Code-owned baseline rendered when a custom provider has no saved provider-level layer.
    /// 自定义供应商没有已保存供应商层时展示的代码持有基线。
    custom_provider_system_model_defaults: CustomProviderModelDefaults,
    /// Custom providers whose removal warning must include backend-owned model or route customizations.
    /// 移除提示必须包含后端模型或路由自定义项的自定义供应商。
    providers_with_model_customizations: BTreeSet<String>,
    /// Persisted per-model protocol preferences as explicit records without composite keys.
    /// 以显式记录形式（无组合键）呈现的持久化逐模型协议偏好。
    custom_model_protocol_preference_records: Vec<PublicAiCustomModelProtocolPreferenceRecord>,
    /// Process-local system manifest revisions keyed by provider for the UI revision marker.
    /// 按供应商索引的进程内系统清单修订号（供 UI 修订标记）。
    provider_manifest_revisions: BTreeMap<String, u64>,
    /// Explicit template decisions without browser-side composite-key parsing.
    /// 不需要浏览器解析组合键的显式模板决策记录。
    custom_model_template_decision_records: Vec<PublicAiCustomModelTemplateDecisionRecord>,
    /// Explicit per-account manual-model availability differences.
    /// 显式逐账号手工模型可用性差异。
    custom_model_connection_availability_records:
        Vec<PublicAiCustomModelConnectionAvailabilityRecord>,
    /// Account-level model access policies and their closed directory semantics.
    /// 账号级模型访问策略及其封闭目录语义。
    connection_model_access_policy_records: Vec<PublicAiConnectionModelAccessPolicyRecord>,
    /// User capability differences stored separately from immutable catalogs.
    /// 与不可变目录分离保存的用户能力差异。
    model_capability_overrides: BTreeMap<String, ModelCapabilityOverrides>,
    /// Provider-level capability differences inherited by every provider model.
    /// 由供应商全部模型继承的供应商级能力差异。
    provider_capability_overrides: BTreeMap<String, ModelCapabilityOverrides>,
    /// Explicit capability difference records that avoid composite-key parsing in the browser.
    /// 避免浏览器解析组合键的显式能力差异记录。
    model_capability_override_records: Vec<PublicAiModelCapabilityOverrideRecord>,
    /// Available exact provider tool targets through configured credentials.
    /// 通过已配置凭据可用的精确供应商工具目标。
    provider_tool_targets: Vec<PublicAiProviderToolTargetSnapshot>,
    /// One explicit selected target per semantic provider tool capability.
    /// 每项语义供应商工具能力唯一显式选择的目标。
    provider_tool_selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    /// Provider models explicitly admitted to image, video, and audio understanding candidates.
    /// 显式允许进入图片、视频与音频理解候选的供应商模型。
    multimodal_understanding_candidate_models: Vec<PublicAiMultimodalUnderstandingCandidateRecord>,
    /// One explicit provider-tool or model-replacement backend per understanding capability.
    /// 每项理解能力唯一显式选择的供应商工具或模型代替后端。
    media_understanding_selections:
        BTreeMap<ProviderToolCapabilityKind, MediaUnderstandingBackendSelection>,
    /// Independently configured provider and model-replacement routes per understanding capability.
    /// 每项理解能力分别配置的供应商专用与模型代替路由。
    media_understanding_backend_options:
        BTreeMap<ProviderToolCapabilityKind, MediaUnderstandingBackendOptions>,
    /// Every currently executable account route eligible for model-replacement understanding.
    /// 当前可执行且符合模型代替理解条件的全部账号路由。
    media_understanding_model_candidates: Vec<PublicAiMediaUnderstandingModelCandidate>,
    /// Provider tool capabilities enabled by default for newly materialized sessions.
    /// 为新物化会话默认启用的供应商工具能力。
    provider_tool_enabled_capabilities: BTreeSet<ProviderToolCapabilityKind>,
    /// Native model-extension targets selected per generative capability.
    /// 按生成类能力选择的原生模型扩展目标集合。
    provider_extension_target_sets:
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
    /// Exact internal inference routes available through current memory catalogs.
    /// 当前内存目录可用的精确内部推理路由。
    internal_inference_candidates: Vec<PublicAiInternalInferenceCandidate>,
    /// Persisted consumer-isolated inference profiles.
    /// 持久化的消费者隔离推理配置档。
    inference_profiles: BTreeMap<String, InferenceProfile>,
    /// Exact failures for persisted routes that the current runtime cannot execute.
    /// 当前运行时无法执行的持久化路由精确失败原因。
    inference_route_diagnostics: Vec<PublicAiInferenceRouteDiagnostic>,
    /// Persisted operation-specific internal model defaults.
    /// 持久化的操作专属内部模型默认值。
    internal_model_overrides: BTreeMap<String, InternalModelOverrides>,
}

/// Browser-facing per-model protocol preference without composite-key parsing.
/// 面向浏览器的逐模型协议偏好记录（无组合键解析）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiCustomModelProtocolPreferenceRecord {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Auto or explicit protocol preference.
    /// 自动或显式协议偏好。
    preference: CustomModelProtocolPreference,
}

/// Browser-facing reviewed template decision for one custom-provider model.
/// 面向浏览器的一个自定义供应商模型已审查模板决策。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiCustomModelTemplateDecisionRecord {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Persisted reviewed decision.
    /// 持久化的已审查决策。
    decision: CustomModelTemplateDecision,
}

/// Browser-facing per-account custom-model availability difference.
/// 面向浏览器的逐账号自定义模型可用性差异。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiCustomModelConnectionAvailabilityRecord {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Explicit availability choice.
    /// 显式可用性选择。
    enabled: bool,
}

/// Identifies how one credential account obtains its model membership.
/// 标识一个凭据账号如何获得其模型成员关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PublicAiModelDirectoryKind {
    /// Models are supplied by a reviewed built-in catalog.
    /// 模型由经过审查的内置目录提供。
    Fixed,
    /// Models are discovered from the exact credential at runtime.
    /// 模型在运行时通过精确凭据发现。
    Dynamic,
    /// Models are declared manually by the user.
    /// 模型由用户手工声明。
    Manual,
    /// The service entry exposes no conversation-model directory.
    /// 服务入口不暴露对话模型目录。
    Unavailable,
}

/// Browser-facing account model access policy with closed directory semantics.
/// 面向浏览器且带封闭目录语义的账号模型访问策略。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiConnectionModelAccessPolicyRecord {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// State inherited by current and future models without an exact override.
    /// 没有精确覆盖时当前与未来模型继承的状态。
    default_enabled: bool,
    /// Sparse exact model decisions that differ from the account default.
    /// 与账号默认值不同的稀疏精确模型决定。
    model_enabled_overrides: BTreeMap<String, bool>,
    /// Closed model-directory kind resolved by the backend registry.
    /// 后端注册表解析出的封闭模型目录类型。
    directory_kind: PublicAiModelDirectoryKind,
}

/// Browser-facing diagnostic for one persisted inference-profile route.
/// 面向浏览器的一条持久化推理配置档路由诊断。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiInferenceRouteDiagnostic {
    /// Stable consumer profile that owns the route.
    /// 持有该路由的稳定消费者配置档。
    consumer_profile_id: String,
    /// Optional stable LLM purpose within the consumer profile.
    /// 消费者配置档内可选的稳定 LLM 用途。
    purpose_id: Option<String>,
    /// Expected operation of the owning profile slot.
    /// 所属配置槽位期望的操作。
    operation: AiOperation,
    /// Exact retained route that failed current runtime resolution.
    /// 当前运行时解析失败但仍保留的精确路由。
    route: InferenceRouteConfiguration,
    /// Exact backend failure produced by strict route validation.
    /// 严格路由校验产生的精确后端失败原因。
    reason: String,
}

/// Browser-facing exact route candidate for one internal inference operation.
/// 面向浏览器的一项内部推理操作精确路由候选。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiInternalInferenceCandidate {
    /// User-visible provider name.
    /// 用户可见供应商名称。
    provider_display_name: String,
    /// User-visible service-entry name.
    /// 用户可见服务入口名称。
    service_entry_display_name: String,
    /// User-visible credential connection name.
    /// 用户可见凭据连接名称。
    connection_display_name: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    model_display_name: String,
    /// Exact immutable route persisted when selected.
    /// 选中时持久化的精确不可变路由。
    pub(super) route: InferenceRouteConfiguration,
    /// Process-local catalog generation that established the model route.
    /// 建立该模型路由的进程内目录代次。
    pub(super) catalog_generation: u64,
    /// Whether the model operation is enabled for internal use.
    /// 当前模型操作是否已为内部使用启用。
    pub(super) enabled: bool,
    /// Exact usage scopes permitted by product, credential, and user narrowing.
    /// 产品、凭据与用户收窄共同允许的精确用途范围。
    usage_scopes: BTreeSet<InferenceUsageScope>,
    /// Exact model capability contract used to generate internal controls.
    /// 用于生成内部控件的精确模型能力契约。
    pub(super) capabilities: ModelCapabilities,
    /// Current operation-scoped model defaults.
    /// 当前操作作用域模型默认值。
    overrides: InternalModelOverrides,
}

/// Browser-facing provider tool target paired with one credential.
/// 面向浏览器且与一个凭据配对的供应商工具目标。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiProviderToolTargetSnapshot {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// User-visible credential name.
    /// 用户可见凭据名称。
    connection_display_name: String,
    /// Frozen manifest source used to enforce native-only model extensions.
    /// 用于强制模型扩展仅限原生清单的冻结清单来源。
    manifest_source_layer: ManifestSourceLayer,
    /// Exact model- or service-native target contract.
    /// 精确的模型或服务原生目标契约。
    target: ProviderToolTarget,
}

/// Browser-facing provider-model admission record for multimodal understanding.
/// 面向浏览器的供应商模型多模态理解准入记录。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiMultimodalUnderstandingCandidateRecord {
    /// Exact provider identifier shared by all credential routes for the model.
    /// 该模型全部凭据路由共享的精确供应商标识。
    provider_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
}

/// Browser-facing exact account route eligible for model-replacement media understanding.
/// 面向浏览器且可用于模型代替媒体理解的精确账号路由。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiMediaUnderstandingModelCandidate {
    /// User-visible provider name.
    /// 用户可见供应商名称。
    provider_display_name: String,
    /// User-visible service-entry name.
    /// 用户可见服务入口名称。
    service_entry_display_name: String,
    /// User-visible credential connection name.
    /// 用户可见凭据连接名称。
    connection_display_name: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    model_display_name: String,
    /// Exact immutable account and model selection persisted when chosen.
    /// 选中时持久化的精确不可变账号与模型选择。
    selection: ActiveAiModelSelection,
    /// Process-local catalog generation that established the route.
    /// 建立该路由的进程内目录代次。
    catalog_generation: u64,
    /// Exact model capability contract used to filter media and reasoning controls.
    /// 用于筛选媒体与思考控件的精确模型能力契约。
    capabilities: ModelCapabilities,
}

/// Browser-facing conflict retained for one session provider tool capability.
/// 面向浏览器保留的一项会话供应商工具能力冲突。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiSessionProviderToolConflict {
    /// Exact semantic capability whose saved state is invalid.
    /// 已保存状态无效的精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// Stable machine-readable conflict kind.
    /// 稳定的机器可读冲突类型。
    kind: String,
    /// Actionable user-visible diagnostic.
    /// 可操作的用户可见诊断。
    message: String,
}

/// Browser-facing complete provider tool snapshot owned by one session.
/// 面向浏览器且由单个会话持有的完整供应商工具快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiSessionProviderToolSettingsSnapshot {
    /// Capabilities enabled only for this session.
    /// 仅为当前会话启用的能力。
    enabled_capabilities: BTreeSet<ProviderToolCapabilityKind>,
    /// Exact model and account selected for every retained capability.
    /// 每项保留能力选择的精确模型与账号。
    selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    /// Exact native model-extension target sets selected for this session.
    /// 当前会话选择的精确原生模型扩展目标集合。
    extension_target_sets:
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
    /// Session-selected backend mode for each enabled media-understanding capability.
    /// 当前会话为每项启用的媒体理解能力选择的后端模式。
    media_understanding_modes:
        BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode>,
    /// Invalid retained choices that require explicit user confirmation.
    /// 需要用户显式确认的无效保留选择。
    conflicts: Vec<PublicAiSessionProviderToolConflict>,
}

/// Browser-facing credential metadata that excludes the host secret-store reference.
/// 面向浏览器且排除宿主秘密存储引用的凭据元数据。
#[derive(Debug, Clone, Serialize)]
pub(super) struct PublicAiConnectionSnapshot {
    /// Stable connection identifier.
    /// 稳定连接标识。
    id: String,
    /// Stable provider identifier.
    /// 稳定供应商标识。
    provider_id: String,
    /// Stable service-entry identifier.
    /// 稳定服务入口标识。
    service_entry_id: String,
    /// Stable credential-profile identifier.
    /// 稳定凭据配置档标识。
    credential_profile_id: String,
    /// Exact billing semantics declared by the selected credential profile.
    /// 所选凭据配置档声明的精确计费语义。
    billing_kind: CredentialBillingKind,
    /// Exact usage-query capability declared by the selected credential profile.
    /// 所选凭据配置档声明的精确用量查询能力。
    usage_capability: AccountUsageQueryKind,
    /// Whether usage refresh requires the optional provider-management credential.
    /// 用量刷新是否需要可选的供应商管理凭据。
    usage_requires_management_key: bool,
    /// Whether this connection has an optional provider-management credential.
    /// 此连接是否具有可选的供应商管理凭据。
    has_management_key: bool,
    /// Optional credential-scoped service origin without secret material.
    /// 不含秘密材料的可选凭据作用域服务源地址。
    service_origin_override: Option<String>,
    /// Credential acquisition mode.
    /// 凭据获取方式。
    credential_mode: CredentialMode,
    /// Stable non-secret credential identifier.
    /// 稳定的非秘密凭据标识。
    credential_id: String,
    /// User-visible connection name.
    /// 用户可见连接名称。
    display_name: String,
    /// Protocol-scoped public cache policies.
    /// 协议作用域的公共缓存策略。
    prompt_cache_by_protocol: BTreeMap<String, PromptCachePolicy>,
    /// Whether normal routing is blocked until an explicit paid model validation succeeds.
    /// 在显式付费模型验证成功前是否阻止常规路由。
    pending_validation: bool,
}

impl PublicAiConnectionSnapshot {
    /// Redacts one internal connection for browser publication.
    /// 对一个内部连接进行脱敏以供浏览器发布。
    ///
    /// # Parameters
    /// 参数
    /// `providers` is the registry captured with the settings generation; `configuration` owns
    /// pending-validation state; and `connection` is the persisted connection being redacted.
    /// `providers` 是与设置代际共同捕获的注册表；`configuration` 持有待验证状态；
    /// `connection` 是待脱敏的持久化连接。
    ///
    /// # Returns
    /// 返回值
    /// A complete browser-safe snapshot with resolved credential semantics.
    /// 返回带已解析凭据语义的完整浏览器安全快照。
    fn new(
        providers: &ProviderRegistry,
        configuration: &PersistedAiConfiguration,
        connection: &CredentialConnection,
    ) -> Result<Self, String> {
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| format!("Provider `{}` is not registered.", connection.provider_id))?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` is not registered for provider `{}`.",
                    connection.service_entry_id, connection.provider_id
                )
            })?;
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, connection.service_entry_id
                )
            })?;
        Ok(Self {
            id: connection.id.clone(),
            provider_id: connection.provider_id.clone(),
            service_entry_id: connection.service_entry_id.clone(),
            credential_profile_id: connection.credential_profile_id.clone(),
            billing_kind: credential_profile.billing_kind,
            usage_capability: credential_profile.account_usage_query,
            usage_requires_management_key: credential_profile
                .account_usage_query
                .requires_management_credential(),
            has_management_key: connection.management_secret_reference.is_some(),
            service_origin_override: connection.service_origin_override.clone(),
            credential_mode: connection.credential_mode,
            credential_id: connection.credential_id.clone(),
            display_name: connection.display_name.clone(),
            prompt_cache_by_protocol: connection.prompt_cache_by_protocol.clone(),
            pending_validation: configuration
                .connection_is_pending_validation(connection.id.as_str()),
        })
    }
}

/// Browser-facing model route status resolved entirely by the backend.
/// 完全由后端解析的面向浏览器模型路由状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiModelRouteRecord {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact service-entry identifier.
    /// 精确服务入口标识。
    service_entry_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Effective protocol frozen in the model binding, when executable.
    /// 可执行时冻结在模型绑定中的生效协议。
    #[serde(skip_serializing_if = "Option::is_none")]
    protocol_id: Option<String>,
    /// Protocol variants available in this exact effective account catalog.
    /// 此精确生效账号目录中可用的协议变体。
    available_protocols: Vec<String>,
    /// Stable source identifier of the frozen binding, when one exists.
    /// 存在冻结绑定时的稳定来源标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    binding_source: Option<String>,
    /// Stable rule identifier when a manifest rule produced the binding.
    /// Manifest 规则生成绑定时的稳定规则标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    binding_rule_id: Option<String>,
    /// Activated provider-manifest revision that owns the binding.
    /// 持有绑定的已激活供应商 Manifest 修订。
    manifest_revision: u64,
    /// Whether request execution can consume this exact account/model route.
    /// 请求执行是否可消费此精确账号/模型路由。
    executable: bool,
    /// Stable backend-owned diagnostic when the route is not executable.
    /// 路由不可执行时由后端持有的稳定诊断。
    #[serde(skip_serializing_if = "Option::is_none")]
    unavailable_reason: Option<String>,
}

/// Browser-facing exact model override record.
/// 面向浏览器的精确模型覆盖记录。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiModelOverrideRecord {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Exact protocol profile identifier.
    /// 精确协议配置档标识。
    protocol_id: String,
    /// Complete local override value.
    /// 完整本地覆盖值。
    overrides: ModelExecutionOverrides,
}

/// Browser-facing provider-model conversation publication state.
/// 面向浏览器的供应商模型会话发布状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiModelAccessRecord {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Effective conversation enabled state after applying the persisted user choice.
    /// 应用持久化用户选择后的有效会话启用状态。
    enabled: bool,
    /// Code-owned provider default before applying a user choice.
    /// 应用用户选择前由代码拥有的供应商默认值。
    default_enabled: bool,
    /// Whether this exact model has an explicit user-owned conversation layer.
    /// 此精确模型是否存在明确的用户持有会话层。
    customized: bool,
}

/// Browser-facing model capability difference without a composite key.
/// 面向浏览器且不含组合键的模型能力差异。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiModelCapabilityOverrideRecord {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// User-owned capability differences.
    /// 用户自有能力差异。
    overrides: ModelCapabilityOverrides,
}

/// Adds one credential connection and its secret.
/// 添加一个凭据连接及其秘密。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiConnectionAddRequest {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    provider_id: String,
    /// Stable service-entry identifier.
    /// 稳定服务入口标识。
    service_entry_id: String,
    /// Selected credential profile.
    /// 选定的凭据配置档。
    credential_profile_id: String,
    /// Selected credential mode.
    /// 选定的凭据方式。
    credential_mode: CredentialMode,
    /// User-visible credential label.
    /// 用户可见凭据标签。
    display_name: String,
    /// Secret entered by the user.
    /// 用户输入的秘密。
    secret: String,
    /// Optional provider-management key used only for allowance queries.
    /// 仅用于用量查询的可选供应商管理密钥。
    management_key: Option<String>,
    /// Optional credential-scoped provider service origin.
    /// 可选凭据作用域供应商服务源地址。
    service_origin_override: Option<String>,
}

/// Updates editable metadata for one credential connection.
/// 更新一个凭据连接的可编辑元数据。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiConnectionUpdateRequest {
    /// Exact connection identifier.
    /// 精确连接标识。
    connection_id: String,
    /// User-visible credential label.
    /// 用户可见凭据标签。
    display_name: String,
    /// Selected service entry.
    /// 选定服务入口。
    service_entry_id: String,
    /// Selected credential profile.
    /// 选定的凭据配置档。
    credential_profile_id: String,
    /// Selected credential mode.
    /// 选定凭据方式。
    credential_mode: CredentialMode,
    /// Optional replacement secret.
    /// 可选替换秘密。
    secret: Option<String>,
    /// Optional replacement provider-management key.
    /// 可选替换供应商管理密钥。
    management_key: Option<String>,
    /// Whether the existing provider-management key should be removed.
    /// 是否移除现有供应商管理密钥。
    #[serde(default)]
    remove_management_key: bool,
    /// Optional credential-scoped provider service origin.
    /// 可选凭据作用域供应商服务源地址。
    service_origin_override: Option<String>,
}

/// Updates the background account-usage refresh interval.
/// 更新后台账号用量刷新间隔。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiUsageRefreshSettingsRequest {
    /// Refresh interval in seconds.
    /// 刷新间隔秒数。
    interval_seconds: u64,
}

/// Saves one exact provider priority used by settings presentation order.
/// 保存一个用于设置界面展示顺序的精确供应商优先级。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiProviderPrioritySaveRequest {
    /// Exact registered provider identifier.
    /// 精确的已注册供应商标识。
    provider_id: String,
    /// Signed priority sorted from high to low; zero restores the inherited default.
    /// 从高到低排序的有符号优先级；零恢复继承默认值。
    priority: i32,
}

/// Saves or clears the system-level Codex context-window policy.
/// 保存或清除系统级 Codex 上下文窗口策略。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiContextWindowPolicyOverridesSaveRequest {
    /// Sparse policy layer; an empty object restores built-in Codex defaults.
    /// 稀疏策略层；空对象恢复内置 Codex 默认值。
    overrides: ContextWindowPolicyOverrides,
}

/// Saves all fixed system context-compaction window node values atomically.
/// 原子保存全部固定系统上下文压缩窗口节点值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiContextCompactionWindowNodesSaveRequest {
    /// Complete five-node configuration with immutable identifiers and order.
    /// 包含不可变标识与顺序的完整五节点配置。
    nodes: ContextCompactionWindowConfiguration,
}

/// Selects one exact public AI model and account.
/// 选择一个精确公共 AI 模型与账号。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiModelSelectionRequest {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Optional persisted conversation receiving this model selection.
    /// 接收本次模型选择的可选持久化会话。
    #[serde(default)]
    conversation_id: Option<String>,
}

/// Saves local narrowing and model parameter values.
/// 保存本地收窄与模型参数值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiModelOverridesSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Exact protocol profile identifier.
    /// 精确协议配置档标识。
    protocol_id: String,
    /// Complete replacement override value, or null to restore protocol defaults.
    /// 完整替换覆盖值；为 null 时还原协议默认值。
    overrides: Option<ModelExecutionOverrides>,
}

/// Saves one provider-model conversation choice independent from credentials and protocols.
/// 保存一个独立于凭据和协议的供应商模型会话选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiModelEnabledSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
    /// Restores the inherited value instead of creating or replacing a model layer.
    /// 恢复继承值，而不是创建或替换模型层。
    #[serde(default)]
    restore: bool,
}

/// Saves one provider-model non-conversation internal-use choice.
/// 保存一项供应商、模型级非会话内部使用选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiInternalModelEnabledSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
}

/// Replaces one exact consumer-profile inference route.
/// 替换一条精确消费者配置档推理路由。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiInferenceRouteSaveRequest {
    /// Stable consumer profile identifier.
    /// 稳定消费者配置档标识。
    consumer_profile_id: String,
    /// Consumer-owned purpose required only for restricted LLM routes.
    /// 仅受限 LLM 路由要求的消费者自有用途。
    #[serde(default)]
    purpose_id: Option<String>,
    /// Exact operation being replaced.
    /// 正在替换的精确操作。
    operation: AiOperation,
    /// Exact route, or absence to clear the configured operation.
    /// 精确路由，缺失时清除已配置操作。
    route: Option<InferenceRouteConfiguration>,
}

/// Requests one real embedding or rerank connection test through a consumer profile.
/// 请求通过一个消费者配置档执行真实向量或重排连接测试。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiInferenceTestRequest {
    /// Stable consumer profile whose persisted route is tested.
    /// 接受测试的持久化路由所属稳定消费者配置档。
    consumer_profile_id: String,
    /// Exact non-LLM operation to execute.
    /// 要执行的精确非 LLM 操作。
    operation: AiOperation,
}

/// Returns compact validated facts from one real internal inference test.
/// 返回一次真实内部推理测试的紧凑已校验事实。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiInferenceTestResult {
    /// Exact operation tested.
    /// 接受测试的精确操作。
    operation: AiOperation,
    /// Frozen provider identifier returned by execution.
    /// 执行返回的冻结供应商标识。
    provider_id: String,
    /// Frozen service-entry identifier returned by execution.
    /// 执行返回的冻结服务入口标识。
    service_entry_id: String,
    /// Frozen credential connection identifier returned by execution.
    /// 执行返回的冻结凭据连接标识。
    connection_id: String,
    /// Frozen model identifier returned by execution.
    /// 执行返回的冻结模型标识。
    model_id: String,
    /// Number of validated result items.
    /// 已校验结果项数量。
    result_count: usize,
    /// Optional validated embedding dimensions.
    /// 可选的已校验向量维度。
    dimensions: Option<u32>,
    /// End-to-end provider execution time in milliseconds.
    /// 供应商端到端执行耗时毫秒数。
    duration_ms: u64,
}

/// Replaces operation-scoped internal defaults for one provider model.
/// 替换一个供应商模型的操作作用域内部默认值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiInternalModelOverridesSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Exact internal operation.
    /// 精确内部操作。
    operation: AiOperation,
    /// Complete replacement override value.
    /// 完整替换覆盖值。
    overrides: InternalModelOverrides,
}

/// Replaces the single selected target for one provider tool capability.
/// 替换一项供应商工具能力唯一选中的目标。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiProviderToolSelectionSaveRequest {
    /// Exact semantic capability.
    /// 精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// One exact target, or absence to disable the capability.
    /// 一个精确目标，缺失时禁用该能力。
    selection: Option<ProviderToolTargetSelection>,
}

/// Enables or disables one provider model as a multimodal understanding candidate.
/// 启用或禁用一个供应商模型作为多模态理解候选。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiMultimodalUnderstandingCandidateSaveRequest {
    /// Exact provider identifier shared by every account route for this switch.
    /// 此开关覆盖全部账号路由的精确供应商标识。
    provider_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Replacement admission state.
    /// 替换后的准入状态。
    enabled: bool,
}

/// Saves or clears one provider-tool or model-replacement media-understanding backend.
/// 保存或清除一项供应商工具或模型代替媒体理解后端。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiMediaUnderstandingSelectionSaveRequest {
    /// Exact image, video, or audio understanding capability.
    /// 精确的图片、视频或音频理解能力。
    capability: ProviderToolCapabilityKind,
    /// One exact backend, or absence to clear only the backend selection.
    /// 一个精确后端；缺失时仅清除后端选择，不改写独立能力开关。
    selection: Option<MediaUnderstandingBackendSelection>,
}

/// Replaces the complete native target set for one model-extension capability.
/// 替换一项模型扩展能力的完整原生目标集合。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiProviderExtensionTargetSetSaveRequest {
    /// Exact generative model-extension capability.
    /// 精确生成类模型扩展能力。
    capability: ProviderToolCapabilityKind,
    /// Complete replacement set of exact native targets.
    /// 精确原生目标的完整替换集合。
    selections: BTreeSet<ProviderToolTargetSelection>,
}

/// Executes one exact search or extraction target for settings diagnostics.
/// 为设置诊断执行一个精确的搜索或网页抓取目标。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiProviderToolTestRequest {
    /// Exact semantic capability under test.
    /// 正在测试的精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// Exact connection and provider-tool target selected by the operator.
    /// 操作员选择的精确连接与供应商工具目标。
    selection: ProviderToolTargetSelection,
    /// Provider-native arguments already shaped by the target's declared schema.
    /// 已按照目标声明 Schema 组织的供应商原生参数。
    arguments: serde_json::Value,
}

/// Changes whether one semantic provider tool capability is enabled by default.
/// 修改一项语义供应商工具能力是否默认启用。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiProviderToolCapabilityEnabledSaveRequest {
    /// Exact semantic capability.
    /// 精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
}

/// Replaces the complete provider tool configuration for one session.
/// 替换单个会话完整的供应商工具配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiSessionProviderToolSelectionsSaveRequest {
    /// Conversation that owns the selection override.
    /// 拥有选择覆盖的会话。
    conversation_id: String,
    /// Semantic capabilities enabled only for this session.
    /// 仅为当前会话启用的语义能力。
    enabled_capabilities: BTreeSet<ProviderToolCapabilityKind>,
    /// Exact target retained independently from capability activation.
    /// 与能力启用状态独立保留的精确目标。
    selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    /// Complete native model-extension target sets retained by this session.
    /// 当前会话保留的完整原生模型扩展目标集合。
    extension_target_sets:
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
    /// Session-selected provider or model-replacement mode per media capability.
    /// 当前会话为每项媒体能力选择的供应商专用或模型代替模式。
    #[serde(default)]
    media_understanding_modes:
        BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode>,
}

/// Changes the explicit default connection for one provider.
/// 修改一个供应商的显式默认连接。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiDefaultConnectionSaveRequest {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    provider_id: String,
    /// Exact connection identifier.
    /// 精确连接标识。
    connection_id: String,
}

/// Removes one connection with an explicit replacement default when required.
/// 在需要时携带显式替代默认账号移除一个连接。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiConnectionRemoveRequest {
    /// Exact connection to remove.
    /// 待移除的精确连接。
    connection_id: String,
    /// Explicit replacement default for the same provider.
    /// 同一供应商的显式替代默认账号。
    replacement_default_connection_id: Option<String>,
}

/// Creates or replaces one user-defined compatible provider.
/// 创建或替换一个用户自定义兼容供应商。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomProviderSaveRequest {
    /// Existing provider identifier when editing.
    /// 编辑时使用的现有供应商标识。
    provider_id: Option<String>,
    /// User-visible provider name.
    /// 用户可见供应商名称。
    display_name: String,
    /// Exact compatible API base URL.
    /// 精确兼容 API 基础 URL。
    base_url: String,
    /// Explicit remote-compaction policy for Codex Responses.
    /// Codex Responses 的显式远端压缩策略。
    #[serde(default)]
    remote_compaction_policy: ResponsesRemoteCompactionPolicy,
    /// Explicit WebSocket policy for Codex Responses.
    /// Codex Responses 的显式 WebSocket 策略。
    #[serde(default)]
    websocket_policy: ResponsesWebsocketPolicy,
    /// Exact API-key header projection.
    /// 精确 API Key 请求头投影。
    authentication: CustomProviderAuthentication,
    /// User-visible label for the first credential, accepted only when creating a provider.
    /// 首个凭据的用户可见标签，仅在创建供应商时接受。
    #[serde(default)]
    credential_display_name: Option<String>,
    /// New secret used only while creating the provider's first credential.
    /// 仅用于创建供应商首个凭据的新秘密。
    #[serde(default)]
    secret: Option<String>,
    /// Existing credential selected for validation while editing a provider.
    /// 编辑供应商时选择用于验证的现有凭据。
    #[serde(default)]
    validation_connection_id: Option<String>,
    /// Selected model-directory discovery behavior.
    /// 选定的模型目录发现行为。
    #[serde(default)]
    model_discovery_mode: CustomProviderModelDiscoveryMode,
    /// Same-origin relative discovery path used by custom-path mode.
    /// 自定义路径模式使用的同源相对发现路径。
    #[serde(default)]
    model_discovery_path: Option<String>,
    /// Non-empty compatible protocol collection served by this provider.
    /// 此供应商提供的非空兼容协议集合。
    protocols: Vec<CustomProviderProtocol>,
    /// Per-protocol route overrides with route-specific base URL, path, discovery, and headers.
    /// 带路由专属基础 URL、路径、发现与请求头的逐协议路由覆盖。
    #[serde(default)]
    protocol_routes: Vec<CustomProtocolRoute>,
    /// Bounded non-sensitive fixed request headers shared by every route.
    /// 全部路由共享的有界非敏感固定请求头。
    #[serde(default)]
    request_headers: BTreeMap<String, String>,
    /// Allows an explicitly confirmed manual fallback after unsupported, invalid, or empty discovery.
    /// 允许在目录不受支持、格式无效或为空后使用已明确确认的手工回退。
    #[serde(default)]
    allow_manual_fallback: bool,
    /// Allows saving a pending provider after a retryable discovery failure.
    /// 允许在可重试发现失败后将供应商保存为待验证状态。
    #[serde(default)]
    allow_pending_validation: bool,
    /// Confirms deletion of protocol-scoped model settings that cannot apply after a protocol switch.
    /// 确认删除协议切换后无法继续应用的协议作用域模型设置。
    #[serde(default)]
    confirm_inactive_protocol_override_deletion: bool,
}

/// Returns one typed, non-persistent custom-provider validation observation.
/// 返回一项类型化且不持久化的自定义供应商验证观测。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiCustomProviderProbeResult {
    /// Closed discovery state without credential material.
    /// 不含凭据材料的封闭发现状态。
    status: CatalogDiscoveryStatus,
    /// Raw models explicitly returned by the candidate credential.
    /// 候选凭据明确返回的原始模型。
    catalog: Option<ConnectionModelCatalog>,
    /// Whether the user may explicitly continue with manual models.
    /// 用户是否可以明确继续使用手工模型。
    manual_fallback_available: bool,
    /// Whether the user may explicitly persist this candidate for later retry.
    /// 用户是否可以明确持久化候选项以便稍后重试。
    pending_save_available: bool,
}

/// Requests one explicit, potentially billable custom-provider model validation.
/// 请求一次显式且可能计费的自定义供应商模型验证。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelValidationRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Exact credential connection under validation.
    /// 接受验证的精确凭据连接。
    connection_id: String,
    /// Exact bidirectional-text model identifier.
    /// 精确双向文本模型标识。
    model_id: String,
}

/// Returns compact facts after one explicit custom-model validation succeeds.
/// 在一次显式自定义模型验证成功后返回紧凑事实。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiCustomModelValidationResult {
    /// Exact provider validated by the request.
    /// 请求验证的精确供应商。
    provider_id: String,
    /// Exact credential connection validated by the request.
    /// 请求验证的精确凭据连接。
    connection_id: String,
    /// Exact model validated by the request.
    /// 请求验证的精确模型。
    model_id: String,
    /// Trimmed provider text returned by the validation request.
    /// 验证请求返回的已去除首尾空白供应商文本。
    response_text: String,
    /// Unicode scalar count in the returned validation text.
    /// 返回验证文本中的 Unicode 标量数量。
    response_characters: usize,
}

/// Removes one user-defined compatible provider.
/// 移除一个用户自定义兼容供应商。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomProviderRemoveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Whether the user explicitly confirmed deleting all credentials owned by this provider.
    /// 用户是否明确确认同时删除该供应商持有的全部凭据。
    #[serde(default)]
    confirm_credential_deletion: bool,
}

/// Replaces the complete user-owned model set for one custom provider.
/// 替换一个自定义供应商的完整用户自有模型集合。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelsSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Complete replacement model set.
    /// 完整替换模型集合。
    models: Vec<CustomModelDefinition>,
    /// Explicit reviewed template decisions committed with the replacement model set.
    /// 与替换模型集合一同提交的显式已审查模板决策。
    #[serde(default)]
    template_decisions: Vec<PublicAiCustomModelTemplateDecisionBatchItem>,
}

/// One per-model protocol preference item for a custom provider.
/// 一个自定义供应商的逐模型协议偏好条目。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelProtocolPreferenceItem {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Auto or explicit protocol preference for this model.
    /// 该模型的自动或显式协议偏好。
    preference: CustomModelProtocolPreference,
}

/// Replaces the per-model protocol preferences for one custom provider.
/// 替换一个自定义供应商的逐模型协议偏好。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelProtocolPreferencesSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Complete replacement preference set.
    /// 完整替换偏好集合。
    preferences: Vec<PublicAiCustomModelProtocolPreferenceItem>,
}

/// Saves or restores the direct provider-wide model settings for one custom provider.
/// 保存或还原一个自定义供应商的直接供应商级模型设置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomProviderModelDefaultsSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Complete provider layer, or null to restore the in-memory system baseline.
    /// 完整供应商层；为 null 时还原内存中的系统基线。
    defaults: Option<CustomProviderModelDefaults>,
}

/// Saves or clears one reviewed common-template decision.
/// 保存或清除一项已审查公共模板决策。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelTemplateDecisionSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Replacement decision, or null to return to an undecided state.
    /// 替换决策；为 null 时恢复未决定状态。
    decision: Option<CustomModelTemplateDecision>,
}

/// Queries offline common-capability template suggestions for one model identifier.
/// 查询一个模型标识的离线公共能力模板建议。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelTemplateSuggestionsRequest {
    /// Exact model identifier entered by the user or discovered from the provider.
    /// 用户输入或供应商发现的精确模型标识。
    model_id: String,
}

/// Maximum number of offline template lookups accepted by one browser request.
/// 单次浏览器请求允许执行的离线模板查询数量上限。
const MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE: usize = 2_000;

/// Queries offline template suggestions for a bounded set of model identifiers.
/// 为一组有界模型标识查询离线模板建议。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelTemplateSuggestionsBatchRequest {
    /// Exact model identifiers shown by one custom-provider catalog union.
    /// 一个自定义供应商目录并集中显示的精确模型标识。
    model_ids: Vec<String>,
}

/// Carries offline suggestions for one exact model without map-key parsing.
/// 承载一个精确模型的离线建议且无需解析映射键。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiCustomModelTemplateSuggestionsRecord {
    /// Exact model identifier used for the lookup.
    /// 用于查询的精确模型标识。
    model_id: String,
    /// Ordered immutable template candidates.
    /// 按顺序排列的不可变模板候选。
    suggestions: Vec<CommonModelTemplateSuggestion>,
}

/// Stores one model decision inside an atomic provider-scoped batch.
/// 在一个供应商作用域原子批次中保存一项模型决策。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelTemplateDecisionBatchItem {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Explicit reviewed decision to persist.
    /// 需要持久化的显式已审查决策。
    decision: CustomModelTemplateDecision,
}

/// Saves a bounded set of reviewed template decisions atomically.
/// 原子保存一组有界的已审查模板决策。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelTemplateDecisionsBatchSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Distinct reviewed decisions.
    /// 互不重复的已审查决策。
    decisions: Vec<PublicAiCustomModelTemplateDecisionBatchItem>,
}

/// Saves or clears one per-account manual-model availability difference.
/// 保存或清除一项逐账号手工模型可用性差异。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomModelConnectionAvailabilitySaveRequest {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact manually declared model identifier.
    /// 精确手工声明模型标识。
    model_id: String,
    /// Explicit state, or null to restore discovery-derived behavior.
    /// 显式状态；为 null 时恢复发现派生行为。
    enabled: Option<bool>,
}

/// Saves one exact model access choice for one credential account.
/// 保存一个凭据账号的一项精确模型访问选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiConnectionModelEnabledSaveRequest {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Exact upstream model identifier from that account's source membership.
    /// 来自该账号源成员关系的精确上游模型标识。
    model_id: String,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
}

/// Replaces the inherited model access state for one credential account.
/// 替换一个凭据账号继承的模型访问状态。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiConnectionModelsEnabledSaveRequest {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// State inherited by every current and future supported model.
    /// 当前与未来每个受支持模型继承的状态。
    enabled: bool,
}

/// Saves or clears one provider-model capability difference.
/// 保存或清除一个供应商模型能力差异。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiModelCapabilityOverridesSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Replacement difference, or null to restore immutable defaults.
    /// 替换差异；为 null 时恢复不可变默认值。
    overrides: Option<ModelCapabilityOverrides>,
}

/// Saves or restores one provider-level capability layer.
/// 保存或恢复一层供应商级能力配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiProviderCapabilityOverridesSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Replacement difference, or null to restore inherited catalog defaults.
    /// 替换差异；为 null 时恢复继承的目录默认值。
    overrides: Option<ModelCapabilityOverrides>,
}

/// Carries one isolated custom-provider candidate through validation and optional persistence.
/// 承载一个隔离的自定义供应商候选项，供验证及可选持久化使用。
struct PreparedCustomProviderCandidate {
    /// Stable candidate provider identifier.
    /// 稳定的候选供应商标识。
    provider_id: String,
    /// Strict projected provider descriptor.
    /// 严格投影后的供应商描述。
    descriptor: ProviderDescriptor,
    /// Exact descriptor-owned activation identities used by every credential account.
    /// 每个凭据账号使用的精确描述符自有激活身份。
    activation_target: CustomProviderActivationTarget,
    /// Optional temporary connection used only when a metadata request has credential material.
    /// 仅在元数据请求具有凭据材料时使用的可选临时连接。
    validation_connection: Option<CredentialConnection>,
    /// New candidate-owned secret kept in memory until the non-cancellable commit boundary.
    /// 在不可取消提交边界前始终只由候选项保存在内存中的新秘密。
    temporary_secret: Option<CredentialSecretEnvelope>,
    /// Whether the request edits one existing provider.
    /// 请求是否编辑一个现有供应商。
    updating_existing: bool,
}

/// Identifies descriptor-owned activation identities for one custom provider.
/// 标识一个自定义供应商由描述符持有的激活身份。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CustomProviderActivationTarget {
    /// Exact service entry selected by the provider descriptor.
    /// 供应商描述符选定的精确服务入口。
    service_entry_id: String,
    /// Exact credential profile selected by the provider descriptor.
    /// 供应商描述符选定的精确凭据配置档。
    credential_profile_id: String,
    /// Exact provider default protocol profile used by protocol-bound internal routes.
    /// 协议绑定内部路由使用的精确供应商默认协议配置档。
    default_protocol_profile_id: String,
    /// Closed protocol profile set declared by the provider descriptor.
    /// 供应商描述符声明的封闭协议配置档集合。
    protocol_profile_ids: BTreeSet<String>,
}

/// Stores one candidate observation before the caller decides whether it may be persisted.
/// 保存一项候选观测，供调用方决定是否可以持久化。
struct CustomProviderCandidateObservation {
    /// Typed process-local discovery status.
    /// 类型化的进程内发现状态。
    status: CatalogDiscoveryStatus,
    /// Raw provider catalog before Vulcan overlays.
    /// 应用 Vulcan 覆盖前的原始供应商目录。
    catalog: Option<ConnectionModelCatalog>,
    /// Original refresh failure retained for save-policy diagnostics.
    /// 为保存策略诊断保留的原始刷新失败。
    refresh_error: Option<String>,
}

/// Records the exact durable state allowed by one credential-specific provider observation.
/// 记录一次凭据级供应商观测所允许的精确持久化状态。
#[derive(Debug, Clone)]
struct CustomProviderConnectionValidationOutcome {
    /// Whether the migrated credential must remain unavailable until a later successful validation.
    /// 迁移后的凭据是否必须保持不可用，直到后续验证成功。
    pending_validation: bool,
    /// Raw source catalog observed through the candidate contract and remapped to the durable connection.
    /// 通过候选契约观测并重映射到持久连接的原始来源目录。
    catalog: Option<ConnectionModelCatalog>,
    /// Exact typed discovery state produced by the candidate observation.
    /// 候选观测产生的精确类型化发现状态。
    status: CatalogDiscoveryStatus,
    /// Controlled discovery diagnostic retained when the accepted observation was non-ready.
    /// 接受的观测并非就绪时保留的受控发现诊断。
    refresh_error: Option<String>,
}

/// Builds one strict candidate and its temporary credential connection.
/// 构建一个严格候选项及其临时凭据连接。
///
/// # Parameters
/// 参数
/// `request` contains the exact user-entered provider and credential contract.
/// `request` 包含用户输入的精确供应商与凭据契约。
///
/// # Returns
/// 返回值
/// Returns a fully validated candidate that has not touched runtime or persistence state.
/// 返回一个尚未接触运行时或持久化状态的完整已校验候选项。
fn prepare_custom_provider_candidate(
    request: &PublicAiCustomProviderSaveRequest,
    current: &PersistedAiConfiguration,
) -> Result<PreparedCustomProviderCandidate, String> {
    validate_non_empty("displayName", request.display_name.as_str())?;
    let updating_existing = request.provider_id.is_some();
    let provider_id = request
        .provider_id
        .clone()
        .unwrap_or_else(|| format!("custom:{}", uuid::Uuid::new_v4().simple()));
    let mut descriptor = CustomProviderDefinition {
        id: provider_id.clone(),
        display_name: request.display_name.clone(),
        base_url: request.base_url.clone(),
        protocols: request.protocols.clone(),
        authentication: request.authentication,
        model_discovery_mode: request.model_discovery_mode,
        model_discovery_path: request.model_discovery_path.clone(),
        protocol_routes: request.protocol_routes.clone(),
        request_headers: request.request_headers.clone(),
    }
    .into_descriptor()?;
    let declares_codex = descriptor
        .service_entries
        .iter()
        .flat_map(|entry| entry.protocol_routes.iter())
        .any(|route| {
            route.protocol_profile_id == CustomProviderProtocol::OpenAiCodexResponses.profile_id()
        });
    if !declares_codex
        && (request.remote_compaction_policy != ResponsesRemoteCompactionPolicy::Disabled
            || request.websocket_policy != ResponsesWebsocketPolicy::Disabled)
    {
        return Err(String::from(
            "Responses capability policies require the Codex Responses protocol.",
        ));
    }
    let route_features = declares_codex.then_some(ResponsesRouteFeatures {
        remote_compaction: request.remote_compaction_policy,
        websocket: request.websocket_policy,
    });
    if let Some(features) = route_features {
        for entry in &mut descriptor.service_entries {
            for route in &mut entry.protocol_routes {
                if route.protocol_profile_id
                    == CustomProviderProtocol::OpenAiCodexResponses.profile_id()
                {
                    route.responses_features = Some(features);
                }
            }
        }
    }
    let entry = descriptor
        .service_entries
        .first()
        .ok_or_else(|| String::from("Custom provider must declare one service entry."))?;
    let route = entry
        .protocol_routes
        .first()
        .ok_or_else(|| String::from("Custom provider must declare one protocol route."))?;
    let profile = entry
        .credential_profiles
        .first()
        .ok_or_else(|| String::from("Custom provider must declare one credential profile."))?;
    // activationTarget contains only descriptor-owned identities and remains valid without credentials.
    // activationTarget 仅包含描述符持有的身份，因此在没有凭据时仍然有效。
    let activation_target = CustomProviderActivationTarget {
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        default_protocol_profile_id: route.protocol_profile_id.clone(),
        protocol_profile_ids: entry
            .protocol_routes
            .iter()
            .map(|route| route.protocol_profile_id.clone())
            .collect(),
    };
    let temporary_connection_id = format!("connection_{}", uuid::Uuid::new_v4().simple());
    let (validation_connection, temporary_secret) = if updating_existing {
        if request.credential_display_name.is_some() || request.secret.is_some() {
            return Err(String::from(
                "Editing a custom provider must reuse one existing credential and cannot create a new secret.",
            ));
        }
        // existing_connection is optional because a provider remains editable after its last credential is removed.
        // existing_connection 为可选值，因为供应商在最后一份凭据被移除后仍应可编辑。
        let existing_connection = request
            .validation_connection_id
            .as_deref()
            .map(|validation_connection_id| {
                let existing_connection = current
                    .connection_graph
                    .connections
                    .get(validation_connection_id)
                    .ok_or_else(|| {
                        format!(
                            "Validation connection `{validation_connection_id}` is not configured."
                        )
                    })?;
                if existing_connection.provider_id != provider_id {
                    return Err(format!(
                        "Validation connection `{validation_connection_id}` does not belong to custom provider `{provider_id}`."
                    ));
                }
                Ok(existing_connection)
            })
            .transpose()?;
        (
            existing_connection.map(|existing_connection| CredentialConnection {
                id: temporary_connection_id,
                provider_id: provider_id.clone(),
                service_entry_id: activation_target.service_entry_id.clone(),
                credential_profile_id: activation_target.credential_profile_id.clone(),
                credential_mode: existing_connection.credential_mode,
                credential_id: existing_connection.credential_id.clone(),
                display_name: existing_connection.display_name.clone(),
                secret_reference: existing_connection.secret_reference.clone(),
                management_secret_reference: existing_connection
                    .management_secret_reference
                    .clone(),
                service_origin_override: existing_connection.service_origin_override.clone(),
                prompt_cache_by_protocol: existing_connection.prompt_cache_by_protocol.clone(),
            }),
            None,
        )
    } else {
        if request.validation_connection_id.is_some() {
            return Err(String::from(
                "validationConnectionId is accepted only while editing a custom provider.",
            ));
        }
        let credential_display_name =
            request.credential_display_name.as_deref().ok_or_else(|| {
                String::from("credentialDisplayName is required while creating a custom provider.")
            })?;
        let secret = request
            .secret
            .as_deref()
            .ok_or_else(|| String::from("secret is required while creating a custom provider."))?;
        validate_non_empty("credentialDisplayName", credential_display_name)?;
        validate_non_empty("secret", secret)?;
        let secret_reference = format!("ai_secret_{}", uuid::Uuid::new_v4().simple());
        (
            Some(CredentialConnection {
                id: temporary_connection_id,
                provider_id: provider_id.clone(),
                service_entry_id: activation_target.service_entry_id.clone(),
                credential_profile_id: activation_target.credential_profile_id.clone(),
                credential_mode: profile.credential_mode,
                credential_id: format!("credential_{}", uuid::Uuid::new_v4().simple()),
                display_name: credential_display_name.trim().to_string(),
                secret_reference,
                management_secret_reference: None,
                service_origin_override: None,
                prompt_cache_by_protocol: BTreeMap::new(),
            }),
            Some(CredentialSecretEnvelope::new_api_key(secret.to_string())?),
        )
    };
    Ok(PreparedCustomProviderCandidate {
        provider_id,
        descriptor,
        activation_target,
        validation_connection,
        temporary_secret,
        updating_existing,
    })
}

/// Creates one isolated configuration containing the candidate provider and temporary connection.
/// 创建一份包含候选供应商与临时连接的隔离配置。
///
/// # Parameters
/// 参数
/// `runtime` supplies the effective provider registry, `current` is immutable persisted state, and
/// `candidate` contains the exact descriptor and validation connection.
/// `runtime` 提供有效供应商注册表，`current` 是不可变持久化状态，`candidate` 包含精确描述与验证连接。
///
/// # Returns
/// 返回值
/// Returns the isolated validation configuration after ownership and protocol checks.
/// 在归属与协议检查后返回隔离验证配置。
fn custom_provider_validation_configuration(
    runtime: &PublicAiRuntime,
    current: &PersistedAiConfiguration,
    candidate: &PreparedCustomProviderCandidate,
) -> Result<PersistedAiConfiguration, String> {
    match current.custom_providers.get(candidate.provider_id.as_str()) {
        Some(existing) if existing.kind != ProviderKind::Custom => {
            return Err(format!(
                "Provider `{}` is not a user-defined provider.",
                candidate.provider_id
            ));
        }
        None if candidate.updating_existing => {
            return Err(format!(
                "Custom provider `{}` is not configured.",
                candidate.provider_id
            ));
        }
        Some(_) | None => {}
    }
    let mut validation = current.clone();
    validation
        .custom_providers
        .insert(candidate.provider_id.clone(), candidate.descriptor.clone());
    let providers = runtime.effective_providers(&validation)?;
    // validation_connection is absent only for credential-free provider edits, which do not perform metadata requests.
    // validation_connection 仅在无凭据供应商编辑时缺失，而此类编辑不会执行元数据请求。
    let validation_connection = candidate.validation_connection.as_ref().ok_or_else(|| {
        format!(
            "Custom provider `{}` has no credential connection available for validation.",
            candidate.provider_id
        )
    })?;
    validation
        .connection_graph
        .add_connection(&providers, validation_connection.clone())?;
    Ok(validation)
}

/// Reports whether an edited provider changes the credential-facing transport contract.
/// 报告供应商编辑是否改变面向凭据的传输契约。
///
/// # Parameters
/// 参数
/// `current` is the durable configuration and `candidate` is the strict edited descriptor.
/// `current` 是持久化配置，`candidate` 是严格的编辑后描述符。
///
/// # Returns
/// 返回值
/// `true` for protocol, endpoint, authentication, or discovery changes; cosmetic names and
/// execution-only Responses capability switches return `false`.
/// 协议、端点、鉴权或发现方式变化时返回 `true`；仅名称变化与只影响执行的 Responses 能力开关返回 `false`。
fn custom_provider_connection_contract_changed(
    current: &PersistedAiConfiguration,
    candidate: &PreparedCustomProviderCandidate,
) -> Result<bool, String> {
    if !candidate.updating_existing {
        return Ok(true);
    }
    // existing is the exact persisted custom descriptor being edited.
    // existing 是正在编辑的精确持久化自定义描述符。
    let existing = current
        .custom_providers
        .get(candidate.provider_id.as_str())
        .ok_or_else(|| {
            format!(
                "Custom provider `{}` is not configured.",
                candidate.provider_id
            )
        })?;
    if existing.kind != ProviderKind::Custom {
        return Err(format!(
            "Provider `{}` is not a user-defined provider.",
            candidate.provider_id
        ));
    }
    // existing_entries and candidate_entries retain every credential-facing field while allowing
    // the explicit execution-only Responses capabilities to be normalized out of this comparison.
    // existing_entries 与 candidate_entries 保留全部面向凭据的字段，同时允许从本次比较中
    // 规范化移除显式且只影响执行的 Responses 能力。
    let existing_entries = existing.service_entries.clone();
    let mut candidate_entries = candidate.descriptor.service_entries.clone();
    // Responses capability switches are the sole fields proven by ProtocolRoute to control
    // request execution rather than endpoint discovery or credential projection. Every
    // candidate route is normalized to the persisted route with the same protocol profile;
    // routes for newly added protocols keep their own features because there is no persisted
    // authority to inherit (plan §R6.9 removes the first-route authority).
    // Responses 能力开关是 ProtocolRoute 已证明只控制请求执行、而不控制端点发现或凭据投影
    // 的唯一字段。每条候选路由按相同协议配置档归一化为持久化路由的值；新增协议的路由
    // 保留自身能力，因为没有可继承的持久化权威（方案 §R6.9 移除首路由权威）。
    let existing_features_by_protocol = existing_entries
        .iter()
        .flat_map(|entry| entry.protocol_routes.iter())
        .map(|route| (route.protocol_profile_id.as_str(), route.responses_features))
        .collect::<BTreeMap<_, _>>();
    for entry in &mut candidate_entries {
        for route in &mut entry.protocol_routes {
            if let Some(existing_features) = existing_features_by_protocol
                .get(route.protocol_profile_id.as_str())
                .copied()
            {
                route.responses_features = existing_features;
            }
        }
    }
    Ok(existing_entries != candidate_entries)
}

/// Reports whether an edited custom provider changes its enabled protocol set.
/// 报告被编辑的自定义供应商是否改变其启用协议集合。
///
/// # Parameters
/// 参数
/// `current` owns the durable descriptor and `candidate` contains the newly selected routes.
/// `current` 持有持久化描述符，`candidate` 包含新选择的路由。
///
/// # Returns
/// 返回值
/// The exact set of protocol profile identifiers removed by the edit; an empty set means the
/// enabled protocol set was not reduced (creation, cosmetics, or additions only).
/// 返回编辑移除的精确协议配置档标识集合；空集合表示启用协议集合未被缩减
/// （新建、展示性修改或仅新增协议）。
fn custom_provider_removed_protocols(
    current: &PersistedAiConfiguration,
    candidate: &PreparedCustomProviderCandidate,
) -> Result<BTreeSet<String>, String> {
    if !candidate.updating_existing {
        return Ok(BTreeSet::new());
    }
    // existing_protocols is the full enabled protocol set of the persisted descriptor; the
    // single-route authority is removed for multi-protocol providers (plan §R6.9).
    // existing_protocols 是持久化描述符的完整启用协议集合；多协议供应商不再使用
    // 单路由权威（方案 §R6.9）。
    let existing_protocols = current
        .custom_providers
        .get(candidate.provider_id.as_str())
        .map(|provider| {
            provider
                .service_entries
                .iter()
                .flat_map(|entry| entry.protocol_routes.iter())
                .map(|route| route.protocol_profile_id.clone())
                .collect::<BTreeSet<_>>()
        })
        .ok_or_else(|| {
            format!(
                "Custom provider `{}` is not configured.",
                candidate.provider_id
            )
        })?;
    if existing_protocols.is_empty() {
        return Err(format!(
            "Custom provider `{}` has no persisted protocol route.",
            candidate.provider_id
        ));
    }
    let candidate_protocols = candidate
        .descriptor
        .service_entries
        .iter()
        .flat_map(|entry| entry.protocol_routes.iter())
        .map(|route| route.protocol_profile_id.clone())
        .collect::<BTreeSet<_>>();
    Ok(existing_protocols
        .difference(&candidate_protocols)
        .cloned()
        .collect())
}

/// Builds one isolated validation candidate for every credential owned by an edited provider.
/// 为被编辑供应商持有的每份凭据构建一个隔离验证候选项。
///
/// # Parameters
/// 参数
/// `current` is the durable configuration and `candidate` contains the newly selected provider contract.
/// `current` 是持久化配置，`candidate` 包含新选择的供应商契约。
///
/// # Returns
/// 返回值
/// Exact source connection identifiers paired with temporary credential-preserving candidates.
/// 返回精确源连接标识及其保留原凭据的临时候选项。
fn custom_provider_edit_validation_candidates(
    current: &PersistedAiConfiguration,
    candidate: &PreparedCustomProviderCandidate,
) -> Result<Vec<(String, PreparedCustomProviderCandidate)>, String> {
    if !candidate.updating_existing {
        return Err(String::from(
            "Credential-wide validation candidates are available only while editing a custom provider.",
        ));
    }
    // existing_connections freezes the complete credential set before any remote validation starts.
    // existing_connections 在开始任何远程验证前冻结完整凭据集合。
    let existing_connections = current
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == candidate.provider_id)
        .cloned()
        .collect::<Vec<_>>();
    Ok(existing_connections
        .into_iter()
        .map(|existing| {
            // source_connection_id preserves the durable identity used by the atomic migration.
            // source_connection_id 保留原子迁移使用的持久化身份。
            let source_connection_id = existing.id.clone();
            // validation_connection reuses only this account's credential and secret references under the new protocol.
            // validation_connection 仅在新协议下复用此账号自身的凭据与秘密引用。
            let validation_connection = CredentialConnection {
                id: format!("connection_{}", uuid::Uuid::new_v4().simple()),
                provider_id: candidate.provider_id.clone(),
                service_entry_id: candidate.activation_target.service_entry_id.clone(),
                credential_profile_id: candidate.activation_target.credential_profile_id.clone(),
                credential_mode: existing.credential_mode,
                credential_id: existing.credential_id,
                display_name: existing.display_name,
                secret_reference: existing.secret_reference,
                management_secret_reference: existing.management_secret_reference,
                service_origin_override: existing.service_origin_override,
                prompt_cache_by_protocol: existing.prompt_cache_by_protocol,
            };
            (
                source_connection_id,
                PreparedCustomProviderCandidate {
                    provider_id: candidate.provider_id.clone(),
                    descriptor: candidate.descriptor.clone(),
                    activation_target: candidate.activation_target.clone(),
                    validation_connection: Some(validation_connection),
                    temporary_secret: None,
                    updating_existing: true,
                },
            )
        })
        .collect())
}

/// Validates every existing credential against an edited provider before durable migration.
/// 在持久化迁移前，使用编辑后的供应商逐一验证全部现有凭据。
///
/// # Parameters
/// 参数
/// `runtime`, `current`, and `candidate` provide isolated discovery; the two flags preserve explicit fallback choices.
/// `runtime`、`current` 与 `candidate` 提供隔离发现；两个标志保留显式回退选择。
///
/// # Returns
/// 返回值
/// One exact validation outcome per durable connection, or an error before any configuration write.
/// 返回每条持久化连接的精确验证结果；任一失败都会在配置写入前返回错误。
async fn validate_custom_provider_edit_connections(
    runtime: &PublicAiRuntime,
    current: &PersistedAiConfiguration,
    candidate: &PreparedCustomProviderCandidate,
    allow_manual_fallback: bool,
    allow_pending_validation: bool,
) -> Result<BTreeMap<String, CustomProviderConnectionValidationOutcome>, String> {
    // candidates contains one temporary catalog scope per durable credential.
    // candidates 为每份持久化凭据保存一个临时目录作用域。
    let candidates = custom_provider_edit_validation_candidates(current, candidate)?;
    // outcomes is complete only after every credential reaches an explicitly accepted state.
    // outcomes 仅在全部凭据均达到显式接受状态后才完整。
    let mut outcomes = BTreeMap::new();
    for (source_connection_id, validation_candidate) in candidates {
        // validation_configuration binds this credential to the new descriptor without mutating durable state.
        // validation_configuration 将此凭据绑定到新描述符，且不修改持久化状态。
        let validation_configuration =
            custom_provider_validation_configuration(runtime, current, &validation_candidate)?;
        // observation is isolated from both the main runtime and the durable credential store.
        // observation 与主运行时及持久凭据仓库完全隔离。
        let observation = observe_custom_provider_candidate(
            runtime,
            &validation_configuration,
            &validation_candidate,
        )
        .await?;
        if !custom_provider_observation_is_accepted(
            observation.status.state,
            allow_manual_fallback,
            allow_pending_validation,
        ) {
            // diagnostic preserves the exact provider response for the credential that blocks migration.
            // diagnostic 保留阻止迁移的精确凭据所对应的供应商响应。
            let diagnostic = observation
                .refresh_error
                .as_deref()
                .or(observation.status.message.as_deref())
                .unwrap_or("The provider model catalog was not validated.");
            return Err(format!(
                "Custom provider credential `{source_connection_id}` cannot migrate to the selected protocol: {diagnostic}"
            ));
        }
        // pendingValidation freezes the explicit user-approved availability state before the
        // observation fields are moved into the durable-connection outcome.
        // pendingValidation 在观测字段移入持久连接结果前冻结用户明确批准的可用性状态。
        let pending_validation = custom_provider_save_requires_pending_validation(
            observation.status.state,
            allow_manual_fallback,
            allow_pending_validation,
        );
        // catalog remaps the temporary validation identity to the existing durable credential.
        // catalog 把临时验证身份重映射到既有持久凭据。
        let catalog = observation.catalog.map(|mut catalog| {
            catalog.connection_id.clone_from(&source_connection_id);
            catalog
        });
        outcomes.insert(
            source_connection_id,
            CustomProviderConnectionValidationOutcome {
                pending_validation,
                catalog,
                status: observation.status,
                refresh_error: observation.refresh_error,
            },
        );
    }
    Ok(outcomes)
}

/// Migrates every existing credential of a custom provider to its newly selected single protocol.
/// 将自定义供应商的全部既有凭据迁移到新选择的唯一协议。
///
/// # Parameters
/// 参数
/// `providers` validates the new descriptor, `configuration` owns the connection graph,
/// `provider_id` scopes the migration, `target` supplies the new route identity, and
/// `validation_outcomes` binds verification to each exact credential.
/// `providers` 校验新描述，`configuration` 持有连接图，`provider_id` 限定迁移范围，
/// `target` 提供新的路由身份，`validation_outcomes` 将验证结果绑定到每份精确凭据。
///
/// # Returns
/// 返回值
/// Success only when every migrated credential is valid under the new provider descriptor.
/// 仅当每个迁移后的凭据在新供应商描述下均有效时返回成功。
fn migrate_custom_provider_connections(
    providers: &ProviderRegistry,
    configuration: &mut PersistedAiConfiguration,
    provider_id: &str,
    target: &CustomProviderActivationTarget,
    validation_outcomes: &BTreeMap<String, CustomProviderConnectionValidationOutcome>,
) -> Result<(), String> {
    // migrated_connections freezes the complete replacement set before any graph is mutated.
    // migrated_connections 在修改任何连接图前冻结完整替换集合。
    let migrated_connections = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .cloned()
        .map(|mut connection| {
            // _validation_outcome proves this exact credential was observed before migration.
            // _validation_outcome 证明此精确凭据已在迁移前完成观测。
            let _validation_outcome = validation_outcomes
                .get(connection.id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Custom provider credential `{}` has no validation outcome for the protocol migration.",
                        connection.id
                    )
                })?;
            connection.service_entry_id = target.service_entry_id.clone();
            connection.credential_profile_id = target.credential_profile_id.clone();
            Ok(connection)
        })
        .collect::<Result<Vec<_>, String>>()?;
    // migrated_connection_ids scopes every dependent inference-route rewrite to the exact account set.
    // migrated_connection_ids 将每项依赖推理路由改写限定到精确账号集合。
    let migrated_connection_ids = migrated_connections
        .iter()
        .map(|connection| connection.id.clone())
        .collect::<BTreeSet<_>>();
    // migrated_graph provides function-local all-or-nothing validation before the durable configuration changes.
    // migrated_graph 在持久化配置改变前提供函数内全有或全无校验。
    let mut migrated_graph = configuration.connection_graph.clone();
    for connection in migrated_connections {
        migrated_graph.connections.remove(connection.id.as_str());
        migrated_graph.add_connection(providers, connection)?;
    }
    // migrated_profiles keeps protocol-bound internal routes consistent with the same atomic connection migration.
    // migrated_profiles 使协议绑定的内部路由与同一次原子连接迁移保持一致。
    let mut migrated_profiles = configuration.inference_profiles.clone();
    for profile in migrated_profiles.values_mut() {
        for route in profile.llm_routes.values_mut() {
            migrate_custom_provider_inference_route(
                route,
                provider_id,
                &migrated_connection_ids,
                target,
            )?;
        }
    }
    configuration.connection_graph = migrated_graph;
    configuration.inference_profiles = migrated_profiles;
    Ok(())
}

/// Migrates one dependent conversation inference route without inventing a model protocol.
/// 在不虚构模型协议的前提下迁移一条依赖的会话推理路由。
///
/// # Parameters
/// 参数
/// `route` is one persisted internal route, `provider_id` and `connection_ids` define ownership,
/// and `target` supplies the new service-entry and enabled protocol identities.
/// `route` 是一条持久化内部路由，`provider_id` 与 `connection_ids` 定义归属，`target`
/// 提供新的服务入口与启用协议身份。
///
/// # Returns
/// 返回值
/// Success after updating the service identity. A unique protocol is migrated exactly; a
/// multi-protocol target preserves the old exact binding so any drift fails closed and remains
/// visible through route diagnostics until the user re-selects the model.
/// 更新服务身份后返回成功。唯一协议会被精确迁移；多协议目标保留旧精确绑定，使任何漂移
/// 失败封闭并通过路由诊断保持可见，直到用户重新选择模型。
fn migrate_custom_provider_inference_route(
    route: &mut InferenceRouteConfiguration,
    provider_id: &str,
    connection_ids: &BTreeSet<String>,
    target: &CustomProviderActivationTarget,
) -> Result<(), String> {
    if route.provider_id != provider_id || !connection_ids.contains(route.connection_id.as_str()) {
        return Ok(());
    }
    if route.operation != AiOperation::ConversationRespond {
        return Err(format!(
            "Custom provider inference route for connection `{}` cannot migrate non-conversation operation `{}`.",
            route.connection_id,
            route.operation.as_str()
        ));
    }
    let InferenceRouteBinding::Protocol {
        protocol_profile_id,
    } = &mut route.binding
    else {
        return Err(format!(
            "Custom provider conversation route for connection `{}` does not use a protocol binding.",
            route.connection_id
        ));
    };
    route.service_entry_id = target.service_entry_id.clone();
    // A unique target protocol is a proven model binding. With multiple protocols, the provider
    // default cannot prove this model's binding and must not rewrite the route.
    // 唯一目标协议能够证明模型绑定。存在多个协议时，供应商默认值无法证明该模型绑定，
    // 因此不得改写路由。
    if target.protocol_profile_ids.len() == 1 {
        *protocol_profile_id = target.default_protocol_profile_id.clone();
    }
    Ok(())
}

/// Executes one candidate discovery request while retaining its typed status for policy decisions.
/// 执行一次候选目录发现请求，并保留其类型化状态供策略决策使用。
///
/// # Parameters
/// 参数
/// `runtime` supplies an isolated observer, `validation` is the candidate configuration, and
/// `candidate` optionally supplies one in-memory temporary secret.
/// `runtime` 提供隔离观测器，`validation` 是候选配置，`candidate` 可选提供一份内存临时秘密。
///
/// # Returns
/// 返回值
/// Returns a typed observation even when the remote discovery request itself fails.
/// 即使远程发现请求失败，也返回类型化观测。
async fn observe_custom_provider_candidate(
    runtime: &PublicAiRuntime,
    validation: &PersistedAiConfiguration,
    candidate: &PreparedCustomProviderCandidate,
) -> Result<CustomProviderCandidateObservation, String> {
    // validation_connection is mandatory for every remote observation and contains no new durable state.
    // validation_connection 是每次远程观测的必需输入，且不包含新的持久化状态。
    let validation_connection = candidate.validation_connection.as_ref().ok_or_else(|| {
        format!(
            "Custom provider `{}` has no credential connection available for observation.",
            candidate.provider_id
        )
    })?;
    // observation lives only inside the adapter-owned isolated runtime; cancellation therefore
    // drops the temporary secret and every catalog projection without compensation work.
    // observation 仅存在于适配器持有的隔离运行时中；因此取消会直接丢弃临时秘密与全部目录投影，
    // 无需补偿操作。
    let observation = runtime
        .observe_connection_catalog_candidate(
            validation,
            validation_connection.id.as_str(),
            candidate.temporary_secret.as_ref(),
        )
        .await?;
    Ok(CustomProviderCandidateObservation {
        status: observation.status,
        catalog: observation.catalog,
        refresh_error: observation.refresh_error,
    })
}

/// Reports whether one closed discovery state supports an explicit manual-model fallback.
/// 报告一个封闭发现状态是否支持显式手工模型回退。
fn manual_fallback_available(state: CatalogDiscoveryState) -> bool {
    matches!(
        state,
        CatalogDiscoveryState::Empty
            | CatalogDiscoveryState::Unsupported
            | CatalogDiscoveryState::InvalidResponse
    )
}

/// Reports whether one closed discovery state may be saved for a later retry.
/// 报告一个封闭发现状态是否可以保存并稍后重试。
fn pending_validation_available(state: CatalogDiscoveryState) -> bool {
    matches!(
        state,
        CatalogDiscoveryState::RateLimited
            | CatalogDiscoveryState::TemporarilyUnavailable
            | CatalogDiscoveryState::Failed
    )
}

/// Reports whether an explicitly accepted non-ready observation must remain routing-isolated.
/// 报告一项被显式接受但未就绪的观测是否必须保持路由隔离。
///
/// # Parameters
/// 参数
/// `state` is the typed observation and both flags represent the user's exact save action.
/// `state` 是类型化观测，两个标志表示用户精确选择的保存动作。
///
/// # Returns
/// 返回值
/// `true` when a later explicit model test is required before normal routing.
/// 当正常路由前仍需执行一次显式模型测试时返回 `true`。
fn custom_provider_save_requires_pending_validation(
    state: CatalogDiscoveryState,
    allow_manual_fallback: bool,
    allow_pending_validation: bool,
) -> bool {
    (allow_pending_validation && pending_validation_available(state))
        || (allow_manual_fallback && manual_fallback_available(state))
}

/// Reports whether the observed catalog state matches one explicit user save action.
/// 报告观测到的目录状态是否与一项显式用户保存动作匹配。
///
/// # Parameters
/// 参数
/// `state` is the typed observation and both flags are explicit manual or deferred acceptance.
/// `state` 是类型化观测，两个标志分别表示显式手工接受或延期接受。
///
/// # Returns
/// 返回值
/// `true` only for ready discovery or a matching explicit acceptance path.
/// 仅在目录就绪或命中对应显式接受路径时返回 `true`。
fn custom_provider_observation_is_accepted(
    state: CatalogDiscoveryState,
    allow_manual_fallback: bool,
    allow_pending_validation: bool,
) -> bool {
    state == CatalogDiscoveryState::Ready
        || (allow_manual_fallback && manual_fallback_available(state))
        || (allow_pending_validation && pending_validation_available(state))
}

/// Probes one custom provider without persisting provider, credential, or catalog state.
/// 探测一个自定义供应商且不持久化供应商、凭据或目录状态。
///
/// # Parameters
/// 参数
/// `state` owns the runtime snapshot and `request` carries the candidate provider and credential.
/// `state` 持有运行时快照，`request` 携带候选供应商与凭据。
///
/// # Returns
/// 返回值
/// Returns typed discovery status and raw model facts from a state-free isolated observation.
/// 从不产生状态的隔离观测返回类型化发现状态与原始模型事实。
pub(super) async fn service_probe_public_ai_custom_provider(
    state: SharedServiceBridgeState,
    request: PublicAiCustomProviderSaveRequest,
) -> Result<PublicAiCustomProviderProbeResult, String> {
    let (runtime, current) = public_ai_runtime_and_configuration(&state)?;
    let candidate = prepare_custom_provider_candidate(&request, &current)?;
    let validation =
        custom_provider_validation_configuration(runtime.as_ref(), &current, &candidate)?;
    let observation =
        observe_custom_provider_candidate(runtime.as_ref(), &validation, &candidate).await?;
    Ok(PublicAiCustomProviderProbeResult {
        manual_fallback_available: manual_fallback_available(observation.status.state),
        pending_save_available: pending_validation_available(observation.status.state),
        status: observation.status,
        catalog: observation.catalog,
    })
}

/// Creates or updates one custom provider after typed credential-scoped discovery validation.
/// 在完成类型化凭据作用域发现验证后创建或更新一个自定义供应商。
pub(super) async fn service_save_public_ai_custom_provider(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomProviderSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let candidate = prepare_custom_provider_candidate(&request, &current_configuration)?;
    // connection_contract_changed prevents cosmetic provider edits from forcing account revalidation.
    // connection_contract_changed 防止仅修改供应商展示信息时强制重新验证账号。
    let connection_contract_changed =
        custom_provider_connection_contract_changed(&current_configuration, &candidate)?;
    // removed_protocols scopes destructive cleanup to the actual protocols the edit deletes;
    // additions never invalidate existing settings (plan §R6.8).
    // removed_protocols 把破坏性清理限定到编辑实际删除的协议；新增协议绝不让既有设置失效
    // （方案 §R6.8）。
    let removed_protocols = custom_provider_removed_protocols(&current_configuration, &candidate)?;
    // inactive_protocol_override_count identifies route settings owned by the removed protocols.
    // inactive_protocol_override_count 标识被移除协议拥有的路由设置。
    let inactive_protocol_override_count = current_configuration
        .removed_protocol_model_override_count(candidate.provider_id.as_str(), &removed_protocols);
    if inactive_protocol_override_count > 0 && !request.confirm_inactive_protocol_override_deletion
    {
        return Err(format!(
            "Removing protocol(s) `{}` from custom provider `{}` would deactivate {inactive_protocol_override_count} protocol-scoped model setting(s). Explicit confirmation is required to delete those incompatible settings.",
            removed_protocols
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            candidate.provider_id
        ));
    }
    // edit_validation_outcomes remains empty for creation and complete for every existing credential during edits.
    // edit_validation_outcomes 在新建时为空，在编辑时包含每份现有凭据的完整结果。
    let edit_validation_outcomes = if candidate.updating_existing && connection_contract_changed {
        validate_custom_provider_edit_connections(
            runtime.as_ref(),
            &current_configuration,
            &candidate,
            request.allow_manual_fallback,
            request.allow_pending_validation,
        )
        .await?
    } else {
        BTreeMap::new()
    };
    // newProviderObservation belongs only to the single newly created credential and remains
    // process-local until the non-cancellable commit section adopts it.
    // newProviderObservation 仅属于新建供应商时的单一凭据，并在不可取消提交区段接管前
    // 始终只存在于进程内。
    let (save_as_pending, new_provider_observation) = if candidate.updating_existing {
        (false, None)
    } else {
        // validation_configuration isolates the new provider and its not-yet-durable credential.
        // validation_configuration 隔离新供应商及其尚未持久化的凭据。
        let validation_configuration = custom_provider_validation_configuration(
            runtime.as_ref(),
            &current_configuration,
            &candidate,
        )?;
        // observation retains the new connection catalog for immediate durable adoption on success.
        // observation 在成功时保留新连接目录，以供持久化状态立即接管。
        let observation = observe_custom_provider_candidate(
            runtime.as_ref(),
            &validation_configuration,
            &candidate,
        )
        .await?;
        if !custom_provider_observation_is_accepted(
            observation.status.state,
            request.allow_manual_fallback,
            request.allow_pending_validation,
        ) {
            // diagnostic preserves the exact discovery reason for the explicit save choice.
            // diagnostic 为显式保存选择保留精确的目录发现原因。
            let diagnostic = observation
                .refresh_error
                .as_deref()
                .or(observation.status.message.as_deref())
                .unwrap_or_else(|| {
                    if observation.status.state == CatalogDiscoveryState::Empty {
                        "The provider returned an empty model catalog."
                    } else {
                        "The provider model catalog was not validated."
                    }
                });
            return Err(format!(
                "Custom provider validation requires an explicit supported action: {diagnostic}"
            ));
        }
        (
            custom_provider_save_requires_pending_validation(
                observation.status.state,
                request.allow_manual_fallback,
                request.allow_pending_validation,
            ),
            Some(observation),
        )
    };

    // commitTask owns the transaction once validation is complete. Dropping the caller's Future
    // detaches the JoinHandle but cannot interrupt secret creation, durable activation, or exact
    // compensation between those two boundaries.
    // commitTask 在验证完成后持有事务。丢弃调用方 Future 只会分离 JoinHandle，无法中断秘密创建、
    // 持久激活或两者边界之间的精确补偿。
    let commit_task = tokio::spawn(async move {
        // transaction remains held through every catalog, credential, configuration, and
        // Workbench projection owned by this save.
        // transaction 在本次保存持有的全部目录、凭据、配置与工作台投影完成前保持持有。
        let transaction = transaction;
        // providerId freezes the exact custom-provider identity moved into persistence.
        // providerId 冻结移入持久化流程的精确自定义供应商身份。
        let provider_id = candidate.provider_id.clone();
        // updatingExisting selects the closed create-versus-edit transaction path.
        // updatingExisting 选择封闭的新建或编辑事务路径。
        let updating_existing = candidate.updating_existing;
        // descriptor is the complete candidate provider contract.
        // descriptor 是完整的候选供应商契约。
        let descriptor = candidate.descriptor.clone();
        // validationConnection becomes durable only for a new provider.
        // validationConnection 仅在新供应商路径转为持久状态。
        let validation_connection = candidate.validation_connection.clone();
        // connectionTarget owns the route identity applied to every edited credential.
        // connectionTarget 持有应用到每份已编辑凭据的路由身份。
        let activation_target = candidate.activation_target.clone();
        // stagedObservations is complete before the first runtime staging write.
        // stagedObservations 在首次运行时暂存写入前已经完整。
        let mut staged_observations = Vec::new();
        if let Some(observation) = new_provider_observation {
            let connection = validation_connection.as_ref().ok_or_else(|| {
                String::from("A new custom provider requires its first credential connection.")
            })?;
            staged_observations.push(StagedConnectionCatalogObservation {
                connection_id: connection.id.clone(),
                catalog: observation.catalog,
                status: observation.status,
                refresh_error: observation.refresh_error,
            });
        }
        for (connection_id, outcome) in &edit_validation_outcomes {
            staged_observations.push(StagedConnectionCatalogObservation {
                connection_id: connection_id.clone(),
                catalog: outcome.catalog.clone(),
                status: outcome.status.clone(),
                refresh_error: outcome.refresh_error.clone(),
            });
        }
        // runtimeBeforeImage is the exact pre-save image for staging, configuration, and final
        // publication compensation.
        // runtimeBeforeImage 是暂存、配置与最终发布补偿共同使用的精确保存前镜像。
        let runtime_before_image = runtime.capture_activation_snapshot()?;
        // stagingBeforeImage is present only when this save mutates staged catalog observations.
        // stagingBeforeImage 仅在本次保存修改暂存目录观测时存在。
        let staging_before_image = if staged_observations.is_empty() {
            None
        } else {
            Some(runtime_before_image.clone())
        };
        // stagingAfterImage is produced by the atomic batch itself, so no fallible read can occur
        // after its first shared assignment.
        // stagingAfterImage 由原子批次自身产生，因此首次共享赋值后不会再发生可失败读取。
        let staging_after_image = if let Some(before_image) = staging_before_image.as_ref() {
            Some(runtime.stage_connection_catalog_observations_if_current(
                before_image,
                &staged_observations,
            )?)
        } else {
            None
        };
        // createdSecretReference is present only after a new provider's validated in-memory key
        // has crossed the durable credential boundary.
        // createdSecretReference 仅在新供应商的已验证内存 Key 越过持久凭据边界后存在。
        let created_secret_reference = if updating_existing {
            None
        } else {
            let connection = validation_connection.as_ref().ok_or_else(|| {
                String::from("A new custom provider requires its first credential connection.")
            })?;
            let secret = candidate.temporary_secret.as_ref().ok_or_else(|| {
                String::from("A new custom provider requires one validated credential secret.")
            })?;
            let secret_store = runtime.credential_secret_store();
            if let Err(primary) = CredentialSecretStore::create_secret(
                secret_store.as_ref(),
                connection.secret_reference.as_str(),
                secret,
            )
            .await
            {
                // stagingRestore is safe because the atomic batch returned its exact CAS image.
                // stagingRestore 是安全的，因为原子批次返回了其精确 CAS 镜像。
                let staging_restore = match (&staging_before_image, &staging_after_image) {
                    (Some(before), Some(after)) => {
                        runtime.restore_activation_snapshot(before, after)
                    }
                    (None, None) => Ok(()),
                    _ => Err(String::from(
                        "Custom provider staging compensation lost one activation image.",
                    )),
                };
                return Err(compensated_operation_error(
                    primary,
                    Ok(()),
                    staging_restore,
                ));
            }
            Some(connection.secret_reference.clone())
        };

        let persist_result = mutate_public_ai_configuration_in_transaction(
            &state,
            "保存自定义供应商失败",
            |configuration| {
                configuration
                    .custom_providers
                    .insert(provider_id.clone(), descriptor.clone());
                // effective_providers validates every connection against the newly selected single protocol.
                // effective_providers 根据新选择的唯一协议校验每一条连接。
                let effective_providers = runtime.effective_providers(configuration)?;
                if !updating_existing {
                    // connection is guaranteed by new-provider preparation and becomes durable only in this branch.
                    // connection 由新供应商准备流程保证存在，并且仅在此分支转为持久状态。
                    let connection = validation_connection.clone().ok_or_else(|| {
                        String::from(
                            "A new custom provider requires its first credential connection.",
                        )
                    })?;
                    configuration
                        .connection_graph
                        .add_connection(&effective_providers, connection.clone())?;
                    configuration.set_connection_pending_validation(
                        connection.id.as_str(),
                        save_as_pending,
                    )?;
                } else if connection_contract_changed {
                    migrate_custom_provider_connections(
                        &effective_providers,
                        configuration,
                        provider_id.as_str(),
                        &activation_target,
                        &edit_validation_outcomes,
                    )?;
                }
                if updating_existing && connection_contract_changed {
                    for (connection_id, outcome) in &edit_validation_outcomes {
                        configuration.set_connection_pending_validation(
                            connection_id.as_str(),
                            outcome.pending_validation,
                        )?;
                    }
                }
                if !removed_protocols.is_empty() {
                    // Cascade cleanup is scoped to the removed protocol set: overrides owned by
                    // removed protocols are deleted. Explicit model preferences that point at a
                    // removed protocol are intentionally retained so binding resolution keeps
                    // failing closed (ExplicitProtocolDisabled) and the model stays blocked until
                    // the user re-selects; silently dropping the preference would fall back to
                    // Auto (plan §R6.8).
                    // 级联清理限定到被删除协议集合：被删协议拥有的覆盖被删除。指向被删协议的
                    // 显式模型偏好被刻意保留，使绑定解析持续失败封闭（ExplicitProtocolDisabled）
                    // 且模型保持 blocked 直到用户重选；静默删除偏好会回退到 Auto（方案 §R6.8）。
                    configuration.remove_removed_protocol_model_overrides(
                        provider_id.as_str(),
                        &removed_protocols,
                    );
                }
                if connection_contract_changed || !updating_existing {
                    // candidateCatalogs uses the validated staged sources without publishing them;
                    // the active selection is reconciled inside this single durable image.
                    // candidateCatalogs 使用已验证暂存来源而不发布；活动选择会在同一份持久镜像内协调。
                    let candidate_catalogs =
                        runtime.preview_staged_model_catalog_overlays(configuration)?;
                    ensure_active_model_selection(
                        runtime.as_ref(),
                        configuration,
                        &candidate_catalogs,
                    )?;
                }
                Ok(())
            },
        );
        let configuration = match persist_result {
            Ok(configuration) => configuration,
            Err(error) => {
                // secretCleanup removes only the UUID-scoped secret created by this exact task.
                // secretCleanup 仅移除由当前精确任务创建的 UUID 作用域秘密。
                let secret_cleanup =
                    if let Some(secret_reference) = created_secret_reference.as_deref() {
                        CredentialSecretStore::delete_secret(
                            runtime.credential_secret_store().as_ref(),
                            secret_reference,
                        )
                        .await
                    } else {
                        Ok(())
                    };
                // stagingRestore returns every staged source to its exact before-image because
                // the candidate never crossed the durable activation boundary.
                // stagingRestore 把每项暂存来源恢复到精确前镜像，因为候选从未越过持久激活边界。
                let staging_restore = match (&staging_before_image, &staging_after_image) {
                    (Some(before), Some(after)) => {
                        runtime.restore_activation_snapshot(before, after)
                    }
                    (None, None) => Ok(()),
                    _ => Err(String::from(
                        "Custom provider staging compensation lost one activation image.",
                    )),
                };
                return Err(compensated_operation_error(
                    error,
                    secret_cleanup,
                    staging_restore,
                ));
            }
        };
        // candidateSnapshot is built before Workbench publication so no fallible response
        // projection remains after browser-visible state changes.
        // candidateSnapshot 在工作台发布前构建，确保浏览器可见状态改变后不再存在可失败响应投影。
        let candidate_snapshot =
            build_public_ai_settings_snapshot(runtime.as_ref(), &configuration);
        // publicationResult commits the browser projection only after the response snapshot is complete.
        // publicationResult 仅在响应快照完整后提交浏览器投影。
        let publication_result = candidate_snapshot.and_then(|snapshot| {
            publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)
                .map(|_| snapshot)
        });
        match publication_result {
            Ok(snapshot) => {
                drop(transaction);
                Ok(snapshot)
            }
            Err(primary) => {
                // configurationRestore returns the durable custom manifests and central document
                // to the exact pre-save snapshot before the runtime maps are restored exactly.
                // configurationRestore 在精确恢复运行时映射前，把持久自定义清单与中央文档恢复到
                // 保存前快照。
                let configuration_restore = mutate_public_ai_configuration_in_transaction(
                    &state,
                    "恢复自定义供应商保存前配置失败",
                    |configuration| {
                        *configuration = current_configuration.clone();
                        Ok(())
                    },
                )
                .map(|_| ());
                // runtimeRestore uses the current post-compensation image as the CAS compare side;
                // a concurrent catalog mutation is reported instead of being overwritten.
                // runtimeRestore 使用当前补偿后镜像作为 CAS 比较端；并发目录变更会被报告，
                // 而不会遭到覆盖。
                let runtime_restore =
                    runtime
                        .capture_activation_snapshot()
                        .and_then(|current_image| {
                            runtime
                                .restore_activation_snapshot(&runtime_before_image, &current_image)
                        });
                // stateRestore combines durable/configuration and exact runtime restoration into
                // one deterministic compensation result.
                // stateRestore 把持久配置恢复与精确运行时恢复组合为一个确定性补偿结果。
                let state_restore = match (configuration_restore, runtime_restore) {
                    (Ok(()), Ok(())) => Ok(()),
                    (configuration, runtime) => Err(format!(
                        "Custom provider save state restoration was incomplete: configuration={configuration:?}; runtime={runtime:?}"
                    )),
                };
                // secretCleanup removes only the UUID-scoped secret introduced by this candidate.
                // secretCleanup 仅移除由当前候选引入的 UUID 作用域秘密。
                let secret_cleanup =
                    if let Some(secret_reference) = created_secret_reference.as_deref() {
                        CredentialSecretStore::delete_secret(
                            runtime.credential_secret_store().as_ref(),
                            secret_reference,
                        )
                        .await
                    } else {
                        Ok(())
                    };
                let compensated =
                    compensated_operation_error(primary, secret_cleanup, state_restore);
                drop(transaction);
                Err(compensated)
            }
        }
    });
    commit_task
        .await
        .map_err(|error| format!("Custom provider commit task failed: {error}"))?
}

/// Executes one explicit minimal model request and releases a pending credential only on success.
/// 执行一次显式最小模型请求，并且仅在成功后解除待验证凭据隔离。
///
/// # Parameters
/// 参数
/// `app` publishes the eligible workbench after success, `state` owns runtime/configuration, and
/// `request` freezes the exact provider, credential, and model selected by the user.
/// `app` 在成功后发布可用工作台，`state` 持有运行时与配置，`request` 冻结用户选择的精确供应商、凭据与模型。
///
/// # Returns
/// 返回值
/// Returns compact response facts only after a terminal stream with visible text.
/// 仅在流以可见文本成功终结后返回紧凑响应事实。
pub(super) async fn service_validate_public_ai_custom_model(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelValidationRequest,
) -> Result<PublicAiCustomModelValidationResult, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    let connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .ok_or_else(|| format!("Connection `{}` is not configured.", request.connection_id))?;
    if connection.provider_id != request.provider_id {
        return Err(format!(
            "Connection `{}` does not belong to custom provider `{}`.",
            request.connection_id, request.provider_id
        ));
    }

    // validation_configuration bypasses only the pending/workbench gates for this explicit action.
    // validation_configuration 仅为此次显式操作绕过待验证与工作台开关门禁。
    let mut validation_configuration = current_configuration.clone();
    validation_configuration
        .pending_validation_connection_ids
        .remove(request.connection_id.as_str());
    validation_configuration.model_enabled_overrides.insert(
        PersistedAiConfiguration::provider_model_key(
            request.provider_id.as_str(),
            request.model_id.as_str(),
        ),
        true,
    );
    let selection = runtime.resolve_connection_model_selection(
        &validation_configuration,
        request.connection_id.as_str(),
        request.provider_id.as_str(),
        request.model_id.as_str(),
    )?;
    let output_limit = selection
        .overrides
        .output_limit
        .or(selection.model.capabilities.output_limit)
        .unwrap_or(256)
        .clamp(1, 256);
    let reasoning_effort = selection
        .model
        .capabilities
        .reasoning_efforts
        .contains(&ReasoningEffort::None)
        .then_some(ReasoningEffort::None);
    // Hidden reasoning is meaningful only when the exact route activates a reasoning effort.
    // 仅当精确路由启用了思考档位时，隐藏思考才有明确语义。
    let reasoning_output = if reasoning_effort.is_some() {
        ReasoningOutputPolicy::Hidden
    } else {
        ReasoningOutputPolicy::ProviderDefault
    };
    // Native attempt persistence requires the logical execution identity to remain a canonical UUID.
    // 原生真实尝试持久化要求逻辑执行标识保持为规范 UUID。
    let execution_id = uuid::Uuid::new_v4().to_string();
    let execution_request = AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: selection.model.id.clone(),
        cache_scope_id: None,
        continuation: None,
        messages: vec![AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: String::from("Reply with exactly OK."),
            }],
            provider_context_items: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: ToolChoicePolicy::Disabled,
        parallel_tool_calls: ParallelToolCallPolicy::Disabled,
        structured_output: StructuredOutputPolicy::None,
        stop_sequences: Vec::new(),
        prompt_cache: selection.overrides.prompt_cache.clone().unwrap_or_default(),
        reasoning_effort,
        reasoning_output,
        model_parameters: selection.overrides.model_parameters.clone(),
        resources: Vec::new(),
        output_token_limit: Some(output_limit),
    };
    let configured_inference_service = configured_inference_service_for_settings(&state)?;
    let response_text = configured_inference_service
        .execute_model_validation(&selection, execution_request, execution_id)
        .await
        .map_err(|error| error.to_string())?;

    let provider_id = request.provider_id.clone();
    let connection_id = request.connection_id.clone();
    let (_configuration, ()) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存自定义模型验证结果失败",
        |configuration| {
            let current_connection = configuration
                .connection_graph
                .connections
                .get(connection_id.as_str())
                .ok_or_else(|| format!("Connection `{connection_id}` is no longer configured."))?;
            if current_connection.provider_id != provider_id
                || current_connection.service_entry_id
                    != selection.route.connection.service_entry_id
                || current_connection.credential_profile_id
                    != selection.route.connection.credential_profile_id
            {
                return Err(format!(
                    "Connection `{connection_id}` changed while model validation was running."
                ));
            }
            configuration.set_connection_pending_validation(connection_id.as_str(), false)?;
            configuration
                .connection_graph
                .account_preferences
                .provider_defaults
                .entry(provider_id.clone())
                .or_insert_with(|| connection_id.clone());
            Ok(())
        },
        |_configuration| Ok(()),
    )?;
    drop(transaction);
    Ok(PublicAiCustomModelValidationResult {
        provider_id: request.provider_id,
        connection_id: request.connection_id,
        model_id: request.model_id,
        response_characters: response_text.chars().count(),
        response_text,
    })
}

/// Removes one custom provider together with every provider-owned setting after confirmation.
/// 确认后移除一个自定义供应商及其持有的全部设置。
///
/// # Parameters
/// 参数
/// `app` publishes the refreshed workbench, `state` owns persisted configuration, and `request` identifies the confirmed provider removal.
/// `app` 发布刷新后的工作台，`state` 持有持久化配置，`request` 标识已确认的供应商移除请求。
///
/// # Returns
/// 返回值
/// Returns the refreshed settings snapshot, or an atomic cleanup error with credential compensation.
/// 返回刷新后的设置快照，或包含凭据补偿结果的原子清理错误。
pub(super) async fn service_remove_public_ai_custom_provider(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomProviderRemoveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    let provider_id = request.provider_id;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // isolatedProvider is identified by the exact standalone-reference/descriptor difference,
    // allowing the explicit repair action even though no invalid descriptor entered the registry.
    // isolatedProvider 由精确的独立引用与描述符差集识别，因此即使无效描述符未进入注册表，
    // 显式修复动作仍可执行。
    validate_removable_custom_provider_identity(&current_configuration, provider_id.as_str())?;
    let provider_connections = current_configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .cloned()
        .collect::<Vec<_>>();
    if !provider_connections.is_empty() && !request.confirm_credential_deletion {
        return Err(format!(
            "Custom provider `{provider_id}` still owns {} credential(s). Explicit confirmation is required to delete those credentials together with the provider.",
            provider_connections.len()
        ));
    }
    let catalogs = runtime.catalog_snapshot()?;
    // runtimeBeforeImage owns every catalog, usage, status, and cache field removed below.
    // runtimeBeforeImage 持有下方将删除的全部目录、用量、状态与缓存字段。
    let runtime_before_image = runtime.capture_activation_snapshot()?;
    let secret_store = runtime.credential_secret_store();
    let deleted_secrets = if provider_connections.is_empty() {
        BTreeMap::new()
    } else {
        delete_provider_credential_secrets(secret_store.as_ref(), &provider_connections).await?
    };
    let remove_result = mutate_public_ai_configuration_in_transaction(
        &state,
        "移除自定义供应商失败",
        |configuration| {
            let was_isolated =
                configuration.custom_provider_manifest_is_isolated(provider_id.as_str());
            // Removing the provider default first lets every provider connection be detached without a fake replacement.
            // 先移除供应商默认连接，确保后续逐个解绑连接时不需要伪造替代默认连接。
            configuration
                .connection_graph
                .account_preferences
                .provider_defaults
                .remove(provider_id.as_str());
            for connection in &provider_connections {
                remove_connection_from_configuration(configuration, connection.id.as_str(), None)?;
            }
            // Confirmed provider removal owns the complete cleanup of model settings and internal routes.
            // 已确认的供应商移除负责完整清理模型设置与内部路由。
            // Custom-provider removal must clear both model settings and hydrated standalone
            // manifest ownership before this exact candidate is installed in the runtime.
            // 自定义供应商移除必须在该精确候选安装到运行时前，同时清理模型设置与已补水
            // 独立 Manifest 所有权。
            let removed = configuration.remove_custom_provider_owned_state(provider_id.as_str());
            match removed {
                Some(removed) if removed.kind != ProviderKind::Custom => {
                    return Err(format!(
                        "Provider `{provider_id}` is not a user-defined provider."
                    ));
                }
                Some(_) => {}
                None if was_isolated => {}
                None => {
                    return Err(format!(
                        "Custom provider `{provider_id}` is not configured."
                    ));
                }
            }
            ensure_active_model_selection(runtime.as_ref(), configuration, &catalogs)?;
            Ok(())
        },
    );
    let configuration = match remove_result {
        Ok(configuration) => configuration,
        Err(error) => {
            let restore =
                restore_provider_credential_secrets(secret_store.as_ref(), &deleted_secrets).await;
            let activation_restore = restore_public_ai_configuration_activation_in_transaction(
                &state,
                runtime.as_ref(),
                &current_configuration,
                &runtime_before_image,
                "恢复自定义供应商移除前配置失败",
            );
            return Err(compensated_operation_error(
                error,
                restore,
                activation_restore,
            ));
        }
    };
    // fallback is the exact post-removal global selection applied to revoked sessions.
    // fallback 是应用于已撤销会话的精确移除后全局选择。
    let fallback = configuration.active.clone();
    let publication_result = (|| {
        for connection in &provider_connections {
            runtime.remove_connection_catalog(connection.id.as_str())?;
            runtime.remove_connection_usage(connection.id.as_str())?;
        }
        // snapshot completes every fallible response projection before Service state changes.
        // snapshot 在 Service 状态改变前完成全部可失败响应投影。
        let snapshot = build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)?;
        publish_public_ai_workbench_with_session_reconciliation_and_commit(
            &app,
            &state,
            runtime.as_ref(),
            &configuration,
            |workspace| {
                Ok(reconcile_explicitly_revoked_session_model_selections(
                    workspace,
                    fallback.as_ref(),
                    |_session_id, runtime_state| {
                        runtime_state.active_provider_id.as_deref() == Some(provider_id.as_str())
                    },
                ))
            },
            "custom_provider_removed",
            || Ok(()),
        )?;
        Ok::<_, String>(snapshot)
    })();
    match publication_result {
        Ok(snapshot) => {
            drop(transaction);
            Ok(snapshot)
        }
        Err(primary) => {
            let secret_restore =
                restore_provider_credential_secrets(secret_store.as_ref(), &deleted_secrets).await;
            let activation_restore = restore_public_ai_configuration_activation_in_transaction(
                &state,
                runtime.as_ref(),
                &current_configuration,
                &runtime_before_image,
                "恢复自定义供应商移除前配置失败",
            );
            drop(transaction);
            Err(compensated_operation_error(
                primary,
                secret_restore,
                activation_restore,
            ))
        }
    }
}

/// Replaces the user-owned model definitions for one custom provider.
/// 替换一个自定义供应商的用户自有模型定义。
///
/// # Parameters
/// 参数
///
/// `app` publishes the refreshed workbench, `state` owns persisted configuration and runtime
/// catalogs, and `request` carries the complete replacement set.
/// `app` 发布刷新的工作台，`state` 持有持久化配置与运行时目录，`request` 携带完整替换集合。
///
/// # Returns
/// 返回值
///
/// Returns the rebuilt browser snapshot after validation and overlay reapplication.
/// 校验并重新应用覆盖后返回重建的浏览器快照。
pub(super) async fn service_save_public_ai_custom_models(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelsSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let provider = current_configuration
        .custom_providers
        .get(request.provider_id.as_str())
        .ok_or_else(|| {
            format!(
                "Custom provider `{}` is not configured.",
                request.provider_id
            )
        })?;
    if provider.kind != ProviderKind::Custom {
        return Err(format!(
            "Provider `{}` is not a user-defined provider.",
            request.provider_id
        ));
    }

    let mut replacement = BTreeMap::new();
    for model in request.models {
        model.validate()?;
        let model_id = model.id.clone();
        if replacement.insert(model_id.clone(), model).is_some() {
            return Err(format!(
                "Custom provider `{}` contains duplicate model `{model_id}`.",
                request.provider_id
            ));
        }
    }
    // seen_template_decisions rejects ambiguous duplicate decisions before persistence begins.
    // seen_template_decisions 在开始持久化前拒绝含义不明确的重复决策。
    let mut seen_template_decisions = BTreeSet::new();
    // template_decisions contains validated composite keys ready for the single configuration transaction.
    // template_decisions 包含已校验的组合键，可直接进入单次配置事务。
    let mut template_decisions = Vec::with_capacity(request.template_decisions.len());
    for item in request.template_decisions {
        validate_non_empty("templateDecisions[].modelId", item.model_id.as_str())?;
        if !seen_template_decisions.insert(item.model_id.clone()) {
            return Err(format!(
                "templateDecisions contains duplicate model `{}`.",
                item.model_id
            ));
        }
        if !replacement.contains_key(item.model_id.as_str()) {
            return Err(format!(
                "templateDecisions references model `{}` outside the replacement model set.",
                item.model_id
            ));
        }
        validate_custom_template_decision(item.model_id.as_str(), &item.decision)?;
        template_decisions.push((
            PersistedAiConfiguration::provider_model_key(
                request.provider_id.as_str(),
                item.model_id.as_str(),
            ),
            item.decision,
        ));
    }
    let removed_model_ids = current_configuration
        .custom_models
        .get(request.provider_id.as_str())
        .into_iter()
        .flat_map(|models| models.keys())
        .filter(|model_id| !replacement.contains_key(model_id.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    let provider_id = request.provider_id;
    // candidate_configuration is authoritative for deciding whether a removed manual overlay
    // also removes the executable model from an exact credential catalog.
    // candidate_configuration 是判断移除手工覆盖后模型是否也从精确凭据目录消失的权威状态。
    let mut candidate_configuration = current_configuration.clone();
    apply_custom_model_replacement(
        &mut candidate_configuration,
        provider_id.as_str(),
        &replacement,
        &removed_model_ids,
        &template_decisions,
    );
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let referenced_removed_models = referenced_custom_model_ids(
        runtime.as_ref(),
        &candidate_configuration,
        provider_id.as_str(),
        &removed_model_ids,
    )?;
    // Session locks are runtime-owned and must block deletion even when another account still exposes the same model.
    // 会话锁定关系由运行时持有，即使其他账号仍暴露同名模型，也必须阻止删除。
    // locked_session_model_ids is copied while holding service state so catalog resolution never
    // nests runtime locks under the workspace mutex.
    // locked_session_model_ids 在持有服务状态锁时完成复制，避免在工作区互斥锁内嵌套目录锁。
    let locked_session_model_ids = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .workspace_snapshot
            .as_ref()
            .into_iter()
            .flat_map(|workspace| {
                workspace
                    .session_runtime_state
                    .values()
                    .chain(workspace.archived_session_runtime_state.values())
            })
            .filter(|runtime_state| {
                runtime_state.active_provider_id.as_deref() == Some(provider_id.as_str())
                    && runtime_state
                        .active_model_id
                        .as_deref()
                        .is_some_and(|model_id| removed_model_ids.contains(model_id))
            })
            .filter_map(|runtime_state| runtime_state.active_model_id.clone())
            .collect::<BTreeSet<_>>()
    };
    if !locked_session_model_ids.is_empty() {
        return Err(format!(
            "Custom models are locked by active or archived sessions and cannot be removed: {}. Switch those sessions to another model first.",
            locked_session_model_ids
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !referenced_removed_models.is_empty() {
        return Err(format!(
            "Custom models still referenced by an active or internal route cannot be removed: {}.",
            referenced_removed_models
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存自定义模型失败",
        |configuration| {
            apply_custom_model_replacement(
                configuration,
                provider_id.as_str(),
                &replacement,
                &removed_model_ids,
                &template_decisions,
            );
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Replaces the per-model protocol preferences for one custom provider.
/// 替换一个自定义供应商的逐模型协议偏好。
///
/// # Parameters
/// 参数
/// `app` publishes the refreshed workbench, `state` owns persisted configuration and runtime
/// catalogs, and `request` carries the complete replacement preference set.
/// `app` 发布刷新的工作台，`state` 持有持久化配置与运行时目录，`request` 携带完整替换偏好集合。
///
/// # Returns
/// 返回值
/// Returns the rebuilt browser snapshot after validation and persistence.
/// 校验并持久化后返回重建的浏览器快照。
pub(super) async fn service_save_public_ai_custom_model_protocol_preferences(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelProtocolPreferencesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let provider = current_configuration
        .custom_providers
        .get(request.provider_id.as_str())
        .ok_or_else(|| {
            format!(
                "Custom provider `{}` is not configured.",
                request.provider_id
            )
        })?;
    if provider.kind != ProviderKind::Custom {
        return Err(format!(
            "Provider `{}` is not a user-defined provider.",
            request.provider_id
        ));
    }
    // enabled_protocols is the authoritative set every explicit preference must reference.
    // enabled_protocols 是每个显式偏好必须引用的权威协议集合。
    let enabled_protocols = provider
        .service_entries
        .iter()
        .flat_map(|entry| entry.protocol_routes.iter())
        .map(|route| route.protocol_profile_id.clone())
        .collect::<BTreeSet<_>>();
    if enabled_protocols.is_empty() {
        return Err(String::from(
            "Custom provider must declare at least one enabled protocol before saving model preferences.",
        ));
    }
    let preferences = validate_custom_model_protocol_preferences(
        request.provider_id.as_str(),
        &enabled_protocols,
        &request.preferences,
    )?;
    let provider_id = request.provider_id;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存自定义模型协议偏好失败",
        |configuration| {
            let provider_model_prefix = format!("{}:{provider_id}:", provider_id.len());
            configuration
                .custom_model_protocol_preferences
                .retain(|key, _| !key.starts_with(provider_model_prefix.as_str()));
            configuration
                .custom_model_protocol_preferences
                .extend(preferences.clone());
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    // Rebuilding the catalogs re-freezes every custom protocol binding at the activation
    // boundary so the saved preferences take effect without a manual refresh (plan §13.6).
    // 重建目录会在激活边界重新冻结全部自定义协议绑定，使已保存偏好无需手动刷新即生效
    // （方案 §13.6）。
    drop(transaction);
    Ok(snapshot)
}

/// Applies one complete custom-model replacement to a candidate or durable configuration.
/// 将一份完整自定义模型替换应用到候选配置或持久配置。
///
/// # Parameters
/// 参数
/// `configuration` is the mutation target, `provider_id` owns all records, `replacement` is the
/// complete model set, `removed_model_ids` scopes cleanup, and `template_decisions` contains
/// already validated composite keys.
/// `configuration` 是修改目标，`provider_id` 持有全部记录，`replacement` 是完整模型集合，
/// `removed_model_ids` 限定清理范围，`template_decisions` 包含已经校验的组合键。
fn apply_custom_model_replacement(
    configuration: &mut PersistedAiConfiguration,
    provider_id: &str,
    replacement: &BTreeMap<String, CustomModelDefinition>,
    removed_model_ids: &BTreeSet<String>,
    template_decisions: &[(String, CustomModelTemplateDecision)],
) {
    if replacement.is_empty() {
        configuration.custom_models.remove(provider_id);
    } else {
        configuration
            .custom_models
            .insert(provider_id.to_string(), replacement.clone());
    }
    for model_id in removed_model_ids {
        configuration.remove_model_owned_settings(provider_id, model_id.as_str());
    }
    for (key, decision) in template_decisions {
        configuration
            .custom_model_template_decisions
            .insert(key.clone(), decision.clone());
    }
}

/// Saves provider-wide model defaults and rebuilds every affected in-memory catalog.
/// 保存供应商级模型默认值并重建所有受影响的内存目录。
pub(super) async fn service_save_public_ai_custom_provider_model_defaults(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomProviderModelDefaultsSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    if request
        .defaults
        .as_ref()
        .is_some_and(|defaults| !defaults.output_resources.is_empty())
    {
        return Err(String::from(
            "Custom provider output is fixed to text in the generic provider settings contract.",
        ));
    }
    if let Some(defaults) = request.defaults.as_ref() {
        defaults.validate()?;
    }
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    // provider_id identifies the exact custom-provider settings layer being replaced.
    // provider_id 标识要被替换的精确自定义供应商设置层。
    let provider_id = request.provider_id;
    // defaults collapses a value equal to the system baseline into an absent provider layer.
    // defaults 将与系统基线相同的值折叠为不存在的供应商层。
    let defaults = request
        .defaults
        .filter(|defaults| *defaults != CustomProviderModelDefaults::default());
    let mut candidate_configuration = current_configuration.clone();
    replace_custom_provider_model_defaults(
        &mut candidate_configuration,
        provider_id.as_str(),
        defaults.as_ref(),
    );
    candidate_configuration.validate_context_window_policy_layers()?;
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存自定义供应商模型默认值失败",
        |configuration| {
            replace_custom_provider_model_defaults(
                configuration,
                provider_id.as_str(),
                defaults.as_ref(),
            );
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Replaces or removes one explicit custom-provider model-settings layer.
/// 替换或移除一层显式自定义供应商模型设置。
///
/// # Parameters
/// 参数
/// `configuration` owns persisted state, `provider_id` identifies the layer, and `defaults`
/// contains the replacement or is absent for restoration.
/// `configuration` 持有持久化状态，`provider_id` 标识设置层，`defaults` 包含替换值或在还原时为空。
fn replace_custom_provider_model_defaults(
    configuration: &mut PersistedAiConfiguration,
    provider_id: &str,
    defaults: Option<&CustomProviderModelDefaults>,
) {
    if let Some(defaults) = defaults {
        configuration
            .custom_provider_model_defaults
            .insert(provider_id.to_string(), defaults.clone());
    } else {
        configuration
            .custom_provider_model_defaults
            .remove(provider_id);
    }
}

/// Returns offline template suggestions without network access or persistent mutation.
/// 在不访问网络且不修改持久化状态的情况下返回离线模板建议。
pub(super) fn service_public_ai_custom_model_template_suggestions(
    request: PublicAiCustomModelTemplateSuggestionsRequest,
) -> Result<Vec<CommonModelTemplateSuggestion>, String> {
    validate_non_empty("modelId", request.model_id.as_str())?;
    Ok(vulcan_provider_adapters::suggest_common_model_templates(
        request.model_id.as_str(),
    ))
}

/// Returns offline template suggestions for distinct model identifiers in one bounded request.
/// 在一次有界请求中返回互不重复模型标识的离线模板建议。
///
/// # Parameters
/// 参数
/// `request` contains exact model identifiers from one browser-visible provider union.
/// `request` 包含一个浏览器可见供应商并集中的精确模型标识。
///
/// # Returns
/// 返回值
/// Ordered suggestion records without network access or persistent mutation.
/// 返回有序建议记录，且不访问网络、不修改持久化状态。
pub(super) fn service_public_ai_custom_model_template_suggestions_batch(
    request: PublicAiCustomModelTemplateSuggestionsBatchRequest,
) -> Result<Vec<PublicAiCustomModelTemplateSuggestionsRecord>, String> {
    if request.model_ids.is_empty() {
        return Err(String::from(
            "modelIds must contain at least one model identifier.",
        ));
    }
    if request.model_ids.len() > MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE {
        return Err(format!(
            "modelIds exceeds the maximum batch size of {MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE}."
        ));
    }
    let mut seen = BTreeSet::new();
    let mut records = Vec::with_capacity(request.model_ids.len());
    for model_id in request.model_ids {
        validate_non_empty("modelIds[]", model_id.as_str())?;
        if !seen.insert(model_id.clone()) {
            return Err(format!("modelIds contains duplicate model `{model_id}`."));
        }
        records.push(PublicAiCustomModelTemplateSuggestionsRecord {
            suggestions: vulcan_provider_adapters::suggest_common_model_templates(
                model_id.as_str(),
            ),
            model_id,
        });
    }
    Ok(records)
}

/// Saves one explicit template decision and rebuilds affected model snapshots.
/// 保存一项显式模板决策并重建受影响的模型快照。
pub(super) async fn service_save_public_ai_custom_model_template_decision(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelTemplateDecisionSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    validate_custom_provider_model_exists(
        runtime.as_ref(),
        &current_configuration,
        request.provider_id.as_str(),
        request.model_id.as_str(),
    )?;
    if let Some(decision) = request.decision.as_ref() {
        validate_custom_template_decision(request.model_id.as_str(), decision)?;
    }
    let key = PersistedAiConfiguration::provider_model_key(
        request.provider_id.as_str(),
        request.model_id.as_str(),
    );
    let decision = request.decision;
    let mut candidate_configuration = current_configuration.clone();
    if let Some(decision) = decision.clone() {
        candidate_configuration
            .custom_model_template_decisions
            .insert(key.clone(), decision);
    } else {
        candidate_configuration
            .custom_model_template_decisions
            .remove(key.as_str());
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存自定义模型模板决策失败",
        |configuration| {
            if let Some(decision) = decision.clone() {
                configuration
                    .custom_model_template_decisions
                    .insert(key.clone(), decision);
            } else {
                configuration
                    .custom_model_template_decisions
                    .remove(key.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Atomically saves a bounded set of explicit template decisions and rebuilds catalogs once.
/// 原子保存一组有界显式模板决策，并且只重建一次目录。
///
/// # Parameters
/// 参数
/// `app` publishes the rebuilt workbench, `state` owns runtime and persistence, and `request`
/// identifies one custom provider plus distinct reviewed decisions.
/// `app` 发布重建后的工作台，`state` 持有运行时与持久化状态，`request`
/// 标识一个自定义供应商及互不重复的已审查决策。
///
/// # Returns
/// 返回值
/// The rebuilt browser snapshot after all decisions pass validation and persist atomically.
/// 所有决策通过校验并原子持久化后返回重建的浏览器快照。
pub(super) async fn service_save_public_ai_custom_model_template_decisions_batch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelTemplateDecisionsBatchSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    if request.decisions.is_empty() {
        return Err(String::from(
            "decisions must contain at least one reviewed template decision.",
        ));
    }
    if request.decisions.len() > MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE {
        return Err(format!(
            "decisions exceeds the maximum batch size of {MAX_CUSTOM_MODEL_TEMPLATE_BATCH_SIZE}."
        ));
    }
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    let mut seen = BTreeSet::new();
    let mut decisions = Vec::with_capacity(request.decisions.len());
    for item in request.decisions {
        validate_non_empty("decisions[].modelId", item.model_id.as_str())?;
        if !seen.insert(item.model_id.clone()) {
            return Err(format!(
                "decisions contains duplicate model `{}`.",
                item.model_id
            ));
        }
        validate_custom_provider_model_exists(
            runtime.as_ref(),
            &current_configuration,
            request.provider_id.as_str(),
            item.model_id.as_str(),
        )?;
        validate_custom_template_decision(item.model_id.as_str(), &item.decision)?;
        decisions.push((
            PersistedAiConfiguration::provider_model_key(
                request.provider_id.as_str(),
                item.model_id.as_str(),
            ),
            item.decision,
        ));
    }
    let mut candidate_configuration = current_configuration.clone();
    for (key, decision) in &decisions {
        candidate_configuration
            .custom_model_template_decisions
            .insert(key.clone(), decision.clone());
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "批量保存自定义模型模板决策失败",
        |configuration| {
            for (key, decision) in decisions.clone() {
                configuration
                    .custom_model_template_decisions
                    .insert(key, decision);
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one per-account manual-model availability difference and rebuilds effective catalogs.
/// 保存一项逐账号手工模型可用性差异并重建有效目录。
pub(super) async fn service_save_public_ai_custom_model_connection_availability(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomModelConnectionAvailabilitySaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential connection `{}` is not configured.",
                request.connection_id
            )
        })?;
    validate_custom_provider_identity(&current_configuration, connection.provider_id.as_str())?;
    if current_configuration
        .custom_model(connection.provider_id.as_str(), request.model_id.as_str())
        .is_none()
    {
        return Err(format!(
            "Custom provider `{}` has no manual model `{}`.",
            connection.provider_id, request.model_id
        ));
    }
    validate_custom_model_connection_availability_choice(
        &current_configuration,
        connection.provider_id.as_str(),
        request.model_id.as_str(),
        request.enabled,
    )?;
    let key = PersistedAiConfiguration::connection_model_key(
        request.connection_id.as_str(),
        request.model_id.as_str(),
    );
    let enabled = request.enabled;
    let mut candidate_configuration = current_configuration.clone();
    if let Some(enabled) = enabled {
        candidate_configuration
            .custom_model_connection_enabled_overrides
            .insert(key.clone(), enabled);
    } else {
        candidate_configuration
            .custom_model_connection_enabled_overrides
            .remove(key.as_str());
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存自定义模型账号可用性失败",
        |configuration| {
            if let Some(enabled) = enabled {
                configuration
                    .custom_model_connection_enabled_overrides
                    .insert(key.clone(), enabled);
            } else {
                configuration
                    .custom_model_connection_enabled_overrides
                    .remove(key.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Rejects account membership choices that would reopen a provider-disabled custom model.
/// 拒绝会重新开启供应商已关闭自定义模型的账号成员关系选择。
///
/// # Parameters
/// 参数
/// `configuration` owns provider publication state; `provider_id`, `model_id`, and `enabled`
/// identify the exact requested account membership difference.
/// `configuration` 持有供应商发布状态；`provider_id`、`model_id` 与 `enabled` 标识精确的账号成员关系差异。
///
/// # Returns
/// 返回值
/// Success for restoration or disablement, and for enablement only while the provider publishes the model.
/// 还原或关闭时返回成功；仅当供应商发布该模型时允许开启。
fn validate_custom_model_connection_availability_choice(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
    enabled: Option<bool>,
) -> Result<(), String> {
    if enabled == Some(true) && !configuration.model_is_enabled(provider_id, model_id) {
        return Err(format!(
            "Provider `{provider_id}` has disabled model `{model_id}`; an account cannot enable it. Enable the model at provider level first."
        ));
    }
    Ok(())
}

/// Reports whether one model belongs to an account independently from its access switch.
/// 报告一个模型是否属于某个账号，且不受其访问开关影响。
///
/// # Parameters
/// 参数
/// `runtime` owns immutable source evidence; `configuration` owns manual membership;
/// `connection_id` and `model_id` select the exact account-model pair.
/// `runtime` 持有不可变源证据；`configuration` 持有手工成员关系；`connection_id` 与 `model_id`
/// 选择精确账号模型组合。
///
/// # Returns
/// 返回值
/// `true` only when the model remains in the resolved support directory after access policy is removed.
/// 仅当移除访问策略后模型仍位于已解析支持目录中时返回 `true`。
fn connection_supports_model(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
    model_id: &str,
) -> Result<bool, String> {
    let mut support_configuration = configuration.clone();
    support_configuration.remove_connection_model_access_policy(connection_id);
    Ok(runtime
        .resolved_model_catalog_snapshot(&support_configuration)?
        .get(connection_id)
        .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id)))
}

/// Applies an account-wide model default without allowing provider-disabled models to re-enter it.
/// 应用账号范围模型默认值，同时不允许供应商已关闭的模型重新进入账号默认值。
///
/// # Parameters
/// 参数
/// `runtime` owns manifest defaults; `configuration` is mutated in place; `connection_id` and
/// `provider_id` identify the exact account route; `enabled` is the requested account default;
/// `support_catalog` is the account's provider-resolved membership before access policies.
/// `runtime` 持有清单默认值；`configuration` 会被原地修改；`connection_id` 与 `provider_id`
/// 标识精确账号路由；`enabled` 是请求的账号默认值；`support_catalog` 是应用访问策略前的成员关系。
fn set_connection_models_enabled_respecting_provider_access(
    runtime: &PublicAiRuntime,
    configuration: &mut PersistedAiConfiguration,
    connection_id: &str,
    provider_id: &str,
    enabled: bool,
    support_catalog: &ConnectionModelCatalog,
) -> Result<(), String> {
    // connection_provider_id validates the exact caller scope before mutating sparse access state.
    // connection_provider_id 在修改稀疏访问状态前校验精确调用方作用域。
    let connection_provider_id = configuration
        .connection_graph
        .connections
        .get(connection_id)
        .map(|connection| connection.provider_id.as_str())
        .ok_or_else(|| format!("Credential connection `{connection_id}` is not configured."))?;
    if connection_provider_id != provider_id {
        return Err(format!(
            "Credential connection `{connection_id}` does not belong to provider `{provider_id}`."
        ));
    }
    configuration.set_connection_models_enabled(connection_id, enabled);
    if !enabled {
        return Ok(());
    }
    for model in &support_catalog.models {
        if !runtime.model_is_enabled_for_connection(
            configuration,
            connection_id,
            model.id.as_str(),
        )? {
            configuration.set_connection_model_enabled(connection_id, model.id.as_str(), false);
        }
    }
    Ok(())
}

/// Saves one credential-scoped model access decision and repairs affected active sessions.
/// 保存一项凭据作用域模型访问决定并修复受影响的活动会话。
pub(super) async fn service_save_public_ai_connection_model_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionModelEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential connection `{}` is not configured.",
                request.connection_id
            )
        })?;
    if !connection_supports_model(
        runtime.as_ref(),
        &current_configuration,
        request.connection_id.as_str(),
        request.model_id.as_str(),
    )? {
        return Err(format!(
            "Credential connection `{}` does not support model `{}`.",
            request.connection_id, request.model_id
        ));
    }
    if request.enabled
        && !runtime.model_is_enabled_for_connection(
            &current_configuration,
            request.connection_id.as_str(),
            request.model_id.as_str(),
        )?
    {
        return Err(format!(
            "Provider `{}` has disabled model `{}`; enable the provider model before enabling it for a credential connection.",
            connection.provider_id, request.model_id
        ));
    }

    let mut candidate_configuration = current_configuration.clone();
    candidate_configuration.set_connection_model_enabled(
        request.connection_id.as_str(),
        request.model_id.as_str(),
        request.enabled,
    );
    let candidate_catalogs = runtime.resolved_model_catalog_snapshot(&candidate_configuration)?;
    let active_was_disabled = !request.enabled
        && candidate_configuration
            .active
            .as_ref()
            .is_some_and(|selection| {
                selection.connection_id == request.connection_id
                    && selection.model_id == request.model_id
            });
    let replacement = if active_was_disabled {
        preferred_replacement_model_selection(
            runtime.as_ref(),
            &candidate_configuration,
            &candidate_catalogs,
            connection.provider_id.as_str(),
            request.model_id.as_str(),
        )?
    } else {
        None
    };
    if active_was_disabled {
        candidate_configuration.active = replacement.clone();
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;

    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_sessions_in_transaction(
            &app,
            &state,
            runtime.as_ref(),
            &current_configuration,
            "保存凭据模型启用状态失败",
            |configuration| {
                *configuration = candidate_configuration.clone();
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |workspace| {
                Ok(if request.enabled {
                    BTreeSet::new()
                } else {
                    reconcile_disabled_connection_sessions(
                        workspace,
                        request.connection_id.as_str(),
                        Some(request.model_id.as_str()),
                        replacement.as_ref(),
                    )
                })
            },
            "connection_model_access_revoked",
        )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one account-wide inherited model access state and repairs affected active sessions.
/// 保存一项账号范围继承模型访问状态并修复受影响的活动会话。
pub(super) async fn service_save_public_ai_connection_models_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionModelsEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential connection `{}` is not configured.",
                request.connection_id
            )
        })?;
    let mut support_configuration = current_configuration.clone();
    support_configuration.remove_connection_model_access_policy(request.connection_id.as_str());
    let support_catalog = runtime
        .resolved_model_catalog_snapshot(&support_configuration)?
        .get(request.connection_id.as_str())
        .cloned();
    let Some(support_catalog) = support_catalog else {
        return Err(format!(
            "Credential connection `{}` has no supported models.",
            request.connection_id
        ));
    };
    if support_catalog.models.is_empty() {
        return Err(format!(
            "Credential connection `{}` has no supported models.",
            request.connection_id
        ));
    }

    let mut candidate_configuration = current_configuration.clone();
    set_connection_models_enabled_respecting_provider_access(
        runtime.as_ref(),
        &mut candidate_configuration,
        request.connection_id.as_str(),
        connection.provider_id.as_str(),
        request.enabled,
        &support_catalog,
    )?;
    let candidate_catalogs = runtime.resolved_model_catalog_snapshot(&candidate_configuration)?;
    let active_was_disabled = !request.enabled
        && candidate_configuration
            .active
            .as_ref()
            .is_some_and(|selection| selection.connection_id == request.connection_id);
    let replacement = if active_was_disabled {
        let active_model_id = candidate_configuration
            .active
            .as_ref()
            .map(|selection| selection.model_id.clone())
            .unwrap_or_default();
        preferred_replacement_model_selection(
            runtime.as_ref(),
            &candidate_configuration,
            &candidate_catalogs,
            connection.provider_id.as_str(),
            active_model_id.as_str(),
        )?
    } else {
        None
    };
    if active_was_disabled {
        candidate_configuration.active = replacement.clone();
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;

    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_sessions_in_transaction(
            &app,
            &state,
            runtime.as_ref(),
            &current_configuration,
            "批量保存凭据模型启用状态失败",
            |configuration| {
                *configuration = candidate_configuration.clone();
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |workspace| {
                Ok(if request.enabled {
                    BTreeSet::new()
                } else {
                    reconcile_disabled_connection_sessions(
                        workspace,
                        request.connection_id.as_str(),
                        None,
                        replacement.as_ref(),
                    )
                })
            },
            "connection_model_access_revoked",
        )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves or clears one user-owned model capability difference.
/// 保存或清除一项用户自有模型能力差异。
///
/// # Parameters
/// 参数
///
/// `app` publishes the refreshed workbench, `state` owns catalogs and configuration, and
/// `request` identifies the provider model plus the optional replacement overlay.
/// `app` 发布刷新的工作台，`state` 持有目录与配置，`request` 标识供应商模型及可选替换覆盖。
///
/// # Returns
/// 返回值
///
/// Returns the rebuilt browser snapshot after the immutable source catalog is reprojected.
/// 在不可变源目录上重新投影后返回重建的浏览器快照。
pub(super) async fn service_save_public_ai_model_capability_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelCapabilityOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    // overrides collapses an empty browser record into restoration semantics.
    // overrides 将空浏览器记录折叠为还原语义。
    let overrides = request.overrides.filter(|overrides| !overrides.is_empty());
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if overrides.is_some()
        && !runtime.provider_conversation_model_is_supported(
            &current_configuration,
            request.provider_id.as_str(),
            request.model_id.as_str(),
        )?
    {
        return Err(format!(
            "Provider `{}` does not support model `{}`.",
            request.provider_id, request.model_id
        ));
    }
    if overrides.is_some() {
        runtime.validate_model_capability_override_candidate(
            &current_configuration,
            request.provider_id.as_str(),
            request.model_id.as_str(),
            overrides.as_ref(),
        )?;
    }
    let key = PersistedAiConfiguration::provider_model_key(
        request.provider_id.as_str(),
        request.model_id.as_str(),
    );
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存模型能力覆盖失败",
        |configuration| {
            if let Some(overrides) = overrides.clone() {
                configuration
                    .model_capability_overrides
                    .insert(key.clone(), overrides);
            } else {
                configuration
                    .model_capability_overrides
                    .remove(key.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider-wide sparse capability layer or restores inherited defaults.
/// 保存一层供应商级稀疏能力配置，或恢复继承默认值。
pub(super) async fn service_save_public_ai_provider_capability_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderCapabilityOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    let overrides = request.overrides.filter(|overrides| !overrides.is_empty());
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if overrides.is_some() {
        runtime.validate_provider_capability_override_candidate(
            &current_configuration,
            request.provider_id.as_str(),
            overrides.as_ref(),
        )?;
    }
    let provider_id = request.provider_id.clone();
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存供应商能力覆盖失败",
        |configuration| {
            if let Some(overrides) = overrides.clone() {
                configuration
                    .provider_capability_overrides
                    .insert(provider_id.clone(), overrides);
            } else {
                configuration
                    .provider_capability_overrides
                    .remove(provider_id.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Finds removed custom models still owned by active or internal inference routes.
/// 查找仍被当前选择或内部推理路由持有的已移除自定义模型。
fn referenced_custom_model_ids(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    removed_model_ids: &BTreeSet<String>,
) -> Result<BTreeSet<String>, String> {
    let mut referenced = BTreeSet::new();
    if let Some(active) = configuration.active.as_ref()
        && active.provider_id == provider_id
        && removed_model_ids.contains(active.model_id.as_str())
        && !model_remains_on_connection(
            runtime,
            configuration,
            Some(active.connection_id.as_str()),
            active.model_id.as_str(),
        )?
    {
        referenced.insert(active.model_id.clone());
    }
    for profile in configuration.inference_profiles.values() {
        let routes = profile
            .llm_routes
            .values()
            .chain(profile.embedding.iter())
            .chain(profile.rerank.iter());
        for route in routes {
            if route.provider_id == provider_id
                && removed_model_ids.contains(route.model_id.as_str())
                && !model_remains_on_connection(
                    runtime,
                    configuration,
                    Some(route.connection_id.as_str()),
                    route.model_id.as_str(),
                )?
            {
                referenced.insert(route.model_id.clone());
            }
        }
    }
    Ok(referenced)
}

/// Checks whether a candidate configuration keeps a model on one exact referenced connection.
/// 检查候选配置是否在一个精确被引用连接上保留模型。
///
/// # Parameters
/// 参数
/// `runtime` owns immutable source catalogs, `configuration` is the candidate state,
/// `connection_id` is the referenced credential when available, and `model_id` is exact.
/// `runtime` 持有不可变源目录，`configuration` 是候选状态，`connection_id` 是可用时的
/// 被引用凭据，`model_id` 为精确模型标识。
///
/// # Returns
/// 返回值
/// `true` only when the connection still exists and its fully resolved catalog contains the model.
/// 仅当连接仍存在且其完整解析目录包含该模型时返回 `true`。
fn model_remains_on_connection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: Option<&str>,
    model_id: &str,
) -> Result<bool, String> {
    let Some(connection_id) = connection_id else {
        return Ok(false);
    };
    if !configuration
        .connection_graph
        .connections
        .contains_key(connection_id)
    {
        return Ok(false);
    }
    runtime.connection_catalog_contains_model(configuration, connection_id, model_id)
}

/// Starts one exact provider OAuth flow without persisting transaction state.
/// 启动一个精确供应商 OAuth 流程且不持久化事务状态。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiOAuthStartRequest {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    provider_id: String,
    /// Stable service-entry identifier.
    /// 稳定服务入口标识。
    service_entry_id: String,
    /// Stable renewable credential-profile identifier.
    /// 稳定的可续期凭据配置档标识。
    credential_profile_id: String,
    /// User-owned OAuth client settings required only by declared flows.
    /// 仅声明需要的流程使用的用户自有 OAuth 客户端设置。
    #[serde(default)]
    client: OAuthClientConfiguration,
}

/// Commits one successful OAuth transaction as a credential connection.
/// 将一个成功 OAuth 事务提交为凭据连接。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiOAuthCommitRequest {
    /// Opaque successful transaction identifier.
    /// 不透明成功事务标识。
    transaction_id: String,
    /// Stable provider identifier selected before authorization.
    /// 授权前选择的稳定供应商标识。
    provider_id: String,
    /// Stable service-entry identifier selected before authorization.
    /// 授权前选择的稳定服务入口标识。
    service_entry_id: String,
    /// Stable renewable credential-profile identifier.
    /// 稳定的可续期凭据配置档标识。
    credential_profile_id: String,
    /// User-visible credential label.
    /// 用户可见凭据标签。
    display_name: String,
    /// Existing connection replaced by this authorization, when reauthorizing.
    /// 重新授权时由此次授权替换的现有连接。
    #[serde(default)]
    connection_id: Option<String>,
}

/// Starts a provider-declared OAuth flow and opens the system browser.
/// 启动供应商声明的 OAuth 流程并打开系统浏览器。
pub(super) async fn service_start_public_ai_oauth(
    state: SharedServiceBridgeState,
    request: PublicAiOAuthStartRequest,
) -> Result<OAuthAuthorizationChallenge, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("serviceEntryId", request.service_entry_id.as_str())?;
    validate_non_empty(
        "credentialProfileId",
        request.credential_profile_id.as_str(),
    )?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let providers = runtime.effective_providers(&configuration)?;
    let provider = providers
        .provider(request.provider_id.as_str())
        .ok_or_else(|| format!("Provider `{}` is not registered.", request.provider_id))?;
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == request.service_entry_id)
        .ok_or_else(|| {
            format!(
                "Service entry `{}` is not registered for provider `{}`.",
                request.service_entry_id, request.provider_id
            )
        })?;
    let credential_profile = entry
        .credential_profile(request.credential_profile_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential profile `{}` is not registered for service entry `{}`.",
                request.credential_profile_id, request.service_entry_id
            )
        })?;
    let authorization = credential_profile.authorization.as_ref().ok_or_else(|| {
        format!(
            "Credential profile `{}` does not declare an authorization flow.",
            request.credential_profile_id
        )
    })?;
    if credential_profile.credential_mode != CredentialMode::Auth {
        return Err(format!(
            "Credential profile `{}` does not support host-managed authorization.",
            request.credential_profile_id
        ));
    }
    let authorization_binding = credential_authorization_binding(
        request.provider_id.as_str(),
        request.service_entry_id.as_str(),
        request.credential_profile_id.as_str(),
        authorization,
    )?;
    let challenge = runtime
        .oauth()
        .start(
            authorization,
            authorization_binding,
            credential_profile.key_prefix.clone(),
            request.client,
        )
        .await?;
    if let Err(error) =
        shell_host_from_state(&state)?.open_url(challenge.authorization_url.as_str())
    {
        let _ = runtime
            .oauth()
            .cancel(challenge.transaction.transaction_id.as_str())
            .await;
        return Err(error);
    }
    Ok(challenge)
}

/// Returns the current state of one process-local OAuth transaction.
/// 返回一个进程内 OAuth 事务的当前状态。
pub(super) async fn service_public_ai_oauth_status(
    state: SharedServiceBridgeState,
    transaction_id: String,
) -> Result<OAuthTransactionSnapshot, String> {
    validate_non_empty("transactionId", transaction_id.as_str())?;
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    runtime.oauth().status(transaction_id.as_str()).await
}

/// Cancels one process-local OAuth transaction.
/// 取消一个进程内 OAuth 事务。
pub(super) async fn service_cancel_public_ai_oauth(
    state: SharedServiceBridgeState,
    transaction_id: String,
) -> Result<OAuthTransactionSnapshot, String> {
    validate_non_empty("transactionId", transaction_id.as_str())?;
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    runtime.oauth().cancel(transaction_id.as_str()).await
}

/// Commits one successful OAuth transaction as a new connection.
/// 将一个成功 OAuth 事务提交为新连接。
pub(super) async fn service_commit_public_ai_oauth_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiOAuthCommitRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("transactionId", request.transaction_id.as_str())?;
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("displayName", request.display_name.as_str())?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let providers = runtime.effective_providers(&configuration)?;
    let provider = providers
        .provider(request.provider_id.as_str())
        .ok_or_else(|| format!("Provider `{}` is not registered.", request.provider_id))?;
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == request.service_entry_id)
        .ok_or_else(|| {
            format!(
                "Service entry `{}` is not registered for provider `{}`.",
                request.service_entry_id, request.provider_id
            )
        })?;
    let snapshot = runtime
        .oauth()
        .status(request.transaction_id.as_str())
        .await?;
    if snapshot.phase != OAuthTransactionPhase::Succeeded {
        return Err(format!(
            "OAuth transaction `{}` is not successful.",
            request.transaction_id
        ));
    }
    let credential_profile = entry
        .credential_profile(request.credential_profile_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential profile `{}` is not registered for service entry `{}`.",
                request.credential_profile_id, request.service_entry_id
            )
        })?;
    let authorization = credential_profile.authorization.as_ref().ok_or_else(|| {
        format!(
            "Credential profile `{}` no longer declares an authorization flow.",
            request.credential_profile_id
        )
    })?;
    let expected_authorization_binding = credential_authorization_binding(
        request.provider_id.as_str(),
        request.service_entry_id.as_str(),
        request.credential_profile_id.as_str(),
        authorization,
    )?;
    if snapshot.authorization_binding != expected_authorization_binding {
        return Err(String::from(
            "Authorization transaction does not belong to the selected service entry.",
        ));
    }
    let material = runtime
        .oauth()
        .take_succeeded(request.transaction_id.as_str())
        .await?;
    if let Some(connection_id) = request.connection_id.as_deref() {
        let transaction = begin_public_ai_configuration_transaction().await;
        let (_, current_configuration) = public_ai_runtime_and_configuration(&state)?;
        // runtimeBeforeImage captures every catalog/status/cache field before OAuth validation.
        // runtimeBeforeImage 在 OAuth 校验前捕获全部目录、状态与缓存字段。
        let runtime_before_image = runtime.capture_activation_snapshot()?;
        let current = current_configuration
            .connection_graph
            .connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
        if current.provider_id != request.provider_id {
            return Err(String::from(
                "OAuth reauthorization provider does not match the existing connection.",
            ));
        }
        let secret_store = runtime.credential_secret_store();
        let previous = CredentialSecretResolver::read_optional_secret(
            secret_store.as_ref(),
            current.secret_reference.as_str(),
        )
        .await?;
        let replacement = match previous.as_ref() {
            Some(previous) => previous.advance_with_material(material)?,
            None => CredentialSecretEnvelope::new(material)?,
        };
        match previous.as_ref() {
            Some(previous) => {
                CredentialSecretStore::replace_secret(
                    secret_store.as_ref(),
                    current.secret_reference.as_str(),
                    previous.revision,
                    &replacement,
                )
                .await?;
            }
            None => {
                CredentialSecretStore::create_secret(
                    secret_store.as_ref(),
                    current.secret_reference.as_str(),
                    &replacement,
                )
                .await?;
            }
        }
        let connection_id = connection_id.to_string();
        let update_request = PublicAiConnectionUpdateRequest {
            connection_id: connection_id.clone(),
            display_name: request.display_name.clone(),
            service_entry_id: request.service_entry_id.clone(),
            credential_profile_id: request.credential_profile_id.clone(),
            credential_mode: CredentialMode::Auth,
            secret: None,
            management_key: None,
            remove_management_key: false,
            service_origin_override: current.service_origin_override.clone(),
        };
        let mut validation_configuration = current_configuration.clone();
        if let Err(error) = apply_connection_update(
            runtime.as_ref(),
            &mut validation_configuration,
            &update_request,
            connection_id.as_str(),
        ) {
            rollback_oauth_reauthorization_secret(
                secret_store.as_ref(),
                current.secret_reference.as_str(),
                previous.as_ref(),
                &replacement,
            )
            .await?;
            return Err(error);
        }
        if let Err(error) = runtime
            .refresh_connection_catalog(&validation_configuration, connection_id.as_str())
            .await
        {
            let rollback_result = rollback_oauth_reauthorization_secret(
                secret_store.as_ref(),
                current.secret_reference.as_str(),
                previous.as_ref(),
                &replacement,
            )
            .await;
            let restore_result =
                restore_public_ai_runtime_activation(runtime.as_ref(), &runtime_before_image);
            return Err(compensated_operation_error(
                format!(
                    "OAuth credential validation failed before the connection was updated: {error}"
                ),
                rollback_result,
                restore_result,
            ));
        }
        let update_result = mutate_public_ai_configuration_in_transaction(
            &state,
            "重新授权公共 AI 连接失败",
            |configuration| {
                *configuration = validation_configuration.clone();
                Ok(())
            },
        );
        let configuration = match update_result {
            Ok(configuration) => configuration,
            Err(error) => {
                let rollback_result = rollback_oauth_reauthorization_secret(
                    secret_store.as_ref(),
                    current.secret_reference.as_str(),
                    previous.as_ref(),
                    &replacement,
                )
                .await;
                let restore_result = restore_public_ai_configuration_activation_in_transaction(
                    &state,
                    runtime.as_ref(),
                    &current_configuration,
                    &runtime_before_image,
                    "恢复 OAuth 重新授权前配置失败",
                );
                return Err(compensated_operation_error(
                    error,
                    rollback_result,
                    restore_result,
                ));
            }
        };
        // publicationResult prepares the complete response before exposing reauthorization.
        // publicationResult 在公开重新授权结果前准备完整响应。
        let publication_result =
            build_public_ai_settings_snapshot(runtime.as_ref(), &configuration).and_then(
                |snapshot| {
                    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)
                        .map(|_| snapshot)
                },
            );
        return match publication_result {
            Ok(snapshot) => {
                drop(transaction);
                Ok(snapshot)
            }
            Err(primary) => {
                let secret_restore = rollback_oauth_reauthorization_secret(
                    secret_store.as_ref(),
                    current.secret_reference.as_str(),
                    previous.as_ref(),
                    &replacement,
                )
                .await;
                let activation_restore = restore_public_ai_configuration_activation_in_transaction(
                    &state,
                    runtime.as_ref(),
                    &current_configuration,
                    &runtime_before_image,
                    "恢复 OAuth 重新授权前配置失败",
                );
                drop(transaction);
                Err(compensated_operation_error(
                    primary,
                    secret_restore,
                    activation_restore,
                ))
            }
        };
    }
    let secret = CredentialSecretEnvelope::new(material)?;
    add_public_ai_connection_with_secret(
        app,
        state,
        request.provider_id,
        request.service_entry_id,
        request.credential_profile_id,
        CredentialMode::Auth,
        request.display_name,
        secret,
        None,
        None,
    )
    .await
}

/// Returns the current public AI settings snapshot.
/// 返回当前公共 AI 设置快照。
pub(super) async fn service_public_ai_settings_snapshot(
    state: SharedServiceBridgeState,
) -> Result<PublicAiSettingsSnapshot, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
}

/// Adds and synchronizes one public AI credential connection.
/// 添加并同步一个公共 AI 凭据连接。
pub(super) async fn service_add_public_ai_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionAddRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    validate_non_empty("serviceEntryId", request.service_entry_id.as_str())?;
    validate_non_empty(
        "credentialProfileId",
        request.credential_profile_id.as_str(),
    )?;
    validate_non_empty("displayName", request.display_name.as_str())?;
    validate_non_empty("secret", request.secret.as_str())?;
    if request.credential_mode != CredentialMode::Token {
        return Err(String::from(
            "Renewable account authorization must be created through the Auth login flow.",
        ));
    }
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    // credential_registration is the manifest-selected onboarding primitive, independent of provider identity.
    // credential_registration 是清单选择的接入原语，与供应商身份无关。
    let credential_registration = runtime
        .effective_providers(&configuration)?
        .provider(request.provider_id.as_str())
        .ok_or_else(|| format!("Provider `{}` is not registered.", request.provider_id))?
        .credential_registration;
    let (service_entry_id, credential_profile_id) = if credential_registration
        == vulcan_provider_adapters::CredentialRegistrationKind::MiniMaxRegionalApiKey
    {
        let registration = runtime
            .metadata_client()
            .register_minimax_key(request.secret.as_str())
            .await?;
        (
            registration.service_entry_id,
            registration.credential_profile_id,
        )
    } else {
        validate_non_empty("serviceEntryId", request.service_entry_id.as_str())?;
        validate_non_empty(
            "credentialProfileId",
            request.credential_profile_id.as_str(),
        )?;
        (
            request.service_entry_id.clone(),
            request.credential_profile_id.clone(),
        )
    };
    let secret = CredentialSecretEnvelope::new_api_key(request.secret)?;
    add_public_ai_connection_with_secret(
        app,
        state,
        request.provider_id,
        service_entry_id,
        credential_profile_id,
        request.credential_mode,
        request.display_name,
        secret,
        request.management_key,
        request.service_origin_override,
    )
    .await
}

/// Persists one validated secret and its exact connection as a single compensating transaction.
/// 以单个补偿事务持久化一个已验证秘密及其精确连接。
async fn add_public_ai_connection_with_secret(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_id: String,
    service_entry_id: String,
    credential_profile_id: String,
    credential_mode: CredentialMode,
    display_name: String,
    secret: CredentialSecretEnvelope,
    management_key: Option<String>,
    service_origin_override: Option<String>,
) -> Result<PublicAiSettingsSnapshot, String> {
    if secret.credential_mode() != credential_mode {
        return Err(String::from(
            "Credential secret mode does not match the requested connection mode.",
        ));
    }
    // The transaction covers discovery and persistence so a concurrent catalog synchronization
    // cannot publish overlays built from an older configuration snapshot.
    // 事务覆盖目录发现与持久化，避免并发目录同步发布基于旧配置快照构建的覆盖结果。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // runtimeBeforeImage captures every discovery, usage, status, and cache field before validation.
    // runtimeBeforeImage 在校验前捕获全部发现、用量、状态与缓存字段。
    let runtime_before_image = runtime.capture_activation_snapshot()?;
    let connection_id = format!("connection_{}", uuid::Uuid::new_v4().simple());
    let credential_id = format!("credential_{}", uuid::Uuid::new_v4().simple());
    let secret_reference = format!("ai_secret_{}", uuid::Uuid::new_v4().simple());
    let management_secret_reference = management_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|_| format!("ai_management_secret_{}", uuid::Uuid::new_v4().simple()));
    let providers = runtime.effective_providers(&current_configuration)?;
    let provider = providers
        .provider(provider_id.as_str())
        .ok_or_else(|| format!("Provider `{provider_id}` is not registered."))?;
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == service_entry_id)
        .ok_or_else(|| {
            format!(
                "Service entry `{}` is not registered for provider `{}`.",
                service_entry_id, provider_id
            )
        })?;
    let service_origin_override =
        vulcan_provider_adapters::normalize_connection_service_origin_override(
            entry,
            service_origin_override.as_deref(),
        )?;
    let credential_profile = entry
        .credential_profile(credential_profile_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential profile `{credential_profile_id}` is not registered for service entry `{service_entry_id}`."
            )
        })?;
    if management_key
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && !credential_profile
            .account_usage_query
            .requires_management_credential()
    {
        return Err(String::from(
            "A management key requires a credential profile whose usage primitive declares a separate management credential.",
        ));
    }
    let service_entry_kind = entry.kind;
    if credential_profile.credential_mode != credential_mode {
        return Err(format!(
            "Credential profile `{credential_profile_id}` does not support credential mode `{credential_mode:?}`."
        ));
    }
    if let Some(required_prefix) = credential_profile.key_prefix.as_deref() {
        let key_matches = matches!(
            &secret.material,
            vulcan_provider_adapters::CredentialSecretMaterial::ApiKey { value }
                if value.starts_with(required_prefix)
        );
        if !key_matches {
            return Err(format!(
                "Credential secret does not match the required `{required_prefix}` key prefix for profile `{credential_profile_id}`."
            ));
        }
    }
    let connection = CredentialConnection {
        id: connection_id.clone(),
        provider_id,
        service_entry_id,
        credential_profile_id,
        credential_mode,
        credential_id,
        display_name: display_name.trim().to_string(),
        secret_reference: secret_reference.clone(),
        management_secret_reference: management_secret_reference.clone(),
        service_origin_override,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    let secret_store = runtime.credential_secret_store();
    CredentialSecretStore::create_secret(secret_store.as_ref(), secret_reference.as_str(), &secret)
        .await?;
    if let (Some(reference), Some(value)) = (
        management_secret_reference.as_deref(),
        management_key
            .as_ref()
            .filter(|value| !value.trim().is_empty()),
    ) {
        let management_secret = CredentialSecretEnvelope::new_api_key(value.clone())?;
        if let Err(error) = CredentialSecretStore::create_secret(
            secret_store.as_ref(),
            reference,
            &management_secret,
        )
        .await
        {
            let cleanup = CredentialSecretStore::delete_secret(
                secret_store.as_ref(),
                secret_reference.as_str(),
            )
            .await;
            return Err(compensated_operation_error(error, cleanup, Ok(())));
        }
    }
    let mut validation_configuration = current_configuration.clone();
    if let Err(error) = validation_configuration
        .connection_graph
        .add_connection(&providers, connection.clone())
    {
        return match delete_connection_secrets(
            secret_store.as_ref(),
            secret_reference.as_str(),
            management_secret_reference.as_deref(),
        )
        .await
        {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(format!(
                "{error} Secret cleanup also failed: {cleanup_error}"
            )),
        };
    }
    if let Err(error) = validate_connection_credential(
        runtime.as_ref(),
        &validation_configuration,
        connection_id.as_str(),
        service_entry_kind,
    )
    .await
    {
        let runtime_restore =
            restore_public_ai_runtime_activation(runtime.as_ref(), &runtime_before_image);
        let secret_cleanup = delete_connection_secrets(
            secret_store.as_ref(),
            secret_reference.as_str(),
            management_secret_reference.as_deref(),
        )
        .await;
        return Err(compensated_operation_error(
            format!("Credential validation failed before the connection was saved: {error}"),
            secret_cleanup,
            runtime_restore,
        ));
    }
    if management_secret_reference.is_some() {
        let usage = match runtime
            .query_connection_usage(&validation_configuration, connection_id.as_str())
            .await
        {
            Ok(usage) => usage,
            Err(error) => {
                let runtime_restore =
                    restore_public_ai_runtime_activation(runtime.as_ref(), &runtime_before_image);
                let secret_cleanup = delete_connection_secrets(
                    secret_store.as_ref(),
                    secret_reference.as_str(),
                    management_secret_reference.as_deref(),
                )
                .await;
                return Err(compensated_operation_error(
                    error,
                    secret_cleanup,
                    runtime_restore,
                ));
            }
        };
        if usage.availability != AccountUsageAvailability::Supported {
            let runtime_restore =
                restore_public_ai_runtime_activation(runtime.as_ref(), &runtime_before_image);
            let secret_cleanup = delete_connection_secrets(
                secret_store.as_ref(),
                secret_reference.as_str(),
                management_secret_reference.as_deref(),
            )
            .await;
            return Err(compensated_operation_error(
                usage
                    .refresh_message
                    .unwrap_or_else(|| String::from("Provider management key validation failed.")),
                secret_cleanup,
                runtime_restore,
            ));
        }
    }
    let add_result = mutate_public_ai_configuration_in_transaction(
        &state,
        "保存公共 AI 连接失败",
        |configuration| {
            let effective_providers = runtime.effective_providers(configuration)?;
            configuration
                .connection_graph
                .add_connection(&effective_providers, connection.clone())
        },
    );
    let configuration = match add_result {
        Ok(configuration) => configuration,
        Err(error) => {
            let secret_cleanup = delete_connection_secrets(
                secret_store.as_ref(),
                secret_reference.as_str(),
                management_secret_reference.as_deref(),
            )
            .await;
            let activation_restore = restore_public_ai_configuration_activation_in_transaction(
                &state,
                runtime.as_ref(),
                &current_configuration,
                &runtime_before_image,
                "恢复公共 AI 连接新增前配置失败",
            );
            return Err(compensated_operation_error(
                error,
                secret_cleanup,
                activation_restore,
            ));
        }
    };
    // publicationResult prepares the response before exposing the new connection in Workbench.
    // publicationResult 在工作台暴露新连接前准备响应。
    let publication_result = build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
        .and_then(|snapshot| {
            publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)
                .map(|_| snapshot)
        });
    match publication_result {
        Ok(snapshot) => {
            drop(transaction);
            Ok(snapshot)
        }
        Err(primary) => {
            let secret_cleanup = delete_connection_secrets(
                secret_store.as_ref(),
                secret_reference.as_str(),
                management_secret_reference.as_deref(),
            )
            .await;
            let activation_restore = restore_public_ai_configuration_activation_in_transaction(
                &state,
                runtime.as_ref(),
                &current_configuration,
                &runtime_before_image,
                "恢复公共 AI 连接新增前配置失败",
            );
            drop(transaction);
            Err(compensated_operation_error(
                primary,
                secret_cleanup,
                activation_restore,
            ))
        }
    }
}

/// Updates one connection and refreshes only when its connection identity changes.
/// 更新一个连接，并仅在连接身份变化时刷新。
/// Validates one credential through the exact execution surface declared by its service entry.
/// 通过服务入口声明的精确执行界面验证一个凭据。
///
/// # Parameters
/// 参数
/// `runtime` owns metadata transports and process-local caches, `configuration` contains
/// the candidate connection, `connection_id` identifies it, and `entry_kind` selects the
/// contractually valid validation path.
/// `runtime` 持有元数据传输与进程内缓存，`configuration` 包含候选连接，
/// `connection_id` 标识该连接，`entry_kind` 选择契约允许的验证路径。
///
/// # Returns
/// 返回值
/// Success after either a conversation catalog refresh or a provider-tool usage probe.
/// 在完成对话目录刷新或供应商工具用量探测后返回成功。
async fn validate_connection_credential(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
    entry_kind: ServiceEntryKind,
) -> Result<(), String> {
    match entry_kind {
        ServiceEntryKind::Conversation => runtime
            .refresh_connection_catalog(configuration, connection_id)
            .await
            .map(|_| ()),
        ServiceEntryKind::ProviderTools => {
            let usage = runtime
                .query_connection_usage(configuration, connection_id)
                .await?;
            if usage.availability == AccountUsageAvailability::Supported {
                Ok(())
            } else {
                Err(usage.refresh_message.unwrap_or_else(|| {
                    String::from(
                        "Provider-tool credential validation did not return a supported usage snapshot.",
                    )
                }))
            }
        }
    }
}

/// Resolves the exact service-entry kind registered for one provider connection.
/// 解析一个供应商连接所注册的精确服务入口类型。
///
/// # Parameters
/// 参数
/// `runtime` owns the effective provider registry, `configuration` supplies custom providers,
/// and the remaining identifiers select one declared entry.
/// `runtime` 持有有效供应商注册表，`configuration` 提供自定义供应商，
/// 其余标识选择一个已声明入口。
///
/// # Returns
/// 返回值
/// The registered entry kind, or an explicit registry error.
/// 返回已注册入口类型，或者显式注册表错误。
fn registered_service_entry_kind(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
) -> Result<ServiceEntryKind, String> {
    Ok(registered_service_entry(runtime, configuration, provider_id, service_entry_id)?.kind)
}

/// Resolves one exact manifest-projected service entry for settings validation.
/// 为设置校验解析一个精确的 Manifest 投影服务入口。
///
/// # Parameters
/// 参数
/// `runtime` and `configuration` provide the published registry, while the identifiers select
/// one exact provider product.
/// `runtime` 与 `configuration` 提供已发布注册表，两个标识选择一个精确供应商产品。
///
/// # Returns
/// 返回值
/// A cloned service entry suitable for use across asynchronous settings mutations.
/// 返回一份可跨异步设置变更使用的服务入口克隆。
fn registered_service_entry(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
) -> Result<ServiceEntry, String> {
    let providers = runtime.effective_providers(configuration)?;
    let provider = providers
        .provider(provider_id)
        .ok_or_else(|| format!("Provider `{provider_id}` is not registered."))?;
    provider
        .service_entries
        .iter()
        .find(|entry| entry.id == service_entry_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "Service entry `{service_entry_id}` is not registered for provider `{provider_id}`."
            )
        })
}

/// Resolves one exact manifest-projected credential profile for settings validation.
/// 为设置校验解析一个精确的 Manifest 投影凭据配置档。
///
/// # Parameters
/// 参数
/// `runtime` and `configuration` provide the published registry; the three identifiers select
/// one exact provider product and credential contract.
/// `runtime` 与 `configuration` 提供已发布注册表；三个标识选择一个精确供应商产品与凭据契约。
///
/// # Returns
/// 返回值
/// A cloned credential profile suitable for use across asynchronous settings mutations.
/// 返回一份可跨异步设置变更使用的凭据配置档克隆。
fn registered_credential_profile(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    credential_profile_id: &str,
) -> Result<vulcan_provider_adapters::CredentialProfile, String> {
    let entry = registered_service_entry(runtime, configuration, provider_id, service_entry_id)?;
    entry
        .credential_profile(credential_profile_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "Credential profile `{credential_profile_id}` is not registered for service entry `{service_entry_id}`."
            )
        })
}

pub(super) async fn service_update_public_ai_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionUpdateRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let mut request = request;
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    validate_non_empty("displayName", request.display_name.as_str())?;
    validate_non_empty("serviceEntryId", request.service_entry_id.as_str())?;
    validate_non_empty(
        "credentialProfileId",
        request.credential_profile_id.as_str(),
    )?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // runtimeBeforeImage freezes every catalog, status, usage, and cache field before validation.
    // runtimeBeforeImage 在校验前冻结全部目录、状态、用量与缓存字段。
    let runtime_before_image = runtime.capture_activation_snapshot()?;
    let current = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .cloned()
        .ok_or_else(|| format!("Connection `{}` is not configured.", request.connection_id))?;
    if request.management_key.is_some() && request.remove_management_key {
        return Err(String::from(
            "A management key cannot be replaced and removed in the same request.",
        ));
    }
    // credential_registration freezes the current provider's manifest-selected onboarding behavior.
    // credential_registration 冻结当前供应商由清单选择的接入行为。
    let credential_registration = runtime
        .effective_providers(&current_configuration)?
        .provider(current.provider_id.as_str())
        .ok_or_else(|| format!("Provider `{}` is not registered.", current.provider_id))?
        .credential_registration;
    if credential_registration
        == vulcan_provider_adapters::CredentialRegistrationKind::MiniMaxRegionalApiKey
        && let Some(secret) = request.secret.as_deref()
    {
        let registration = runtime
            .metadata_client()
            .register_minimax_key(secret)
            .await?;
        request.service_entry_id = registration.service_entry_id;
        request.credential_profile_id = registration.credential_profile_id;
        request.credential_mode = CredentialMode::Token;
    }
    let current_profile = registered_credential_profile(
        runtime.as_ref(),
        &current_configuration,
        current.provider_id.as_str(),
        current.service_entry_id.as_str(),
        current.credential_profile_id.as_str(),
    )?;
    let target_profile = registered_credential_profile(
        runtime.as_ref(),
        &current_configuration,
        current.provider_id.as_str(),
        request.service_entry_id.as_str(),
        request.credential_profile_id.as_str(),
    )?;
    if request.management_key.is_some()
        && !target_profile
            .account_usage_query
            .requires_management_credential()
    {
        return Err(String::from(
            "A management key requires a credential profile whose usage primitive declares a separate management credential.",
        ));
    }
    if request.remove_management_key
        && !current_profile
            .account_usage_query
            .requires_management_credential()
    {
        return Err(String::from(
            "The current credential profile does not support a management key.",
        ));
    }
    let target_entry = registered_service_entry(
        runtime.as_ref(),
        &current_configuration,
        current.provider_id.as_str(),
        request.service_entry_id.as_str(),
    )?;
    request.service_origin_override =
        vulcan_provider_adapters::normalize_connection_service_origin_override(
            &target_entry,
            request.service_origin_override.as_deref(),
        )?;
    let target_entry_kind = registered_service_entry_kind(
        runtime.as_ref(),
        &current_configuration,
        current.provider_id.as_str(),
        request.service_entry_id.as_str(),
    )?;
    if current.credential_mode != request.credential_mode && request.secret.is_none() {
        return Err(String::from(
            "Changing credential mode requires a newly validated credential.",
        ));
    }
    if current.credential_profile_id != request.credential_profile_id && request.secret.is_none() {
        return Err(String::from(
            "Changing credential profile requires a newly validated credential.",
        ));
    }
    let identity_changed = current.service_entry_id != request.service_entry_id
        || current.credential_profile_id != request.credential_profile_id
        || current.credential_mode != request.credential_mode
        || current.service_origin_override != request.service_origin_override
        || request.secret.is_some();
    let secret_store = runtime.credential_secret_store();
    let replaced_secret = if let Some(secret) = request.secret.as_deref() {
        validate_non_empty("secret", secret)?;
        if request.credential_mode != CredentialMode::Token {
            return Err(String::from(
                "Renewable account authorization must be updated through the Auth login flow.",
            ));
        }
        let previous = CredentialSecretResolver::read_secret(
            secret_store.as_ref(),
            current.secret_reference.as_str(),
        )
        .await?;
        let replacement = previous.replace_api_key(secret.to_string())?;
        CredentialSecretStore::replace_secret(
            secret_store.as_ref(),
            current.secret_reference.as_str(),
            previous.revision,
            &replacement,
        )
        .await?;
        Some((previous, replacement))
    } else {
        None
    };
    let management_mutation = match prepare_management_secret_mutation(
        secret_store.as_ref(),
        &current,
        request.management_key.as_deref(),
        request.remove_management_key,
    )
    .await
    {
        Ok(mutation) => mutation,
        Err(error) => {
            let rollback = rollback_replaced_secret(
                secret_store.as_ref(),
                current.secret_reference.as_str(),
                replaced_secret.as_ref(),
            )
            .await;
            return Err(compensated_operation_error(error, rollback, Ok(())));
        }
    };
    let connection_id = request.connection_id.clone();
    let mut validation_configuration = current_configuration.clone();
    if let Err(error) = apply_connection_update(
        runtime.as_ref(),
        &mut validation_configuration,
        &request,
        connection_id.as_str(),
    ) {
        rollback_replaced_secret(
            secret_store.as_ref(),
            current.secret_reference.as_str(),
            replaced_secret.as_ref(),
        )
        .await?;
        rollback_management_secret_mutation(secret_store.as_ref(), &management_mutation).await?;
        return Err(error);
    }
    if let Some(connection) = validation_configuration
        .connection_graph
        .connections
        .get_mut(connection_id.as_str())
    {
        connection.management_secret_reference = management_mutation.resulting_reference(&current);
    }
    if identity_changed
        && let Err(error) = validate_connection_credential(
            runtime.as_ref(),
            &validation_configuration,
            connection_id.as_str(),
            target_entry_kind,
        )
        .await
    {
        let rollback_result = rollback_replaced_secret(
            secret_store.as_ref(),
            current.secret_reference.as_str(),
            replaced_secret.as_ref(),
        )
        .await;
        let management_rollback =
            rollback_management_secret_mutation(secret_store.as_ref(), &management_mutation).await;
        let restore_result =
            restore_public_ai_runtime_activation(runtime.as_ref(), &runtime_before_image);
        return Err(compensated_operation_error(
            format!("Credential validation failed before the connection was updated: {error}"),
            combine_results(rollback_result, management_rollback),
            restore_result,
        ));
    }
    if management_mutation.requires_validation() {
        let usage = runtime
            .query_connection_usage(&validation_configuration, connection_id.as_str())
            .await;
        let validation_error = match usage {
            Ok(snapshot) if snapshot.availability == AccountUsageAvailability::Supported => None,
            Ok(snapshot) => Some(
                snapshot
                    .refresh_message
                    .unwrap_or_else(|| String::from("Provider management key validation failed.")),
            ),
            Err(error) => Some(error),
        };
        if let Some(error) = validation_error {
            let primary_rollback = rollback_replaced_secret(
                secret_store.as_ref(),
                current.secret_reference.as_str(),
                replaced_secret.as_ref(),
            )
            .await;
            let management_rollback =
                rollback_management_secret_mutation(secret_store.as_ref(), &management_mutation)
                    .await;
            let runtime_restore =
                restore_public_ai_runtime_activation(runtime.as_ref(), &runtime_before_image);
            return Err(compensated_operation_error(
                error,
                combine_results(primary_rollback, management_rollback),
                runtime_restore,
            ));
        }
    }
    let update_result = mutate_public_ai_configuration_in_transaction(
        &state,
        "更新公共 AI 连接失败",
        |configuration| {
            *configuration = validation_configuration.clone();
            Ok(())
        },
    );
    let configuration = match update_result {
        Ok(configuration) => configuration,
        Err(error) => {
            let rollback_result = rollback_replaced_secret(
                secret_store.as_ref(),
                current.secret_reference.as_str(),
                replaced_secret.as_ref(),
            )
            .await;
            let management_rollback =
                rollback_management_secret_mutation(secret_store.as_ref(), &management_mutation)
                    .await;
            let restore_result = restore_public_ai_configuration_activation_in_transaction(
                &state,
                runtime.as_ref(),
                &current_configuration,
                &runtime_before_image,
                "恢复公共 AI 连接更新前配置失败",
            );
            return Err(compensated_operation_error(
                error,
                combine_results(rollback_result, management_rollback),
                restore_result,
            ));
        }
    };
    // publicationResult completes runtime cleanup and response projection before Workbench swap.
    // publicationResult 在工作台交换前完成运行时清理与响应投影。
    let publication_result = (|| {
        if management_mutation.removes_key() {
            runtime.remove_connection_usage(connection_id.as_str())?;
        }
        let snapshot = build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)?;
        publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
        Ok::<_, String>(snapshot)
    })();
    match publication_result {
        Ok(snapshot) => {
            drop(transaction);
            Ok(snapshot)
        }
        Err(primary) => {
            let primary_rollback = rollback_replaced_secret(
                secret_store.as_ref(),
                current.secret_reference.as_str(),
                replaced_secret.as_ref(),
            )
            .await;
            let management_rollback =
                rollback_management_secret_mutation(secret_store.as_ref(), &management_mutation)
                    .await;
            let activation_restore = restore_public_ai_configuration_activation_in_transaction(
                &state,
                runtime.as_ref(),
                &current_configuration,
                &runtime_before_image,
                "恢复公共 AI 连接更新前配置失败",
            );
            drop(transaction);
            Err(compensated_operation_error(
                primary,
                combine_results(primary_rollback, management_rollback),
                activation_restore,
            ))
        }
    }
}

/// Applies one validated connection edit to an isolated configuration snapshot.
/// 将一个已校验连接编辑应用到隔离配置快照。
///
/// # Parameters
/// 参数
/// `runtime` owns provider descriptors, `configuration` is the isolated candidate,
/// `request` is the user edit, and `connection_id` is the exact target.
/// `runtime` 持有供应商描述，`configuration` 是隔离候选配置，
/// `request` 是用户编辑，`connection_id` 是精确目标。
///
/// # Returns
/// 返回值
/// Success when the updated connection remains valid in the registered graph.
/// 当更新后的连接在注册图中仍然有效时返回成功。
fn apply_connection_update(
    runtime: &PublicAiRuntime,
    configuration: &mut PersistedAiConfiguration,
    request: &PublicAiConnectionUpdateRequest,
    connection_id: &str,
) -> Result<(), String> {
    let latest = configuration
        .connection_graph
        .connections
        .get(connection_id)
        .cloned()
        .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
    let providers = runtime.effective_providers(configuration)?;
    let provider = providers
        .provider(latest.provider_id.as_str())
        .ok_or_else(|| format!("Provider `{}` is not registered.", latest.provider_id))?;
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == request.service_entry_id)
        .ok_or_else(|| {
            format!(
                "Service entry `{}` is not registered for provider `{}`.",
                request.service_entry_id, latest.provider_id
            )
        })?;
    let mut candidate = latest;
    candidate.display_name = request.display_name.trim().to_string();
    candidate.service_entry_id = request.service_entry_id.clone();
    candidate.credential_profile_id = request.credential_profile_id.clone();
    candidate.credential_mode = request.credential_mode;
    candidate.service_origin_override =
        vulcan_provider_adapters::normalize_connection_service_origin_override(
            entry,
            request.service_origin_override.as_deref(),
        )?;
    configuration
        .connection_graph
        .connections
        .remove(connection_id);
    configuration
        .connection_graph
        .add_connection(&providers, candidate)
}

/// Restores or removes an OAuth reauthorization secret after a later operation fails.
/// 在后续操作失败后恢复或移除 OAuth 重新授权秘密。
///
/// # Parameters
/// 参数
/// `previous` is the optional secret present before reauthorization and `replacement` is the
/// newly stored OAuth envelope.
/// `previous` 是重新授权前可选存在的秘密，`replacement` 是新存储的 OAuth 封装。
///
/// # Returns
/// 返回值
/// Success after restoring a prior revision or deleting a newly reconstructed missing secret.
/// 恢复先前修订版或删除新重建的缺失秘密后返回成功。
async fn rollback_oauth_reauthorization_secret(
    secret_store: &dyn CredentialSecretStore,
    secret_reference: &str,
    previous: Option<&CredentialSecretEnvelope>,
    replacement: &CredentialSecretEnvelope,
) -> Result<(), String> {
    let Some(previous) = previous else {
        return CredentialSecretStore::delete_secret(secret_store, secret_reference).await;
    };
    let rollback = replacement.advance_with_material(previous.material.clone())?;
    CredentialSecretStore::replace_secret(
        secret_store,
        secret_reference,
        replacement.revision,
        &rollback,
    )
    .await
}

/// Restores a replaced secret with a monotonic revision when validation or persistence fails.
/// 当验证或持久化失败时使用单调递增修订恢复已替换秘密。
///
/// # Parameters
/// 参数
/// `secret_store` is the secure store, `secret_reference` is opaque, and
/// `replaced_secret` contains the previous and replacement envelopes when a secret changed.
/// `secret_store` 是安全仓库，`secret_reference` 是不透明引用，
/// `replaced_secret` 在秘密变更时包含旧封装与替换封装。
async fn rollback_replaced_secret(
    secret_store: &dyn CredentialSecretStore,
    secret_reference: &str,
    replaced_secret: Option<&(CredentialSecretEnvelope, CredentialSecretEnvelope)>,
) -> Result<(), String> {
    let Some((previous, replacement)) = replaced_secret else {
        return Ok(());
    };
    let rollback = replacement.advance_with_material(previous.material.clone())?;
    CredentialSecretStore::replace_secret(
        secret_store,
        secret_reference,
        replacement.revision,
        &rollback,
    )
    .await
}

/// Records one reversible provider-management secret mutation.
/// 记录一次可回滚的供应商管理秘密变更。
enum ManagementSecretMutation {
    /// No management credential changed.
    /// 管理凭据未发生变化。
    Unchanged,
    /// A new management credential was created.
    /// 已创建新的管理凭据。
    Created { reference: String },
    /// An existing management credential was replaced.
    /// 已替换现有管理凭据。
    Replaced {
        reference: String,
        previous: Box<CredentialSecretEnvelope>,
        replacement: Box<CredentialSecretEnvelope>,
    },
    /// An existing management credential was removed.
    /// 已移除现有管理凭据。
    Removed {
        reference: String,
        previous: Box<CredentialSecretEnvelope>,
    },
}

impl ManagementSecretMutation {
    /// Resolves the connection reference produced by this mutation.
    /// 解析此变更产生的连接引用。
    fn resulting_reference(&self, current: &CredentialConnection) -> Option<String> {
        match self {
            Self::Unchanged => current.management_secret_reference.clone(),
            Self::Created { reference } | Self::Replaced { reference, .. } => {
                Some(reference.clone())
            }
            Self::Removed { .. } => None,
        }
    }

    /// Returns whether the resulting key must be verified against the provider.
    /// 返回产生的密钥是否必须向供应商验证。
    fn requires_validation(&self) -> bool {
        matches!(self, Self::Created { .. } | Self::Replaced { .. })
    }

    /// Returns whether this mutation removes the allowance-query credential.
    /// 返回此变更是否移除用量查询凭据。
    fn removes_key(&self) -> bool {
        matches!(self, Self::Removed { .. })
    }
}

/// Applies the requested optional management-key edit before configuration persistence.
/// 在配置持久化前应用请求的可选管理密钥编辑。
async fn prepare_management_secret_mutation(
    secret_store: &dyn CredentialSecretStore,
    current: &CredentialConnection,
    management_key: Option<&str>,
    remove_management_key: bool,
) -> Result<ManagementSecretMutation, String> {
    if remove_management_key {
        let Some(reference) = current.management_secret_reference.as_deref() else {
            return Ok(ManagementSecretMutation::Unchanged);
        };
        let previous = CredentialSecretResolver::read_secret(secret_store, reference).await?;
        CredentialSecretStore::delete_secret(secret_store, reference).await?;
        return Ok(ManagementSecretMutation::Removed {
            reference: reference.to_string(),
            previous: Box::new(previous),
        });
    }
    let Some(value) = management_key.filter(|value| !value.trim().is_empty()) else {
        return Ok(ManagementSecretMutation::Unchanged);
    };
    if let Some(reference) = current.management_secret_reference.as_deref() {
        let previous = CredentialSecretResolver::read_secret(secret_store, reference).await?;
        let replacement = previous.replace_api_key(value.to_string())?;
        CredentialSecretStore::replace_secret(
            secret_store,
            reference,
            previous.revision,
            &replacement,
        )
        .await?;
        return Ok(ManagementSecretMutation::Replaced {
            reference: reference.to_string(),
            previous: Box::new(previous),
            replacement: Box::new(replacement),
        });
    }
    let reference = format!("ai_management_secret_{}", uuid::Uuid::new_v4().simple());
    let secret = CredentialSecretEnvelope::new_api_key(value.to_string())?;
    CredentialSecretStore::create_secret(secret_store, reference.as_str(), &secret).await?;
    Ok(ManagementSecretMutation::Created { reference })
}

/// Reverts one provider-management secret mutation.
/// 回滚一次供应商管理秘密变更。
async fn rollback_management_secret_mutation(
    secret_store: &dyn CredentialSecretStore,
    mutation: &ManagementSecretMutation,
) -> Result<(), String> {
    match mutation {
        ManagementSecretMutation::Unchanged => Ok(()),
        ManagementSecretMutation::Created { reference } => {
            CredentialSecretStore::delete_secret(secret_store, reference).await
        }
        ManagementSecretMutation::Replaced {
            reference,
            previous,
            replacement,
        } => {
            let rollback = replacement.advance_with_material(previous.material.clone())?;
            CredentialSecretStore::replace_secret(
                secret_store,
                reference,
                replacement.revision,
                &rollback,
            )
            .await
        }
        ManagementSecretMutation::Removed {
            reference,
            previous,
        } => {
            CredentialSecretStore::restore_secret(secret_store, reference, previous.as_ref()).await
        }
    }
}

/// Deletes the primary and optional management secrets as one cleanup operation.
/// 将主秘密和可选管理秘密作为一次清理操作删除。
async fn delete_connection_secrets(
    secret_store: &dyn CredentialSecretStore,
    primary_reference: &str,
    management_reference: Option<&str>,
) -> Result<(), String> {
    CredentialSecretStore::delete_secret(secret_store, primary_reference).await?;
    if let Some(reference) = management_reference
        && let Err(error) = CredentialSecretStore::delete_secret(secret_store, reference).await
    {
        return Err(format!(
            "Primary secret was deleted but management secret cleanup failed: {error}"
        ));
    }
    Ok(())
}

/// Restores one connection's exact primary and optional management secret before-images.
/// 恢复一个连接的精确主秘密与可选管理秘密前镜像。
///
/// # Parameters
/// 参数
/// `secret_store` owns durable secret state; `primary_reference` and `primary_secret` identify the
/// required credential; `management_secret` optionally supplies the exact secondary reference and
/// envelope.
/// `secret_store` 持有持久秘密状态；`primary_reference` 与 `primary_secret` 标识必需凭据；
/// `management_secret` 可选提供精确次级引用与信封。
///
/// # Returns
/// 返回值
/// Success only when every supplied secret was restored; otherwise all restoration failures are
/// preserved.
/// 仅当全部已提供秘密均恢复时返回成功，否则保留所有恢复失败。
async fn restore_connection_secrets(
    secret_store: &dyn CredentialSecretStore,
    primary_reference: &str,
    primary_secret: &CredentialSecretEnvelope,
    management_secret: Option<(&str, &CredentialSecretEnvelope)>,
) -> Result<(), String> {
    // primaryRestore returns the required credential to its exact revision and material.
    // primaryRestore 将必需凭据恢复到精确修订与材料。
    let primary_restore =
        CredentialSecretStore::restore_secret(secret_store, primary_reference, primary_secret)
            .await;
    // managementRestore independently restores the declared secondary credential when present.
    // managementRestore 在存在时独立恢复已声明的次级凭据。
    let management_restore = if let Some((reference, secret)) = management_secret {
        CredentialSecretStore::restore_secret(secret_store, reference, secret).await
    } else {
        Ok(())
    };
    combine_results(primary_restore, management_restore)
}

/// Combines two independent rollback results without hiding either failure.
/// 合并两个独立回滚结果且不隐藏任何失败。
fn combine_results(first: Result<(), String>, second: Result<(), String>) -> Result<(), String> {
    match (first, second) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(first), Err(second)) => Err(format!("{first}; {second}")),
    }
}

/// Preserves a primary failure while exposing any failed compensation step.
/// 保留主失败，并显式暴露任何失败的补偿步骤。
///
/// # Parameters
/// 参数
/// `primary` describes the original operation failure, while `first` and `second`
/// are independent cleanup or state-restoration results.
/// `primary` 描述原始操作失败，`first` 与 `second` 是彼此独立的清理或状态恢复结果。
///
/// # Returns
/// 返回值
/// The primary error with deterministic compensation diagnostics appended.
/// 返回追加确定性补偿诊断后的主错误。
fn compensated_operation_error(
    primary: String,
    first: Result<(), String>,
    second: Result<(), String>,
) -> String {
    let mut errors = Vec::new();
    if let Err(error) = first {
        errors.push(format!("first compensation failed: {error}"));
    }
    if let Err(error) = second {
        errors.push(format!("second compensation failed: {error}"));
    }
    if errors.is_empty() {
        primary
    } else {
        format!("{primary} Compensation diagnostics: {}.", errors.join("; "))
    }
}

/// Restores one exact persisted/runtime configuration image after pre-publication failure.
/// 在发布前失败后恢复一份精确的持久化/运行时配置镜像。
///
/// # Parameters
/// 参数
/// `state` resolves persistence, `runtime` owns the current activation, `before_configuration`
/// and `before_runtime` are the exact paired before-images, and `restore_error_context` labels
/// a failed durable compensation.
/// `state` 解析持久化，`runtime` 持有当前激活；`before_configuration` 与 `before_runtime`
/// 是精确配对的前镜像；`restore_error_context` 标记持久补偿失败。
///
/// # Returns
/// 返回值
/// Success only when both the durable configuration and every runtime catalog field return to
/// their exact before-image; otherwise one deterministic uncertainty diagnostic.
/// 仅当持久配置与全部运行时目录字段均返回精确前镜像时成功；否则返回确定性不确定状态诊断。
fn restore_public_ai_configuration_activation_in_transaction(
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    before_runtime: &vulcan_provider_adapters::ProviderRuntimeActivationSnapshot,
    restore_error_context: &str,
) -> Result<(), String> {
    // configurationRestore returns the standalone custom manifests and central document to the
    // exact durable before-image before the runtime maps are restored exactly.
    // configurationRestore 在精确恢复运行时映射前，把独立自定义清单与中央文档恢复到
    // 精确持久前镜像。
    let configuration_restore = mutate_public_ai_configuration_in_transaction(
        state,
        restore_error_context,
        |configuration| {
            *configuration = before_configuration.clone();
            Ok(())
        },
    )
    .map(|_| ());
    // runtimeRestore uses the post-compensation image as the CAS compare side so an unrelated
    // mutation is surfaced instead of overwritten.
    // runtimeRestore 使用补偿后的镜像作为 CAS 比较端，使无关变更被显式报告而不是遭到覆盖。
    let runtime_restore = runtime
        .capture_activation_snapshot()
        .and_then(|current_runtime| {
            runtime.restore_activation_snapshot(before_runtime, &current_runtime)
        });
    match (configuration_restore, runtime_restore) {
        (Ok(()), Ok(())) => Ok(()),
        (configuration, runtime) => Err(format!(
            "Public AI configuration activation restoration was incomplete: configuration={configuration:?}; runtime={runtime:?}"
        )),
    }
}

/// Restores one exact runtime-only activation image through compare-and-swap.
/// 通过比较交换恢复一份精确的仅运行时激活镜像。
///
/// # Parameters
/// 参数
/// `runtime` owns the current generation and `before_runtime` is the exact image captured before a
/// validation, discovery, or usage side effect that did not yet persist configuration.
/// `runtime` 持有当前代际，`before_runtime` 是在尚未持久化配置的校验、发现或用量副作用前
/// 捕获的精确镜像。
///
/// # Returns
/// 返回值
/// Success after exact restoration, or an explicit snapshot/CAS failure without overwriting
/// unrelated state.
/// 精确恢复后返回成功；若快照或比较交换失败则显式报错，且不覆盖无关状态。
fn restore_public_ai_runtime_activation(
    runtime: &PublicAiRuntime,
    before_runtime: &vulcan_provider_adapters::ProviderRuntimeActivationSnapshot,
) -> Result<(), String> {
    // currentRuntime is the exact compare side that prevents blind restoration.
    // currentRuntime 是防止盲目恢复的精确比较端。
    let current_runtime = runtime.capture_activation_snapshot()?;
    runtime
        .restore_activation_snapshot(before_runtime, &current_runtime)
        .map(|_| ())
}

/// Mutates configuration and atomically publishes its Workbench/response projection or restores it.
/// 变更配置并原子发布配套工作台/响应投影，失败时恢复原状。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `runtime` own the publication boundary; `before_configuration` is the
/// exact durable baseline; `save_error_context` labels the forward write; `mutate` builds the
/// candidate; `prepare_output` completes every fallible response projection before Workbench
/// state changes.
/// `app`、`state` 与 `runtime` 持有发布边界；`before_configuration` 是精确持久基线；
/// `save_error_context` 标记正向写入；`mutate` 构建候选；`prepare_output` 在工作台状态改变前
/// 完成全部可失败响应投影。
///
/// # Returns
/// 返回值
/// The installed configuration and prepared response only after Workbench publication succeeds;
/// any later failure restores the paired durable/runtime before-image.
/// 仅在工作台发布成功后返回已安装配置与预备响应；任何后续失败都会恢复配对的持久/运行时前镜像。
fn mutate_public_ai_configuration_and_publish_in_transaction<Mutation, PrepareOutput, Output>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    save_error_context: &str,
    mutate: Mutation,
    prepare_output: PrepareOutput,
) -> Result<(PersistedAiConfiguration, Output), String>
where
    Mutation: FnOnce(&mut PersistedAiConfiguration) -> Result<(), String>,
    PrepareOutput: FnOnce(&PersistedAiConfiguration) -> Result<Output, String>,
{
    mutate_public_ai_configuration_and_publish_using_in_transaction(
        state,
        runtime,
        before_configuration,
        save_error_context,
        mutate,
        prepare_output,
        |configuration| publish_public_ai_workbench(app, state, runtime, configuration).map(|_| ()),
    )
}

/// Mutates provider-tool configuration without reconciling unrelated session model routes.
/// 修改供应商工具配置时不协调无关的会话模型路由。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `runtime` own the publication boundary; `before_configuration` is the
/// durable before-image; `save_error_context` labels persistence; `mutate` builds the candidate;
/// `prepare_output` completes the response projection.
/// `app`、`state` 与 `runtime` 持有发布边界；`before_configuration` 是持久化前镜像；
/// `save_error_context` 标记持久化；`mutate` 构建候选；`prepare_output` 完成响应投影。
///
/// # Returns
/// 返回值
/// The installed configuration and output after provider-tool publication, or a compensated error.
/// 供应商工具发布后的已安装配置与输出，或已完成补偿的错误。
fn mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction<
    Mutation,
    PrepareOutput,
    Output,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    save_error_context: &str,
    mutate: Mutation,
    prepare_output: PrepareOutput,
) -> Result<(PersistedAiConfiguration, Output), String>
where
    Mutation: FnOnce(&mut PersistedAiConfiguration) -> Result<(), String>,
    PrepareOutput: FnOnce(&PersistedAiConfiguration) -> Result<Output, String>,
{
    mutate_public_ai_configuration_and_publish_using_in_transaction(
        state,
        runtime,
        before_configuration,
        save_error_context,
        mutate,
        prepare_output,
        |configuration| {
            publish_public_ai_workbench_without_session_model_reconciliation(
                app,
                state,
                runtime,
                configuration,
            )
            .map(|_| ())
        },
    )
}

/// Mutates configuration and atomically publishes explicit session-route revocations.
/// 修改配置并原子发布显式会话路由撤销。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `before_configuration` own the activation boundary;
/// `save_error_context` labels persistence; `mutate` builds the candidate; `prepare_output`
/// completes the response; `reconcile_sessions` updates a detached workspace and returns exact
/// lease owners to revoke; `persistence_reason` labels session-state persistence.
/// `app`、`state`、`runtime` 与 `before_configuration` 持有激活边界；`save_error_context` 标记
/// 持久化；`mutate` 构建候选；`prepare_output` 完成响应；`reconcile_sessions` 更新独立工作区并
/// 返回要撤销租约的精确所有者；`persistence_reason` 标记会话状态持久化。
///
/// # Returns
/// 返回值
/// The installed configuration and prepared output only after configuration, leases, workspace,
/// and Workbench commit together; any failure restores configuration/runtime and leases.
/// 仅当配置、租约、工作区与工作台共同提交后返回已安装配置及预备输出；任何失败都会恢复
/// 配置、运行时与租约。
fn mutate_public_ai_configuration_and_publish_sessions_in_transaction<
    Mutation,
    PrepareOutput,
    ReconcileSessions,
    Output,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    save_error_context: &str,
    mutate: Mutation,
    prepare_output: PrepareOutput,
    reconcile_sessions: ReconcileSessions,
    persistence_reason: &'static str,
) -> Result<(PersistedAiConfiguration, Output), String>
where
    Mutation: FnOnce(&mut PersistedAiConfiguration) -> Result<(), String>,
    PrepareOutput: FnOnce(&PersistedAiConfiguration) -> Result<Output, String>,
    ReconcileSessions: FnOnce(
        &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    ) -> Result<BTreeSet<crate::SessionId>, String>,
{
    mutate_public_ai_configuration_and_publish_using_in_transaction(
        state,
        runtime,
        before_configuration,
        save_error_context,
        mutate,
        prepare_output,
        |configuration| {
            publish_public_ai_workbench_with_session_reconciliation_and_commit(
                app,
                state,
                runtime,
                configuration,
                reconcile_sessions,
                persistence_reason,
                || Ok(()),
            )
            .map(|_| ())
        },
    )
}

/// Executes one reversible configuration activation with a caller-selected publication boundary.
/// 使用调用方选择的发布边界执行一次可逆配置激活。
///
/// # Parameters
/// 参数
/// `state`, `runtime`, and `before_configuration` identify the exact before-image;
/// `save_error_context` labels persistence; `mutate` builds the candidate; `prepare_output`
/// completes the response; `publish` atomically commits the browser/session projection.
/// `state`、`runtime` 与 `before_configuration` 标识精确前镜像；`save_error_context` 标记持久化；
/// `mutate` 构建候选；`prepare_output` 完成响应；`publish` 原子提交浏览器与会话投影。
///
/// # Returns
/// 返回值
/// The installed configuration and output after publication, or a compensated error.
/// 发布后返回已安装配置与输出，失败则返回已补偿错误。
fn mutate_public_ai_configuration_and_publish_using_in_transaction<
    Mutation,
    PrepareOutput,
    Publish,
    Output,
>(
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    save_error_context: &str,
    mutate: Mutation,
    prepare_output: PrepareOutput,
    publish: Publish,
) -> Result<(PersistedAiConfiguration, Output), String>
where
    Mutation: FnOnce(&mut PersistedAiConfiguration) -> Result<(), String>,
    PrepareOutput: FnOnce(&PersistedAiConfiguration) -> Result<Output, String>,
    Publish: FnOnce(&PersistedAiConfiguration) -> Result<(), String>,
{
    // runtimeBeforeImage pairs every catalog/status/cache field with the durable baseline.
    // runtimeBeforeImage 把每个目录、状态与缓存字段同持久基线配对。
    let runtime_before_image = runtime.capture_activation_snapshot()?;
    // configuration crosses the durable/runtime activation boundary before any browser state.
    // configuration 在任何浏览器状态前越过持久/运行时激活边界。
    let configuration =
        mutate_public_ai_configuration_in_transaction(state, save_error_context, mutate)?;
    // publicationResult prepares the complete RPC result before the atomic Workbench swap.
    // publicationResult 在原子工作台交换前准备完整 RPC 结果。
    let publication_result =
        prepare_output(&configuration).and_then(|output| publish(&configuration).map(|_| output));
    match publication_result {
        Ok(output) => Ok((configuration, output)),
        Err(primary) => {
            // restoration owns every configuration/runtime field changed after the before-image.
            // restoration 持有前镜像后发生变更的全部配置/运行时字段。
            let restoration = restore_public_ai_configuration_activation_in_transaction(
                state,
                runtime,
                before_configuration,
                &runtime_before_image,
                "恢复公共 AI 配置发布前状态失败",
            );
            Err(compensated_operation_error(primary, restoration, Ok(())))
        }
    }
}

/// Deletes every unique credential secret owned by a provider and compensates partial deletion.
/// 删除供应商持有的全部唯一凭据秘密，并对部分删除执行补偿恢复。
async fn delete_provider_credential_secrets(
    secret_store: &dyn CredentialSecretStore,
    connections: &[CredentialConnection],
) -> Result<BTreeMap<String, CredentialSecretEnvelope>, String> {
    let mut references = BTreeSet::new();
    for connection in connections {
        references.insert(connection.secret_reference.clone());
        if let Some(reference) = connection.management_secret_reference.as_ref() {
            references.insert(reference.clone());
        }
    }

    let mut secrets = BTreeMap::new();
    for reference in references {
        let secret =
            CredentialSecretResolver::read_secret(secret_store, reference.as_str()).await?;
        secrets.insert(reference, secret);
    }

    let mut deleted = BTreeMap::new();
    for (reference, secret) in secrets {
        if let Err(error) =
            CredentialSecretStore::delete_secret(secret_store, reference.as_str()).await
        {
            let restore_current =
                CredentialSecretStore::restore_secret(secret_store, reference.as_str(), &secret)
                    .await;
            let restore_previous =
                restore_provider_credential_secrets(secret_store, &deleted).await;
            return Err(compensated_operation_error(
                format!("Failed to delete provider credential `{reference}`: {error}"),
                restore_current,
                restore_previous,
            ));
        }
        deleted.insert(reference, secret);
    }
    Ok(deleted)
}

/// Restores credential secrets deleted during a provider removal attempt.
/// 恢复供应商移除尝试期间已经删除的凭据秘密。
async fn restore_provider_credential_secrets(
    secret_store: &dyn CredentialSecretStore,
    secrets: &BTreeMap<String, CredentialSecretEnvelope>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for (reference, secret) in secrets {
        if let Err(error) =
            CredentialSecretStore::restore_secret(secret_store, reference.as_str(), secret).await
        {
            errors.push(format!("`{reference}`: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Failed to restore provider credentials: {}.",
            errors.join("; ")
        ))
    }
}

/// Saves one explicit provider default connection.
/// 保存一个显式供应商默认连接。
pub(super) async fn service_save_public_ai_default_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiDefaultConnectionSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction retains writer ownership through durable, runtime, Workbench, and response publication.
    // transaction 在持久、运行时、工作台与响应发布全程保留写入所有权。
    let transaction = begin_public_ai_configuration_transaction().await;
    // currentConfiguration is the exact paired before-image for rollback.
    // currentConfiguration 是用于回滚的精确配对前镜像。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // snapshot is prepared and published inside the same compensating activation transaction.
    // snapshot 在同一补偿激活事务内完成预备与发布。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存公共 AI 默认账号失败",
        |configuration| {
            let connection = configuration
                .connection_graph
                .connections
                .get(request.connection_id.as_str())
                .ok_or_else(|| {
                    format!("Connection `{}` is not configured.", request.connection_id)
                })?;
            if connection.provider_id != request.provider_id {
                return Err(format!(
                    "Connection `{}` does not belong to provider `{}`.",
                    request.connection_id, request.provider_id
                ));
            }
            if configuration.connection_is_pending_validation(request.connection_id.as_str()) {
                return Err(format!(
                    "Connection `{}` is pending explicit model validation and cannot become a provider default.",
                    request.connection_id
                ));
            }
            configuration
                .connection_graph
                .account_preferences
                .provider_defaults
                .insert(request.provider_id, request.connection_id);
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Removes one public AI credential connection and its secret.
/// 移除一个公共 AI 凭据连接及其秘密。
pub(super) async fn service_remove_public_ai_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionRemoveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let connection_id = request.connection_id;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let catalogs = runtime.catalog_snapshot()?;
    // runtimeBeforeImage owns every runtime field removed after durable configuration activation.
    // runtimeBeforeImage 持有持久配置激活后将删除的每个运行时字段。
    let runtime_before_image = runtime.capture_activation_snapshot()?;
    let removed = current_configuration
        .connection_graph
        .connections
        .get(connection_id.as_str())
        .cloned()
        .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
    let secret_store = runtime.credential_secret_store();
    let removed_secret = CredentialSecretResolver::read_secret(
        secret_store.as_ref(),
        removed.secret_reference.as_str(),
    )
    .await?;
    let removed_management_secret =
        if let Some(reference) = removed.management_secret_reference.as_deref() {
            Some(CredentialSecretResolver::read_secret(secret_store.as_ref(), reference).await?)
        } else {
            None
        };
    CredentialSecretStore::delete_secret(secret_store.as_ref(), removed.secret_reference.as_str())
        .await?;
    if let Some(reference) = removed.management_secret_reference.as_deref()
        && let Err(error) =
            CredentialSecretStore::delete_secret(secret_store.as_ref(), reference).await
    {
        let restore = CredentialSecretStore::restore_secret(
            secret_store.as_ref(),
            removed.secret_reference.as_str(),
            &removed_secret,
        )
        .await;
        return Err(compensated_operation_error(error, restore, Ok(())));
    }
    let remove_result = mutate_public_ai_configuration_in_transaction(
        &state,
        "移除公共 AI 连接失败",
        |configuration| {
            remove_connection_from_configuration(
                configuration,
                connection_id.as_str(),
                request.replacement_default_connection_id.as_deref(),
            )?;
            ensure_active_model_selection(runtime.as_ref(), configuration, &catalogs)?;
            Ok(())
        },
    );
    let configuration = match remove_result {
        Ok(configuration) => configuration,
        Err(error) => {
            let secret_restore = restore_connection_secrets(
                secret_store.as_ref(),
                removed.secret_reference.as_str(),
                &removed_secret,
                removed
                    .management_secret_reference
                    .as_deref()
                    .zip(removed_management_secret.as_ref()),
            )
            .await;
            let activation_restore = restore_public_ai_configuration_activation_in_transaction(
                &state,
                runtime.as_ref(),
                &current_configuration,
                &runtime_before_image,
                "恢复公共 AI 连接移除前配置失败",
            );
            return Err(compensated_operation_error(
                error,
                secret_restore,
                activation_restore,
            ));
        }
    };
    // fallback is the exact post-removal global selection used for every revoked owner.
    // fallback 是用于每个被撤销所有者的精确移除后全局选择。
    let fallback = configuration.active.clone();
    let publication_result = (|| {
        runtime.remove_connection_catalog(connection_id.as_str())?;
        runtime.remove_connection_usage(connection_id.as_str())?;
        // snapshot finishes the complete RPC projection before browser-visible state changes.
        // snapshot 在浏览器可见状态改变前完成完整 RPC 投影。
        let snapshot = build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)?;
        publish_public_ai_workbench_with_session_reconciliation_and_commit(
            &app,
            &state,
            runtime.as_ref(),
            &configuration,
            |workspace| {
                Ok(reconcile_disabled_connection_sessions(
                    workspace,
                    connection_id.as_str(),
                    None,
                    fallback.as_ref(),
                ))
            },
            "connection_removed",
            || Ok(()),
        )?;
        Ok::<_, String>(snapshot)
    })();
    match publication_result {
        Ok(snapshot) => {
            drop(transaction);
            Ok(snapshot)
        }
        Err(primary) => {
            let secret_restore = restore_connection_secrets(
                secret_store.as_ref(),
                removed.secret_reference.as_str(),
                &removed_secret,
                removed
                    .management_secret_reference
                    .as_deref()
                    .zip(removed_management_secret.as_ref()),
            )
            .await;
            let activation_restore = restore_public_ai_configuration_activation_in_transaction(
                &state,
                runtime.as_ref(),
                &current_configuration,
                &runtime_before_image,
                "恢复公共 AI 连接移除前配置失败",
            );
            drop(transaction);
            Err(compensated_operation_error(
                primary,
                secret_restore,
                activation_restore,
            ))
        }
    }
}

/// Refreshes one connection's declared runtime metadata on demand.
/// 按需刷新一个连接声明的运行时元数据。
pub(super) async fn service_sync_public_ai_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    connection_id: String,
) -> Result<PublicAiSettingsSnapshot, String> {
    // Manual synchronization shares the configuration transaction with overlay mutations so its
    // remote response cannot replace a newer in-memory projection with stale user settings.
    // 手动同步与覆盖配置变更共用配置事务，避免远程响应以过期用户设置替换较新的内存投影。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    // runtimeBeforeImage restores an unpublished refresh candidate without discarding a published observation.
    // runtimeBeforeImage 恢复未发布的刷新候选，但不会丢弃已发布观测。
    let runtime_before_image = runtime.capture_activation_snapshot()?;
    let connection = configuration
        .connection_graph
        .connections
        .get(connection_id.as_str())
        .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
    let entry_kind = registered_service_entry_kind(
        runtime.as_ref(),
        &configuration,
        connection.provider_id.as_str(),
        connection.service_entry_id.as_str(),
    )?;
    let refresh_result = validate_connection_credential(
        runtime.as_ref(),
        &configuration,
        connection_id.as_str(),
        entry_kind,
    )
    .await;
    // activation_result publishes either the refreshed source or the existing/manual fallback
    // as one immutable generation even when remote discovery itself reports a controlled failure.
    // activation_result 即使远程发现返回受控失败，也会把刷新来源或现有/手工回退作为一个
    // 不可变代际发布。
    let activation_result = runtime.reapply_model_catalog_overlays(&configuration);
    if let Err(primary) = activation_result {
        let runtime_restore =
            restore_public_ai_runtime_activation(runtime.as_ref(), &runtime_before_image);
        drop(transaction);
        return Err(compensated_operation_error(
            primary,
            runtime_restore,
            Ok(()),
        ));
    }
    // publicationResult repairs an invalid global account binding against the refreshed catalog
    // before exposing the new generation. Existing session leases remain untouched.
    // publicationResult 在公开新代际前依据刷新后的目录修复失效的全局账号绑定；既有会话租约保持不变。
    let publication_result = (|| {
        // refreshedCatalogs is the exact effective account inventory published above.
        // refreshedCatalogs 是上方已发布的精确账号有效目录。
        let refreshed_catalogs = runtime.catalog_snapshot()?;
        // repairedConfiguration changes only the global fallback used by future sessions.
        // repairedConfiguration 只调整未来会话使用的全局回退选择。
        let mut repaired_configuration = configuration.clone();
        ensure_active_model_selection(
            runtime.as_ref(),
            &mut repaired_configuration,
            &refreshed_catalogs,
        )?;
        if repaired_configuration.active != configuration.active {
            // repairedActive is copied into the latest transaction snapshot and persisted before
            // Workbench publication, preventing another restart from restoring the invalid pair.
            // repairedActive 会写入最新事务快照并在工作台发布前持久化，避免重启后恢复无效组合。
            let repaired_active = repaired_configuration.active.clone();
            let (_installed_configuration, snapshot) =
                mutate_public_ai_configuration_and_publish_in_transaction(
                    &app,
                    &state,
                    runtime.as_ref(),
                    &configuration,
                    "修复公共 AI 活动模型账号失败",
                    |candidate| {
                        candidate.active = repaired_active;
                        Ok(())
                    },
                    |candidate| build_public_ai_settings_snapshot(runtime.as_ref(), candidate),
                )?;
            Ok(snapshot)
        } else {
            build_public_ai_settings_snapshot(runtime.as_ref(), &configuration).and_then(
                |snapshot| {
                    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)
                        .map(|_| snapshot)
                },
            )
        }
    })();
    let snapshot = match publication_result {
        Ok(snapshot) => snapshot,
        Err(primary) => {
            let runtime_restore =
                restore_public_ai_runtime_activation(runtime.as_ref(), &runtime_before_image);
            drop(transaction);
            return Err(compensated_operation_error(
                primary,
                runtime_restore,
                Ok(()),
            ));
        }
    };
    drop(transaction);
    refresh_result?;
    Ok(snapshot)
}

/// Runs the production provider-manifest sync call chain for every system provider.
/// 为每个系统供应商执行生产清单同步调用链。
///
/// The sync service remains unconfigured until the external confirmation gate opens; the
/// production call chain exists and reports `not_configured` without guessing any remote
/// endpoint (plan §7.2/§17.3).
/// 同步服务在外部确认门开启前保持未配置；生产调用链存在并报告 `not_configured`，
/// 绝不猜测任何远端端点（方案 §7.2/§17.3）。
/// Activates every affected connection catalog and captures the exact published runtime image.
/// 激活每个受影响连接目录并捕获精确的已发布运行时镜像。
///
/// # Parameters
/// 参数
/// `provider_ids` contains only providers whose manifest generation was newly activated.
/// `provider_ids` 只包含清单代际刚完成激活的供应商。
///
/// # Returns
/// 返回值
/// The exact post-activation image used as compensation's compare-and-swap guard.
/// 返回作为补偿比较交换护栏的精确激活后镜像。
async fn activate_synced_provider_catalogs(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_ids: &std::collections::BTreeSet<String>,
    transaction: &vulcan_provider_adapters::ProviderCatalogActivationTransaction,
) -> Result<vulcan_provider_adapters::ProviderRuntimeActivationSnapshot, String> {
    // connectionIds is the complete deterministic projection set for the affected providers.
    // connectionIds 是受影响供应商的完整确定性投影集合。
    let connection_ids = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| provider_ids.contains(&connection.provider_id))
        .map(|connection| connection.id.clone())
        .collect::<Vec<_>>();
    runtime
        .activate_staged_manifest_catalogs_in_transaction(
            configuration,
            &connection_ids,
            transaction,
        )
        .await
}

/// Closed result of a failed provider projection publication transaction.
/// 供应商投影发布事务失败后的封闭结果。
#[derive(Debug)]
enum ProviderProjectionPublicationFailure {
    /// Every disk, staged-manifest, runtime, and Workbench before-image was restored exactly.
    /// 每份磁盘、暂存 Manifest、运行时与工作台前镜像均已精确恢复。
    Compensated(String),
    /// At least one compensation failed, so continuing could publish from an uncertain authority.
    /// 至少一项补偿失败，继续执行可能从状态不确定的权威发布。
    Uncertain(String),
}

/// Restores one staged provider activation when its asynchronous publication future is cancelled.
/// 在异步发布 Future 被取消时恢复一次已暂存供应商激活。
struct StagedProviderActivationCancellationGuard<'a> {
    /// Service owning the durable Manifest transaction and compensation methods.
    /// 持有持久化 Manifest 事务与补偿方法的服务。
    service: &'a crate::provider_manifest_sync::ProviderManifestSyncService,
    /// Runtime owning the staged manifest catalog that must match the durable before-image.
    /// 持有必须与持久化前镜像一致的暂存 Manifest 目录的运行时。
    runtime: &'a PublicAiRuntime,
    /// Reports identifying the staged activation for provider-scoped telemetry.
    /// 标识已暂存激活并用于逐供应商遥测的报告。
    reports: &'a [crate::provider_manifest_sync::ProviderSyncReport],
    /// Exact provider-scoped before-images restored on cancellation.
    /// 在取消时恢复的精确逐供应商前镜像。
    snapshots: &'a [crate::provider_manifest_sync::ProviderManifestActivationSnapshot],
    /// Armed state retained only while an asynchronous cancellation point can still be crossed.
    /// 仅在仍可能越过异步取消点时保持的启用状态。
    armed: bool,
}

impl<'a> StagedProviderActivationCancellationGuard<'a> {
    /// Arms cancellation compensation for one exact staged activation set.
    /// 为一组精确已暂存激活启用取消补偿。
    ///
    /// # Parameters
    /// 参数
    /// `service`, `runtime`, `reports`, and `snapshots` are the complete transaction authority.
    /// `service`、`runtime`、`reports` 与 `snapshots` 构成完整事务权威。
    ///
    /// # Returns
    /// 返回值
    /// An armed guard that restores before-images if dropped before `disarm`.
    /// 返回一个已启用护栏；若在 `disarm` 前被丢弃则恢复前镜像。
    fn new(
        service: &'a crate::provider_manifest_sync::ProviderManifestSyncService,
        runtime: &'a PublicAiRuntime,
        reports: &'a [crate::provider_manifest_sync::ProviderSyncReport],
        snapshots: &'a [crate::provider_manifest_sync::ProviderManifestActivationSnapshot],
    ) -> Self {
        Self {
            service,
            runtime,
            reports,
            snapshots,
            armed: true,
        }
    }

    /// Disarms automatic compensation after execution has left every asynchronous cancellation point.
    /// 在执行离开全部异步取消点后停用自动补偿。
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagedProviderActivationCancellationGuard<'_> {
    /// Restores staged disk/runtime ownership on asynchronous cancellation.
    /// 在异步取消时恢复已暂存磁盘与运行时所有权。
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // restoreResult is intentionally best-effort in Drop; an incomplete runtime restore keeps
        // the Applying journal for deterministic startup recovery and the process remains fail-closed.
        // restoreResult 在 Drop 中有意采用尽力恢复；运行时恢复不完整时会保留 Applying 日志，
        // 供启动确定性恢复，且当前进程保持失败关闭。
        let restore_result = self
            .service
            .restore_activation_snapshots(self.runtime, self.snapshots);
        if restore_result.is_ok() {
            self.service.record_compensated_rollback_events(
                self.runtime,
                self.reports,
                "activation_cancelled",
            );
        }
    }
}

/// Validates and extracts every exact Manifest activation before-image required for publication.
/// 校验并提取发布所需的每份精确 Manifest 激活前镜像。
///
/// # Parameters
/// 参数
/// `reports` are provider-scoped activation outcomes and `provider_ids` is the exact projection set.
/// `reports` 是逐供应商激活结果，`provider_ids` 是精确投影集合。
///
/// # Returns
/// 返回值
/// The complete ordered before-image list, or an uncertain contract failure that forbids publication.
/// 返回完整有序前镜像列表；若契约状态不确定则返回禁止发布的失败。
fn required_provider_activation_snapshots(
    reports: &[crate::provider_manifest_sync::ProviderSyncReport],
    provider_ids: &std::collections::BTreeSet<String>,
) -> Result<
    Vec<crate::provider_manifest_sync::ProviderManifestActivationSnapshot>,
    ProviderProjectionPublicationFailure,
> {
    let report_provider_ids = reports
        .iter()
        .map(|report| report.provider_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if reports.is_empty()
        || report_provider_ids.len() != reports.len()
        || &report_provider_ids != provider_ids
    {
        return Err(ProviderProjectionPublicationFailure::Uncertain(
            String::from("Provider activation reports do not match the exact projection set."),
        ));
    }
    reports
        .iter()
        .map(|report| {
            if report.outcome != "activated" {
                return Err(ProviderProjectionPublicationFailure::Uncertain(
                    String::from("Provider projection received a non-activation report."),
                ));
            }
            let snapshot = report.activation_snapshot.clone().ok_or_else(|| {
                ProviderProjectionPublicationFailure::Uncertain(String::from(
                    "Provider activation report is missing its exact before-image.",
                ))
            })?;
            if snapshot.provider_id != report.provider_id {
                return Err(ProviderProjectionPublicationFailure::Uncertain(
                    String::from(
                        "Provider activation before-image identity does not match its report.",
                    ),
                ));
            }
            Ok(snapshot)
        })
        .collect()
}

/// Restores staged Manifest ownership before any runtime generation was published.
/// 在任何运行时代际发布前恢复已暂存 Manifest 的所有权。
///
/// # Parameters
/// 参数
/// `service` and `runtime` own the durable and staged layers; `reports` identify telemetry;
/// `snapshots` are exact before-images; and `primary` is the preparation failure.
/// `service` 与 `runtime` 持有持久层和暂存层；`reports` 标识遥测；`snapshots` 是精确前镜像；
/// `primary` 是准备阶段失败。
///
/// # Returns
/// 返回值
/// A compensated provider-local failure only when every before-image is restored, otherwise uncertain.
/// 仅当每份前镜像都恢复时返回逐供应商已补偿失败，否则返回状态不确定。
fn compensate_unpublished_provider_activation(
    service: &crate::provider_manifest_sync::ProviderManifestSyncService,
    runtime: &PublicAiRuntime,
    reports: &[crate::provider_manifest_sync::ProviderSyncReport],
    snapshots: &[crate::provider_manifest_sync::ProviderManifestActivationSnapshot],
    primary: String,
) -> ProviderProjectionPublicationFailure {
    let manifest_restore = service.restore_activation_snapshots(runtime, snapshots);
    if manifest_restore.is_ok() {
        service.record_compensated_rollback_events(
            runtime,
            reports,
            "activation_publication_failed",
        );
        ProviderProjectionPublicationFailure::Compensated(primary)
    } else {
        ProviderProjectionPublicationFailure::Uncertain(format!(
            "{primary}; Manifest activation before-image restore: {manifest_restore:?}"
        ))
    }
}

/// Publishes post-sync projections or restores every exact activation before-image on failure.
/// 发布同步后投影，或在失败时恢复每份精确激活前镜像。
///
/// # Parameters
/// 参数
/// `service` owns the durable store, `reports` retain per-activation before-images, and
/// `provider_ids` is the exact projection set.
/// `service` 持有持久存储，`reports` 保留逐激活前镜像，`provider_ids` 是精确投影集合。
///
/// # Returns
/// 返回值
/// Success only after the new generation is fully visible; failures distinguish an exact
/// provider-local compensation from an uncertain shared state that must stop the batch.
/// 仅当新代际完全可见后返回成功；失败会区分精确逐供应商补偿与必须中止批次的共享状态不确定。
async fn publish_synced_provider_projections_or_restore(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    service: &crate::provider_manifest_sync::ProviderManifestSyncService,
    reports: &[crate::provider_manifest_sync::ProviderSyncReport],
    provider_ids: &std::collections::BTreeSet<String>,
) -> Result<(), ProviderProjectionPublicationFailure> {
    let manifest_snapshots = required_provider_activation_snapshots(reports, provider_ids)?;
    // cancellationGuard owns exact compensation across every following asynchronous wait.
    // cancellationGuard 在后续每个异步等待点期间持有精确补偿责任。
    let mut cancellation_guard = StagedProviderActivationCancellationGuard::new(
        service,
        runtime,
        reports,
        &manifest_snapshots,
    );
    // configurationTransaction prevents settings writes from crossing configuration capture,
    // publication, or compensation for this exact provider activation.
    // configurationTransaction 防止设置写入穿越当前精确供应商激活的配置捕获、发布或补偿。
    let _configuration_transaction = begin_public_ai_configuration_transaction().await;
    // configuration is captured only after the provider before-images are proven complete.
    // configuration 仅在供应商前镜像被证明完整后捕获。
    let configuration = match runtime.configuration_snapshot() {
        Ok(configuration) => configuration,
        Err(primary) => {
            // No asynchronous cancellation point remains before explicit compensation completes.
            // 显式补偿完成前已不存在异步取消点。
            cancellation_guard.disarm();
            return Err(compensate_unpublished_provider_activation(
                service,
                runtime,
                reports,
                &manifest_snapshots,
                format!("Provider configuration snapshot failed before projection: {primary}"),
            ));
        }
    };
    // catalogTransaction remains owned until success or every compensation/republish step ends.
    // catalogTransaction 持续持有到成功或全部补偿/重新发布步骤结束。
    let catalog_transaction = runtime.begin_catalog_activation_transaction().await;
    // runtimeBeforeImage is the single exact catalog/generation snapshot before batch publication.
    // runtimeBeforeImage 是批量发布前唯一的精确目录与代际快照。
    let runtime_before_image = match runtime.capture_activation_snapshot() {
        Ok(snapshot) => snapshot,
        Err(primary) => {
            // Explicit synchronous compensation owns all remaining work for this failure.
            // 本次失败的全部剩余工作由显式同步补偿持有。
            cancellation_guard.disarm();
            return Err(compensate_unpublished_provider_activation(
                service,
                runtime,
                reports,
                &manifest_snapshots,
                format!("Provider runtime before-image capture failed: {primary}"),
            ));
        }
    };
    // publication includes durable last-checked bookkeeping so an `activated` report is never
    // returned before its operator-visible success state exists.
    // publication 包含持久化最近检查簿记，使 `activated` 报告绝不会在操作员可见成功状态
    // 存在前返回。
    // runtimeAfterImage exists only after the isolated activation crossed its single publication
    // boundary. A preparation conflict leaves the main runtime untouched and therefore needs no
    // runtime compensation.
    // runtimeAfterImage 仅在隔离激活越过唯一发布边界后存在；准备冲突不会触碰主运行时，
    // 因而无需运行时补偿。
    // activationResult is the final asynchronous cancellation boundary in this transaction.
    // activationResult 是本事务中的最后一个异步取消边界。
    let activation_result = activate_synced_provider_catalogs(
        runtime,
        &configuration,
        provider_ids,
        &catalog_transaction,
    )
    .await;
    cancellation_guard.disarm();
    let (runtime_after_image, publication) = match activation_result {
        Ok(after_image) => {
            // publication keeps browser state unchanged while check-state persistence and journal
            // commit remain fallible. Once that closure succeeds, the Workbench swap is infallible.
            // publication 在检查状态持久化与日志提交仍可失败时保持浏览器状态不变；闭包成功后，
            // 工作台交换不再失败。
            let publication = publish_public_ai_workbench_with_commit(
                app,
                state,
                runtime,
                &configuration,
                || {
                    // Check state remains inside the Applying before-image until it is durable;
                    // only then may the journal cross its commit boundary.
                    // 检查状态在持久化完成前始终处于 Applying 前镜像保护内；之后日志才可越过提交边界。
                    service.record_sync_check_reports(reports)?;
                    service.commit_activation_snapshots(&manifest_snapshots)
                },
            )
            .map(|_| ());
            (Some(after_image), publication)
        }
        Err(error) => (None, Err(error)),
    };
    match publication {
        Ok(()) => {
            service.record_committed_activation_events(runtime, reports);
            Ok(())
        }
        Err(primary) => {
            let manifest_restore =
                service.restore_activation_snapshots(runtime, &manifest_snapshots);
            let runtime_restore = runtime_after_image.as_ref().map_or(Ok(()), |after_image| {
                runtime.restore_activation_snapshot(&runtime_before_image, after_image)
            });
            let workbench_republish = match (&runtime_restore, runtime_after_image.is_some()) {
                (Ok(()), true) => {
                    publish_public_ai_workbench(app, state, runtime, &configuration).map(|_| ())
                }
                (Ok(()), false) => Ok(()),
                (Err(_), _) => Err(String::from(
                    "Old-generation Workbench projection was not republished because runtime compensation failed.",
                )),
            };
            let compensation_complete =
                manifest_restore.is_ok() && runtime_restore.is_ok() && workbench_republish.is_ok();
            if compensation_complete {
                service.record_compensated_rollback_events(
                    runtime,
                    reports,
                    "activation_publication_failed",
                );
            }
            let diagnostic = format!(
                "Provider manifest projection publication failed: {primary}; manifest compensation: {manifest_restore:?}; runtime compensation: {runtime_restore:?}; old-generation Workbench republication: {workbench_republish:?}"
            );
            if compensation_complete {
                Err(ProviderProjectionPublicationFailure::Compensated(
                    diagnostic,
                ))
            } else {
                Err(ProviderProjectionPublicationFailure::Uncertain(diagnostic))
            }
        }
    }
}

/// Converts one fully compensated activation into a provider-local rejected sync report.
/// 把一次已完整补偿的激活转换为逐供应商拒绝同步报告。
///
/// # Parameters
/// 参数
/// `report` is the staged activation identity and `diagnostic` is the local compensation detail.
/// `report` 是已暂存激活身份，`diagnostic` 是本地补偿详情。
///
/// # Returns
/// 返回值
/// A serializable rejection with no retained before-image and one bounded public diagnostic.
/// 返回不再保留前镜像且仅含一条有界公开诊断的可序列化拒绝结果。
fn compensated_provider_projection_report(
    mut report: crate::provider_manifest_sync::ProviderSyncReport,
    diagnostic: &str,
) -> crate::provider_manifest_sync::ProviderSyncReport {
    report.outcome = "rejected";
    report.failure_kind = Some(String::from("activation_publication_failed"));
    report.failure_message =
        Some(crate::provider_manifest_sync::bounded_public_manifest_diagnostic(diagnostic));
    report.activation_snapshot = None;
    report
}

/// Finalizes one provider sync report as an isolated activation and check-state transaction.
/// 将一份供应商同步报告作为隔离的激活与检查状态事务完成。
///
/// # Parameters
/// 参数
/// `app` and `state` own the visible projection, `runtime` owns the immutable generation,
/// `service` owns durable Manifest layers, and `report` is the exact provider-scoped outcome.
/// `app` 与 `state` 持有可见投影，`runtime` 持有不可变代际，`service` 持有持久 Manifest 层，
/// `report` 是精确的逐供应商结果。
///
/// # Returns
/// 返回值
/// The same report only after its activation or non-activation check state is durably finalized.
/// 仅在对应激活或非激活检查状态持久完成后返回同一报告。
async fn finalize_provider_sync_report(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    service: &crate::provider_manifest_sync::ProviderManifestSyncService,
    mut report: crate::provider_manifest_sync::ProviderSyncReport,
) -> Result<crate::provider_manifest_sync::ProviderSyncReport, String> {
    if report.outcome != "activated" {
        if let Err(error) = service.record_sync_check_reports(std::slice::from_ref(&report)) {
            // primaryFailureKind preserves a candidate rejection when one already exists; a
            // successful remote outcome instead becomes an explicit check-state rejection.
            // primaryFailureKind 在候选已被拒绝时保留其分类；成功远端结果则转为明确的
            // check-state 拒绝。
            let primary_failure_kind = report
                .failure_kind
                .clone()
                .unwrap_or_else(|| String::from("check_state_publication_failed"));
            report.outcome = "rejected";
            report.failure_kind = Some(primary_failure_kind);
            report.failure_message = Some(
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str()),
            );
            report.activation_snapshot = None;
        }
        return Ok(report);
    }

    // providerIds deliberately contains one member so failure compensation cannot roll back an
    // unrelated provider that already committed earlier in the shared Index batch.
    // providerIds 有意只包含一个成员，使失败补偿不会回滚共享 Index 批次中较早提交的无关供应商。
    let provider_ids = std::collections::BTreeSet::from([report.provider_id.clone()]);
    // publicationResult owns the complete provider-scoped activation transaction.
    // publicationResult 持有完整的逐供应商激活事务。
    let publication_result = publish_synced_provider_projections_or_restore(
        app,
        state,
        runtime,
        service,
        std::slice::from_ref(&report),
        &provider_ids,
    )
    .await;
    match publication_result {
        Ok(()) => Ok(report),
        Err(ProviderProjectionPublicationFailure::Compensated(primary)) => {
            // rejectedReport closes this provider transaction without suppressing later providers
            // from the same authenticated Index snapshot (plan §17.4).
            // rejectedReport 封闭当前供应商事务，同时不压制同一认证 Index 快照中的后续
            // 供应商（方案 §17.4）。
            report = compensated_provider_projection_report(report, primary.as_str());
            if service
                .record_sync_check_reports(std::slice::from_ref(&report))
                .is_err()
            {
                report.failure_message = Some(String::from(
                    "Activation publication was rolled back, and its check-state update could not be persisted.",
                ));
            }
            Ok(report)
        }
        Err(ProviderProjectionPublicationFailure::Uncertain(primary)) => {
            // failureState is best-effort observability after an uncertain compensation; the
            // batch must stop regardless because catalog authority can no longer be proven.
            // failureState 是补偿状态不确定后的尽力可观测性；无论其结果如何都必须中止批次，
            // 因为目录权威已无法证明。
            let failure_state = service.record_sync_check_failures(
                std::slice::from_ref(&report.provider_id),
                "activation_publication_failed",
            );
            let primary =
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(primary.as_str());
            if failure_state.is_err() {
                Err(format!(
                    "{primary} Check-state publication also failed after compensation became uncertain."
                ))
            } else {
                Err(primary)
            }
        }
    }
}

/// Bounds and redacts one internal failure before it crosses a Provider Manifest RPC boundary.
/// 在内部失败越过 Provider Manifest RPC 边界前进行有界化与脱敏。
///
/// # Parameters
/// 参数
/// `error` is the exact internal diagnostic, which may contain local paths or oversized text.
/// `error` 是可能包含本地路径或超长文本的精确内部诊断。
///
/// # Returns
/// 返回值
/// A UTF-8-safe public diagnostic that retains no detected local path or secret assignment.
/// 返回 UTF-8 安全的公开诊断，且不保留检测到的本地路径或秘密赋值。
fn bounded_provider_manifest_rpc_error(error: String) -> String {
    crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
}

pub(super) async fn service_sync_provider_manifests(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<Vec<crate::provider_manifest_sync::ProviderSyncReport>, String> {
    let (runtime, _initial_configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    // The injected sync service is reused instead of constructing an empty-key/None-URL
    // service per RPC call (plan §R7.13).
    // 复用注入的同步服务，而不是每次 RPC 调用构造空密钥/None URL 的服务（方案 §R7.13）。
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    // Keep the single-writer guard through projection/compensation; releasing it after the
    // download would allow another trigger to replace the staged manifest before this call
    // publishes its matching catalogs and report.
    // 持有单写者守卫直到投影/补偿完成；若下载后立即释放，另一个触发器可能在本次调用
    // 发布匹配目录与报告前替换暂存清单。
    let _activation_guard = service.begin_activation_transaction().await;
    // providers includes every identity pinned by the trusted bundled index, including a
    // provider-local failed baseline that still requires periodic recovery checks.
    // providers 包含可信内置 Index 固定的全部身份，包括仍需周期恢复检查的逐供应商失败基线。
    let providers = runtime
        .system_manifest_provider_ids()
        .map_err(bounded_provider_manifest_rpc_error)?
        .into_iter()
        .collect::<Vec<_>>();
    // syncAllWithReportHandler fetches the signed Index exactly once, but completes each provider
    // before the next candidate is staged. This preserves both shared-index efficiency and the
    // independent transaction requirement (plan §17.4/§R7.7).
    // syncAllWithReportHandler 只获取一次签名 Index，但会在暂存下一候选前完成当前供应商；
    // 这样同时保留共享 Index 效率与独立事务要求（方案 §17.4/§R7.7）。
    // reportsResult retains top-level Index/network failures long enough to persist a classified
    // last-checked failure for every targeted provider.
    // reportsResult 保留顶层 Index/网络失败，以便为每个目标供应商持久化分类最近检查失败。
    let reports = match service
        .sync_all_with_report_handler(runtime.as_ref(), &providers, |report| {
            // Each future owns stable clones so it can await configuration and projection locks
            // without borrowing the synchronization loop's stack frame.
            // 每个 Future 持有稳定克隆，因此可等待配置与投影锁，而不借用同步循环栈帧。
            let app = app.clone();
            let state = state.clone();
            let runtime = Arc::clone(&runtime);
            let service = service.clone();
            async move {
                finalize_provider_sync_report(&app, &state, runtime.as_ref(), &service, report)
                    .await
            }
        })
        .await
    {
        Ok(reports) => reports,
        Err(crate::provider_manifest_sync::ProviderSyncBatchFailure::Remote(primary)) => {
            // checkStateResult is independent observability compensation and never hides primary.
            // checkStateResult 是独立可观测性补偿且绝不隐藏主错误。
            let check_state_result =
                service.record_sync_check_failures(&providers, "index_check_failed");
            let primary =
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(primary.as_str());
            return if check_state_result.is_err() {
                Err(format!(
                    "Provider manifest synchronization failed: {primary} Check-state publication also failed."
                ))
            } else {
                Err(format!(
                    "Provider manifest synchronization failed: {primary}"
                ))
            };
        }
        Err(crate::provider_manifest_sync::ProviderSyncBatchFailure::Projection(primary)) => {
            // The provider handler already attempted exact compensation and provider-scoped
            // failed-check publication; previously committed providers remain active.
            // 逐供应商处理器已经尝试精确补偿与供应商级失败检查发布；此前已提交供应商保持活动。
            return Err(
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(primary.as_str()),
            );
        }
    };
    Ok(reports)
}

/// Returns the typed status snapshot for every system provider manifest (plan §R10).
/// 返回全部系统供应商清单的 typed 状态快照（方案 §R10）。
///
/// # Returns
/// 返回值
/// A typed `Vec<ProviderManifestStatus>` instead of a temporary JSON value.
/// 返回 typed `Vec<ProviderManifestStatus>`，而不是临时 JSON 值。
pub(super) async fn service_get_provider_manifest_status(
    state: SharedServiceBridgeState,
) -> Result<Vec<crate::provider_manifest_sync::ProviderManifestStatus>, String> {
    let (runtime, _configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    // activationGuard prevents the status reader from mixing store layers with a runtime
    // generation while synchronization or rollback is between their publication steps.
    // activationGuard 防止状态读取者在同步或回滚的发布步骤之间混合存储层与运行时代际。
    let _activation_guard = service.begin_activation_transaction().await;
    // configurationGuard closes the independent race where a settings transaction installs a
    // new configuration before publishing its matching model-catalog generation.
    // configurationGuard 关闭设置事务先安装新配置、后发布配对模型目录代际的独立竞争窗口。
    let _configuration_guard = begin_public_ai_configuration_transaction().await;
    service
        .provider_manifest_status_all(runtime.as_ref())
        .map_err(bounded_provider_manifest_rpc_error)
}

/// Previews one signed provider manifest candidate without mutating any local layer or state.
/// 预览一个签名供应商 Manifest 候选，且不修改任何本地层或状态。
///
/// # Parameters
/// 参数
/// `state` owns the immutable runtime and configured sync service; `provider_id` is exact.
/// `state` 持有不可变运行时与已配置同步服务；`provider_id` 为精确标识。
///
/// # Returns
/// 返回值
/// A verified typed semantic diff, or a controlled signed-index/network failure.
/// 返回已验证的类型化语义差异，或受控签名 Index/网络失败。
pub(super) async fn service_preview_provider_manifest(
    state: SharedServiceBridgeState,
    provider_id: String,
) -> Result<crate::provider_manifest_sync::ProviderManifestCandidatePreview, String> {
    // runtime is the single immutable comparison authority captured for this preview.
    // runtime 是为本次预览捕获的唯一不可变比较权威。
    let (runtime, _configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    crate::provider_manifest_sync::require_trusted_system_manifest_provider(
        runtime.as_ref(),
        provider_id.as_str(),
    )
    .map_err(bounded_provider_manifest_rpc_error)?;
    // service reuses the exact production trust store and secure HTTP client.
    // service 复用精确的生产信任存储与安全 HTTP 客户端。
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    // baseline is captured while both writers are excluded, then the guards are released before
    // remote I/O so a slow preview cannot block activation or settings changes.
    // baseline 在排除两类写者时捕获，随后在远端 I/O 前释放守卫，避免慢速预览阻塞激活或设置变更。
    let baseline = {
        let _activation_guard = service.begin_activation_transaction().await;
        let _configuration_guard = begin_public_ai_configuration_transaction().await;
        service
            .capture_preview_baseline(runtime.as_ref(), provider_id.as_str())
            .map_err(|error| {
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
            })?
    };
    service
        .preview_provider_manifest(baseline, provider_id.as_str())
        .await
        .map_err(|error| {
            crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
        })
}

/// Rolls back one provider's active manifest to its previous layer (plan §R10).
/// 把一个供应商的 active 清单回滚到 previous 层（方案 §R10）。
///
/// # Returns
/// 返回值
/// A typed rollback report with the restored revision.
/// 返回带恢复修订号的 typed 回滚报告。
pub(super) async fn service_rollback_provider_manifest(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_id: String,
) -> Result<crate::provider_manifest_sync::ProviderRollbackReport, String> {
    let (runtime, _initial_configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    crate::provider_manifest_sync::require_trusted_system_manifest_provider(
        runtime.as_ref(),
        provider_id.as_str(),
    )
    .map_err(bounded_provider_manifest_rpc_error)?;
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    let _activation_guard = service.begin_activation_transaction().await;
    // configuration_transaction serializes rollback publication with every settings mutation.
    // configuration_transaction 把回滚发布与每次设置修改串行化。
    let configuration_transaction = begin_public_ai_configuration_transaction().await;
    // configuration is the latest durable/runtime snapshot at the rollback activation boundary.
    // configuration 是回滚激活边界上的最新持久化与运行时快照。
    let configuration = runtime
        .configuration_snapshot()
        .map_err(bounded_provider_manifest_rpc_error)?;
    let report = service
        .rollback_provider_manifest(runtime.as_ref(), &provider_id)
        .map_err(|error| {
            crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
        })?;
    let result = service
        .finalize_rollback_projection(
            runtime.as_ref(),
            &configuration,
            report,
            |commit| {
                publish_public_ai_workbench_with_commit(
                    &app,
                    &state,
                    runtime.as_ref(),
                    &configuration,
                    commit,
                )
                .map(|_| ())
            },
            || {
                publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)
                    .map(|_| ())
            },
        )
        .await;
    drop(configuration_transaction);
    result.map_err(|error| {
        crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
    })
}

/// Synchronizes exactly one provider through the reviewed production flow (plan §R10).
/// 通过已审核生产流程同步精确一个供应商（方案 §R10）。
///
/// # Returns
/// 返回值
/// A typed sync report scoped to the requested provider.
/// 返回限定到所请求供应商的 typed 同步报告。
pub(super) async fn service_sync_provider_manifest(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_id: String,
) -> Result<crate::provider_manifest_sync::ProviderSyncReport, String> {
    let (runtime, _initial_configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    crate::provider_manifest_sync::require_trusted_system_manifest_provider(
        runtime.as_ref(),
        provider_id.as_str(),
    )
    .map_err(bounded_provider_manifest_rpc_error)?;
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    let _activation_guard = service.begin_activation_transaction().await;
    // providerIds retains the exact single target for classified top-level failure bookkeeping.
    // providerIds 保留用于分类顶层失败簿记的精确单一目标。
    let provider_ids = vec![provider_id.clone()];
    // report records a checked failure even when Index verification or network I/O returns Err.
    // report 即使 Index 校验或网络 I/O 返回 Err 也会记录已检查失败。
    let report = match service.sync_one(runtime.as_ref(), &provider_id).await {
        Ok(report) => report,
        Err(primary) => {
            // failureState persists only a stable classification, never the URL-bearing diagnostic.
            // failureState 仅持久化稳定分类，绝不持久化可能含 URL 的诊断。
            let failure_state =
                service.record_sync_check_failures(&provider_ids, "index_check_failed");
            let primary =
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(primary.as_str());
            return if failure_state.is_err() {
                Err(format!(
                    "Provider manifest synchronization failed: {primary} Check-state publication also failed."
                ))
            } else {
                Err(format!(
                    "Provider manifest synchronization failed: {primary}"
                ))
            };
        }
    };
    finalize_provider_sync_report(&app, &state, runtime.as_ref(), &service, report)
        .await
        .map_err(bounded_provider_manifest_rpc_error)
}

/// Queries usage for one exact connection without caching it on disk.
/// 查询一个精确连接的用量且不写入磁盘缓存。
pub(super) async fn service_query_public_ai_connection_usage(
    state: SharedServiceBridgeState,
    connection_id: String,
) -> Result<AccountUsageSnapshot, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    runtime
        .query_connection_usage(&configuration, connection_id.as_str())
        .await
}

/// Saves the background account-usage refresh interval.
/// 保存后台账号用量刷新间隔。
pub(super) async fn service_save_public_ai_usage_refresh_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiUsageRefreshSettingsRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if !(30..=86_400).contains(&request.interval_seconds) {
        return Err(String::from(
            "Account usage refresh interval must be between 30 and 86400 seconds.",
        ));
    }
    // transaction keeps the settings writer serialized through Workbench publication.
    // transaction 让设置写入方在工作台发布完成前保持串行化。
    let transaction = begin_public_ai_configuration_transaction().await;
    // currentConfiguration is the exact paired before-image for compensation.
    // currentConfiguration 是用于补偿的精确配对前镜像。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // snapshot is prepared before the atomic browser-visible state swap.
    // snapshot 在浏览器可见状态原子交换前完成预备。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存账号用量刷新间隔失败",
        |configuration| {
            configuration.usage_refresh_interval_seconds = request.interval_seconds;
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider priority after validating its current registered identity.
/// 校验当前已注册身份后保存一个供应商优先级。
pub(super) async fn service_save_public_ai_provider_priority(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderPrioritySaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction keeps validation, persistence, and publication on one serialized generation.
    // transaction 让校验、持久化和发布保持在同一个串行代际。
    let transaction = begin_public_ai_configuration_transaction().await;
    // currentConfiguration is the exact paired before-image for compensation.
    // currentConfiguration 是用于补偿的精确配对前镜像。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if runtime
        .providers()?
        .provider(request.provider_id.as_str())
        .is_none()
    {
        return Err(format!(
            "Provider `{}` is not registered in the current catalog.",
            request.provider_id
        ));
    }
    // snapshot is prepared before the atomic browser-visible state swap.
    // snapshot 在浏览器可见状态原子交换前完成预备。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存供应商优先级失败",
        |configuration| {
            if request.priority == 0 {
                configuration
                    .provider_priority_overrides
                    .remove(request.provider_id.as_str());
            } else {
                configuration
                    .provider_priority_overrides
                    .insert(request.provider_id.clone(), request.priority);
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves the process-wide default Codex context-window policy.
/// 保存进程级默认 Codex 上下文窗口策略。
pub(super) async fn service_save_public_ai_context_window_policy_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiContextWindowPolicyOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    request.overrides.validate()?;
    // transaction retains ownership through the paired runtime and Workbench publication.
    // transaction 在配对运行时与工作台发布全程保留所有权。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let overrides = request.overrides;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存系统级上下文窗口策略失败",
        |configuration| configuration.set_context_window_policy_overrides(overrides),
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves the complete process-wide context-compaction window node configuration.
/// 保存完整的进程级上下文压缩窗口节点配置。
pub(super) async fn service_save_public_ai_context_compaction_window_nodes(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiContextCompactionWindowNodesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    request.nodes.validate()?;
    // transaction retains ownership through persistence and Workbench publication.
    // transaction 在持久化与 Workbench 发布全程保留所有权。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let nodes = request.nodes;
    // nodesChanged prevents an idempotent save from invalidating otherwise reusable budgets.
    // nodesChanged 防止幂等保存使原本可复用的预算失效。
    let nodes_changed = nodes != current_configuration.context_compaction_window_nodes;
    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_using_in_transaction(
            &state,
            runtime.as_ref(),
            &current_configuration,
            "保存上下文压缩窗口节点失败",
            |configuration| configuration.set_context_compaction_window_nodes(nodes),
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |configuration| {
                publish_public_ai_workbench_after_context_compaction_window_nodes_change(
                    &app,
                    &state,
                    runtime.as_ref(),
                    configuration,
                    nodes_changed,
                )
                .map(|_| ())
            },
        )?;
    drop(transaction);
    Ok(snapshot)
}

/// Appends one durable, display-only audit record for an explicit session model selection.
/// 为一次显式会话模型选择追加一条可靠且仅用于展示的审计记录。
///
/// # Parameters
/// 参数
/// `workspace` owns the canonical conversation and display projections; `session_id` selects the
/// exact session; `selection` and the three display names describe the user-selected route.
/// `workspace` 持有规范会话与显示投影；`session_id` 选择精确会话；`selection` 与三个显示名称
/// 描述用户选择的路由。
///
/// # Returns
/// 返回值
/// Success after both canonical and display transcript layers contain the new record.
/// 在规范层与显示层转录都包含新记录后返回成功。
fn append_session_model_switch_record(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    session_id: crate::SessionId,
    selection: &SessionPendingModelSelection,
    provider_display_name: &str,
    connection_display_name: &str,
    model_display_name: &str,
) -> Result<(), String> {
    // projectedEntry contains only the newly canonicalized row, never a clone of conversation history.
    // projectedEntry 仅包含刚完成规范化的新记录，绝不克隆会话历史。
    let projected_entry = {
        // runtimeState is the unique canonical owner of the model-switch audit record.
        // runtimeState 是模型切换审计记录的唯一规范所有者。
        let runtime_state = workspace
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        // entry is context-excluded so the UI record can never become provider-visible prompt input.
        // entry 不参与上下文，因此界面记录绝不会变成 Provider 可见的提示词输入。
        let mut entry = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "",
            format!(
                "provider_id={}\nconnection_id={}\nmodel_id={}",
                selection.provider_id, selection.connection_id, selection.model_id
            ),
        );
        entry.record_kind = SessionMessageRecordKind::DisplayNote;
        entry.visible = true;
        entry.metadata = Some(serde_json::json!({
            "notice_kind": "model_switch",
            "activation": "next_provider_request",
            "provider_id": selection.provider_id,
            "provider_name": provider_display_name,
            "connection_id": selection.connection_id,
            "connection_name": connection_display_name,
            "model_id": selection.model_id,
            "model_name": model_display_name,
        }));
        let message_id =
            crate::append_webui_session_message_entry_to_state(runtime_state, session_id, entry)?;
        let appended_record = runtime_state
            .conversation_state
            .records
            .last()
            .filter(|record| record.id == message_id)
            .ok_or_else(|| {
                String::from(
                    "The canonical model-switch record was not retained at the append boundary.",
                )
            })?;
        crate::session_message_entry_from_conversation_record(appended_record)
    };
    crate::append_new_canonical_row_to_loaded_display_window(
        workspace,
        session_id,
        projected_entry,
    );
    crate::sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    Ok(())
}

/// Plans one exact session model selection without disturbing the route frozen by the current turn.
/// 在不干扰当前回合冻结路由的前提下规划一个精确会话模型选择。
///
/// # Parameters
/// 参数
/// `workspace` is the detached transaction candidate; `session_id` identifies its owner;
/// `selection` and display names identify the exact next-Provider-request route and its audit labels.
/// `workspace` 是独立事务候选；`session_id` 标识其所有者；`selection` 与显示名称标识下一次
/// 真实 Provider 请求使用的精确路由及其审计标签。
///
/// # Returns
/// 返回值
/// A workspace-only reconciliation when the future selection changes, otherwise an empty plan.
/// 当未来选择发生变化时返回仅工作区协调，否则返回空计划。
fn plan_session_model_selection_for_next_provider_request(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    session_id: crate::SessionId,
    selection: &SessionPendingModelSelection,
    provider_display_name: &str,
    connection_display_name: &str,
    model_display_name: &str,
) -> Result<PublicAiWorkspaceReconciliation, String> {
    // futureSelectionAlreadyMatches compares against pending authority first, then the active tuple.
    // futureSelectionAlreadyMatches 优先比较待生效权威，再比较当前活动三元组。
    let future_selection_already_matches = {
        // runtimeState is read before mutation so an idempotent picker action creates no duplicate record.
        // runtimeState 在修改前读取，确保幂等选择器操作不会创建重复记录。
        let runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        if let Some(pending) = runtime_state.pending_model_selection.as_ref() {
            pending == selection
        } else {
            runtime_state.active_provider_id.as_deref() == Some(selection.provider_id.as_str())
                && runtime_state.active_connection_id.as_deref()
                    == Some(selection.connection_id.as_str())
                && runtime_state.active_model_id.as_deref() == Some(selection.model_id.as_str())
        }
    };
    if future_selection_already_matches {
        return Ok(PublicAiWorkspaceReconciliation::default());
    }
    // runtimeState receives only pending authority; the active tuple and route lease remain frozen.
    // runtimeState 仅接收待生效权威；当前活动三元组与路由租约保持冻结。
    let runtime_state = workspace
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    runtime_state.pending_model_selection = Some(selection.clone());
    runtime_state.updated_at = crate::current_unix_millis_text();
    append_session_model_switch_record(
        workspace,
        session_id,
        selection,
        provider_display_name,
        connection_display_name,
        model_display_name,
    )?;
    Ok(PublicAiWorkspaceReconciliation::workspace_only(
        BTreeSet::from([session_id]),
    ))
}

/// Selects one model and remembers its exact account.
/// 选择一个模型并记住其精确账号。
pub(super) async fn service_select_public_ai_model(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelSelectionRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // conversation_id is the validated optional session scope for this selection.
    // conversation_id 是本次选择经过校验的可选会话作用域。
    let conversation_id = request
        .conversation_id
        .as_deref()
        .map(|value| {
            value
                .parse::<crate::SessionId>()
                .map_err(|error| format!("Invalid conversation id `{value}`: {error}"))
        })
        .transpose()?;
    // transaction serializes validation, durable preferences, pending intent, and publication.
    // transaction 串行化校验、持久偏好、待生效意图与发布。
    let transaction = begin_public_ai_configuration_transaction().await;
    // session_context_compaction_window_node_id preserves the current session lock across models.
    // session_context_compaction_window_node_id 在模型之间保留当前会话锁定节点。
    let session_context_compaction_window_node_id = if let Some(session_id) = conversation_id {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        // session_exists prevents account-preference persistence when the target session is already gone.
        // session_exists 防止目标会话已经消失时仍持久化账号偏好。
        let session_exists = guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|workspace| workspace.session_runtime_state.contains_key(&session_id));
        if !session_exists {
            return Err(format!("Conversation `{session_id}` has no runtime state."));
        }
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .and_then(|runtime_state| runtime_state.context_compaction_window_node_id)
    } else {
        None
    };
    // current_configuration is the immutable validation baseline before persistent mutation.
    // current_configuration 是持久化修改前使用的不可变校验基线。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // selected_connection is the exact credential row requested by the composer account selector.
    // selected_connection 是输入区账号选择器请求的精确凭据行。
    let selected_connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .ok_or_else(|| format!("Connection `{}` is not configured.", request.connection_id))?;
    if selected_connection.provider_id != request.provider_id {
        return Err(format!(
            "Connection `{}` does not belong to provider `{}`.",
            request.connection_id, request.provider_id
        ));
    }
    if current_configuration.connection_is_pending_validation(request.connection_id.as_str()) {
        return Err(format!(
            "Connection `{}` is pending explicit model validation and cannot be selected for a conversation.",
            request.connection_id
        ));
    }
    if !runtime.model_is_enabled_for_connection(
        &current_configuration,
        request.connection_id.as_str(),
        request.model_id.as_str(),
    )? {
        return Err(format!(
            "Provider `{}` model `{}` is disabled.",
            request.provider_id, request.model_id
        ));
    }
    // selected_connection_exposes_model rejects mismatched account/model requests before mutation.
    // selected_connection_exposes_model 在修改前拒绝账号与模型不匹配的请求。
    let selected_connection_exposes_model = runtime
        .catalog_snapshot()?
        .get(request.connection_id.as_str())
        .is_some_and(|catalog| {
            catalog
                .models
                .iter()
                .any(|model| model.id == request.model_id)
        });
    if !selected_connection_exposes_model {
        return Err(format!(
            "Connection `{}` does not expose model `{}`.",
            request.connection_id, request.model_id
        ));
    }
    // selected_model is the exact target used to validate an existing session compression lock.
    // selected_model 是用于校验既有会话压缩锁定的精确目标。
    let selected_model = runtime.resolve_connection_model_selection(
        &current_configuration,
        request.connection_id.as_str(),
        request.provider_id.as_str(),
        request.model_id.as_str(),
    )?;
    if let Some(node_id) = session_context_compaction_window_node_id {
        // node_tokens resolves the stable persisted id against the current global configuration.
        // node_tokens 根据当前全局配置解析持久化的稳定标识。
        let node_tokens = current_configuration
            .context_compaction_window_nodes
            .tokens_for(node_id)?;
        // model_context_limit is the unchanged hard window declared by the selected model.
        // model_context_limit 是所选模型声明且保持不变的硬窗口。
        let model_context_limit =
            selected_model
                .model
                .capabilities
                .context_limit
                .ok_or_else(|| {
                    String::from("目标模型未声明上下文窗口，无法保留当前会话的上下文压缩锁定。")
                })?;
        if node_tokens >= model_context_limit {
            return Err(format!(
                "当前会话锁定的上下文压缩节点 `{}` 为 {} Token，必须严格小于目标模型上下文窗口 {} Token。请先切回默认窗口。",
                node_id.as_str(),
                node_tokens,
                model_context_limit
            ));
        }
    }
    // selectedProviderDisplayName is frozen with the validated provider generation for audit display.
    // selectedProviderDisplayName 与已校验供应商代际一同冻结，供审计展示使用。
    let selected_provider_display_name = runtime
        .providers()?
        .provider(request.provider_id.as_str())
        .map(|provider| provider.display_name.clone())
        .ok_or_else(|| {
            format!(
                "Provider `{}` is not registered in the current catalog.",
                request.provider_id
            )
        })?;
    // selectedConnectionDisplayName is the exact account label accepted by validation.
    // selectedConnectionDisplayName 是校验已接受的精确账号标签。
    let selected_connection_display_name = selected_connection.display_name.clone();
    // selectedModelDisplayName is the exact catalog label accepted by route resolution.
    // selectedModelDisplayName 是路由解析已接受的精确目录模型标签。
    let selected_model_display_name = selected_model.model.display_name.clone();
    // selectedTarget is the exact future session tuple or process-wide active tuple.
    // selectedTarget 是精确的未来会话三元组或进程级活动三元组。
    let selected_target = ActiveAiModelSelection {
        provider_id: request.provider_id.clone(),
        model_id: request.model_id.clone(),
        connection_id: request.connection_id.clone(),
    };
    // pendingTarget keeps the session-scoped selection distinct from the current turn's active route.
    // pendingTarget 让会话作用域选择与当前回合的活动路由保持分离。
    let pending_target = SessionPendingModelSelection {
        provider_id: request.provider_id.clone(),
        connection_id: request.connection_id.clone(),
        model_id: request.model_id.clone(),
    };
    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_using_in_transaction(
            &state,
            runtime.as_ref(),
            &current_configuration,
            "选择公共 AI 模型失败",
            |configuration| {
                configuration.connection_graph.remember_model_connection(
                    request.provider_id.as_str(),
                    request.model_id.as_str(),
                    request.connection_id.as_str(),
                )?;
                if conversation_id.is_none() {
                    configuration.active = Some(selected_target.clone());
                }
                // Validate the exact target against the published generation; the draft active
                // selection cannot be resolved until this transaction is committed.
                // 使用已发布代际校验精确目标；事务提交前不能解析草稿中的活动选择。
                runtime.resolve_connection_model_selection(
                    configuration,
                    request.connection_id.as_str(),
                    request.provider_id.as_str(),
                    request.model_id.as_str(),
                )?;
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |configuration| {
                publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
                    &app,
                    &state,
                    runtime.as_ref(),
                    configuration,
                    |workspace| {
                        if let Some(session_id) = conversation_id {
                            plan_session_model_selection_for_next_provider_request(
                                workspace,
                                session_id,
                                &pending_target,
                                selected_provider_display_name.as_str(),
                                selected_connection_display_name.as_str(),
                                selected_model_display_name.as_str(),
                            )
                        } else {
                            Ok(PublicAiWorkspaceReconciliation::default())
                        }
                    },
                    false,
                    "model_selection",
                    || Ok(()),
                )
                .map(|_| ())
            },
        )?;
    if let Some(session_id) = conversation_id {
        // controllerRuntime is captured after atomic publication; the persistence gate reads the exact retained session later.
        // controllerRuntime 在原子发布后捕获；持久化闸门随后读取精确保留会话。
        let controller_runtime = {
            let guard = state
                .lock()
                .map_err(|error| format!("Failed to lock service state: {error}"))?;
            guard
                .controller_runtime
                .clone()
                .ok_or_else(|| {
                    String::from(
                        "Session model selection cannot persist because controller workspace storage is unavailable.",
                    )
                })?
        };
        // The authoritative write gate reloads the newest retained session row, so this synchronous
        // durability boundary cannot overwrite tool, Continue, or terminal mutations committed after publication.
        // 权威写入闸门会重新加载最新保留会话行，因此该同步可靠边界不会覆盖发布后提交的工具、Continue 或终态变更。
        crate::persist_latest_retained_workspace_session_update(
            &state,
            controller_runtime,
            session_id,
        )
        .await?;
    }
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider-model-protocol override set after validating every matching account route.
/// 在校验全部匹配账号路由后保存一组供应商、模型与协议覆盖。
pub(super) async fn service_save_public_ai_model_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // overrides collapses an empty browser record into restoration semantics.
    // overrides 将空浏览器记录折叠为还原语义。
    let overrides = request.overrides.filter(|overrides| !overrides.is_empty());
    if overrides.as_ref().is_some_and(|overrides| {
        overrides.context_limit.is_some() || overrides.output_limit.is_some()
    }) {
        return Err(String::from(
            "Context and output limits must be saved through direct model settings, not protocol execution settings.",
        ));
    }
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let key = PersistedAiConfiguration::provider_model_protocol_key(
        request.provider_id.as_str(),
        request.model_id.as_str(),
        request.protocol_id.as_str(),
    );
    if let Some(overrides) = overrides.as_ref() {
        runtime.validate_model_execution_override_candidate(
            &current_configuration,
            request.provider_id.as_str(),
            request.model_id.as_str(),
            request.protocol_id.as_str(),
            overrides,
        )?;
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存公共 AI 模型覆盖失败",
        |configuration| {
            if let Some(overrides) = overrides.clone() {
                configuration.model_overrides.insert(key.clone(), overrides);
            } else {
                configuration.model_overrides.remove(key.as_str());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider-model conversation choice and reconciles invalid active selections.
/// 保存一个供应商模型会话选择，并协调失效的当前选择。
pub(super) async fn service_save_public_ai_model_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    // transaction keeps manifest publication, support validation, persistence, and response projection ordered.
    // transaction 让 Manifest 发布、支持校验、持久化与响应投影保持有序。
    let _transaction = begin_public_ai_configuration_transaction().await;
    // current_configuration is re-read only after transaction serialization.
    // current_configuration 仅在事务串行化后重新读取。
    let current_configuration = runtime.configuration_snapshot()?;
    // settings_catalog captures conversation model membership from one generation.
    // settings_catalog 从同一代际捕获会话模型成员关系。
    let settings_catalog = runtime.settings_catalog_snapshot(&current_configuration)?;
    // conversation_model_supported accepts only manual or catalog models that explicitly expose
    // text conversation. Provider extension targets have their own selection boundary.
    // conversation_model_supported 仅接受手工模型或明确公开文本会话的目录模型；Provider 扩展目标
    // 拥有独立选择边界。
    let conversation_model_supported = current_configuration
        .custom_model(request.provider_id.as_str(), request.model_id.as_str())
        .is_some_and(|model| {
            current_configuration
                .custom_providers
                .contains_key(request.provider_id.as_str())
                || vulcan_provider_adapters::model_supports_text_conversation(&model.capabilities)
        })
        || current_configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == request.provider_id)
            .any(|connection| {
                settings_catalog
                    .model_support_catalogs
                    .get(connection.id.as_str())
                    .is_some_and(|catalog| {
                        catalog.models.iter().any(|model| {
                            model.id == request.model_id
                                && catalog_model_supports_text_conversation(model)
                        })
                    })
            });
    if !conversation_model_supported {
        return Err(format!(
            "Provider `{}` does not expose model `{}` for text conversation.",
            request.provider_id, request.model_id
        ));
    }
    // catalogs retains the same generation used for support validation and fallback selection.
    // catalogs 保留用于支持校验与回退选择的同一代际。
    let catalogs = settings_catalog.catalogs;
    // candidateConfiguration freezes restore/default semantics before any durable activation.
    // candidateConfiguration 在任何持久激活前冻结恢复与默认值语义。
    let mut candidate_configuration = current_configuration.clone();
    if request.restore {
        candidate_configuration
            .restore_model_enabled(request.provider_id.as_str(), request.model_id.as_str());
    } else {
        candidate_configuration.set_model_enabled(
            request.provider_id.as_str(),
            request.model_id.as_str(),
            request.enabled,
        );
    }
    // modelEffectivelyDisabled follows the resolved manifest default after a restore request.
    // modelEffectivelyDisabled 在恢复请求后遵循已解析 Manifest 默认值。
    let model_effectively_disabled = !runtime.model_is_enabled(
        &candidate_configuration,
        request.provider_id.as_str(),
        request.model_id.as_str(),
    );
    // fallback is one deterministic enabled route and never probes another protocol.
    // fallback 是一个确定性的已启用路由，绝不探测其他协议。
    let fallback = if model_effectively_disabled {
        first_enabled_model_selection(runtime.as_ref(), &candidate_configuration, &catalogs)?
    } else {
        None
    };
    let active_model_is_disabled =
        candidate_configuration
            .active
            .as_ref()
            .is_some_and(|selection| {
                selection.provider_id == request.provider_id
                    && selection.model_id == request.model_id
                    && model_effectively_disabled
            });
    if active_model_is_disabled {
        candidate_configuration.active = fallback.clone();
    }
    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_sessions_in_transaction(
            &app,
            &state,
            runtime.as_ref(),
            &current_configuration,
            "保存公共 AI 模型启用状态失败",
            |configuration| {
                *configuration = candidate_configuration.clone();
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |workspace| {
                Ok(if model_effectively_disabled {
                    reconcile_explicitly_revoked_session_model_selections(
                        workspace,
                        fallback.as_ref(),
                        |_session_id, runtime_state| {
                            runtime_state.active_provider_id.as_deref()
                                == Some(request.provider_id.as_str())
                                && runtime_state.active_model_id.as_deref()
                                    == Some(request.model_id.as_str())
                        },
                    )
                } else {
                    BTreeSet::new()
                })
            },
            "provider_model_access_revoked",
        )?;
    Ok(snapshot)
}

/// Saves one model-level internal-use switch without changing conversation availability.
/// 保存一项模型级内部使用开关且不改变会话可用性。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `request` identify the settings transaction and exact model.
/// `app`、`state` 与 `request` 标识设置事务及精确模型。
///
/// # Returns
/// 返回值
/// The complete refreshed settings snapshot.
/// 返回完整刷新后的设置快照。
pub(super) async fn service_save_public_ai_internal_model_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiInternalModelEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction serializes validation, activation, and Workbench publication.
    // transaction 串行化校验、激活与工作台发布。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let providers = runtime.effective_providers(&current_configuration)?;
    let catalogs = runtime.catalog_snapshot()?;
    let exists =
        build_internal_inference_candidates(&current_configuration, &providers, &catalogs)?
            .iter()
            .any(|candidate| {
                candidate.route.provider_id == request.provider_id
                    && candidate.route.model_id == request.model_id
            });
    if !exists {
        return Err(format!(
            "Provider `{}` model `{}` does not expose a registered internal inference operation.",
            request.provider_id, request.model_id
        ));
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存内部推理模型启用状态失败",
        |configuration| {
            configuration.set_internal_model_enabled(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                request.enabled,
            );
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves operation-scoped internal defaults after validating every exact runtime route.
/// 在校验全部精确运行时路由后保存操作作用域内部默认值。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `request` identify one provider-model-operation replacement.
/// `app`、`state` 与 `request` 标识一个供应商、模型、操作的替换请求。
///
/// # Returns
/// 返回值
/// The complete refreshed settings snapshot.
/// 返回完整刷新后的设置快照。
pub(super) async fn service_save_public_ai_internal_model_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiInternalModelOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if !matches!(
        request.operation,
        AiOperation::ConversationRespond
            | AiOperation::EmbeddingCreate
            | AiOperation::RerankDocuments
    ) {
        return Err(format!(
            "Operation `{}` is not configurable as an internal inference model.",
            request.operation.as_str()
        ));
    }
    // transaction serializes validation, activation, and Workbench publication.
    // transaction 串行化校验、激活与工作台发布。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let providers = runtime.effective_providers(&current_configuration)?;
    let catalogs = runtime.catalog_snapshot()?;
    let matching_candidates =
        build_internal_inference_candidates(&current_configuration, &providers, &catalogs)?
            .into_iter()
            .filter(|candidate| {
                candidate.route.provider_id == request.provider_id
                    && candidate.route.model_id == request.model_id
                    && candidate.route.operation == request.operation
            })
            .collect::<Vec<_>>();
    if matching_candidates.is_empty() {
        return Err(format!(
            "Provider `{}` model `{}` does not expose registered internal operation `{}`.",
            request.provider_id,
            request.model_id,
            request.operation.as_str()
        ));
    }
    for candidate in &matching_candidates {
        validate_internal_model_overrides(
            &candidate.capabilities,
            &candidate.route,
            &request.overrides,
        )?;
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存内部推理模型覆盖失败",
        |configuration| {
            let key = PersistedAiConfiguration::internal_model_operation_key(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                request.operation,
            );
            if request.overrides == InternalModelOverrides::default() {
                configuration.internal_model_overrides.remove(key.as_str());
            } else {
                configuration
                    .internal_model_overrides
                    .insert(key, request.overrides.clone());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves or clears one exact route in a consumer-isolated inference profile.
/// 在消费者隔离推理配置档内保存或清除一条精确路由。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `request` identify the settings transaction and route replacement.
/// `app`、`state` 与 `request` 标识设置事务及路由替换。
///
/// # Returns
/// 返回值
/// The complete refreshed settings snapshot after strict route validation.
/// 严格校验路由后返回完整刷新设置快照。
pub(super) async fn service_save_public_ai_inference_route(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiInferenceRouteSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("consumerProfileId", request.consumer_profile_id.as_str())?;
    match request.operation {
        AiOperation::ConversationRespond => {
            let purpose_id = request.purpose_id.as_deref().ok_or_else(|| {
                String::from("Restricted LLM inference route requires purposeId.")
            })?;
            validate_non_empty("purposeId", purpose_id)?;
        }
        AiOperation::EmbeddingCreate | AiOperation::RerankDocuments => {
            if request.purpose_id.is_some() {
                return Err(String::from(
                    "Embedding and rerank inference routes do not accept purposeId.",
                ));
            }
        }
        _ => {
            return Err(format!(
                "Operation `{}` is not configurable through the inference profile.",
                request.operation.as_str()
            ));
        }
    }
    // transaction serializes validation, activation, and Workbench publication.
    // transaction 串行化校验、激活与工作台发布。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if let Some(route) = request.route.as_ref() {
        if route.operation != request.operation {
            return Err(String::from(
                "Inference route operation differs from the requested profile operation.",
            ));
        }
        // Route selection consumes the model-level state owned by Model List and must never
        // mutate or bypass it from a consumer-specific settings surface.
        // 路由选择只消费“模型列表”持有的模型级状态，绝不能从消费者专属设置界面修改或绕过它。
        let selection =
            runtime.resolve_internal_inference_selection(&current_configuration, route)?;
        validate_vmm_llm_reasoning_policy(
            request.consumer_profile_id.as_str(),
            request.operation,
            &selection.model.capabilities,
        )?;
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存内部推理路由失败",
        |configuration| {
            let profile = configuration
                .inference_profiles
                .entry(request.consumer_profile_id.clone())
                .or_default();
            match request.operation {
                AiOperation::ConversationRespond => {
                    let purpose_id = request.purpose_id.as_deref().ok_or_else(|| {
                        String::from("Restricted LLM inference route requires purposeId.")
                    })?;
                    if let Some(route) = request.route.clone() {
                        profile.llm_routes.insert(purpose_id.to_string(), route);
                    } else {
                        profile.llm_routes.remove(purpose_id);
                    }
                }
                AiOperation::EmbeddingCreate => {
                    profile.embedding = request.route.clone();
                }
                AiOperation::RerankDocuments => {
                    profile.rerank = request.route.clone();
                }
                _ => {
                    return Err(format!(
                        "Operation `{}` is not configurable through the inference profile.",
                        request.operation.as_str()
                    ));
                }
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Executes one real provider-backed embedding or rerank settings test.
/// 执行一次由真实供应商支撑的向量或重排设置测试。
///
/// # Parameters
/// 参数
/// `state` owns the configured inference service and `request` selects only an authorized
/// consumer profile operation; it cannot inject a provider route.
/// `state` 持有已配置推理服务，`request` 只能选择获授权消费者配置档操作，不能注入供应商路由。
///
/// # Returns
/// 返回值
/// Compact frozen route facts after request and response invariants are independently verified.
/// 请求与响应不变量经独立校验后返回紧凑冻结路由事实。
pub(super) async fn service_test_public_ai_inference_route(
    state: SharedServiceBridgeState,
    request: PublicAiInferenceTestRequest,
) -> Result<PublicAiInferenceTestResult, String> {
    validate_non_empty("consumerProfileId", request.consumer_profile_id.as_str())?;
    let permission = match request.operation {
        AiOperation::EmbeddingCreate => InferencePermission::Embedding,
        AiOperation::RerankDocuments => InferencePermission::Rerank,
        _ => {
            return Err(format!(
                "Operation `{}` does not have a settings connection test.",
                request.operation.as_str()
            ));
        }
    };
    let (_, configuration) = public_ai_runtime_and_configuration(&state)?;
    let configured_route = configured_settings_inference_test_route(
        &configuration,
        request.consumer_profile_id.as_str(),
        request.operation,
    )?;
    let service = configured_inference_service_for_settings(&state)?;
    let caller = settings_inference_test_caller(
        request.consumer_profile_id.as_str(),
        permission,
        configured_route.usage_scope,
    )?;
    let capabilities = service
        .capability_snapshot(&caller, request.consumer_profile_id.as_str())
        .map_err(|error| error.to_string())?;
    let operation = capabilities
        .operations
        .iter()
        .find(|candidate| {
            candidate.operation == request.operation && candidate.purpose_id.is_none()
        })
        .ok_or_else(|| {
            format!(
                "Consumer profile `{}` does not expose operation `{}`.",
                request.consumer_profile_id,
                request.operation.as_str()
            )
        })?;
    if !operation.available {
        return Err(operation.unavailable_reason.clone().unwrap_or_else(|| {
            format!(
                "Consumer profile `{}` operation `{}` is unavailable.",
                request.consumer_profile_id,
                request.operation.as_str()
            )
        }));
    }
    match request.operation {
        AiOperation::EmbeddingCreate => {
            let embedding = operation.embedding.as_ref().ok_or_else(|| {
                String::from("The configured route does not declare embedding capabilities.")
            })?;
            let request_id = format!("settings_embedding_test_{}", uuid::Uuid::new_v4().simple());
            let response = service
                .create_embeddings(
                    &caller,
                    EmbeddingRequest {
                        contract_version: INFERENCE_CONTRACT_VERSION,
                        request_id,
                        idempotency_key: None,
                        consumer_profile_id: request.consumer_profile_id,
                        inputs: vec![EmbeddingInput {
                            input_id: String::from("settings-test-input"),
                            content: EmbeddingInputContent::Text {
                                text: String::from(
                                    "Vulcan Code internal embedding connection test.",
                                ),
                            },
                        }],
                        task: preferred_embedding_task(embedding)?,
                        dimensions: None,
                        output_kind: preferred_embedding_output_kind(embedding)?,
                        encoding: preferred_embedding_encoding(embedding)?,
                        invalid_input_policy: Default::default(),
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
            validate_embedding_test_response(&response)?;
            Ok(PublicAiInferenceTestResult {
                operation: AiOperation::EmbeddingCreate,
                provider_id: response.route.provider_id,
                service_entry_id: response.route.service_entry_id,
                connection_id: response.route.connection_id,
                model_id: response.route.model_id,
                result_count: response.results.len(),
                dimensions: Some(response.fingerprint.dimensions),
                duration_ms: response.duration_ms,
            })
        }
        AiOperation::RerankDocuments => {
            let rerank = operation.rerank.as_ref().ok_or_else(|| {
                String::from("The configured route does not declare rerank capabilities.")
            })?;
            let overrides = configuration.internal_model_overrides.get(
                PersistedAiConfiguration::internal_model_operation_key(
                    configured_route.provider_id.as_str(),
                    configured_route.model_id.as_str(),
                    AiOperation::RerankDocuments,
                )
                .as_str(),
            );
            let fixture_minimum = rerank.maximum_candidates.unwrap_or(2).clamp(1, 2);
            let requested_top_n = overrides
                .and_then(|value| value.rerank_top_n)
                .unwrap_or(fixture_minimum);
            let candidate_count = requested_top_n.max(fixture_minimum);
            if rerank
                .maximum_candidates
                .is_some_and(|maximum| candidate_count > maximum)
            {
                return Err(format!(
                    "Configured rerank test requires {candidate_count} candidates but the model maximum is {}.",
                    rerank.maximum_candidates.unwrap_or_default()
                ));
            }
            let candidates = fixed_rerank_test_candidates(candidate_count);
            let request_id = format!("settings_rerank_test_{}", uuid::Uuid::new_v4().simple());
            let response = service
                .rerank_documents(
                    &caller,
                    RerankRequest {
                        contract_version: INFERENCE_CONTRACT_VERSION,
                        request_id,
                        idempotency_key: None,
                        consumer_profile_id: request.consumer_profile_id,
                        query: RerankText {
                            id: String::from("settings-test-query"),
                            text: String::from("Which document describes a programming language?"),
                        },
                        candidates,
                        top_n: requested_top_n,
                        truncation: overrides
                            .and_then(|value| value.rerank_truncation)
                            .unwrap_or(preferred_rerank_truncation(rerank)?),
                        return_content: overrides
                            .and_then(|value| value.rerank_return_content)
                            .unwrap_or(false),
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
            validate_rerank_test_response(&response, candidate_count)?;
            Ok(PublicAiInferenceTestResult {
                operation: AiOperation::RerankDocuments,
                provider_id: response.route.provider_id,
                service_entry_id: response.route.service_entry_id,
                connection_id: response.route.connection_id,
                model_id: response.route.model_id,
                result_count: response.results.len(),
                dimensions: None,
                duration_ms: response.duration_ms,
            })
        }
        _ => Err(format!(
            "Operation `{}` does not have a settings connection test.",
            request.operation.as_str()
        )),
    }
}

/// Returns the configured inference service without retaining the shared state lock.
/// 返回已配置推理服务且不保留共享状态锁。
///
/// # Parameters
/// 参数
/// `state` is the shared service bridge whose runtime owns the live configured-inference facade.
/// `state` 是共享服务桥，其运行时持有实时已配置推理门面。
///
/// # Returns
/// 返回值
/// A cloned service handle, or an explicit startup/lock diagnostic without keeping the state lock.
/// 返回克隆的服务句柄，或明确的启动/锁诊断，且不会继续持有状态锁。
fn configured_inference_service_for_settings(
    state: &SharedServiceBridgeState,
) -> Result<Arc<crate::configured_inference::ConfiguredInferenceService>, String> {
    state
        .lock()
        .map_err(|error| format!("Failed to lock service state for inference test: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| Arc::clone(&runtime.configured_inference_service))
        .ok_or_else(|| String::from("Configured inference runtime is still starting."))
}

/// Builds the host-owned identity used only by explicit settings connection tests.
/// 构建仅供显式设置连接测试使用的宿主持有身份。
///
/// # Parameters
/// 参数
/// `consumer_profile_id` and `permission` select the exact test operation while
/// `usage_scope` is copied from the persisted route rather than accepted from browser input.
/// `consumer_profile_id` 与 `permission` 选择精确测试操作，`usage_scope`
/// 从持久化路由复制而非接受浏览器输入。
///
/// # Returns
/// 返回值
/// A short-lived caller context constrained to one profile, one operation, and one route scope.
/// 返回仅限一个配置档、一项操作与一个路由用途范围的短期调用方上下文。
fn settings_inference_test_caller(
    consumer_profile_id: &str,
    permission: InferencePermission,
    usage_scope: InferenceUsageScope,
) -> Result<InferenceCallerContext, String> {
    let now_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("System clock cannot issue an inference test deadline: {error}"))
        .and_then(|duration| {
            u64::try_from(duration.as_millis())
                .map_err(|_| String::from("System time exceeds the supported millisecond range."))
        })?;
    Ok(InferenceCallerContext {
        caller_id: String::from("vulcan-code-settings-inference-test"),
        allowed_consumer_profile_ids: BTreeSet::from([consumer_profile_id.to_string()]),
        permissions: BTreeSet::from([InferencePermission::Status, permission]),
        usage_scope,
        interactive_session_id: None,
        deadline_unix_ms: Some(now_unix_ms.saturating_add(120_000)),
        budget: InferenceCallerBudget {
            max_concurrent_executions: 1,
            max_input_units_per_request: Some(16_384),
            max_output_tokens_per_request: None,
            max_requests_per_minute: None,
            max_input_units_per_minute: None,
            max_execution_duration_ms: Some(120_000),
        },
    })
}

/// Resolves the exact host-owned route used by one settings inference test.
/// 解析一次设置推理测试使用的精确宿主持有路由。
///
/// # Parameters
/// 参数
/// `configuration` owns consumer profiles, `consumer_profile_id` selects one profile,
/// and `operation` must be embedding or rerank.
/// `configuration` 持有消费者配置档，`consumer_profile_id` 选择一个配置档，
/// `operation` 必须为向量或重排。
///
/// # Returns
/// 返回值
/// A cloned exact route whose operation matches its profile slot, or a controlled diagnostic.
/// 返回操作与配置槽位一致的精确路由副本，或受控诊断。
fn configured_settings_inference_test_route(
    configuration: &PersistedAiConfiguration,
    consumer_profile_id: &str,
    operation: AiOperation,
) -> Result<InferenceRouteConfiguration, String> {
    let profile = configuration
        .inference_profiles
        .get(consumer_profile_id)
        .ok_or_else(|| format!("Consumer profile `{consumer_profile_id}` is not configured."))?;
    let route = match operation {
        AiOperation::EmbeddingCreate => profile.embedding.as_ref(),
        AiOperation::RerankDocuments => profile.rerank.as_ref(),
        _ => None,
    }
    .ok_or_else(|| {
        format!(
            "Consumer profile `{consumer_profile_id}` does not configure operation `{}`.",
            operation.as_str()
        )
    })?;
    if route.operation != operation {
        return Err(format!(
            "Consumer profile `{consumer_profile_id}` stores operation `{}` in the `{}` test slot.",
            route.operation.as_str(),
            operation.as_str()
        ));
    }
    Ok(route.clone())
}

/// Selects a deterministic declared embedding task for the fixed test request.
/// 为固定测试请求选择一项确定且已声明的向量任务。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact configured route contract returned by the inference facade.
/// `capabilities` 是推理门面返回的精确已配置路由契约。
///
/// # Returns
/// 返回值
/// `ProviderDefault` when declared, otherwise the first ordered declared task, or an error.
/// 已声明时返回 `ProviderDefault`，否则返回有序声明中的第一项任务，若为空则报错。
fn preferred_embedding_task(
    capabilities: &vulcan_ai_contract::EmbeddingCapabilities,
) -> Result<EmbeddingTask, String> {
    if capabilities.tasks.contains(&EmbeddingTask::ProviderDefault) {
        return Ok(EmbeddingTask::ProviderDefault);
    }
    capabilities
        .tasks
        .iter()
        .next()
        .copied()
        .ok_or_else(|| String::from("The embedding route does not declare any supported task."))
}

/// Selects a deterministic declared embedding output shape for the fixed test request.
/// 为固定测试请求选择一项确定且已声明的向量输出形态。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact configured embedding contract.
/// `capabilities` 是精确已配置向量契约。
///
/// # Returns
/// 返回值
/// Dense output when declared, otherwise the first ordered output kind, or an explicit error.
/// 已声明时返回稠密输出，否则返回有序输出类型中的第一项，若为空则显式报错。
fn preferred_embedding_output_kind(
    capabilities: &vulcan_ai_contract::EmbeddingCapabilities,
) -> Result<EmbeddingOutputKind, String> {
    if capabilities
        .output_kinds
        .contains(&EmbeddingOutputKind::Dense)
    {
        return Ok(EmbeddingOutputKind::Dense);
    }
    capabilities
        .output_kinds
        .iter()
        .next()
        .copied()
        .ok_or_else(|| {
            String::from("The embedding route does not declare any supported output shape.")
        })
}

/// Selects a deterministic declared embedding encoding for the fixed test request.
/// 为固定测试请求选择一项确定且已声明的向量编码。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact configured embedding contract.
/// `capabilities` 是精确已配置向量契约。
///
/// # Returns
/// 返回值
/// Float encoding when declared, otherwise the first ordered encoding, or an explicit error.
/// 已声明时返回浮点编码，否则返回有序编码中的第一项，若为空则显式报错。
fn preferred_embedding_encoding(
    capabilities: &vulcan_ai_contract::EmbeddingCapabilities,
) -> Result<EmbeddingEncoding, String> {
    if capabilities.encodings.contains(&EmbeddingEncoding::Float) {
        return Ok(EmbeddingEncoding::Float);
    }
    capabilities
        .encodings
        .iter()
        .next()
        .copied()
        .ok_or_else(|| {
            String::from("The embedding route does not declare any supported value encoding.")
        })
}

/// Selects one deterministic declared rerank truncation policy.
/// 选择一项确定且已声明的重排截断策略。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact configured rerank contract.
/// `capabilities` 是精确已配置重排契约。
///
/// # Returns
/// 返回值
/// Disabled truncation when declared, otherwise the first ordered policy, or an explicit error.
/// 已声明时返回禁用截断，否则返回有序策略中的第一项，若为空则显式报错。
fn preferred_rerank_truncation(
    capabilities: &vulcan_ai_contract::RerankCapabilities,
) -> Result<RerankTruncation, String> {
    if capabilities
        .truncation
        .contains(&RerankTruncation::Disabled)
    {
        return Ok(RerankTruncation::Disabled);
    }
    capabilities
        .truncation
        .iter()
        .next()
        .copied()
        .ok_or_else(|| {
            String::from("The rerank route does not declare any supported truncation policy.")
        })
}

/// Builds a deterministic candidate set large enough for the configured test result count.
/// 构建一组足以覆盖已配置测试结果数量的确定候选。
///
/// # Parameters
/// 参数
/// `candidate_count` is already bounded by the exact model capability and configured Top N.
/// `candidate_count` 已受精确模型能力与已配置 Top N 约束。
///
/// # Returns
/// 返回值
/// Stable text candidates with unique IDs and no user content.
/// 返回带唯一标识且不包含用户内容的稳定文本候选。
fn fixed_rerank_test_candidates(candidate_count: u32) -> Vec<RerankText> {
    (0..candidate_count)
        .map(|index| {
            let text = match index {
                0 => String::from("Rust is a systems programming language."),
                1 => String::from("A maple tree loses leaves during autumn."),
                _ => format!("Fixed unrelated settings test document {index}."),
            };
            RerankText {
                id: format!("settings-test-candidate-{index}"),
                text,
            }
        })
        .collect()
}

/// Independently verifies embedding count, identity, dimensions, and finite numeric values.
/// 独立校验向量数量、身份、维度与有限数值。
///
/// # Parameters
/// 参数
/// `response` is the normalized provider result returned by the configured inference facade.
/// `response` 是已配置推理门面返回的归一化供应商结果。
///
/// # Returns
/// 返回值
/// Success only when one stable result matches its declared fingerprint and numeric invariants.
/// 仅当一个稳定结果符合其声明指纹与数值不变量时返回成功。
fn validate_embedding_test_response(
    response: &vulcan_ai_contract::EmbeddingResponse,
) -> Result<(), String> {
    if response.results.len() != 1 {
        return Err(format!(
            "Embedding test expected one result but received {}.",
            response.results.len()
        ));
    }
    let result = &response.results[0];
    if result.input_id != "settings-test-input" || result.input_index != 0 {
        return Err(String::from(
            "Embedding test response did not preserve input identity and index.",
        ));
    }
    let dimensions = usize::try_from(response.fingerprint.dimensions)
        .map_err(|_| String::from("Embedding dimensions cannot be represented on this host."))?;
    if dimensions == 0 {
        return Err(String::from(
            "Embedding test response reported zero dimensions.",
        ));
    }
    match &result.embedding {
        EmbeddingPayload::Dense { values } => {
            if values.len() != dimensions || values.iter().any(|value| !value.is_finite()) {
                return Err(String::from(
                    "Embedding test dense result has invalid dimensions or non-finite values.",
                ));
            }
        }
        EmbeddingPayload::Sparse { indices, values } => {
            if indices.len() != values.len()
                || values.iter().any(|value| !value.is_finite())
                || indices
                    .iter()
                    .any(|index| *index >= response.fingerprint.dimensions)
            {
                return Err(String::from(
                    "Embedding test sparse result has invalid indices or non-finite values.",
                ));
            }
        }
        EmbeddingPayload::MultiVector { vectors } => {
            if vectors.is_empty()
                || vectors.iter().any(|vector| {
                    vector.len() != dimensions || vector.iter().any(|value| !value.is_finite())
                })
            {
                return Err(String::from(
                    "Embedding test multi-vector result has invalid dimensions or values.",
                ));
            }
        }
        EmbeddingPayload::Base64 { value } => {
            if value.is_empty() {
                return Err(String::from(
                    "Embedding test Base64 result must not be empty.",
                ));
            }
        }
    }
    Ok(())
}

/// Independently verifies rerank indices, ranks, ordering, and finite raw scores.
/// 独立校验重排索引、名次、顺序与有限原始分数。
///
/// # Parameters
/// 参数
/// `response` is the normalized rerank result and `candidate_count` is the exact submitted size.
/// `response` 是归一化重排结果，`candidate_count` 是精确提交数量。
///
/// # Returns
/// 返回值
/// Success only when every result is unique, in range, finite, and follows declared ordering.
/// 仅当每项结果唯一、未越界、为有限值且遵守声明排序时返回成功。
fn validate_rerank_test_response(
    response: &vulcan_ai_contract::RerankResponse,
    candidate_count: u32,
) -> Result<(), String> {
    if response.results.is_empty() {
        return Err(String::from("Rerank test returned no results."));
    }
    let mut original_indices = BTreeSet::new();
    for (expected_rank, result) in response.results.iter().enumerate() {
        if result.rank != u32::try_from(expected_rank).unwrap_or(u32::MAX)
            || result.original_index >= candidate_count
            || !original_indices.insert(result.original_index)
            || !result.score.is_finite()
        {
            return Err(String::from(
                "Rerank test response contains invalid ranks, indices, or scores.",
            ));
        }
    }
    let ordered =
        response
            .results
            .windows(2)
            .all(|pair| match response.fingerprint.score_semantics {
                RerankScoreSemantics::HigherIsMoreRelevant => pair[0].score >= pair[1].score,
                RerankScoreSemantics::LowerIsMoreRelevant => pair[0].score <= pair[1].score,
                RerankScoreSemantics::Unknown => true,
            });
    if !ordered {
        return Err(String::from(
            "Rerank test scores do not follow the declared score semantics.",
        ));
    }
    Ok(())
}

/// Saves or clears the single selected target for one provider tool capability.
/// 保存或清除一项供应商工具能力唯一选中的目标。
pub(super) async fn service_save_public_ai_provider_tool_selection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderToolSelectionSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if media_understanding_resource_kind(request.capability).is_some() {
        return Err(format!(
            "Media-understanding capability `{:?}` must be configured through its dedicated backend selection.",
            request.capability
        ));
    }
    if !request.capability.is_direct_selection() {
        return Err(format!(
            "Provider tool capability `{:?}` must be configured through its model-extension target set.",
            request.capability
        ));
    }
    // transaction serializes target validation through Workbench publication.
    // transaction 把目标校验串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if let Some(selection) = request.selection.as_ref() {
        let available = runtime.available_provider_tool_targets(&current_configuration)?;
        let valid = available.iter().any(|candidate| {
            candidate.connection_id == selection.connection_id
                && candidate.target.target_id == selection.target_id
                && candidate.target.contract.capability == request.capability
        });
        if !valid {
            return Err(format!(
                "Provider tool target `{}` is not available through connection `{}` for capability `{:?}`.",
                selection.target_id, selection.connection_id, request.capability
            ));
        }
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存公共 AI 工具目标失败",
        |configuration| {
            if let Some(selection) = request.selection.clone() {
                configuration
                    .provider_tool_selections
                    .insert(request.capability, selection);
            } else {
                configuration
                    .provider_tool_selections
                    .remove(&request.capability);
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider-model admission switch for all multimodal understanding lists.
/// 保存一个覆盖全部多模态理解列表的供应商模型准入开关。
///
/// # Parameters
/// 参数
/// `app` publishes the resulting settings, `state` owns runtime state, and `request` is the replacement switch.
/// `app` 发布结果设置，`state` 持有运行时状态，`request` 是替换后的开关值。
///
/// # Returns
/// 返回值
/// The authoritative settings snapshot after persistence and publication.
/// 持久化并发布后的权威设置快照。
pub(super) async fn service_save_public_ai_multimodal_understanding_candidate(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiMultimodalUnderstandingCandidateSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction serializes candidate validation through authoritative settings publication.
    // transaction 把候选校验串行化到权威设置发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    // runtime and currentConfiguration freeze validation against one persisted generation.
    // runtime 与 currentConfiguration 将校验冻结在同一持久化代际。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if request.enabled {
        // settingsCatalog freezes effective route capabilities independently from conversation switches.
        // settingsCatalog 独立于会话开关冻结生效路由能力。
        let settings_catalog = runtime.settings_catalog_snapshot(&current_configuration)?;
        // candidates contains every currently executable account route before the user admission switch.
        // candidates 包含用户准入开关之前当前可执行的全部账号路由。
        let candidates = build_media_understanding_model_candidates(
            &settings_catalog.runtime_configuration,
            &settings_catalog.providers,
            &settings_catalog.catalogs,
        )?;
        // eligible requires at least one exact account route for the shared provider-model switch.
        // eligible 要求共享供应商模型开关至少存在一条精确账号路由。
        let eligible = candidates.iter().any(|candidate| {
            candidate.selection.provider_id == request.provider_id
                && candidate.selection.model_id == request.model_id
        });
        if !eligible {
            return Err(format!(
                "Provider `{}` model `{}` has no executable multimodal conversation route with stable structured output.",
                request.provider_id, request.model_id
            ));
        }
    }
    // snapshot is rebuilt only after the replacement switch is persisted successfully.
    // snapshot 仅在替换后的开关成功持久化后重建。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存多模态理解模型候选开关失败",
        |configuration| {
            configuration.set_multimodal_understanding_candidate_enabled(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                request.enabled,
            );
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves or clears one image, video, or audio understanding backend selection.
/// 保存或清除一项图片、视频或音频理解后端选择。
///
/// # Parameters
/// 参数
/// `app` publishes settings, `state` owns runtime state, and `request` selects one closed backend mode.
/// `app` 发布设置，`state` 持有运行时状态，`request` 选择一个封闭后端模式。
///
/// # Returns
/// 返回值
/// The authoritative settings snapshot after strict target validation.
/// 严格目标校验后的权威设置快照。
pub(super) async fn service_save_public_ai_media_understanding_selection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiMediaUnderstandingSelectionSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if media_understanding_resource_kind(request.capability).is_none() {
        return Err(format!(
            "Capability `{:?}` is not an image, video, or audio understanding capability.",
            request.capability
        ));
    }
    // transaction serializes backend validation through authoritative settings publication.
    // transaction 把后端校验串行化到权威设置发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    // runtime and currentConfiguration freeze targets and candidates to one generation.
    // runtime 与 currentConfiguration 将目标和候选冻结到同一代际。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // settingsCatalog supplies provider targets and resolved model capabilities from one generation.
    // settingsCatalog 从同一代际提供供应商目标与解析后的模型能力。
    let settings_catalog = runtime.settings_catalog_snapshot(&current_configuration)?;
    // modelCandidates remains access-switch independent but still requires executable credentials.
    // modelCandidates 与会话访问开关独立，但仍要求凭据可执行。
    let model_candidates = build_media_understanding_model_candidates(
        &settings_catalog.runtime_configuration,
        &settings_catalog.providers,
        &settings_catalog.catalogs,
    )?;
    if let Some(selection) = request.selection.as_ref() {
        match selection {
            MediaUnderstandingBackendSelection::ProviderTool { selection } => {
                // validTarget binds the exact credential target to the requested semantic capability.
                // validTarget 将精确凭据目标绑定到请求的语义能力。
                let valid_target =
                    settings_catalog
                        .available_provider_tool_targets
                        .iter()
                        .any(|candidate| {
                            candidate.connection_id == selection.connection_id
                                && candidate.target.target_id == selection.target_id
                                && candidate.target.contract.capability == request.capability
                        });
                if !valid_target {
                    return Err(format!(
                        "Provider tool target `{}` is not available through connection `{}` for capability `{:?}`.",
                        selection.target_id, selection.connection_id, request.capability
                    ));
                }
            }
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection,
                reasoning_effort,
            } => {
                if !settings_catalog
                    .runtime_configuration
                    .multimodal_understanding_candidate_is_enabled(
                        selection.provider_id.as_str(),
                        selection.model_id.as_str(),
                    )
                {
                    return Err(format!(
                        "Provider `{}` model `{}` is not enabled as a multimodal understanding candidate.",
                        selection.provider_id, selection.model_id
                    ));
                }
                // candidate is the one immutable account route selected by the user.
                // candidate 是用户选择的唯一不可变账号路由。
                let candidate = model_candidates
                    .iter()
                    .find(|candidate| candidate.selection == *selection)
                    .ok_or_else(|| {
                        format!(
                            "Provider `{}` model `{}` is not executable through connection `{}`.",
                            selection.provider_id, selection.model_id, selection.connection_id
                        )
                    })?;
                if !model_route_supports_media_understanding(
                    &candidate.capabilities,
                    request.capability,
                ) {
                    return Err(format!(
                        "Provider `{}` model `{}` does not support capability `{:?}` with stable structured output.",
                        selection.provider_id, selection.model_id, request.capability
                    ));
                }
                if let Some(reasoning_effort) = reasoning_effort {
                    validate_internal_reasoning_effort(&candidate.capabilities, *reasoning_effort)?;
                }
            }
        }
    }
    // snapshot is rebuilt only after the complete closed backend selection is persisted.
    // snapshot 仅在完整封闭后端选择成功持久化后重建。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存媒体理解后端选择失败",
        |configuration| {
            if let Some(selection) = request.selection.clone() {
                configuration
                    .media_understanding_backend_options
                    .entry(request.capability)
                    .or_default()
                    .update_from_selection(&selection);
                configuration
                    .media_understanding_selections
                    .insert(request.capability, selection);
            } else {
                configuration
                    .media_understanding_selections
                    .remove(&request.capability);
                configuration
                    .media_understanding_backend_options
                    .remove(&request.capability);
            }
            // Remove any legacy global direct target for the same media capability.
            // 同步移除同一媒体能力遗留的全局直连目标。
            configuration
                .provider_tool_selections
                .remove(&request.capability);
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves the complete native target set for one model-extension capability.
/// 保存一项模型扩展能力的完整原生目标集合。
pub(super) async fn service_save_public_ai_provider_extension_target_set(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderExtensionTargetSetSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction serializes native-target validation through Workbench publication.
    // transaction 把原生目标校验串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let replacement = BTreeMap::from([(request.capability, request.selections.clone())]);
    validate_provider_extension_target_sets(
        runtime.as_ref(),
        &current_configuration,
        &replacement,
    )?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存公共 AI 模型扩展目标集合失败",
        |configuration| {
            if request.selections.is_empty() {
                configuration
                    .provider_extension_target_sets
                    .remove(&request.capability);
            } else {
                configuration
                    .provider_extension_target_sets
                    .insert(request.capability, request.selections.clone());
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Runs one real search or extraction diagnostic through the selected row without changing defaults.
/// 通过所选行运行一次真实搜索或网页抓取诊断且不修改默认项。
///
/// # Parameters
/// 参数
/// `state` owns the current runtime and configuration; `request` freezes the exact target and inputs.
/// `state` 持有当前运行时与配置；`request` 固定精确目标与输入。
///
/// # Returns
/// 返回值
/// A unified search or extraction result, or an explicit validation/provider failure.
/// 返回统一搜索或网页抓取结果，或明确的校验与供应商错误。
pub(super) async fn service_test_public_ai_provider_tool(
    state: SharedServiceBridgeState,
    request: PublicAiProviderToolTestRequest,
) -> Result<ProviderToolExecutionResult, String> {
    if !matches!(
        request.capability,
        ProviderToolCapabilityKind::WebSearch | ProviderToolCapabilityKind::WebExtract
    ) {
        return Err(format!(
            "Provider tool settings diagnostics do not support capability `{:?}`.",
            request.capability
        ));
    }
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    runtime
        .execute_provider_tool_target_for_owner(
            &configuration,
            request.capability,
            &request.selection,
            request.arguments,
            None,
        )
        .await
}

/// Saves the default enabled state of one semantic provider tool capability.
/// 保存一项语义供应商工具能力的默认启用状态。
pub(super) async fn service_save_public_ai_provider_tool_capability_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderToolCapabilityEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if !request.capability.is_direct_selection() {
        return Err(format!(
            "Provider tool capability `{:?}` is enabled by selecting its model-extension targets.",
            request.capability
        ));
    }
    // transaction serializes capability validation through Workbench publication.
    // transaction 把能力校验串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if request.enabled {
        // settingsCatalog freezes provider-tool and conversation-model inventories to one generation.
        // settingsCatalog 将供应商工具与会话模型目录冻结在同一代际。
        let settings_catalog = runtime.settings_catalog_snapshot(&current_configuration)?;
        // mediaModelCandidates is derived only for fixed media facades; search and fetch keep their direct-target rule.
        // mediaModelCandidates 仅为固定媒体外壳派生；搜索与抓取继续遵循直连目标规则。
        let media_model_candidates =
            if media_understanding_resource_kind(request.capability).is_some() {
                build_media_understanding_model_candidates(
                    &settings_catalog.runtime_configuration,
                    &settings_catalog.providers,
                    &settings_catalog.catalogs,
                )?
            } else {
                Vec::new()
            };
        if !provider_tool_capability_backend_is_available(
            &settings_catalog.runtime_configuration,
            request.capability,
            &settings_catalog.available_provider_tool_targets,
            &media_model_candidates,
        ) {
            return Err(format!(
                "Provider tool capability `{:?}` has no available configured backend.",
                request.capability
            ));
        }
    }
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存公共 AI 工具能力启用状态失败",
        |configuration| {
            if request.enabled {
                configuration
                    .provider_tool_enabled_capabilities
                    .insert(request.capability);
            } else {
                configuration
                    .provider_tool_enabled_capabilities
                    .remove(&request.capability);
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Returns the effective single-select provider tool configuration for one session.
/// 返回单个会话生效的供应商工具单选配置。
pub(super) async fn service_public_ai_session_provider_tool_selections(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<PublicAiSessionProviderToolSettingsSnapshot, String> {
    let session_id = conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| format!("Invalid conversation id `{conversation_id}`: {error}"))?;
    // transaction freezes provider-tool defaults through optional lazy-state publication.
    // transaction 在可选惰性状态发布期间冻结供应商工具默认值。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    // preview determines whether the authoritative session row requires any lazy migration.
    // preview 判断权威会话行是否需要任何惰性迁移。
    let (preview, requires_publication) = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("Workspace is not initialized."))?;
        // runtimeState contains the authoritative session row whose provider-tool fields are projected below.
        // runtimeState 包含权威会话记录，其供应商工具字段会在下方投影。
        let runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        // plannedRuntimeState copies only fields consumed and mutated by provider-tool reconciliation.
        // plannedRuntimeState 只复制供应商工具协调会消费和修改的字段。
        let mut planned_runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
            provider_tool_selections: runtime_state.provider_tool_selections.clone(),
            media_understanding_modes: runtime_state.media_understanding_modes.clone(),
            provider_tool_enabled_capabilities: runtime_state
                .provider_tool_enabled_capabilities
                .clone(),
            provider_extension_target_sets: runtime_state.provider_extension_target_sets.clone(),
            provider_tool_settings_initialized: runtime_state.provider_tool_settings_initialized,
            provider_tool_selections_overridden: runtime_state.provider_tool_selections_overridden,
            updated_at: runtime_state.updated_at.clone(),
            ..Default::default()
        };
        let (
            enabled_capabilities,
            selections,
            media_understanding_modes,
            extension_target_sets,
            state_changed,
        ) = reconcile_session_provider_tool_settings(&mut planned_runtime_state, &configuration)?;
        let conflicts = provider_tool_session_conflicts(
            runtime.as_ref(),
            &configuration,
            &enabled_capabilities,
            &selections,
            &media_understanding_modes,
            &extension_target_sets,
        )?;
        (
            PublicAiSessionProviderToolSettingsSnapshot {
                enabled_capabilities,
                selections,
                media_understanding_modes,
                extension_target_sets,
                conflicts,
            },
            state_changed,
        )
    };
    if !requires_publication {
        drop(transaction);
        return Ok(preview);
    }
    // committedSnapshot is written only by the detached reconciliation that wins publication.
    // committedSnapshot 仅由赢得发布的独立协调写入。
    let committed_snapshot = std::cell::RefCell::new(None);
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        &app,
        &state,
        runtime.as_ref(),
        &configuration,
        |workspace| {
            let runtime_state = workspace
                .session_runtime_state
                .get_mut(&session_id)
                .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
            let (
                enabled_capabilities,
                selections,
                media_understanding_modes,
                extension_target_sets,
                state_changed,
            ) = reconcile_session_provider_tool_settings(runtime_state, &configuration)?;
            let conflicts = provider_tool_session_conflicts(
                runtime.as_ref(),
                &configuration,
                &enabled_capabilities,
                &selections,
                &media_understanding_modes,
                &extension_target_sets,
            )?;
            committed_snapshot.replace(Some(PublicAiSessionProviderToolSettingsSnapshot {
                enabled_capabilities,
                selections,
                media_understanding_modes,
                extension_target_sets,
                conflicts,
            }));
            Ok(PublicAiWorkspaceReconciliation::workspace_only(
                state_changed.then_some(session_id).into_iter().collect(),
            ))
        },
        false,
        "provider_tool_settings_initialize",
        || Ok(()),
    )?;
    drop(transaction);
    committed_snapshot.into_inner().ok_or_else(|| {
        String::from("Provider tool session settings publication produced no committed snapshot.")
    })
}

/// Reconciles one session's persisted provider-tool fields against current global defaults.
/// 根据当前全局默认值协调一个会话的持久供应商工具字段。
///
/// # Parameters
/// 参数
/// `runtime_state` is a detached or authoritative session row; `configuration` owns the current
/// default selections and enabled capability set.
/// `runtime_state` 是独立或权威会话行；`configuration` 持有当前默认选择与已启用能力集合。
///
/// # Returns
/// 返回值
/// The normalized direct capabilities, direct selections, extension target sets, and whether persisted fields changed.
/// 返回规范化的直连能力、直连选择、扩展目标集合，以及持久字段是否发生变化。
fn reconcile_session_provider_tool_settings(
    runtime_state: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    configuration: &PersistedAiConfiguration,
) -> Result<
    (
        BTreeSet<ProviderToolCapabilityKind>,
        BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
        BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode>,
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
        bool,
    ),
    String,
> {
    // stateChanged controls both durable persistence and Workbench publication.
    // stateChanged 同时控制持久化与工作台发布。
    let mut state_changed = false;
    // mediaCapabilityKeys identifies legacy direct targets that must never remain in session state.
    // mediaCapabilityKeys 标识绝不能继续保留在会话状态中的历史媒体直连目标。
    let media_capability_keys = [
        ProviderToolCapabilityKind::ImageUnderstand,
        ProviderToolCapabilityKind::VideoUnderstand,
        ProviderToolCapabilityKind::AudioUnderstand,
    ]
    .into_iter()
    .map(provider_tool_capability_key)
    .collect::<BTreeSet<_>>();
    if runtime_state.provider_tool_settings_initialized {
        if !runtime_state.provider_tool_selections_overridden {
            // nextSelections mirrors the exact current global target defaults.
            // nextSelections 镜像精确的当前全局目标默认值。
            let next_selections = configuration
                .provider_tool_selections
                .iter()
                .filter(|(capability, _)| media_understanding_resource_kind(**capability).is_none())
                .map(|(capability, selection)| {
                    (
                        provider_tool_capability_key(*capability),
                        vulcan_workbench_model::SessionProviderToolTargetSelection {
                            connection_id: selection.connection_id.clone(),
                            target_id: selection.target_id.clone(),
                        },
                    )
                })
                .collect();
            // nextEnabledCapabilities mirrors the exact current global enabled defaults.
            // nextEnabledCapabilities 镜像精确的当前全局启用默认值。
            let next_enabled_capabilities = configuration
                .provider_tool_enabled_capabilities
                .iter()
                .map(|capability| provider_tool_capability_key(*capability))
                .collect();
            // nextMediaUnderstandingModes mirrors the active global media backend family while
            // keeping the session field independent from exact model and account selections.
            // nextMediaUnderstandingModes 镜像活动全局媒体后端类型，同时让会话字段独立于精确模型与账号选择。
            let next_media_understanding_modes = session_media_understanding_modes_to_runtime(
                &session_media_understanding_modes_from_configuration(configuration),
            );
            // nextExtensionTargetSets mirrors the exact current global extension defaults.
            // nextExtensionTargetSets 镜像精确的当前全局扩展目标默认集合。
            let next_extension_target_sets = configuration
                .provider_extension_target_sets
                .iter()
                .map(|(capability, selections)| {
                    (
                        provider_tool_capability_key(*capability),
                        selections
                            .iter()
                            .map(session_provider_tool_selection_from_contract)
                            .collect(),
                    )
                })
                .collect();
            if runtime_state.provider_tool_selections != next_selections
                || runtime_state.provider_tool_enabled_capabilities != next_enabled_capabilities
                || runtime_state.media_understanding_modes != next_media_understanding_modes
                || runtime_state.provider_extension_target_sets != next_extension_target_sets
            {
                runtime_state.provider_tool_selections = next_selections;
                runtime_state.provider_tool_enabled_capabilities = next_enabled_capabilities;
                runtime_state.media_understanding_modes = next_media_understanding_modes;
                runtime_state.provider_extension_target_sets = next_extension_target_sets;
                state_changed = true;
            }
        }
    } else {
        // Schema 4 intentionally discards pre-extension Provider Tool state and rematerializes
        // the current defaults instead of guessing an old-to-new semantic conversion.
        // Schema 4 有意丢弃模型扩展前的 Provider Tool 状态，并重新物化当前默认值，
        // 而不是猜测旧语义到新语义的转换。
        runtime_state.provider_tool_selections = configuration
            .provider_tool_selections
            .iter()
            .filter(|(capability, _)| media_understanding_resource_kind(**capability).is_none())
            .map(|(capability, selection)| {
                (
                    provider_tool_capability_key(*capability),
                    session_provider_tool_selection_from_contract(selection),
                )
            })
            .collect();
        runtime_state.provider_tool_enabled_capabilities = configuration
            .provider_tool_enabled_capabilities
            .iter()
            .map(|capability| provider_tool_capability_key(*capability))
            .collect();
        runtime_state.media_understanding_modes = session_media_understanding_modes_to_runtime(
            &session_media_understanding_modes_from_configuration(configuration),
        );
        runtime_state.provider_extension_target_sets = configuration
            .provider_extension_target_sets
            .iter()
            .map(|(capability, selections)| {
                (
                    provider_tool_capability_key(*capability),
                    selections
                        .iter()
                        .map(session_provider_tool_selection_from_contract)
                        .collect(),
                )
            })
            .collect();
        runtime_state.provider_tool_selections_overridden = false;
        runtime_state.provider_tool_settings_initialized = true;
        state_changed = true;
    }
    // cleanedSelections removes legacy media targets even when the session has explicit overrides.
    // cleanedSelections 即使会话存在显式覆盖，也会移除历史媒体目标。
    let cleaned_selections = runtime_state
        .provider_tool_selections
        .iter()
        .filter(|(capability, _)| !media_capability_keys.contains(*capability))
        .map(|(capability, selection)| (capability.clone(), selection.clone()))
        .collect::<BTreeMap<_, _>>();
    if cleaned_selections != runtime_state.provider_tool_selections {
        runtime_state.provider_tool_selections = cleaned_selections;
        state_changed = true;
    }
    // cleanedMediaModes removes malformed legacy capability keys and fills only missing media
    // defaults for an explicit session override created before this field existed.
    // cleanedMediaModes 清理旧的非法能力键，并仅为新增字段之前创建的显式会话补齐缺失媒体默认值。
    let mut cleaned_media_modes = runtime_state
        .media_understanding_modes
        .iter()
        .filter(|(capability, _)| media_capability_keys.contains(*capability))
        .map(|(capability, mode)| (capability.clone(), *mode))
        .collect::<BTreeMap<_, _>>();
    if runtime_state.provider_tool_selections_overridden {
        for (capability, mode) in
            session_media_understanding_modes_from_configuration(configuration)
        {
            cleaned_media_modes
                .entry(provider_tool_capability_key(capability))
                .or_insert(mode);
        }
    }
    if cleaned_media_modes != runtime_state.media_understanding_modes {
        runtime_state.media_understanding_modes = cleaned_media_modes;
        state_changed = true;
    }
    if state_changed {
        runtime_state.updated_at = crate::current_unix_millis_text();
    }
    let enabled_capabilities =
        session_provider_tool_enabled_capabilities_from_runtime(runtime_state)?;
    let selections = session_provider_tool_selections_from_runtime(runtime_state)?;
    let media_understanding_modes = session_media_understanding_modes_from_runtime(runtime_state)?;
    let extension_target_sets = session_provider_extension_target_sets_from_runtime(runtime_state)?;
    Ok((
        enabled_capabilities,
        selections,
        media_understanding_modes,
        extension_target_sets,
        state_changed,
    ))
}

/// Saves the complete single-select provider tool configuration for one session.
/// 保存单个会话完整的供应商工具单选配置。
pub(super) async fn service_save_public_ai_session_provider_tool_selections(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiSessionProviderToolSelectionsSaveRequest,
) -> Result<PublicAiSessionProviderToolSettingsSnapshot, String> {
    // normalizedRequest preserves only the session enable switch and backend mode for media capabilities.
    // normalizedRequest 对媒体能力只保留会话启用开关与后端模式。
    let mut request = request;
    request
        .selections
        .retain(|capability, _| media_understanding_resource_kind(*capability).is_none());
    request
        .media_understanding_modes
        .retain(|capability, _| media_understanding_resource_kind(*capability).is_some());
    let session_id = request
        .conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| {
            format!(
                "Invalid conversation id `{}`: {error}",
                request.conversation_id
            )
        })?;
    // transaction keeps target validation and detached workspace publication on one generation.
    // transaction 让目标校验与独立工作区发布保持在同一代际。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_provider_tool_session_settings(
        runtime.as_ref(),
        &configuration,
        &request.enabled_capabilities,
        &request.selections,
        &request.media_understanding_modes,
        &request.extension_target_sets,
    )?;
    // snapshot is complete before any browser-visible or authoritative workspace assignment.
    // snapshot 在任何浏览器可见或权威工作区赋值前保持完整。
    let snapshot = PublicAiSessionProviderToolSettingsSnapshot {
        enabled_capabilities: request.enabled_capabilities,
        selections: request.selections,
        media_understanding_modes: request.media_understanding_modes,
        extension_target_sets: request.extension_target_sets,
        conflicts: Vec::new(),
    };
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        &app,
        &state,
        runtime.as_ref(),
        &configuration,
        |workspace| {
            let runtime_state = workspace
                .session_runtime_state
                .get_mut(&session_id)
                .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
            runtime_state.provider_tool_selections = snapshot
                .selections
                .iter()
                .map(|(capability, selection)| {
                    (
                        provider_tool_capability_key(*capability),
                        vulcan_workbench_model::SessionProviderToolTargetSelection {
                            connection_id: selection.connection_id.clone(),
                            target_id: selection.target_id.clone(),
                        },
                    )
                })
                .collect();
            runtime_state.provider_tool_enabled_capabilities = snapshot
                .enabled_capabilities
                .iter()
                .map(|capability| provider_tool_capability_key(*capability))
                .collect();
            runtime_state.media_understanding_modes =
                session_media_understanding_modes_to_runtime(&snapshot.media_understanding_modes);
            runtime_state.provider_extension_target_sets = snapshot
                .extension_target_sets
                .iter()
                .map(|(capability, selections)| {
                    (
                        provider_tool_capability_key(*capability),
                        selections
                            .iter()
                            .map(session_provider_tool_selection_from_contract)
                            .collect(),
                    )
                })
                .collect();
            runtime_state.provider_tool_settings_initialized = true;
            runtime_state.provider_tool_selections_overridden = true;
            runtime_state.updated_at = crate::current_unix_millis_text();
            Ok(PublicAiWorkspaceReconciliation::workspace_only(
                BTreeSet::from([session_id]),
            ))
        },
        false,
        "provider_tool_settings",
        || Ok(()),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Validates one complete session provider tool snapshot before persistence or dispatch.
/// 在持久化或分派前校验一份完整会话供应商工具快照。
pub(crate) fn validate_provider_tool_session_settings(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    enabled_capabilities: &BTreeSet<ProviderToolCapabilityKind>,
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    media_understanding_modes: &BTreeMap<
        ProviderToolCapabilityKind,
        vulcan_workbench_model::SessionMediaUnderstandingMode,
    >,
    extension_target_sets: &BTreeMap<
        ProviderToolCapabilityKind,
        BTreeSet<ProviderToolTargetSelection>,
    >,
) -> Result<(), String> {
    let conflicts = provider_tool_session_conflicts(
        runtime,
        configuration,
        enabled_capabilities,
        selections,
        media_understanding_modes,
        extension_target_sets,
    )?;
    if conflicts.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Provider tool session settings require confirmation: {}",
        conflicts
            .iter()
            .map(|conflict| conflict.message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

/// Finds every unavailable or incomplete provider tool choice retained by one session.
/// 查找单个会话保留的全部不可用或不完整供应商工具选择。
fn provider_tool_session_conflicts(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    enabled_capabilities: &BTreeSet<ProviderToolCapabilityKind>,
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    media_understanding_modes: &BTreeMap<
        ProviderToolCapabilityKind,
        vulcan_workbench_model::SessionMediaUnderstandingMode,
    >,
    extension_target_sets: &BTreeMap<
        ProviderToolCapabilityKind,
        BTreeSet<ProviderToolTargetSelection>,
    >,
) -> Result<Vec<PublicAiSessionProviderToolConflict>, String> {
    // effectiveConfiguration applies session-selected media modes without mutating global persistence.
    // effectiveConfiguration 应用会话选定的媒体模式，但不修改全局持久化配置。
    let mut effective_configuration = configuration.clone();
    let mut media_mode_errors = BTreeMap::new();
    for (capability, mode) in media_understanding_modes {
        let backend_mode = match mode {
            vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool => {
                vulcan_provider_adapters::MediaUnderstandingBackendMode::ProviderTool
            }
            vulcan_workbench_model::SessionMediaUnderstandingMode::ModelReplacement => {
                vulcan_provider_adapters::MediaUnderstandingBackendMode::ModelReplacement
            }
        };
        if let Err(error) =
            effective_configuration.apply_media_understanding_mode(*capability, backend_mode)
        {
            media_mode_errors.insert(*capability, error);
        }
    }
    // settingsCatalog supplies the exact conversation routes needed to validate session-selected media backends.
    // settingsCatalog 提供校验会话选定媒体后端所需的精确会话路由。
    let settings_catalog = runtime.settings_catalog_snapshot(&effective_configuration)?;
    // available belongs to the same immutable catalog generation as every media model candidate.
    // available 与全部媒体模型候选归属于同一份不可变目录代际。
    let available = &settings_catalog.available_provider_tool_targets;
    // mediaCapabilitiesEnabled avoids deriving an unrelated model inventory for sessions that use only search or fetch.
    // mediaCapabilitiesEnabled 避免为仅使用搜索或抓取的会话派生无关模型目录。
    let media_capabilities_enabled = enabled_capabilities
        .iter()
        .copied()
        .any(|capability| media_understanding_resource_kind(capability).is_some());
    // mediaModelCandidates applies the same structured-output and media-input route projection as settings.
    // mediaModelCandidates 应用与设置页相同的结构化输出及媒体输入路由投影。
    let media_model_candidates = if media_capabilities_enabled {
        build_media_understanding_model_candidates(
            &settings_catalog.runtime_configuration,
            &settings_catalog.providers,
            &settings_catalog.catalogs,
        )?
    } else {
        Vec::new()
    };
    let capabilities = enabled_capabilities.clone();
    let mut conflicts = Vec::new();
    for capability in capabilities {
        if !capability.is_direct_selection() {
            conflicts.push(PublicAiSessionProviderToolConflict {
                capability,
                kind: String::from("invalid_direct_capability"),
                message: format!(
                    "Capability `{:?}` must be configured as a model-extension target set.",
                    capability
                ),
            });
            continue;
        }
        if media_understanding_resource_kind(capability).is_some() {
            let Some(mode) = media_understanding_modes.get(&capability) else {
                conflicts.push(PublicAiSessionProviderToolConflict {
                    capability,
                    kind: String::from("missing_selection"),
                    message: format!(
                        "Media-understanding capability `{:?}` is enabled but has no selected backend mode.",
                        capability
                    ),
                });
                continue;
            };
            if let Some(error) = media_mode_errors.get(&capability) {
                conflicts.push(PublicAiSessionProviderToolConflict {
                    capability,
                    kind: String::from("target_unavailable"),
                    message: error.clone(),
                });
            } else if !media_understanding_backend_is_available(
                &settings_catalog.runtime_configuration,
                capability,
                available,
                &media_model_candidates,
            ) {
                conflicts.push(PublicAiSessionProviderToolConflict {
                    capability,
                    kind: String::from("target_unavailable"),
                    message: format!(
                        "The `{mode:?}` backend for media-understanding capability `{:?}` is no longer available; configure it again before sending.",
                        capability,
                    ),
                });
            }
            continue;
        }
        let Some(selection) = selections.get(&capability) else {
            conflicts.push(PublicAiSessionProviderToolConflict {
                capability,
                kind: String::from("missing_selection"),
                message: format!(
                    "Capability `{:?}` is enabled but has no selected model and account.",
                    capability
                ),
            });
            continue;
        };
        if !available.iter().any(|candidate| {
            candidate.connection_id == selection.connection_id
                && candidate.target.target_id == selection.target_id
                && candidate.target.contract.capability == capability
        }) {
            conflicts.push(PublicAiSessionProviderToolConflict {
                capability,
                kind: String::from("target_unavailable"),
                message: format!(
                    "The saved model target `{}` or account `{}` for capability `{:?}` is no longer available; select them again before sending.",
                    selection.target_id, selection.connection_id, capability
                ),
            });
        }
    }
    if let Err(error) =
        validate_provider_extension_target_sets(runtime, configuration, extension_target_sets)
    {
        conflicts.push(PublicAiSessionProviderToolConflict {
            capability: extension_target_sets
                .keys()
                .next()
                .copied()
                .unwrap_or(ProviderToolCapabilityKind::ImageGenerate),
            kind: String::from("invalid_extension_target_set"),
            message: error,
        });
    }
    Ok(conflicts)
}

/// Reports whether one configured media-understanding backend remains exactly executable.
/// 报告一项已配置媒体理解后端是否仍可精确执行。
///
/// # Parameters
/// 参数
/// `configuration` owns the closed backend choice and candidate switch, `capability` identifies
/// one fixed media tool, and the two slices are frozen provider-tool and conversation inventories.
/// `configuration` 持有封闭后端选择与候选开关，`capability` 标识一个固定媒体工具，两个切片分别是
/// 冻结的供应商工具目录与会话模型目录。
///
/// # Returns
/// 返回值
/// `true` only when the exact provider target or admitted structured-output model route still exists.
/// 仅当精确供应商目标或已准入的结构化输出模型路由仍然存在时返回 `true`。
fn media_understanding_backend_is_available(
    configuration: &PersistedAiConfiguration,
    capability: ProviderToolCapabilityKind,
    available_provider_targets: &[AvailableProviderToolTarget],
    model_candidates: &[PublicAiMediaUnderstandingModelCandidate],
) -> bool {
    let Some(backend) = configuration
        .media_understanding_selections
        .get(&capability)
    else {
        return false;
    };
    match backend {
        MediaUnderstandingBackendSelection::ProviderTool { selection } => {
            available_provider_targets.iter().any(|candidate| {
                candidate.connection_id == selection.connection_id
                    && candidate.target.target_id == selection.target_id
                    && candidate.target.contract.capability == capability
            })
        }
        MediaUnderstandingBackendSelection::ModelReplacement {
            selection,
            reasoning_effort,
        } => {
            configuration.multimodal_understanding_candidate_is_enabled(
                selection.provider_id.as_str(),
                selection.model_id.as_str(),
            ) && model_candidates.iter().any(|candidate| {
                candidate.selection == *selection
                    && model_route_supports_media_understanding(&candidate.capabilities, capability)
                    && reasoning_effort.is_none_or(|effort| {
                        validate_internal_reasoning_effort(&candidate.capabilities, effort).is_ok()
                    })
            })
        }
    }
}

/// Reports whether one direct capability has an executable provider or replacement backend.
/// 报告一项直连能力是否具有可执行的供应商后端或模型代替后端。
///
/// # Parameters
/// 参数
/// `configuration` owns media selections, `capability` selects the validation rule, and the two
/// inventories belong to one settings-catalog generation.
/// `configuration` 持有媒体选择，`capability` 选择校验规则，两份目录归属于同一设置目录代际。
///
/// # Returns
/// 返回值
/// `true` for an exact executable media backend or at least one matching ordinary direct target.
/// 精确媒体后端可执行，或至少存在一个匹配的普通直连目标时返回 `true`。
fn provider_tool_capability_backend_is_available(
    configuration: &PersistedAiConfiguration,
    capability: ProviderToolCapabilityKind,
    available_provider_targets: &[AvailableProviderToolTarget],
    model_candidates: &[PublicAiMediaUnderstandingModelCandidate],
) -> bool {
    if media_understanding_resource_kind(capability).is_some() {
        return media_understanding_backend_is_available(
            configuration,
            capability,
            available_provider_targets,
            model_candidates,
        );
    }
    available_provider_targets
        .iter()
        .any(|candidate| candidate.target.contract.capability == capability)
}

/// Filters historical selections to targets that still exist and remain enabled.
/// 将历史选择过滤为仍存在且保持启用的当前目标。
///
/// # Parameters
/// 参数
/// `available` is the target inventory captured with the settings generation; `selections` owns
/// the persisted capability choices to validate against that exact inventory.
/// `available` 是与设置代际共同捕获的目标目录；`selections` 持有要针对该精确目录校验的
/// 持久化能力选择。
///
/// # Returns
/// 返回值
/// Only exact connection/target/capability triples present in the supplied inventory.
/// 仅返回传入目录中存在的精确连接、目标与能力三元组。
fn filter_current_provider_tool_selections(
    available: &[AvailableProviderToolTarget],
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
) -> BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection> {
    selections
        .iter()
        .filter(|(capability, selection)| {
            available.iter().any(|candidate| {
                candidate.connection_id == selection.connection_id
                    && candidate.target.target_id == selection.target_id
                    && candidate.target.contract.capability == **capability
            })
        })
        .map(|(capability, selection)| (*capability, selection.clone()))
        .collect()
}

/// Validates complete native model-extension target sets against one frozen runtime inventory.
/// 针对一份冻结运行时目录校验完整原生模型扩展目标集合。
fn validate_provider_extension_target_sets(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    target_sets: &BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
) -> Result<(), String> {
    let available = runtime.available_provider_tool_targets(configuration)?;
    for (capability, selections) in target_sets {
        if !capability.is_model_extension() {
            return Err(format!(
                "Provider tool capability `{:?}` must use direct single-target selection.",
                capability
            ));
        }
        let mut public_model_ids = BTreeSet::new();
        for selection in selections {
            let candidate = available
                .iter()
                .find(|candidate| {
                    candidate.connection_id == selection.connection_id
                        && candidate.target.target_id == selection.target_id
                        && candidate.target.contract.capability == *capability
                })
                .ok_or_else(|| {
                    format!(
                        "Model-extension target `{}` is not available through connection `{}` for capability `{:?}`.",
                        selection.target_id, selection.connection_id, capability
                    )
                })?;
            if candidate.manifest_source_layer == ManifestSourceLayer::Custom {
                return Err(format!(
                    "Custom provider target `{}` cannot be selected as a model extension.",
                    selection.target_id
                ));
            }
            if candidate.target.contract.extension_profile.is_none() {
                return Err(format!(
                    "Native model-extension target `{}` has no reviewed extension profile.",
                    selection.target_id
                ));
            }
            let public_model_id = format!(
                "{}/{}",
                candidate.target.provider_id, candidate.target.upstream_target_id
            );
            if !public_model_ids.insert(public_model_id.clone()) {
                return Err(format!(
                    "Model-extension capability `{:?}` exposes duplicate public model id `{public_model_id}`.",
                    capability
                ));
            }
        }
    }
    Ok(())
}

/// Converts the active global media selections into session backend modes.
/// 将活动的全局媒体选择转换为会话后端模式。
///
/// # Parameters
/// 参数
/// `configuration` owns the currently active global media backend selection for each capability.
/// `configuration` 持有每项能力当前活动的全局媒体后端选择。
///
/// # Returns
/// 返回值
/// A closed map containing only image, video, and audio understanding modes.
/// 仅包含图片、视频与音频理解模式的封闭映射。
pub(crate) fn session_media_understanding_modes_from_configuration(
    configuration: &PersistedAiConfiguration,
) -> BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode> {
    configuration
        .media_understanding_selections
        .iter()
        .filter_map(|(capability, selection)| {
            let mode = match selection {
                MediaUnderstandingBackendSelection::ProviderTool { .. } => {
                    vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool
                }
                MediaUnderstandingBackendSelection::ModelReplacement { .. } => {
                    vulcan_workbench_model::SessionMediaUnderstandingMode::ModelReplacement
                }
            };
            media_understanding_resource_kind(*capability).map(|_| (*capability, mode))
        })
        .collect()
}

/// Converts one session runtime mode map into the typed provider contract.
/// 将会话运行时模式映射转换为类型化供应商契约。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the persisted snake_case capability keys and closed mode values.
/// `runtime_state` 持有持久化的 snake_case 能力键与封闭模式值。
///
/// # Returns
/// 返回值
/// The typed media mode map, or an explicit invalid-key error.
/// 类型化媒体模式映射；键非法时返回明确错误。
fn session_media_understanding_modes_from_runtime(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> Result<
    BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode>,
    String,
> {
    runtime_state
        .media_understanding_modes
        .iter()
        .map(|(capability, mode)| {
            let capability = serde_json::from_value(serde_json::Value::String(capability.clone()))
                .map_err(|error| {
                    format!(
                        "Session media-understanding capability `{capability}` is invalid: {error}"
                    )
                })?;
            if media_understanding_resource_kind(capability).is_none() {
                return Err(format!(
                    "Session capability `{capability:?}` cannot use a media-understanding mode."
                ));
            }
            Ok((capability, *mode))
        })
        .collect()
}

/// Converts one typed media mode map into the session runtime persistence contract.
/// 将类型化媒体模式映射转换为会话运行时持久化契约。
fn session_media_understanding_modes_to_runtime(
    modes: &BTreeMap<
        ProviderToolCapabilityKind,
        vulcan_workbench_model::SessionMediaUnderstandingMode,
    >,
) -> BTreeMap<String, vulcan_workbench_model::SessionMediaUnderstandingMode> {
    modes
        .iter()
        .map(|(capability, mode)| (provider_tool_capability_key(*capability), *mode))
        .collect()
}

/// Converts one provider-contract selection into the session persistence contract.
/// 把一个供应商契约选择转换为会话持久化契约。
fn session_provider_tool_selection_from_contract(
    selection: &ProviderToolTargetSelection,
) -> vulcan_workbench_model::SessionProviderToolTargetSelection {
    vulcan_workbench_model::SessionProviderToolTargetSelection {
        connection_id: selection.connection_id.clone(),
        target_id: selection.target_id.clone(),
    }
}

/// Converts one runtime-state selection map into the provider contract.
/// 将运行状态选择映射转换为供应商契约。
fn session_provider_tool_selections_from_runtime(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> Result<BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>, String> {
    runtime_state
        .provider_tool_selections
        .iter()
        .map(|(capability, selection)| {
            Ok((
                serde_json::from_value(serde_json::Value::String(capability.clone())).map_err(
                    |error| {
                        format!(
                            "Session provider tool capability `{capability}` is invalid: {error}"
                        )
                    },
                )?,
                ProviderToolTargetSelection {
                    connection_id: selection.connection_id.clone(),
                    target_id: selection.target_id.clone(),
                },
            ))
        })
        .collect()
}

/// Converts session model-extension target sets into the provider contract.
/// 把会话模型扩展目标集合转换为供应商契约。
fn session_provider_extension_target_sets_from_runtime(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> Result<BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>, String> {
    runtime_state
        .provider_extension_target_sets
        .iter()
        .map(|(capability, selections)| {
            let capability = serde_json::from_value(serde_json::Value::String(capability.clone()))
                .map_err(|error| {
                    format!("Session model-extension capability `{capability}` is invalid: {error}")
                })?;
            let selections = selections
                .iter()
                .map(|selection| ProviderToolTargetSelection {
                    connection_id: selection.connection_id.clone(),
                    target_id: selection.target_id.clone(),
                })
                .collect();
            Ok((capability, selections))
        })
        .collect()
}

/// Converts one runtime-state capability set into the provider contract.
/// 将一个运行状态能力集合转换为供应商契约。
fn session_provider_tool_enabled_capabilities_from_runtime(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> Result<BTreeSet<ProviderToolCapabilityKind>, String> {
    runtime_state
        .provider_tool_enabled_capabilities
        .iter()
        .map(|capability| {
            serde_json::from_value(serde_json::Value::String(capability.clone())).map_err(|error| {
                format!("Session provider tool capability `{capability}` is invalid: {error}")
            })
        })
        .collect()
}

/// Serializes one capability key exactly as the public JSON contract.
/// 按公共 JSON 契约精确序列化一项能力键。
pub(crate) fn provider_tool_capability_key(capability: ProviderToolCapabilityKind) -> String {
    serde_json::to_value(capability)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .expect("provider tool capability must serialize as a string")
}

/// Returns every media resource backed by a valid understanding bridge for the effective session.
/// 返回当前会话生效配置中由有效理解桥接承载的全部媒体资源。
///
/// Inherited sessions read the current global defaults, while explicit session overrides remain isolated.
/// 继承全局设置的会话读取当前全局默认值，会话显式覆盖则保持隔离。
///
/// # Parameters
/// 参数
/// `runtime` resolves currently available provider-tool targets; `configuration` owns global selections; `runtime_state` is the selected session state.
/// `runtime` 解析当前可用的供应商工具目标；`configuration` 持有全局选择；`runtime_state` 是选中会话的运行态。
///
/// # Returns
/// 返回值
/// A closed set containing image, video, or audio only when the corresponding capability is enabled
/// and its exact provider-tool or model-replacement backend remains executable.
/// 返回封闭集合；仅当对应能力已启用且精确供应商工具或模型代替后端仍可执行时，
/// 集合才包含图片、视频或音频。
pub(crate) fn public_ai_session_media_understanding_bridge_kinds(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
) -> Result<BTreeSet<ResourceKind>, String> {
    let explicit_session_settings = runtime_state.is_some_and(|state| {
        state.provider_tool_settings_initialized && state.provider_tool_selections_overridden
    });
    // enabledCapabilities preserves the session switch layer while media modes are applied to a
    // detached configuration copy for this bridge calculation.
    // enabledCapabilities 保留会话开关层，同时把媒体模式应用到独立配置副本供本次桥接计算使用。
    let enabled_capabilities = if explicit_session_settings {
        let runtime_state = runtime_state.expect("explicit session settings require runtime state");
        session_provider_tool_enabled_capabilities_from_runtime(runtime_state)?
    } else {
        configuration.provider_tool_enabled_capabilities.clone()
    };
    let mut effective_configuration = configuration.clone();
    if explicit_session_settings {
        let runtime_state = runtime_state.expect("explicit session settings require runtime state");
        for (capability, mode) in session_media_understanding_modes_from_runtime(runtime_state)? {
            let backend_mode = match mode {
                vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool => {
                    vulcan_provider_adapters::MediaUnderstandingBackendMode::ProviderTool
                }
                vulcan_workbench_model::SessionMediaUnderstandingMode::ModelReplacement => {
                    vulcan_provider_adapters::MediaUnderstandingBackendMode::ModelReplacement
                }
            };
            effective_configuration.apply_media_understanding_mode(capability, backend_mode)?;
        }
    }
    let settings_catalog = runtime.settings_catalog_snapshot(&effective_configuration)?;
    // availableProviderTargets shares one immutable generation with modelCandidates.
    // availableProviderTargets 与 modelCandidates 共享同一份不可变目录代际。
    let available_provider_targets = &settings_catalog.available_provider_tool_targets;
    let model_candidates = build_media_understanding_model_candidates(
        &settings_catalog.runtime_configuration,
        &settings_catalog.providers,
        &settings_catalog.catalogs,
    )?;
    let mut bridged_media_kinds = BTreeSet::new();
    for capability in [
        ProviderToolCapabilityKind::ImageUnderstand,
        ProviderToolCapabilityKind::VideoUnderstand,
        ProviderToolCapabilityKind::AudioUnderstand,
    ] {
        if !enabled_capabilities.contains(&capability) {
            continue;
        }
        if media_understanding_backend_is_available(
            &settings_catalog.runtime_configuration,
            capability,
            available_provider_targets,
            &model_candidates,
        ) {
            bridged_media_kinds.insert(
                media_understanding_resource_kind(capability)
                    .expect("closed media-understanding capability must map to a resource"),
            );
        }
    }
    Ok(bridged_media_kinds)
}

/// Reports whether one exact provider-model-account selection is still executable.
/// 报告一个精确的供应商、模型、账号选择是否仍可执行。
///
/// # Parameters
/// 参数
/// `configuration` owns model access and connections; `catalogs` owns process-local models; `selection`
/// identifies the exact persisted choice.
/// `configuration` 持有模型访问与连接；`catalogs` 持有进程内模型；`selection` 标识精确持久化选择。
///
/// # Returns
/// 返回值
/// `true` only when the exact connection, provider, scoped enabled state, and text conversation
/// model remain available; runtime snapshot failures are returned explicitly.
/// 仅当精确连接、供应商、作用域启用状态及文本会话模型仍可用时返回 `true`；运行时快照失败会显式返回。
fn model_selection_is_available(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    selection: &ActiveAiModelSelection,
) -> Result<bool, String> {
    let Some(connection) = configuration
        .connection_graph
        .connections
        .get(selection.connection_id.as_str())
    else {
        return Ok(false);
    };
    if connection.provider_id != selection.provider_id
        || configuration.connection_is_pending_validation(selection.connection_id.as_str())
        || !configuration.connection_model_is_enabled(
            selection.connection_id.as_str(),
            selection.model_id.as_str(),
        )
    {
        return Ok(false);
    }
    if !runtime.model_is_enabled_for_connection(
        configuration,
        selection.connection_id.as_str(),
        selection.model_id.as_str(),
    )? {
        return Ok(false);
    }
    Ok(catalogs
        .get(selection.connection_id.as_str())
        .is_some_and(|catalog| {
            catalog.models.iter().any(|model| {
                model.id == selection.model_id
                    && vulcan_provider_adapters::model_supports_text_conversation(
                        &model.capabilities,
                    )
            })
        }))
}

/// Resolves the persisted active model or the first stable enabled model as a fallback.
/// 解析持久化当前模型；若其失效，则回退到稳定排序的首个启用模型。
#[cfg(test)]
fn preferred_fallback_model_selection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Option<ActiveAiModelSelection>, String> {
    if let Some(selection) = configuration.active.as_ref()
        && model_selection_is_available(runtime, configuration, catalogs, selection)?
    {
        return Ok(Some(selection.clone()));
    }
    first_enabled_model_selection(runtime, configuration, catalogs)
}

/// Replaces an invalid persisted global selection with the default or first available model.
/// 将失效的持久化全局模型选择替换为默认模型或首个可用模型。
fn ensure_active_model_selection(
    runtime: &PublicAiRuntime,
    configuration: &mut PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<(), String> {
    // activeSelection is cloned so the invalid exact account can be replaced without extending
    // an immutable borrow of the configuration.
    // activeSelection 通过克隆避免在替换无效精确账号时延长配置的不可变借用。
    let Some(active_selection) = configuration.active.clone() else {
        return Ok(());
    };
    if model_selection_is_available(runtime, configuration, catalogs, &active_selection)? {
        return Ok(());
    }
    // sameModelSelection preserves the user's model choice when another credential account still
    // exposes it; only a provider/model disappearance falls back to the first enabled model.
    // sameModelSelection 在其他凭据账号仍公开该模型时保留用户模型选择；只有供应商/模型消失时
    // 才回退到首个启用模型。
    let same_model_selection = preferred_available_connection_for_model(
        runtime,
        configuration,
        catalogs,
        active_selection.provider_id.as_str(),
        active_selection.model_id.as_str(),
    )?;
    configuration.active = match same_model_selection {
        Some(selection) => Some(selection),
        None => first_enabled_model_selection(runtime, configuration, catalogs)?,
    };
    Ok(())
}

/// Resolves the preferred available credential for one existing provider/model choice.
/// 为一个既有供应商/模型选择解析首选的可用凭据。
///
/// # Parameters
/// 参数
/// `runtime` validates provider-level access, `configuration` owns account preferences,
/// `catalogs` is the effective account inventory, and the identifiers name the model.
/// `runtime` 校验供应商级访问，`configuration` 持有账号偏好，`catalogs` 是有效账号目录，
/// 两个标识用于指定模型。
///
/// # Returns
/// 返回值
/// The exact preferred available selection, or `None` when no account exposes the model.
/// 返回精确的首选可用选择；没有账号公开该模型时返回 `None`。
fn preferred_available_connection_for_model(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<ActiveAiModelSelection>, String> {
    // eligibleCatalogs excludes credentials that cannot participate in normal conversation routing.
    // eligibleCatalogs 排除不能参与常规会话路由的凭据。
    let mut eligible_catalogs = catalogs.clone();
    eligible_catalogs.retain(|connection_id, _| {
        !configuration.connection_is_pending_validation(connection_id.as_str())
    });
    let eligible_catalogs =
        vulcan_provider_adapters::RuntimeConnectionCatalogs::from_snapshot(eligible_catalogs);
    // modelExists proves resolveConnection has a valid deterministic candidate rather than
    // converting its no-candidate error into control flow.
    // modelExists 先证明 resolveConnection 存在确定候选，避免把无候选错误当作控制流吞掉。
    let model_exists = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .any(|connection| {
            eligible_catalogs
                .get(connection.id.as_str())
                .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id))
        });
    if !model_exists {
        return Ok(None);
    }
    let connection = configuration.connection_graph.resolve_connection(
        provider_id,
        model_id,
        &eligible_catalogs,
    )?;
    let selection = ActiveAiModelSelection {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        connection_id: connection.id.clone(),
    };
    model_selection_is_available(runtime, configuration, catalogs, &selection)
        .map(|available| available.then_some(selection))
}

/// Preserves every explicitly selected session model across catalog-only reconciliation.
/// 在仅目录协调期间保留每个显式选择的会话模型。
///
/// # Parameters
/// 参数
/// `workspace` owns active and archived session runtime selections; the remaining arguments identify
/// the catalog generation that must not rewrite those user-owned tuples.
/// `workspace` 持有活动及归档会话运行态选择；其余参数标识不得改写这些用户自有三元组的目录代际。
///
/// # Returns
/// 返回值
/// An unchanged detached workspace candidate and an empty changed-session set.
/// 返回一份未改写的独立工作区候选与空的会话变更集合。
fn plan_reconciled_session_model_selections(
    workspace: vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    _runtime: &PublicAiRuntime,
    _configuration: &PersistedAiConfiguration,
    _catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<
    (
        vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
        BTreeSet<crate::SessionId>,
    ),
    String,
> {
    // Catalog refresh is observational for established sessions. An unavailable exact tuple is
    // hidden by projection and requires explicit reselection; it is never rewritten to another
    // provider, account, or model.
    // 对已有会话而言，目录刷新只负责观测。不可用的精确三元组由投影隐藏并要求显式重选，
    // 绝不会被改写成其他供应商、账号或模型。
    Ok((workspace, BTreeSet::new()))
}

/// Applies observational catalog reconciliation without rewriting explicit session selections.
/// 应用仅观测的目录协调，不改写显式会话选择。
///
/// # Parameters
/// 参数
/// `workspace` owns active and archived session runtime selections; `configuration` and `catalogs`
/// identify the observed generation whose availability is handled by projection.
/// `workspace` 持有活动及归档会话运行态选择；`configuration` 与 `catalogs` 标识由投影处理
/// 可用性的观测代际。
///
/// # Returns
/// 返回值
/// An empty changed-session set while preserving every exact tuple. Any planning or lease-lock
/// failure leaves the caller's workspace unchanged.
/// 返回空的会话变更集合并保留每个精确三元组；规划或租约锁失败时，调用方工作区保持不变。
#[cfg(test)]
pub(crate) fn reconcile_missing_session_model_selections(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<BTreeSet<crate::SessionId>, String> {
    // plannedWorkspace is detached until every fallible lease operation succeeds.
    // plannedWorkspace 在全部可失败租约操作成功前保持独立。
    let (planned_workspace, changed_session_ids) = plan_reconciled_session_model_selections(
        workspace.clone(),
        runtime,
        configuration,
        catalogs,
    )?;
    // leaseScopeIds materializes stable strings for one exact absence/exclusion transaction.
    // leaseScopeIds 为一次精确的缺失校验/排除事务物化稳定字符串。
    let lease_scope_ids = changed_session_ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    // leaseGuard prevents a concurrent turn from installing a route between planning and commit.
    // leaseGuard 防止并发回合在规划与提交之间安装路由。
    let _lease_guard = runtime.acquire_session_selection_lease_absence_guard(&lease_scope_ids)?;
    *workspace = planned_workspace;
    Ok(changed_session_ids)
}

/// Selects the first stable enabled provider-model-account tuple from runtime catalogs.
/// 从运行时目录选择首个稳定的已启用供应商模型账号组合。
///
/// # Parameters
/// 参数
/// `configuration` owns model access and connections; `catalogs` owns process-local models.
/// `configuration` 持有模型访问与连接；`catalogs` 持有进程内模型。
///
/// # Returns
/// 返回值
/// The lexicographically first enabled tuple, `None` when no model remains, or an explicit runtime
/// snapshot error.
/// 返回按字典序排列的首个启用组合、没有剩余模型时返回 `None`，或显式运行时快照错误。
fn first_enabled_model_selection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Option<ActiveAiModelSelection>, String> {
    let mut candidates = Vec::new();
    for connection in configuration.connection_graph.connections.values() {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            continue;
        }
        let Some(catalog) = catalogs.get(connection.id.as_str()) else {
            continue;
        };
        for model in &catalog.models {
            if runtime.model_is_enabled_for_connection(
                configuration,
                connection.id.as_str(),
                model.id.as_str(),
            )? && configuration
                .connection_model_is_enabled(connection.id.as_str(), model.id.as_str())
                && vulcan_provider_adapters::model_supports_text_conversation(&model.capabilities)
            {
                candidates.push(ActiveAiModelSelection {
                    provider_id: connection.provider_id.clone(),
                    model_id: model.id.clone(),
                    connection_id: connection.id.clone(),
                });
            }
        }
    }
    candidates.sort_by(|left, right| {
        left.provider_id
            .cmp(&right.provider_id)
            .then_with(|| left.model_id.cmp(&right.model_id))
            .then_with(|| left.connection_id.cmp(&right.connection_id))
    });
    Ok(candidates.into_iter().next())
}

/// Selects an enabled replacement while preferring the same provider and model on another account.
/// 选择一个已启用替代项，并优先保留相同供应商与模型的其他账号。
///
/// # Parameters
/// 参数
/// `configuration` and `catalogs` describe the candidate state; `provider_id` and `model_id`
/// identify the user's current model intent.
/// `configuration` 与 `catalogs` 描述候选状态；`provider_id` 与 `model_id` 标识用户当前模型意图。
///
/// # Returns
/// 返回值
/// The first same-model account, otherwise the stable global fallback; scoped runtime failures are
/// returned explicitly.
/// 返回首个同模型账号，否则返回稳定的全局降级项；作用域运行时失败会显式返回。
fn preferred_replacement_model_selection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<ActiveAiModelSelection>, String> {
    let mut same_model_candidates = Vec::new();
    for connection in configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
    {
        if configuration.connection_is_pending_validation(connection.id.as_str())
            || !runtime.model_is_enabled_for_connection(
                configuration,
                connection.id.as_str(),
                model_id,
            )?
            || !configuration.connection_model_is_enabled(connection.id.as_str(), model_id)
        {
            continue;
        }
        let supports_model = catalogs.get(connection.id.as_str()).is_some_and(|catalog| {
            catalog.models.iter().any(|model| {
                model.id == model_id
                    && vulcan_provider_adapters::model_supports_text_conversation(
                        &model.capabilities,
                    )
            })
        });
        if supports_model {
            same_model_candidates.push(ActiveAiModelSelection {
                provider_id: provider_id.to_string(),
                model_id: model_id.to_string(),
                connection_id: connection.id.clone(),
            });
        }
    }
    same_model_candidates.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));
    if let Some(selection) = same_model_candidates.into_iter().next() {
        return Ok(Some(selection));
    }
    first_enabled_model_selection(runtime, configuration, catalogs)
}

/// Reconciles loaded session selections after one credential model becomes unavailable.
/// 在一个凭据模型变为不可用后协调已加载会话选择。
///
/// # Parameters
/// 参数
/// `workspace` is the detached loaded-session candidate; `connection_id` selects the affected account;
/// `model_id` narrows one model when present; `fallback` is the resolved replacement.
/// `workspace` 是独立的已加载会话候选；`connection_id` 选择受影响账号；存在 `model_id` 时收窄到一个模型；
/// `fallback` 是已解析替代项。
///
/// # Returns
/// 返回值
/// The complete active and archived session set whose exact connection route was revoked.
/// 返回精确连接路由已被撤销的完整活动与归档会话集合。
fn reconcile_disabled_connection_sessions(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    connection_id: &str,
    model_id: Option<&str>,
    fallback: Option<&ActiveAiModelSelection>,
) -> BTreeSet<crate::SessionId> {
    reconcile_explicitly_revoked_session_model_selections(
        workspace,
        fallback,
        |_session_id, runtime_state| {
            let exact_connection =
                runtime_state.active_connection_id.as_deref() == Some(connection_id);
            let exact_model = model_id
                .is_none_or(|model_id| runtime_state.active_model_id.as_deref() == Some(model_id));
            exact_connection && exact_model
        },
    )
}

/// Reconciles every explicitly revoked active or archived session to one deterministic fallback.
/// 将每个被显式撤销的活动或归档会话协调到一个确定性回退选择。
///
/// # Parameters
/// 参数
/// `workspace` is the detached candidate; `fallback` is the exact replacement without protocol
/// probing; `matches` identifies the complete set whose current route authority was revoked.
/// `workspace` 是独立候选；`fallback` 是不进行协议探测的精确替代选择；`matches` 标识当前
/// 路由权威已被撤销的完整会话集合。
///
/// # Returns
/// 返回值
/// The exact active and archived session identifiers whose persisted route and lease identity were
/// replaced.
/// 返回持久路由与租约身份已被替换的精确活动及归档会话标识集合。
fn reconcile_explicitly_revoked_session_model_selections<Matches>(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    fallback: Option<&ActiveAiModelSelection>,
    matches: Matches,
) -> BTreeSet<crate::SessionId>
where
    Matches: Fn(crate::SessionId, &vulcan_workbench_model::SessionRuntimeStateEntry) -> bool,
{
    // changedSessionIds records the complete lease-revocation and persistence scope.
    // changedSessionIds 记录完整租约撤销与持久化范围。
    let mut changed_session_ids = BTreeSet::new();
    // reconcileStates applies the same closed transition to active and archived stores.
    // reconcileStates 对活动与归档存储应用相同的封闭转换。
    let mut reconcile_states = |states: &mut BTreeMap<
        crate::SessionId,
        vulcan_workbench_model::SessionRuntimeStateEntry,
    >| {
        for (session_id, runtime_state) in states.iter_mut() {
            if !matches(*session_id, runtime_state) {
                continue;
            }
            runtime_state.active_provider_id =
                fallback.map(|selection| selection.provider_id.clone());
            runtime_state.active_connection_id =
                fallback.map(|selection| selection.connection_id.clone());
            runtime_state.active_model_id = fallback.map(|selection| selection.model_id.clone());
            runtime_state.provider_route_lease = None;
            runtime_state.updated_at = crate::current_unix_millis_text();
            changed_session_ids.insert(*session_id);
        }
    };
    reconcile_states(&mut workspace.session_runtime_state);
    reconcile_states(&mut workspace.archived_session_runtime_state);
    changed_session_ids
}

/// Clones the shared runtime and authoritative public AI configuration.
/// 克隆共享运行时与权威公共 AI 配置。
pub(crate) fn public_ai_runtime_and_configuration(
    state: &SharedServiceBridgeState,
) -> Result<(Arc<PublicAiRuntime>, PersistedAiConfiguration), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    let agent_runtime = guard
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("Service is still starting; AI runtime is not ready."))?;
    let runtime = Arc::clone(&agent_runtime.public_ai_runtime);
    let configuration = runtime.configuration_snapshot()?;
    Ok((runtime, configuration))
}

/// Returns every currently available public AI provider/model selector.
/// 返回当前全部可用的公共 AI 供应商/模型选择器。
///
/// # Parameters
/// 参数
///
/// `state` owns the exact process-local catalogs and persisted connection graph.
/// `state` 持有精确的进程内目录与持久化连接图。
///
/// # Returns
/// 返回值
///
/// A stable deduplicated selector set derived without reading legacy provider catalogs.
/// 返回不读取旧供应商目录而派生的稳定去重选择器集合。
pub(crate) fn public_ai_available_model_selectors(
    state: &SharedServiceBridgeState,
) -> Result<std::collections::BTreeSet<String>, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)?;
    runtime.available_model_selectors(&configuration)
}

/// Resolves one account's closed model-directory kind from its active Manifest routes.
/// 从账号的活动 Manifest 路由解析封闭模型目录类型。
///
/// # Parameters
/// 参数
/// `providers` is the effective registry and `connection` is the exact persisted account.
/// `providers` 是有效注册表，`connection` 是精确持久化账号。
///
/// # Returns
/// 返回值
/// A browser-safe aggregate kind. Any remote route makes the account synchronizable; otherwise
/// fixed identity outranks manual identity. Entries without a verified route are unavailable.
/// 返回浏览器安全的聚合类型。任一远端路由都会使账号可同步；否则固定身份优先于手工身份。
/// 没有已验证路由的入口返回不可用。
fn connection_model_directory_kind(
    providers: &ProviderRegistry,
    connection: &CredentialConnection,
) -> PublicAiModelDirectoryKind {
    let Some(provider) = providers.provider(connection.provider_id.as_str()) else {
        return PublicAiModelDirectoryKind::Unavailable;
    };
    let Some(entry) = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == connection.service_entry_id)
    else {
        return PublicAiModelDirectoryKind::Unavailable;
    };
    // hasDynamic, hasFixed, and hasManual summarize route-owned discovery contracts.
    // hasDynamic、hasFixed 与 hasManual 汇总路由持有的发现契约。
    let mut has_dynamic = false;
    let mut has_fixed = false;
    let mut has_manual = false;
    for route in entry.protocol_routes.iter().filter(|route| route.verified) {
        match route.resolved_model_discovery(entry.model_discovery) {
            ModelDiscoveryKind::Manual => has_manual = true,
            ModelDiscoveryKind::ManifestStatic => has_fixed = true,
            ModelDiscoveryKind::OpenRouter
            | ModelDiscoveryKind::OpenAi
            | ModelDiscoveryKind::OpenAiCodex
            | ModelDiscoveryKind::Anthropic
            | ModelDiscoveryKind::AnthropicCompatible
            | ModelDiscoveryKind::Gemini
            | ModelDiscoveryKind::GoogleCodeAssist => has_dynamic = true,
            ModelDiscoveryKind::ProviderToolsOnly => {}
        }
    }
    if has_dynamic {
        PublicAiModelDirectoryKind::Dynamic
    } else if has_fixed {
        PublicAiModelDirectoryKind::Fixed
    } else if has_manual {
        PublicAiModelDirectoryKind::Manual
    } else {
        PublicAiModelDirectoryKind::Unavailable
    }
}

/// Projects every credential account's sparse model access policy for the browser.
/// 为浏览器投影每个凭据账号的稀疏模型访问策略。
///
/// # Parameters
/// 参数
/// `providers` resolves exact directory semantics and `configuration` owns user decisions.
/// `providers` 解析精确目录语义，`configuration` 持有用户决定。
///
/// # Returns
/// 返回值
/// Stable policy records in connection order.
/// 返回按连接顺序排列的稳定策略记录。
fn build_connection_model_access_policy_records(
    providers: &ProviderRegistry,
    configuration: &PersistedAiConfiguration,
) -> Vec<PublicAiConnectionModelAccessPolicyRecord> {
    configuration
        .connection_graph
        .connections
        .values()
        .map(|connection| {
            let policy: ConnectionModelAccessPolicy =
                configuration.connection_model_access_policy(connection.id.as_str());
            PublicAiConnectionModelAccessPolicyRecord {
                connection_id: connection.id.clone(),
                default_enabled: policy.default_enabled,
                model_enabled_overrides: policy.model_enabled_overrides,
                directory_kind: connection_model_directory_kind(providers, connection),
            }
        })
        .collect()
}

/// Returns the only protocol authority accepted for one frozen catalog model.
/// 返回一个冻结目录模型唯一可接受的协议权威。
///
/// # Parameters
/// 参数
/// `model` owns the immutable model binding.
/// `model` 持有不可变模型绑定。
///
/// # Returns
/// 返回值
/// The frozen model protocol, or `None` when the model is not executable.
/// 返回冻结的模型协议；模型不可执行时返回 `None`。
fn effective_catalog_model_protocol_id(
    model: &vulcan_provider_adapters::ConnectionModel,
) -> Option<&str> {
    model
        .resolved_protocol_binding
        .as_ref()
        .map(|binding| binding.protocol_profile_id.as_str())
}

/// Reports whether a catalog model is declared for text conversation in any authoritative view.
/// 报告一个目录模型是否在任一权威视图中声明文本会话能力。
///
/// # Parameters
/// 参数
/// `model` owns the effective projection plus every protocol-specific capability variant.
/// `model` 持有生效投影与全部协议专属能力变体。
///
/// # Returns
/// 返回值
/// `true` when the effective projection or any retained variant declares conversation. Inspecting
/// variants keeps a blocked conversation model manageable after its effective projection is cleared.
/// 当生效投影或任一保留变体声明会话时返回 `true`。检查变体可使生效投影被清空后的受阻会话模型
/// 仍可管理。
fn catalog_model_supports_text_conversation(
    model: &vulcan_provider_adapters::ConnectionModel,
) -> bool {
    vulcan_provider_adapters::model_supports_text_conversation(&model.capabilities)
        || model.protocol_variants.values().any(|variant| {
            vulcan_provider_adapters::model_supports_text_conversation(&variant.capabilities)
        })
}

/// Returns the stable browser-facing identifier for one frozen model-binding source.
/// 返回一个冻结模型绑定来源的稳定浏览器标识。
///
/// # Parameters
/// 参数
/// `source` is the exact backend binding-source enum retained by the effective catalog.
/// `source` 是生效目录保留的精确后端绑定来源枚举。
///
/// # Returns
/// 返回值
/// A closed non-secret identifier suitable for settings UI and diagnostics.
/// 返回适用于设置界面与诊断的封闭非秘密标识。
fn public_ai_model_binding_source_id(
    source: vulcan_provider_adapters::ModelProtocolBindingSource,
) -> &'static str {
    match source {
        vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest => "system_manifest",
        vulcan_provider_adapters::ModelProtocolBindingSource::CustomUser => "custom_user",
        vulcan_provider_adapters::ModelProtocolBindingSource::CustomAutoDefault => {
            "custom_auto_default"
        }
    }
}

/// Explains why one custom-provider model has no frozen protocol binding.
/// 解释一个自定义供应商模型为何没有冻结协议绑定。
///
/// # Parameters
/// 参数
/// The configuration owns the exact preference, providers owns activated protocol routes,
/// the connection identifies the service entry, and the model owns observed protocol variants.
/// configuration 持有精确偏好，providers 持有已激活协议路由，connection 标识服务入口，
/// model 持有已观测协议变体。
///
/// # Returns
/// 返回值
/// A stable backend-owned diagnostic derived from the authoritative custom binding resolver.
/// 返回由权威自定义绑定解析器派生的稳定后端诊断。
fn custom_model_route_unavailable_reason(
    configuration: &PersistedAiConfiguration,
    providers: &ProviderRegistry,
    connection: &CredentialConnection,
    model: &vulcan_provider_adapters::ConnectionModel,
    manifest_revision: u64,
) -> String {
    let Some(provider) = providers.provider(connection.provider_id.as_str()) else {
        return format!(
            "Provider '{}' is absent from the activated provider generation.",
            connection.provider_id
        );
    };
    let Some(entry) = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == connection.service_entry_id)
    else {
        return format!(
            "Service entry '{}' is absent from the activated provider '{}'.",
            connection.service_entry_id, connection.provider_id
        );
    };
    let enabled_protocols = entry
        .protocol_routes
        .iter()
        .map(|route| route.protocol_profile_id.as_str())
        .collect::<Vec<_>>();
    let available_protocols = model
        .protocol_variants
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let preference_key = PersistedAiConfiguration::provider_model_key(
        connection.provider_id.as_str(),
        model.id.as_str(),
    );
    let explicit_protocol = configuration
        .custom_model_protocol_preferences
        .get(preference_key.as_str())
        .and_then(|preference| (*preference).explicit_protocol())
        .map(|protocol| protocol.profile_id());
    match vulcan_provider_adapters::resolve_custom_model_protocol_binding(
        enabled_protocols.as_slice(),
        available_protocols.as_slice(),
        model.id.as_str(),
        manifest_revision,
        explicit_protocol,
    ) {
        Ok(binding) => format!(
            "Model '{}' resolves to protocol '{}', but the activated support catalog has no frozen binding; reactivate the provider catalog.",
            model.id, binding.protocol_profile_id
        ),
        Err(error) => error.to_string(),
    }
}

/// Builds backend-owned protocol status records for every conversation-capable catalog model.
/// 为目录中每个具备会话能力的模型构建由后端持有的协议状态记录。
///
/// # Parameters
/// 参数
/// The configuration owns exact connection identities and preferences; providers, catalogs,
/// and manifest revisions are frozen at the same runtime publication boundary.
/// configuration 持有精确连接身份与偏好；providers、catalogs 与 manifest revisions
/// 冻结在同一运行时发布边界。
///
/// # Returns
/// 返回值
/// Stable per-connection model statuses, including blocked models and their backend diagnostics.
/// 返回稳定的逐连接模型状态，包含被阻止模型及其后端诊断。
fn build_public_ai_model_route_records(
    configuration: &PersistedAiConfiguration,
    providers: &ProviderRegistry,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    manifest_revisions: &BTreeMap<String, u64>,
) -> Vec<PublicAiModelRouteRecord> {
    let mut records = Vec::new();
    for (connection_id, catalog) in catalogs {
        let Some(connection) = configuration
            .connection_graph
            .connections
            .get(connection_id.as_str())
        else {
            continue;
        };
        for model in &catalog.models {
            // Conversation route records are intentionally narrower than the provider's full
            // system-model inventory. Generation-only system models belong to provider extensions;
            // custom providers remain conversation-only and keep their management diagnostics.
            // 会话路由记录有意窄于系统供应商完整模型清单；纯生成系统模型属于 Provider 扩展；
            // 自定义供应商仍仅支持会话并保留其管理诊断。
            if !configuration
                .custom_providers
                .contains_key(connection.provider_id.as_str())
                && !catalog_model_supports_text_conversation(model)
            {
                continue;
            }
            // protocolId consumes only the frozen model-level binding.
            // protocolId 只消费冻结的模型级绑定。
            let protocol_id = effective_catalog_model_protocol_id(model).map(str::to_string);
            // binding is the immutable backend authority for source, rule, and revision facts.
            // binding 是来源、规则与修订事实的不可变后端权威。
            let binding = model.resolved_protocol_binding.as_ref();
            // bindingSource identifies the exact model-level binding authority.
            // bindingSource 单独标识当前模型级绑定来源。
            let binding_source = binding
                .map(|binding| public_ai_model_binding_source_id(binding.source))
                .map(str::to_string);
            let manifest_revision = binding
                .map(|binding| binding.manifest_revision)
                .or_else(|| {
                    manifest_revisions
                        .get(connection.provider_id.as_str())
                        .copied()
                })
                .unwrap_or_default();
            // unavailableReason is computed from the same captured provider/catalog generation.
            // unavailableReason 从同一捕获供应商/目录代际计算。
            let unavailable_reason = match protocol_id.as_deref() {
                Some(bound_protocol)
                    if binding.is_some()
                        && !model.protocol_variants.contains_key(bound_protocol) =>
                {
                    Some(format!(
                        "Model '{}' has no capability variant for its bound protocol '{}'; execution is blocked.",
                        model.id, bound_protocol
                    ))
                }
                Some(_) => None,
                None if configuration
                    .custom_providers
                    .contains_key(connection.provider_id.as_str()) =>
                {
                    Some(custom_model_route_unavailable_reason(
                        configuration,
                        providers,
                        connection,
                        model,
                        manifest_revision,
                    ))
                }
                None => Some(format!(
                    "Model '{}' has no protocol binding frozen in the activated provider generation.",
                    model.id
                )),
            };
            let executable = protocol_id.is_some() && unavailable_reason.is_none();
            records.push(PublicAiModelRouteRecord {
                provider_id: connection.provider_id.clone(),
                connection_id: connection.id.clone(),
                service_entry_id: connection.service_entry_id.clone(),
                model_id: model.id.clone(),
                protocol_id,
                available_protocols: model.protocol_variants.keys().cloned().collect(),
                binding_source,
                binding_rule_id: binding
                    .map(|binding| binding.rule_id.clone())
                    .filter(|rule_id| !rule_id.is_empty()),
                manifest_revision,
                executable,
                unavailable_reason,
            });
        }
    }
    records.sort_by(|left, right| {
        left.provider_id
            .cmp(&right.provider_id)
            .then_with(|| left.connection_id.cmp(&right.connection_id))
            .then_with(|| left.model_id.cmp(&right.model_id))
    });
    records
}

/// Builds a redacted browser snapshot from persisted and process-local state.
/// 从持久化状态与进程内状态构建脱敏浏览器快照。
fn build_public_ai_settings_snapshot(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Result<PublicAiSettingsSnapshot, String> {
    // persistedConfiguration retains the repair authority and bounded failure diagnostics.
    // persistedConfiguration 保留修复权威与有界失败诊断。
    let persisted_configuration = configuration;
    // Build the browser-facing catalog from immutable discovery data plus persisted overlays.
    // 从不可变发现数据与持久化覆盖构建面向浏览器的有效目录。
    // This keeps capability edits visible after reloads and prevents a stale process snapshot
    // from making the conversation selector report reasoning as unsupported.
    // 这样可确保刷新后能力编辑仍然生效，避免旧进程快照把思考能力误报为不支持。
    // settings_catalog_snapshot keeps every catalog, template, target, and revision on one generation.
    // settings_catalog_snapshot 让全部目录、模板、目标与修订号处于同一代际。
    let PublicAiRuntimeSettingsCatalogSnapshot {
        runtime_configuration,
        providers,
        catalogs,
        model_support_catalogs,
        source_catalogs,
        manifest_revisions,
        provider_tool_template_models,
        model_default_enabled_by_key,
        available_provider_tool_targets,
    } = runtime.settings_catalog_snapshot(persisted_configuration)?;
    // configuration is the executable projection captured with the catalog generation above.
    // configuration 是与上述目录代际共同捕获的可执行投影。
    let configuration = &runtime_configuration;
    let mut model_override_records_by_key = BTreeMap::new();
    let mut model_access_records_by_key = BTreeMap::new();
    let mut custom_model_template_decision_records_by_key = BTreeMap::new();
    // Protocol records are projected directly from persistence so inactive settings remain visible
    // for protocol-switch warnings instead of becoming inaccessible composite-key state.
    // 协议记录直接从持久化状态投影，使非活动设置仍可用于协议切换警告，
    // 而不会变成无法访问的组合键状态。
    for (key, overrides) in &configuration.model_overrides {
        let Some((provider_id, model_id, protocol_id)) =
            PersistedAiConfiguration::provider_model_protocol_key_parts(key.as_str())
        else {
            continue;
        };
        model_override_records_by_key.insert(
            key.clone(),
            PublicAiModelOverrideRecord {
                provider_id: provider_id.to_string(),
                model_id: model_id.to_string(),
                protocol_id: protocol_id.to_string(),
                overrides: overrides.clone(),
            },
        );
    }
    // Capability records are projected directly from persistence so provider-disabled, account-hidden,
    // and credential-free custom models retain an editable and restorable settings surface.
    // 能力记录直接从持久化状态投影，使供应商关闭、账号隐藏以及无凭据自定义模型
    // 仍保留可编辑、可还原的设置入口。
    let model_capability_override_records = configuration
        .model_capability_overrides
        .iter()
        .filter_map(|(key, overrides)| {
            let (provider_id, model_id) =
                PersistedAiConfiguration::provider_model_key_parts(key.as_str())?;
            Some(PublicAiModelCapabilityOverrideRecord {
                provider_id: provider_id.to_string(),
                model_id: model_id.to_string(),
                overrides: overrides.clone(),
            })
        })
        .collect::<Vec<_>>();
    // Provider access records must use support catalogs so an account-wide closure does not erase
    // the provider-level state needed to reject or hide a disabled model.
    // 供应商访问记录必须使用支持目录，避免账号范围关闭后丢失拒绝或隐藏供应商关闭模型所需的状态。
    for (connection_id, catalog) in &model_support_catalogs {
        let Some(connection) = configuration
            .connection_graph
            .connections
            .get(connection_id.as_str())
        else {
            continue;
        };
        for model in &catalog.models {
            // modelAccessRecords owns only conversation publication. Provider extension choices
            // are persisted independently in providerExtensionTargetSets. Custom providers remain
            // conversation-only and therefore retain their model access surface.
            // modelAccessRecords 只持有会话发布状态；Provider 扩展选择独立持久化在
            // providerExtensionTargetSets 中；自定义供应商仍仅支持会话，因此保留模型访问入口。
            if !configuration
                .custom_providers
                .contains_key(connection.provider_id.as_str())
                && !catalog_model_supports_text_conversation(model)
            {
                continue;
            }
            let access_key = PersistedAiConfiguration::provider_model_key(
                connection.provider_id.as_str(),
                model.id.as_str(),
            );
            // default_enabled is frozen with the same settings catalog generation as this model.
            // default_enabled 与该模型的设置目录代际共同冻结。
            let default_enabled = model_default_enabled_by_key
                .get(access_key.as_str())
                .copied()
                .ok_or_else(|| {
                    format!(
                        "The settings generation has no model access default for provider `{}` model `{}`.",
                        connection.provider_id, model.id
                    )
                })?;
            model_access_records_by_key
                .entry(access_key.clone())
                .or_insert(PublicAiModelAccessRecord {
                    provider_id: connection.provider_id.clone(),
                    model_id: model.id.clone(),
                    enabled: configuration.model_is_enabled_with_default(
                        connection.provider_id.as_str(),
                        model.id.as_str(),
                        default_enabled,
                    ),
                    default_enabled,
                    customized: configuration
                        .model_enabled_overrides
                        .contains_key(access_key.as_str()),
                });
        }
    }
    for (connection_id, catalog) in &catalogs {
        let Some(connection) = configuration
            .connection_graph
            .connections
            .get(connection_id.as_str())
        else {
            continue;
        };
        for model in &catalog.models {
            let access_key = PersistedAiConfiguration::provider_model_key(
                connection.provider_id.as_str(),
                model.id.as_str(),
            );
            if let Some(decision) = configuration
                .custom_model_template_decision(connection.provider_id.as_str(), model.id.as_str())
                .cloned()
            {
                custom_model_template_decision_records_by_key
                    .entry(access_key.clone())
                    .or_insert_with(|| PublicAiCustomModelTemplateDecisionRecord {
                        provider_id: connection.provider_id.clone(),
                        model_id: model.id.clone(),
                        decision,
                    });
            }
        }
    }
    // model_override_records projects protocol-scoped execution differences for the browser.
    // model_override_records 向浏览器投影协议作用域执行差异。
    let model_override_records = model_override_records_by_key.into_values().collect();
    // model_access_records contains only conversation publication choices.
    // model_access_records 仅包含会话发布选择。
    let model_access_records = model_access_records_by_key.into_values().collect();
    for (provider_id, models) in &configuration.custom_models {
        for model_id in models.keys() {
            let key = PersistedAiConfiguration::provider_model_key(provider_id, model_id);
            if let Some(decision) = configuration
                .custom_model_template_decision(provider_id, model_id)
                .cloned()
            {
                custom_model_template_decision_records_by_key
                    .entry(key)
                    .or_insert_with(|| PublicAiCustomModelTemplateDecisionRecord {
                        provider_id: provider_id.clone(),
                        model_id: model_id.clone(),
                        decision,
                    });
            }
        }
    }
    let custom_model_template_decision_records = custom_model_template_decision_records_by_key
        .into_values()
        .collect();
    let custom_model_connection_availability_records = configuration
        .connection_graph
        .connections
        .values()
        .flat_map(|connection| {
            configuration
                .custom_models
                .get(connection.provider_id.as_str())
                .into_iter()
                .flat_map(move |models| {
                    models.keys().filter_map(move |model_id| {
                        configuration
                            .custom_model_connection_enabled_override(
                                connection.id.as_str(),
                                model_id.as_str(),
                            )
                            .map(|enabled| PublicAiCustomModelConnectionAvailabilityRecord {
                                connection_id: connection.id.clone(),
                                model_id: model_id.clone(),
                                enabled,
                            })
                    })
                })
        })
        .collect();
    let provider_tool_selections = filter_current_provider_tool_selections(
        &available_provider_tool_targets,
        &configuration.provider_tool_selections,
    );
    // multimodalUnderstandingCandidateModels exposes the shared provider-model switch without
    // requiring the browser to decode persisted composite keys.
    // multimodalUnderstandingCandidateModels 公开共享供应商模型开关，浏览器无需解析持久化组合键。
    let multimodal_understanding_candidate_models = configuration
        .multimodal_understanding_candidate_models
        .iter()
        .filter_map(|key| {
            let (provider_id, model_id) =
                PersistedAiConfiguration::provider_model_key_parts(key.as_str())?;
            Some(PublicAiMultimodalUnderstandingCandidateRecord {
                provider_id: provider_id.to_string(),
                model_id: model_id.to_string(),
            })
        })
        .collect();
    // mediaUnderstandingModelCandidates remains independent from conversation publication and
    // the user admission switch so settings can both enable and select exact account routes.
    // mediaUnderstandingModelCandidates 与会话发布和用户准入开关相互独立，使设置可启用并选择精确账号路由。
    let media_understanding_model_candidates =
        build_media_understanding_model_candidates(configuration, &providers, &catalogs)?;
    let internal_inference_candidates =
        build_internal_inference_candidates(configuration, &providers, &catalogs)?;
    let inference_route_diagnostics =
        build_internal_inference_route_diagnostics(runtime, configuration);
    let connection_model_access_policy_records =
        build_connection_model_access_policy_records(&providers, configuration);
    // modelRouteRecords projects every access-independent support model, including blocked
    // bindings, so WebUI never needs a second protocol resolver or a guessed failure reason.
    // modelRouteRecords 投影每个独立于访问开关的支持模型（包括被阻止的绑定），使 WebUI
    // 永远不需要第二套协议解析器或猜测失败原因。
    let model_route_records = build_public_ai_model_route_records(
        configuration,
        &providers,
        &model_support_catalogs,
        &manifest_revisions,
    );
    // providers_with_model_customizations lets WebUI disclose cascade cleanup that is not fully countable in the browser snapshot.
    // providers_with_model_customizations 让 WebUI 提示浏览器快照无法完整计数的级联清理内容。
    let providers_with_model_customizations = configuration
        .custom_providers
        .keys()
        .filter(|provider_id| configuration.provider_has_model_customizations(provider_id))
        .cloned()
        .collect();
    // runtimeConnectionIds bounds every process-local observation to the same healthy connection
    // graph used by this snapshot.
    // runtimeConnectionIds 使每项进程内观测都限定在本快照使用的同一健康连接图内。
    let runtime_connection_ids = configuration
        .connection_graph
        .connections
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let usage_cache = runtime
        .usage_cache_snapshot()?
        .into_iter()
        .filter(|(connection_id, _)| runtime_connection_ids.contains(connection_id.as_str()))
        .collect();
    let catalog_statuses = runtime
        .catalog_status_snapshot()?
        .into_iter()
        .filter(|(connection_id, _)| runtime_connection_ids.contains(connection_id.as_str()))
        .collect();
    let catalog_errors = runtime
        .catalog_error_snapshot()?
        .into_iter()
        .filter(|(connection_id, _)| runtime_connection_ids.contains(connection_id.as_str()))
        .collect();
    Ok(PublicAiSettingsSnapshot {
        context_window_policy_overrides: configuration.context_window_policy_overrides,
        context_compaction_window_nodes: configuration.context_compaction_window_nodes.clone(),
        provider_context_window_policy_overrides: configuration
            .provider_context_window_policy_overrides
            .clone(),
        usage_refresh_interval_seconds: configuration.usage_refresh_interval_seconds,
        provider_priorities: configuration.provider_priority_overrides.clone(),
        usage_cache,
        providers: providers.providers().into_iter().cloned().collect(),
        connections: configuration
            .connection_graph
            .connections
            .values()
            .map(|connection| {
                PublicAiConnectionSnapshot::new(&providers, configuration, connection)
            })
            .collect::<Result<Vec<_>, _>>()?,
        provider_defaults: configuration
            .connection_graph
            .account_preferences
            .provider_defaults
            .clone(),
        active: configuration.active.clone(),
        catalogs,
        source_catalogs,
        model_support_catalogs,
        model_route_records,
        catalog_statuses,
        catalog_errors,
        model_overrides: configuration.model_overrides.clone(),
        model_override_records,
        model_access_records,
        provider_tool_template_models,
        custom_models: configuration.custom_models.clone(),
        custom_provider_manifest_failures: persisted_configuration
            .custom_provider_manifest_failures
            .clone(),
        custom_provider_manifest_failure_total_count: persisted_configuration
            .custom_provider_manifest_failure_total_count,
        custom_provider_manifest_failures_truncated: persisted_configuration
            .custom_provider_manifest_failures_truncated,
        custom_provider_model_defaults: configuration.custom_provider_model_defaults.clone(),
        custom_provider_system_model_defaults: CustomProviderModelDefaults::default(),
        providers_with_model_customizations,
        custom_model_protocol_preference_records: configuration
            .custom_model_protocol_preferences
            .iter()
            .filter_map(|(key, preference)| {
                let (provider_id, model_id) =
                    PersistedAiConfiguration::provider_model_key_parts(key.as_str())?;
                Some(PublicAiCustomModelProtocolPreferenceRecord {
                    provider_id: provider_id.to_string(),
                    model_id: model_id.to_string(),
                    preference: *preference,
                })
            })
            .collect(),
        provider_manifest_revisions: manifest_revisions,
        custom_model_template_decision_records,
        custom_model_connection_availability_records,
        connection_model_access_policy_records,
        model_capability_overrides: configuration.model_capability_overrides.clone(),
        provider_capability_overrides: configuration.provider_capability_overrides.clone(),
        model_capability_override_records,
        provider_tool_targets: available_provider_tool_targets
            .into_iter()
            .map(|available| PublicAiProviderToolTargetSnapshot {
                connection_id: available.connection_id,
                connection_display_name: available.connection_display_name,
                manifest_source_layer: available.manifest_source_layer,
                target: available.target,
            })
            .collect(),
        provider_tool_selections,
        multimodal_understanding_candidate_models,
        media_understanding_selections: configuration.media_understanding_selections.clone(),
        media_understanding_backend_options: configuration
            .media_understanding_backend_options
            .clone(),
        media_understanding_model_candidates,
        provider_tool_enabled_capabilities: configuration
            .provider_tool_enabled_capabilities
            .clone(),
        provider_extension_target_sets: configuration.provider_extension_target_sets.clone(),
        internal_inference_candidates,
        inference_profiles: configuration.inference_profiles.clone(),
        inference_route_diagnostics,
        internal_model_overrides: configuration.internal_model_overrides.clone(),
    })
}

/// Diagnoses every retained consumer route with the same strict resolver used for execution.
/// 使用与真实执行相同的严格解析器诊断每条保留的消费者路由。
///
/// # Parameters
/// 参数
/// `runtime` resolves current credentials, models, bindings, and usage scopes while
/// `configuration` owns the retained consumer routes.
/// `runtime` 解析当前凭据、模型、绑定与用途范围，`configuration` 持有保留的消费者路由。
///
/// # Returns
/// 返回值
/// Only failed routes, each paired with its exact profile slot and backend reason.
/// 仅返回失败路由，并携带其精确配置槽位与后端原因。
fn build_internal_inference_route_diagnostics(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Vec<PublicAiInferenceRouteDiagnostic> {
    let mut diagnostics = Vec::new();
    for (consumer_profile_id, profile) in &configuration.inference_profiles {
        for (purpose_id, route) in &profile.llm_routes {
            push_internal_inference_route_diagnostic(
                &mut diagnostics,
                runtime,
                configuration,
                consumer_profile_id,
                Some(purpose_id.as_str()),
                AiOperation::ConversationRespond,
                route,
            );
        }
        if let Some(route) = profile.embedding.as_ref() {
            push_internal_inference_route_diagnostic(
                &mut diagnostics,
                runtime,
                configuration,
                consumer_profile_id,
                None,
                AiOperation::EmbeddingCreate,
                route,
            );
        }
        if let Some(route) = profile.rerank.as_ref() {
            push_internal_inference_route_diagnostic(
                &mut diagnostics,
                runtime,
                configuration,
                consumer_profile_id,
                None,
                AiOperation::RerankDocuments,
                route,
            );
        }
    }
    diagnostics
}

/// Appends one diagnostic only when a retained profile route is not currently executable.
/// 仅在保留的配置档路由当前不可执行时追加一项诊断。
///
/// # Parameters
/// 参数
/// The profile slot supplies trusted expected identity and `runtime` performs exact resolution.
/// 配置档槽位提供可信期望身份，`runtime` 执行精确解析。
///
/// # Returns
/// 返回值
/// This function mutates `diagnostics` and has no independent return value.
/// 本函数修改 `diagnostics`，不返回独立值。
#[allow(clippy::too_many_arguments)]
fn push_internal_inference_route_diagnostic(
    diagnostics: &mut Vec<PublicAiInferenceRouteDiagnostic>,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    consumer_profile_id: &str,
    purpose_id: Option<&str>,
    expected_operation: AiOperation,
    route: &InferenceRouteConfiguration,
) {
    let resolution = if route.operation != expected_operation {
        Err(format!(
            "Persisted inference route operation `{}` does not match profile slot operation `{}`.",
            route.operation.as_str(),
            expected_operation.as_str()
        ))
    } else {
        runtime
            .resolve_internal_inference_selection(configuration, route)
            .and_then(|selection| {
                validate_vmm_llm_reasoning_policy(
                    consumer_profile_id,
                    expected_operation,
                    &selection.model.capabilities,
                )
            })
    };
    if let Err(reason) = resolution {
        diagnostics.push(PublicAiInferenceRouteDiagnostic {
            consumer_profile_id: consumer_profile_id.to_string(),
            purpose_id: purpose_id.map(str::to_string),
            operation: expected_operation,
            route: route.clone(),
            reason,
        });
    }
}

/// Builds exact internal inference route candidates from current connection catalogs.
/// 从当前连接目录构建精确内部推理路由候选。
///
/// # Parameters
/// 参数
/// `configuration`, `providers`, and `catalogs` are one coherent settings snapshot.
/// `configuration`、`providers` 与 `catalogs` 构成一份一致设置快照。
///
/// # Returns
/// 返回值
/// Stable candidates that preserve product, credential, binding, operation, and usage scope.
/// 返回保留产品、凭据、绑定、操作与用途范围的稳定候选。
/// Maps one media-understanding capability to its required model input resource.
/// 将一项媒体理解能力映射到模型必须支持的输入资源。
///
/// # Parameters
/// 参数
/// `capability` is the exact semantic provider-tool capability.
/// `capability` 是精确的语义供应商工具能力。
///
/// # Returns
/// 返回值
/// The required media kind, or absence for every non-understanding capability.
/// 返回所需媒体类型；非理解能力返回空。
fn media_understanding_resource_kind(
    capability: ProviderToolCapabilityKind,
) -> Option<ResourceKind> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Some(ResourceKind::Image),
        ProviderToolCapabilityKind::VideoUnderstand => Some(ResourceKind::Video),
        ProviderToolCapabilityKind::AudioUnderstand => Some(ResourceKind::Audio),
        _ => None,
    }
}

/// Reports whether one exact model route can replace a selected media-understanding backend.
/// 报告一条精确模型路由能否替代所选媒体理解后端。
///
/// # Parameters
/// 参数
/// `capabilities` is the resolved route contract and `capability` selects the media kind.
/// `capabilities` 是解析后的路由契约，`capability` 选择媒体类型。
///
/// # Returns
/// 返回值
/// `true` only for conversation routes with the required input and stable JSON output.
/// 仅对支持所需输入、会话能力及稳定 JSON 输出的路由返回 `true`。
fn model_route_supports_media_understanding(
    capabilities: &ModelCapabilities,
    capability: ProviderToolCapabilityKind,
) -> bool {
    let Some(resource_kind) = media_understanding_resource_kind(capability) else {
        return false;
    };
    capabilities
        .operations
        .contains(&AiOperation::ConversationRespond)
        && capabilities.input_resources.contains(&resource_kind)
        && capabilities.structured_output != StructuredOutputCapability::Unsupported
}

/// Projects model media-input declarations through the encoder set of one frozen protocol route.
/// 将模型媒体输入声明投影到一条冻结协议路由实际注册的编码器集合。
///
/// # Parameters
/// 参数
/// `capabilities` is the resolved model contract and `protocol_profile_id` is its immutable route binding.
/// `capabilities` 是已解析模型契约，`protocol_profile_id` 是其不可变路由绑定。
///
/// # Returns
/// 返回值
/// A cloned capability contract whose image, video, and audio inputs are retained only when the
/// selected protocol can encode them; unrelated resource declarations are preserved unchanged.
/// 返回克隆后的能力契约；仅当所选协议可编码时保留图片、视频与音频输入，其他资源声明保持不变。
fn protocol_projected_media_capabilities(
    capabilities: &ModelCapabilities,
    protocol_profile_id: &str,
) -> ModelCapabilities {
    // projected prevents model metadata from claiming a media route that the selected wire protocol cannot carry.
    // projected 防止模型元数据声明所选 Wire 协议无法承载的媒体路由。
    let mut projected = capabilities.clone();
    projected.input_resources.retain(|resource_kind| {
        let attachment_kind = match resource_kind {
            ResourceKind::Image => Some(ConversationAttachmentKind::Image),
            ResourceKind::Video => Some(ConversationAttachmentKind::Video),
            ResourceKind::Audio => Some(ConversationAttachmentKind::Audio),
            ResourceKind::File => None,
        };
        attachment_kind.is_none_or(|attachment_kind| {
            vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_attachment_kind(
                protocol_profile_id,
                attachment_kind,
            )
        })
    });
    if !vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_structured_output(
        protocol_profile_id,
        projected.structured_output,
    ) {
        // Unsupported is the only honest projection when the frozen encoder cannot represent the model policy.
        // 当冻结编码器无法表达模型策略时，Unsupported 是唯一诚实的投影结果。
        projected.structured_output = StructuredOutputCapability::Unsupported;
    }
    projected
}

/// Builds every executable account route eligible for at least one media-understanding kind.
/// 构建至少符合一种媒体理解类型的全部可执行账号路由。
///
/// # Parameters
/// 参数
/// `configuration` owns credentials, `providers` owns labels, and `catalogs` owns effective routes.
/// `configuration` 持有凭据，`providers` 持有展示名，`catalogs` 持有生效路由。
///
/// # Returns
/// 返回值
/// Stable sorted route candidates independent from conversation publication switches.
/// 返回与会话发布开关独立、稳定排序的路由候选。
fn build_media_understanding_model_candidates(
    configuration: &PersistedAiConfiguration,
    providers: &vulcan_provider_adapters::ProviderRegistry,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Vec<PublicAiMediaUnderstandingModelCandidate>, String> {
    let mut candidates = Vec::new();
    for connection in configuration.connection_graph.connections.values() {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            continue;
        }
        let Some(catalog) = catalogs.get(connection.id.as_str()) else {
            continue;
        };
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider `{}` referenced by connection `{}` is not registered.",
                    connection.provider_id, connection.id
                )
            })?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` referenced by connection `{}` is not registered.",
                    connection.service_entry_id, connection.id
                )
            })?;
        for model in &catalog.models {
            let Some(protocol_profile_id) = effective_catalog_model_protocol_id(model) else {
                continue;
            };
            // candidateCapabilities intersects model declarations with the exact protocol encoder set.
            // candidateCapabilities 将模型声明与精确协议编码器集合取交集。
            let candidate_capabilities =
                protocol_projected_media_capabilities(&model.capabilities, protocol_profile_id);
            let supports_any_media = [
                ProviderToolCapabilityKind::ImageUnderstand,
                ProviderToolCapabilityKind::VideoUnderstand,
                ProviderToolCapabilityKind::AudioUnderstand,
            ]
            .into_iter()
            .any(|capability| {
                model_route_supports_media_understanding(&candidate_capabilities, capability)
            });
            if !supports_any_media {
                continue;
            }
            candidates.push(PublicAiMediaUnderstandingModelCandidate {
                provider_display_name: provider.display_name.clone(),
                service_entry_display_name: entry.display_name.clone(),
                connection_display_name: connection.display_name.clone(),
                model_display_name: model.display_name.clone(),
                selection: ActiveAiModelSelection {
                    provider_id: connection.provider_id.clone(),
                    model_id: model.id.clone(),
                    connection_id: connection.id.clone(),
                },
                catalog_generation: catalog.generation,
                capabilities: candidate_capabilities,
            });
        }
    }
    candidates.sort_by(|left, right| {
        (
            left.provider_display_name.as_str(),
            left.model_display_name.as_str(),
            left.connection_display_name.as_str(),
        )
            .cmp(&(
                right.provider_display_name.as_str(),
                right.model_display_name.as_str(),
                right.connection_display_name.as_str(),
            ))
    });
    Ok(candidates)
}

fn build_internal_inference_candidates(
    configuration: &PersistedAiConfiguration,
    providers: &vulcan_provider_adapters::ProviderRegistry,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Vec<PublicAiInternalInferenceCandidate>, String> {
    // defaultInternalEnabledByProvider resolves each provider-wide suggestion once so every
    // credential route for the same model exposes one stable shared switch.
    // defaultInternalEnabledByProvider 只解析一次各供应商级建议，使同一模型的所有凭据路由
    // 公开一个稳定的共享开关。
    let default_internal_enabled_by_provider = providers
        .providers()
        .into_iter()
        .map(|provider| {
            provider
                .default_internal_use_for_connections(
                    configuration
                        .connection_graph
                        .connections
                        .values()
                        .filter(|connection| {
                            connection.provider_id == provider.id
                                && !configuration
                                    .connection_is_pending_validation(connection.id.as_str())
                        }),
                )
                .map(|enabled| (provider.id.clone(), enabled))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    // internalUsageScopes preserves runtime scheduling and authorization classifications while
    // model enablement remains shared across Knowledge and VMM consumers.
    // internalUsageScopes 保留运行时调度与鉴权分类，同时让知识库与 VMM 消费方共享模型启用状态。
    let internal_usage_scopes = BTreeSet::from([
        InferenceUsageScope::InternalBackground,
        InferenceUsageScope::ExternalService,
    ]);
    let mut candidates = Vec::new();
    for connection in configuration.connection_graph.connections.values() {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            continue;
        }
        let Some(catalog) = catalogs.get(connection.id.as_str()) else {
            continue;
        };
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider `{}` referenced by connection `{}` is not registered.",
                    connection.provider_id, connection.id
                )
            })?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` referenced by connection `{}` is not registered.",
                    connection.service_entry_id, connection.id
                )
            })?;
        let default_internal_enabled = default_internal_enabled_by_provider
            .get(provider.id.as_str())
            .copied()
            .ok_or_else(|| {
                format!(
                    "Provider `{}` is missing its internal-use default projection.",
                    provider.id
                )
            })?;
        for model in &catalog.models {
            for operation in model
                .capabilities
                .operations
                .iter()
                .copied()
                .filter(|operation| {
                    matches!(
                        operation,
                        AiOperation::ConversationRespond
                            | AiOperation::EmbeddingCreate
                            | AiOperation::RerankDocuments
                    )
                })
            {
                let binding = match operation {
                    AiOperation::ConversationRespond => {
                        // Internal LLM consumers must persist the same frozen model binding used
                        // by interactive execution; connection credentials never resolve protocols.
                        // 内部 LLM 消费方必须持久化交互执行所使用的同一冻结模型绑定；连接凭据
                        // 绝不解析协议。
                        let Some(protocol_profile_id) = effective_catalog_model_protocol_id(model)
                        else {
                            continue;
                        };
                        InferenceRouteBinding::Protocol {
                            protocol_profile_id: protocol_profile_id.to_string(),
                        }
                    }
                    AiOperation::EmbeddingCreate | AiOperation::RerankDocuments => {
                        let Some(action_binding_id) =
                            registered_native_action_binding_id(entry, operation)
                        else {
                            continue;
                        };
                        InferenceRouteBinding::Action {
                            action_binding_id: action_binding_id.to_string(),
                        }
                    }
                    _ => continue,
                };
                for usage_scope in internal_usage_scopes.iter().copied() {
                    candidates.push(PublicAiInternalInferenceCandidate {
                        provider_display_name: provider.display_name.clone(),
                        service_entry_display_name: entry.display_name.clone(),
                        connection_display_name: connection.display_name.clone(),
                        model_display_name: model.display_name.clone(),
                        route: InferenceRouteConfiguration {
                            provider_id: connection.provider_id.clone(),
                            service_entry_id: connection.service_entry_id.clone(),
                            connection_id: connection.id.clone(),
                            model_id: model.id.clone(),
                            binding: binding.clone(),
                            operation,
                            usage_scope,
                        },
                        catalog_generation: catalog.generation,
                        enabled: configuration.internal_model_is_enabled(
                            connection.provider_id.as_str(),
                            model.id.as_str(),
                            default_internal_enabled,
                        ),
                        usage_scopes: internal_usage_scopes.clone(),
                        capabilities: model.capabilities.clone(),
                        overrides: configuration
                            .internal_model_overrides
                            .get(
                                PersistedAiConfiguration::internal_model_operation_key(
                                    connection.provider_id.as_str(),
                                    model.id.as_str(),
                                    operation,
                                )
                                .as_str(),
                            )
                            .cloned()
                            .unwrap_or_default(),
                    });
                }
            }
        }
    }
    candidates.sort_by(|left, right| {
        (
            left.route.operation,
            left.provider_display_name.as_str(),
            left.model_display_name.as_str(),
            left.connection_display_name.as_str(),
            left.route.usage_scope,
        )
            .cmp(&(
                right.route.operation,
                right.provider_display_name.as_str(),
                right.model_display_name.as_str(),
                right.connection_display_name.as_str(),
                right.route.usage_scope,
            ))
    });
    Ok(candidates)
}

/// Resolves one exact official internal route for a knowledge-library setting.
/// 为资料库配置解析一条精确的官方内部路由。
pub(super) fn resolve_knowledge_internal_inference_candidate(
    state: &SharedServiceBridgeState,
    provider_id: &str,
    service_entry_id: &str,
    connection_id: Option<&str>,
    model_id: &str,
    binding: &InferenceRouteBinding,
    operation: AiOperation,
    usage_scope: InferenceUsageScope,
) -> Result<PublicAiInternalInferenceCandidate, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)?;
    let providers = runtime.effective_providers(&configuration)?;
    let catalogs = runtime.catalog_snapshot()?;
    let candidate = build_internal_inference_candidates(&configuration, &providers, &catalogs)?
        .into_iter()
        .find(|candidate| {
            candidate.route.provider_id == provider_id
                && candidate.route.service_entry_id == service_entry_id
                && connection_id.is_none_or(|value| candidate.route.connection_id == value)
                && candidate.route.model_id == model_id
                && candidate.route.binding == *binding
                && candidate.route.operation == operation
                && candidate.route.usage_scope == usage_scope
        })
        .ok_or_else(|| {
            format!(
                "未找到可执行的官方知识库模型路由：provider={provider_id}, service={service_entry_id}, model={model_id}, operation={}",
                operation.as_str()
            )
        })?;
    if !candidate.enabled {
        return Err(format!(
            "知识库模型当前未在模型列表启用内部使用：provider={provider_id}, model={model_id}"
        ));
    }
    Ok(candidate)
}

/// Synchronizes one library-owned inference profile with its validated model routes.
/// 将一个资料库独立拥有的推理配置档同步为经过校验的模型路由。
///
/// The profile is intentionally keyed by the immutable library identifier so two libraries
/// can select different models without overwriting one another or the VMM profile.
/// 配置档故意使用不可变资料库标识作为键，确保不同资料库可以选择不同模型，且不会覆盖 VMM 配置档。
pub(super) async fn sync_knowledge_inference_profile(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    consumer_profile_id: &str,
    curation_route: Option<InferenceRouteConfiguration>,
    embedding_route: Option<InferenceRouteConfiguration>,
) -> Result<(), String> {
    validate_non_empty("consumerProfileId", consumer_profile_id)?;
    // transaction serializes route validation through profile and Workbench publication.
    // transaction 把路由校验串行化到配置档与工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    for route in [curation_route.as_ref(), embedding_route.as_ref()]
        .into_iter()
        .flatten()
    {
        if !matches!(
            route.operation,
            AiOperation::ConversationRespond | AiOperation::EmbeddingCreate
        ) {
            return Err(format!(
                "资料库配置档不支持内部操作 `{}`",
                route.operation.as_str()
            ));
        }
        let selection =
            runtime.resolve_internal_inference_selection(&current_configuration, route)?;
        validate_vmm_llm_reasoning_policy(
            consumer_profile_id,
            route.operation,
            &selection.model.capabilities,
        )?;
    }
    let (_configuration, ()) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存资料库独立模型配置失败",
        |configuration| {
            let profile = configuration
                .inference_profiles
                .entry(consumer_profile_id.to_string())
                .or_default();
            if let Some(route) = curation_route.clone() {
                profile
                    .llm_routes
                    .insert(String::from(KNOWLEDGE_CURATION_PURPOSE_ID), route);
            } else {
                profile.llm_routes.remove(KNOWLEDGE_CURATION_PURPOSE_ID);
            }
            profile.embedding = embedding_route.clone();
            Ok(())
        },
        |_configuration| Ok(()),
    )?;
    drop(transaction);
    Ok(())
}

/// Removes one library-owned inference profile after the library is deleted.
/// 删除资料库后移除该资料库独立拥有的推理配置档。
pub(super) async fn remove_knowledge_inference_profile(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    consumer_profile_id: &str,
) -> Result<(), String> {
    validate_non_empty("consumerProfileId", consumer_profile_id)?;
    // transaction serializes profile removal through Workbench publication.
    // transaction 把配置档删除串行化到工作台发布完成。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let (_configuration, ()) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "删除资料库独立模型配置失败",
        |configuration| {
            configuration.inference_profiles.remove(consumer_profile_id);
            Ok(())
        },
        |_configuration| Ok(()),
    )?;
    drop(transaction);
    Ok(())
}

/// Validates one complete internal override against one exact model route.
/// 根据一条精确模型路由校验一份完整内部覆盖。
fn validate_internal_model_overrides(
    capabilities: &ModelCapabilities,
    route: &InferenceRouteConfiguration,
    overrides: &InternalModelOverrides,
) -> Result<(), String> {
    validate_internal_model_parameters(capabilities, route, overrides)?;
    match route.operation {
        AiOperation::ConversationRespond => {
            ensure_no_embedding_overrides(overrides)?;
            ensure_no_rerank_overrides(overrides)?;
            validate_positive_limit(
                "Internal context limit",
                overrides.context_limit,
                capabilities.context_limit,
            )?;
            validate_positive_limit(
                "Internal output limit",
                overrides.output_limit,
                capabilities.output_limit,
            )?;
            if let Some(effort) = overrides.reasoning_effort {
                validate_internal_reasoning_effort(capabilities, effort)?;
            }
        }
        AiOperation::EmbeddingCreate => {
            ensure_no_llm_overrides(overrides)?;
            ensure_no_rerank_overrides(overrides)?;
            let embedding = capabilities.embedding.as_ref().ok_or_else(|| {
                String::from("The selected route does not declare embedding capabilities.")
            })?;
            if let Some(task) = overrides.embedding_task
                && !embedding.tasks.contains(&task)
            {
                return Err(format!(
                    "Embedding task `{:?}` is not supported by the selected model.",
                    task
                ));
            }
            if let Some(dimensions) = overrides.embedding_dimensions {
                let in_discrete_set =
                    embedding.dimensions.is_empty() || embedding.dimensions.contains(&dimensions);
                let above_minimum = embedding
                    .minimum_dimensions
                    .is_none_or(|minimum| dimensions >= minimum);
                let below_maximum = embedding
                    .maximum_dimensions
                    .is_none_or(|maximum| dimensions <= maximum);
                if dimensions == 0 || !in_discrete_set || !above_minimum || !below_maximum {
                    return Err(format!(
                        "Embedding dimensions {dimensions} are not supported by the selected model."
                    ));
                }
            }
            if let Some(output_kind) = overrides.embedding_output_kind
                && !embedding.output_kinds.contains(&output_kind)
            {
                return Err(format!(
                    "Embedding output kind `{:?}` is not supported by the selected model.",
                    output_kind
                ));
            }
            if let Some(encoding) = overrides.embedding_encoding
                && !embedding.encodings.contains(&encoding)
            {
                return Err(format!(
                    "Embedding encoding `{:?}` is not supported by the selected model.",
                    encoding
                ));
            }
        }
        AiOperation::RerankDocuments => {
            ensure_no_llm_overrides(overrides)?;
            ensure_no_embedding_overrides(overrides)?;
            let rerank = capabilities.rerank.as_ref().ok_or_else(|| {
                String::from("The selected route does not declare rerank capabilities.")
            })?;
            if let Some(top_n) = overrides.rerank_top_n {
                if top_n == 0 {
                    return Err(String::from("Rerank top N must be greater than zero."));
                }
                if rerank
                    .maximum_candidates
                    .is_some_and(|maximum| top_n > maximum)
                {
                    return Err(format!(
                        "Rerank top N {top_n} exceeds the selected model limit."
                    ));
                }
            }
            if let Some(truncation) = overrides.rerank_truncation
                && !rerank.truncation.contains(&truncation)
            {
                return Err(format!(
                    "Rerank truncation `{:?}` is not supported by the selected model.",
                    truncation
                ));
            }
            if overrides.rerank_return_content == Some(true) && !rerank.supports_return_content {
                return Err(String::from(
                    "The selected rerank model cannot return candidate content.",
                ));
            }
        }
        _ => {
            return Err(format!(
                "Operation `{}` cannot own internal model overrides.",
                route.operation.as_str()
            ));
        }
    }
    Ok(())
}

/// Enforces VMM's no-reasoning policy for one resolved route.
/// 对一条已解析路由强制执行 VMM 禁用思考策略。
///
/// # Parameters
/// 参数
/// `consumer_profile_id` and `operation` identify the profile slot; `capabilities` is exact.
/// `consumer_profile_id` 与 `operation` 标识配置档槽位；`capabilities` 是精确能力。
///
/// # Returns
/// 返回值
/// Success outside VMM LLM slots or when the model explicitly supports disabled reasoning.
/// 非 VMM LLM 槽位或模型明确支持关闭思考时返回成功。
fn validate_vmm_llm_reasoning_policy(
    consumer_profile_id: &str,
    operation: AiOperation,
    capabilities: &ModelCapabilities,
) -> Result<(), String> {
    if consumer_profile_id != VMM_CONSUMER_PROFILE_ID
        || operation != AiOperation::ConversationRespond
        || model_supports_disabled_reasoning(capabilities)
    {
        return Ok(());
    }
    Err(String::from(
        "VMM LLM routes require a model that explicitly supports disabled reasoning.",
    ))
}

/// Validates registered model parameters and their dependency graph for one exact route.
/// 校验一条精确路由已注册的模型参数及其依赖图。
fn validate_internal_model_parameters(
    capabilities: &ModelCapabilities,
    route: &InferenceRouteConfiguration,
    overrides: &InternalModelOverrides,
) -> Result<(), String> {
    let protocol_id = match &route.binding {
        InferenceRouteBinding::Protocol {
            protocol_profile_id,
        } => Some(protocol_profile_id.as_str()),
        InferenceRouteBinding::Action { .. } => None,
    };
    for (parameter_id, value) in &overrides.model_parameters {
        let definition = capabilities
            .model_parameters
            .iter()
            .find(|definition| {
                definition.id == *parameter_id
                    && (definition.model_ids.is_empty()
                        || definition.model_ids.contains(&route.model_id))
                    && (definition.protocol_ids.is_empty()
                        || protocol_id.is_some_and(|protocol_id| {
                            definition
                                .protocol_ids
                                .iter()
                                .any(|candidate| candidate == protocol_id)
                        }))
            })
            .ok_or_else(|| {
                format!(
                    "Model parameter `{parameter_id}` is not registered for the selected route."
                )
            })?;
        definition.validate_value(value)?;
        for dependency in &definition.dependent_required {
            if !overrides.model_parameters.contains_key(dependency) {
                return Err(format!(
                    "Model parameter `{parameter_id}` requires `{dependency}`."
                ));
            }
        }
        for conflict in &definition.conflicts {
            if overrides.model_parameters.contains_key(conflict) {
                return Err(format!(
                    "Model parameter `{parameter_id}` conflicts with `{conflict}`."
                ));
            }
        }
    }
    Ok(())
}

/// Validates one positive local limit against an exact provider maximum.
/// 根据精确供应商上限校验一个正数本地限制。
fn validate_positive_limit(
    label: &str,
    requested: Option<u64>,
    maximum: Option<u64>,
) -> Result<(), String> {
    let Some(requested) = requested else {
        return Ok(());
    };
    if requested == 0 {
        return Err(format!("{label} must be greater than zero."));
    }
    let maximum = maximum.ok_or_else(|| {
        format!("{label} cannot be overridden because the provider did not declare a maximum.")
    })?;
    if requested > maximum {
        return Err(format!(
            "{label} {requested} exceeds the provider maximum {maximum}."
        ));
    }
    Ok(())
}

/// Validates one explicit internal reasoning default against exact control metadata.
/// 根据精确控制元数据校验一个显式内部推理默认值。
fn validate_internal_reasoning_effort(
    capabilities: &ModelCapabilities,
    effort: ReasoningEffort,
) -> Result<(), String> {
    let valid = match capabilities.reasoning_control {
        ReasoningControlMode::FixedOff => effort == ReasoningEffort::None,
        ReasoningControlMode::FixedOn => {
            effort != ReasoningEffort::None && capabilities.reasoning_efforts.contains(&effort)
        }
        ReasoningControlMode::Toggle
        | ReasoningControlMode::EffortSelection
        | ReasoningControlMode::ProviderAdaptive => {
            capabilities.reasoning_efforts.contains(&effort)
        }
        ReasoningControlMode::Unknown => false,
    };
    valid.then_some(()).ok_or_else(|| {
        format!(
            "Reasoning effort `{}` is not declared by the selected model route.",
            effort.as_str()
        )
    })
}

/// Rejects LLM-only fields on non-LLM operations.
/// 拒绝非 LLM 操作上的 LLM 专属字段。
fn ensure_no_llm_overrides(overrides: &InternalModelOverrides) -> Result<(), String> {
    if overrides.context_limit.is_some()
        || overrides.output_limit.is_some()
        || overrides.reasoning_effort.is_some()
    {
        return Err(String::from(
            "LLM limits and reasoning defaults cannot be saved for this operation.",
        ));
    }
    Ok(())
}

/// Rejects embedding-only fields on other operations.
/// 拒绝其他操作上的向量专属字段。
fn ensure_no_embedding_overrides(overrides: &InternalModelOverrides) -> Result<(), String> {
    if overrides.embedding_task.is_some()
        || overrides.embedding_dimensions.is_some()
        || overrides.embedding_output_kind.is_some()
        || overrides.embedding_encoding.is_some()
    {
        return Err(String::from(
            "Embedding defaults cannot be saved for this operation.",
        ));
    }
    Ok(())
}

/// Rejects rerank-only fields on other operations.
/// 拒绝其他操作上的重排专属字段。
fn ensure_no_rerank_overrides(overrides: &InternalModelOverrides) -> Result<(), String> {
    if overrides.rerank_top_n.is_some()
        || overrides.rerank_truncation.is_some()
        || overrides.rerank_return_content.is_some()
    {
        return Err(String::from(
            "Rerank defaults cannot be saved for this operation.",
        ));
    }
    Ok(())
}

/// Invalidates node-dependent budget fingerprints for every locked active or archived session.
/// 使所有已锁定活动或归档会话中依赖节点的预算指纹失效。
///
/// # Parameters
/// 参数
/// `workspace` is the detached candidate whose session node ids remain unchanged.
/// `workspace` 是会话节点 ID 保持不变的独立候选工作区。
///
/// # Returns
/// 返回值
/// Exact session ids whose in-memory live budget snapshots must also be removed at commit.
/// 返回提交时还必须移除内存 live 预算快照的精确会话 ID。
fn invalidate_context_compaction_window_budget_fingerprints(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> BTreeSet<crate::SessionId> {
    // invalidatedSessionIds identifies only sessions whose selected node resolves through global values.
    // invalidatedSessionIds 仅标识所选节点通过全局数值解析的会话。
    let mut invalidated_session_ids = BTreeSet::new();
    for (session_id, runtime_state) in workspace
        .session_runtime_state
        .iter_mut()
        .chain(workspace.archived_session_runtime_state.iter_mut())
    {
        if runtime_state.context_compaction_window_node_id.is_none() {
            continue;
        }
        runtime_state.live_context_budget_fingerprint = None;
        runtime_state.anchor_context_budget_fingerprint = None;
        invalidated_session_ids.insert(*session_id);
    }
    invalidated_session_ids
}

/// Publishes changed global node values with atomic session-budget invalidation.
/// 以原子会话预算失效方式发布已变化的全局节点数值。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `configuration` identify the exact committed generation;
/// `nodes_changed` distinguishes a real value change from an idempotent save.
/// `app`、`state`、`runtime` 与 `configuration` 标识精确提交代际；`nodes_changed`
/// 区分真实数值变化与幂等保存。
///
/// # Returns
/// 返回值
/// The atomically installed Workbench snapshot, or one explicit publication error.
/// 返回原子安装的工作台快照，或一条明确的发布错误。
fn publish_public_ai_workbench_after_context_compaction_window_nodes_change(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    nodes_changed: bool,
) -> Result<WorkbenchSnapshot, String> {
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        app,
        state,
        runtime,
        configuration,
        |workspace| {
            // invalidatedSessionIds is empty for an idempotent save and preserves every route lease.
            // invalidatedSessionIds 在幂等保存时为空，并始终保留全部路由租约。
            let invalidated_session_ids = nodes_changed
                .then(|| invalidate_context_compaction_window_budget_fingerprints(workspace))
                .unwrap_or_default();
            Ok(
                PublicAiWorkspaceReconciliation::context_budget_invalidations(
                    invalidated_session_ids,
                ),
            )
        },
        true,
        "context_compaction_window_nodes",
        || Ok(()),
    )
}

/// Publishes public AI model rows through the existing shared workbench event.
/// 通过既有共享工作台事件发布公共 AI 模型行。
///
/// # Parameters
/// 参数
/// `app` emits the snapshot, `state` owns workbench state, and runtime/configuration are exact.
/// `app` 发出快照，`state` 持有工作台状态，运行时与配置均为精确快照。
///
/// # Returns
/// 返回值
/// The updated workbench snapshot after an atomic state replacement.
/// 原子替换状态后返回更新的工作台快照。
pub(crate) fn publish_public_ai_workbench(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Result<WorkbenchSnapshot, String> {
    publish_public_ai_workbench_with_commit(app, state, runtime, configuration, || Ok(()))
}

/// Publishes provider-tool changes while preserving every session model route lease.
/// 发布供应商工具变更，同时保留全部会话模型路由租约。
///
/// # Parameters
/// 参数
/// `app` emits the snapshot, `state` owns Workbench state, and `runtime` plus `configuration`
/// identify the exact provider projection.
/// `app` 发出快照，`state` 持有工作台状态，`runtime` 与 `configuration` 标识精确供应商投影。
///
/// # Returns
/// 返回值
/// The updated Workbench snapshot after an atomic provider-tool-only state replacement.
/// 在原子替换仅供应商工具状态后返回更新的工作台快照。
fn publish_public_ai_workbench_without_session_model_reconciliation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Result<WorkbenchSnapshot, String> {
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        app,
        state,
        runtime,
        configuration,
        |_workspace| Ok(PublicAiWorkspaceReconciliation::default()),
        false,
        "provider_tool_settings",
        || Ok(()),
    )
}

/// Publishes one public AI Workbench candidate after a caller-owned pre-publication commit.
/// 在调用方持有的发布前提交成功后发布一份公共 AI 工作台候选。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `configuration` identify the exact projection; `commit` performs
/// any final fallible durable transition while browser-visible state is still unchanged.
/// `app`、`state`、`runtime` 与 `configuration` 标识精确投影；`commit` 在浏览器可见状态仍未
/// 改变时执行最终可失败持久转换。
///
/// # Returns
/// 返回值
/// The atomically installed Workbench snapshot, or an error that leaves Service state unchanged.
/// 返回原子安装的工作台快照，或使 Service 状态保持不变的错误。
fn publish_public_ai_workbench_with_commit<Commit>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    commit: Commit,
) -> Result<WorkbenchSnapshot, String>
where
    Commit: FnOnce() -> Result<(), String>,
{
    publish_public_ai_workbench_with_session_reconciliation_and_commit(
        app,
        state,
        runtime,
        configuration,
        |_workspace| Ok(BTreeSet::new()),
        "missing_model_fallback",
        commit,
    )
}

/// One detached workspace reconciliation split by persistence and route-lease ownership.
/// 一份按持久化与路由租约所有权拆分的独立工作区协调结果。
#[derive(Default)]
struct PublicAiWorkspaceReconciliation {
    /// Every session runtime row changed by the detached plan.
    /// 独立规划修改的每个会话运行态行。
    changed_session_ids: BTreeSet<crate::SessionId>,
    /// Changed sessions whose process-local model route leases must be revoked.
    /// 已修改且必须撤销进程内模型路由租约的会话。
    revoked_session_ids: BTreeSet<crate::SessionId>,
    /// Sessions whose node-dependent in-memory context budgets must be discarded at commit.
    /// 提交时必须丢弃依赖节点的内存上下文预算的会话。
    invalidated_context_budget_session_ids: BTreeSet<crate::SessionId>,
}

impl PublicAiWorkspaceReconciliation {
    /// Creates a reconciliation whose every changed row is an explicit route revocation.
    /// 创建一份每个已修改行均属于显式路由撤销的协调结果。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` is the exact changed and revoked owner set.
    /// `session_ids` 是精确的已修改与已撤销所有者集合。
    ///
    /// # Returns
    /// 返回值
    /// A reconciliation with identical changed and revoked sets.
    /// 返回已修改与已撤销集合完全相同的协调结果。
    fn route_revocations(session_ids: BTreeSet<crate::SessionId>) -> Self {
        Self {
            changed_session_ids: session_ids.clone(),
            revoked_session_ids: session_ids,
            invalidated_context_budget_session_ids: BTreeSet::new(),
        }
    }

    /// Creates a workspace-only reconciliation that preserves model route leases.
    /// 创建一份保留模型路由租约的仅工作区协调结果。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` identifies runtime rows that require persistence only.
    /// `session_ids` 标识仅需持久化的运行态行。
    ///
    /// # Returns
    /// 返回值
    /// A reconciliation with changed rows and no revoked route owners.
    /// 返回包含已修改行但不包含已撤销路由所有者的协调结果。
    fn workspace_only(session_ids: BTreeSet<crate::SessionId>) -> Self {
        Self {
            changed_session_ids: session_ids,
            revoked_session_ids: BTreeSet::new(),
            invalidated_context_budget_session_ids: BTreeSet::new(),
        }
    }

    /// Creates an in-memory budget invalidation that preserves durable controls and route leases.
    /// 创建一份保留持久控制项与路由租约的内存预算失效结果。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` identifies locked active or archived sessions affected by changed node values.
    /// `session_ids` 标识受节点数值变化影响的已锁定活动或归档会话。
    ///
    /// # Returns
    /// 返回值
    /// A reconciliation that invalidates context budgets without scheduling redundant addon writes.
    /// 返回一份使上下文预算失效且不安排冗余 addon 写入的协调结果。
    fn context_budget_invalidations(session_ids: BTreeSet<crate::SessionId>) -> Self {
        Self {
            changed_session_ids: BTreeSet::new(),
            revoked_session_ids: BTreeSet::new(),
            invalidated_context_budget_session_ids: session_ids,
        }
    }
}

/// Publishes one Workbench candidate with explicit session-route reconciliation.
/// 通过显式会话路由协调发布一份工作台候选。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `configuration` identify the exact projection;
/// `reconcile_sessions` changes a detached workspace and returns leases that must be revoked;
/// `persistence_reason` labels durable session-state persistence; `commit` performs the final
/// fallible external transition before browser-visible state changes.
/// `app`、`state`、`runtime` 与 `configuration` 标识精确投影；`reconcile_sessions` 修改独立
/// 工作区并返回必须撤销租约的会话；`persistence_reason` 标记持久会话状态写入；`commit`
/// 在浏览器可见状态改变前执行最终可失败外部转换。
///
/// # Returns
/// 返回值
/// The atomically installed Workbench snapshot, or an error that restores revoked leases and
/// leaves Service state unchanged.
/// 返回原子安装的工作台快照；失败时恢复已撤销租约并保持 Service 状态不变。
fn publish_public_ai_workbench_with_session_reconciliation_and_commit<ReconcileSessions, Commit>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    reconcile_sessions: ReconcileSessions,
    persistence_reason: &'static str,
    commit: Commit,
) -> Result<WorkbenchSnapshot, String>
where
    ReconcileSessions: FnOnce(
        &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    ) -> Result<BTreeSet<crate::SessionId>, String>,
    Commit: FnOnce() -> Result<(), String>,
{
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        app,
        state,
        runtime,
        configuration,
        |workspace| {
            reconcile_sessions(workspace).map(PublicAiWorkspaceReconciliation::route_revocations)
        },
        true,
        persistence_reason,
        commit,
    )
}

/// Publishes one Workbench candidate from a detached general workspace reconciliation.
/// 从一份独立的通用工作区协调结果发布工作台候选。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `configuration` identify the projection; `reconcile_workspace`
/// returns separately tracked changed and route-revoked sessions; `reconcile_session_models`
/// controls whether unrelated catalog-only model reconciliation runs; `persistence_reason` labels
/// durable session rows; `commit` performs the final fallible external transition.
/// `app`、`state`、`runtime` 与 `configuration` 标识投影；`reconcile_workspace` 分别返回已修改
/// 与路由已撤销会话；`reconcile_session_models` 控制是否运行无关的仅目录模型协调；
/// `persistence_reason` 标记持久会话行；`commit` 执行最终可失败外部转换。
///
/// # Returns
/// 返回值
/// The atomically installed Workbench snapshot, or an error with Service state and revoked leases
/// restored.
/// 返回原子安装的工作台快照；失败时恢复 Service 状态与已撤销租约。
fn publish_public_ai_workbench_with_workspace_reconciliation_and_commit<
    ReconcileWorkspace,
    Commit,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    reconcile_workspace: ReconcileWorkspace,
    reconcile_session_models: bool,
    persistence_reason: &'static str,
    commit: Commit,
) -> Result<WorkbenchSnapshot, String>
where
    ReconcileWorkspace: FnOnce(
        &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    ) -> Result<PublicAiWorkspaceReconciliation, String>,
    Commit: FnOnce() -> Result<(), String>,
{
    // catalogs freezes the exact runtime model image used by the whole candidate projection.
    // catalogs 冻结整个候选投影使用的精确运行时模型镜像。
    let catalogs = runtime.catalog_snapshot()?;
    let (workbench, persistence_context) = {
        // guard excludes every concurrent Service state writer while the detached candidate is
        // prepared and committed.
        // guard 在独立候选准备与提交期间排除全部并发 Service 状态写入方。
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        // plannedWorkspace and changedSessionIds remain detached until every fallible projection
        // and lease-exclusion check has succeeded.
        // plannedWorkspace 与 changedSessionIds 在全部可失败投影及租约排除检查成功前保持独立。
        let (
            planned_workspace,
            revoked_session_ids,
            changed_session_ids,
            invalidated_context_budget_session_ids,
        ) = if let Some(workspace) = guard.workspace_snapshot.as_ref() {
            // forcedWorkspace applies explicit access revocation before catalog-only fallback.
            // forcedWorkspace 在仅目录回退前应用显式访问撤销。
            let mut forced_workspace = workspace.clone();
            let PublicAiWorkspaceReconciliation {
                mut changed_session_ids,
                revoked_session_ids,
                invalidated_context_budget_session_ids,
            } = reconcile_workspace(&mut forced_workspace)?;
            let (planned_workspace, catalog_changed_session_ids) = if reconcile_session_models {
                plan_reconciled_session_model_selections(
                    forced_workspace,
                    runtime,
                    configuration,
                    &catalogs,
                )?
            } else {
                // Provider-tool changes do not change the model route owned by this session.
                // 供应商工具变更不会改变该会话拥有的模型路由。
                (forced_workspace, BTreeSet::new())
            };
            changed_session_ids.extend(catalog_changed_session_ids);
            (
                Some(planned_workspace),
                revoked_session_ids,
                changed_session_ids,
                invalidated_context_budget_session_ids,
            )
        } else {
            (None, BTreeSet::new(), BTreeSet::new(), BTreeSet::new())
        };
        // selectedSessionId and selectedRuntimeState identify the same candidate workbench owner
        // for the shared lease-aware projection helper.
        // selectedSessionId 与 selectedRuntimeState 为共享租约感知投影辅助函数标识同一候选工作台所有者。
        let selected_session_id = planned_workspace
            .as_ref()
            .map(|workspace| workspace.selected_session);
        let selected_runtime_state = planned_workspace.as_ref().and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&workspace.selected_session)
        });
        // plannedLiveContextBudgets removes stale node-derived snapshots without touching other sessions.
        // plannedLiveContextBudgets 移除依赖旧节点值的快照，同时不影响其他会话。
        let planned_live_context_budgets = if invalidated_context_budget_session_ids.is_empty() {
            None
        } else {
            let mut live_context_budgets = guard.live_context_budgets.clone();
            live_context_budgets.retain(|session_id, _| {
                !invalidated_context_budget_session_ids.contains(session_id)
            });
            Some(live_context_budgets)
        };
        // effectiveSelection prefers the immutable process-local session lease before current catalogs.
        // effectiveSelection 在当前目录之前优先采用进程内不可变会话租约。
        let effective_selection = if selected_session_id
            .is_some_and(|session_id| revoked_session_ids.contains(&session_id))
        {
            public_ai_workbench_selection_after_route_revocation(
                configuration,
                runtime,
                selected_runtime_state,
            )?
        } else {
            public_ai_workbench_selection(
                configuration,
                runtime,
                selected_session_id,
                selected_runtime_state,
            )?
        };
        // bridgedMediaKinds freezes every exact attachment bridge result for this candidate.
        // bridgedMediaKinds 冻结当前候选的每一项精确附件桥接结果。
        let bridged_media_kinds = public_ai_session_media_understanding_bridge_kinds(
            runtime,
            configuration,
            selected_runtime_state,
        )?;
        // rows is the complete provider projection derived from the same immutable selection.
        // rows 是从同一不可变选择派生的完整供应商投影。
        let rows = public_ai_provider_rows_for_selection(
            configuration,
            runtime,
            effective_selection.as_ref(),
        )?;
        // plannedWorkbench isolates all browser-visible mutations until the final commit boundary.
        // plannedWorkbench 把全部浏览器可见变更隔离到最终提交边界。
        let mut planned_workbench = guard.workbench.clone();
        planned_workbench.providers = rows;
        // contextCompactionWindowNodes publishes the same validated configuration generation as providers.
        // contextCompactionWindowNodes 与供应商一起发布同一份已校验配置代际。
        planned_workbench.context_compaction_window_nodes = configuration
            .context_compaction_window_nodes
            .nodes
            .iter()
            .map(|node| ContextCompactionWindowNodeSummary {
                id: node.id.as_str().to_string(),
                tokens: node.tokens,
            })
            .collect();
        // selectedNodeId remains session-owned and is never replaced by the global token values.
        // selectedNodeId 始终由会话持有，绝不会被全局 Token 数值替代。
        planned_workbench.session_context_compaction_window_node_id = selected_runtime_state
            .and_then(|runtime_state| runtime_state.context_compaction_window_node_id)
            .map(|node_id| node_id.as_str().to_string());
        if selected_session_id
            .is_some_and(|session_id| invalidated_context_budget_session_ids.contains(&session_id))
        {
            // contextUsage is rebuilt from historical usage and the new node value in the same generation.
            // contextUsage 在同一代际中根据历史用量与新节点值重新构建。
            let live_context_budgets = planned_live_context_budgets
                .as_ref()
                .unwrap_or(&guard.live_context_budgets);
            let compression_window_limit =
                crate::context_compaction_window_limit_from_configuration(
                    selected_runtime_state,
                    Some(configuration),
                );
            planned_workbench.context_usage = selected_session_id.and_then(|session_id| {
                crate::context_usage_snapshot_from_runtime_state(
                    session_id,
                    selected_runtime_state,
                    live_context_budgets,
                    effective_selection
                        .as_ref()
                        .map(|selection| &selection.model.capabilities),
                    compression_window_limit,
                )
            });
        }
        if let Some(provider_configuration) = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| agent_runtime.provider_configuration.clone())
        {
            planned_workbench.reasoning_selection = crate::reasoning_selection(
                &provider_configuration,
                effective_selection
                    .as_ref()
                    .map(|selection| &selection.model.capabilities),
            );
            planned_workbench.composer_attachment_capabilities =
                crate::composer_attachment_capabilities(
                    effective_selection.as_ref().map(|selection| {
                        selection.route.protocol_route.protocol_profile_id.as_str()
                    }),
                    effective_selection
                        .as_ref()
                        .map(|selection| &selection.model.capabilities),
                    &bridged_media_kinds,
                );
        }
        if let Some(planned_workspace) = planned_workspace.as_ref() {
            // selectedMessages publishes any transaction-owned display record in the same revision as the picker state.
            // selectedMessages 在与选择器状态相同的修订中发布事务自有的显示记录。
            crate::project_conversation_services::refresh_selected_workbench_messages_from_workspace(
                planned_workspace,
                &mut planned_workbench,
            );
        }
        // leaseScopeIds materializes the exact sessions proven unleased during planning.
        // leaseScopeIds 物化规划期间被证明未租用的精确会话。
        let revoked_lease_scope_ids = revoked_session_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        // requiredAbsentLeaseScopeIds contains catalog-only changes whose leases must remain absent.
        // requiredAbsentLeaseScopeIds 包含租约必须保持缺失的仅目录变更。
        let required_absent_lease_scope_ids = if reconcile_session_models {
            changed_session_ids
                .difference(&revoked_session_ids)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        } else {
            // Provider-tool persistence is independent of model-route lease ownership.
            // 供应商工具持久化独立于模型路由租约所有权。
            Vec::new()
        };
        // leaseGuard atomically removes explicit revocations and excludes every concurrent writer.
        // leaseGuard 原子移除显式撤销，并排除全部并发写入方。
        let lease_guard =
            if revoked_lease_scope_ids.is_empty() && required_absent_lease_scope_ids.is_empty() {
                None
            } else {
                Some(
                    runtime.acquire_session_selection_lease_reconciliation_guard(
                        &revoked_lease_scope_ids,
                        &required_absent_lease_scope_ids,
                    )?,
                )
            };
        // commit is the last fallible external transition and runs while Service state and lease
        // admission remain excluded. A failure therefore leaves the browser before-image intact.
        // commit 是最后一个可失败外部转换，并在 Service 状态与租约准入均被排除时运行；
        // 因此失败会让浏览器前镜像保持完整。
        commit()?;
        // A published provider projection is a new authoritative snapshot and must outrank the
        // previous WebUI revision. No fallible operation remains after these assignments.
        // 已发布的供应商投影是新的权威快照，必须以更高版本覆盖 WebUI 中的旧快照；
        // 以下赋值之后不再存在可失败操作。
        guard.workspace_snapshot = planned_workspace;
        if let Some(planned_live_context_budgets) = planned_live_context_budgets {
            guard.live_context_budgets = planned_live_context_budgets;
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        planned_workbench.revision = guard.workbench_generation;
        guard.workbench = planned_workbench;
        // Committing after both assignments makes lease deletion inseparable from state publication.
        // 在两项赋值之后提交，使租约删除与状态发布不可分割。
        if let Some(lease_guard) = lease_guard {
            lease_guard.commit();
        }
        // persistenceContext captures only exact directory and addon payloads after the state swap.
        // persistenceContext 只在状态交换后捕获精确目录与附属状态载荷。
        let persistence_context = if changed_session_ids.is_empty() {
            None
        } else {
            let data_root = guard
                .controller_runtime
                .as_ref()
                .map(|runtime| runtime.data_root.clone())
                .ok_or_else(|| String::from("controller runtime is not ready"))?;
            let mut payloads = Vec::with_capacity(changed_session_ids.len());
            for session_id in changed_session_ids {
                let Some(addon_state) = guard.workspace_snapshot.as_ref().and_then(|workspace| {
                    crate::session_addon_state_from_workspace_snapshot(
                        workspace,
                        session_id,
                        &[crate::SessionAddonNamespace::RuntimeControls],
                    )
                }) else {
                    continue;
                };
                let session_directory =
                    crate::require_segmented_session_directory_for_retained_session(
                        &guard, &data_root, session_id,
                    )?;
                payloads.push((session_directory, session_id, addon_state));
            }
            Some(payloads)
        };
        (guard.workbench.clone(), persistence_context)
    };
    if let Some(payloads) = persistence_context {
        for (session_directory, session_id, addon_state) in payloads {
            crate::spawn_exact_session_addon_state_persist_task(
                app.clone(),
                state.clone(),
                session_directory,
                session_id,
                addon_state,
                vec![crate::SessionAddonNamespace::RuntimeControls],
                Some(String::from(persistence_reason)),
            );
        }
    }
    emit_workbench_event(app, &workbench);
    Ok(workbench)
}

/// Removes one connection and all preferences that reference it.
/// 移除一个连接以及引用它的全部偏好。
fn remove_connection_from_configuration(
    configuration: &mut PersistedAiConfiguration,
    connection_id: &str,
    replacement_default_connection_id: Option<&str>,
) -> Result<(), String> {
    let removed = configuration
        .connection_graph
        .connections
        .get(connection_id)
        .cloned()
        .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
    let deleting_provider_default = configuration
        .connection_graph
        .account_preferences
        .provider_defaults
        .get(removed.provider_id.as_str())
        .is_some_and(|value| value == connection_id);
    let remaining_same_provider = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| {
            connection.provider_id == removed.provider_id && connection.id != connection_id
        })
        .filter(|connection| {
            !configuration.connection_is_pending_validation(connection.id.as_str())
        })
        .collect::<Vec<_>>();
    let replacement = if deleting_provider_default && !remaining_same_provider.is_empty() {
        let replacement_id = replacement_default_connection_id.ok_or_else(|| {
            format!(
                "Deleting default connection `{connection_id}` requires an explicit replacement default."
            )
        })?;
        Some(remaining_same_provider
            .iter()
            .find(|connection| connection.id == replacement_id)
            .map(|connection| connection.id.clone())
            .ok_or_else(|| {
                format!(
                    "Replacement connection `{replacement_id}` is not a remaining connection for provider `{}`.",
                    removed.provider_id
                )
            })?)
    } else {
        if replacement_default_connection_id.is_some() {
            return Err(String::from(
                "A replacement default is only accepted when deleting the current provider default.",
            ));
        }
        None
    };
    configuration
        .connection_graph
        .connections
        .remove(connection_id);
    configuration
        .pending_validation_connection_ids
        .remove(connection_id);
    configuration
        .connection_graph
        .account_preferences
        .model_last_connections
        .retain(|_, value| value != connection_id);
    if deleting_provider_default {
        match replacement {
            Some(replacement) => {
                configuration
                    .connection_graph
                    .account_preferences
                    .provider_defaults
                    .insert(removed.provider_id.clone(), replacement);
            }
            None => {
                configuration
                    .connection_graph
                    .account_preferences
                    .provider_defaults
                    .remove(removed.provider_id.as_str());
            }
        }
    }
    if configuration
        .active
        .as_ref()
        .is_some_and(|active| active.connection_id == connection_id)
    {
        configuration.active = None;
    }
    configuration.secondary_routes.retain(|key, _| {
        !key.starts_with(format!("{}:{connection_id}:", connection_id.len()).as_str())
    });
    configuration
        .custom_model_connection_enabled_overrides
        .retain(|key, _| {
            !key.starts_with(format!("{}:{connection_id}:", connection_id.len()).as_str())
        });
    configuration.remove_connection_model_access_policy(connection_id);
    configuration
        .provider_tool_selections
        .retain(|_, selection| selection.connection_id != connection_id);
    // Media backends and native extension sets freeze exact accounts; deleting an account clears
    // only those references and deliberately leaves the independent capability switches unchanged.
    // 媒体后端与原生扩展集合冻结精确账号；删除账号时仅清理这些引用，并有意保留独立能力开关不变。
    configuration
        .media_understanding_selections
        .retain(|_, backend| match backend {
            MediaUnderstandingBackendSelection::ProviderTool { selection } => {
                selection.connection_id != connection_id
            }
            MediaUnderstandingBackendSelection::ModelReplacement { selection, .. } => {
                selection.connection_id != connection_id
            }
        });
    configuration
        .media_understanding_backend_options
        .retain(|_, options| {
            if options
                .provider_tool
                .as_ref()
                .is_some_and(|selection| selection.connection_id == connection_id)
            {
                options.provider_tool = None;
            }
            if options
                .model_replacement
                .as_ref()
                .is_some_and(|route| route.selection.connection_id == connection_id)
            {
                options.model_replacement = None;
            }
            !options.is_empty()
        });
    for selections in configuration.provider_extension_target_sets.values_mut() {
        selections.retain(|selection| selection.connection_id != connection_id);
    }
    configuration
        .provider_extension_target_sets
        .retain(|_, selections| !selections.is_empty());
    Ok(())
}

/// Validates that one exact identifier owns a configured user-defined provider.
/// 校验一个精确标识持有已配置的用户自定义供应商。
fn validate_custom_provider_identity(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
) -> Result<(), String> {
    let provider = configuration
        .custom_providers
        .get(provider_id)
        .ok_or_else(|| format!("Custom provider `{provider_id}` is not configured."))?;
    if provider.kind != ProviderKind::Custom {
        return Err(format!(
            "Provider `{provider_id}` is not a user-defined provider."
        ));
    }
    Ok(())
}

/// Validates one custom-provider removal target, including an isolated standalone reference.
/// 校验一个自定义供应商删除目标，包括已隔离的独立引用。
///
/// # Parameters
/// 参数
/// `configuration` owns the exact reference and hydrated descriptor sets; `provider_id` is the
/// requested deletion identity.
/// `configuration` 持有精确引用集与已补水描述符集；`provider_id` 是请求删除的身份。
///
/// # Returns
/// 返回值
/// `true` for an exact isolated reference, `false` for a healthy custom provider, or an error.
/// 精确隔离引用返回 `true`，健康自定义供应商返回 `false`，否则返回错误。
fn validate_removable_custom_provider_identity(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
) -> Result<bool, String> {
    validate_custom_provider_identifier(provider_id)?;
    let isolated = configuration.custom_provider_manifest_is_isolated(provider_id);
    if !isolated {
        validate_custom_provider_identity(configuration, provider_id)?;
    }
    Ok(isolated)
}

/// Validates that one custom-provider model exists in a current catalog or manual declaration.
/// 校验一个自定义供应商模型存在于当前目录或手工声明中。
fn validate_custom_provider_model_exists(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
) -> Result<(), String> {
    if runtime.provider_model_is_supported(configuration, provider_id, model_id)? {
        Ok(())
    } else {
        Err(format!(
            "Custom provider `{provider_id}` does not expose model `{model_id}`."
        ))
    }
}

/// Validates a persisted template decision against the immutable local suggestion catalog.
/// 根据不可变本地建议目录校验持久化模板决策。
fn validate_custom_template_decision(
    model_id: &str,
    decision: &CustomModelTemplateDecision,
) -> Result<(), String> {
    decision.validate()?;
    let suggestions = vulcan_provider_adapters::suggest_common_model_templates(model_id);
    let matches = match decision {
        CustomModelTemplateDecision::Applied { selection } => suggestions.iter().any(|candidate| {
            candidate.template_key == selection.template_key
                && candidate.template_revision == selection.template_revision
                && candidate.selection() == *selection
        }),
        CustomModelTemplateDecision::Rejected {
            template_key,
            template_revision,
        } => suggestions.iter().any(|candidate| {
            candidate.template_key == *template_key
                && candidate.template_revision == *template_revision
        }),
    };
    if matches {
        Ok(())
    } else {
        Err(format!(
            "The reviewed common template decision no longer matches model `{model_id}`."
        ))
    }
}

/// Rejects empty browser string fields before mutation.
/// 在变更前拒绝浏览器传入的空字符串字段。
fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Field `{field}` must not be empty."));
    }
    Ok(())
}

/// Validates and keys one complete per-model protocol preference set.
/// 校验并建键一组完整的逐模型协议偏好。
///
/// # Parameters
/// 参数
/// `provider_id` labels the exact custom provider, `enabled_protocols` is its authoritative
/// enabled protocol set, and `preferences` is the complete replacement set.
/// `provider_id` 标识精确自定义供应商，`enabled_protocols` 是其权威启用协议集合，
/// `preferences` 是完整替换集合。
///
/// # Returns
/// 返回值
/// A collision-resistant provider-model keyed map, or one actionable validation message.
/// 返回抗冲突的供应商模型组合键映射，或一条可操作的校验消息。
fn validate_custom_model_protocol_preferences(
    provider_id: &str,
    enabled_protocols: &BTreeSet<String>,
    preferences: &[PublicAiCustomModelProtocolPreferenceItem],
) -> Result<BTreeMap<String, CustomModelProtocolPreference>, String> {
    let mut keyed = BTreeMap::new();
    for item in preferences {
        validate_non_empty("preferences[].modelId", item.model_id.as_str())?;
        if let Some(protocol) = item.preference.explicit_protocol()
            && !enabled_protocols.contains(protocol.profile_id())
        {
            return Err(format!(
                "Model `{}` selects protocol `{}` which is not enabled for provider `{}`.",
                item.model_id,
                protocol.profile_id(),
                provider_id
            ));
        }
        let key = PersistedAiConfiguration::provider_model_key(provider_id, item.model_id.as_str());
        if keyed.insert(key, item.preference).is_some() {
            return Err(format!(
                "Custom provider `{provider_id}` contains duplicate preference for model `{}`.",
                item.model_id
            ));
        }
    }
    Ok(keyed)
}

#[cfg(test)]
mod context_compaction_window_node_tests {
    use super::*;
    use vulcan_context::ContextCompactionWindowNodeId;
    use vulcan_workbench_model::SessionRuntimeStateEntry;

    /// Verifies changed global node values invalidate only locked active and archived budgets.
    /// 验证全局节点数值变化只使已锁定活动与归档会话的预算失效。
    #[test]
    fn changed_node_values_invalidate_locked_session_budgets_only() {
        // activeLockedId identifies a visible session whose node resolves through global values.
        // activeLockedId 标识节点通过全局数值解析的可见会话。
        let active_locked_id = crate::SessionId::from_u128(0x6201);
        // activeUnlockedId identifies a visible default-window session that must remain untouched.
        // activeUnlockedId 标识必须保持不变的可见默认窗口会话。
        let active_unlocked_id = crate::SessionId::from_u128(0x6202);
        // archivedLockedId proves archived session state follows the same invalidation contract.
        // archivedLockedId 证明归档会话状态遵循相同失效契约。
        let archived_locked_id = crate::SessionId::from_u128(0x6203);
        // lockedState carries both budget and assembly fingerprints to prove only budget identity changes.
        // lockedState 同时携带预算与组装指纹，用于证明只有预算身份发生变化。
        let locked_state = SessionRuntimeStateEntry {
            context_compaction_window_node_id: Some(
                ContextCompactionWindowNodeId::CompressionWindow2,
            ),
            live_context_budget_fingerprint: Some(String::from("live-budget")),
            live_context_assembly_fingerprint: Some(String::from("live-assembly")),
            anchor_context_budget_fingerprint: Some(String::from("anchor-budget")),
            anchor_context_assembly_fingerprint: Some(String::from("anchor-assembly")),
            ..SessionRuntimeStateEntry::default()
        };
        // unlockedState carries stale-looking values that are independent of global node configuration.
        // unlockedState 携带看似过期但与全局节点配置无关的值。
        let unlocked_state = SessionRuntimeStateEntry {
            live_context_budget_fingerprint: Some(String::from("unlocked-live-budget")),
            anchor_context_budget_fingerprint: Some(String::from("unlocked-anchor-budget")),
            ..SessionRuntimeStateEntry::default()
        };
        // workspace contains both active and archived runtime-state stores used by production publication.
        // workspace 包含生产发布使用的活动与归档运行态存储。
        let mut workspace = vulcan_workspace_store::workspace_store::WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([
                (active_locked_id, locked_state.clone()),
                (active_unlocked_id, unlocked_state),
            ]),
            archived_session_runtime_state: BTreeMap::from([(archived_locked_id, locked_state)]),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: active_locked_id,
            hydrated_session_ids: BTreeSet::from([
                active_locked_id,
                active_unlocked_id,
                archived_locked_id,
            ]),
        };

        // invalidatedSessionIds must include every locked state and no default-window state.
        // invalidatedSessionIds 必须包含全部锁定状态，且不包含默认窗口状态。
        let invalidated_session_ids =
            invalidate_context_compaction_window_budget_fingerprints(&mut workspace);

        assert_eq!(
            invalidated_session_ids,
            BTreeSet::from([active_locked_id, archived_locked_id])
        );
        for session_id in [active_locked_id, archived_locked_id] {
            let runtime_state = workspace
                .session_runtime_state
                .get(&session_id)
                .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
                .expect("locked runtime state should remain present");
            assert_eq!(runtime_state.live_context_budget_fingerprint, None);
            assert_eq!(runtime_state.anchor_context_budget_fingerprint, None);
            assert_eq!(
                runtime_state.live_context_assembly_fingerprint.as_deref(),
                Some("live-assembly")
            );
            assert_eq!(
                runtime_state.anchor_context_assembly_fingerprint.as_deref(),
                Some("anchor-assembly")
            );
        }
        let unlocked_runtime_state = workspace
            .session_runtime_state
            .get(&active_unlocked_id)
            .expect("unlocked runtime state should remain present");
        assert_eq!(
            unlocked_runtime_state
                .live_context_budget_fingerprint
                .as_deref(),
            Some("unlocked-live-budget")
        );
        assert_eq!(
            unlocked_runtime_state
                .anchor_context_budget_fingerprint
                .as_deref(),
            Some("unlocked-anchor-budget")
        );
        let reconciliation = PublicAiWorkspaceReconciliation::context_budget_invalidations(
            invalidated_session_ids.clone(),
        );
        assert!(reconciliation.changed_session_ids.is_empty());
        assert!(reconciliation.revoked_session_ids.is_empty());
        assert_eq!(
            reconciliation.invalidated_context_budget_session_ids,
            invalidated_session_ids
        );
    }
}

#[cfg(test)]
mod inference_settings_route_tests {
    use super::*;

    /// Verifies a media capability can be enabled with only an admitted model-replacement route.
    /// 验证媒体能力仅凭已准入的模型代替路由即可启用。
    #[test]
    fn media_capability_backend_accepts_model_replacement_without_provider_target() {
        // selection is the exact provider-model-account route shared by configuration and inventory.
        // selection 是配置与目录共同持有的精确供应商、模型、账号路由。
        let selection = ActiveAiModelSelection {
            provider_id: String::from("replacement-provider"),
            model_id: String::from("replacement-video-model"),
            connection_id: String::from("replacement-account"),
        };
        // configuration enables the one shared candidate switch and selects it for video understanding.
        // configuration 启用唯一共享候选开关，并将其选作视频理解后端。
        let mut configuration = PersistedAiConfiguration::default();
        configuration.set_multimodal_understanding_candidate_enabled(
            selection.provider_id.as_str(),
            selection.model_id.as_str(),
            true,
        );
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::VideoUnderstand,
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection: selection.clone(),
                reasoning_effort: None,
            },
        );
        // candidate proves an executable conversation route with video input and stable JSON output.
        // candidate 证明存在支持视频输入和稳定 JSON 输出的可执行会话路由。
        let candidate = PublicAiMediaUnderstandingModelCandidate {
            provider_display_name: String::from("Replacement Provider"),
            service_entry_display_name: String::from("Chat"),
            connection_display_name: String::from("Replacement Account"),
            model_display_name: String::from("Replacement Video Model"),
            selection,
            catalog_generation: 1,
            capabilities: ModelCapabilities {
                operations: vec![AiOperation::ConversationRespond],
                input_resources: vec![ResourceKind::Video],
                structured_output: StructuredOutputCapability::JsonObject,
                ..ModelCapabilities::default()
            },
        };

        assert!(provider_tool_capability_backend_is_available(
            &configuration,
            ProviderToolCapabilityKind::VideoUnderstand,
            &[],
            &[candidate],
        ));
        // search remains target-owned and cannot borrow the unrelated replacement inventory.
        // 搜索仍由目标持有，不能借用无关的模型代替目录。
        assert!(!provider_tool_capability_backend_is_available(
            &configuration,
            ProviderToolCapabilityKind::WebSearch,
            &[],
            &[],
        ));
    }

    /// Verifies account removal clears every exact media and native-extension reference it owns.
    /// 验证账号移除会清理其持有的全部精确媒体与原生扩展引用。
    #[test]
    fn connection_removal_clears_exact_media_and_extension_references() {
        // removedConnection is the only configured account and therefore needs no replacement default.
        // removedConnection 是唯一已配置账号，因此不需要替代默认账号。
        let removed_connection = CredentialConnection {
            id: String::from("removed-account"),
            provider_id: String::from("removed-provider"),
            service_entry_id: String::from("chat"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("removed-credential"),
            display_name: String::from("Removed Account"),
            secret_reference: String::from("removed-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        // removedTarget and retainedTarget prove cleanup is exact rather than capability-wide.
        // removedTarget 与 retainedTarget 证明清理按精确账号执行，而不是整项能力清空。
        let removed_target = ProviderToolTargetSelection {
            connection_id: removed_connection.id.clone(),
            target_id: String::from("removed-target"),
        };
        let retained_target = ProviderToolTargetSelection {
            connection_id: String::from("retained-account"),
            target_id: String::from("retained-target"),
        };
        // configuration contains both media backend modes plus a mixed native-extension set.
        // configuration 同时包含两种媒体后端模式与一个混合原生扩展集合。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .connections
            .insert(removed_connection.id.clone(), removed_connection.clone());
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::ImageUnderstand,
            MediaUnderstandingBackendSelection::ProviderTool {
                selection: removed_target.clone(),
            },
        );
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::VideoUnderstand,
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection: ActiveAiModelSelection {
                    provider_id: removed_connection.provider_id.clone(),
                    model_id: String::from("video-model"),
                    connection_id: removed_connection.id.clone(),
                },
                reasoning_effort: None,
            },
        );
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::AudioUnderstand,
            MediaUnderstandingBackendSelection::ProviderTool {
                selection: retained_target.clone(),
            },
        );
        configuration.media_understanding_backend_options.insert(
            ProviderToolCapabilityKind::ImageUnderstand,
            MediaUnderstandingBackendOptions {
                provider_tool: Some(removed_target.clone()),
                ..MediaUnderstandingBackendOptions::default()
            },
        );
        configuration.media_understanding_backend_options.insert(
            ProviderToolCapabilityKind::VideoUnderstand,
            MediaUnderstandingBackendOptions {
                model_replacement: Some(
                    vulcan_provider_adapters::MediaUnderstandingModelReplacementSelection {
                        selection: ActiveAiModelSelection {
                            provider_id: removed_connection.provider_id.clone(),
                            model_id: String::from("video-model"),
                            connection_id: removed_connection.id.clone(),
                        },
                        reasoning_effort: None,
                    },
                ),
                ..MediaUnderstandingBackendOptions::default()
            },
        );
        configuration.media_understanding_backend_options.insert(
            ProviderToolCapabilityKind::AudioUnderstand,
            MediaUnderstandingBackendOptions {
                provider_tool: Some(retained_target.clone()),
                ..MediaUnderstandingBackendOptions::default()
            },
        );
        configuration.provider_extension_target_sets.insert(
            ProviderToolCapabilityKind::ImageGenerate,
            BTreeSet::from([removed_target, retained_target.clone()]),
        );

        remove_connection_from_configuration(
            &mut configuration,
            removed_connection.id.as_str(),
            None,
        )
        .expect("connection removal should succeed");

        assert!(
            !configuration
                .connection_graph
                .connections
                .contains_key(removed_connection.id.as_str())
        );
        assert_eq!(configuration.media_understanding_selections.len(), 1);
        assert!(
            configuration
                .media_understanding_selections
                .contains_key(&ProviderToolCapabilityKind::AudioUnderstand)
        );
        assert_eq!(configuration.media_understanding_backend_options.len(), 1);
        assert_eq!(
            configuration
                .media_understanding_backend_options
                .get(&ProviderToolCapabilityKind::AudioUnderstand)
                .and_then(|options| options.provider_tool.as_ref()),
            Some(&retained_target)
        );
        assert_eq!(
            configuration
                .provider_extension_target_sets
                .get(&ProviderToolCapabilityKind::ImageGenerate),
            Some(&BTreeSet::from([retained_target]))
        );
    }

    /// Verifies replacement candidates expose only media inputs encoded by the frozen protocol.
    /// 验证模型代替候选只公开冻结协议能够编码的媒体输入。
    #[test]
    fn media_candidate_capabilities_intersect_model_and_protocol_support() {
        // capabilities deliberately over-declares every media kind so protocol projection is observable.
        // capabilities 有意声明全部媒体类型，使协议投影结果可观察。
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            input_resources: vec![
                ResourceKind::Image,
                ResourceKind::Video,
                ResourceKind::Audio,
                ResourceKind::File,
            ],
            ..ModelCapabilities::default()
        };

        // responses retains only its registered image projector and preserves unrelated file metadata.
        // responses 仅保留已注册图片编码器，并保持无关文件元数据。
        let responses = protocol_projected_media_capabilities(
            &capabilities,
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
        );
        assert_eq!(
            responses.input_resources,
            vec![ResourceKind::Image, ResourceKind::File]
        );
        // chat exposes all three media encoders used by model replacement.
        // chat 公开模型代替所需的三种媒体编码器。
        let chat = protocol_projected_media_capabilities(
            &capabilities,
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
        );
        assert_eq!(chat.input_resources, capabilities.input_resources);
    }

    /// Verifies the explicit Anthropic video profile cannot accidentally admit audio replacement.
    /// 验证显式 Anthropic 视频协议不会意外准入音频模型代替。
    #[test]
    fn anthropic_video_candidate_excludes_unregistered_audio_input() {
        // capabilities represents a multimodal model whose wire route is narrower than its catalog metadata.
        // capabilities 表示 Wire 路由窄于目录元数据的多模态模型。
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            input_resources: vec![
                ResourceKind::Image,
                ResourceKind::Video,
                ResourceKind::Audio,
            ],
            structured_output: StructuredOutputCapability::JsonSchema,
            ..ModelCapabilities::default()
        };

        let projected = protocol_projected_media_capabilities(
            &capabilities,
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        );
        assert_eq!(
            projected.input_resources,
            vec![ResourceKind::Image, ResourceKind::Video]
        );
        assert!(model_route_supports_media_understanding(
            &projected,
            ProviderToolCapabilityKind::VideoUnderstand,
        ));
        assert!(!model_route_supports_media_understanding(
            &projected,
            ProviderToolCapabilityKind::AudioUnderstand,
        ));
    }

    /// Verifies Anthropic routes cannot admit schema-free JSON Object output as stable replacement output.
    /// 验证 Anthropic 路由不能把无 Schema 的 JSON Object 输出准入为稳定替代输出。
    #[test]
    fn anthropic_candidate_requires_json_schema_capability() {
        // capabilities uses the product default so the protocol projection must close the mismatch.
        // capabilities 使用产品默认值，因此协议投影必须关闭这一不匹配。
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            input_resources: vec![ResourceKind::Image],
            ..ModelCapabilities::default()
        };

        let projected = protocol_projected_media_capabilities(
            &capabilities,
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
        );
        assert_eq!(
            projected.structured_output,
            StructuredOutputCapability::Unsupported
        );
        assert!(!model_route_supports_media_understanding(
            &projected,
            ProviderToolCapabilityKind::ImageUnderstand,
        ));
    }

    /// Verifies fixed-off routes accept only the explicit no-reasoning effort without a declared effort list.
    /// 验证固定关闭路由无需声明档位列表，但只接受显式无思考档位。
    #[test]
    fn fixed_off_reasoning_accepts_only_none() {
        // capabilities models a route whose control mode itself is the complete authority.
        // capabilities 模拟一条由控制模式本身提供完整权威的路由。
        let capabilities = ModelCapabilities {
            reasoning_control: ReasoningControlMode::FixedOff,
            reasoning_efforts: Vec::new(),
            ..ModelCapabilities::default()
        };

        assert!(validate_internal_reasoning_effort(&capabilities, ReasoningEffort::None).is_ok());
        assert!(validate_internal_reasoning_effort(&capabilities, ReasoningEffort::Low).is_err());
    }

    /// Verifies manifest RPCs accept only identities pinned by the trusted bundled index.
    /// 验证 Manifest RPC 仅接受可信内置 Index 固定的身份。
    #[test]
    fn manifest_rpc_identity_guard_rejects_unknown_and_oversized_values() {
        // runtime supplies the exact trusted system identity set used by production RPCs.
        // runtime 提供生产 RPC 使用的精确可信系统身份集合。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        crate::provider_manifest_sync::require_trusted_system_manifest_provider(
            &runtime,
            "openrouter",
        )
        .expect("known bundled provider must pass");
        let unknown = "x"
            .repeat(vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN + 1);
        let error = crate::provider_manifest_sync::require_trusted_system_manifest_provider(
            &runtime,
            unknown.as_str(),
        )
        .expect_err("unknown oversized identity must fail before any store operation");
        assert!(!error.contains(unknown.as_str()));
        assert!(error.contains("trusted system provider identifier"));
    }

    /// Verifies exact compensation becomes a provider-local rejection instead of a batch error.
    /// 验证精确补偿会成为逐供应商拒绝，而不是批次错误。
    #[test]
    fn compensated_projection_becomes_bounded_provider_rejection() {
        // report represents the staged activation before its Workbench projection fails.
        // report 表示工作台投影失败前的暂存激活。
        let report = crate::provider_manifest_sync::ProviderSyncReport {
            provider_id: String::from("deepseek"),
            outcome: "activated",
            revision: 7,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: None,
        };
        let rejected = compensated_provider_projection_report(
            report,
            "failed to republish C:\\Users\\operator\\workbench.json",
        );
        assert_eq!(rejected.outcome, "rejected");
        assert_eq!(
            rejected.failure_kind.as_deref(),
            Some("activation_publication_failed")
        );
        assert_eq!(
            rejected.failure_message.as_deref(),
            Some("<redacted-local-path>")
        );
        assert!(rejected.activation_snapshot.is_none());
    }

    /// Verifies projection cannot start without the exact activation before-image contract.
    /// 验证缺少精确激活前镜像契约时不得开始投影。
    #[test]
    fn provider_projection_rejects_missing_activation_before_image() {
        // report deliberately violates the internal activated-report contract.
        // report 有意违反内部 activated 报告契约。
        let report = crate::provider_manifest_sync::ProviderSyncReport {
            provider_id: String::from("deepseek"),
            outcome: "activated",
            revision: 7,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: None,
        };
        let provider_ids = std::collections::BTreeSet::from([String::from("deepseek")]);
        let error =
            required_provider_activation_snapshots(std::slice::from_ref(&report), &provider_ids)
                .expect_err("missing exact before-image must fail closed");
        assert!(matches!(
            error,
            ProviderProjectionPublicationFailure::Uncertain(message)
                if message.contains("missing its exact before-image")
        ));
    }

    /// Verifies dropping an armed publication guard restores disk and staged runtime ownership.
    /// 验证丢弃已启用发布护栏会恢复磁盘与暂存运行时所有权。
    #[test]
    fn cancelled_provider_publication_restores_staged_before_images() {
        use sha2::Digest;

        // directory owns the exact durable transaction inspected after cancellation.
        // directory 持有取消后检查的精确持久化事务。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // service owns compensation for the staged file transaction.
        // service 持有已暂存文件事务的补偿责任。
        let service = crate::provider_manifest_sync::ProviderManifestSyncService::new(
            directory.path(),
            reqwest::Client::new(),
            Vec::new(),
            None,
        );
        // runtime owns the immutable bundled baseline and mutable staging catalog.
        // runtime 持有不可变内置基线与可变暂存目录。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // bundled_revision is the exact runtime identity before staging.
        // bundled_revision 是暂存前的精确运行时身份。
        let bundled_revision = runtime
            .system_manifest("deepseek")
            .expect("bundled DeepSeek manifest should exist")
            .revision;
        // candidate_value starts from the trusted bundled document and changes only revision.
        // candidate_value 从可信内置文档开始，并且只修改修订号。
        let mut candidate_value = serde_json::from_str::<serde_json::Value>(
            std::fs::read_to_string(
                vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                    .join("deepseek.json"),
            )
            .expect("bundled DeepSeek document should read")
            .as_str(),
        )
        .expect("bundled DeepSeek document should parse");
        // candidate_revision is strictly newer than the bundled authority.
        // candidate_revision 严格晚于内置权威。
        let candidate_revision = bundled_revision + 1;
        candidate_value["revision"] = serde_json::json!(candidate_revision);
        // candidate_bytes are the exact durable candidate payload.
        // candidate_bytes 是精确持久化候选载荷。
        let candidate_bytes =
            serde_json::to_vec(&candidate_value).expect("candidate should encode");
        // digest pins the exact candidate byte identity.
        // digest 固定精确候选字节身份。
        let digest = sha2::Sha256::digest(&candidate_bytes);
        // entry is the authenticated remote pin consumed by staged activation.
        // entry 是已暂存激活消费的认证远端固定项。
        let entry = vulcan_provider_config::manifest_sync::RemoteProviderEntry {
            provider_id: String::from("deepseek"),
            revision: candidate_revision,
            relative_url: String::from("/deepseek.json"),
            sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
            bytes: candidate_bytes.len() as u64,
            minimum_engine_contract: 1,
        };
        // outcome retains the file before-image and compiled staged manifest.
        // outcome 保留文件前镜像与已编译暂存 Manifest。
        let outcome = vulcan_provider_config::manifest_sync::stage_synced_provider_activation(
            &vulcan_provider_config::manifest_store::ManifestStoreLayout::new(directory.path()),
            &entry,
            &candidate_bytes,
            None,
            None,
            None,
        )
        .expect("candidate should stage");
        // activation_parts are the exact ownership transition required by the guard.
        // activation_parts 是护栏所需的精确所有权转换。
        let (manifest, store_snapshot) = match outcome {
            vulcan_provider_config::manifest_sync::SyncOutcome::Activated {
                manifest,
                store_snapshot,
                ..
            } => (manifest, store_snapshot),
            _ => panic!("expected staged activation"),
        };
        // staged_manifest_mutation is the runtime CAS before/after image.
        // staged_manifest_mutation 是运行时 CAS 前后镜像。
        let staged_manifest_mutation = runtime
            .replace_system_manifest_transactional("deepseek", manifest)
            .expect("runtime manifest should stage");
        // reports and snapshots form the complete cancellation transaction.
        // reports 与 snapshots 构成完整取消事务。
        let snapshots = vec![
            crate::provider_manifest_sync::ProviderManifestActivationSnapshot {
                provider_id: String::from("deepseek"),
                store: store_snapshot,
                staged_manifest_mutation,
            },
        ];
        let reports = vec![crate::provider_manifest_sync::ProviderSyncReport {
            provider_id: String::from("deepseek"),
            outcome: "activated",
            revision: candidate_revision,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: Some(snapshots[0].clone()),
        }];

        {
            // guard emulates an RPC Future dropped at an asynchronous projection boundary.
            // guard 模拟在异步投影边界被丢弃的 RPC Future。
            let _guard = StagedProviderActivationCancellationGuard::new(
                &service, &runtime, &reports, &snapshots,
            );
        }

        // store proves both the Active before-image and Applying journal were restored/cleared.
        // store 证明 Active 前镜像与 Applying 日志均已恢复/清除。
        let store =
            vulcan_provider_config::manifest_store::ManifestStoreLayout::new(directory.path());
        assert_eq!(
            store.read_active("deepseek").expect("active should read"),
            None
        );
        assert!(
            !store
                .recover_system_transaction("deepseek")
                .expect("journal probe should succeed")
        );
        assert_eq!(
            runtime
                .system_manifest("deepseek")
                .expect("restored DeepSeek manifest should exist")
                .revision,
            bundled_revision
        );
    }

    /// Verifies browser settings retain diagnostics without projecting an isolated provider account.
    /// 验证浏览器设置会保留诊断，同时不投影已隔离供应商账号。
    #[test]
    fn settings_snapshot_excludes_isolated_custom_provider_connection() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let provider_id = "custom:isolated-settings";
        let connection_id = "isolated-settings-account";
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_provider_file_refs
            .insert(provider_id.to_string());
        configuration.connection_graph.connections.insert(
            connection_id.to_string(),
            CredentialConnection {
                id: connection_id.to_string(),
                provider_id: provider_id.to_string(),
                service_entry_id: String::from("default"),
                credential_profile_id: String::from("api-key"),
                credential_mode: CredentialMode::Token,
                credential_id: String::from("isolated-settings-credential"),
                display_name: String::from("Isolated Settings Account"),
                secret_reference: String::from("isolated-settings-secret"),
                management_secret_reference: None,
                service_origin_override: None,
                prompt_cache_by_protocol: BTreeMap::new(),
            },
        );
        configuration.custom_provider_manifest_failures = vec![CustomProviderManifestFailure {
            provider_id: provider_id.to_string(),
            message: String::from("invalid standalone document"),
        }];
        configuration.custom_provider_manifest_failure_total_count = 1;

        let snapshot = build_public_ai_settings_snapshot(&runtime, &configuration)
            .expect("healthy settings snapshot should remain available");

        assert!(snapshot.connections.is_empty());
        assert!(snapshot.catalogs.is_empty());
        assert_eq!(snapshot.custom_provider_manifest_failure_total_count, 1);
        assert_eq!(
            snapshot.custom_provider_manifest_failures[0].provider_id,
            provider_id
        );
        assert!(
            configuration
                .connection_graph
                .connections
                .contains_key(connection_id),
            "the persisted repair authority must remain unchanged"
        );
    }

    /// Verifies removal accepts an exact isolated custom reference but never a system namespace.
    /// 验证删除接受精确隔离自定义引用，但绝不接受系统命名空间。
    #[test]
    fn isolated_custom_provider_removal_identity_is_namespace_safe() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_provider_file_refs
            .insert(String::from("custom:isolated-removal"));
        configuration
            .custom_provider_file_refs
            .insert(String::from("openai"));

        assert_eq!(
            validate_removable_custom_provider_identity(&configuration, "custom:isolated-removal"),
            Ok(true)
        );
        let error = validate_removable_custom_provider_identity(&configuration, "openai")
            .expect_err("system provider identity must never enter custom removal");
        assert!(error.contains("custom:"), "{error}");
    }

    /// Verifies connection mutation RPCs never accept a user-selected protocol authority.
    /// 验证连接变更 RPC 绝不接受用户选择的协议权威。
    #[test]
    fn connection_requests_reject_user_selected_protocol() {
        // requests are the exact browser payloads after connection-level protocol selection was removed.
        // requests 是移除连接级协议选择后的精确浏览器载荷。
        let requests = [
            (
                "add",
                serde_json::json!({
                    "providerId": "openrouter",
                    "serviceEntryId": "public-api",
                    "credentialProfileId": "api-key",
                    "credentialMode": "token",
                    "displayName": "Primary",
                    "secret": "test-secret"
                }),
            ),
            (
                "update",
                serde_json::json!({
                    "connectionId": "connection",
                    "displayName": "Primary",
                    "serviceEntryId": "public-api",
                    "credentialProfileId": "api-key",
                    "credentialMode": "token"
                }),
            ),
            (
                "oauth",
                serde_json::json!({
                    "transactionId": "transaction",
                    "providerId": "openai",
                    "serviceEntryId": "codex-subscription",
                    "credentialProfileId": "chatgpt-oauth",
                    "displayName": "Subscription"
                }),
            ),
        ];

        for (kind, request) in requests {
            let accepted = match kind {
                "add" => serde_json::from_value::<PublicAiConnectionAddRequest>(request.clone())
                    .map(|_| ()),
                "update" => {
                    serde_json::from_value::<PublicAiConnectionUpdateRequest>(request.clone())
                        .map(|_| ())
                }
                "oauth" => serde_json::from_value::<PublicAiOAuthCommitRequest>(request.clone())
                    .map(|_| ()),
                _ => unreachable!("closed request fixture kind"),
            };
            accepted.unwrap_or_else(|error| panic!("{kind} request should deserialize: {error}"));

            let mut forbidden = request;
            forbidden["selectedProtocol"] =
                serde_json::Value::String(String::from("openai-chat-completions.v1"));
            let rejected = match kind {
                "add" => {
                    serde_json::from_value::<PublicAiConnectionAddRequest>(forbidden).map(|_| ())
                }
                "update" => {
                    serde_json::from_value::<PublicAiConnectionUpdateRequest>(forbidden).map(|_| ())
                }
                "oauth" => {
                    serde_json::from_value::<PublicAiOAuthCommitRequest>(forbidden).map(|_| ())
                }
                _ => unreachable!("closed request fixture kind"),
            };
            let error = rejected.expect_err("selectedProtocol must be rejected");
            assert!(error.to_string().contains("selectedProtocol"));
        }
    }

    /// Builds one exact internal inference route for settings-boundary regression tests.
    /// 为设置边界回归测试构建一条精确内部推理路由。
    ///
    /// # Parameters
    /// 参数
    /// `operation` and `usage_scope` define the exact persisted route facts.
    /// `operation` 与 `usage_scope` 定义精确持久化路由事实。
    ///
    /// # Returns
    /// 返回值
    /// A deterministic route with a native action binding.
    /// 返回带原生动作绑定的确定性路由。
    fn route_fixture(
        operation: AiOperation,
        usage_scope: InferenceUsageScope,
    ) -> InferenceRouteConfiguration {
        InferenceRouteConfiguration {
            provider_id: String::from("provider"),
            service_entry_id: String::from("entry"),
            connection_id: String::from("connection"),
            model_id: String::from("model"),
            binding: InferenceRouteBinding::Action {
                action_binding_id: String::from("action"),
            },
            operation,
            usage_scope,
        }
    }

    /// Verifies a settings test retains the persisted route usage scope exactly.
    /// 验证设置测试会精确保留持久化路由用途范围。
    #[test]
    fn settings_test_route_retains_exact_usage_scope() {
        let expected = route_fixture(
            AiOperation::EmbeddingCreate,
            InferenceUsageScope::InternalBackground,
        );
        let mut configuration = PersistedAiConfiguration::default();
        configuration.inference_profiles.insert(
            String::from("vmm"),
            InferenceProfile {
                embedding: Some(expected.clone()),
                ..InferenceProfile::default()
            },
        );

        let actual = configured_settings_inference_test_route(
            &configuration,
            "vmm",
            AiOperation::EmbeddingCreate,
        )
        .expect("the exact embedding route should resolve");

        assert_eq!(actual, expected);
    }

    /// Verifies persisted direct settings remain visible without an account catalog.
    /// 验证没有账号目录时持久化直接设置仍保持可见。
    #[test]
    fn settings_snapshot_projects_capability_overrides_without_catalogs() {
        // runtime supplies the built-in provider registry without requiring network state.
        // runtime 提供内置供应商注册表且不需要网络状态。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // configuration represents a credential-free custom provider with one saved model difference.
        // configuration 表示一个无凭据自定义供应商及一项已保存模型差异。
        let mut configuration = PersistedAiConfiguration::default();
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
            ModelCapabilityOverrides {
                context_limit: Some(16_384),
                output_limit: Some(4_096),
                ..ModelCapabilityOverrides::default()
            },
        );
        runtime
            .replace_configuration(configuration.clone())
            .expect("capability overrides should cross the settings activation boundary");

        // snapshot must expose the record directly instead of depending on a visible connection model.
        // snapshot 必须直接公开该记录，而不是依赖可见的连接模型。
        let snapshot = build_public_ai_settings_snapshot(&runtime, &configuration)
            .expect("settings snapshot should build");

        assert_eq!(snapshot.model_capability_override_records.len(), 1);
        let record = &snapshot.model_capability_override_records[0];
        assert_eq!(record.provider_id, "custom:test");
        assert_eq!(record.model_id, "private/chat");
        assert_eq!(record.overrides.context_limit, Some(16_384));
        assert_eq!(record.overrides.output_limit, Some(4_096));
    }

    /// Verifies provider-tool template models remain extension-manageable without becoming conversation models.
    /// 验证供应商工具模板模型保持可由扩展管理且不会成为会话模型。
    #[test]
    fn settings_snapshot_separates_provider_tool_templates_from_conversation_access() {
        // runtime supplies the bundled Alibaba template catalog without network access.
        // runtime 在不访问网络的情况下提供内置阿里云模板目录。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // configuration owns one eligible Model Studio credential and one explicit model closure.
        // configuration 持有一个合格百炼凭据与一个显式模型关闭选择。
        let mut configuration = PersistedAiConfiguration::default();
        // connection binds the exact service and credential profile that own the reviewed templates.
        // connection 绑定拥有已审核模板的精确服务与凭据配置档。
        let connection = CredentialConnection {
            id: String::from("alibaba-template-account"),
            provider_id: String::from("alibaba"),
            service_entry_id: String::from("model-studio-cn"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("alibaba-template-credential"),
            display_name: String::from("Alibaba template account"),
            secret_reference: String::from("alibaba-template-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection,
            )
            .expect("Alibaba template connection should register");
        configuration.set_model_enabled("alibaba", "qwen-image-2.0-pro", false);
        runtime
            .replace_configuration(configuration.clone())
            .expect("template access should cross the settings activation boundary");

        // snapshot must project declarations and selectable extension targets from one generation.
        // snapshot 必须从同一代际投影声明与可选择的扩展目标。
        let snapshot = build_public_ai_settings_snapshot(&runtime, &configuration)
            .expect("settings snapshot should build");
        assert!(snapshot.provider_tool_template_models.iter().any(|model| {
            model.provider_id == "alibaba" && model.model_id == "qwen-image-2.0-pro"
        }));
        assert!(!snapshot.model_access_records.iter().any(|record| {
            record.provider_id == "alibaba" && record.model_id == "qwen-image-2.0-pro"
        }));
        assert!(snapshot.model_route_records.iter().all(|record| {
            record.provider_id != "alibaba" || record.model_id != "qwen-image-2.0-pro"
        }));
        assert!(
            snapshot
                .provider_tool_targets
                .iter()
                .any(|available| { available.target.upstream_target_id == "qwen-image-2.0-pro" })
        );
    }

    /// Verifies internal conversation candidates consume the frozen model binding.
    /// 验证内部对话候选消费冻结的模型绑定。
    #[test]
    fn internal_conversation_candidate_uses_model_protocol_binding() {
        // runtime supplies the signed OpenRouter registry without external network access.
        // runtime 在不访问外部网络的情况下提供已签名 OpenRouter 注册表。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // connection owns credential identity only; the model binding owns protocol selection.
        // connection 只持有凭据身份；模型绑定持有协议选择。
        let connection = CredentialConnection {
            id: String::from("openrouter-model-binding-account"),
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("openrouter-model-binding-credential"),
            display_name: String::from("OpenRouter model binding account"),
            secret_reference: String::from("openrouter-model-binding-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        // capabilities expose one internal conversation operation through the frozen variant.
        // capabilities 通过冻结变体公开一项内部对话操作。
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            ..ModelCapabilities::default()
        };
        // model binds Responses even though the credential-level compatibility field is Chat.
        // model 在凭据级兼容字段为 Chat 时仍绑定 Responses。
        let model = vulcan_provider_adapters::ConnectionModel {
            id: String::from("openai/gpt-model-bound-to-responses"),
            display_name: String::from("Responses-bound model"),
            capabilities: capabilities.clone(),
            capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::from([(
                String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
                vulcan_provider_adapters::ProtocolModelVariant {
                    capabilities,
                    capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(
                    ),
                    discovery_signature: String::from("responses-bound-model-test"),
                    catalog_generation: 7,
                },
            )]),
            resolved_protocol_binding: Some(vulcan_provider_adapters::ModelProtocolBinding {
                protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
                source: vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest,
                manifest_revision: 2,
                rule_id: String::from("responses-bound-model-rule"),
            }),
        };
        // mediaCapabilities exposes only image generation and must never become a conversation row.
        // mediaCapabilities 仅公开图片生成，绝不能成为会话行。
        let media_capabilities = ModelCapabilities {
            operations: vec![AiOperation::ImageGenerate],
            ..ModelCapabilities::default()
        };
        // mediaModel deliberately owns a valid protocol binding to prove binding alone does not
        // authorize conversation publication.
        // mediaModel 有意持有有效协议绑定，以证明仅有绑定并不能授权会话发布。
        let media_model = vulcan_provider_adapters::ConnectionModel {
            id: String::from("image-generation-only"),
            display_name: String::from("Image generation only"),
            capabilities: media_capabilities.clone(),
            capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::from([(
                String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
                vulcan_provider_adapters::ProtocolModelVariant {
                    capabilities: media_capabilities,
                    capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(
                    ),
                    discovery_signature: String::from("responses-media-model-test"),
                    catalog_generation: 7,
                },
            )]),
            resolved_protocol_binding: Some(vulcan_provider_adapters::ModelProtocolBinding {
                protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
                source: vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest,
                manifest_revision: 2,
                rule_id: String::from("responses-media-model-rule"),
            }),
        };
        // configuration owns the exact credential identity used by the catalog.
        // configuration 持有目录使用的精确凭据身份。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection.clone(),
            )
            .expect("OpenRouter connection should register");
        // Independent switches create the exact internal-only state from the acceptance matrix.
        // 两个独立开关构造验收矩阵中的精确“仅内部使用”状态。
        configuration.set_model_enabled("openrouter", "openai/gpt-model-bound-to-responses", false);
        configuration.set_internal_model_enabled(
            "openrouter",
            "openai/gpt-model-bound-to-responses",
            true,
        );
        // catalogs contains the coherent frozen account snapshot under review.
        // catalogs 包含本次检查的一致冻结账号快照。
        let catalogs = BTreeMap::from([(
            connection.id.clone(),
            ConnectionModelCatalog {
                connection_id: connection.id,
                models: vec![model, media_model],
                generation: 7,
                page_count: 1,
                discovery_schema_revision: String::from("model-binding-test-v1"),
            },
        )]);

        // candidates must persist Responses and must never inherit the Chat connection field.
        // candidates 必须持久化 Responses，且绝不能继承连接上的 Chat 字段。
        let candidates = build_internal_inference_candidates(
            &configuration,
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            &catalogs,
        )
        .expect("internal candidates should build");
        assert_eq!(
            candidates.len(),
            2,
            "one conversation model should project both supported internal usage scopes"
        );
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.route.model_id != "image-generation-only"),
            "generation-only models must not become internal LLM candidates"
        );
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.route.operation == AiOperation::ConversationRespond)
            .expect("conversation candidate should exist");
        assert_eq!(
            candidate.route.binding,
            InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,),
            }
        );
        assert!(
            candidate.enabled,
            "conversation disablement must not hide an explicitly enabled internal model"
        );
        configuration.set_internal_model_enabled(
            "openrouter",
            "openai/gpt-model-bound-to-responses",
            false,
        );
        // disabledCandidates proves an explicit internal denial wins over the provider default
        // without removing manageable route identities.
        // disabledCandidates 证明显式内部禁用优先于供应商默认值，且不会删除可管理路由身份。
        let disabled_candidates = build_internal_inference_candidates(
            &configuration,
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            &catalogs,
        )
        .expect("disabled internal candidates should remain manageable");
        assert!(
            disabled_candidates
                .iter()
                .all(|candidate| !candidate.enabled)
        );
        // route_records must expose the same frozen protocol authority and its complete provenance.
        // route_records 必须公开同一冻结协议权威及其完整溯源。
        let route_records = build_public_ai_model_route_records(
            &configuration,
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            &catalogs,
            &BTreeMap::from([(String::from("openrouter"), 2)]),
        );
        assert_eq!(route_records.len(), 1);
        // route_record is the exact settings DTO row for the tested account and model.
        // route_record 是测试账号与模型对应的精确设置 DTO 行。
        let route_record = &route_records[0];
        assert_eq!(
            route_record.protocol_id,
            Some(String::from(
                vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
            ))
        );
        assert_eq!(
            route_record.available_protocols,
            vec![String::from(
                vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
            )]
        );
        assert_eq!(
            route_record.binding_source.as_deref(),
            Some("system_manifest")
        );
        assert_eq!(
            route_record.binding_rule_id.as_deref(),
            Some("responses-bound-model-rule")
        );
        assert_eq!(route_record.manifest_revision, 2);
        assert!(route_record.executable);
        assert!(route_record.unavailable_reason.is_none());
    }

    /// Verifies blocked custom models remain visible with an exact backend-owned reason.
    /// 验证被阻止的自定义模型仍保持可见，并携带精确的后端原因。
    #[test]
    fn custom_model_route_status_exposes_model_specific_protocol_block() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let provider_id = String::from("custom:model-route-status");
        let descriptor = CustomProviderDefinition {
            id: provider_id.clone(),
            display_name: String::from("Model Route Status"),
            base_url: String::from("https://model-route-status.example.test/v1"),
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocols: vec![
                CustomProviderProtocol::OpenAiResponses,
                CustomProviderProtocol::AnthropicMessages,
            ],
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("multi-protocol custom provider should compile");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(provider_id.clone(), descriptor);
        let connection = CredentialConnection {
            id: String::from("model-route-status-account"),
            provider_id: provider_id.clone(),
            service_entry_id: String::from("default"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("model-route-status-credential"),
            display_name: String::from("Model Route Status Account"),
            secret_reference: String::from("model-route-status-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        let providers = runtime
            .effective_providers(&configuration)
            .expect("effective custom providers should resolve");
        configuration
            .connection_graph
            .add_connection(&providers, connection.clone())
            .expect("custom connection should register");
        let model_id = String::from("anthropic-only/model");
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key(provider_id.as_str(), model_id.as_str()),
            CustomModelProtocolPreference::Explicit {
                protocol: CustomProviderProtocol::OpenAiResponses,
            },
        );
        let model = vulcan_provider_adapters::ConnectionModel {
            id: model_id.clone(),
            display_name: String::from("Anthropic-only Model"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::from([(
                String::from(vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID),
                vulcan_provider_adapters::ProtocolModelVariant {
                    capabilities: ModelCapabilities::default(),
                    capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(
                    ),
                    discovery_signature: String::from("anthropic-only-route-status"),
                    catalog_generation: 1,
                },
            )]),
            resolved_protocol_binding: None,
        };
        let catalogs = BTreeMap::from([(
            connection.id.clone(),
            ConnectionModelCatalog {
                connection_id: connection.id,
                models: vec![model],
                generation: 1,
                page_count: 1,
                discovery_schema_revision: String::from("custom-route-status-v1"),
            },
        )]);

        let records = build_public_ai_model_route_records(
            &configuration,
            &providers,
            &catalogs,
            &BTreeMap::from([(provider_id, 0)]),
        );
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert!(!record.executable);
        assert!(record.protocol_id.is_none());
        assert!(record.binding_source.is_none());
        assert_eq!(
            record.available_protocols,
            vec![String::from(
                vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
            )]
        );
        assert!(
            record
                .unavailable_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("unavailable for this model"))
        );
    }

    /// Verifies an invalid global account rebinds to the same model on its preferred account.
    /// 验证失效的全局账号会重新绑定到首选账号上的同一模型。
    #[test]
    fn invalid_active_account_rebinds_same_model_before_model_fallback() {
        // providerId and modelId identify the user choice that must survive account discovery drift.
        // providerId 与 modelId 标识在账号发现变化后仍必须保留的用户选择。
        let provider_id = String::from("shared-provider");
        let model_id = String::from("shared-model");
        // staleConnection is the credential that no longer exposes the selected model.
        // staleConnection 是已不再公开所选模型的凭据。
        let stale_connection = CredentialConnection {
            id: String::from("stale-connection"),
            provider_id: provider_id.clone(),
            service_entry_id: String::from("shared-entry"),
            credential_profile_id: String::from("shared-profile"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("stale-credential"),
            display_name: String::from("Stale account"),
            secret_reference: String::from("stale-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        // availableConnection is the exact preferred credential that still exposes the model.
        // availableConnection 是仍公开该模型的精确首选凭据。
        let available_connection = CredentialConnection {
            id: String::from("available-connection"),
            provider_id: provider_id.clone(),
            service_entry_id: String::from("shared-entry"),
            credential_profile_id: String::from("shared-profile"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("available-credential"),
            display_name: String::from("Available account"),
            secret_reference: String::from("available-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        // configuration retains the stale exact active account and the valid model-level preference.
        // configuration 保留失效的精确活动账号与有效的模型级账号偏好。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .connections
            .insert(stale_connection.id.clone(), stale_connection.clone());
        configuration.connection_graph.connections.insert(
            available_connection.id.clone(),
            available_connection.clone(),
        );
        configuration
            .connection_graph
            .remember_model_connection(
                provider_id.as_str(),
                model_id.as_str(),
                available_connection.id.as_str(),
            )
            .expect("preferred model account should persist");
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            connection_id: stale_connection.id,
        });
        configuration.set_model_enabled(provider_id.as_str(), model_id.as_str(), true);
        // capabilities prove the surviving model remains a valid conversation model.
        // capabilities 证明保留下来的模型仍是有效会话模型。
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            ..ModelCapabilities::default()
        };
        // catalogs contains the selected model only under the available credential.
        // catalogs 只在可用凭据下包含所选模型。
        let catalogs = BTreeMap::from([(
            available_connection.id.clone(),
            ConnectionModelCatalog {
                connection_id: available_connection.id.clone(),
                models: vec![vulcan_provider_adapters::ConnectionModel {
                    id: model_id.clone(),
                    display_name: String::from("Shared model"),
                    capabilities,
                    capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(
                    ),
                    protocol_variants: BTreeMap::new(),
                    resolved_protocol_binding: None,
                }],
                generation: 2,
                page_count: 1,
                discovery_schema_revision: String::from("account-rebind-test-v1"),
            },
        )]);
        // runtime supplies provider-level access defaults without remote execution.
        // runtime 提供供应商级访问默认值，不执行远程请求。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");

        ensure_active_model_selection(&runtime, &mut configuration, &catalogs)
            .expect("invalid account should rebind");

        assert_eq!(
            configuration.active,
            Some(ActiveAiModelSelection {
                provider_id,
                model_id,
                connection_id: available_connection.id,
            })
        );
    }

    /// Verifies a session picker action preserves the frozen route and records one pending selection.
    /// 验证会话选择器操作会保留冻结路由，并记录一个待生效选择。
    #[test]
    fn session_model_selection_waits_for_next_provider_request_and_is_idempotent() {
        // sessionId identifies the sole workspace owner under test.
        // sessionId 标识本测试唯一的工作区所有者。
        let session_id = crate::SessionId::from_u128(0x50f0);
        // frozenLease models the route already owned by the current or most recently prepared turn.
        // frozenLease 模拟当前或最近已准备回合拥有的路由。
        let frozen_lease = vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
            schema_version: 1,
            provider_id: String::from("basis-provider"),
            connection_id: String::from("basis-account"),
            model_id: String::from("deepseek"),
            protocol_profile_id: String::from("openai-responses.v1"),
            manifest_revision: 1,
            manifest_generation: 1,
            catalog_revision: String::from("catalog-basis"),
            route_revision: String::from("route-basis"),
        };
        // runtimeState starts on the frozen provider/account/model tuple.
        // runtimeState 从冻结的供应商、账号、模型三元组开始。
        let runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("basis-provider")),
            active_connection_id: Some(String::from("basis-account")),
            active_model_id: Some(String::from("deepseek")),
            provider_route_lease: Some(frozen_lease.clone()),
            ..vulcan_workbench_model::SessionRuntimeStateEntry::default()
        };
        // workspace provides both canonical and display transcript stores.
        // workspace 提供规范转录与显示转录两类存储。
        let mut workspace = vulcan_workspace_store::workspace_store::WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        // target is the exact Command Code account choice that must activate only at the next real Provider request.
        // target 是只能在下一次真实 Provider 请求边界激活的精确 Command Code 账号选择。
        let target = SessionPendingModelSelection {
            provider_id: String::from("command-code"),
            connection_id: String::from("command-account"),
            model_id: String::from("deepseek"),
        };

        // firstPlan records one workspace-only mutation without route revocation.
        // firstPlan 记录一次仅工作区修改，不撤销路由。
        let first_plan = plan_session_model_selection_for_next_provider_request(
            &mut workspace,
            session_id,
            &target,
            "Command Code",
            "Command Code account",
            "DeepSeek",
        )
        .expect("session selection should be accepted for the next Provider request");

        assert_eq!(first_plan.changed_session_ids, BTreeSet::from([session_id]));
        assert!(first_plan.revoked_session_ids.is_empty());
        // retainedState proves current route authority stayed untouched while pending authority changed.
        // retainedState 证明待生效权威改变时，当前路由权威保持不变。
        let retained_state = workspace
            .session_runtime_state
            .get(&session_id)
            .expect("session runtime state should remain present");
        assert_eq!(
            retained_state.active_provider_id.as_deref(),
            Some("basis-provider")
        );
        assert_eq!(
            retained_state.active_connection_id.as_deref(),
            Some("basis-account")
        );
        assert_eq!(retained_state.active_model_id.as_deref(), Some("deepseek"));
        assert_eq!(
            retained_state.provider_route_lease.as_ref(),
            Some(&frozen_lease)
        );
        assert_eq!(
            retained_state.pending_model_selection.as_ref(),
            Some(&target)
        );
        assert_eq!(retained_state.conversation_state.records.len(), 1);
        assert_eq!(
            retained_state.conversation_state.records[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("notice_kind"))
                .and_then(serde_json::Value::as_str),
            Some("model_switch")
        );
        assert_eq!(
            crate::display_session_messages_from_workspace(&workspace, session_id)
                .map(|messages| messages.len()),
            Some(1)
        );

        // secondPlan proves selecting the same future tuple does not append a second audit row.
        // secondPlan 证明再次选择同一未来三元组不会追加第二条审计记录。
        let second_plan = plan_session_model_selection_for_next_provider_request(
            &mut workspace,
            session_id,
            &target,
            "Command Code",
            "Command Code account",
            "DeepSeek",
        )
        .expect("repeated session selection should be idempotent");
        assert!(second_plan.changed_session_ids.is_empty());
        assert_eq!(
            workspace
                .session_runtime_state
                .get(&session_id)
                .map(|state| state.conversation_state.records.len()),
            Some(1)
        );
    }

    /// Verifies catalog-only reconciliation never rewrites active or archived session selections.
    /// 验证仅目录协调绝不会改写活动或归档会话选择。
    #[test]
    fn missing_session_models_remain_exact_until_explicit_reselection_or_revocation() {
        // provider_id identifies the deterministic provider shared by both fallback candidates.
        // provider_id 标识两个回退候选共同使用的确定性供应商。
        let provider_id = String::from("fallback-provider");
        // connection_id identifies the only available credential-scoped model catalog.
        // connection_id 标识唯一可用的凭据作用域模型目录。
        let connection_id = String::from("fallback-connection");
        // first_model_id sorts before the preferred model and therefore exercises first-model fallback.
        // first_model_id 排在首选模型之前，因此可验证首模型回退。
        let first_model_id = String::from("model-a-first");
        // preferred_model_id represents the currently selected global default.
        // preferred_model_id 表示当前选中的全局默认模型。
        let preferred_model_id = String::from("model-b-preferred");
        // connection supplies the exact provider and account identity required by availability checks.
        // connection 提供可用性检查所需的精确供应商与账号身份。
        let connection = CredentialConnection {
            id: connection_id.clone(),
            provider_id: provider_id.clone(),
            service_entry_id: String::from("fallback-entry"),
            credential_profile_id: String::from("fallback-profile"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("fallback-credential"),
            display_name: String::from("Fallback account"),
            secret_reference: String::from("fallback-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        // conversation_capabilities make both catalog models valid text-conversation candidates.
        // conversation_capabilities 使两个目录模型都成为有效文本会话候选。
        let conversation_capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            ..ModelCapabilities::default()
        };
        // catalog exposes two enabled models in deliberately non-preferred lexical order.
        // catalog 公开两个启用模型，并刻意设置与首选项不同的字典序。
        let catalog = ConnectionModelCatalog {
            connection_id: connection_id.clone(),
            models: vec![
                vulcan_provider_adapters::ConnectionModel {
                    id: preferred_model_id.clone(),
                    display_name: String::from("Preferred model"),
                    capabilities: conversation_capabilities.clone(),
                    capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(
                    ),
                    protocol_variants: BTreeMap::new(),
                    resolved_protocol_binding: None,
                },
                vulcan_provider_adapters::ConnectionModel {
                    id: first_model_id.clone(),
                    display_name: String::from("First model"),
                    capabilities: conversation_capabilities,
                    capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(
                    ),
                    protocol_variants: BTreeMap::new(),
                    resolved_protocol_binding: None,
                },
            ],
            generation: 1,
            page_count: 1,
            discovery_schema_revision: String::from("session-fallback-test-v1"),
        };
        // configuration owns one available connection and an explicit preferred global model.
        // configuration 持有一个可用连接与一个显式首选全局模型。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .connections
            .insert(connection_id.clone(), connection);
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: provider_id.clone(),
            model_id: preferred_model_id.clone(),
            connection_id: connection_id.clone(),
        });
        // Explicit model access keeps this isolated reconciliation fixture independent of any
        // system Manifest identity.
        // 显式模型访问使该隔离协调夹具不依赖任何系统 Manifest 身份。
        configuration.set_model_enabled(provider_id.as_str(), preferred_model_id.as_str(), true);
        configuration.set_model_enabled(provider_id.as_str(), first_model_id.as_str(), true);
        // catalogs contains the authoritative process-local account model catalog.
        // catalogs 包含权威的进程内账号模型目录。
        let catalogs = BTreeMap::from([(connection_id.clone(), catalog)]);
        // active_session_id identifies the visible session with a missing model selection.
        // active_session_id 标识模型选择已失效的可见会话。
        let active_session_id = crate::SessionId::from_u128(0x5101);
        // archived_session_id identifies the archived session with the same invalid selection.
        // archived_session_id 标识具有相同失效选择的归档会话。
        let archived_session_id = crate::SessionId::from_u128(0x5102);
        // missing_selection represents a complete but unavailable session model identity.
        // missing_selection 表示完整但已不可用的会话模型身份。
        let missing_selection = vulcan_workbench_model::SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("removed-provider")),
            active_connection_id: Some(String::from("removed-connection")),
            active_model_id: Some(String::from("removed-model")),
            ..vulcan_workbench_model::SessionRuntimeStateEntry::default()
        };
        // workspace contains both runtime-state stores exercised by provider/model removal.
        // workspace 包含供应商或模型移除时需要处理的两类运行态存储。
        let mut workspace = vulcan_workspace_store::workspace_store::WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(active_session_id, missing_selection.clone())]),
            archived_session_runtime_state: BTreeMap::from([(
                archived_session_id,
                missing_selection,
            )]),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: active_session_id,
            hydrated_session_ids: BTreeSet::from([active_session_id, archived_session_id]),
        };

        // changedSessionIds remains empty because catalog observation has no authority to rewrite user choices.
        // changedSessionIds 保持为空，因为目录观测无权改写用户选择。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let changed_session_ids = reconcile_missing_session_model_selections(
            &mut workspace,
            &runtime,
            &configuration,
            &catalogs,
        )
        .expect("catalog-only reconciliation should remain observational");
        assert!(changed_session_ids.is_empty());
        for runtime_state in workspace
            .session_runtime_state
            .values()
            .chain(workspace.archived_session_runtime_state.values())
        {
            assert_eq!(
                runtime_state.active_provider_id.as_deref(),
                Some("removed-provider")
            );
            assert_eq!(
                runtime_state.active_connection_id.as_deref(),
                Some("removed-connection")
            );
            assert_eq!(
                runtime_state.active_model_id.as_deref(),
                Some("removed-model")
            );
        }

        // revokedStates reset both stores to one explicitly disabled credential route.
        // revokedStates 将两类存储重置到同一条被显式禁用的凭据路由。
        for runtime_state in workspace
            .session_runtime_state
            .values_mut()
            .chain(workspace.archived_session_runtime_state.values_mut())
        {
            runtime_state.active_provider_id = Some(String::from("revoked-provider"));
            runtime_state.active_connection_id = Some(String::from("revoked-connection"));
            runtime_state.active_model_id = Some(String::from("revoked-model"));
        }
        // revokedSessionIds proves explicit access revocation covers active and archived owners.
        // revokedSessionIds 证明显式访问撤销会覆盖活动与归档所有者。
        let revoked_session_ids = reconcile_disabled_connection_sessions(
            &mut workspace,
            "revoked-connection",
            None,
            configuration.active.as_ref(),
        );
        assert_eq!(
            revoked_session_ids,
            BTreeSet::from([active_session_id, archived_session_id])
        );
        for runtime_state in workspace
            .session_runtime_state
            .values()
            .chain(workspace.archived_session_runtime_state.values())
        {
            assert_eq!(
                runtime_state.active_connection_id.as_deref(),
                Some("fallback-connection")
            );
        }

        configuration.active = None;
        assert_eq!(
            preferred_fallback_model_selection(&runtime, &configuration, &catalogs),
            Ok(Some(ActiveAiModelSelection {
                provider_id,
                model_id: first_model_id,
                connection_id,
            }))
        );
    }

    /// Verifies account membership cannot reopen a model disabled by its provider.
    /// 验证账号成员关系不能重新开启已被供应商关闭的模型。
    #[test]
    fn account_membership_cannot_enable_provider_disabled_custom_model() {
        // configuration stores the exact provider-level disabled decision under test.
        // configuration 保存本测试所需的精确供应商级关闭决定。
        let mut configuration = PersistedAiConfiguration::default();
        configuration.set_model_enabled("custom:test", "manual/chat", false);

        assert!(
            validate_custom_model_connection_availability_choice(
                &configuration,
                "custom:test",
                "manual/chat",
                Some(true),
            )
            .is_err()
        );
        assert!(
            validate_custom_model_connection_availability_choice(
                &configuration,
                "custom:test",
                "manual/chat",
                Some(false),
            )
            .is_ok()
        );
        assert!(
            validate_custom_model_connection_availability_choice(
                &configuration,
                "custom:test",
                "manual/chat",
                None,
            )
            .is_ok()
        );
    }

    /// Verifies VMM LLM route validation rejects models that cannot disable reasoning.
    /// 验证 VMM LLM 路由校验拒绝无法关闭思考的模型。
    #[test]
    fn vmm_llm_route_requires_disabled_reasoning_capability() {
        let fixed_on = ModelCapabilities {
            reasoning_control: ReasoningControlMode::FixedOn,
            reasoning_efforts: vec![ReasoningEffort::High],
            ..ModelCapabilities::default()
        };
        assert!(
            validate_vmm_llm_reasoning_policy(
                VMM_CONSUMER_PROFILE_ID,
                AiOperation::ConversationRespond,
                &fixed_on,
            )
            .is_err()
        );
        assert!(
            validate_vmm_llm_reasoning_policy(
                "another-consumer",
                AiOperation::ConversationRespond,
                &fixed_on,
            )
            .is_ok()
        );
        assert!(
            validate_vmm_llm_reasoning_policy(
                VMM_CONSUMER_PROFILE_ID,
                AiOperation::EmbeddingCreate,
                &fixed_on,
            )
            .is_ok()
        );
    }

    /// Verifies a route stored in the wrong operation slot is rejected explicitly.
    /// 验证存入错误操作槽位的路由会被显式拒绝。
    #[test]
    fn settings_test_route_rejects_operation_slot_mismatch() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.inference_profiles.insert(
            String::from("vmm"),
            InferenceProfile {
                embedding: Some(route_fixture(
                    AiOperation::RerankDocuments,
                    InferenceUsageScope::ExternalService,
                )),
                ..InferenceProfile::default()
            },
        );

        let error = configured_settings_inference_test_route(
            &configuration,
            "vmm",
            AiOperation::EmbeddingCreate,
        )
        .expect_err("a mismatched operation slot must fail");

        assert!(error.contains("stores operation `rerank_documents`"));
    }

    /// Verifies an enabled session capability without a model-account pair is an explicit conflict.
    /// 验证缺少模型账号组合的已启用会话能力会形成显式冲突。
    #[test]
    fn session_provider_tool_settings_reject_missing_selection() {
        let runtime = PublicAiRuntime::unavailable().expect("test public AI runtime");
        let configuration = PersistedAiConfiguration::default();
        let conflicts = provider_tool_session_conflicts(
            &runtime,
            &configuration,
            &BTreeSet::from([ProviderToolCapabilityKind::WebSearch]),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("conflict analysis");

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, "missing_selection");
        assert_eq!(
            conflicts[0].capability,
            ProviderToolCapabilityKind::WebSearch
        );
    }

    /// Verifies an enabled media capability reads its dedicated backend store instead of the legacy session target map.
    /// 验证已启用媒体能力读取专用后端存储，而不是旧会话目标映射。
    #[test]
    fn session_media_understanding_settings_reject_missing_backend() {
        let runtime = PublicAiRuntime::unavailable().expect("test public AI runtime");
        let configuration = PersistedAiConfiguration::default();
        let conflicts = provider_tool_session_conflicts(
            &runtime,
            &configuration,
            &BTreeSet::from([ProviderToolCapabilityKind::VideoUnderstand]),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("media conflict analysis");

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, "missing_selection");
        assert_eq!(
            conflicts[0].capability,
            ProviderToolCapabilityKind::VideoUnderstand
        );
    }

    /// Verifies lazy session provider-tool initialization is deterministic and idempotent.
    /// 验证惰性会话供应商工具初始化具备确定性与幂等性。
    #[test]
    fn session_provider_tool_settings_reconciliation_is_idempotent() {
        // configuration owns one exact global capability and model-account default.
        // configuration 持有一项精确的全局能力与模型账号默认值。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .provider_tool_enabled_capabilities
            .insert(ProviderToolCapabilityKind::WebSearch);
        configuration.provider_tool_selections.insert(
            ProviderToolCapabilityKind::WebSearch,
            ProviderToolTargetSelection {
                connection_id: String::from("search-connection"),
                target_id: String::from("search-model"),
            },
        );
        // extensionDefault owns one multi-select generation target inherited by a new session.
        // extensionDefault 持有一项由新会话继承的多选生成目标。
        let extension_default = ProviderToolTargetSelection {
            connection_id: String::from("image-connection"),
            target_id: String::from("image-model"),
        };
        configuration.provider_extension_target_sets.insert(
            ProviderToolCapabilityKind::ImageGenerate,
            BTreeSet::from([extension_default.clone()]),
        );
        // runtimeState starts in the current uninitialized representation.
        // runtimeState 从当前未初始化表示开始。
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();
        // firstReconciliation must materialize the global defaults exactly once.
        // firstReconciliation 必须且仅须一次物化全局默认值。
        let (
            enabled_capabilities,
            selections,
            media_understanding_modes,
            extension_target_sets,
            first_changed,
        ) = reconcile_session_provider_tool_settings(&mut runtime_state, &configuration)
            .expect("uninitialized session settings should reconcile");
        assert!(first_changed);
        assert_eq!(
            enabled_capabilities,
            BTreeSet::from([ProviderToolCapabilityKind::WebSearch])
        );
        assert_eq!(
            selections
                .get(&ProviderToolCapabilityKind::WebSearch)
                .map(|selection| selection.connection_id.as_str()),
            Some("search-connection")
        );
        assert!(media_understanding_modes.is_empty());
        assert_eq!(
            extension_target_sets.get(&ProviderToolCapabilityKind::ImageGenerate),
            Some(&BTreeSet::from([extension_default]))
        );
        // secondChanged proves an identical generation performs no authoritative state write.
        // secondChanged 证明相同代际不会再次执行权威状态写入。
        let (
            _enabled_capabilities,
            _selections,
            _media_understanding_modes,
            _extension_target_sets,
            second_changed,
        ) = reconcile_session_provider_tool_settings(&mut runtime_state, &configuration)
            .expect("initialized session settings should remain readable");
        assert!(!second_changed);
    }

    /// Verifies legacy media targets are removed even from sessions with explicit overrides.
    /// 验证即使会话存在显式覆盖，也会移除历史媒体目标。
    #[test]
    fn session_media_direct_targets_are_removed_from_overridden_sessions() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.provider_tool_selections.insert(
            ProviderToolCapabilityKind::VideoUnderstand,
            ProviderToolTargetSelection {
                connection_id: String::from("legacy-video-connection"),
                target_id: String::from("legacy-video-target"),
            },
        );
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
            provider_tool_settings_initialized: true,
            provider_tool_selections_overridden: true,
            ..Default::default()
        };
        runtime_state.provider_tool_selections.insert(
            provider_tool_capability_key(ProviderToolCapabilityKind::VideoUnderstand),
            vulcan_workbench_model::SessionProviderToolTargetSelection {
                connection_id: String::from("legacy-video-connection"),
                target_id: String::from("legacy-video-target"),
            },
        );
        runtime_state.provider_tool_selections.insert(
            provider_tool_capability_key(ProviderToolCapabilityKind::WebSearch),
            vulcan_workbench_model::SessionProviderToolTargetSelection {
                connection_id: String::from("search-connection"),
                target_id: String::from("search-target"),
            },
        );

        let (
            _enabled_capabilities,
            selections,
            _media_understanding_modes,
            _extension_target_sets,
            changed,
        ) = reconcile_session_provider_tool_settings(&mut runtime_state, &configuration)
            .expect("legacy media targets should be normalized");

        assert!(changed);
        assert!(!runtime_state.provider_tool_selections.contains_key(
            &provider_tool_capability_key(ProviderToolCapabilityKind::VideoUnderstand)
        ));
        assert!(runtime_state.provider_tool_selections.contains_key(
            &provider_tool_capability_key(ProviderToolCapabilityKind::WebSearch)
        ));
        assert!(!selections.contains_key(&ProviderToolCapabilityKind::VideoUnderstand));
        assert!(selections.contains_key(&ProviderToolCapabilityKind::WebSearch));
    }

    /// Verifies first session materialization persists the active global media backend mode.
    /// 验证首次物化会话时会持久化当前全局媒体后端模式。
    #[test]
    fn session_media_backend_mode_is_materialized_with_session_defaults() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.media_understanding_selections.insert(
            ProviderToolCapabilityKind::ImageUnderstand,
            MediaUnderstandingBackendSelection::ProviderTool {
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("image-account"),
                    target_id: String::from("image-target"),
                },
            },
        );
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();

        let (_enabled_capabilities, _selections, modes, _extension_target_sets, changed) =
            reconcile_session_provider_tool_settings(&mut runtime_state, &configuration)
                .expect("session defaults should materialize");

        assert!(changed);
        assert_eq!(
            modes.get(&ProviderToolCapabilityKind::ImageUnderstand),
            Some(&vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool)
        );
        assert_eq!(
            runtime_state.media_understanding_modes,
            BTreeMap::from([(
                String::from("image_understand"),
                vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool,
            )])
        );
    }

    /// Verifies explicit Codex capability switches survive the custom-provider candidate projection.
    /// 验证显式 Codex 能力开关可完整通过自定义供应商候选投影。
    #[test]
    fn custom_codex_provider_candidate_preserves_explicit_responses_capabilities() {
        // request represents a user explicitly enabling both opt-in Codex capabilities.
        // request 表示用户明确启用两项可选 Codex 能力。
        let request = PublicAiCustomProviderSaveRequest {
            provider_id: None,
            display_name: String::from("Codex Proxy"),
            base_url: String::from("https://proxy.example.test/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiCodexResponses],
            remote_compaction_policy: ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback,
            websocket_policy: ResponsesWebsocketPolicy::UnverifiedOverride,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: Some(String::from("Test Account")),
            secret: Some(String::from("test-secret")),
            validation_connection_id: None,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            allow_manual_fallback: false,
            allow_pending_validation: false,
            confirm_inactive_protocol_override_deletion: false,
        };
        // candidate is the exact non-persisted descriptor produced by the save boundary.
        // candidate 是保存边界生成的精确未持久化描述符。
        let candidate =
            prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
                .expect("explicit Codex capability candidate should compile");
        // features is the route-scoped persisted capability projection under audit.
        // features 是本轮审核的路由级持久化能力投影。
        let features = candidate.descriptor.service_entries[0].protocol_routes[0]
            .responses_features
            .expect("Codex route should preserve explicit capability switches");

        assert_eq!(
            features.remote_compaction,
            ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback
        );
        assert_eq!(
            features.websocket,
            ResponsesWebsocketPolicy::UnverifiedOverride
        );
    }

    /// Verifies execution-only Codex capability switches do not trigger credential revalidation.
    /// 验证只影响执行的 Codex 能力开关不会触发凭据重新验证。
    #[test]
    fn custom_codex_capability_switch_is_not_a_connection_contract_change() {
        // providerId identifies the exact credential-free custom provider being edited.
        // providerId 标识正在编辑的精确无凭据自定义供应商。
        let provider_id = String::from("custom:codex-capability-switch");
        // descriptor is the persisted Codex route with both optional capabilities disabled by default.
        // descriptor 是两项可选能力默认关闭的持久化 Codex 路由。
        let descriptor = CustomProviderDefinition {
            id: provider_id.clone(),
            display_name: String::from("Codex Proxy"),
            base_url: String::from("https://proxy.example.test/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiCodexResponses],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("default Codex provider should compile");
        // configuration owns the exact persisted descriptor used as the comparison baseline.
        // configuration 持有作为比较基线的精确持久化描述符。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(provider_id.clone(), descriptor.clone());
        // request changes only the two explicit execution capabilities.
        // request 仅修改两项显式执行能力。
        let request = PublicAiCustomProviderSaveRequest {
            provider_id: Some(provider_id),
            display_name: String::from("Codex Proxy"),
            base_url: String::from("https://proxy.example.test/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiCodexResponses],
            remote_compaction_policy: ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback,
            websocket_policy: ResponsesWebsocketPolicy::UnverifiedOverride,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: None,
            secret: None,
            validation_connection_id: None,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            allow_manual_fallback: false,
            allow_pending_validation: false,
            confirm_inactive_protocol_override_deletion: false,
        };
        // candidate preserves the changed feature values while retaining the same connection contract.
        // candidate 保留已变更能力值，同时维持相同连接契约。
        let candidate = prepare_custom_provider_candidate(&request, &configuration)
            .expect("capability-only edit should compile");

        assert_ne!(
            descriptor.service_entries,
            candidate.descriptor.service_entries
        );
        assert!(
            !custom_provider_connection_contract_changed(&configuration, &candidate)
                .expect("capability-only edit should compare")
        );
    }

    /// Verifies protocol edits reuse secrets, migrate every credential, and reject partial invalid migrations.
    /// 验证协议编辑会复用秘密、迁移全部凭据，并拒绝部分无效迁移。
    #[test]
    fn custom_provider_edit_atomically_migrates_every_existing_credential() {
        let provider_id = String::from("custom:test-provider");
        let descriptor = CustomProviderDefinition {
            id: provider_id.clone(),
            display_name: String::from("Test Provider"),
            base_url: String::from("https://old.example.test/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiResponses],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("custom provider fixture should compile");
        let entry = descriptor
            .service_entries
            .first()
            .expect("custom provider entry should exist");
        let profile = entry
            .credential_profiles
            .first()
            .expect("custom provider credential profile should exist");
        let route = entry
            .protocol_routes
            .first()
            .expect("custom provider protocol route should exist");
        let existing_protocol_id = route.protocol_profile_id.clone();
        let existing = CredentialConnection {
            id: String::from("connection-existing"),
            provider_id: provider_id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: profile.id.clone(),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential-existing"),
            display_name: String::from("Existing Account"),
            secret_reference: String::from("secret-existing"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(provider_id.clone(), descriptor);
        configuration
            .connection_graph
            .connections
            .insert(existing.id.clone(), existing.clone());
        let request = PublicAiCustomProviderSaveRequest {
            provider_id: Some(provider_id.clone()),
            display_name: String::from("Renamed Provider"),
            base_url: String::from("https://new.example.test/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
            websocket_policy: ResponsesWebsocketPolicy::Disabled,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: None,
            secret: None,
            validation_connection_id: Some(existing.id.clone()),
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            allow_manual_fallback: false,
            allow_pending_validation: false,
            confirm_inactive_protocol_override_deletion: false,
        };

        let candidate = prepare_custom_provider_candidate(&request, &configuration)
            .expect("editing should reuse the selected account");
        assert!(candidate.updating_existing);
        assert!(candidate.temporary_secret.is_none());
        // candidate_connection is the selected account's isolated metadata-validation copy.
        // candidate_connection 是所选账号用于隔离元数据验证的副本。
        let candidate_connection = candidate
            .validation_connection
            .as_ref()
            .expect("selected credential should create a validation connection");
        assert_ne!(candidate_connection.id, existing.id);
        assert_eq!(
            candidate_connection.secret_reference,
            existing.secret_reference
        );
        assert_eq!(candidate_connection.credential_id, existing.credential_id);
        assert_eq!(candidate_connection.display_name, existing.display_name);
        assert_ne!(
            candidate.activation_target.default_protocol_profile_id,
            existing_protocol_id
        );
        assert!(
            custom_provider_connection_contract_changed(&configuration, &candidate)
                .expect("protocol edit should compare")
        );
        // cosmetic_request changes only the provider-facing display name.
        // cosmetic_request 仅修改面向用户的供应商展示名称。
        let cosmetic_request = PublicAiCustomProviderSaveRequest {
            provider_id: Some(provider_id.clone()),
            display_name: String::from("Cosmetic Rename"),
            base_url: String::from("https://old.example.test/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiResponses],
            remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
            websocket_policy: ResponsesWebsocketPolicy::Disabled,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: None,
            secret: None,
            validation_connection_id: Some(existing.id.clone()),
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            allow_manual_fallback: false,
            allow_pending_validation: false,
            confirm_inactive_protocol_override_deletion: false,
        };
        // cosmetic_candidate proves display-only edits do not disturb credential state.
        // cosmetic_candidate 验证仅展示信息编辑不会扰动凭据状态。
        let cosmetic_candidate =
            prepare_custom_provider_candidate(&cosmetic_request, &configuration)
                .expect("cosmetic edit should compile");
        assert!(
            !custom_provider_connection_contract_changed(&configuration, &cosmetic_candidate)
                .expect("cosmetic edit should compare")
        );
        // credential_free_configuration represents the legal state after the provider's last account is removed.
        // credential_free_configuration 表示供应商最后一个账号被移除后的合法状态。
        let mut credential_free_configuration = configuration.clone();
        credential_free_configuration
            .connection_graph
            .connections
            .clear();
        let credential_free_request = PublicAiCustomProviderSaveRequest {
            validation_connection_id: None,
            ..request.clone()
        };
        // credential_free_candidate keeps the target protocol exact without inventing secret-bearing state.
        // credential_free_candidate 在不虚构秘密状态的前提下保持目标协议精确。
        let credential_free_candidate = prepare_custom_provider_candidate(
            &credential_free_request,
            &credential_free_configuration,
        )
        .expect("credential-free provider should remain editable");
        assert!(credential_free_candidate.validation_connection.is_none());
        assert!(
            custom_provider_edit_validation_candidates(
                &credential_free_configuration,
                &credential_free_candidate,
            )
            .expect("credential-free edit has no account to validate")
            .is_empty()
        );

        // sibling proves the provider protocol changes as one provider-wide operation.
        // sibling 验证供应商协议会作为供应商级操作整体切换。
        let mut sibling = existing.clone();
        sibling.id = String::from("connection-sibling");
        sibling.credential_id = String::from("credential-sibling");
        sibling.secret_reference = String::from("secret-sibling");
        configuration
            .connection_graph
            .connections
            .insert(sibling.id.clone(), sibling.clone());
        // inference_route represents a durable VMM route bound to the provider's old single protocol.
        // inference_route 表示绑定到供应商旧唯一协议的持久化 VMM 路由。
        let inference_route = InferenceRouteConfiguration {
            provider_id: provider_id.clone(),
            service_entry_id: existing.service_entry_id.clone(),
            connection_id: existing.id.clone(),
            model_id: String::from("internal-model"),
            binding: InferenceRouteBinding::Protocol {
                protocol_profile_id: existing_protocol_id,
            },
            operation: AiOperation::ConversationRespond,
            usage_scope: InferenceUsageScope::ExternalService,
        };
        configuration.inference_profiles.insert(
            String::from("vmm"),
            InferenceProfile {
                llm_routes: BTreeMap::from([(String::from("precheck_l1"), inference_route)]),
                ..InferenceProfile::default()
            },
        );
        // validation_candidates proves provider edits validate every credential without sharing secrets.
        // validation_candidates 验证供应商编辑会逐份验证凭据且不会共享秘密。
        let validation_candidates =
            custom_provider_edit_validation_candidates(&configuration, &candidate)
                .expect("every credential must receive a validation candidate");
        assert_eq!(validation_candidates.len(), 2);
        for (source_connection_id, validation_candidate) in &validation_candidates {
            // source_connection is the exact durable account mirrored by this temporary candidate.
            // source_connection 是此临时候选项所镜像的精确持久化账号。
            let source_connection =
                &configuration.connection_graph.connections[source_connection_id.as_str()];
            // validation_connection is the exact isolated copy for this source account.
            // validation_connection 是此源账号的精确隔离副本。
            let validation_connection = validation_candidate
                .validation_connection
                .as_ref()
                .expect("existing credential should create a validation connection");
            assert_ne!(validation_connection.id, source_connection.id);
            assert_eq!(
                validation_connection.secret_reference,
                source_connection.secret_reference
            );
            assert_eq!(
                validation_connection.service_entry_id,
                candidate.activation_target.service_entry_id
            );
            assert_eq!(
                validation_connection.credential_profile_id,
                candidate.activation_target.credential_profile_id
            );
        }
        // providers is the effective registry containing only the newly selected custom protocol.
        // providers 是仅包含新选自定义协议的有效供应商注册表。
        let providers = ProviderRegistry::effective(&BTreeMap::from([(
            provider_id.clone(),
            candidate.descriptor.clone(),
        )]))
        .expect("new custom provider descriptor must register");
        // validation_outcomes proves each credential carries its own verification result into migration.
        // validation_outcomes 验证每份凭据都会把自身验证结果带入迁移。
        let validation_outcomes = BTreeMap::from([
            (
                existing.id.clone(),
                CustomProviderConnectionValidationOutcome {
                    pending_validation: false,
                    catalog: None,
                    status: CatalogDiscoveryStatus {
                        state: CatalogDiscoveryState::Ready,
                        failure_kind: None,
                        message: None,
                        model_count: 1,
                        generation: 1,
                        observed_at_ms: 1,
                    },
                    refresh_error: None,
                },
            ),
            (
                sibling.id.clone(),
                CustomProviderConnectionValidationOutcome {
                    pending_validation: true,
                    catalog: None,
                    status: CatalogDiscoveryStatus {
                        state: CatalogDiscoveryState::Ready,
                        failure_kind: None,
                        message: None,
                        model_count: 1,
                        generation: 1,
                        observed_at_ms: 1,
                    },
                    refresh_error: None,
                },
            ),
        ]);
        migrate_custom_provider_connections(
            &providers,
            &mut configuration,
            provider_id.as_str(),
            &candidate.activation_target,
            &validation_outcomes,
        )
        .expect("every compatible credential must migrate");
        for (connection_id, secret_reference) in [
            (existing.id.as_str(), existing.secret_reference.as_str()),
            (sibling.id.as_str(), sibling.secret_reference.as_str()),
        ] {
            let migrated = configuration
                .connection_graph
                .connections
                .get(connection_id)
                .expect("migrated connection must remain addressable");
            assert_eq!(
                migrated.service_entry_id,
                candidate.activation_target.service_entry_id
            );
            assert_eq!(
                migrated.credential_profile_id,
                candidate.activation_target.credential_profile_id
            );
            assert_eq!(migrated.secret_reference, secret_reference);
        }
        // migrated_inference_route proves internal consumers remain executable after the protocol switch.
        // migrated_inference_route 验证内部消费者在协议切换后仍保持可执行。
        let migrated_inference_route =
            &configuration.inference_profiles["vmm"].llm_routes["precheck_l1"];
        assert_eq!(
            migrated_inference_route.service_entry_id,
            candidate.activation_target.service_entry_id
        );
        assert_eq!(
            migrated_inference_route.binding,
            InferenceRouteBinding::Protocol {
                protocol_profile_id: candidate
                    .activation_target
                    .default_protocol_profile_id
                    .clone(),
            }
        );

        // failing_configuration contains one credential mode the new token profile cannot accept.
        // failing_configuration 包含一个新 Token 配置档无法接受的凭据模式。
        let mut failing_configuration = PersistedAiConfiguration::default();
        failing_configuration
            .connection_graph
            .connections
            .insert(existing.id.clone(), existing.clone());
        let mut incompatible = sibling;
        incompatible.credential_mode = CredentialMode::Auth;
        failing_configuration
            .connection_graph
            .connections
            .insert(incompatible.id.clone(), incompatible);
        let original_graph = failing_configuration.connection_graph.clone();
        assert!(
            migrate_custom_provider_connections(
                &providers,
                &mut failing_configuration,
                provider_id.as_str(),
                &candidate.activation_target,
                &validation_outcomes,
            )
            .is_err()
        );
        assert_eq!(failing_configuration.connection_graph, original_graph);

        let invalid_request = PublicAiCustomProviderSaveRequest {
            secret: Some(String::from("replacement-secret")),
            ..request
        };
        assert!(
            prepare_custom_provider_candidate(&invalid_request, &configuration)
                .err()
                .expect("editing must reject replacement secret material")
                .contains("cannot create a new secret")
        );
    }

    /// Verifies every discovery state accepts only its exact explicit save path.
    /// 验证每个发现状态只接受其精确对应的显式保存路径。
    #[test]
    fn custom_provider_observation_acceptance_is_closed_and_explicit() {
        assert!(custom_provider_observation_is_accepted(
            CatalogDiscoveryState::Ready,
            false,
            false,
        ));
        for state in [
            CatalogDiscoveryState::Empty,
            CatalogDiscoveryState::Unsupported,
            CatalogDiscoveryState::InvalidResponse,
        ] {
            assert!(!custom_provider_observation_is_accepted(
                state, false, false,
            ));
            assert!(custom_provider_observation_is_accepted(state, true, false));
            assert!(!custom_provider_observation_is_accepted(state, false, true,));
        }
        for state in [
            CatalogDiscoveryState::RateLimited,
            CatalogDiscoveryState::TemporarilyUnavailable,
            CatalogDiscoveryState::Failed,
        ] {
            assert!(!custom_provider_observation_is_accepted(
                state, false, false,
            ));
            assert!(!custom_provider_observation_is_accepted(state, true, false,));
            assert!(custom_provider_observation_is_accepted(state, false, true));
        }
        for state in [
            CatalogDiscoveryState::NotChecked,
            CatalogDiscoveryState::AuthenticationFailed,
            CatalogDiscoveryState::Stale,
        ] {
            assert!(!custom_provider_observation_is_accepted(
                state, false, false,
            ));
            assert!(!custom_provider_observation_is_accepted(state, true, true,));
        }
    }

    /// Verifies only accepted non-ready states are marked pending validation.
    /// 验证只有被接受的未就绪状态才会标记为待验证。
    #[test]
    fn custom_provider_pending_validation_matches_non_ready_acceptance() {
        assert!(!custom_provider_save_requires_pending_validation(
            CatalogDiscoveryState::Ready,
            false,
            false,
        ));
        for state in [
            CatalogDiscoveryState::Empty,
            CatalogDiscoveryState::Unsupported,
            CatalogDiscoveryState::InvalidResponse,
        ] {
            assert!(!custom_provider_save_requires_pending_validation(
                state, false, false,
            ));
            assert!(custom_provider_save_requires_pending_validation(
                state, true, false,
            ));
        }
        for state in [
            CatalogDiscoveryState::RateLimited,
            CatalogDiscoveryState::TemporarilyUnavailable,
            CatalogDiscoveryState::Failed,
        ] {
            assert!(!custom_provider_save_requires_pending_validation(
                state, false, false,
            ));
            assert!(custom_provider_save_requires_pending_validation(
                state, false, true,
            ));
        }
        for state in [
            CatalogDiscoveryState::NotChecked,
            CatalogDiscoveryState::AuthenticationFailed,
            CatalogDiscoveryState::Stale,
        ] {
            assert!(!custom_provider_save_requires_pending_validation(
                state, true, true,
            ));
        }
    }

    /// Verifies older model-save clients may omit the newly added template decision batch.
    /// 验证旧版模型保存客户端可以省略新增的模板决策批次。
    #[test]
    fn custom_model_save_request_defaults_missing_template_decisions() {
        // request preserves the previous browser payload shape while the service owns the default.
        // request 保留旧版浏览器载荷形态，并由服务端负责提供默认值。
        let request: PublicAiCustomModelsSaveRequest = serde_json::from_value(serde_json::json!({
            "providerId": "custom:test",
            "models": []
        }))
        .expect("the previous custom-model save payload should remain valid");

        assert_eq!(request.provider_id, "custom:test");
        assert!(request.models.is_empty());
        assert!(request.template_decisions.is_empty());
    }

    /// Verifies a system connection derives its directory kind from Manifest routes alone.
    /// 验证系统连接只从 Manifest 路由派生目录类型。
    #[test]
    fn system_connection_directory_uses_manifest_routes() {
        // providers supplies one exact built-in route for the isolation boundary.
        // providers 提供一条精确内置路由，用于验证隔离边界。
        let providers = ProviderRegistry::first_batch().expect("built-in provider registry");
        // provider is the exact MiniMax descriptor used by the fixture connection.
        // provider 是测试连接使用的精确 MiniMax 描述。
        let provider = providers.provider("minimax").expect("MiniMax provider");
        // entry supplies the exact service and credential identifiers without guessing names.
        // entry 提供精确服务与凭据标识，不猜测名称。
        let entry = provider
            .service_entries
            .first()
            .expect("MiniMax service entry");
        // profile supplies the exact credential profile registered under the service entry.
        // profile 提供服务入口下注册的精确凭据配置档。
        let profile = entry
            .credential_profiles
            .first()
            .expect("MiniMax credential profile");
        // connection contains no protocol state by design.
        // connection 按设计不包含协议状态。
        let connection = CredentialConnection {
            id: String::from("legacy-incomplete"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: profile.id.clone(),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("legacy-credential"),
            display_name: String::from("Legacy account"),
            secret_reference: String::from("legacy-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };

        assert_eq!(
            connection_model_directory_kind(&providers, &connection),
            PublicAiModelDirectoryKind::Dynamic
        );
    }

    /// Verifies account synchronization remains available when any protocol route is remote.
    /// 验证任一协议路由为远端发现时账号同步仍保持可用。
    #[test]
    fn multi_protocol_directory_kind_aggregates_route_discovery() {
        // provider declares a manual first route and a remote second route.
        // provider 声明一条手工首路由与一条远端次路由。
        let provider = vulcan_provider_adapters::CustomProviderDefinition {
            id: String::from("custom:mixed-directory"),
            display_name: String::from("Mixed Directory"),
            base_url: String::from("https://mixed-directory.example.test/v1"),
            protocols: vec![
                CustomProviderProtocol::OpenAiResponses,
                CustomProviderProtocol::OpenAiChat,
            ],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocol_routes: vec![vulcan_provider_adapters::CustomProtocolRoute {
                protocol: CustomProviderProtocol::OpenAiChat,
                base_url: None,
                endpoint_path: None,
                authentication: None,
                model_discovery_mode: Some(CustomProviderModelDiscoveryMode::Standard),
                model_discovery_path: None,
                request_headers: BTreeMap::new(),
            }],
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("mixed directory provider should project");
        // entry supplies the exact generated service and credential identifiers.
        // entry 提供精确生成的服务与凭据标识。
        let entry = provider
            .service_entries
            .first()
            .expect("custom provider should expose one entry");
        let profile = entry
            .credential_profiles
            .first()
            .expect("custom provider should expose one credential profile");
        // connection deliberately selects the manual compatibility route first.
        // connection 刻意首先选择手工兼容路由。
        let connection = CredentialConnection {
            id: String::from("mixed-directory-account"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: profile.id.clone(),
            credential_mode: profile.credential_mode,
            credential_id: String::from("mixed-directory-credential"),
            display_name: String::from("Mixed directory account"),
            secret_reference: String::from("mixed-directory-secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        // providers is the exact effective registry consumed by the browser projection.
        // providers 是浏览器投影消费的精确有效注册表。
        let providers =
            ProviderRegistry::effective(&BTreeMap::from([(provider.id.clone(), provider)]))
                .expect("mixed directory registry should build");

        assert_eq!(
            connection_model_directory_kind(&providers, &connection),
            PublicAiModelDirectoryKind::Dynamic,
        );
    }

    /// Verifies multi-protocol activation never replaces a model binding with the provider default.
    /// 验证多协议激活绝不会用供应商默认值替换模型绑定。
    #[test]
    fn multi_protocol_migration_preserves_exact_internal_route_binding() {
        // target exposes Responses first while retaining Chat as another enabled protocol.
        // target 首选 Responses，同时保留 Chat 作为另一项启用协议。
        let target = CustomProviderActivationTarget {
            service_entry_id: String::from("custom-entry-v2"),
            credential_profile_id: String::from("api-key"),
            default_protocol_profile_id: String::from(
                vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            ),
            protocol_profile_ids: BTreeSet::from([
                String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
                String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID),
            ]),
        };
        // route represents a model explicitly bound to Chat before the provider edit.
        // route 表示供应商编辑前显式绑定到 Chat 的模型。
        let mut route = InferenceRouteConfiguration {
            provider_id: String::from("custom:multi-route"),
            service_entry_id: String::from("custom-entry-v1"),
            connection_id: String::from("custom-account"),
            model_id: String::from("chat-only-model"),
            binding: InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID),
            },
            operation: AiOperation::ConversationRespond,
            usage_scope: InferenceUsageScope::ExternalService,
        };
        // owned_connections scopes migration to the exact provider account.
        // owned_connections 将迁移限定到精确供应商账号。
        let owned_connections = BTreeSet::from([String::from("custom-account")]);

        migrate_custom_provider_inference_route(
            &mut route,
            "custom:multi-route",
            &owned_connections,
            &target,
        )
        .expect("multi-protocol route migration should remain structurally valid");

        assert_eq!(route.service_entry_id, "custom-entry-v2");
        assert_eq!(
            route.binding,
            InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,),
            },
            "the provider default must not replace an exact model route"
        );
    }

    /// Verifies a multi-protocol custom provider projects one verified route per protocol.
    /// 验证多协议自定义供应商为每个协议投影一条已验证路由。
    #[test]
    fn multi_protocol_custom_provider_projects_one_route_per_protocol() {
        let request = PublicAiCustomProviderSaveRequest {
            provider_id: None,
            display_name: String::from("Multi Protocol"),
            base_url: String::from("https://multi.example.test/v1"),
            remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
            websocket_policy: ResponsesWebsocketPolicy::Disabled,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: Some(String::from("Account")),
            secret: Some(String::from("secret")),
            validation_connection_id: None,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocols: vec![
                CustomProviderProtocol::OpenAiResponses,
                CustomProviderProtocol::AnthropicMessages,
                CustomProviderProtocol::OpenAiChat,
            ],
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            allow_manual_fallback: false,
            allow_pending_validation: false,
            confirm_inactive_protocol_override_deletion: false,
        };
        let candidate =
            prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
                .expect("multi-protocol candidate should compile");
        let entry = &candidate.descriptor.service_entries[0];
        assert_eq!(entry.protocol_routes.len(), 3);
        assert_eq!(
            candidate.activation_target.protocol_profile_ids.len(),
            3,
            "every enabled protocol must stay verified for the connection"
        );
        assert_eq!(
            candidate.activation_target.default_protocol_profile_id,
            entry.protocol_routes[0].protocol_profile_id
        );
    }

    /// Verifies removing one protocol reports exactly that protocol and preserves the rest.
    /// 验证删除一个协议时精确报告该协议，并完整保留其余协议。
    #[test]
    fn removed_protocols_reports_only_the_deleted_protocol() {
        let mut current = PersistedAiConfiguration::default();
        let request = PublicAiCustomProviderSaveRequest {
            provider_id: Some(String::from("custom:multi")),
            display_name: String::from("Multi Protocol"),
            base_url: String::from("https://multi.example.test/v1"),
            remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
            websocket_policy: ResponsesWebsocketPolicy::Disabled,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: None,
            secret: None,
            validation_connection_id: None,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocols: vec![
                CustomProviderProtocol::OpenAiResponses,
                CustomProviderProtocol::AnthropicMessages,
            ],
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            allow_manual_fallback: false,
            allow_pending_validation: false,
            confirm_inactive_protocol_override_deletion: false,
        };
        let candidate =
            prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
                .expect("candidate should compile");
        // existing_descriptor carries the full three-protocol set before the edit drops Chat.
        // existing_descriptor 在编辑删除 Chat 前携带完整三协议集合。
        let existing_descriptor = CustomProviderDefinition {
            id: String::from("custom:multi"),
            display_name: String::from("Multi Protocol"),
            base_url: String::from("https://multi.example.test/v1"),
            protocols: vec![
                CustomProviderProtocol::OpenAiResponses,
                CustomProviderProtocol::AnthropicMessages,
                CustomProviderProtocol::OpenAiChat,
            ],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("existing descriptor should compile");
        current
            .custom_providers
            .insert(String::from("custom:multi"), existing_descriptor);

        let removed = custom_provider_removed_protocols(&current, &candidate)
            .expect("removed protocols should resolve");
        assert_eq!(
            removed,
            BTreeSet::from([String::from("openai-chat-completions.v1")]),
            "only Chat is removed; Responses and Anthropic survive"
        );
        // A cosmetic edit with the same protocol set removes nothing.
        // 协议集合不变的展示性编辑不删除任何协议。
        let cosmetic = current.clone();
        let mut unchanged_candidate = candidate;
        unchanged_candidate.descriptor = current.custom_providers["custom:multi"].clone();
        let removed = custom_provider_removed_protocols(&cosmetic, &unchanged_candidate)
            .expect("unchanged protocol set should resolve");
        assert!(removed.is_empty());
    }

    /// Verifies the connection contract comparison normalizes every route, not only the first.
    /// 验证连接契约比较会归一化每一条路由，而不只是第一条。
    #[test]
    fn connection_contract_comparison_normalizes_every_route() {
        let mut current = PersistedAiConfiguration::default();
        let existing_descriptor = CustomProviderDefinition {
            id: String::from("custom:multi"),
            display_name: String::from("Multi Protocol"),
            base_url: String::from("https://multi.example.test/v1"),
            protocols: vec![
                CustomProviderProtocol::OpenAiResponses,
                CustomProviderProtocol::OpenAiChat,
            ],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("existing descriptor should compile");
        current
            .custom_providers
            .insert(String::from("custom:multi"), existing_descriptor.clone());
        // candidate keeps every credential-facing route identical but carries an explicit
        // execution-only Codex-style Responses feature on the second route.
        // candidate 保持全部面向凭据的路由一致，但第二条路由携带显式且只影响执行的
        // Responses 能力开关。
        let mut candidate_descriptor = existing_descriptor.clone();
        if let Some(route) = candidate_descriptor.service_entries[0]
            .protocol_routes
            .iter_mut()
            .find(|route| route.protocol_profile_id == "openai-chat-completions.v1")
        {
            route.responses_features = Some(vulcan_provider_adapters::ResponsesRouteFeatures {
                remote_compaction: vulcan_provider_adapters::ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback,
                websocket: vulcan_provider_adapters::ResponsesWebsocketPolicy::Disabled,
            });
        }
        let candidate = PreparedCustomProviderCandidate {
            provider_id: String::from("custom:multi"),
            updating_existing: true,
            descriptor: candidate_descriptor,
            validation_connection: None,
            temporary_secret: None,
            activation_target: CustomProviderActivationTarget {
                service_entry_id: String::from("default"),
                credential_profile_id: String::from("api-key"),
                default_protocol_profile_id: String::from("openai-responses.v1"),
                protocol_profile_ids: BTreeSet::from([
                    String::from("openai-responses.v1"),
                    String::from("openai-chat-completions.v1"),
                ]),
            },
        };
        let changed = custom_provider_connection_contract_changed(&current, &candidate)
            .expect("contract comparison should resolve");
        assert!(
            !changed,
            "execution-only Responses features must not count as a credential contract change on any route"
        );
    }

    /// Verifies bounded non-sensitive fixed headers survive prepare and registry revalidation.
    /// 验证有界非敏感固定请求头可通过保存与注册表重验。
    #[test]
    fn bounded_fixed_headers_survive_prepare_and_registry_revalidation() {
        let request = PublicAiCustomProviderSaveRequest {
            provider_id: None,
            display_name: String::from("Header Provider"),
            base_url: String::from("https://headers.example.test/v1"),
            remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
            websocket_policy: ResponsesWebsocketPolicy::Disabled,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: Some(String::from("Account")),
            secret: Some(String::from("secret")),
            validation_connection_id: None,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::from([
                (String::from("accept"), String::from("application/json")),
                (String::from("x-provider-version"), String::from("2026-08")),
            ]),
            allow_manual_fallback: false,
            allow_pending_validation: false,
            confirm_inactive_protocol_override_deletion: false,
        };
        let candidate =
            prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
                .expect("bounded fixed headers should survive prepare");
        // Runtime revalidation must accept the same headers after a reload.
        // 运行时重验必须在重载后仍接受相同的请求头。
        ProviderRegistry::effective(&BTreeMap::from([(
            candidate.descriptor.id.clone(),
            candidate.descriptor.clone(),
        )]))
        .expect("registry revalidation must accept bounded fixed headers");
    }

    /// Verifies secret-header injection is rejected at the Service save boundary.
    /// 验证服务保存边界拒绝 Secret 请求头注入。
    #[test]
    fn secret_fixed_headers_are_rejected_at_service_save_boundary() {
        for forbidden in ["authorization", "x-api-key", "cookie", "host"] {
            let request = PublicAiCustomProviderSaveRequest {
                provider_id: None,
                display_name: String::from("Unsafe Header"),
                base_url: String::from("https://unsafe.example.test/v1"),
                remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
                websocket_policy: ResponsesWebsocketPolicy::Disabled,
                authentication: CustomProviderAuthentication::BearerAuthorization,
                credential_display_name: Some(String::from("Account")),
                secret: Some(String::from("secret")),
                validation_connection_id: None,
                model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
                model_discovery_path: None,
                protocols: vec![CustomProviderProtocol::OpenAiChat],
                protocol_routes: Vec::new(),
                request_headers: BTreeMap::from([(
                    String::from(forbidden),
                    String::from("private"),
                )]),
                allow_manual_fallback: false,
                allow_pending_validation: false,
                confirm_inactive_protocol_override_deletion: false,
            };
            let error =
                prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
                    .map(|_| ())
                    .expect_err("secret fixed headers must fail at the save boundary");
            assert!(
                error.contains("request headers"),
                "unexpected rejection message: {error}"
            );
        }
    }

    /// Verifies explicit preferences must reference an enabled protocol and keys are unique.
    /// 验证显式偏好必须引用启用协议，且建键必须唯一。
    #[test]
    fn explicit_preferences_require_enabled_protocols() {
        let enabled = BTreeSet::from([
            String::from("openai-responses.v1"),
            String::from("anthropic-messages.v1"),
        ]);
        let items = vec![PublicAiCustomModelProtocolPreferenceItem {
            model_id: String::from("model-a"),
            preference: CustomModelProtocolPreference::Explicit {
                protocol: CustomProviderProtocol::AnthropicMessages,
            },
        }];
        let keyed = validate_custom_model_protocol_preferences("custom:test", &enabled, &items)
            .expect("explicit protocol inside the enabled set must save");
        assert_eq!(keyed.len(), 1);

        let items = vec![PublicAiCustomModelProtocolPreferenceItem {
            model_id: String::from("model-b"),
            preference: CustomModelProtocolPreference::Explicit {
                protocol: CustomProviderProtocol::GeminiGenerateContent,
            },
        }];
        let error = validate_custom_model_protocol_preferences("custom:test", &enabled, &items)
            .expect_err("explicit protocol outside the enabled set must be rejected");
        assert!(error.contains("not enabled"), "unexpected error: {error}");

        let items = vec![
            PublicAiCustomModelProtocolPreferenceItem {
                model_id: String::from("model-c"),
                preference: CustomModelProtocolPreference::Auto,
            },
            PublicAiCustomModelProtocolPreferenceItem {
                model_id: String::from("model-c"),
                preference: CustomModelProtocolPreference::Auto,
            },
        ];
        let error = validate_custom_model_protocol_preferences("custom:test", &enabled, &items)
            .expect_err("duplicate preferences must be rejected");
        assert!(error.contains("duplicate"), "unexpected error: {error}");
    }
}
