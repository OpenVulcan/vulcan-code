//! Application-wide OpenVulcan account lifecycle shared by every project.
//! 所有工程共享的应用全局 OpenVulcan 账户生命周期。

use super::{
    PublicAiConnectionRemoveRequest, PublicAiSettingsSnapshot,
    add_public_ai_connection_with_secret, public_ai_runtime_and_configuration,
};
use crate::{
    SharedServiceBridgeState, public_ai_configuration_app_data_root, shell_host_from_state,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use tokio::sync::Mutex;
use vulcan_provider_adapters::{
    CredentialAuthorization, CredentialDirectoryVisibility, CredentialMode,
    CredentialSecretEnvelope, CredentialSecretMaterial, OAuthAuthorizationChallenge,
    OAuthClientConfiguration, OAuthProviderKind, OAuthTransactionPhase, OAuthTransactionSnapshot,
    OpenVulcanRegion, PersistedAiConfiguration, PublicAiRuntime, credential_authorization_binding,
    query_openvulcan_identity, revoke_openvulcan_authorization,
};
use vulcan_service_api::ServiceEventEmitter;

#[cfg(test)]
mod discovery_tests;

/// Exact pending authorization, retained across provider runtime generations.
/// 跨供应商运行时代际保留的准确待处理授权。
struct PendingAccount {
    /// Runtime that owns the callback task and cancellation handle.
    /// 持有回调任务及取消句柄的运行时。
    runtime: Arc<PublicAiRuntime>,
    /// Exact transaction and manifest identities frozen at authorization start.
    /// 授权开始时冻结的准确事务及 Manifest 身份。
    transaction_id: String,
    /// Signed provider identity captured before the browser flow begins.
    /// 浏览器流程开始前捕获的签名供应商身份。
    provider_id: String,
    /// Regional entry captured from that provider declaration.
    /// 从该供应商声明捕获的区域入口。
    service_entry_id: String,
    /// Exact native authorization profile captured within the selected entry.
    /// 在所选入口内捕获的准确原生授权配置档。
    credential_profile_id: String,
    /// Authorization declaration binding prevents cross-entry commits.
    /// 授权声明绑定防止跨入口提交。
    authorization_binding: String,
}

/// Returns the process-wide account operation lock keyed by application data root.
/// 返回按应用数据根目录隔离的进程全局账户操作锁。
fn lifecycle() -> &'static Mutex<BTreeMap<PathBuf, PendingAccount>> {
    // sessions serializes begin/commit/cancel/logout so a stale callback cannot revive a signed-out account.
    // sessions 串行化开始、提交、取消和退出，防止过期回调复活已退出账户。
    static SESSIONS: OnceLock<Mutex<BTreeMap<PathBuf, PendingAccount>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Browser-safe global account state; it never contains credential material.
/// 浏览器安全的全局账户状态；绝不包含凭据材料。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountSnapshot {
    /// Whether a usable global login exists.
    /// 是否存在可用的全局登录。
    connected: bool,
    /// Exact installation connection, absent after logout.
    /// 准确安装连接，退出后缺省。
    connection_id: Option<String>,
    /// Last verified user label, shared with the core provider connection.
    /// 最近验证的用户标签，与核心供应商连接共享。
    display_name: Option<String>,
    /// Credential-owned deployment.
    /// 凭据持有的部署。
    region: Option<OpenVulcanRegion>,
    /// Controlled reauthorization or transport diagnostic.
    /// 受控重新授权或传输诊断。
    message: Option<String>,
}

/// Finds the sole core connection declared by the current signed provider registry.
/// 查找当前签名供应商注册表声明的唯一核心连接。
/// runtime resolves provider ownership; configuration supplies global connections. Returns none or one connection, rejecting ambiguity.
/// runtime 解析供应商归属；configuration 提供全局连接。返回无连接或唯一连接，拒绝歧义。
fn account_connection(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Result<Option<vulcan_provider_adapters::CredentialConnection>, String> {
    let providers = runtime.effective_providers(configuration)?;
    let mut accounts = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| {
            providers
                .provider(&connection.provider_id)
                .is_some_and(|provider| {
                    provider.credential_directory_visibility
                        == CredentialDirectoryVisibility::CoreAccount
                })
        });
    let account = accounts.next().cloned();
    if accounts.next().is_some() {
        return Err(String::from(
            "Multiple core account connections violate the global account contract.",
        ));
    }
    Ok(account)
}

/// Rejects a generic mutation for provider_id when its account is lifecycle-managed.
/// 当 provider_id 的账户由生命周期管理时拒绝通用变更。
/// state resolves the current registry; success admits ordinary providers, while a core provider returns a controlled error.
/// state 解析当前注册表；成功时准入普通供应商，核心供应商返回受控错误。
pub(super) fn reject_generic_provider_mutation(
    state: &SharedServiceBridgeState,
    provider_id: &str,
) -> Result<(), String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)?;
    if runtime
        .effective_providers(&configuration)?
        .provider(provider_id)
        .is_some_and(|provider| {
            provider.credential_directory_visibility == CredentialDirectoryVisibility::CoreAccount
        })
    {
        return Err(String::from(
            "Core account connections must be managed through account sign-in and sign-out.",
        ));
    }
    Ok(())
}

/// Applies the same lifecycle guard to an exact persisted connection_id.
/// 对准确持久连接 connection_id 应用同一生命周期护栏。
/// state supplies the global connection graph; returns the provider guard result without changing persisted state.
/// state 提供全局连接图；返回供应商护栏结果，不更改持久状态。
pub(super) fn reject_generic_connection_mutation(
    state: &SharedServiceBridgeState,
    connection_id: &str,
) -> Result<(), String> {
    let (_, configuration) = public_ai_runtime_and_configuration(state)?;
    if let Some(connection) = configuration
        .connection_graph
        .connections
        .get(connection_id)
    {
        reject_generic_provider_mutation(state, &connection.provider_id)?;
    }
    Ok(())
}

/// Atomically claims the once-per-installation login prompt; false means another window already showed it.
/// 原子认领每次安装仅出现一次的登录提示；false 表示其他窗口已经展示。
pub(crate) async fn claim_prompt() -> Result<bool, String> {
    claim_prompt_at(&public_ai_configuration_app_data_root())
}

/// Claims root's installation marker atomically without depending on the currently selected project.
/// 原子认领 root 的安装标记，不依赖当前选中的工程。
fn claim_prompt_at(root: &std::path::Path) -> Result<bool, String> {
    std::fs::create_dir_all(root)
        .map_err(|error| format!("Failed to create global account directory: {error}"))?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("openvulcan-onboarding-v1.seen"))
    {
        Ok(file) => {
            file.sync_all()
                .map_err(|error| format!("Failed to persist onboarding marker: {error}"))?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(format!(
            "Failed to claim global account onboarding: {error}"
        )),
    }
}

/// Reads cursor for the sole logged-in account under the lifecycle lock and returns exact usage strings.
/// 在生命周期锁下为唯一已登录账户读取 cursor，并返回准确用量字符串。
/// state selects the shared runtime; cursor is an opaque continuation. Missing login, transport and protocol failures return errors.
/// state 选择共享运行时；cursor 是不透明续页标识。缺少登录、传输失败或协议失败时返回错误。
pub(crate) async fn usage_page(
    state: SharedServiceBridgeState,
    cursor: String,
) -> Result<vulcan_provider_adapters::OpenVulcanUsagePage, String> {
    let _sessions = lifecycle().lock().await;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let connection = account_connection(&runtime, &configuration)?
        .ok_or_else(|| String::from("Sign in to OpenVulcan before reading usage."))?;
    let secret = runtime
        .resolve_openvulcan_account_credential(&configuration, &connection.id)
        .await?;
    let providers = runtime.effective_providers(&configuration)?;
    let entry = providers
        .provider(&connection.provider_id)
        .and_then(|provider| {
            provider
                .service_entries
                .iter()
                .find(|entry| entry.id == connection.service_entry_id)
        })
        .ok_or_else(|| String::from("OpenVulcan account entry is unavailable."))?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| String::from("Failed to create account transport."))?;
    vulcan_provider_adapters::query_openvulcan_usage_page(&http, entry, &secret, &cursor).await
}

/// Resolves the signed-in account's credential for a backend-only request, renewing it through the
/// shared lease; `None` when nobody is signed in. The connection id identifies the account.
/// 为仅限后端的请求解析已登录账户的凭据，经共享租约续期；无人登录时为 `None`。连接 id 标识该账户。
pub(crate) async fn signed_in_credential(
    state: &SharedServiceBridgeState,
) -> Result<Option<(String, vulcan_provider_adapters::CredentialExecutionSecret)>, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(state)?;
    let Some(connection) = account_connection(&runtime, &configuration)? else {
        return Ok(None);
    };
    let secret = runtime
        .resolve_openvulcan_account_credential(&configuration, &connection.id)
        .await?;
    Ok(Some((connection.id, secret)))
}

/// Returns shared account state and renews expiring credentials through the existing single-flight lease.
/// 返回共享账户状态，并通过既有单飞租约续期即将到期的凭据。
/// app publishes invalidation and state owns the global runtime; returns public identity and diagnostics without credential material.
/// app 发布失效变更，state 持有全局运行时；返回公开身份及诊断，不包含凭据材料。
pub(crate) async fn snapshot(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<AccountSnapshot, String> {
    let _sessions = lifecycle().lock().await;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let Some(connection) = account_connection(&runtime, &configuration)? else {
        return Ok(AccountSnapshot {
            connected: false,
            connection_id: None,
            display_name: None,
            region: None,
            message: None,
        });
    };
    let resolved = runtime
        .resolve_openvulcan_account_credential(&configuration, &connection.id)
        .await;
    let stored = runtime
        .credential_secret_store()
        .read_secret(&connection.secret_reference)
        .await?;
    let CredentialSecretMaterial::OpenVulcanAuth {
        region,
        rotation_pending,
        expires_at_unix_ms,
        ..
    } = stored.material
    else {
        return Err(String::from(
            "Core account contains an unsupported credential.",
        ));
    };
    // Expired or interrupted single-use credentials no longer constitute a logged-in account.
    // 已过期或中断的单次凭据不再构成已登录账户。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| String::from("System clock precedes Unix epoch."))?
        .as_millis();
    // Remote authorization rejection clears login; transport outages preserve valid local credentials.
    // 远端拒绝授权时清除登录；传输故障保留有效本地凭据。
    let remote_failure = if let Ok(secret) = &resolved {
        let providers = runtime.effective_providers(&configuration)?;
        let entry = providers
            .provider(&connection.provider_id)
            .and_then(|provider| {
                provider
                    .service_entries
                    .iter()
                    .find(|entry| entry.id == connection.service_entry_id)
            })
            .ok_or_else(|| String::from("OpenVulcan account entry is unavailable."))?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| String::from("Failed to create account transport."))?;
        query_openvulcan_identity(&http, entry, secret).await.err()
    } else {
        None
    };
    let rejected = remote_failure.as_ref().is_some_and(|failure| {
        failure.kind == vulcan_provider_adapters::ModelDiscoveryFailureKind::AuthenticationFailed
    });
    let connected = resolved.is_ok() && !rejected;
    let message = resolved
        .err()
        .or_else(|| remote_failure.map(|failure| failure.message));
    if rejected || (!connected && (rotation_pending || u128::from(expires_at_unix_ms) <= now)) {
        super::connections::remove_public_ai_connection_inner(
            app,
            state,
            PublicAiConnectionRemoveRequest {
                connection_id: connection.id,
                replacement_default_connection_id: None,
            },
        )
        .await?;
        return Ok(AccountSnapshot {
            connected: false,
            connection_id: None,
            display_name: None,
            region: None,
            message,
        });
    }
    Ok(AccountSnapshot {
        connected,
        connection_id: Some(connection.id),
        display_name: Some(connection.display_name),
        region: Some(region),
        message,
    })
}

/// Starts one declared regional login and opens the user's browser; replacing a pending login cancels its callback.
/// 启动一个已声明区域登录并打开用户浏览器；替换待处理登录时取消其回调。
/// state provides the runtime and browser host; region selects the isolated authority. Returns the exact transaction challenge or an error.
/// state 提供运行时及浏览器宿主；region 选择隔离的授权端。返回准确事务挑战或错误。
pub(crate) async fn start(
    state: SharedServiceBridgeState,
    region: OpenVulcanRegion,
) -> Result<OAuthAuthorizationChallenge, String> {
    let mut sessions = lifecycle().lock().await;
    let root = public_ai_configuration_app_data_root();
    if let Some(previous) = sessions.remove(&root) {
        let _ = previous
            .runtime
            .oauth()
            .cancel_openvulcan_login(&previous.transaction_id)
            .await;
    }
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    if account_connection(&runtime, &configuration)?.is_some() {
        return Err(String::from(
            "Sign out of the current OpenVulcan account before starting another login.",
        ));
    }
    let providers = runtime.effective_providers(&configuration)?;
    let kind = match region {
        OpenVulcanRegion::China => OAuthProviderKind::OpenVulcanChina,
        OpenVulcanRegion::Global => OAuthProviderKind::OpenVulcanGlobal,
    };
    // selection derives every identifier from the signed manifest, never from UI-supplied provider fields.
    // selection 从签名 Manifest 派生全部标识，绝不来自界面提供的供应商字段。
    let selection = providers.providers().into_iter().filter(|provider| provider.credential_directory_visibility == CredentialDirectoryVisibility::CoreAccount).flat_map(|provider| provider.service_entries.iter().flat_map(move |entry| entry.credential_profiles.iter().map(move |profile| (provider, entry, profile)))).find(|(_, _, profile)| matches!(&profile.authorization, Some(CredentialAuthorization::RegisteredOauth { provider }) if *provider == kind)).ok_or_else(|| String::from("The requested OpenVulcan region has no registered account entry."))?;
    let (provider, entry, profile) = selection;
    let authorization = profile
        .authorization
        .as_ref()
        .ok_or_else(|| String::from("Core account entry omitted authorization."))?;
    let binding =
        credential_authorization_binding(&provider.id, &entry.id, &profile.id, authorization)?;
    let challenge = runtime
        .oauth()
        .start(
            authorization,
            binding.clone(),
            None,
            OAuthClientConfiguration {
                return_page_locale: crate::service_ui_locale_from_state(&state),
                ..OAuthClientConfiguration::default()
            },
        )
        .await?;
    if let Err(error) = shell_host_from_state(&state)?.open_url(&challenge.authorization_url) {
        let _ = runtime
            .oauth()
            .cancel_openvulcan_login(&challenge.transaction.transaction_id)
            .await;
        return Err(error);
    }
    sessions.insert(
        root,
        PendingAccount {
            runtime: Arc::clone(&runtime),
            transaction_id: challenge.transaction.transaction_id.clone(),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: profile.id.clone(),
            authorization_binding: binding,
        },
    );
    Ok(challenge)
}

/// Observes only the currently owned account transaction, including across runtime replacement.
/// 仅观察当前持有的账户事务，包括运行时替换期间。
/// transaction_id must identify the current global pending login; returns its public phase or an inactive-transaction error.
/// transaction_id 必须标识当前全局待处理登录；返回其公开阶段或事务失效错误。
pub(crate) async fn status(transaction_id: String) -> Result<OAuthTransactionSnapshot, String> {
    let sessions = lifecycle().lock().await;
    let pending = sessions
        .get(&public_ai_configuration_app_data_root())
        .filter(|pending| pending.transaction_id == transaction_id)
        .ok_or_else(|| String::from("OpenVulcan authorization is no longer active."))?;
    pending.runtime.oauth().status(&transaction_id).await
}

/// Cancels the exact transaction; a stale cancellation never cancels a newer login.
/// 取消准确事务；过期取消操作绝不取消更新的登录。
/// transaction_id selects only its owned flow; returns success after local removal and best-effort remote revocation.
/// transaction_id 仅选择其持有的流程；本地移除及尽力远程撤销后返回成功。
pub(crate) async fn cancel(transaction_id: String) -> Result<(), String> {
    let mut sessions = lifecycle().lock().await;
    let root = public_ai_configuration_app_data_root();
    if sessions
        .get(&root)
        .is_some_and(|pending| pending.transaction_id == transaction_id)
    {
        let pending = sessions
            .remove(&root)
            .ok_or_else(|| String::from("OpenVulcan authorization disappeared."))?;
        let _ = pending
            .runtime
            .oauth()
            .cancel_openvulcan_login(&pending.transaction_id)
            .await;
    }
    Ok(())
}

/// Verifies account identity and atomically creates its global core provider connection.
/// 验证账户身份，并原子创建其全局核心供应商连接。
/// transaction_id binds completed authorization; state/app own persistence and publication. Returns the activated settings or compensates a failed commit.
/// transaction_id 绑定已完成授权；state/app 持有持久化及发布。返回已激活设置，或对失败提交执行补偿。
pub(crate) async fn commit(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    transaction_id: String,
) -> Result<PublicAiSettingsSnapshot, String> {
    let mut sessions = lifecycle().lock().await;
    let root = public_ai_configuration_app_data_root();
    let pending = sessions
        .get(&root)
        .filter(|pending| pending.transaction_id == transaction_id)
        .ok_or_else(|| String::from("OpenVulcan authorization is no longer active."))?;
    let transaction = pending.runtime.oauth().status(&transaction_id).await?;
    if transaction.phase != OAuthTransactionPhase::Succeeded
        || transaction.authorization_binding != pending.authorization_binding
    {
        return Err(String::from(
            "OpenVulcan authorization has not completed for this account entry.",
        ));
    }
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    if account_connection(&runtime, &configuration)?.is_some() {
        return Err(String::from("An OpenVulcan account is already connected."));
    }
    let providers = runtime.effective_providers(&configuration)?;
    let provider = providers
        .provider(&pending.provider_id)
        .ok_or_else(|| String::from("OpenVulcan provider is no longer registered."))?;
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == pending.service_entry_id)
        .ok_or_else(|| String::from("OpenVulcan entry is no longer registered."))?;
    let profile = entry
        .credential_profile(&pending.credential_profile_id)
        .ok_or_else(|| String::from("OpenVulcan account profile is no longer registered."))?;
    let authorization = profile
        .authorization
        .as_ref()
        .ok_or_else(|| String::from("OpenVulcan authorization declaration disappeared."))?;
    if credential_authorization_binding(&provider.id, &entry.id, &profile.id, authorization)?
        != pending.authorization_binding
    {
        return Err(String::from(
            "OpenVulcan authorization declaration changed during login.",
        ));
    }
    let material = pending
        .runtime
        .oauth()
        .take_succeeded(&transaction_id)
        .await?;
    let envelope = CredentialSecretEnvelope::new(material.clone())?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| String::from("Failed to create account transport."))?;
    let identity = query_openvulcan_identity(&http, entry, &envelope.execution_secret()?).await;
    let pending = sessions
        .remove(&root)
        .ok_or_else(|| String::from("OpenVulcan authorization disappeared during finalization."))?;
    let identity = match identity {
        Ok(identity) => identity,
        Err(error) => {
            let _ = revoke_openvulcan_authorization(&material).await;
            return Err(error.message);
        }
    };
    let result = add_public_ai_connection_with_secret(
        app,
        state,
        pending.provider_id,
        pending.service_entry_id,
        pending.credential_profile_id,
        CredentialMode::Auth,
        identity.username,
        envelope,
        None,
        None,
    )
    .await;
    if result.is_err() {
        let _ = revoke_openvulcan_authorization(&material).await;
    }
    result
}

/// Cancels pending login, removes local credentials, then revokes the exact device remotely.
/// 取消待处理登录，删除本地凭据，然后远程撤销准确设备。
/// A remote error is returned after local sign-out, so offline users never remain locally logged in.
/// 远端错误在本地退出后返回，保证离线用户不会继续保持本地登录。
/// app publishes the cleared graph and state resolves the account; returns success or the explicit cleanup/revocation failure.
/// app 发布已清理连接图，state 解析账户；返回成功或显式清理／撤销失败。
pub(crate) async fn logout(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<(), String> {
    let mut sessions = lifecycle().lock().await;
    if let Some(pending) = sessions.remove(&public_ai_configuration_app_data_root()) {
        let _ = pending
            .runtime
            .oauth()
            .cancel_openvulcan_login(&pending.transaction_id)
            .await;
    }
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let Some(connection) = account_connection(&runtime, &configuration)? else {
        return Ok(());
    };
    let secret = runtime
        .credential_secret_store()
        .read_secret(&connection.secret_reference)
        .await?;
    super::connections::remove_public_ai_connection_inner(
        app,
        state,
        PublicAiConnectionRemoveRequest {
            connection_id: connection.id,
            replacement_default_connection_id: None,
        },
    )
    .await?;
    revoke_openvulcan_authorization(&secret.material).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Concurrent windows get one shared first-run claim, and a reopened process observes the marker.
    /// 并发窗口仅获得一个共享首启认领，重新打开的进程可观察到标记。
    #[test]
    fn openvulcan_prompt_is_atomic_and_installation_scoped() {
        // directory is an isolated fixture; no user onboarding or account files are touched.
        // directory 是隔离夹具；不会触碰用户引导或账户文件。
        let directory = tempfile::tempdir().unwrap();
        let workers = (0..8)
            .map(|_| {
                let root = directory.path().to_path_buf();
                std::thread::spawn(move || claim_prompt_at(&root).unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .filter(|claimed| *claimed)
                .count(),
            1
        );
        assert!(!claim_prompt_at(directory.path()).unwrap());
        let second = tempfile::tempdir().unwrap();
        assert!(claim_prompt_at(second.path()).unwrap());
    }
}
