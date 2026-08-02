//! Public AI connection settings and runtime metadata services.
//! 公共 AI 连接设置与运行时元数据服务。

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use vulcan_ai_contract::{
    AccountUsageAvailability, AccountUsageSnapshot, AiExecutionRequest, AiMessage, AiOperation,
    AiStreamEvent, ContentItem, EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent,
    EmbeddingOutputKind, EmbeddingPayload, EmbeddingRequest, EmbeddingTask,
    INFERENCE_CONTRACT_VERSION, InferenceCallerBudget, InferenceCallerContext, InferencePermission,
    InferenceUsageScope, MessageRole, ModelCapabilities, ParallelToolCallPolicy, PromptCachePolicy,
    ReasoningControlMode, ReasoningEffort, ReasoningOutputPolicy, RerankRequest,
    RerankScoreSemantics, RerankText, RerankTruncation, StructuredOutputPolicy, ToolChoicePolicy,
};
use vulcan_provider_adapters::{
    AccountUsageQueryKind, ActiveAiModelSelection, CatalogDiscoveryState, CatalogDiscoveryStatus,
    CommonModelTemplateSuggestion, ConnectionModelAccessPolicy, ConnectionModelCatalog,
    CredentialBillingKind, CredentialConnection, CredentialMode, CredentialSecretEnvelope,
    CredentialSecretResolver, CredentialSecretStore, CustomModelDefinition,
    CustomModelTemplateDecision, CustomProviderAuthentication, CustomProviderDefinition,
    CustomProviderModelDefaults, CustomProviderModelDiscoveryMode, CustomProviderProtocol,
    InferenceProfile, InferenceRouteBinding, InferenceRouteConfiguration, InternalModelOverrides,
    ModelCapabilityOverrides, ModelDiscoveryKind, ModelExecutionOverrides,
    OAuthAuthorizationChallenge, OAuthClientConfiguration, OAuthTransactionPhase,
    OAuthTransactionSnapshot, PersistedAiConfiguration, ProviderDescriptor, ProviderKind,
    ProviderRegistry, ProviderToolCapabilityKind, ProviderToolExecutionResult, ProviderToolTarget,
    ProviderToolTargetSelection, PublicAiRuntime, ServiceEntryKind,
    registered_native_action_binding_id,
};
use vulcan_service_api::{ServiceEventEmitter, WorkbenchSnapshot};

use crate::{
    SharedServiceBridgeState,
    ai_credential_secret_store::OsAiCredentialSecretStore,
    begin_public_ai_configuration_transaction,
    configured_inference::{VMM_CONSUMER_PROFILE_ID, model_supports_disabled_reasoning},
    emit_workbench_event, mutate_public_ai_configuration,
    mutate_public_ai_configuration_in_transaction, public_ai_provider_rows_for_selection,
    shell_host_from_state,
};

/// Browser-facing public AI settings snapshot.
/// 面向浏览器的公共 AI 设置快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiSettingsSnapshot {
    /// Persisted background account-usage refresh interval.
    /// 持久化的后台账号用量刷新间隔。
    usage_refresh_interval_seconds: u64,
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
    /// Effective model access records derived from runtime catalogs and persisted choices.
    /// 从运行时目录与持久化选择派生的有效模型访问记录。
    model_access_records: Vec<PublicAiModelAccessRecord>,
    /// User-owned custom models grouped by provider without credential duplication.
    /// 按供应商分组且不复制凭据的用户自有自定义模型。
    custom_models: BTreeMap<String, BTreeMap<String, CustomModelDefinition>>,
    /// Capability defaults grouped by custom provider identifier.
    /// 按自定义供应商标识分组的能力默认值。
    custom_provider_model_defaults: BTreeMap<String, CustomProviderModelDefaults>,
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
    /// Explicit capability difference records that avoid composite-key parsing in the browser.
    /// 避免浏览器解析组合键的显式能力差异记录。
    model_capability_override_records: Vec<PublicAiModelCapabilityOverrideRecord>,
    /// Available exact provider tool targets through configured credentials.
    /// 通过已配置凭据可用的精确供应商工具目标。
    provider_tool_targets: Vec<PublicAiProviderToolTargetSnapshot>,
    /// One explicit selected target per semantic provider tool capability.
    /// 每项语义供应商工具能力唯一显式选择的目标。
    provider_tool_selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    /// Provider tool capabilities enabled by default for newly materialized sessions.
    /// 为新物化会话默认启用的供应商工具能力。
    provider_tool_enabled_capabilities: BTreeSet<ProviderToolCapabilityKind>,
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
    route: InferenceRouteConfiguration,
    /// Process-local catalog generation that established the model route.
    /// 建立该模型路由的进程内目录代次。
    catalog_generation: u64,
    /// Whether the model operation is enabled for internal use.
    /// 当前模型操作是否已为内部使用启用。
    enabled: bool,
    /// Exact usage scopes permitted by product, credential, and user narrowing.
    /// 产品、凭据与用户收窄共同允许的精确用途范围。
    usage_scopes: BTreeSet<InferenceUsageScope>,
    /// Exact model capability contract used to generate internal controls.
    /// 用于生成内部控件的精确模型能力契约。
    capabilities: ModelCapabilities,
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
    /// Exact model- or service-native target contract.
    /// 精确的模型或服务原生目标契约。
    target: ProviderToolTarget,
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
    /// Verified protocol profile identifiers.
    /// 已验证协议 Profile 标识。
    verified_protocols: BTreeSet<String>,
    /// Currently selected protocol profile.
    /// 当前选定协议 Profile。
    selected_protocol: String,
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
    /// `runtime` owns the strict provider registry and `connection` is the persisted connection.
    /// `runtime` 持有严格供应商注册表，`connection` 是持久化连接。
    ///
    /// # Returns
    /// 返回值
    /// A complete browser-safe snapshot with resolved credential semantics.
    /// 返回带已解析凭据语义的完整浏览器安全快照。
    fn new(
        runtime: &PublicAiRuntime,
        configuration: &PersistedAiConfiguration,
        connection: &CredentialConnection,
    ) -> Result<Self, String> {
        let providers = runtime.effective_providers(configuration)?;
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
            has_management_key: connection.management_secret_reference.is_some(),
            service_origin_override: connection.service_origin_override.clone(),
            credential_mode: connection.credential_mode,
            credential_id: connection.credential_id.clone(),
            display_name: connection.display_name.clone(),
            verified_protocols: connection.verified_protocols.clone(),
            selected_protocol: connection.selected_protocol.clone(),
            prompt_cache_by_protocol: connection.prompt_cache_by_protocol.clone(),
            pending_validation: configuration
                .connection_is_pending_validation(connection.id.as_str()),
        })
    }
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

/// Browser-facing provider-model access state.
/// 面向浏览器的供应商模型访问状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicAiModelAccessRecord {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Effective enabled state after applying the persisted user choice.
    /// 应用持久化用户选择后的有效启用状态。
    enabled: bool,
    /// Code-owned provider default before applying a user choice.
    /// 应用用户选择前由代码拥有的供应商默认值。
    default_enabled: bool,
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
    /// Selected verified protocol profile.
    /// 选定的已验证协议 Profile。
    selected_protocol: String,
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
    /// Selected verified protocol profile.
    /// 选定的已验证协议 Profile。
    selected_protocol: String,
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
    /// Complete replacement override value.
    /// 完整替换覆盖值。
    overrides: ModelExecutionOverrides,
}

/// Saves one provider-model access choice independent from credentials and protocols.
/// 保存一个独立于凭据和协议的供应商模型访问选择。
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
}

/// Saves one enabled state for every synchronized model owned by a provider.
/// 为一个供应商拥有的全部已同步模型保存同一启用状态。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiProviderModelsEnabledSaveRequest {
    /// Exact provider identifier whose synchronized model set is mutated.
    /// 要修改其已同步模型集合的精确供应商标识。
    provider_id: String,
    /// Requested enabled state for the complete provider model set.
    /// 供应商完整模型集合请求的启用状态。
    enabled: bool,
}

/// Saves one provider-model-operation internal access choice.
/// 保存一项供应商、模型与操作作用域的内部访问选择。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiInternalModelEnabledSaveRequest {
    /// Exact provider identifier.
    /// 精确供应商标识。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// Exact internal operation.
    /// 精确内部操作。
    operation: AiOperation,
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
    /// The single compatible protocol served by this base URL.
    /// 此基础 URL 提供的唯一兼容协议。
    protocol: CustomProviderProtocol,
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
    /// Allows an explicitly confirmed manual fallback after unsupported, invalid, or empty discovery.
    /// 允许在目录不受支持、格式无效或为空后使用已明确确认的手工回退。
    #[serde(default)]
    allow_manual_fallback: bool,
    /// Allows saving a pending provider after a retryable discovery failure.
    /// 允许在可重试发现失败后将供应商保存为待验证状态。
    #[serde(default)]
    allow_pending_validation: bool,
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

/// Saves provider-wide fallback capabilities for one custom provider.
/// 保存一个自定义供应商的供应商级回退能力。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiCustomProviderModelDefaultsSaveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Complete provider fallback contract.
    /// 完整供应商回退契约。
    defaults: CustomProviderModelDefaults,
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

/// Carries one isolated custom-provider candidate through validation and optional persistence.
/// 承载一个隔离的自定义供应商候选项，供验证及可选持久化使用。
struct PreparedCustomProviderCandidate {
    /// Stable candidate provider identifier.
    /// 稳定的候选供应商标识。
    provider_id: String,
    /// Strict projected provider descriptor.
    /// 严格投影后的供应商描述。
    descriptor: ProviderDescriptor,
    /// Temporary connection used by the metadata request.
    /// 元数据请求使用的临时连接。
    connection: CredentialConnection,
    /// New candidate-owned secret created only for first-provider setup.
    /// 仅在首次供应商设置时创建且由候选项拥有的新秘密。
    temporary_secret: Option<CredentialSecretEnvelope>,
    /// Whether the request edits one existing provider.
    /// 请求是否编辑一个现有供应商。
    updating_existing: bool,
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
    let descriptor = CustomProviderDefinition {
        id: provider_id.clone(),
        display_name: request.display_name.clone(),
        base_url: request.base_url.clone(),
        protocol: request.protocol,
        authentication: request.authentication,
        model_discovery_mode: request.model_discovery_mode,
        model_discovery_path: request.model_discovery_path.clone(),
    }
    .into_descriptor()?;
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
    let temporary_connection_id = format!("connection_{}", uuid::Uuid::new_v4().simple());
    let (connection, temporary_secret) = if updating_existing {
        if request.credential_display_name.is_some() || request.secret.is_some() {
            return Err(String::from(
                "Editing a custom provider must reuse one existing credential and cannot create a new secret.",
            ));
        }
        let validation_connection_id =
            request.validation_connection_id.as_deref().ok_or_else(|| {
                String::from("validationConnectionId is required while editing a custom provider.")
            })?;
        let existing_connection = current
            .connection_graph
            .connections
            .get(validation_connection_id)
            .ok_or_else(|| {
                format!("Validation connection `{validation_connection_id}` is not configured.")
            })?;
        if existing_connection.provider_id != provider_id {
            return Err(format!(
                "Validation connection `{validation_connection_id}` does not belong to custom provider `{provider_id}`."
            ));
        }
        (
            CredentialConnection {
                id: temporary_connection_id,
                provider_id: provider_id.clone(),
                service_entry_id: entry.id.clone(),
                credential_profile_id: profile.id.clone(),
                credential_mode: existing_connection.credential_mode,
                credential_id: existing_connection.credential_id.clone(),
                display_name: existing_connection.display_name.clone(),
                secret_reference: existing_connection.secret_reference.clone(),
                management_secret_reference: existing_connection
                    .management_secret_reference
                    .clone(),
                service_origin_override: existing_connection.service_origin_override.clone(),
                verified_protocols: BTreeSet::from([route.protocol_profile_id.clone()]),
                selected_protocol: route.protocol_profile_id.clone(),
                prompt_cache_by_protocol: existing_connection.prompt_cache_by_protocol.clone(),
                usage_scope_override: existing_connection.usage_scope_override.clone(),
            },
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
            CredentialConnection {
                id: temporary_connection_id,
                provider_id: provider_id.clone(),
                service_entry_id: entry.id.clone(),
                credential_profile_id: profile.id.clone(),
                credential_mode: CredentialMode::Token,
                credential_id: format!("credential_{}", uuid::Uuid::new_v4().simple()),
                display_name: credential_display_name.trim().to_string(),
                secret_reference,
                management_secret_reference: None,
                service_origin_override: None,
                verified_protocols: BTreeSet::from([route.protocol_profile_id.clone()]),
                selected_protocol: route.protocol_profile_id.clone(),
                prompt_cache_by_protocol: BTreeMap::new(),
                usage_scope_override: None,
            },
            Some(CredentialSecretEnvelope::new_api_key(secret.to_string())?),
        )
    };
    Ok(PreparedCustomProviderCandidate {
        provider_id,
        descriptor,
        connection,
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
    let candidate_protocol = candidate.connection.selected_protocol.as_str();
    for existing_connection in current
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == candidate.provider_id)
    {
        if existing_connection.selected_protocol != candidate_protocol {
            return Err(format!(
                "Custom provider `{}` cannot change protocol while credential `{}` uses `{}`.",
                candidate.provider_id,
                existing_connection.display_name,
                existing_connection.selected_protocol
            ));
        }
    }
    let mut validation = current.clone();
    validation
        .custom_providers
        .insert(candidate.provider_id.clone(), candidate.descriptor.clone());
    let providers = runtime.effective_providers(&validation)?;
    validation
        .connection_graph
        .add_connection(&providers, candidate.connection.clone())?;
    Ok(validation)
}

/// Executes one candidate discovery request while retaining its typed status for policy decisions.
/// 执行一次候选目录发现请求，并保留其类型化状态供策略决策使用。
///
/// # Parameters
/// 参数
/// `runtime` owns process-local catalogs, `validation` is isolated, and `candidate` owns the temporary secret.
/// `runtime` 持有进程内目录，`validation` 是隔离配置，`candidate` 持有临时秘密。
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
    let secret_store = OsAiCredentialSecretStore;
    if let Some(secret) = candidate.temporary_secret.as_ref() {
        CredentialSecretStore::create_secret(
            &secret_store,
            candidate.connection.secret_reference.as_str(),
            secret,
        )?;
    }
    let refresh_error = runtime
        .refresh_connection_catalog(validation, candidate.connection.id.as_str())
        .await
        .err();
    let status = runtime
        .catalog_status_snapshot()?
        .remove(candidate.connection.id.as_str())
        .ok_or_else(|| {
            refresh_error.clone().unwrap_or_else(|| {
                String::from("Custom provider discovery did not publish a typed status.")
            })
        })?;
    let catalog = runtime
        .source_catalog_snapshot()?
        .remove(candidate.connection.id.as_str());
    Ok(CustomProviderCandidateObservation {
        status,
        catalog,
        refresh_error,
    })
}

/// Removes every process-local and secure-store artifact owned by one temporary candidate.
/// 移除一个临时候选项拥有的全部进程内与安全存储工件。
///
/// # Parameters
/// 参数
/// `runtime` owns candidate catalogs and `candidate` identifies the exact secret and connection.
/// `runtime` 持有候选目录，`candidate` 标识精确秘密与连接。
///
/// # Returns
/// 返回值
/// Returns success only when both cleanup operations succeed.
/// 仅当两项清理操作都成功时返回成功。
fn cleanup_custom_provider_candidate(
    runtime: &PublicAiRuntime,
    candidate: &PreparedCustomProviderCandidate,
) -> Result<(), String> {
    let secret_store = OsAiCredentialSecretStore;
    let secret_cleanup = if candidate.temporary_secret.is_some() {
        CredentialSecretStore::delete_secret(
            &secret_store,
            candidate.connection.secret_reference.as_str(),
        )
    } else {
        Ok(())
    };
    combine_results(
        secret_cleanup,
        runtime.remove_connection_catalog(candidate.connection.id.as_str()),
    )
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
/// Returns typed discovery status and raw model facts after guaranteed temporary cleanup.
/// 在保证清理临时状态后返回类型化发现状态与原始模型事实。
pub(super) async fn service_probe_public_ai_custom_provider(
    state: SharedServiceBridgeState,
    request: PublicAiCustomProviderSaveRequest,
) -> Result<PublicAiCustomProviderProbeResult, String> {
    let (runtime, current) = public_ai_runtime_and_configuration(&state)?;
    let candidate = prepare_custom_provider_candidate(&request, &current)?;
    let validation =
        custom_provider_validation_configuration(runtime.as_ref(), &current, &candidate)?;
    let observation =
        observe_custom_provider_candidate(runtime.as_ref(), &validation, &candidate).await;
    let cleanup = cleanup_custom_provider_candidate(runtime.as_ref(), &candidate);
    let observation =
        observation.map_err(|error| compensated_operation_error(error, cleanup.clone(), Ok(())))?;
    cleanup?;
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
    let validation_configuration = custom_provider_validation_configuration(
        runtime.as_ref(),
        &current_configuration,
        &candidate,
    )?;
    let observation = match observe_custom_provider_candidate(
        runtime.as_ref(),
        &validation_configuration,
        &candidate,
    )
    .await
    {
        Ok(observation) => observation,
        Err(error) => {
            let cleanup = cleanup_custom_provider_candidate(runtime.as_ref(), &candidate);
            return Err(compensated_operation_error(error, cleanup, Ok(())));
        }
    };
    let save_as_pending = custom_provider_save_requires_pending_validation(
        observation.status.state,
        request.allow_manual_fallback,
        request.allow_pending_validation,
    );
    let accepted = custom_provider_observation_is_accepted(
        observation.status.state,
        request.allow_manual_fallback,
        request.allow_pending_validation,
    );
    if !accepted {
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
        let cleanup = cleanup_custom_provider_candidate(runtime.as_ref(), &candidate);
        return Err(compensated_operation_error(
            format!(
                "Custom provider validation requires an explicit supported action: {diagnostic}"
            ),
            cleanup,
            Ok(()),
        ));
    }

    if candidate.updating_existing {
        // Existing-provider validation artifacts are temporary and must be removed before the
        // durable edit, so a cleanup failure cannot report a false transactional rollback.
        // 既有供应商的验证工件属于临时数据，必须在持久化编辑前移除，避免清理失败造成虚假的事务回滚提示。
        cleanup_custom_provider_candidate(runtime.as_ref(), &candidate)?;
    }

    let provider_id = candidate.provider_id.clone();
    let updating_existing = candidate.updating_existing;
    let descriptor = candidate.descriptor.clone();
    let connection = candidate.connection.clone();
    let persist_result = mutate_public_ai_configuration_in_transaction(
        &state,
        "保存自定义供应商失败",
        |configuration| {
            configuration
                .custom_providers
                .insert(provider_id.clone(), descriptor.clone());
            configuration
                .custom_provider_model_defaults
                .entry(provider_id.clone())
                .or_default();
            if !updating_existing {
                let effective_providers = runtime.effective_providers(configuration)?;
                configuration
                    .connection_graph
                    .add_connection(&effective_providers, connection.clone())?;
                configuration
                    .set_connection_pending_validation(connection.id.as_str(), save_as_pending)?;
            } else if save_as_pending {
                let connection_ids = configuration
                    .connection_graph
                    .connections
                    .values()
                    .filter(|existing_connection| existing_connection.provider_id == provider_id)
                    .map(|existing_connection| existing_connection.id.clone())
                    .collect::<Vec<_>>();
                for connection_id in connection_ids {
                    configuration
                        .set_connection_pending_validation(connection_id.as_str(), true)?;
                }
            }
            Ok(())
        },
    );
    let configuration = match persist_result {
        Ok(configuration) => configuration,
        Err(error) => {
            // Existing-provider artifacts were already removed before persistence; new-provider
            // artifacts must still be compensated when their first durable write fails.
            // 既有供应商工件已在持久化前移除；新供应商首次持久化失败时仍需补偿其临时工件。
            let cleanup = if updating_existing {
                Ok(())
            } else {
                cleanup_custom_provider_candidate(runtime.as_ref(), &candidate)
            };
            return Err(compensated_operation_error(error, cleanup, Ok(())));
        }
    };

    if updating_existing {
        let existing_connection_ids = configuration
            .connection_graph
            .connections
            .values()
            .filter(|existing_connection| existing_connection.provider_id == provider_id)
            .map(|existing_connection| existing_connection.id.clone())
            .collect::<Vec<_>>();
        for existing_connection_id in existing_connection_ids {
            runtime.remove_connection_catalog(existing_connection_id.as_str())?;
            // The provider edit is already durable; each refresh publishes its own typed status and
            // a failed remote directory must not roll the descriptor back.
            // 供应商编辑已经持久化；每次刷新都会发布自己的类型化状态，远程目录失败不得回滚描述。
            let _refresh_result = runtime
                .refresh_connection_catalog(&configuration, existing_connection_id.as_str())
                .await;
        }
    }

    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let execution_id = format!("custom-model-validation-{}", uuid::Uuid::new_v4().simple());
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
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: ToolChoicePolicy::Disabled,
        parallel_tool_calls: ParallelToolCallPolicy::Disabled,
        structured_output: StructuredOutputPolicy::None,
        stop_sequences: Vec::new(),
        prompt_cache: selection.overrides.prompt_cache.clone().unwrap_or_default(),
        reasoning_effort,
        reasoning_output: ReasoningOutputPolicy::Hidden,
        model_parameters: selection.overrides.model_parameters.clone(),
        resources: Vec::new(),
        output_token_limit: Some(output_limit),
    };
    let mut stream = runtime
        .execute_internal_llm(&selection, execution_request, execution_id)
        .await
        .map_err(|failure| failure.to_string())?;
    let mut response_text = String::new();
    let mut terminal_observed = false;
    while let Some(event) = stream.next().await {
        match event.map_err(|failure| failure.to_string())? {
            AiStreamEvent::ResponseItemStarted {
                item: ContentItem::Text { text },
                ..
            }
            | AiStreamEvent::ResponseTextDelta { delta: text, .. } => {
                response_text.push_str(text.as_str());
            }
            AiStreamEvent::ResponseCompleted => {
                terminal_observed = true;
                break;
            }
            AiStreamEvent::ResponseFailed { error } => return Err(error.message),
            AiStreamEvent::ResponseCancelled => {
                return Err(String::from("Custom model validation was cancelled."));
            }
            _ => {}
        }
    }
    if !terminal_observed {
        return Err(String::from(
            "Custom model validation stream ended before a terminal completion event.",
        ));
    }
    let response_text = response_text.trim().to_string();
    if response_text.is_empty() {
        return Err(String::from(
            "Custom model validation completed without visible text output.",
        ));
    }

    let provider_id = request.provider_id.clone();
    let connection_id = request.connection_id.clone();
    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
        "保存自定义模型验证结果失败",
        |configuration| {
            let current_connection = configuration
                .connection_graph
                .connections
                .get(connection_id.as_str())
                .ok_or_else(|| format!("Connection `{connection_id}` is no longer configured."))?;
            if current_connection.provider_id != provider_id
                || current_connection.selected_protocol
                    != selection.route.connection.selected_protocol
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
    )?;
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    Ok(PublicAiCustomModelValidationResult {
        provider_id: request.provider_id,
        connection_id: request.connection_id,
        model_id: request.model_id,
        response_characters: response_text.chars().count(),
        response_text,
    })
}

/// Removes one unreferenced custom provider.
/// 移除一个未被引用的自定义供应商。
pub(super) async fn service_remove_public_ai_custom_provider(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomProviderRemoveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("providerId", request.provider_id.as_str())?;
    let provider_id = request.provider_id;
    let configuration =
        mutate_public_ai_configuration(&state, "移除自定义供应商失败", |configuration| {
            if configuration
                .connection_graph
                .connections
                .values()
                .any(|connection| connection.provider_id == provider_id)
            {
                return Err(format!(
                    "Custom provider `{provider_id}` still has credentials and cannot be removed."
                ));
            }
            if configuration
                .custom_models
                .get(provider_id.as_str())
                .is_some_and(|models| !models.is_empty())
            {
                return Err(format!(
                    "Custom provider `{provider_id}` still has model definitions. Remove its models before removing the provider."
                ));
            }
            let removed = configuration
                .custom_providers
                .remove(provider_id.as_str())
                .ok_or_else(|| format!("Custom provider `{provider_id}` is not configured."))?;
            if removed.kind != ProviderKind::Custom {
                return Err(format!(
                    "Provider `{provider_id}` is not a user-defined provider."
                ));
            }
            configuration.remove_provider_owned_model_settings(provider_id.as_str());
            Ok(())
        })
        .await?;
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let mut referenced_removed_models = referenced_custom_model_ids(
        runtime.as_ref(),
        &candidate_configuration,
        provider_id.as_str(),
        &removed_model_ids,
    )?;
    // Loaded session selections are runtime-owned and must be checked separately from persisted defaults.
    // 已加载会话选择由运行时持有，必须与持久化默认值分开检查。
    // loaded_session_references is copied while holding service state so catalog resolution never
    // nests runtime locks under the workspace mutex.
    // loaded_session_references 在持有服务状态锁时完成复制，避免在工作区互斥锁内嵌套目录锁。
    let loaded_session_references = {
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
            .filter_map(|runtime_state| {
                runtime_state
                    .active_model_id
                    .as_ref()
                    .map(|model_id| (runtime_state.active_connection_id.clone(), model_id.clone()))
            })
            .collect::<BTreeSet<_>>()
    };
    for (connection_id, model_id) in loaded_session_references {
        if !model_remains_on_connection(
            runtime.as_ref(),
            &candidate_configuration,
            connection_id.as_deref(),
            model_id.as_str(),
        )? {
            referenced_removed_models.insert(model_id);
        }
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

    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
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
    )?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    request.defaults.validate()?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_custom_provider_identity(&current_configuration, request.provider_id.as_str())?;
    let provider_id = request.provider_id;
    let defaults = request.defaults;
    let mut candidate_configuration = current_configuration.clone();
    candidate_configuration
        .custom_provider_model_defaults
        .insert(provider_id.clone(), defaults.clone());
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;
    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
        "保存自定义供应商模型默认值失败",
        |configuration| {
            configuration
                .custom_provider_model_defaults
                .insert(provider_id.clone(), defaults.clone());
            Ok(())
        },
    )?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
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
    )?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
        "批量保存自定义模型模板决策失败",
        |configuration| {
            for (key, decision) in decisions.clone() {
                configuration
                    .custom_model_template_decisions
                    .insert(key, decision);
            }
            Ok(())
        },
    )?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
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
    )?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
            &candidate_configuration,
            &candidate_catalogs,
            connection.provider_id.as_str(),
            request.model_id.as_str(),
        )
    } else {
        None
    };
    if active_was_disabled {
        candidate_configuration.active = replacement.clone();
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;

    let active_selection = candidate_configuration.active.clone();
    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
        "保存凭据模型启用状态失败",
        |configuration| {
            configuration.set_connection_model_enabled(
                request.connection_id.as_str(),
                request.model_id.as_str(),
                request.enabled,
            );
            if active_was_disabled {
                configuration.active = active_selection.clone();
            }
            Ok(())
        },
    )?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    if !request.enabled {
        reconcile_disabled_connection_sessions(
            &state,
            request.connection_id.as_str(),
            Some(request.model_id.as_str()),
            replacement.as_ref(),
        )?;
    }
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let supported_model_count = runtime
        .resolved_model_catalog_snapshot(&support_configuration)?
        .get(request.connection_id.as_str())
        .map_or(0, |catalog| catalog.models.len());
    if supported_model_count == 0 {
        return Err(format!(
            "Credential connection `{}` has no supported models.",
            request.connection_id
        ));
    }

    let mut candidate_configuration = current_configuration.clone();
    candidate_configuration
        .set_connection_models_enabled(request.connection_id.as_str(), request.enabled);
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
            &candidate_configuration,
            &candidate_catalogs,
            connection.provider_id.as_str(),
            active_model_id.as_str(),
        )
    } else {
        None
    };
    if active_was_disabled {
        candidate_configuration.active = replacement.clone();
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;

    let active_selection = candidate_configuration.active.clone();
    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
        "批量保存凭据模型启用状态失败",
        |configuration| {
            configuration
                .set_connection_models_enabled(request.connection_id.as_str(), request.enabled);
            if active_was_disabled {
                configuration.active = active_selection.clone();
            }
            Ok(())
        },
    )?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    if !request.enabled {
        reconcile_disabled_connection_sessions(
            &state,
            request.connection_id.as_str(),
            None,
            replacement.as_ref(),
        )?;
    }
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let catalogs = runtime.catalog_snapshot()?;
    let model_exists = current_configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == request.provider_id)
        .any(|connection| {
            catalogs.get(connection.id.as_str()).is_some_and(|catalog| {
                catalog
                    .models
                    .iter()
                    .any(|model| model.id == request.model_id)
            })
        });
    if !model_exists {
        return Err(format!(
            "Provider `{}` does not expose synchronized model `{}`.",
            request.provider_id, request.model_id
        ));
    }
    runtime.validate_model_capability_override_candidate(
        &current_configuration,
        request.provider_id.as_str(),
        request.model_id.as_str(),
        request.overrides.as_ref(),
    )?;
    let key = PersistedAiConfiguration::provider_model_key(
        request.provider_id.as_str(),
        request.model_id.as_str(),
    );
    let configuration = mutate_public_ai_configuration_in_transaction(
        &state,
        "保存模型能力覆盖失败",
        |configuration| {
            if let Some(overrides) = request.overrides.clone() {
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
    )?;
    runtime.reapply_model_catalog_overlays(&configuration)?;
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    /// Selected verified protocol profile.
    /// 选定的已验证协议 Profile。
    selected_protocol: String,
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
    let oauth_provider = credential_profile.oauth_provider.ok_or_else(|| {
        format!(
            "Credential profile `{}` does not declare an OAuth flow.",
            request.credential_profile_id
        )
    })?;
    if credential_profile.credential_mode != CredentialMode::Auth {
        return Err(format!(
            "Credential profile `{}` does not support renewable authorization.",
            request.credential_profile_id
        ));
    }
    let challenge = runtime
        .oauth()
        .start(oauth_provider, request.client)
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
    if credential_profile.oauth_provider != Some(snapshot.provider_kind) {
        return Err(String::from(
            "OAuth transaction does not belong to the selected service entry.",
        ));
    }
    let material = runtime
        .oauth()
        .take_succeeded(request.transaction_id.as_str())
        .await?;
    if let Some(connection_id) = request.connection_id.as_deref() {
        let transaction = begin_public_ai_configuration_transaction().await;
        let (_, current_configuration) = public_ai_runtime_and_configuration(&state)?;
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
        let secret_store = OsAiCredentialSecretStore;
        let previous = CredentialSecretResolver::read_secret(
            &secret_store,
            current.secret_reference.as_str(),
        )?;
        let replacement = previous.advance_with_material(material)?;
        CredentialSecretStore::replace_secret(
            &secret_store,
            current.secret_reference.as_str(),
            previous.revision,
            &replacement,
        )?;
        let connection_id = connection_id.to_string();
        let update_request = PublicAiConnectionUpdateRequest {
            connection_id: connection_id.clone(),
            display_name: request.display_name.clone(),
            service_entry_id: request.service_entry_id.clone(),
            credential_profile_id: request.credential_profile_id.clone(),
            credential_mode: CredentialMode::Auth,
            selected_protocol: request.selected_protocol.clone(),
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
            rollback_replaced_secret(
                &secret_store,
                current.secret_reference.as_str(),
                Some(&(previous, replacement)),
            )?;
            return Err(error);
        }
        if let Err(error) = runtime
            .refresh_connection_catalog(&validation_configuration, connection_id.as_str())
            .await
        {
            let rollback_result = rollback_replaced_secret(
                &secret_store,
                current.secret_reference.as_str(),
                Some(&(previous.clone(), replacement.clone())),
            );
            let restore_result = runtime
                .refresh_connection_catalog(&current_configuration, connection_id.as_str())
                .await
                .map(|_| ());
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
                let rollback_result = rollback_replaced_secret(
                    &secret_store,
                    current.secret_reference.as_str(),
                    Some(&(previous, replacement)),
                );
                let restore_result = runtime
                    .refresh_connection_catalog(&current_configuration, connection_id.as_str())
                    .await
                    .map(|_| ());
                return Err(compensated_operation_error(
                    error,
                    rollback_result,
                    restore_result,
                ));
            }
        };
        drop(transaction);
        publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
        return build_public_ai_settings_snapshot(runtime.as_ref(), &configuration);
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
        request.selected_protocol,
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
    let (service_entry_id, credential_profile_id) = if request.provider_id == "minimax" {
        let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
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
        request.selected_protocol,
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
    selected_protocol: String,
    secret: CredentialSecretEnvelope,
    management_key: Option<String>,
    service_origin_override: Option<String>,
) -> Result<PublicAiSettingsSnapshot, String> {
    if provider_id != "openrouter"
        && management_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(String::from(
            "Provider management keys are currently supported only for OpenRouter.",
        ));
    }
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
    let connection_id = format!("connection_{}", uuid::Uuid::new_v4().simple());
    let credential_id = format!("credential_{}", uuid::Uuid::new_v4().simple());
    let secret_reference = format!("ai_secret_{}", uuid::Uuid::new_v4().simple());
    let management_secret_reference = management_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|_| format!("ai_management_secret_{}", uuid::Uuid::new_v4().simple()));
    let providers = runtime.effective_providers(&current_configuration)?;
    let service_origin_override =
        vulcan_provider_adapters::normalize_connection_service_origin_override(
            provider_id.as_str(),
            service_entry_id.as_str(),
            service_origin_override.as_deref(),
        )?;
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
    let credential_profile = entry
        .credential_profile(credential_profile_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential profile `{credential_profile_id}` is not registered for service entry `{service_entry_id}`."
            )
        })?;
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
    let verified_protocols = entry
        .protocol_routes
        .iter()
        .filter(|route| route.verified)
        .map(|route| route.protocol_profile_id.clone())
        .collect();
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
        verified_protocols,
        selected_protocol,
        prompt_cache_by_protocol: BTreeMap::new(),
        usage_scope_override: None,
    };
    let secret_store = OsAiCredentialSecretStore;
    CredentialSecretStore::create_secret(&secret_store, secret_reference.as_str(), &secret)?;
    if let (Some(reference), Some(value)) = (
        management_secret_reference.as_deref(),
        management_key
            .as_ref()
            .filter(|value| !value.trim().is_empty()),
    ) {
        let management_secret = CredentialSecretEnvelope::new_api_key(value.clone())?;
        if let Err(error) =
            CredentialSecretStore::create_secret(&secret_store, reference, &management_secret)
        {
            let cleanup =
                CredentialSecretStore::delete_secret(&secret_store, secret_reference.as_str());
            return Err(compensated_operation_error(error, cleanup, Ok(())));
        }
    }
    let mut validation_configuration = current_configuration;
    if let Err(error) = validation_configuration
        .connection_graph
        .add_connection(&providers, connection.clone())
    {
        return match delete_connection_secrets(
            &secret_store,
            secret_reference.as_str(),
            management_secret_reference.as_deref(),
        ) {
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
        let catalog_cleanup = runtime.remove_connection_catalog(connection_id.as_str());
        let secret_cleanup = delete_connection_secrets(
            &secret_store,
            secret_reference.as_str(),
            management_secret_reference.as_deref(),
        );
        return Err(compensated_operation_error(
            format!("Credential validation failed before the connection was saved: {error}"),
            secret_cleanup,
            catalog_cleanup,
        ));
    }
    if management_secret_reference.is_some() {
        let usage = match runtime
            .query_connection_usage(&validation_configuration, connection_id.as_str())
            .await
        {
            Ok(usage) => usage,
            Err(error) => {
                let catalog_cleanup = combine_results(
                    runtime.remove_connection_catalog(connection_id.as_str()),
                    runtime.remove_connection_usage(connection_id.as_str()),
                );
                let secret_cleanup = delete_connection_secrets(
                    &secret_store,
                    secret_reference.as_str(),
                    management_secret_reference.as_deref(),
                );
                return Err(compensated_operation_error(
                    error,
                    secret_cleanup,
                    catalog_cleanup,
                ));
            }
        };
        if usage.availability != AccountUsageAvailability::Supported {
            let catalog_cleanup = combine_results(
                runtime.remove_connection_catalog(connection_id.as_str()),
                runtime.remove_connection_usage(connection_id.as_str()),
            );
            let secret_cleanup = delete_connection_secrets(
                &secret_store,
                secret_reference.as_str(),
                management_secret_reference.as_deref(),
            );
            return Err(compensated_operation_error(
                usage.refresh_message.unwrap_or_else(|| {
                    String::from("OpenRouter management key validation failed.")
                }),
                secret_cleanup,
                catalog_cleanup,
            ));
        }
    }
    let add_result =
        mutate_public_ai_configuration(&state, "保存公共 AI 连接失败", |configuration| {
            let effective_providers = runtime.effective_providers(configuration)?;
            configuration
                .connection_graph
                .add_connection(&effective_providers, connection.clone())
        })
        .await;
    let configuration = match add_result {
        Ok(configuration) => configuration,
        Err(error) => {
            let catalog_cleanup = combine_results(
                runtime.remove_connection_catalog(connection_id.as_str()),
                runtime.remove_connection_usage(connection_id.as_str()),
            );
            let secret_cleanup = delete_connection_secrets(
                &secret_store,
                secret_reference.as_str(),
                management_secret_reference.as_deref(),
            );
            return Err(compensated_operation_error(
                error,
                secret_cleanup,
                catalog_cleanup,
            ));
        }
    };
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let providers = runtime.effective_providers(configuration)?;
    let provider = providers
        .provider(provider_id)
        .ok_or_else(|| format!("Provider `{provider_id}` is not registered."))?;
    provider
        .service_entries
        .iter()
        .find(|entry| entry.id == service_entry_id)
        .map(|entry| entry.kind)
        .ok_or_else(|| {
            format!(
                "Service entry `{service_entry_id}` is not registered for provider `{provider_id}`."
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
    if current.provider_id != "openrouter"
        && (request.management_key.is_some() || request.remove_management_key)
    {
        return Err(String::from(
            "Provider management keys are currently supported only for OpenRouter.",
        ));
    }
    if current.provider_id == "minimax"
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
    request.service_origin_override =
        vulcan_provider_adapters::normalize_connection_service_origin_override(
            current.provider_id.as_str(),
            request.service_entry_id.as_str(),
            request.service_origin_override.as_deref(),
        )?;
    let current_entry_kind = registered_service_entry_kind(
        runtime.as_ref(),
        &current_configuration,
        current.provider_id.as_str(),
        current.service_entry_id.as_str(),
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
        || current.selected_protocol != request.selected_protocol
        || current.service_origin_override != request.service_origin_override
        || request.secret.is_some();
    let secret_store = OsAiCredentialSecretStore;
    let replaced_secret = if let Some(secret) = request.secret.as_deref() {
        validate_non_empty("secret", secret)?;
        if request.credential_mode != CredentialMode::Token {
            return Err(String::from(
                "Renewable account authorization must be updated through the Auth login flow.",
            ));
        }
        let previous = CredentialSecretResolver::read_secret(
            &secret_store,
            current.secret_reference.as_str(),
        )?;
        let replacement = previous.replace_api_key(secret.to_string())?;
        CredentialSecretStore::replace_secret(
            &secret_store,
            current.secret_reference.as_str(),
            previous.revision,
            &replacement,
        )?;
        Some((previous, replacement))
    } else {
        None
    };
    let management_mutation = match prepare_management_secret_mutation(
        &secret_store,
        &current,
        request.management_key.as_deref(),
        request.remove_management_key,
    ) {
        Ok(mutation) => mutation,
        Err(error) => {
            let rollback = rollback_replaced_secret(
                &secret_store,
                current.secret_reference.as_str(),
                replaced_secret.as_ref(),
            );
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
            &secret_store,
            current.secret_reference.as_str(),
            replaced_secret.as_ref(),
        )?;
        rollback_management_secret_mutation(&secret_store, &management_mutation)?;
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
            &secret_store,
            current.secret_reference.as_str(),
            replaced_secret.as_ref(),
        );
        let management_rollback =
            rollback_management_secret_mutation(&secret_store, &management_mutation);
        let restore_result = validate_connection_credential(
            runtime.as_ref(),
            &current_configuration,
            connection_id.as_str(),
            current_entry_kind,
        )
        .await;
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
            Ok(snapshot) => {
                Some(snapshot.refresh_message.unwrap_or_else(|| {
                    String::from("OpenRouter management key validation failed.")
                }))
            }
            Err(error) => Some(error),
        };
        if let Some(error) = validation_error {
            let primary_rollback = rollback_replaced_secret(
                &secret_store,
                current.secret_reference.as_str(),
                replaced_secret.as_ref(),
            );
            let management_rollback =
                rollback_management_secret_mutation(&secret_store, &management_mutation);
            let cache_cleanup = runtime.remove_connection_usage(connection_id.as_str());
            return Err(compensated_operation_error(
                error,
                combine_results(primary_rollback, management_rollback),
                cache_cleanup,
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
                &secret_store,
                current.secret_reference.as_str(),
                replaced_secret.as_ref(),
            );
            let management_rollback =
                rollback_management_secret_mutation(&secret_store, &management_mutation);
            let cache_cleanup = runtime.remove_connection_usage(connection_id.as_str());
            let restore_result = validate_connection_credential(
                runtime.as_ref(),
                &current_configuration,
                connection_id.as_str(),
                current_entry_kind,
            )
            .await;
            return Err(compensated_operation_error(
                error,
                combine_results(rollback_result, management_rollback),
                combine_results(cache_cleanup, restore_result),
            ));
        }
    };
    drop(transaction);
    if management_mutation.removes_key() {
        runtime.remove_connection_usage(connection_id.as_str())?;
    }
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    candidate.selected_protocol = request.selected_protocol.clone();
    candidate.service_origin_override =
        vulcan_provider_adapters::normalize_connection_service_origin_override(
            candidate.provider_id.as_str(),
            request.service_entry_id.as_str(),
            request.service_origin_override.as_deref(),
        )?;
    candidate.verified_protocols = entry
        .protocol_routes
        .iter()
        .filter(|route| route.verified)
        .map(|route| route.protocol_profile_id.clone())
        .collect();
    configuration
        .connection_graph
        .connections
        .remove(connection_id);
    configuration
        .connection_graph
        .add_connection(&providers, candidate)
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
fn rollback_replaced_secret(
    secret_store: &OsAiCredentialSecretStore,
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
fn prepare_management_secret_mutation(
    secret_store: &OsAiCredentialSecretStore,
    current: &CredentialConnection,
    management_key: Option<&str>,
    remove_management_key: bool,
) -> Result<ManagementSecretMutation, String> {
    if remove_management_key {
        let Some(reference) = current.management_secret_reference.as_deref() else {
            return Ok(ManagementSecretMutation::Unchanged);
        };
        let previous = CredentialSecretResolver::read_secret(secret_store, reference)?;
        CredentialSecretStore::delete_secret(secret_store, reference)?;
        return Ok(ManagementSecretMutation::Removed {
            reference: reference.to_string(),
            previous: Box::new(previous),
        });
    }
    let Some(value) = management_key.filter(|value| !value.trim().is_empty()) else {
        return Ok(ManagementSecretMutation::Unchanged);
    };
    if let Some(reference) = current.management_secret_reference.as_deref() {
        let previous = CredentialSecretResolver::read_secret(secret_store, reference)?;
        let replacement = previous.replace_api_key(value.to_string())?;
        CredentialSecretStore::replace_secret(
            secret_store,
            reference,
            previous.revision,
            &replacement,
        )?;
        return Ok(ManagementSecretMutation::Replaced {
            reference: reference.to_string(),
            previous: Box::new(previous),
            replacement: Box::new(replacement),
        });
    }
    let reference = format!("ai_management_secret_{}", uuid::Uuid::new_v4().simple());
    let secret = CredentialSecretEnvelope::new_api_key(value.to_string())?;
    CredentialSecretStore::create_secret(secret_store, reference.as_str(), &secret)?;
    Ok(ManagementSecretMutation::Created { reference })
}

/// Reverts one provider-management secret mutation.
/// 回滚一次供应商管理秘密变更。
fn rollback_management_secret_mutation(
    secret_store: &OsAiCredentialSecretStore,
    mutation: &ManagementSecretMutation,
) -> Result<(), String> {
    match mutation {
        ManagementSecretMutation::Unchanged => Ok(()),
        ManagementSecretMutation::Created { reference } => {
            CredentialSecretStore::delete_secret(secret_store, reference)
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
        }
        ManagementSecretMutation::Removed {
            reference,
            previous,
        } => secret_store.restore_deleted_secret(reference, previous.as_ref()),
    }
}

/// Deletes the primary and optional management secrets as one cleanup operation.
/// 将主秘密和可选管理秘密作为一次清理操作删除。
fn delete_connection_secrets(
    secret_store: &OsAiCredentialSecretStore,
    primary_reference: &str,
    management_reference: Option<&str>,
) -> Result<(), String> {
    CredentialSecretStore::delete_secret(secret_store, primary_reference)?;
    if let Some(reference) = management_reference
        && let Err(error) = CredentialSecretStore::delete_secret(secret_store, reference)
    {
        return Err(format!(
            "Primary secret was deleted but management secret cleanup failed: {error}"
        ));
    }
    Ok(())
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

/// Saves one explicit provider default connection.
/// 保存一个显式供应商默认连接。
pub(super) async fn service_save_public_ai_default_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiDefaultConnectionSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    let configuration = mutate_public_ai_configuration(
        &state,
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
    )
    .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let removed = current_configuration
        .connection_graph
        .connections
        .get(connection_id.as_str())
        .cloned()
        .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
    let secret_store = OsAiCredentialSecretStore;
    let removed_secret =
        CredentialSecretResolver::read_secret(&secret_store, removed.secret_reference.as_str())?;
    let removed_management_secret = removed
        .management_secret_reference
        .as_deref()
        .map(|reference| CredentialSecretResolver::read_secret(&secret_store, reference))
        .transpose()?;
    CredentialSecretStore::delete_secret(&secret_store, removed.secret_reference.as_str())?;
    if let Some(reference) = removed.management_secret_reference.as_deref()
        && let Err(error) = CredentialSecretStore::delete_secret(&secret_store, reference)
    {
        let restore =
            secret_store.restore_deleted_secret(removed.secret_reference.as_str(), &removed_secret);
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
            )
        },
    );
    let configuration = match remove_result {
        Ok(configuration) => configuration,
        Err(error) => {
            let _ = secret_store
                .restore_deleted_secret(removed.secret_reference.as_str(), &removed_secret);
            if let (Some(reference), Some(secret)) = (
                removed.management_secret_reference.as_deref(),
                removed_management_secret.as_ref(),
            ) {
                let _ = secret_store.restore_deleted_secret(reference, secret);
            }
            return Err(error);
        }
    };
    drop(transaction);
    runtime.remove_connection_catalog(connection_id.as_str())?;
    runtime.remove_connection_usage(connection_id.as_str())?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    drop(transaction);
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    refresh_result?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    let configuration = mutate_public_ai_configuration(
        &state,
        "保存账号用量刷新间隔失败",
        |configuration| {
            configuration.usage_refresh_interval_seconds = request.interval_seconds;
            Ok(())
        },
    )
    .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    if let Some(session_id) = conversation_id {
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
    }
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
    if !current_configuration
        .model_is_enabled(request.provider_id.as_str(), request.model_id.as_str())
    {
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
    let configuration =
        mutate_public_ai_configuration(&state, "选择公共 AI 模型失败", |configuration| {
            configuration.connection_graph.remember_model_connection(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                request.connection_id.as_str(),
            )?;
            if conversation_id.is_none() {
                configuration.active = Some(ActiveAiModelSelection {
                    provider_id: request.provider_id.clone(),
                    model_id: request.model_id.clone(),
                    connection_id: request.connection_id.clone(),
                });
                runtime.resolve_active_selection(configuration)?;
            } else {
                runtime.resolve_connection_model_selection(
                    configuration,
                    request.connection_id.as_str(),
                    request.provider_id.as_str(),
                    request.model_id.as_str(),
                )?;
            }
            Ok(())
        })
        .await?;
    // persistence_context captures the exact runtime-controls addon after updating a persisted session.
    // persistence_context 在更新持久化会话后捕获精确的运行时控制附加状态。
    let persistence_context = if let Some(session_id) = conversation_id {
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("Workspace is not initialized."))?;
        let runtime_state = workspace
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        runtime_state.active_provider_id = Some(request.provider_id.clone());
        runtime_state.active_connection_id = Some(request.connection_id.clone());
        runtime_state.active_model_id = Some(request.model_id.clone());
        runtime_state.updated_at = crate::current_unix_millis_text();
        let controller_runtime = guard.controller_runtime.clone();
        let workspace_snapshot = guard.workspace_snapshot.clone();
        Some((controller_runtime, session_id, workspace_snapshot))
    } else {
        None
    };
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    if let Some((Some(controller_runtime), session_id, Some(workspace_snapshot))) =
        persistence_context
        && let Some(addon_state) = crate::session_addon_state_from_workspace_snapshot(
            &workspace_snapshot,
            session_id,
            &[crate::SessionAddonNamespace::RuntimeControls],
        )
    {
        crate::spawn_workspace_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            workspace_snapshot,
            session_id,
            addon_state,
            vec![crate::SessionAddonNamespace::RuntimeControls],
            Some(String::from("model_selection")),
        );
    }
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
}

/// Saves one provider-model-protocol override set after validating every matching account route.
/// 在校验全部匹配账号路由后保存一组供应商、模型与协议覆盖。
pub(super) async fn service_save_public_ai_model_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    let catalogs = runtime.catalog_snapshot()?;
    let key = PersistedAiConfiguration::provider_model_protocol_key(
        request.provider_id.as_str(),
        request.model_id.as_str(),
        request.protocol_id.as_str(),
    );
    let configuration = mutate_public_ai_configuration(
        &state,
        "保存公共 AI 模型覆盖失败",
        |configuration| {
            let matching_connection_ids = configuration
                .connection_graph
                .connections
                .values()
                .filter(|connection| connection.provider_id == request.provider_id)
                .filter(|connection| connection.selected_protocol == request.protocol_id)
                .filter(|connection| {
                    catalogs.get(connection.id.as_str()).is_some_and(|catalog| {
                        catalog
                            .models
                            .iter()
                            .any(|model| model.id == request.model_id)
                    })
                })
                .map(|connection| connection.id.clone())
                .collect::<Vec<_>>();
            if matching_connection_ids.is_empty() {
                return Err(format!(
                    "Provider `{}` model `{}` has no synchronized account using protocol `{}`.",
                    request.provider_id, request.model_id, request.protocol_id
                ));
            }
            let previous = configuration
                .model_overrides
                .insert(key.clone(), request.overrides);
            for connection_id in matching_connection_ids {
                let mut validation_configuration = configuration.clone();
                validation_configuration.active = Some(ActiveAiModelSelection {
                    provider_id: request.provider_id.clone(),
                    model_id: request.model_id.clone(),
                    connection_id,
                });
                if let Err(error) = runtime.resolve_active_selection(&validation_configuration) {
                    match previous {
                        Some(value) => {
                            configuration.model_overrides.insert(key.clone(), value);
                        }
                        None => {
                            configuration.model_overrides.remove(key.as_str());
                        }
                    }
                    return Err(error);
                }
            }
            Ok(())
        },
    )
    .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
}

/// Saves one provider-model access choice and reconciles invalid active selections.
/// 保存一个供应商模型访问选择，并协调失效的当前选择。
pub(super) async fn service_save_public_ai_model_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let catalogs = runtime.catalog_snapshot()?;
    let model_exists = current_configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == request.provider_id)
        .any(|connection| {
            catalogs.get(connection.id.as_str()).is_some_and(|catalog| {
                catalog
                    .models
                    .iter()
                    .any(|model| model.id == request.model_id)
            })
        });
    if !model_exists {
        return Err(format!(
            "Provider `{}` does not expose synchronized model `{}`.",
            request.provider_id, request.model_id
        ));
    }
    let configuration = mutate_public_ai_configuration(
        &state,
        "保存公共 AI 模型启用状态失败",
        |configuration| {
            configuration.set_model_enabled(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                request.enabled,
            );
            if !request.enabled
                && configuration.active.as_ref().is_some_and(|selection| {
                    selection.provider_id == request.provider_id
                        && selection.model_id == request.model_id
                })
            {
                configuration.active = first_enabled_model_selection(configuration, &catalogs);
            }
            Ok(())
        },
    )
    .await?;
    if !request.enabled {
        let fallback = first_enabled_model_selection(&configuration, &catalogs);
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        if let Some(workspace) = guard.workspace_snapshot.as_mut() {
            for runtime_state in workspace.session_runtime_state.values_mut() {
                if runtime_state.active_provider_id.as_deref() == Some(request.provider_id.as_str())
                    && runtime_state.active_model_id.as_deref() == Some(request.model_id.as_str())
                {
                    runtime_state.active_provider_id = fallback
                        .as_ref()
                        .map(|selection| selection.provider_id.clone());
                    runtime_state.active_connection_id = fallback
                        .as_ref()
                        .map(|selection| selection.connection_id.clone());
                    runtime_state.active_model_id = fallback
                        .as_ref()
                        .map(|selection| selection.model_id.clone());
                    runtime_state.updated_at = crate::current_unix_millis_text();
                }
            }
        }
    }
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
}

/// Saves one provider-wide model access state and reconciles every affected active selection.
/// 保存供应商范围的模型访问状态，并协调全部受影响的当前选择。
///
/// # Parameters
/// 参数
/// `app` publishes the refreshed workbench, `state` owns the persisted graph, and `request` identifies one provider and state.
/// `app` 发布刷新的工作台，`state` 持有持久化图，`request` 标识一个供应商及状态。
///
/// # Returns
/// 返回值
/// The complete refreshed settings snapshot after one atomic configuration mutation.
/// 返回一次原子配置变更后的完整刷新设置快照。
pub(super) async fn service_save_public_ai_provider_models_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderModelsEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let catalogs = runtime.catalog_snapshot()?;
    let model_ids = current_configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == request.provider_id)
        .filter_map(|connection| catalogs.get(connection.id.as_str()))
        .flat_map(|catalog| catalog.models.iter().map(|model| model.id.clone()))
        .collect::<BTreeSet<_>>();
    if model_ids.is_empty() {
        return Err(format!(
            "Provider `{}` does not expose synchronized models.",
            request.provider_id
        ));
    }
    let configuration = mutate_public_ai_configuration(
        &state,
        "批量保存公共 AI 模型启用状态失败",
        |configuration| {
            for model_id in &model_ids {
                configuration.set_model_enabled(
                    request.provider_id.as_str(),
                    model_id.as_str(),
                    request.enabled,
                );
            }
            if !request.enabled
                && configuration
                    .active
                    .as_ref()
                    .is_some_and(|selection| selection.provider_id == request.provider_id)
            {
                configuration.active = first_enabled_model_selection(configuration, &catalogs);
            }
            Ok(())
        },
    )
    .await?;
    if !request.enabled {
        let fallback = first_enabled_model_selection(&configuration, &catalogs);
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        if let Some(workspace) = guard.workspace_snapshot.as_mut() {
            for runtime_state in workspace.session_runtime_state.values_mut() {
                if runtime_state.active_provider_id.as_deref() == Some(request.provider_id.as_str())
                {
                    runtime_state.active_provider_id = fallback
                        .as_ref()
                        .map(|selection| selection.provider_id.clone());
                    runtime_state.active_connection_id = fallback
                        .as_ref()
                        .map(|selection| selection.connection_id.clone());
                    runtime_state.active_model_id = fallback
                        .as_ref()
                        .map(|selection| selection.model_id.clone());
                    runtime_state.updated_at = crate::current_unix_millis_text();
                }
            }
        }
    }
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
}

/// Saves one internal model operation switch without changing conversation availability.
/// 保存一项内部模型操作开关且不改变会话可用性。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `request` identify the settings transaction and exact operation.
/// `app`、`state` 与 `request` 标识设置事务及精确操作。
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
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let providers = runtime.effective_providers(&current_configuration)?;
    let catalogs = runtime.catalog_snapshot()?;
    let exists =
        build_internal_inference_candidates(&current_configuration, &providers, &catalogs)?
            .iter()
            .any(|candidate| {
                candidate.route.provider_id == request.provider_id
                    && candidate.route.model_id == request.model_id
                    && candidate.route.operation == request.operation
            });
    if !exists {
        return Err(format!(
            "Provider `{}` model `{}` does not expose registered internal operation `{}`.",
            request.provider_id,
            request.model_id,
            request.operation.as_str()
        ));
    }
    let configuration = mutate_public_ai_configuration(
        &state,
        "保存内部推理模型启用状态失败",
        |configuration| {
            configuration.set_internal_model_enabled(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                request.operation,
                request.enabled,
            );
            Ok(())
        },
    )
    .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let configuration = mutate_public_ai_configuration(
        &state,
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
    )
    .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if let Some(route) = request.route.as_ref() {
        if route.operation != request.operation {
            return Err(String::from(
                "Inference route operation differs from the requested profile operation.",
            ));
        }
        // Selecting a route is an explicit enable action for that exact internal operation.
        // 选择路由即显式启用该精确内部操作。
        let mut validation_configuration = current_configuration.clone();
        enable_internal_inference_route(&mut validation_configuration, route);
        let selection =
            runtime.resolve_internal_inference_selection(&validation_configuration, route)?;
        validate_vmm_llm_reasoning_policy(
            request.consumer_profile_id.as_str(),
            request.operation,
            &selection.model.capabilities,
        )?;
    }
    let configuration =
        mutate_public_ai_configuration(&state, "保存内部推理路由失败", |configuration| {
            if let Some(route) = request.route.as_ref() {
                // Persist capability enablement and route binding in one configuration mutation.
                // 在一次配置变更中同时持久化能力启用与路由绑定。
                enable_internal_inference_route(configuration, route);
            }
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
        })
        .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
}

/// Enables the exact internal operation owned by one selected inference route.
/// 启用一条已选推理路由所拥有的精确内部操作。
///
/// # Parameters
/// 参数
/// `configuration` receives the explicit capability state and `route` identifies its owner.
/// `configuration` 接收显式能力状态，`route` 标识其归属。
fn enable_internal_inference_route(
    configuration: &mut PersistedAiConfiguration,
    route: &InferenceRouteConfiguration,
) {
    configuration.set_internal_model_enabled(
        route.provider_id.as_str(),
        route.model_id.as_str(),
        route.operation,
        true,
    );
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
    let configuration = mutate_public_ai_configuration(
        &state,
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
    )
    .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let available = runtime.available_provider_tool_targets(&current_configuration)?;
    if request.enabled
        && !available
            .iter()
            .any(|candidate| candidate.target.contract.capability == request.capability)
    {
        return Err(format!(
            "Provider tool capability `{:?}` has no available model and account.",
            request.capability
        ));
    }
    let configuration = mutate_public_ai_configuration(
        &state,
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
    )
    .await?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
}

/// Returns the effective single-select provider tool configuration for one session.
/// 返回单个会话生效的供应商工具单选配置。
pub(super) fn service_public_ai_session_provider_tool_selections(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<PublicAiSessionProviderToolSettingsSnapshot, String> {
    let session_id = conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| format!("Invalid conversation id `{conversation_id}`: {error}"))?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let (enabled_capabilities, selections, persistence_context) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("Workspace is not initialized."))?;
        let runtime_state = workspace
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        let initialized_now = if runtime_state.provider_tool_settings_initialized {
            false
        } else {
            if runtime_state.provider_tool_selections_overridden {
                runtime_state.provider_tool_enabled_capabilities = runtime_state
                    .provider_tool_selections
                    .keys()
                    .cloned()
                    .collect();
            } else {
                runtime_state.provider_tool_selections = configuration
                    .provider_tool_selections
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
                runtime_state.provider_tool_enabled_capabilities = configuration
                    .provider_tool_enabled_capabilities
                    .iter()
                    .map(|capability| provider_tool_capability_key(*capability))
                    .collect();
            }
            runtime_state.provider_tool_settings_initialized = true;
            runtime_state.provider_tool_selections_overridden = false;
            runtime_state.updated_at = crate::current_unix_millis_text();
            true
        };
        let enabled_capabilities =
            session_provider_tool_enabled_capabilities_from_runtime(runtime_state)?;
        let selections = session_provider_tool_selections_from_runtime(runtime_state)?;
        let persistence_context = initialized_now.then(|| {
            (
                guard.controller_runtime.clone(),
                guard.workspace_snapshot.clone(),
            )
        });
        (enabled_capabilities, selections, persistence_context)
    };
    if let Some((Some(controller_runtime), Some(workspace_snapshot))) = persistence_context
        && let Some(addon_state) = crate::session_addon_state_from_workspace_snapshot(
            &workspace_snapshot,
            session_id,
            &[crate::SessionAddonNamespace::RuntimeControls],
        )
    {
        crate::spawn_workspace_session_addon_state_persist_task(
            app,
            state,
            controller_runtime,
            workspace_snapshot,
            session_id,
            addon_state,
            vec![crate::SessionAddonNamespace::RuntimeControls],
            Some(String::from("provider_tool_settings_initialize")),
        );
    }
    let conflicts = provider_tool_session_conflicts(
        runtime.as_ref(),
        &configuration,
        &enabled_capabilities,
        &selections,
    )?;
    Ok(PublicAiSessionProviderToolSettingsSnapshot {
        enabled_capabilities,
        selections,
        conflicts,
    })
}

/// Saves the complete single-select provider tool configuration for one session.
/// 保存单个会话完整的供应商工具单选配置。
pub(super) fn service_save_public_ai_session_provider_tool_selections(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiSessionProviderToolSelectionsSaveRequest,
) -> Result<PublicAiSessionProviderToolSettingsSnapshot, String> {
    let session_id = request
        .conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| {
            format!(
                "Invalid conversation id `{}`: {error}",
                request.conversation_id
            )
        })?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_provider_tool_session_settings(
        runtime.as_ref(),
        &configuration,
        &request.enabled_capabilities,
        &request.selections,
    )?;
    let persistence_context = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("Workspace is not initialized."))?;
        let runtime_state = workspace
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
        runtime_state.provider_tool_selections = request
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
        runtime_state.provider_tool_enabled_capabilities = request
            .enabled_capabilities
            .iter()
            .map(|capability| provider_tool_capability_key(*capability))
            .collect();
        runtime_state.provider_tool_settings_initialized = true;
        runtime_state.provider_tool_selections_overridden = false;
        runtime_state.updated_at = crate::current_unix_millis_text();
        (
            guard.controller_runtime.clone(),
            guard.workspace_snapshot.clone(),
        )
    };
    if let (Some(controller_runtime), Some(workspace_snapshot)) = persistence_context
        && let Some(addon_state) = crate::session_addon_state_from_workspace_snapshot(
            &workspace_snapshot,
            session_id,
            &[crate::SessionAddonNamespace::RuntimeControls],
        )
    {
        crate::spawn_workspace_session_addon_state_persist_task(
            app,
            state,
            controller_runtime,
            workspace_snapshot,
            session_id,
            addon_state,
            vec![crate::SessionAddonNamespace::RuntimeControls],
            Some(String::from("provider_tool_settings")),
        );
    }
    Ok(PublicAiSessionProviderToolSettingsSnapshot {
        enabled_capabilities: request.enabled_capabilities,
        selections: request.selections,
        conflicts: Vec::new(),
    })
}

/// Validates one complete session provider tool snapshot before persistence or dispatch.
/// 在持久化或分派前校验一份完整会话供应商工具快照。
pub(crate) fn validate_provider_tool_session_settings(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    enabled_capabilities: &BTreeSet<ProviderToolCapabilityKind>,
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
) -> Result<(), String> {
    let conflicts =
        provider_tool_session_conflicts(runtime, configuration, enabled_capabilities, selections)?;
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
) -> Result<Vec<PublicAiSessionProviderToolConflict>, String> {
    let available = runtime.available_provider_tool_targets(configuration)?;
    let capabilities = enabled_capabilities.clone();
    let mut conflicts = Vec::new();
    for capability in capabilities {
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
    Ok(conflicts)
}

/// Filters historical selections to targets that still exist and remain enabled.
/// 将历史选择过滤为仍存在且保持启用的当前目标。
fn filter_current_provider_tool_selections(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
) -> Result<BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>, String> {
    let available = runtime.available_provider_tool_targets(configuration)?;
    Ok(selections
        .iter()
        .filter(|(capability, selection)| {
            available.iter().any(|candidate| {
                candidate.connection_id == selection.connection_id
                    && candidate.target.target_id == selection.target_id
                    && candidate.target.contract.capability == **capability
            })
        })
        .map(|(capability, selection)| (*capability, selection.clone()))
        .collect())
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
/// The lexicographically first enabled tuple, or `None` when no model remains.
/// 返回按字典序排列的首个启用组合；没有剩余模型时返回 `None`。
fn first_enabled_model_selection(
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Option<ActiveAiModelSelection> {
    let mut candidates = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| {
            !configuration.connection_is_pending_validation(connection.id.as_str())
        })
        .flat_map(|connection| {
            catalogs
                .get(connection.id.as_str())
                .into_iter()
                .flat_map(|catalog| {
                    catalog
                        .models
                        .iter()
                        .filter(|model| {
                            configuration.model_is_enabled(
                                connection.provider_id.as_str(),
                                model.id.as_str(),
                            ) && vulcan_provider_adapters::model_supports_text_conversation(
                                &model.capabilities,
                            )
                        })
                        .map(|model| ActiveAiModelSelection {
                            provider_id: connection.provider_id.clone(),
                            model_id: model.id.clone(),
                            connection_id: connection.id.clone(),
                        })
                })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.provider_id
            .cmp(&right.provider_id)
            .then_with(|| left.model_id.cmp(&right.model_id))
            .then_with(|| left.connection_id.cmp(&right.connection_id))
    });
    candidates.into_iter().next()
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
/// The first same-model account, otherwise the stable global fallback.
/// 返回首个同模型账号，否则返回稳定的全局降级项。
fn preferred_replacement_model_selection(
    configuration: &PersistedAiConfiguration,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
    provider_id: &str,
    model_id: &str,
) -> Option<ActiveAiModelSelection> {
    let mut same_model_candidates = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .filter(|connection| {
            !configuration.connection_is_pending_validation(connection.id.as_str())
        })
        .filter_map(|connection| {
            catalogs
                .get(connection.id.as_str())
                .filter(|catalog| {
                    catalog.models.iter().any(|model| {
                        model.id == model_id
                            && configuration.model_is_enabled(provider_id, model_id)
                            && vulcan_provider_adapters::model_supports_text_conversation(
                                &model.capabilities,
                            )
                    })
                })
                .map(|_| ActiveAiModelSelection {
                    provider_id: provider_id.to_string(),
                    model_id: model_id.to_string(),
                    connection_id: connection.id.clone(),
                })
        })
        .collect::<Vec<_>>();
    same_model_candidates.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));
    same_model_candidates
        .into_iter()
        .next()
        .or_else(|| first_enabled_model_selection(configuration, catalogs))
}

/// Reconciles loaded session selections after one credential model becomes unavailable.
/// 在一个凭据模型变为不可用后协调已加载会话选择。
///
/// # Parameters
/// 参数
/// `state` owns loaded session runtime state; `connection_id` selects the affected account;
/// `model_id` narrows one model when present; `fallback` is the resolved replacement.
/// `state` 持有已加载会话运行状态；`connection_id` 选择受影响账号；存在 `model_id` 时收窄到一个模型；
/// `fallback` 是已解析替代项。
fn reconcile_disabled_connection_sessions(
    state: &SharedServiceBridgeState,
    connection_id: &str,
    model_id: Option<&str>,
    fallback: Option<&ActiveAiModelSelection>,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    let Some(workspace) = guard.workspace_snapshot.as_mut() else {
        return Ok(());
    };
    for runtime_state in workspace.session_runtime_state.values_mut() {
        let exact_connection = runtime_state.active_connection_id.as_deref() == Some(connection_id);
        let exact_model = model_id
            .is_none_or(|model_id| runtime_state.active_model_id.as_deref() == Some(model_id));
        if !exact_connection || !exact_model {
            continue;
        }
        runtime_state.active_provider_id = fallback.map(|selection| selection.provider_id.clone());
        runtime_state.active_connection_id =
            fallback.map(|selection| selection.connection_id.clone());
        runtime_state.active_model_id = fallback.map(|selection| selection.model_id.clone());
        runtime_state.updated_at = crate::current_unix_millis_text();
    }
    Ok(())
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

/// Resolves one account's closed model-directory kind from its exact registry route.
/// 从账号的精确注册路由解析封闭模型目录类型。
///
/// # Parameters
/// 参数
/// `providers` is the effective registry and `connection` is the exact persisted account.
/// `providers` 是有效注册表，`connection` 是精确持久化账号。
///
/// # Returns
/// 返回值
/// A browser-safe directory kind. Incomplete legacy routes are isolated as unavailable.
/// 返回浏览器安全的目录类型；不完整的历史路由会被隔离为不可用。
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
    let Some(route) = entry
        .protocol_routes
        .iter()
        .find(|route| route.protocol_profile_id == connection.selected_protocol)
    else {
        return PublicAiModelDirectoryKind::Unavailable;
    };
    match route.resolved_model_discovery(entry.model_discovery) {
        ModelDiscoveryKind::Manual => PublicAiModelDirectoryKind::Manual,
        ModelDiscoveryKind::ProviderToolsOnly => PublicAiModelDirectoryKind::Unavailable,
        ModelDiscoveryKind::AlibabaCodingPlanStatic
        | ModelDiscoveryKind::AlibabaTokenPlanStatic
        | ModelDiscoveryKind::AlibabaModelStudioStatic => PublicAiModelDirectoryKind::Fixed,
        ModelDiscoveryKind::MiniMaxAnthropic
        | ModelDiscoveryKind::MiniMaxOpenAi
        | ModelDiscoveryKind::OpenRouter
        | ModelDiscoveryKind::OpenAi
        | ModelDiscoveryKind::OpenAiCodex
        | ModelDiscoveryKind::Anthropic
        | ModelDiscoveryKind::AnthropicCompatible
        | ModelDiscoveryKind::Gemini
        | ModelDiscoveryKind::GoogleCodeAssist => PublicAiModelDirectoryKind::Dynamic,
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

/// Builds a redacted browser snapshot from persisted and process-local state.
/// 从持久化状态与进程内状态构建脱敏浏览器快照。
fn build_public_ai_settings_snapshot(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Result<PublicAiSettingsSnapshot, String> {
    let catalogs = runtime.catalog_snapshot()?;
    let providers = runtime.effective_providers(configuration)?;
    let mut model_override_records_by_key = BTreeMap::new();
    let mut model_access_records_by_key = BTreeMap::new();
    let mut model_capability_override_records_by_key = BTreeMap::new();
    let mut custom_model_template_decision_records_by_key = BTreeMap::new();
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
            model_access_records_by_key
                .entry(access_key.clone())
                .or_insert_with(|| PublicAiModelAccessRecord {
                    provider_id: connection.provider_id.clone(),
                    model_id: model.id.clone(),
                    enabled: configuration
                        .model_is_enabled(connection.provider_id.as_str(), model.id.as_str()),
                    default_enabled: configuration
                        .custom_model(connection.provider_id.as_str(), model.id.as_str())
                        .map_or_else(
                            || {
                                vulcan_provider_adapters::default_model_enabled(
                                    connection.provider_id.as_str(),
                                    model.id.as_str(),
                                )
                            },
                            |definition| definition.enabled,
                        ),
                });
            if let Some(overrides) = configuration
                .model_capability_overrides
                .get(access_key.as_str())
                .cloned()
            {
                model_capability_override_records_by_key
                    .entry(access_key.clone())
                    .or_insert_with(|| PublicAiModelCapabilityOverrideRecord {
                        provider_id: connection.provider_id.clone(),
                        model_id: model.id.clone(),
                        overrides,
                    });
            }
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
            let key = PersistedAiConfiguration::provider_model_protocol_key(
                connection.provider_id.as_str(),
                model.id.as_str(),
                connection.selected_protocol.as_str(),
            );
            if let Some(overrides) = configuration.model_overrides.get(key.as_str()).cloned() {
                model_override_records_by_key.entry(key).or_insert_with(|| {
                    PublicAiModelOverrideRecord {
                        provider_id: connection.provider_id.clone(),
                        model_id: model.id.clone(),
                        protocol_id: connection.selected_protocol.clone(),
                        overrides,
                    }
                });
            }
        }
    }
    let model_override_records = model_override_records_by_key.into_values().collect();
    let model_access_records = model_access_records_by_key.into_values().collect();
    let model_capability_override_records = model_capability_override_records_by_key
        .into_values()
        .collect();
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
    let available_provider_tool_targets = runtime.available_provider_tool_targets(configuration)?;
    let provider_tool_selections = filter_current_provider_tool_selections(
        runtime,
        configuration,
        &configuration.provider_tool_selections,
    )?;
    let internal_inference_candidates =
        build_internal_inference_candidates(configuration, &providers, &catalogs)?;
    let inference_route_diagnostics =
        build_internal_inference_route_diagnostics(runtime, configuration);
    let connection_model_access_policy_records =
        build_connection_model_access_policy_records(&providers, configuration);
    let mut support_configuration = configuration.clone();
    support_configuration
        .connection_model_access_policies
        .clear();
    let model_support_catalogs = runtime.resolved_model_catalog_snapshot(&support_configuration)?;
    Ok(PublicAiSettingsSnapshot {
        usage_refresh_interval_seconds: configuration.usage_refresh_interval_seconds,
        usage_cache: runtime.usage_cache_snapshot()?,
        providers: providers.providers().into_iter().cloned().collect(),
        connections: configuration
            .connection_graph
            .connections
            .values()
            .map(|connection| PublicAiConnectionSnapshot::new(runtime, configuration, connection))
            .collect::<Result<Vec<_>, _>>()?,
        provider_defaults: configuration
            .connection_graph
            .account_preferences
            .provider_defaults
            .clone(),
        active: configuration.active.clone(),
        catalogs,
        source_catalogs: runtime.source_catalog_snapshot()?,
        model_support_catalogs,
        catalog_statuses: runtime.catalog_status_snapshot()?,
        catalog_errors: runtime.catalog_error_snapshot()?,
        model_overrides: configuration.model_overrides.clone(),
        model_override_records,
        model_access_records,
        custom_models: configuration.custom_models.clone(),
        custom_provider_model_defaults: configuration.custom_provider_model_defaults.clone(),
        custom_model_template_decision_records,
        custom_model_connection_availability_records,
        connection_model_access_policy_records,
        model_capability_overrides: configuration.model_capability_overrides.clone(),
        model_capability_override_records,
        provider_tool_targets: available_provider_tool_targets
            .into_iter()
            .map(|available| PublicAiProviderToolTargetSnapshot {
                connection_id: available.connection_id,
                connection_display_name: available.connection_display_name,
                target: available.target,
            })
            .collect(),
        provider_tool_selections,
        provider_tool_enabled_capabilities: configuration
            .provider_tool_enabled_capabilities
            .clone(),
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
fn build_internal_inference_candidates(
    configuration: &PersistedAiConfiguration,
    providers: &vulcan_provider_adapters::ProviderRegistry,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Vec<PublicAiInternalInferenceCandidate>, String> {
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
        let usage_scopes = entry.effective_usage_scopes(connection)?;
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
                    AiOperation::ConversationRespond => InferenceRouteBinding::Protocol {
                        protocol_profile_id: connection.selected_protocol.clone(),
                    },
                    AiOperation::EmbeddingCreate | AiOperation::RerankDocuments => {
                        let Some(action_binding_id) = registered_native_action_binding_id(
                            connection.provider_id.as_str(),
                            connection.service_entry_id.as_str(),
                            operation,
                        ) else {
                            continue;
                        };
                        InferenceRouteBinding::Action {
                            action_binding_id: action_binding_id.to_string(),
                        }
                    }
                    _ => continue,
                };
                for usage_scope in usage_scopes.iter().copied() {
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
                            operation,
                        ),
                        usage_scopes: usage_scopes.clone(),
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
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        // effective_selection prefers the selected conversation model, then degrades to the global default.
        // effective_selection 优先使用当前会话模型，失效时再降级到全局默认。
        let effective_selection = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| {
                workspace
                    .session_runtime_state
                    .get(&workspace.selected_session)
            })
            .and_then(|runtime_state| {
                runtime_state
                    .active_provider_id
                    .as_deref()
                    .zip(runtime_state.active_connection_id.as_deref())
                    .zip(runtime_state.active_model_id.as_deref())
            })
            .and_then(|((provider_id, connection_id), model_id)| {
                runtime
                    .resolve_connection_model_selection(
                        configuration,
                        connection_id,
                        provider_id,
                        model_id,
                    )
                    .ok()
            })
            .or_else(|| {
                guard
                    .workspace_snapshot
                    .as_ref()
                    .and_then(|workspace| {
                        workspace
                            .session_runtime_state
                            .get(&workspace.selected_session)
                    })
                    .and_then(|runtime_state| {
                        runtime_state
                            .active_provider_id
                            .as_deref()
                            .zip(runtime_state.active_model_id.as_deref())
                    })
                    .and_then(|(provider_id, model_id)| {
                        runtime
                            .resolve_model_selection(configuration, provider_id, model_id)
                            .ok()
                    })
            })
            .or_else(|| runtime.resolve_active_selection(configuration).ok());
        let rows = public_ai_provider_rows_for_selection(
            configuration,
            runtime,
            effective_selection.as_ref(),
        )?;
        guard.workbench.providers = rows;
        if let Some(provider_configuration) = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| agent_runtime.provider_configuration.clone())
        {
            guard.workbench.reasoning_selection = crate::reasoning_selection(
                &provider_configuration,
                effective_selection
                    .as_ref()
                    .map(|selection| &selection.model.capabilities),
            );
            guard.workbench.composer_attachment_capabilities =
                crate::composer_attachment_capabilities(
                    effective_selection
                        .as_ref()
                        .map(|selection| &selection.model.capabilities),
                );
        }
        // A published provider projection is a new authoritative snapshot and must outrank the previous WebUI revision.
        // 已发布的供应商投影是新的权威快照，必须以更高版本覆盖 WebUI 中的旧快照。
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        guard.workbench.clone()
    };
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

/// Validates that one custom-provider model exists in a current catalog or manual declaration.
/// 校验一个自定义供应商模型存在于当前目录或手工声明中。
fn validate_custom_provider_model_exists(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
) -> Result<(), String> {
    if configuration.custom_model(provider_id, model_id).is_some() {
        return Ok(());
    }
    let catalogs = runtime.catalog_snapshot()?;
    let exists = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .any(|connection| {
            catalogs
                .get(connection.id.as_str())
                .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id))
        });
    if exists {
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

#[cfg(test)]
mod inference_settings_route_tests {
    use super::*;

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

    /// Verifies choosing one route enables only its exact model operation.
    /// 验证选择一条路由只会启用其精确模型操作。
    #[test]
    fn selected_route_enables_exact_internal_operation() {
        let route = route_fixture(
            AiOperation::ConversationRespond,
            InferenceUsageScope::ExternalService,
        );
        let mut configuration = PersistedAiConfiguration::default();

        enable_internal_inference_route(&mut configuration, &route);

        assert!(configuration.internal_model_is_enabled(
            route.provider_id.as_str(),
            route.model_id.as_str(),
            AiOperation::ConversationRespond,
        ));
        assert!(!configuration.internal_model_is_enabled(
            route.provider_id.as_str(),
            route.model_id.as_str(),
            AiOperation::EmbeddingCreate,
        ));
        assert!(!configuration.internal_model_is_enabled(
            route.provider_id.as_str(),
            "other-model",
            AiOperation::ConversationRespond,
        ));
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
        )
        .expect("conflict analysis");

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, "missing_selection");
        assert_eq!(
            conflicts[0].capability,
            ProviderToolCapabilityKind::WebSearch
        );
    }

    /// Verifies editing a custom provider reuses the selected credential secret without accepting a replacement key.
    /// 验证编辑自定义供应商会复用所选凭据秘密，并且不接受替换 Key。
    #[test]
    fn custom_provider_edit_reuses_selected_existing_credential() {
        let provider_id = String::from("custom:test-provider");
        let descriptor = CustomProviderDefinition {
            id: provider_id.clone(),
            display_name: String::from("Test Provider"),
            base_url: String::from("https://old.example.test/v1"),
            protocol: CustomProviderProtocol::OpenAiChat,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
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
            verified_protocols: BTreeSet::from([route.protocol_profile_id.clone()]),
            selected_protocol: route.protocol_profile_id.clone(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
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
            protocol: CustomProviderProtocol::OpenAiChat,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: None,
            secret: None,
            validation_connection_id: Some(existing.id.clone()),
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            allow_manual_fallback: false,
            allow_pending_validation: false,
        };

        let candidate = prepare_custom_provider_candidate(&request, &configuration)
            .expect("editing should reuse the selected account");
        assert!(candidate.updating_existing);
        assert!(candidate.temporary_secret.is_none());
        assert_ne!(candidate.connection.id, existing.id);
        assert_eq!(
            candidate.connection.secret_reference,
            existing.secret_reference
        );
        assert_eq!(candidate.connection.credential_id, existing.credential_id);
        assert_eq!(candidate.connection.display_name, existing.display_name);

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

    /// Verifies one incomplete legacy protocol route cannot block the complete settings snapshot.
    /// 验证一条不完整的历史协议路由不会阻塞完整设置快照。
    #[test]
    fn incomplete_connection_protocol_is_isolated_from_model_management() {
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
        // connection intentionally retains an empty protocol from an incomplete legacy record.
        // connection 刻意保留不完整历史记录中的空协议。
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
            verified_protocols: BTreeSet::new(),
            selected_protocol: String::new(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };

        assert_eq!(
            connection_model_directory_kind(&providers, &connection),
            PublicAiModelDirectoryKind::Unavailable
        );
    }
}
