//! Account-usage observations, declared credential roles, and access-evidence retention.
//! 账号用量观测、声明凭据角色与访问证据保留。

use super::PublicAiRuntime;
use crate::{PersistedAiConfiguration, ProviderMetadataClient};
use std::collections::BTreeMap;
use vulcan_ai_contract::AccountUsageSnapshot;

/// Selects the credential reference declared by one account-usage query kind.
/// 选择一个账号用量查询类型声明的凭据引用。
///
/// # Parameters
/// 参数
/// `connection` owns the available credential roles and `credential_profile` declares which role
/// the normalized usage implementation requires.
/// `connection` 持有可用凭据角色，`credential_profile` 声明归一化用量实现需要的角色。
///
/// # Returns
/// 返回值
/// The exact secret reference, or an actionable error when the declared role is absent.
/// 返回精确密钥引用；声明的角色缺失时返回可操作错误。
pub(super) fn account_usage_secret_reference<'a>(
    connection: &'a crate::CredentialConnection,
    credential_profile: &crate::CredentialProfile,
) -> Result<&'a str, String> {
    match credential_profile.account_usage_query {
        crate::AccountUsageQueryKind::OpenRouterCredits => connection
            .management_secret_reference
            .as_deref()
            .ok_or_else(|| {
                String::from("Add an optional OpenRouter management key to query account credits.")
            }),
        crate::AccountUsageQueryKind::Unsupported
        | crate::AccountUsageQueryKind::MiniMaxTokenPlan
        | crate::AccountUsageQueryKind::TavilyCredits
        | crate::AccountUsageQueryKind::DeepSeekBalance
        | crate::AccountUsageQueryKind::KimiCodingPlan
        | crate::AccountUsageQueryKind::CommandCodeUsage => {
            Ok(connection.secret_reference.as_str())
        }
    }
}

impl PublicAiRuntime {
    /// Queries normalized account usage for one exact credential connection.
    /// 查询一个精确凭据连接的归一化账号用量。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the persisted public AI configuration and `connection_id` is exact.
    /// `configuration` 是持久化公共 AI 配置，`connection_id` 为精确连接标识。
    ///
    /// # Returns
    /// 返回值
    /// A provider-normalized usage snapshot.
    /// 返回供应商归一化后的用量快照。
    pub async fn query_connection_usage(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
    ) -> Result<AccountUsageSnapshot, String> {
        if !configuration.connection_is_enabled(connection_id) {
            return Err(format!(
                "Connection `{connection_id}` is disabled and cannot query account usage."
            ));
        }
        let _refresh_guard = self.catalog_refresh_flight.lock().await;
        let (_provider, entry, connection) =
            self.resolve_connection_metadata(configuration, connection_id)?;
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                )
            })?;
        let secret_reference = account_usage_secret_reference(&connection, credential_profile)?;
        let secret = self
            .resolve_fresh_execution_secret(connection.id.as_str(), secret_reference)
            .await?;
        let snapshot = self
            .metadata_client
            .query_account_usage(&entry, &connection, &secret)
            .await;
        let account_model_access_enabled = credential_profile.account_model_access.is_some();
        let snapshot =
            self.cache_usage_snapshot(connection_id, snapshot, account_model_access_enabled)?;
        if account_model_access_enabled {
            self.rebuild_catalog_after_usage(configuration, connection_id)?;
        }
        Ok(snapshot)
    }

    /// Caches one usage observation while retaining confirmed access evidence through transient failures.
    /// 缓存一次用量观测，并在暂时失败时保留已确认的访问证据。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the exact credential, `snapshot` is the provider observation,
    /// and `preserve_access_evidence` protects the last confirmed plan and quota evidence.
    /// `connection_id` 标识精确凭据，`snapshot` 是供应商观测，`preserve_access_evidence`
    /// 保护最后一次确认的套餐与额度证据。
    ///
    /// # Returns
    /// 返回值
    /// The snapshot stored in the process-local cache.
    /// 已写入进程内缓存的观测。
    pub(super) fn cache_usage_snapshot(
        &self,
        connection_id: &str,
        mut snapshot: AccountUsageSnapshot,
        preserve_access_evidence: bool,
    ) -> Result<AccountUsageSnapshot, String> {
        // mutation_guard serializes plan facts with every immutable catalog generation publication.
        // mutation_guard 把套餐事实与每次不可变目录代际发布串行化。
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let mut cache = self
            .usage_cache
            .write()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?;
        if preserve_access_evidence
            && !matches!(
                snapshot.availability,
                vulcan_ai_contract::AccountUsageAvailability::Supported
            )
            && let Some(previous) = cache.get(connection_id)
        {
            if snapshot.plan_name.is_none() {
                snapshot.plan_name = previous.plan_name.clone();
            }
            if snapshot.plan_id.is_none() {
                snapshot.plan_id = previous.plan_id.clone();
            }
            if snapshot.quota_windows.is_empty() {
                snapshot.quota_windows = previous.quota_windows.clone();
            }
        }
        cache.insert(connection_id.to_string(), snapshot.clone());
        drop(cache);
        self.advance_catalog_state_revision_locked()?;
        Ok(snapshot)
    }

    /// Reads the normalized account-usage evidence associated with one exact credential.
    /// 读取一个精确凭据关联的归一化账号用量证据。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the process-local usage observation.
    /// `connection_id` 标识进程内用量观测。
    ///
    /// # Returns
    /// 返回值
    /// The frozen usage snapshot when the cache contains one.
    /// 当缓存包含观测时返回冻结用量快照。
    pub(super) fn cached_account_usage(
        &self,
        connection_id: &str,
    ) -> Result<Option<AccountUsageSnapshot>, String> {
        self.usage_cache
            .read()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))
            .map(|cache| cache.get(connection_id).cloned())
    }

    /// Returns the complete process-local account-usage cache.
    /// 返回完整的进程内账号用量缓存。
    ///
    /// # Returns
    /// 返回值
    /// A coherent cloned cache while catalog mutation is excluded, or an explicit lock failure.
    /// 返回排除目录并发变更期间克隆的一致缓存，或显式锁失败。
    pub fn usage_cache_snapshot(&self) -> Result<BTreeMap<String, AccountUsageSnapshot>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.usage_cache_snapshot_locked()
    }

    /// Clones the account-usage cache while the caller holds the catalog mutation lock.
    /// 在调用方持有目录变更锁时克隆账号用量缓存。
    ///
    /// # Returns
    /// 返回值
    /// The exact process-local usage snapshot paired with the caller's other catalog reads.
    /// 返回与调用方其他目录读取配对的精确进程内用量快照。
    pub(super) fn usage_cache_snapshot_locked(
        &self,
    ) -> Result<BTreeMap<String, AccountUsageSnapshot>, String> {
        self.usage_cache
            .read()
            .map(|cache| cache.clone())
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))
    }

    /// Removes one stale process-local account-usage observation.
    /// 移除一个过期的进程内账号用量观测。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the exact observation to remove.
    /// `connection_id` 标识待移除的精确观测。
    ///
    /// # Returns
    /// 返回值
    /// Success after removal and revision advancement, or an activation/lock conflict.
    /// 移除并推进修订号后返回成功；激活或锁冲突时返回错误。
    pub fn remove_connection_usage(&self, connection_id: &str) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry usage removal after activation completes.",
            )
        })?;
        // mutation_guard prevents usage removal from racing a catalog-generation snapshot.
        // mutation_guard 防止用量移除与目录代际快照竞争。
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.usage_cache
            .write()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
            .remove(connection_id);
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Refreshes every configured connection that declares a supported usage query.
    /// 刷新每个声明支持用量查询的已配置连接。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` selects configured connections and their declared usage-query policy.
    /// `configuration` 选择已配置连接及其声明的用量查询策略。
    ///
    /// # Returns
    /// 返回值
    /// Success after eligible refresh attempts; invalid connection metadata returns an error.
    /// 完成符合条件的刷新尝试后返回成功；无效连接元数据返回错误。
    pub async fn refresh_supported_connection_usage(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<(), String> {
        for connection in configuration.connection_graph.connections.values() {
            if !configuration.connection_is_enabled(connection.id.as_str())
                || configuration
                    .custom_provider_manifest_is_isolated(connection.provider_id.as_str())
            {
                continue;
            }
            let (_, entry, _) =
                self.resolve_connection_metadata(configuration, connection.id.as_str())?;
            let profile = entry
                .credential_profile(connection.credential_profile_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Credential profile `{}` is not registered for service entry `{}`.",
                        connection.credential_profile_id, entry.id
                    )
                })?;
            let should_refresh = match profile.account_usage_query {
                crate::AccountUsageQueryKind::Unsupported => false,
                crate::AccountUsageQueryKind::MiniMaxTokenPlan => true,
                crate::AccountUsageQueryKind::TavilyCredits => true,
                crate::AccountUsageQueryKind::DeepSeekBalance => true,
                crate::AccountUsageQueryKind::KimiCodingPlan => true,
                crate::AccountUsageQueryKind::CommandCodeUsage => true,
                crate::AccountUsageQueryKind::OpenRouterCredits => {
                    connection.management_secret_reference.is_some()
                }
            };
            if should_refresh {
                let _ = self
                    .query_connection_usage(configuration, connection.id.as_str())
                    .await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
