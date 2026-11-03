//! Credential connection lifecycle, exact validation, and compensating secret mutations.
//! 凭据连接生命周期、精确验证与秘密变更补偿。

use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot};
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, CredentialSecretEnvelope, CredentialSecretMaterial,
    CredentialSecretSnapshot, CredentialSecretStore, MediaUnderstandingBackendSelection,
    PersistedAiConfiguration, PublicAiRuntime, ServiceEntry, ServiceEntryKind,
};
use vulcan_service_api::ServiceEventEmitter;

use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    catalog_projection::{PublicAiModelDirectoryKind, service_entry_model_directory_kind},
    combine_results, compensated_operation_error, ensure_active_model_selection,
    mutate_public_ai_configuration_and_publish_in_transaction,
    preferred_replacement_model_selection, public_ai_runtime_and_configuration,
    publication::mutate_public_ai_configuration_and_publish_sessions_in_transaction,
    publish_public_ai_workbench,
    publish_public_ai_workbench_with_session_reconciliation_and_commit,
    reconcile_disabled_connection_sessions,
    restore_public_ai_configuration_activation_in_transaction,
    restore_public_ai_runtime_activation, validate_non_empty,
};
use crate::{
    SharedServiceBridgeState, begin_public_ai_configuration_transaction,
    mutate_public_ai_configuration_in_transaction,
};

/// Adds one credential connection and its secret.
/// 添加一个凭据连接及其秘密。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiConnectionAddRequest {
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
pub(crate) struct PublicAiConnectionUpdateRequest {
    /// Exact connection identifier.
    /// 精确连接标识。
    pub(super) connection_id: String,
    /// User-visible credential label.
    /// 用户可见凭据标签。
    pub(super) display_name: String,
    /// Selected service entry.
    /// 选定服务入口。
    pub(super) service_entry_id: String,
    /// Selected credential profile.
    /// 选定的凭据配置档。
    pub(super) credential_profile_id: String,
    /// Selected credential mode.
    /// 选定凭据方式。
    pub(super) credential_mode: CredentialMode,
    /// Optional replacement secret.
    /// 可选替换秘密。
    pub(super) secret: Option<String>,
    /// Optional replacement provider-management key.
    /// 可选替换供应商管理密钥。
    pub(super) management_key: Option<String>,
    /// Whether the existing provider-management key should be removed.
    /// 是否移除现有供应商管理密钥。
    #[serde(default)]
    pub(super) remove_management_key: bool,
    /// Optional credential-scoped provider service origin.
    /// 可选凭据作用域供应商服务源地址。
    pub(super) service_origin_override: Option<String>,
}

/// Changes the explicit default connection for one provider.
/// 修改一个供应商的显式默认连接。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiDefaultConnectionSaveRequest {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    provider_id: String,
    /// Exact connection identifier.
    /// 精确连接标识。
    connection_id: String,
}

/// Saves the reversible runtime enabled state for one credential connection.
/// 保存一个凭据连接可逆的运行时启用状态。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiConnectionEnabledSaveRequest {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: String,
    /// Requested enabled state.
    /// 请求的启用状态。
    enabled: bool,
}

/// Removes one connection with an explicit replacement default when required.
/// 在需要时携带显式替代默认账号移除一个连接。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiConnectionRemoveRequest {
    /// Exact connection to remove.
    /// 待移除的精确连接。
    pub(super) connection_id: String,
    /// Explicit replacement default for the same provider.
    /// 同一供应商的显式替代默认账号。
    pub(super) replacement_default_connection_id: Option<String>,
}

/// Adds and synchronizes one public AI credential connection.
/// 添加并同步一个公共 AI 凭据连接。
///
/// # Parameters
/// 参数
/// `app` publishes the workspace; `state` owns the runtime; `request` contains the exact static credential and connection metadata.
/// `app` 发布工作台；`state` 持有运行时；`request` 包含精确静态凭据与连接元数据。
///
/// # Returns
/// 返回值
/// The published settings snapshot, or an error from validation, persistence, or compensation.
/// 返回已发布设置快照，或校验、持久化、补偿产生的错误。
pub(crate) async fn service_add_public_ai_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionAddRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    super::openvulcan_account::reject_generic_provider_mutation(&state, &request.provider_id)?;
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
///
/// # Parameters
/// 参数
/// `app` and `state` own publication and configuration; provider, entry, profile, mode, and label identify the connection; `secret` is its validated envelope; the optional management key and service origin follow the declared credential contract.
/// `app` 与 `state` 持有发布与配置；供应商、入口、凭据档、模式及标签标识连接；`secret` 是已校验封装；可选管理密钥与服务源遵循声明的凭据契约。
///
/// # Returns
/// 返回值
/// The settings snapshot after successful publication, with failures retaining the existing secret and activation compensation.
/// 成功发布后返回设置快照，失败时保留既有秘密与激活补偿。
// Connection creation keeps credential material distinct from provider routing and management credentials.
// 连接创建将凭据材料与供应商路由、管理凭据分开。
#[expect(
    clippy::too_many_arguments,
    reason = "Connection creation keeps credential material distinct from provider routing and management credentials."
)]
pub(super) async fn add_public_ai_connection_with_secret(
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
    // Core accounts are unique and bind their secure region to the signed authorization declaration.
    // 核心账户保持唯一，并将安全区域绑定到签名授权声明。
    if provider.credential_directory_visibility
        == vulcan_provider_adapters::CredentialDirectoryVisibility::CoreAccount
    {
        if current_configuration
            .connection_graph
            .connections
            .values()
            .any(|connection| connection.provider_id == provider.id)
        {
            return Err(String::from("A core account is already connected."));
        }
        let vulcan_provider_adapters::CredentialSecretMaterial::OpenVulcanAuth {
            region,
            rotation_pending: false,
            ..
        } = &secret.material
        else {
            return Err(String::from(
                "Core accounts require usable native authorization.",
            ));
        };
        let expected = match region {
            vulcan_provider_adapters::OpenVulcanRegion::China => {
                vulcan_provider_adapters::OAuthProviderKind::OpenVulcanChina
            }
            vulcan_provider_adapters::OpenVulcanRegion::Global => {
                vulcan_provider_adapters::OAuthProviderKind::OpenVulcanGlobal
            }
        };
        if !matches!(&credential_profile.authorization, Some(vulcan_provider_adapters::CredentialAuthorization::RegisteredOauth { provider }) if *provider == expected)
        {
            return Err(String::from(
                "Core account region does not match its authorization declaration.",
            ));
        }
    }
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
        "Failed to save the public AI connection",
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
                "Failed to restore the configuration from before the public AI connection was added",
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
                "Failed to restore the configuration from before the public AI connection was added",
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
/// Success after discarding the previous identity's usage and validating the new credential.
/// 丢弃上一身份用量并验证新凭据后返回成功。
/// Callers retain the activation before-image and restore it if validation or publication fails.
/// 调用方保留激活前镜像，并在验证或发布失败时恢复。
async fn validate_connection_credential(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
    entry_kind: ServiceEntryKind,
) -> Result<(), String> {
    // This entry point is used for new credentials or explicit identity changes, never a name edit.
    // 此入口仅用于新凭据或显式身份变更，绝不用于名称编辑。
    runtime.remove_connection_usage(connection_id)?;
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

/// Rejects model synchronization unless the backend-resolved directory is remotely dynamic.
/// 除非后端解析出的模型目录可远端动态更新，否则拒绝模型同步。
///
/// # Parameters
/// 参数
/// `connection_id` identifies the requested account and `directory_kind` is the exact
/// Manifest-derived aggregate policy for its service entry.
/// `connection_id` 标识请求账号，`directory_kind` 是其服务入口由 Manifest 推导的精确聚合策略。
///
/// # Returns
/// 返回值
/// Success only for dynamic directories, or a controlled error before credential validation.
/// 仅动态目录返回成功，否则在凭据验证前返回受控错误。
fn validate_model_synchronization_directory(
    connection_id: &str,
    directory_kind: PublicAiModelDirectoryKind,
) -> Result<(), String> {
    if directory_kind == PublicAiModelDirectoryKind::Dynamic {
        Ok(())
    } else {
        Err(format!(
            "Connection `{connection_id}` uses a non-dynamic model directory and cannot synchronize models."
        ))
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

/// Updates one connection and refreshes only when its connection identity changes.
/// 更新一个连接，并仅在连接身份变化时刷新。
///
/// # Parameters
/// 参数
/// `app` publishes the workspace; `state` owns the current configuration and secure store; `request` supplies edits for one exact connection.
/// `app` 发布工作台；`state` 持有当前配置与安全仓库；`request` 提供一个精确连接的编辑。
///
/// # Returns
/// 返回值
/// The published settings snapshot, or an error after the applicable compensation path.
/// 返回已发布设置快照，或执行对应补偿路径后的错误。
pub(crate) async fn service_update_public_ai_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionUpdateRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    super::openvulcan_account::reject_generic_connection_mutation(&state, &request.connection_id)?;
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
        let replacement = super::credential_repair::replace_credential_material(
            secret_store.as_ref(),
            current.secret_reference.as_str(),
            CredentialSecretMaterial::ApiKey {
                value: secret.to_string(),
            },
        )
        .await?;
        Some(replacement)
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
        "Failed to update the public AI connection",
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
                "Failed to restore the configuration from before the public AI connection update",
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
                "Failed to restore the configuration from before the public AI connection update",
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
pub(super) fn apply_connection_update(
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

/// Restores a replaced secret with a monotonic revision when validation or persistence fails.
/// 当验证或持久化失败时使用单调递增修订恢复已替换秘密。
///
/// # Parameters
/// 参数
/// `secret_store` is the secure store, `secret_reference` is opaque, and
/// `replaced_secret` contains the previous and replacement envelopes when a secret changed.
/// `secret_store` 是安全仓库，`secret_reference` 是不透明引用，
/// `replaced_secret` 在秘密变更时包含旧封装与替换封装。
///
/// # Returns
/// 返回值
/// Success for no replacement or a revision-checked restore, otherwise the explicit store error.
/// 未替换或修订校验恢复成功时返回成功，否则返回明确仓库错误。
async fn rollback_replaced_secret(
    secret_store: &dyn CredentialSecretStore,
    secret_reference: &str,
    replaced_secret: Option<&(Option<CredentialSecretSnapshot>, CredentialSecretEnvelope)>,
) -> Result<(), String> {
    let Some((previous, replacement)) = replaced_secret else {
        return Ok(());
    };
    super::credential_repair::rollback_credential_replacement(
        secret_store,
        secret_reference,
        previous.as_ref(),
        replacement,
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
        previous: Option<CredentialSecretSnapshot>,
        replacement: Box<CredentialSecretEnvelope>,
    },
    /// An existing management credential was removed.
    /// 已移除现有管理凭据。
    Removed {
        reference: String,
        previous: Option<CredentialSecretSnapshot>,
    },
}

impl ManagementSecretMutation {
    /// Resolves the connection reference produced by this mutation.
    /// 解析此变更产生的连接引用。
    ///
    /// # Parameters
    /// 参数
    /// `self` records the applied edit; `current` supplies the original reference for an unchanged edit.
    /// `self` 记录已应用的编辑；`current` 为未变更情形提供原引用。
    ///
    /// # Returns
    /// 返回值
    /// The resulting optional reference without changing persisted secret state.
    /// 返回结果可选引用，不修改持久秘密状态。
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
    ///
    /// # Parameters
    /// 参数
    /// `self` is the applied management credential edit.
    /// `self` 是已应用的管理凭据编辑。
    ///
    /// # Returns
    /// 返回值
    /// True only when a newly created or replaced credential requires verification.
    /// 仅当新建或替换凭据需要验证时返回true。
    fn requires_validation(&self) -> bool {
        matches!(self, Self::Created { .. } | Self::Replaced { .. })
    }

    /// Returns whether this mutation removes the allowance-query credential.
    /// 返回此变更是否移除用量查询凭据。
    ///
    /// # Parameters
    /// 参数
    /// `self` is the applied management credential edit.
    /// `self` 是已应用的管理凭据编辑。
    ///
    /// # Returns
    /// 返回值
    /// True only for removal, allowing the caller to invalidate the corresponding usage cache.
    /// 仅移除状态返回true，供调用方失效对应的用量缓存。
    fn removes_key(&self) -> bool {
        matches!(self, Self::Removed { .. })
    }
}

/// Applies the requested optional management-key edit before configuration persistence.
/// 在配置持久化前应用请求的可选管理密钥编辑。
///
/// # Parameters
/// 参数
/// `secret_store` owns secure state; `current` identifies the connection; `management_key` supplies an optional replacement; `remove_management_key` requests explicit removal.
/// `secret_store` 持有安全状态；`current` 标识连接；`management_key` 提供可选替换值；`remove_management_key` 请求显式移除。
///
/// # Returns
/// 返回值
/// The exact reversible mutation, or a secure-store error before configuration publication.
/// 返回精确可逆变更，或配置发布前的安全仓库错误。
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
        let previous = secret_store.capture_secret(reference).await?;
        CredentialSecretStore::delete_secret(secret_store, reference).await?;
        return Ok(ManagementSecretMutation::Removed {
            reference: reference.to_string(),
            previous,
        });
    }
    let Some(value) = management_key.filter(|value| !value.trim().is_empty()) else {
        return Ok(ManagementSecretMutation::Unchanged);
    };
    if let Some(reference) = current.management_secret_reference.as_deref() {
        let (previous, replacement) = super::credential_repair::replace_credential_material(
            secret_store,
            reference,
            CredentialSecretMaterial::ApiKey {
                value: value.to_string(),
            },
        )
        .await?;
        return Ok(ManagementSecretMutation::Replaced {
            reference: reference.to_string(),
            previous,
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
///
/// # Parameters
/// 参数
/// `secret_store` owns secure state and `mutation` carries the exact before-image and replacement revision.
/// `secret_store` 持有安全状态，`mutation` 携带精确前镜像与替换修订。
///
/// # Returns
/// 返回值
/// Success when the matching inverse operation finishes, or its explicit secure-store error.
/// 对应逆操作完成时返回成功，否则返回明确安全仓库错误。
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
            super::credential_repair::rollback_credential_replacement(
                secret_store,
                reference,
                previous.as_ref(),
                replacement,
            )
            .await
        }
        ManagementSecretMutation::Removed {
            reference,
            previous,
        } => {
            super::credential_repair::restore_deleted_credential(
                secret_store,
                reference,
                previous.as_ref(),
            )
            .await
        }
    }
}

/// Deletes the primary and optional management secrets as one cleanup operation.
/// 将主秘密和可选管理秘密作为一次清理操作删除。
///
/// # Parameters
/// 参数
/// `secret_store` owns secure state; `primary_reference` is required and `management_reference` identifies the optional secondary credential.
/// `secret_store` 持有安全状态；`primary_reference` 为必需主引用，`management_reference` 标识可选次级凭据。
///
/// # Returns
/// 返回值
/// Success after required deletions, or an explicit diagnostic preserving partial cleanup failure.
/// 必需删除完成后返回成功，否则返回保留部分清理失败的明确诊断。
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
    primary_secret: &Option<CredentialSecretSnapshot>,
    management_secret: Option<(&str, &Option<CredentialSecretSnapshot>)>,
) -> Result<(), String> {
    // primaryRestore returns the required credential to its exact revision and material.
    // primaryRestore 将必需凭据恢复到精确修订与材料。
    let primary_restore = super::credential_repair::restore_deleted_credential(
        secret_store,
        primary_reference,
        primary_secret.as_ref(),
    )
    .await;
    // managementRestore independently restores the declared secondary credential when present.
    // managementRestore 在存在时独立恢复已声明的次级凭据。
    let management_restore = if let Some((reference, secret)) = management_secret {
        super::credential_repair::restore_deleted_credential(
            secret_store,
            reference,
            secret.as_ref(),
        )
        .await
    } else {
        Ok(())
    };
    combine_results(primary_restore, management_restore)
}

/// Saves one explicit provider default connection.
/// 保存一个显式供应商默认连接。
///
/// # Parameters
/// 参数
/// `app` and `state` own publication and configuration; `request` names the provider and its exact replacement default connection.
/// `app` 与 `state` 持有发布和配置；`request` 指定供应商及其精确替换默认连接。
///
/// # Returns
/// 返回值
/// The published snapshot, or a validation/publication error through the compensating transaction.
/// 返回已发布快照，或补偿事务中的校验/发布错误。
pub(crate) async fn service_save_public_ai_default_connection(
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
        "Failed to save the public AI default account",
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
            if !configuration.connection_is_enabled(request.connection_id.as_str()) {
                return Err(format!(
                    "Connection `{}` is disabled and cannot become a provider default.",
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

/// Enables or disables one stored credential without deleting its secret or metadata.
/// 启用或停用一个已保存凭据，且不删除其秘密或元数据。
///
/// # Parameters
/// 参数
/// `app` publishes the workbench, `state` owns runtime state, and `request` identifies the connection and desired state.
/// `app` 发布工作台，`state` 持有运行时状态，`request` 标识连接及目标状态。
///
/// # Returns
/// 返回值
/// The authoritative settings snapshot after atomic persistence, activation, and session reconciliation.
/// 原子持久化、激活与会话协调后的权威设置快照。
pub(crate) async fn service_save_public_ai_connection_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionEnabledSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    super::openvulcan_account::reject_generic_connection_mutation(&state, &request.connection_id)?;
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    // transaction retains writer ownership across persistence, activation, sessions, and publication.
    // transaction 在持久化、激活、会话与发布期间保持写入所有权。
    let transaction = begin_public_ai_configuration_transaction().await;
    // currentConfiguration is the exact rollback image and source for the requested transition.
    // currentConfiguration 是精确的回滚镜像，也是请求状态转换的来源。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // connection freezes the current owner before the candidate configuration is mutated.
    // connection 在候选配置变更前冻结当前归属。
    let connection = current_configuration
        .connection_graph
        .connections
        .get(request.connection_id.as_str())
        .cloned()
        .ok_or_else(|| format!("Connection `{}` is not configured.", request.connection_id))?;
    // disabledActive captures the exact global selection that needs a deterministic replacement.
    // disabledActive 捕获需要确定性替换的精确全局选择。
    let disabled_active = (!request.enabled)
        .then(|| current_configuration.active.clone())
        .flatten()
        .filter(|selection| selection.connection_id.as_str() == connection.id.as_str());
    // candidateConfiguration carries the sparse disabled state through the normal activation path.
    // candidateConfiguration 通过常规激活路径承载稀疏停用状态。
    let mut candidate_configuration = current_configuration.clone();
    candidate_configuration
        .set_connection_enabled(request.connection_id.as_str(), request.enabled)?;
    // candidateCatalogs proves the disabled connection no longer contributes an effective catalog.
    // candidateCatalogs 证明已停用连接不再贡献有效目录。
    let candidate_catalogs = runtime.resolved_model_catalog_snapshot(&candidate_configuration)?;
    if let Some(selection) = disabled_active {
        candidate_configuration.active = preferred_replacement_model_selection(
            runtime.as_ref(),
            &candidate_configuration,
            &candidate_catalogs,
            selection.provider_id.as_str(),
            selection.model_id.as_str(),
        )?;
    }
    runtime.validate_model_catalog_overlays(&candidate_configuration)?;

    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_sessions_in_transaction(
            &app,
            &state,
            runtime.as_ref(),
            &current_configuration,
            "Failed to save the credential enable state",
            |configuration| {
                *configuration = candidate_configuration.clone();
                Ok(())
            },
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |workspace| {
                Ok(if request.enabled {
                    std::collections::BTreeSet::new()
                } else {
                    reconcile_disabled_connection_sessions(
                        workspace,
                        request.connection_id.as_str(),
                        None,
                    )
                })
            },
            "credential_connection_disabled",
        )?;
    drop(transaction);
    Ok(snapshot)
}

/// Removes one public AI credential connection and its secret.
/// 移除一个公共 AI 凭据连接及其秘密。
///
/// # Parameters
/// 参数
/// `app` and `state` own publication and configuration; `request` names the removed connection and an explicit replacement default when required.
/// `app` 与 `state` 持有发布和配置；`request` 指定待删除连接及必要时的显式替换默认账号。
///
/// # Returns
/// 返回值
/// The published snapshot after exact reference cleanup, or a compensated failure.
/// 精确引用清理并发布后返回快照，否则返回补偿后的失败。
pub(super) async fn remove_public_ai_connection_inner(
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
    let removed_secret = secret_store
        .capture_secret(removed.secret_reference.as_str())
        .await?;
    let removed_management_secret =
        if let Some(reference) = removed.management_secret_reference.as_deref() {
            Some(secret_store.capture_secret(reference).await?)
        } else {
            None
        };
    CredentialSecretStore::delete_secret(secret_store.as_ref(), removed.secret_reference.as_str())
        .await?;
    if let Some(reference) = removed.management_secret_reference.as_deref()
        && let Err(error) =
            CredentialSecretStore::delete_secret(secret_store.as_ref(), reference).await
    {
        let restore = super::credential_repair::restore_deleted_credential(
            secret_store.as_ref(),
            removed.secret_reference.as_str(),
            removed_secret.as_ref(),
        )
        .await;
        return Err(compensated_operation_error(error, restore, Ok(())));
    }
    let remove_result = mutate_public_ai_configuration_in_transaction(
        &state,
        "Failed to remove the public AI connection",
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
                "Failed to restore the configuration from before the public AI connection was removed",
            );
            return Err(compensated_operation_error(
                error,
                secret_restore,
                activation_restore,
            ));
        }
    };
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
                "Failed to restore the configuration from before the public AI connection was removed",
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

/// Removes a user-managed connection; core accounts must use their complete logout lifecycle.
/// 移除用户管理的连接；核心账户必须使用完整退出生命周期。
pub(crate) async fn service_remove_public_ai_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiConnectionRemoveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    super::openvulcan_account::reject_generic_connection_mutation(&state, &request.connection_id)?;
    remove_public_ai_connection_inner(app, state, request).await
}

/// Refreshes one connection's declared runtime metadata on demand.
/// 按需刷新一个连接声明的运行时元数据。
///
/// # Parameters
/// 参数
/// `app` publishes the workspace; `state` owns the runtime and configuration; `connection_id` selects the exact account to refresh.
/// `app` 发布工作台；`state` 持有运行时与配置；`connection_id` 选择要刷新的精确账号。
///
/// # Returns
/// 返回值
/// The published settings snapshot, or the retained discovery, activation, or publication failure.
/// 返回已发布设置快照，或保留的发现、激活、发布失败。
pub(crate) async fn service_sync_public_ai_connection(
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
    if !configuration.connection_is_enabled(connection_id.as_str()) {
        return Err(format!(
            "Connection `{connection_id}` is disabled and cannot synchronize models."
        ));
    }
    let entry = registered_service_entry(
        runtime.as_ref(),
        &configuration,
        connection.provider_id.as_str(),
        connection.service_entry_id.as_str(),
    )?;
    validate_model_synchronization_directory(
        connection_id.as_str(),
        service_entry_model_directory_kind(
            &entry,
            configuration
                .custom_providers
                .contains_key(&connection.provider_id),
        ),
    )?;
    let refresh_result = validate_connection_credential(
        runtime.as_ref(),
        &configuration,
        connection_id.as_str(),
        entry.kind,
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
                    "Failed to repair the public AI active model account",
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

/// Queries usage for one exact connection without caching it on disk.
/// 查询一个精确连接的用量且不写入磁盘缓存。
///
/// # Parameters
/// 参数
/// `state` owns the runtime and configuration; `connection_id` selects one exact declared account.
/// `state` 持有运行时与配置；`connection_id` 选择一个精确声明的账号。
///
/// # Returns
/// 返回值
/// The current normalized usage observation, or an explicit runtime/provider error.
/// 返回当前归一化用量观测，或明确的运行时/供应商错误。
pub(crate) async fn service_query_public_ai_connection_usage(
    state: SharedServiceBridgeState,
    connection_id: String,
) -> Result<AccountUsageSnapshot, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    runtime
        .query_connection_usage(&configuration, connection_id.as_str())
        .await
}

/// Removes one connection and all preferences that reference it.
/// 移除一个连接以及引用它的全部偏好。
///
/// # Parameters
/// 参数
/// `configuration` is the candidate state; `connection_id` selects the removed account; `replacement_default_connection_id` supplies the required explicit replacement only when deleting a non-final default.
/// `configuration` 是候选状态；`connection_id` 选择待移除账号；仅删除并非最后一个的默认账号时，`replacement_default_connection_id` 提供必需的显式替换。
///
/// # Returns
/// 返回值
/// Success after exact reference cleanup, or a validation error before mutation begins.
/// 精确引用清理后返回成功，或变更前返回校验错误。
pub(super) fn remove_connection_from_configuration(
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
            configuration.connection_is_enabled(connection.id.as_str())
                && !configuration.connection_is_pending_validation(connection.id.as_str())
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
    configuration.disabled_connection_ids.remove(connection_id);
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
    if configuration
        .voice_input_selection
        .as_ref()
        .is_some_and(|selection| selection.connection_id == connection_id)
    {
        configuration.voice_input_selection = None;
    }
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
    configuration
        .provider_extension_modes
        .retain(|_, selection| {
            if selection
                .single_target
                .as_ref()
                .is_some_and(|target| target.connection_id == connection_id)
            {
                selection.single_target = None;
            }
            !selection.is_default()
        });
    Ok(())
}

#[cfg(test)]
mod tests;
