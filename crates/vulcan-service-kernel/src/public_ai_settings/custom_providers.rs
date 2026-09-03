//! Custom-provider candidate validation, credential migration, and compensated lifecycle services.
//! 自定义供应商候选验证、凭据迁移与可补偿生命周期服务。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::AiOperation;
use vulcan_provider_adapters::{
    CatalogDiscoveryState, CatalogDiscoveryStatus, ConnectionModelCatalog, CredentialConnection,
    CredentialSecretEnvelope, CredentialSecretResolver, CredentialSecretStore, CustomProtocolRoute,
    CustomProviderAuthentication, CustomProviderDefinition, CustomProviderModelDiscoveryMode,
    CustomProviderProtocol, InferenceRouteBinding, InferenceRouteConfiguration,
    PersistedAiConfiguration, ProviderDescriptor, ProviderKind, ProviderRegistry, PublicAiRuntime,
    ResponsesRemoteCompactionPolicy, ResponsesRouteFeatures, ResponsesWebsocketPolicy,
    StagedConnectionCatalogObservation, validate_custom_provider_identifier,
};
use vulcan_service_api::ServiceEventEmitter;

use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    connections::remove_connection_from_configuration,
    ensure_active_model_selection, public_ai_runtime_and_configuration,
    publication::{
        compensated_operation_error, publish_public_ai_workbench,
        publish_public_ai_workbench_with_session_reconciliation_and_commit,
        restore_public_ai_configuration_activation_in_transaction,
    },
    reconcile_explicitly_revoked_session_model_selections, validate_non_empty,
};
use crate::{
    SharedServiceBridgeState, begin_public_ai_configuration_transaction,
    mutate_public_ai_configuration_in_transaction,
};

/// Creates or replaces one user-defined compatible provider.
/// 创建或替换一个用户自定义兼容供应商。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomProviderSaveRequest {
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
    /// Optional regular-conversation session-header name for this provider.
    /// 此供应商可选的常规会话请求头名称。
    #[serde(default)]
    session_header: Option<String>,
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
pub(crate) struct PublicAiCustomProviderProbeResult {
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

/// Removes one user-defined compatible provider.
/// 移除一个用户自定义兼容供应商。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiCustomProviderRemoveRequest {
    /// Exact custom provider identifier.
    /// 精确自定义供应商标识。
    provider_id: String,
    /// Whether the user explicitly confirmed deleting all credentials owned by this provider.
    /// 用户是否明确确认同时删除该供应商持有的全部凭据。
    #[serde(default)]
    confirm_credential_deletion: bool,
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
/// `current` supplies existing provider declarations and exact credential ownership for edits.
/// `current` 提供编辑所需的既有供应商声明与精确凭据归属。
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
        session_header: request.session_header.clone(),
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

/// Migrates every existing credential to the newly declared service and credential contract.
/// 将全部既有凭据迁移到新声明的服务与凭据契约。
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
///
/// # Parameters
/// 参数
/// `state` is the exact credential-scoped discovery result.
/// `state` 是精确的凭据作用域发现结果。
///
/// # Returns
/// 返回值
/// True only when the state permits explicit manual model entry.
/// 仅当该状态允许显式手工录入模型时返回真。
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
///
/// # Parameters
/// 参数
/// `state` is the exact credential-scoped discovery result.
/// `state` 是精确的凭据作用域发现结果。
///
/// # Returns
/// 返回值
/// True only when a transient discovery failure permits deferred validation.
/// 仅当临时发现失败允许延后验证时返回真。
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
pub(crate) async fn service_probe_public_ai_custom_provider(
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
///
/// # Parameters
/// 参数
/// `app` emits the committed snapshot; `state` owns the service runtime; `request` declares the provider edit.
/// `app` 发出已提交快照；`state` 持有服务运行时；`request` 声明供应商编辑。
///
/// # Returns
/// 返回值
/// Returns the published settings snapshot or an explicit validation, commit, or compensation error.
/// 返回已发布的设置快照，或显式验证、提交、补偿错误。
pub(crate) async fn service_save_public_ai_custom_provider(
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
                // effective_providers validates every connection against the newly declared provider contract.
                // effective_providers 根据新声明的供应商契约校验每一条连接。
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
pub(crate) async fn service_remove_public_ai_custom_provider(
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

/// Deletes every unique credential secret owned by a provider and compensates partial deletion.
/// 删除供应商持有的全部唯一凭据秘密，并对部分删除执行补偿恢复。
///
/// # Parameters
/// 参数
/// `secret_store` owns credential persistence; `connections` identifies the provider-owned references.
/// `secret_store` 持有凭据持久化能力；`connections` 标识供应商持有的引用。
///
/// # Returns
/// 返回值
/// Returns the deleted envelopes for compensation, or an error including partial-deletion recovery failures.
/// 返回供补偿使用的已删除信封，或包含部分删除恢复失败的错误。
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
///
/// # Parameters
/// 参数
/// `secret_store` restores persistence; `secrets` maps exact references to their original envelopes.
/// `secret_store` 恢复持久化；`secrets` 将精确引用映射到原始信封。
///
/// # Returns
/// 返回值
/// Returns success after all restores, or the collected failures without skipping remaining references.
/// 全部恢复后返回成功，否则汇总失败且不中断其余引用的恢复。
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

/// Validates that one exact identifier owns a configured user-defined provider.
/// 校验一个精确标识持有已配置的用户自定义供应商。
///
/// # Parameters
/// 参数
/// `configuration` owns the provider declarations; `provider_id` is the exact identifier to validate.
/// `configuration` 持有供应商声明；`provider_id` 是待校验的精确标识。
///
/// # Returns
/// 返回值
/// Returns success only for a configured custom provider, otherwise an explicit identity error.
/// 仅为已配置的自定义供应商返回成功，否则返回显式身份错误。
pub(super) fn validate_custom_provider_identity(
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

#[cfg(test)]
mod tests;
