//! Provider-declared OAuth authorization and reauthorization lifecycle.
//! 供应商声明的 OAuth 授权与重新授权生命周期。

use serde::Deserialize;
use vulcan_provider_adapters::{
    CredentialMode, CredentialSecretEnvelope, CredentialSecretSnapshot, CredentialSecretStore,
    OAuthAuthorizationChallenge, OAuthClientConfiguration, OAuthTransactionPhase,
    OAuthTransactionSnapshot, PersistedAiConfiguration, PublicAiRuntime,
    credential_authorization_binding,
};
use vulcan_service_api::ServiceEventEmitter;

use super::{
    PublicAiConnectionUpdateRequest, PublicAiSettingsSnapshot,
    add_public_ai_connection_with_secret, apply_connection_update,
    build_public_ai_settings_snapshot, compensated_operation_error,
    public_ai_runtime_and_configuration, publish_public_ai_workbench,
    restore_public_ai_configuration_activation_in_transaction,
    restore_public_ai_runtime_activation, validate_non_empty,
};
use crate::{
    SharedServiceBridgeState, begin_public_ai_configuration_transaction,
    mutate_public_ai_configuration_in_transaction, shell_host_from_state,
};

/// Starts one exact provider OAuth flow without persisting transaction state.
/// 启动一个精确供应商 OAuth 流程且不持久化事务状态。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiOAuthStartRequest {
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
pub(crate) struct PublicAiOAuthCommitRequest {
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
///
/// # Parameters
/// 参数
/// `state` owns the live authorization runtime; `request` selects the exact declared credential profile.
/// `state` 持有实时授权运行时；`request` 选择精确声明的凭据档。
///
/// # Returns
/// 返回值
/// The started authorization challenge, or an explicit profile, startup, or browser error.
/// 返回已启动的授权挑战，或明确的凭据档、启动或浏览器错误。
pub(crate) async fn service_start_public_ai_oauth(
    state: SharedServiceBridgeState,
    request: PublicAiOAuthStartRequest,
) -> Result<OAuthAuthorizationChallenge, String> {
    super::openvulcan_account::reject_generic_provider_mutation(&state, &request.provider_id)?;
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
        &entry.base_url,
        authorization,
    )?;
    let challenge = runtime
        .oauth()
        .start(
            authorization,
            authorization_binding,
            credential_profile.key_prefix.clone(),
            &entry.base_url,
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
///
/// # Parameters
/// 参数
/// `state` owns process-local transactions; `transaction_id` identifies the exact transaction to observe.
/// `state` 持有进程内事务；`transaction_id` 标识要观测的精确事务。
///
/// # Returns
/// 返回值
/// The current transaction snapshot, or an explicit lookup error.
/// 返回当前事务快照，或明确的查询错误。
pub(crate) async fn service_public_ai_oauth_status(
    state: SharedServiceBridgeState,
    transaction_id: String,
) -> Result<OAuthTransactionSnapshot, String> {
    validate_non_empty("transactionId", transaction_id.as_str())?;
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    runtime.oauth().status(transaction_id.as_str()).await
}

/// Cancels one process-local OAuth transaction.
/// 取消一个进程内 OAuth 事务。
///
/// # Parameters
/// 参数
/// `state` owns process-local transactions; `transaction_id` identifies the exact transaction to cancel.
/// `state` 持有进程内事务；`transaction_id` 标识要取消的精确事务。
///
/// # Returns
/// 返回值
/// The cancellation result snapshot, or an explicit transaction error.
/// 返回取消结果快照，或明确的事务错误。
pub(crate) async fn service_cancel_public_ai_oauth(
    state: SharedServiceBridgeState,
    transaction_id: String,
) -> Result<OAuthTransactionSnapshot, String> {
    validate_non_empty("transactionId", transaction_id.as_str())?;
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    runtime.oauth().cancel(transaction_id.as_str()).await
}

/// Commits one successful OAuth transaction as a new or reauthorized connection.
/// 将一个成功 OAuth 事务提交为新连接或已有连接的重新授权。
///
/// # Parameters
/// 参数
/// `app` publishes the resulting workspace; `state` owns configuration and secrets; `request` selects the successful transaction and optional existing connection.
/// `app` 发布结果工作台；`state` 持有配置和秘密；`request` 选择成功事务及可选的现有连接。
///
/// # Returns
/// 返回值
/// The complete settings snapshot after publication, or an error preserving the existing compensation paths.
/// 发布后返回完整设置快照，失败时保留既有补偿路径并返回错误。
pub(crate) async fn service_commit_public_ai_oauth_connection(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiOAuthCommitRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    super::openvulcan_account::reject_generic_provider_mutation(&state, &request.provider_id)?;
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
        &entry.base_url,
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
        let (previous, replacement) = super::credential_repair::replace_credential_material(
            secret_store.as_ref(),
            current.secret_reference.as_str(),
            material,
        )
        .await?;
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
        if let Err(error) = refresh_reauthorized_connection_catalog(
            runtime.as_ref(),
            &validation_configuration,
            connection_id.as_str(),
        )
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
            "Failed to reauthorize the public AI connection",
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
                    "Failed to restore the configuration from before the OAuth reauthorization",
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
                    "Failed to restore the configuration from before the OAuth reauthorization",
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

/// Invalidates the previous account observation before validating a newly authorized catalog.
/// 在验证新授权目录前，使上一账号观测失效。
/// `runtime` owns account caches, `configuration` selects the new profile, and `connection_id` is exact.
/// `runtime` 持有账号缓存，`configuration` 选择新配置档，`connection_id` 是精确连接标识。
/// Returns success after catalog validation; the caller retains its complete before-image for rollback.
/// 目录验证后返回成功；调用方保留完整前镜像以供回滚。
async fn refresh_reauthorized_connection_catalog(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
) -> Result<(), String> {
    // A stable connection ID may now belong to another user or billing workspace.
    // 稳定连接 ID 此时可能已属于另一用户或计费工作区。
    // Clear old quota and access evidence before rebuilding; fresh account-access queries may repopulate it.
    // 重建前清除旧额度及访问证据；新的账号访问查询可重新填充缓存。
    runtime.remove_connection_usage(connection_id)?;
    runtime
        .refresh_connection_catalog(configuration, connection_id)
        .await
        .map(|_| ())
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
    previous: Option<&CredentialSecretSnapshot>,
    replacement: &CredentialSecretEnvelope,
) -> Result<(), String> {
    super::credential_repair::rollback_credential_replacement(
        secret_store,
        secret_reference,
        previous,
        replacement,
    )
    .await
}

#[cfg(test)]
mod tests;
