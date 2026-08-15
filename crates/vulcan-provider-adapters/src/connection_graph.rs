//! Deterministic provider connection selection and mutation.
//! 确定性的供应商连接选择与变更。

use crate::{
    AccountSelectionPreferences, ConnectionModelCatalog, CredentialConnection, ProviderRegistry,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Persists user connection choices without model catalog data or secret material.
/// 持久化用户连接选择，不包含模型目录数据或秘钥材料。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PersistedConnectionGraph {
    /// Connections keyed by stable connection identifier.
    /// 按稳定连接标识索引的连接。
    #[serde(default)]
    pub connections: BTreeMap<String, CredentialConnection>,
    /// Explicit account selection preferences.
    /// 显式账号选择偏好。
    #[serde(default)]
    pub account_preferences: AccountSelectionPreferences,
}

/// Holds runtime-only model catalogs separately from persisted configuration.
/// 将仅运行时模型目录与持久化配置分离保存。
#[derive(Debug, Clone, Default)]
pub struct RuntimeConnectionCatalogs {
    /// In-memory catalogs keyed by connection identifier.
    /// 按连接标识索引的内存目录。
    catalogs: BTreeMap<String, ConnectionModelCatalog>,
}

impl RuntimeConnectionCatalogs {
    /// Restores the runtime lookup wrapper from an owned process-local snapshot.
    /// 从拥有所有权的进程内快照恢复运行时查询包装器。
    ///
    /// # Parameters
    /// 参数
    /// `catalogs` contains exact credential-scoped model catalogs.
    /// `catalogs` 包含精确的凭据作用域模型目录。
    pub fn from_snapshot(catalogs: BTreeMap<String, ConnectionModelCatalog>) -> Self {
        Self { catalogs }
    }

    /// Replaces one connection catalog after a successful refresh.
    /// 在成功刷新后替换一个连接目录。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the complete new in-memory snapshot.
    /// `catalog` 是完整的新内存快照。
    pub fn replace(&mut self, catalog: ConnectionModelCatalog) {
        self.catalogs.insert(catalog.connection_id.clone(), catalog);
    }

    /// Returns one connection catalog.
    /// 返回一个连接目录。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` is the stable connection identifier.
    /// `connection_id` 是稳定连接标识。
    ///
    /// # Returns
    /// 返回值
    /// The current process-local catalog when available.
    /// 返回当前进程内可用目录。
    pub fn get(&self, connection_id: &str) -> Option<&ConnectionModelCatalog> {
        self.catalogs.get(connection_id)
    }

    /// Removes runtime data for a deleted or materially changed connection.
    /// 删除已移除或发生实质变化连接的运行时数据。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` is the stable connection identifier.
    /// `connection_id` 是稳定连接标识。
    pub fn remove(&mut self, connection_id: &str) {
        self.catalogs.remove(connection_id);
    }

    /// Returns a complete process-local catalog snapshot.
    /// 返回完整的进程内目录快照。
    ///
    /// # Returns
    /// 返回值
    /// A cloned map that callers may serialize without holding the runtime lock.
    /// 返回可在不持有运行时锁的情况下序列化的克隆映射。
    pub fn snapshot(&self) -> BTreeMap<String, ConnectionModelCatalog> {
        self.catalogs.clone()
    }
}

impl PersistedConnectionGraph {
    /// Adds a validated connection and assigns the first provider default.
    /// 添加已校验连接，并为首个连接设置供应商默认值。
    ///
    /// # Parameters
    /// 参数
    /// `registry` is the strict provider registry and `connection` is the candidate snapshot.
    /// `registry` 是严格供应商注册表，`connection` 是候选快照。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` on an atomic accepted mutation.
    /// 原子接受变更时返回 `Ok(())`。
    pub fn add_connection(
        &mut self,
        registry: &ProviderRegistry,
        connection: CredentialConnection,
    ) -> Result<(), String> {
        validate_connection(registry, &connection)?;
        if self.connections.contains_key(connection.id.as_str()) {
            return Err(format!(
                "Connection `{}` is already configured.",
                connection.id
            ));
        }
        let provider_has_default = self
            .account_preferences
            .provider_defaults
            .contains_key(connection.provider_id.as_str());
        if !provider_has_default {
            self.account_preferences
                .provider_defaults
                .insert(connection.provider_id.clone(), connection.id.clone());
        }
        self.connections.insert(connection.id.clone(), connection);
        Ok(())
    }

    /// Resolves the explicit connection for one model without execution-time failover.
    /// 为一个模型解析显式连接，不进行执行时容灾。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `model_id` identify the target; `catalogs` contains process-local
    /// credential-scoped model visibility.
    /// `provider_id` 与 `model_id` 标识目标；`catalogs` 包含进程内凭据作用域模型可见性。
    ///
    /// # Returns
    /// 返回值
    /// The remembered account, provider default, or first stable visible connection.
    /// 返回已记忆账号、供应商默认账号或首个稳定可见连接。
    pub fn resolve_connection<'a>(
        &'a self,
        provider_id: &str,
        model_id: &str,
        catalogs: &RuntimeConnectionCatalogs,
    ) -> Result<&'a CredentialConnection, String> {
        let model_key = AccountSelectionPreferences::model_key(provider_id, model_id);
        if let Some(connection_id) = self
            .account_preferences
            .model_last_connections
            .get(model_key.as_str())
            && let Some(connection) =
                self.visible_connection(connection_id, provider_id, model_id, catalogs)
        {
            return Ok(connection);
        }
        if let Some(connection_id) = self.account_preferences.provider_defaults.get(provider_id)
            && let Some(connection) =
                self.visible_connection(connection_id, provider_id, model_id, catalogs)
        {
            return Ok(connection);
        }
        let visible = self
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .filter(|connection| {
                catalogs
                    .get(connection.id.as_str())
                    .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id))
            })
            .collect::<Vec<_>>();
        visible.first().copied().ok_or_else(|| {
            format!("No configured `{provider_id}` connection exposes model `{model_id}`.")
        })
    }

    /// Records a user-confirmed model account selection.
    /// 记录用户确认的模型账号选择。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `model_id`, and `connection_id` identify the exact selection.
    /// `provider_id`、`model_id` 与 `connection_id` 标识精确选择。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when the connection belongs to the provider.
    /// 当连接属于该供应商时返回 `Ok(())`。
    pub fn remember_model_connection(
        &mut self,
        provider_id: &str,
        model_id: &str,
        connection_id: &str,
    ) -> Result<(), String> {
        let connection = self
            .connections
            .get(connection_id)
            .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
        if connection.provider_id != provider_id {
            return Err(format!(
                "Connection `{connection_id}` does not belong to provider `{provider_id}`."
            ));
        }
        self.account_preferences.model_last_connections.insert(
            AccountSelectionPreferences::model_key(provider_id, model_id),
            connection_id.to_string(),
        );
        Ok(())
    }

    /// Finds a model-visible connection with exact provider ownership.
    /// 查找具有精确供应商归属且模型可见的连接。
    fn visible_connection<'a>(
        &'a self,
        connection_id: &str,
        provider_id: &str,
        model_id: &str,
        catalogs: &RuntimeConnectionCatalogs,
    ) -> Option<&'a CredentialConnection> {
        let connection = self.connections.get(connection_id)?;
        if connection.provider_id != provider_id {
            return None;
        }
        let catalog = catalogs.get(connection_id)?;
        catalog
            .models
            .iter()
            .any(|model| model.id == model_id)
            .then_some(connection)
    }
}

/// Validates one connection against the registered provider graph.
/// 根据已注册供应商图校验一个连接。
fn validate_connection(
    registry: &ProviderRegistry,
    connection: &CredentialConnection,
) -> Result<(), String> {
    let provider = registry
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
    let normalized_service_origin = crate::normalize_connection_service_origin_override(
        entry,
        connection.service_origin_override.as_deref(),
    )?;
    if normalized_service_origin != connection.service_origin_override {
        return Err(String::from(
            "The credential-scoped service origin is not in canonical form.",
        ));
    }
    let credential_profile = entry
        .credential_profile(connection.credential_profile_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential profile `{}` is not registered for service entry `{}`.",
                connection.credential_profile_id, entry.id
            )
        })?;
    if credential_profile.credential_mode != connection.credential_mode {
        return Err(format!(
            "Credential mode does not match credential profile `{}`.",
            credential_profile.id
        ));
    }
    if entry.kind == crate::ServiceEntryKind::ProviderTools {
        return Ok(());
    }
    // Protocol authority belongs to the active Provider Manifest, never to one credential record.
    // 协议权威属于活动 Provider Manifest，绝不属于某条凭据记录。
    if !entry.protocol_routes.iter().any(|route| route.verified) {
        return Err(format!(
            "Service entry `{}` has no verified Manifest protocol route.",
            entry.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CredentialMode, ProviderRegistry};

    /// Builds one strict OpenRouter registry.
    /// 构建一个严格 OpenRouter 注册表。
    fn registry() -> ProviderRegistry {
        let mut registry = ProviderRegistry::default();
        // descriptor is compiled from the authoritative bundled manifest instead of a Rust oracle.
        // descriptor 从权威内置 Manifest 编译，而不是来自 Rust 对照实现。
        let descriptor = crate::provider_manifest::load_bundled_resolved_catalog()
            .manifest("openrouter")
            .expect("bundled OpenRouter manifest must exist")
            .descriptor
            .clone();
        registry
            .register(descriptor)
            .expect("descriptor must register");
        registry
    }

    /// Builds one test connection.
    /// 构建一个测试连接。
    fn connection(id: &str) -> CredentialConnection {
        CredentialConnection {
            id: id.to_string(),
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: format!("credential-{id}"),
            display_name: id.to_string(),
            secret_reference: format!("secret-{id}"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        }
    }

    /// Verifies the first connection becomes the provider default.
    /// 验证首个连接成为供应商默认连接。
    #[test]
    fn first_connection_becomes_default() {
        let registry = registry();
        let mut graph = PersistedConnectionGraph::default();
        graph
            .add_connection(&registry, connection("one"))
            .expect("connection should register");
        graph
            .add_connection(&registry, connection("two"))
            .expect("connection should register");
        assert_eq!(
            graph.account_preferences.provider_defaults["openrouter"],
            "one"
        );
    }

    /// Verifies connection admission depends on Manifest routes instead of credential protocol state.
    /// 验证连接准入依赖 Manifest 路由，而不是凭据协议状态。
    #[test]
    fn system_connection_uses_manifest_protocol_authority() {
        let registry = registry();
        let mut graph = PersistedConnectionGraph::default();
        graph
            .add_connection(&registry, connection("system"))
            .expect("signed system routes should authorize the credential connection");
    }
}
