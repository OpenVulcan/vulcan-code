//! Redacted browser settings snapshots assembled from one catalog generation.
//! 基于同一目录代际组装的脱敏浏览器设置快照。

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{AccountUsageSnapshot, PromptCachePolicy};
use vulcan_context::{
    ContextCompactionWindowNodeId, ContextWindowPolicy, ContextWindowPolicyOverrides,
};
use vulcan_provider_adapters::{
    AccountUsageQueryKind, ActiveAiModelSelection, CUSTOM_PROVIDER_DEFAULT_CONTEXT_LIMIT,
    CUSTOM_PROVIDER_DEFAULT_OUTPUT_LIMIT, CatalogDiscoveryStatus, ConnectionModelCatalog,
    ContextCompactionWindowConfiguration, CredentialBillingKind, CredentialConnection,
    CredentialMode, CustomModelDefinition, CustomModelProtocolPreference,
    CustomModelTemplateDecision, CustomProviderManifestFailure, CustomProviderModelDefaults,
    DeclaredProviderToolTemplateModel, InferenceProfile, InternalModelOverrides,
    ManifestSourceLayer, MediaUnderstandingBackendOptions, MediaUnderstandingBackendSelection,
    ModelCapabilityOverrides, ModelExecutionOverrides, PersistedAiConfiguration,
    ProviderDescriptor, ProviderRegistry, ProviderToolCapabilityKind, ProviderToolTarget,
    ProviderToolTargetSelection, PublicAiRuntime, PublicAiRuntimeSettingsCatalogSnapshot,
};

use super::{
    catalog_projection::{
        PublicAiConnectionModelAccessPolicyRecord, PublicAiModelRouteRecord,
        build_connection_model_access_policy_records, build_public_ai_model_route_records,
        catalog_model_supports_text_conversation,
    },
    inference_settings::{
        PublicAiInferenceRouteDiagnostic, PublicAiInternalInferenceCandidate,
        build_internal_inference_candidates, build_internal_inference_route_diagnostics,
    },
    media_candidates::{
        PublicAiMediaUnderstandingModelCandidate, build_media_understanding_model_candidates,
    },
    media_delivery_settings::PublicAiMediaDeliverySettingsSnapshot,
    provider_tools::filter_current_provider_tool_selections,
};

/// Shares one backend-owned default pair with provider-level manual model forms.
/// 向供应商级手工模型表单共享一组由后端持有的默认值。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiManualModelDefaults {
    /// Default input context capacity in tokens.
    /// 默认输入上下文 Token 容量。
    context_limit: u64,
    /// Default output ceiling in tokens.
    /// 默认输出 Token 上限。
    output_limit: u64,
}

/// Browser-facing public AI settings snapshot.
/// 面向浏览器的公共 AI 设置快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiSettingsSnapshot {
    /// Backend-owned numeric defaults for a newly entered provider-level conversation model.
    /// 后端持有的供应商级新增会话模型数值默认值。
    manual_model_defaults: PublicAiManualModelDefaults,
    /// Effective media policy, its independent revision and backend configuration limits.
    /// 生效媒体策略、其独立修订及后端配置限制。
    media_delivery: PublicAiMediaDeliverySettingsSnapshot,
    /// System-level sparse Codex context-window policy inherited by every route.
    /// 所有路由继承的系统级稀疏 Codex 上下文窗口策略。
    context_window_policy_overrides: ContextWindowPolicyOverrides,
    /// Built-in values every route uses for a policy field nobody overrides.
    /// 无人覆盖的策略字段在所有路由上使用的内置值。
    context_window_policy_defaults: ContextWindowPolicy,
    /// Fixed system context-compaction window nodes in immutable display order.
    /// 按不可变展示顺序排列的固定系统上下文压缩窗口节点。
    context_compaction_window_nodes: ContextCompactionWindowConfiguration,
    /// Default window node applied only when a new conversation is created.
    /// 仅在新建会话时应用的默认窗口节点。
    new_session_context_compaction_window_node_id: Option<ContextCompactionWindowNodeId>,
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
    /// Immutable provider-discovery catalogs before user overlays, limited to custom-provider
    /// connections because only their model editor compares against the raw discovery result.
    /// 应用用户覆盖前的不可变供应商发现目录；只包含自定义供应商连接，
    /// 因为只有其模型编辑器需要对照原始发现结果。
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
pub(crate) struct PublicAiCustomModelProtocolPreferenceRecord {
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
pub(crate) struct PublicAiCustomModelTemplateDecisionRecord {
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
pub(crate) struct PublicAiCustomModelConnectionAvailabilityRecord {
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

/// Browser-facing provider tool target paired with one credential.
/// 面向浏览器且与一个凭据配对的供应商工具目标。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiProviderToolTargetSnapshot {
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
pub(crate) struct PublicAiMultimodalUnderstandingCandidateRecord {
    /// Exact provider identifier shared by all credential routes for the model.
    /// 该模型全部凭据路由共享的精确供应商标识。
    provider_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
}

/// Browser-facing credential metadata that excludes the host secret-store reference.
/// 面向浏览器且排除宿主秘密存储引用的凭据元数据。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PublicAiConnectionSnapshot {
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
    /// Whether this credential connection currently participates in runtime work.
    /// 此凭据连接当前是否参与运行时工作。
    enabled: bool,
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
            enabled: configuration.connection_is_enabled(connection.id.as_str()),
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
pub(crate) struct PublicAiModelOverrideRecord {
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
pub(crate) struct PublicAiModelAccessRecord {
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
pub(crate) struct PublicAiModelCapabilityOverrideRecord {
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

/// Builds a redacted browser snapshot from persisted and process-local state.
/// 从持久化状态与进程内状态构建脱敏浏览器快照。
///
/// # Parameters
/// 参数
/// `runtime` supplies one frozen catalog generation; `configuration` retains persisted settings and repair diagnostics.
/// `runtime` 提供同一冻结目录代际；`configuration` 保留持久设置与修复诊断。
///
/// # Returns
/// 返回值
/// A secret-free browser projection, or an explicit generation, configuration, or projection failure.
/// 返回不含秘密的浏览器投影，或明确的代际、配置、投影失败。
pub(super) fn build_public_ai_settings_snapshot(
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
        connection_model_initial_policies,
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
    let connection_model_access_policy_records = build_connection_model_access_policy_records(
        &providers,
        configuration,
        &connection_model_initial_policies,
    )?;
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
        manual_model_defaults: PublicAiManualModelDefaults {
            context_limit: CUSTOM_PROVIDER_DEFAULT_CONTEXT_LIMIT,
            output_limit: CUSTOM_PROVIDER_DEFAULT_OUTPUT_LIMIT,
        },
        media_delivery: PublicAiMediaDeliverySettingsSnapshot::from_configuration(configuration)?,
        context_window_policy_overrides: configuration.context_window_policy_overrides,
        context_window_policy_defaults: ContextWindowPolicy::default(),
        context_compaction_window_nodes: configuration.context_compaction_window_nodes.clone(),
        new_session_context_compaction_window_node_id: configuration
            .new_session_context_compaction_window_node_id,
        usage_refresh_interval_seconds: configuration.usage_refresh_interval_seconds,
        provider_priorities: configuration.provider_priority_overrides.clone(),
        usage_cache,
        providers: providers
            .providers()
            .into_iter()
            .filter(|provider| {
                provider.credential_directory_visibility
                    != vulcan_provider_adapters::CredentialDirectoryVisibility::CoreAccount
                    || configuration
                        .connection_graph
                        .connections
                        .values()
                        .any(|connection| connection.provider_id == provider.id)
            })
            .cloned()
            .collect(),
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
        source_catalogs: source_catalogs
            .into_iter()
            .filter(|(connection_id, _)| {
                configuration
                    .connection_graph
                    .connections
                    .get(connection_id.as_str())
                    .is_some_and(|connection| {
                        configuration
                            .custom_providers
                            .contains_key(connection.provider_id.as_str())
                    })
            })
            .collect(),
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

#[cfg(test)]
mod tests;
