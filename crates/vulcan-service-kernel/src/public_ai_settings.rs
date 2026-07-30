//! Public AI connection settings and runtime metadata services.
//! 公共 AI 连接设置与运行时元数据服务。

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use vulcan_ai_contract::{
    AccountUsageAvailability, AccountUsageSnapshot, AiOperation, EmbeddingEncoding, EmbeddingInput,
    EmbeddingInputContent, EmbeddingOutputKind, EmbeddingPayload, EmbeddingRequest, EmbeddingTask,
    INFERENCE_CONTRACT_VERSION, InferenceCallerBudget, InferenceCallerContext, InferencePermission,
    InferenceUsageScope, ModelCapabilities, PromptCachePolicy, ReasoningControlMode,
    ReasoningEffort, RerankRequest, RerankScoreSemantics, RerankText, RerankTruncation,
};
use vulcan_provider_adapters::{
    AccountUsageQueryKind, ActiveAiModelSelection, ConnectionModelCatalog, CredentialBillingKind,
    CredentialConnection, CredentialMode, CredentialSecretEnvelope, CredentialSecretResolver,
    CredentialSecretStore, CustomProviderAuthentication, CustomProviderDefinition,
    CustomProviderProtocol, InferenceProfile, InferenceRouteBinding, InferenceRouteConfiguration,
    InternalModelOverrides, ModelExecutionOverrides, OAuthAuthorizationChallenge,
    OAuthClientConfiguration, OAuthTransactionPhase, OAuthTransactionSnapshot,
    PersistedAiConfiguration, ProviderDescriptor, ProviderKind, ProviderToolCapabilityKind,
    ProviderToolExecutionResult, ProviderToolTarget, ProviderToolTargetSelection, PublicAiRuntime,
    ServiceEntryKind, registered_native_action_binding_id,
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
    /// Available exact provider tool targets through configured credentials.
    /// 通过已配置凭据可用的精确供应商工具目标。
    provider_tool_targets: Vec<PublicAiProviderToolTargetSnapshot>,
    /// One explicit selected target per semantic provider tool capability.
    /// 每项语义供应商工具能力唯一显式选择的目标。
    provider_tool_selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
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
    /// Whether this current catalog target is enabled for session selection.
    /// 当前目录目标是否已启用并可供会话选择。
    enabled: bool,
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
            credential_mode: connection.credential_mode,
            credential_id: connection.credential_id.clone(),
            display_name: connection.display_name.clone(),
            verified_protocols: connection.verified_protocols.clone(),
            selected_protocol: connection.selected_protocol.clone(),
            prompt_cache_by_protocol: connection.prompt_cache_by_protocol.clone(),
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

/// Changes whether one current provider tool target is available to sessions.
/// 修改一个当前供应商工具目标是否可供会话使用。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiProviderToolTargetEnabledSaveRequest {
    /// Exact semantic capability.
    /// 精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// Exact current target.
    /// 精确的当前目标。
    selection: ProviderToolTargetSelection,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
}

/// Replaces the complete single-select provider tool configuration for one session.
/// 替换单个会话完整的供应商工具单选配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicAiSessionProviderToolSelectionsSaveRequest {
    /// Conversation that owns the selection override.
    /// 拥有选择覆盖的会话。
    conversation_id: String,
    /// Exact target selected for each enabled semantic capability.
    /// 每项已启用语义能力所选择的精确目标。
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
    /// User-visible label for the first credential or validation identity.
    /// 首个凭据或验证身份的用户可见标签。
    credential_display_name: String,
    /// Secret used for authenticated model discovery.
    /// 用于鉴权模型发现的秘密。
    secret: String,
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

/// Creates or updates one custom provider without changing any credential.
/// 创建或更新一个自定义供应商且不改变任何凭据。
pub(super) async fn service_save_public_ai_custom_provider(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiCustomProviderSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    validate_non_empty("displayName", request.display_name.as_str())?;
    validate_non_empty(
        "credentialDisplayName",
        request.credential_display_name.as_str(),
    )?;
    validate_non_empty("secret", request.secret.as_str())?;
    let updating_existing = request.provider_id.is_some();
    let provider_id = request
        .provider_id
        .clone()
        .unwrap_or_else(|| format!("custom:{}", uuid::Uuid::new_v4().simple()));
    let descriptor = CustomProviderDefinition {
        id: provider_id.clone(),
        display_name: request.display_name,
        base_url: request.base_url,
        protocol: request.protocol,
        authentication: request.authentication,
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
    let connection_id = format!("connection_{}", uuid::Uuid::new_v4().simple());
    let secret_reference = format!("ai_secret_{}", uuid::Uuid::new_v4().simple());
    let connection = CredentialConnection {
        id: connection_id.clone(),
        provider_id: provider_id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        credential_mode: CredentialMode::Token,
        credential_id: format!("credential_{}", uuid::Uuid::new_v4().simple()),
        display_name: request.credential_display_name.trim().to_string(),
        secret_reference: secret_reference.clone(),
        management_secret_reference: None,
        verified_protocols: BTreeSet::from([route.protocol_profile_id.clone()]),
        selected_protocol: route.protocol_profile_id.clone(),
        prompt_cache_by_protocol: BTreeMap::new(),
        usage_scope_override: None,
    };
    let secret = CredentialSecretEnvelope::new_api_key(request.secret)?;
    let _transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    match current_configuration
        .custom_providers
        .get(provider_id.as_str())
    {
        Some(existing) if existing.kind != ProviderKind::Custom => {
            return Err(format!(
                "Provider `{provider_id}` is not a user-defined provider."
            ));
        }
        None if updating_existing => {
            return Err(format!(
                "Custom provider `{provider_id}` is not configured."
            ));
        }
        Some(_) | None => {}
    }
    for existing_connection in current_configuration
        .connection_graph
        .connections
        .values()
        .filter(|existing_connection| existing_connection.provider_id == provider_id)
    {
        if existing_connection.selected_protocol != route.protocol_profile_id {
            return Err(format!(
                "Custom provider `{provider_id}` cannot change protocol while credential `{}` uses `{}`.",
                existing_connection.display_name, existing_connection.selected_protocol
            ));
        }
    }
    let mut validation_configuration = current_configuration.clone();
    validation_configuration
        .custom_providers
        .insert(provider_id.clone(), descriptor.clone());
    let validation_providers = runtime.effective_providers(&validation_configuration)?;
    validation_configuration
        .connection_graph
        .add_connection(&validation_providers, connection.clone())?;
    let secret_store = OsAiCredentialSecretStore;
    CredentialSecretStore::create_secret(&secret_store, secret_reference.as_str(), &secret)?;
    if let Err(error) = runtime
        .refresh_connection_catalog(&validation_configuration, connection_id.as_str())
        .await
    {
        let catalog_cleanup = runtime.remove_connection_catalog(connection_id.as_str());
        let secret_cleanup =
            CredentialSecretStore::delete_secret(&secret_store, secret_reference.as_str());
        return Err(compensated_operation_error(
            format!("Custom provider model discovery failed before save: {error}"),
            secret_cleanup,
            catalog_cleanup,
        ));
    }
    let persist_result = mutate_public_ai_configuration_in_transaction(
        &state,
        "保存自定义供应商失败",
        |configuration| {
            configuration
                .custom_providers
                .insert(provider_id.clone(), descriptor.clone());
            if !updating_existing {
                let effective_providers = runtime.effective_providers(configuration)?;
                configuration
                    .connection_graph
                    .add_connection(&effective_providers, connection.clone())?;
            }
            Ok(())
        },
    );
    let configuration = match persist_result {
        Ok(configuration) => configuration,
        Err(error) => {
            let catalog_cleanup = runtime.remove_connection_catalog(connection_id.as_str());
            let secret_cleanup =
                CredentialSecretStore::delete_secret(&secret_store, secret_reference.as_str());
            return Err(compensated_operation_error(
                error,
                secret_cleanup,
                catalog_cleanup,
            ));
        }
    };
    if updating_existing {
        let catalog_cleanup = runtime.remove_connection_catalog(connection_id.as_str());
        let secret_cleanup =
            CredentialSecretStore::delete_secret(&secret_store, secret_reference.as_str());
        let cleanup_error =
            compensated_operation_error(String::new(), secret_cleanup, catalog_cleanup);
        if !cleanup_error.is_empty() {
            return Err(format!(
                "Custom provider was saved, but temporary validation cleanup failed: {cleanup_error}"
            ));
        }
    }
    let connection_ids = configuration
        .connection_graph
        .connections
        .values()
        .filter(|existing_connection| {
            existing_connection.provider_id == provider_id
                && existing_connection.id != connection_id
        })
        .map(|existing_connection| existing_connection.id.clone())
        .collect::<Vec<_>>();
    for existing_connection_id in connection_ids {
        runtime.remove_connection_catalog(existing_connection_id.as_str())?;
    }
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
            let removed = configuration
                .custom_providers
                .remove(provider_id.as_str())
                .ok_or_else(|| format!("Custom provider `{provider_id}` is not configured."))?;
            if removed.kind != ProviderKind::Custom {
                return Err(format!(
                    "Provider `{provider_id}` is not a user-defined provider."
                ));
            }
            Ok(())
        })
        .await?;
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)?;
    build_public_ai_settings_snapshot(runtime.as_ref(), &configuration)
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
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
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
        }) && !current_configuration
            .provider_tool_disabled_targets
            .get(&request.capability)
            .is_some_and(|disabled| disabled.contains(selection));
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

/// Saves the enabled state of one current provider tool target.
/// 保存一个当前供应商工具目标的启用状态。
pub(super) async fn service_save_public_ai_provider_tool_target_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderToolTargetEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let available = runtime.available_provider_tool_targets(&current_configuration)?;
    let valid = available.iter().any(|candidate| {
        candidate.connection_id == request.selection.connection_id
            && candidate.target.target_id == request.selection.target_id
            && candidate.target.contract.capability == request.capability
    });
    if !valid {
        return Err(format!(
            "Provider tool target `{}` is not available through connection `{}` for capability `{:?}`.",
            request.selection.target_id, request.selection.connection_id, request.capability
        ));
    }
    let configuration = mutate_public_ai_configuration(
        &state,
        "保存公共 AI 工具目标启用状态失败",
        |configuration| {
            if request.enabled {
                if let Some(disabled) = configuration
                    .provider_tool_disabled_targets
                    .get_mut(&request.capability)
                {
                    disabled.remove(&request.selection);
                    if disabled.is_empty() {
                        configuration
                            .provider_tool_disabled_targets
                            .remove(&request.capability);
                    }
                }
            } else {
                configuration
                    .provider_tool_disabled_targets
                    .entry(request.capability)
                    .or_default()
                    .insert(request.selection.clone());
                if configuration
                    .provider_tool_selections
                    .get(&request.capability)
                    == Some(&request.selection)
                {
                    configuration
                        .provider_tool_selections
                        .remove(&request.capability);
                }
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
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>, String> {
    let session_id = conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| format!("Invalid conversation id `{conversation_id}`: {error}"))?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
        .ok_or_else(|| format!("Conversation `{session_id}` has no runtime state."))?;
    let selections = if runtime_state.provider_tool_selections_overridden {
        session_provider_tool_selections_from_runtime(runtime_state)?
    } else {
        configuration.provider_tool_selections.clone()
    };
    filter_current_enabled_provider_tool_selections(runtime.as_ref(), &configuration, &selections)
}

/// Saves the complete single-select provider tool configuration for one session.
/// 保存单个会话完整的供应商工具单选配置。
pub(super) fn service_save_public_ai_session_provider_tool_selections(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiSessionProviderToolSelectionsSaveRequest,
) -> Result<BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>, String> {
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
    validate_provider_tool_selections(runtime.as_ref(), &configuration, &request.selections)?;
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
        runtime_state.provider_tool_selections_overridden = true;
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
            Some(String::from("provider_tool_selections")),
        );
    }
    Ok(request.selections)
}

/// Validates that every selected target belongs to its exact capability.
/// 校验每个选中目标均属于其精确能力。
pub(crate) fn validate_provider_tool_selections(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
) -> Result<(), String> {
    let available = runtime.available_provider_tool_targets(configuration)?;
    for (capability, selection) in selections {
        let valid = available.iter().any(|candidate| {
            candidate.connection_id == selection.connection_id
                && candidate.target.target_id == selection.target_id
                && candidate.target.contract.capability == *capability
        }) && !configuration
            .provider_tool_disabled_targets
            .get(capability)
            .is_some_and(|disabled| disabled.contains(selection));
        if !valid {
            return Err(format!(
                "Provider tool target `{}` is not available through connection `{}` for capability `{:?}`.",
                selection.target_id, selection.connection_id, capability
            ));
        }
    }
    Ok(())
}

/// Filters historical selections to targets that still exist and remain enabled.
/// 将历史选择过滤为仍存在且保持启用的当前目标。
fn filter_current_enabled_provider_tool_selections(
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
            }) && !configuration
                .provider_tool_disabled_targets
                .get(capability)
                .is_some_and(|disabled| disabled.contains(selection))
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
                .entry(access_key)
                .or_insert_with(|| PublicAiModelAccessRecord {
                    provider_id: connection.provider_id.clone(),
                    model_id: model.id.clone(),
                    enabled: configuration
                        .model_is_enabled(connection.provider_id.as_str(), model.id.as_str()),
                    default_enabled: vulcan_provider_adapters::default_model_enabled(
                        connection.provider_id.as_str(),
                        model.id.as_str(),
                    ),
                });
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
    let available_provider_tool_targets = runtime.available_provider_tool_targets(configuration)?;
    let provider_tool_selections = filter_current_enabled_provider_tool_selections(
        runtime,
        configuration,
        &configuration.provider_tool_selections,
    )?;
    let internal_inference_candidates =
        build_internal_inference_candidates(configuration, &providers, &catalogs)?;
    let inference_route_diagnostics =
        build_internal_inference_route_diagnostics(runtime, configuration);
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
        catalog_errors: runtime.catalog_error_snapshot()?,
        model_overrides: configuration.model_overrides.clone(),
        model_override_records,
        model_access_records,
        provider_tool_targets: available_provider_tool_targets
            .into_iter()
            .map(|available| {
                let capability = available.target.contract.capability;
                let selection = ProviderToolTargetSelection {
                    connection_id: available.connection_id.clone(),
                    target_id: available.target.target_id.clone(),
                };
                let enabled = !configuration
                    .provider_tool_disabled_targets
                    .get(&capability)
                    .is_some_and(|disabled| disabled.contains(&selection));
                PublicAiProviderToolTargetSnapshot {
                    connection_id: available.connection_id,
                    connection_display_name: available.connection_display_name,
                    target: available.target,
                    enabled,
                }
            })
            .collect(),
        provider_tool_selections,
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
        .provider_tool_selections
        .retain(|_, selection| selection.connection_id != connection_id);
    Ok(())
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
}
