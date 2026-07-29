//! Public AI connection settings and runtime metadata services.
//! 公共 AI 连接设置与运行时元数据服务。

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, PromptCachePolicy};
use vulcan_provider_adapters::{
    AccountUsageQueryKind, ActiveAiModelSelection, ConnectionModelCatalog, CredentialBillingKind,
    CredentialConnection, CredentialMode, CredentialSecretEnvelope, CredentialSecretResolver,
    CredentialSecretStore, CustomProviderAuthentication, CustomProviderDefinition,
    CustomProviderProtocol, ModelExecutionOverrides, OAuthAuthorizationChallenge,
    OAuthClientConfiguration, OAuthTransactionPhase, OAuthTransactionSnapshot,
    PersistedAiConfiguration, ProviderDescriptor, ProviderKind, ProviderToolCapabilityKind,
    ProviderToolTarget, ProviderToolTargetSelection, PublicAiRuntime, ServiceEntryKind,
};
use vulcan_service_api::{ServiceEventEmitter, WorkbenchSnapshot};

use crate::{
    SharedServiceBridgeState, ai_credential_secret_store::OsAiCredentialSecretStore,
    begin_public_ai_configuration_transaction, emit_workbench_event,
    mutate_public_ai_configuration, mutate_public_ai_configuration_in_transaction,
    public_ai_provider_rows_for_selection, shell_host_from_state,
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
fn validate_provider_tool_selections(
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
fn provider_tool_capability_key(capability: ProviderToolCapabilityKind) -> String {
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
    })
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
