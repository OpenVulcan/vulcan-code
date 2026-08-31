//! Credential-scoped secondary-route discovery and bounded content-addressed caching.
//! 凭据作用域二级路由发现与有界内容寻址缓存。

use super::catalog_views::effective_catalog_snapshot_from_generation;
use super::{PublicAiRuntime, ResolvedProviderGeneration};
use crate::{
    CredentialConnection, PersistedAiConfiguration, ProviderMetadataClient, SecondaryRouteOption,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Keeps provider endpoint discovery results fresh for six hours by default.
/// 默认将供应商端点发现结果保鲜六小时。
const SECONDARY_ROUTE_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Maximum process-local secondary-route cache partitions retained across all accounts.
/// 全部账号合计保留的进程内二级路由缓存分区上限。
const SECONDARY_ROUTE_CACHE_MAX_ENTRIES: usize = 512;

/// Stores one process-local secondary-route discovery result.
/// 保存一份进程内二级路由发现结果。
#[derive(Debug, Clone)]
pub(super) struct SecondaryRouteCacheEntry {
    /// Exact credential connection owning this cache partition.
    /// 此缓存分区所属的精确凭据连接。
    pub(super) connection_id: String,
    /// Exact provider-reported route options.
    /// 供应商精确报告的路由选项。
    routes: Vec<SecondaryRouteOption>,
    /// Monotonic instant when the successful response was received.
    /// 成功收到响应时的单调时钟时间点。
    fetched_at: Instant,
}

impl PublicAiRuntime {
    /// Builds the content-addressed secondary-route cache key for one published route identity.
    /// 为一个已发布路由身份构建内容寻址的二级路由缓存键。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `connection_id`, and `model_id` select the exact account/model; every
    /// non-secret provider, service, connection, and Manifest input is frozen into the digest.
    /// `configuration`、`connection_id` 与 `model_id` 选择精确账号/模型；每项非秘密供应商、
    /// 服务、连接与 Manifest 输入都会被冻结进摘要。
    ///
    /// # Returns
    /// 返回值
    /// A stable SHA-256 cache key, resolved metadata, and the immutable generation used for lookup.
    /// 返回稳定 SHA-256 缓存键、已解析元数据以及查找使用的不可变代际。
    fn secondary_route_cache_identity(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
    ) -> Result<
        (
            String,
            crate::ProviderDescriptor,
            crate::ServiceEntry,
            CredentialConnection,
            Arc<ResolvedProviderGeneration>,
        ),
        String,
    > {
        let generation = self.selection_catalog_snapshot()?;
        let (provider, entry, connection) = self.resolve_connection_metadata_from_generation(
            configuration,
            connection_id,
            &generation,
        )?;
        // identityBytes contain no credential value; secret_reference is an opaque host-owned key.
        // identityBytes 不包含凭据值；secret_reference 是宿主持有的不透明键。
        let identity_bytes = serde_json::to_vec(&(
            generation.manifests.content_sha256.as_str(),
            &provider,
            &entry,
            &connection,
            model_id,
        ))
        .map_err(|error| format!("Failed to serialize secondary-route cache identity: {error}"))?;
        let cache_key = hex::encode(Sha256::digest(identity_bytes));
        Ok((cache_key, provider, entry, connection, generation))
    }

    /// Inserts one secondary-route observation with global expiry cleanup and bounded eviction.
    /// 通过全局过期清理与有界淘汰插入一项二级路由观测。
    ///
    /// # Parameters
    /// 参数
    /// `cache_key` is the content-addressed route identity; `connection_id` owns the partition;
    /// `routes` is the exact provider response; and `fetched_at` is its monotonic observation time.
    /// `cache_key` 是内容寻址路由身份；`connection_id` 持有该分区；`routes` 是精确供应商响应；
    /// `fetched_at` 是其单调观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Success after all expired entries are removed and the cache remains within its hard limit.
    /// 移除全部过期项且缓存保持在硬上限内后返回成功。
    fn insert_secondary_route_cache_entry(
        &self,
        cache_key: String,
        connection_id: &str,
        routes: Vec<SecondaryRouteOption>,
        fetched_at: Instant,
    ) -> Result<(), String> {
        let mut cache = self
            .secondary_route_cache
            .write()
            .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?;
        // Expired entries are removed globally on every successful insertion. If the remaining
        // cache is full, stable fetched-at/key ordering evicts exactly one oldest partition.
        // 每次成功插入都会全局移除过期项；若剩余缓存已满，则按 fetched-at/key 稳定顺序
        // 精确淘汰一个最旧分区。
        cache.retain(|_, cached| cached.fetched_at.elapsed() < SECONDARY_ROUTE_CACHE_TTL);
        if !cache.contains_key(cache_key.as_str())
            && cache.len() >= SECONDARY_ROUTE_CACHE_MAX_ENTRIES
            && let Some(oldest_key) = cache
                .iter()
                .min_by(|(left_key, left), (right_key, right)| {
                    left.fetched_at
                        .cmp(&right.fetched_at)
                        .then_with(|| left_key.cmp(right_key))
                })
                .map(|(key, _)| key.clone())
        {
            cache.remove(oldest_key.as_str());
        }
        cache.insert(
            cache_key,
            SecondaryRouteCacheEntry {
                connection_id: connection_id.to_string(),
                routes,
                fetched_at,
            },
        );
        Ok(())
    }

    /// Discovers secondary routes for one exact connection and model.
    /// 发现一个精确连接与模型可用的二级路由。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the authoritative connection graph, `connection_id` selects one
    /// credential, `model_id` identifies the exact provider model, and `force_refresh` bypasses
    /// the six-hour process cache.
    /// `configuration` 是权威连接图，`connection_id` 选择一个凭据，
    /// `model_id` 标识精确供应商模型，`force_refresh` 绕过六小时进程缓存。
    ///
    /// # Returns
    /// 返回值
    /// Provider-reported route options without persisting metadata locally.
    /// 返回供应商报告的路由选项，不在本地持久化元数据。
    pub async fn discover_secondary_routes(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
        force_refresh: bool,
    ) -> Result<Vec<crate::SecondaryRouteOption>, String> {
        let (cache_key, provider, entry, connection, generation) =
            self.secondary_route_cache_identity(configuration, connection_id, model_id)?;
        let catalog = effective_catalog_snapshot_from_generation(&generation)
            .get(connection_id)
            .cloned()
            .ok_or_else(|| {
                format!("Connection `{connection_id}` has no published model catalog.")
            })?;
        if !catalog.models.iter().any(|model| model.id == model_id) {
            return Err(format!(
                "Model `{model_id}` is not present in connection `{connection_id}`."
            ));
        }
        if !force_refresh {
            let cached_routes = self
                .secondary_route_cache
                .read()
                .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?
                .get(cache_key.as_str())
                .filter(|cached| cached.fetched_at.elapsed() < SECONDARY_ROUTE_CACHE_TTL)
                .map(|cached| cached.routes.clone());
            if let Some(cached_routes) = cached_routes {
                return Ok(cached_routes);
            }
        }
        let secret = self
            .resolve_fresh_execution_secret(
                connection.id.as_str(),
                connection.secret_reference.as_str(),
            )
            .await?;
        let routes = self
            .metadata_client
            .discover_secondary_routes(&provider, &entry, &connection, model_id, &secret)
            .await?;
        self.insert_secondary_route_cache_entry(
            cache_key,
            connection_id,
            routes.clone(),
            Instant::now(),
        )?;
        Ok(routes)
    }

    /// Returns the last successful secondary-route discovery without any provider request.
    /// 在不发起任何供应商请求的情况下返回最近一次成功的二级路由发现结果。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the connection graph; `connection_id` and `model_id` identify
    /// the exact cache partition under the captured manifest generation.
    /// `configuration` 持有连接图；`connection_id` 与 `model_id` 标识已捕获清单代际下
    /// 的精确缓存分区。
    ///
    /// # Returns
    /// 返回值
    /// Fresh route options for the exact current route identity, or an explicit cache-miss error.
    /// 返回精确当前路由身份的新鲜路由选项，或显式的缓存未命中错误。
    pub fn cached_secondary_routes(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
    ) -> Result<Vec<SecondaryRouteOption>, String> {
        // cache_key includes the exact current provider generation and non-secret route inputs.
        // cache_key 包含精确当前供应商代际与非秘密路由输入。
        let (cache_key, _, _, _, _) =
            self.secondary_route_cache_identity(configuration, connection_id, model_id)?;
        self.secondary_route_cache
            .read()
            .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?
            .get(cache_key.as_str())
            .filter(|cached| cached.fetched_at.elapsed() < SECONDARY_ROUTE_CACHE_TTL)
            .map(|cached| cached.routes.clone())
            .ok_or_else(|| {
                format!(
                    "No cached secondary routes exist for connection `{connection_id}` and model `{model_id}`. Open or refresh the secondary provider list first."
                )
            })
    }
}

#[cfg(test)]
mod tests;
