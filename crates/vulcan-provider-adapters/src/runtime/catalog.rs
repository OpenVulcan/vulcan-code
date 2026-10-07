//! Credential-scoped source discovery, typed observations, and atomic catalog staging.
//! 凭据作用域来源发现、类型化观测与原子目录暂存。

use super::catalog_policy::{
    MANUAL_MODEL_CATALOG_SCHEMA_REVISION, apply_model_catalog_overlays_with_account_access,
    manual_custom_catalog, retain_models_with_valid_limits, validate_catalog_model_count,
    validate_resolved_catalog_limits,
};
use super::catalog_sources::{
    build_manifest_identity_catalog, empty_model_source_catalog, model_discovery_is_remote,
    resolved_model_source_policy,
};
use super::catalog_views::effective_catalog_snapshot_from_generation;
use super::credentials::IsolatedCredentialSecretStore;
use super::usage::account_usage_secret_reference;
use super::{
    ProviderRuntimeActivationSnapshot, PublicAiRuntime, ResolvedProviderGeneration,
    current_unix_millis,
};
use crate::{
    ConnectionModelCatalog, CredentialConnection, CredentialSecretEnvelope,
    ModelDiscoveryFailureKind, PersistedAiConfiguration, ProviderMetadataClient,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, atomic::Ordering};

/// Identifies the closed outcome of one credential-scoped model discovery attempt.
/// 标识一次凭据作用域模型发现尝试的封闭结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogDiscoveryState {
    /// No discovery has run since process start.
    /// 进程启动后尚未执行发现。
    NotChecked,
    /// A non-empty authoritative catalog was installed.
    /// 已安装非空权威目录。
    Ready,
    /// Discovery succeeded but returned no models.
    /// 发现成功但没有返回模型。
    Empty,
    /// The endpoint explicitly does not support model discovery.
    /// 端点明确不支持模型发现。
    Unsupported,
    /// The supplied credential cannot access model discovery.
    /// 所提供凭据无法访问模型发现。
    AuthenticationFailed,
    /// The provider rate-limited model discovery.
    /// 供应商对模型发现进行了限流。
    RateLimited,
    /// A network or upstream availability failure may recover later.
    /// 网络或上游可用性故障可能稍后恢复。
    TemporarilyUnavailable,
    /// The provider returned a response that violates the selected protocol.
    /// 供应商返回了违反所选协议的响应。
    InvalidResponse,
    /// A previous successful catalog remains active after the latest failure.
    /// 最近一次失败后仍继续使用先前成功目录。
    Stale,
    /// A controlled failure did not match another closed category.
    /// 受控失败未匹配其他封闭类别。
    Failed,
}

/// Identifies the failure behind a stale catalog without changing its usability.
/// 标识过期目录背后的失败，同时不改变其可用性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogDiscoveryFailureKind {
    /// The endpoint does not implement model discovery.
    /// 端点未实现模型发现。
    Unsupported,
    /// Authentication or authorization failed.
    /// 身份认证或授权失败。
    AuthenticationFailed,
    /// The provider rejected the request due to rate limiting.
    /// 供应商因限流拒绝请求。
    RateLimited,
    /// A transient network or upstream failure occurred.
    /// 发生暂时性网络或上游故障。
    TemporarilyUnavailable,
    /// The response violated the selected discovery protocol.
    /// 响应违反所选发现协议。
    InvalidResponse,
    /// The error did not match another closed category.
    /// 错误未匹配其他封闭类别。
    Failed,
}

/// Stores one process-local model-discovery observation without credential material.
/// 保存一项不含凭据材料的进程内模型发现观测。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDiscoveryStatus {
    /// Current user-facing state.
    /// 当前面向用户的状态。
    pub state: CatalogDiscoveryState,
    /// Latest failure kind when the current state is stale.
    /// 当前状态为过期时的最近失败类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<CatalogDiscoveryFailureKind>,
    /// Controlled diagnostic message.
    /// 受控诊断消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Number of models retained for this connection.
    /// 此连接保留的模型数量。
    pub model_count: usize,
    /// Process-local catalog generation.
    /// 进程内目录代次。
    pub generation: u64,
    /// Wall-clock observation time in Unix milliseconds.
    /// Unix 毫秒墙钟观测时间。
    pub observed_at_ms: u64,
}

/// Carries one catalog observation prepared without publishing runtime or durable state.
/// 承载一项在不发布运行时或持久状态的情况下准备的目录观测。
#[derive(Debug, Clone, PartialEq)]
pub struct IsolatedConnectionCatalogObservation {
    /// Typed discovery state produced inside the isolated runtime.
    /// 在隔离运行时内产生的类型化发现状态。
    pub status: CatalogDiscoveryStatus,
    /// Raw source catalog observed before user overlays.
    /// 应用用户覆盖前观测到的原始来源目录。
    pub catalog: Option<ConnectionModelCatalog>,
    /// Controlled discovery failure retained for explicit save-policy decisions.
    /// 为显式保存策略决策保留的受控发现失败。
    pub refresh_error: Option<String>,
}

/// Identifies one validated catalog observation awaiting atomic runtime staging.
/// 标识一项等待原子运行时暂存的已验证目录观测。
#[derive(Debug, Clone, PartialEq)]
pub struct StagedConnectionCatalogObservation {
    /// Exact durable credential connection identity.
    /// 精确的持久凭据连接身份。
    pub connection_id: String,
    /// Optional raw source catalog owned by the connection.
    /// 连接持有的可选原始来源目录。
    pub catalog: Option<ConnectionModelCatalog>,
    /// Exact typed discovery state.
    /// 精确的类型化发现状态。
    pub status: CatalogDiscoveryStatus,
    /// Controlled refresh diagnostic retained for settings.
    /// 为设置保留的受控刷新诊断。
    pub refresh_error: Option<String>,
}

impl PublicAiRuntime {
    /// Replaces one credential-scoped catalog atomically.
    /// 原子替换一个凭据作用域目录。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the complete latest remote snapshot.
    /// `catalog` 是完整的最新远端快照。
    pub fn replace_catalog(&self, catalog: crate::ConnectionModelCatalog) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry the catalog replacement after activation completes.",
            )
        })?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.replace_catalog_locked(catalog)?;
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Replaces one mutable connection catalog while the caller holds the catalog mutation lock.
    /// 在调用方持有目录变更锁时替换一个可变连接目录。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the complete replacement snapshot and does not advance the state revision alone.
    /// `catalog` 是完整替换快照；本方法本身不会推进状态修订号。
    ///
    /// # Returns
    /// 返回值
    /// Success after the in-memory map owns the replacement.
    /// 内存映射持有替换值后返回成功。
    pub(super) fn replace_catalog_locked(
        &self,
        catalog: crate::ConnectionModelCatalog,
    ) -> Result<(), String> {
        self.catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .replace(catalog);
        Ok(())
    }

    /// Stages one immutable discovery source catalog for cross-crate activation tests.
    /// 为跨 crate 激活测试暂存一份不可变发现源目录。
    ///
    /// This seam deliberately does not publish an effective catalog. Tests must invoke the same
    /// explicit overlay activation used by production after staging the source evidence.
    /// 该接缝刻意不发布有效目录；测试在暂存来源证据后必须调用与生产相同的显式覆盖激活。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the exact pre-overlay discovery snapshot owned by one connection.
    /// `catalog` 是一个连接持有的精确覆盖前发现快照。
    ///
    /// # Returns
    /// 返回值
    /// Success after the staged source map owns the snapshot without changing the published generation.
    /// 暂存来源表持有该快照且不改变已发布代际后返回成功。
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn stage_source_catalog_for_testing(
        &self,
        catalog: crate::ConnectionModelCatalog,
    ) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry source staging after activation completes.",
            )
        })?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.source_catalogs
            .write()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .replace(catalog);
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Returns all model catalogs from one immutable published generation.
    /// 返回同一不可变已发布代际中的全部模型目录。
    ///
    /// # Returns
    /// 返回值
    /// A coherent cloned connection-to-catalog map that cannot expose staged discovery writes.
    /// 返回一份不会暴露暂存发现写入的一致连接到目录克隆映射。
    pub fn catalog_snapshot(&self) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        let generation = self.selection_catalog_snapshot()?;
        Ok(effective_catalog_snapshot_from_generation(&generation))
    }

    /// Returns the latest process-local synchronization errors.
    /// 返回最近的进程内同步错误。
    ///
    /// # Returns
    /// 返回值
    /// A cloned connection-to-error map.
    /// 返回克隆后的连接到错误映射。
    pub fn catalog_error_snapshot(&self) -> Result<BTreeMap<String, String>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        Ok(self
            .catalog_errors
            .read()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .clone())
    }

    /// Returns provider-discovery catalogs frozen in one published generation before overlays.
    /// 返回在一个已发布代际中、应用用户覆盖前冻结的供应商发现目录。
    ///
    /// # Returns
    /// 返回值
    /// A coherent cloned connection-to-source-catalog map that excludes unactivated candidates.
    /// 返回排除未激活候选的一致连接到源目录克隆映射。
    pub fn source_catalog_snapshot(
        &self,
    ) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        Ok(self
            .selection_catalog_snapshot()?
            .source_catalogs
            .snapshot())
    }

    /// Returns one staged source catalog solely for an isolated credential-validation transaction.
    /// 仅为隔离凭据验证事务返回一份暂存源目录。
    ///
    /// This method never changes publication state. Ordinary request, UI, and task readers must
    /// use `source_catalog_snapshot`, which is bound to the immutable published generation.
    /// 此方法绝不改变发布状态。普通请求、UI 与任务读取方必须使用绑定不可变已发布代际的
    /// `source_catalog_snapshot`。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the exact temporary or durable connection observed by discovery.
    /// `connection_id` 标识发现过程观察的精确临时或持久连接。
    ///
    /// # Returns
    /// 返回值
    /// The current staged source catalog when present, without exposing any other candidate.
    /// 当前暂存源目录存在时返回它，且不暴露任何其他候选。
    pub fn staged_source_catalog(
        &self,
        connection_id: &str,
    ) -> Result<Option<ConnectionModelCatalog>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        Ok(self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection_id)
            .cloned())
    }

    /// Atomically stages a complete observation batch when its exact before-image is still current.
    /// 仅在精确前镜像仍为当前状态时，原子暂存一批完整观测。
    ///
    /// # Parameters
    /// 参数
    /// `expected_before` is the compare side and `observations` contains distinct durable
    /// connection identities with their complete discovery facts.
    /// `expected_before` 是比较端，`observations` 包含互不重复的持久连接身份及其完整发现事实。
    ///
    /// # Returns
    /// 返回值
    /// The exact post-staging image suitable for deterministic compensation, or no write on any
    /// validation, lock, revision, or compare failure.
    /// 返回适用于确定性补偿的精确暂存后镜像；若发生任何校验、锁、修订号或比较失败则不写入。
    pub fn stage_connection_catalog_observations_if_current(
        &self,
        expected_before: &ProviderRuntimeActivationSnapshot,
        observations: &[StagedConnectionCatalogObservation],
    ) -> Result<ProviderRuntimeActivationSnapshot, String> {
        if observations.is_empty() {
            return Err(String::from(
                "A staged connection catalog observation batch cannot be empty.",
            ));
        }
        // seenConnectionIds rejects ambiguous last-write-wins behavior before any lock or write.
        // seenConnectionIds 在任何加锁或写入前拒绝含糊的后写覆盖行为。
        let mut seen_connection_ids = BTreeSet::new();
        for observation in observations {
            if observation.connection_id.trim().is_empty() {
                return Err(String::from(
                    "A staged connection catalog observation requires a non-empty connection id.",
                ));
            }
            if !seen_connection_ids.insert(observation.connection_id.as_str()) {
                return Err(format!(
                    "Staged catalog observations contain duplicate connection `{}`.",
                    observation.connection_id
                ));
            }
            if let Some(catalog) = observation.catalog.as_ref() {
                if catalog.connection_id != observation.connection_id {
                    return Err(format!(
                        "Staged catalog connection `{}` does not match observation connection `{}`.",
                        catalog.connection_id, observation.connection_id
                    ));
                }
                // Source observations retain invalid model facts for management and repair.
                // 来源观测保留无效模型事实，以供管理与修复。
                validate_catalog_model_count(catalog)?;
            }
        }
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry staging the connection observations after activation completes.",
            )
        })?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        // currentRevision is the single compare-and-swap input protecting unrelated mutations.
        // currentRevision 是保护无关变更的唯一比较交换输入。
        let current_revision = self.catalog_state_revision.load(Ordering::Acquire);
        if current_revision != expected_before.state_revision {
            return Err(format!(
                "The AI runtime changed while connection observations were prepared (expected state revision {}, observed {}); staging refused.",
                expected_before.state_revision, current_revision
            ));
        }
        // publishedRevision is proven before any candidate map is assigned.
        // publishedRevision 在分配任何候选映射前完成证明。
        let published_revision = current_revision.checked_add(1).ok_or_else(|| {
            String::from("The AI catalog state revision exhausted the supported u64 range.")
        })?;
        // Every guard is acquired before candidate construction and the first assignment.
        // 在候选构建与首次赋值前取得全部守卫。
        let mut catalogs = self
            .catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?;
        let mut source_catalogs = self
            .source_catalogs
            .write()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?;
        let activated_effective_catalogs = self
            .activated_effective_catalogs
            .read()
            .map_err(|_| String::from("The activated AI catalog lock is poisoned."))?
            .clone();
        let mut catalog_errors = self
            .catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?;
        let mut catalog_statuses = self
            .catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?;
        let usage_cache = self
            .usage_cache
            .read()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
            .clone();
        let resolved_provider_generation = self.selection_catalog_snapshot()?;
        // Candidate maps are complete before the first shared assignment.
        // 候选映射在首次共享赋值前已经完整。
        let mut next_source_catalogs = source_catalogs.clone();
        let mut next_catalogs = catalogs.clone();
        let mut next_catalog_errors = catalog_errors.clone();
        let mut next_catalog_statuses = catalog_statuses.clone();
        for observation in observations {
            if let Some(catalog) = observation.catalog.as_ref() {
                next_source_catalogs.replace(catalog.clone());
            } else {
                next_source_catalogs.remove(observation.connection_id.as_str());
            }
            // An old effective fallback must never cross a changed provider contract.
            // 旧有效回退绝不能越过已改变的供应商契约。
            next_catalogs.remove(observation.connection_id.as_str());
            if let Some(error) = observation.refresh_error.as_ref() {
                next_catalog_errors.insert(observation.connection_id.clone(), error.clone());
            } else {
                next_catalog_errors.remove(observation.connection_id.as_str());
            }
            next_catalog_statuses.insert(
                observation.connection_id.clone(),
                observation.status.clone(),
            );
        }
        *source_catalogs = next_source_catalogs.clone();
        *catalogs = next_catalogs.clone();
        *catalog_errors = next_catalog_errors.clone();
        *catalog_statuses = next_catalog_statuses.clone();
        self.catalog_state_revision
            .store(published_revision, Ordering::Release);
        Ok(ProviderRuntimeActivationSnapshot {
            state_revision: published_revision,
            catalogs: next_catalogs,
            source_catalogs: next_source_catalogs,
            activated_effective_catalogs,
            catalog_errors: next_catalog_errors,
            catalog_statuses: next_catalog_statuses,
            usage_cache,
            resolved_provider_generation,
        })
    }

    /// Returns the latest closed process-local discovery states.
    /// 返回最近的封闭进程内发现状态。
    ///
    /// # Returns
    /// 返回值
    /// A cloned connection-to-status map.
    /// 返回克隆后的连接到状态映射。
    pub fn catalog_status_snapshot(
        &self,
    ) -> Result<BTreeMap<String, CatalogDiscoveryStatus>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        Ok(self
            .catalog_statuses
            .read()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
            .clone())
    }

    /// Removes runtime-only metadata for one deleted connection.
    /// 移除一个已删除连接的仅运行时元数据。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the exact deleted connection.
    /// `connection_id` 标识被删除的精确连接。
    pub fn remove_connection_catalog(&self, connection_id: &str) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry connection removal after activation completes.",
            )
        })?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.source_catalogs
            .write()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .remove(connection_id);
        self.catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .remove(connection_id);
        self.catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .remove(connection_id);
        self.catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
            .remove(connection_id);
        self.secondary_route_cache
            .write()
            .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?
            .retain(|_, cached| cached.connection_id != connection_id);
        self.refresh_locks
            .lock()
            .map_err(|_| String::from("The AI credential refresh lock is poisoned."))?
            .remove(connection_id);
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Records one catalog refresh failure while preserving the last immutable source snapshot.
    /// 记录一次目录刷新失败，同时保留最近的不可变源快照。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` scopes the observation, `generation` and `observed_at_ms` identify the
    /// attempt, `failure_kind` is closed, and `message` is already controlled for display.
    /// `connection_id` 限定观测作用域，`generation` 与 `observed_at_ms` 标识本次尝试，
    /// `failure_kind` 为封闭分类，`message` 已经完成可展示控制。
    ///
    /// # Returns
    /// 返回值
    /// Success after both error and typed status maps are updated.
    /// 在错误与强类型状态映射均更新后返回成功。
    fn record_catalog_refresh_failure(
        &self,
        connection_id: &str,
        generation: u64,
        observed_at_ms: u64,
        failure_kind: CatalogDiscoveryFailureKind,
        message: &str,
    ) -> Result<(), String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let stale_catalog = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection_id)
            .cloned();
        self.catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .insert(connection_id.to_string(), message.to_string());
        self.catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
            .insert(
                connection_id.to_string(),
                CatalogDiscoveryStatus {
                    state: if stale_catalog.is_some() {
                        CatalogDiscoveryState::Stale
                    } else {
                        discovery_state_for_failure(failure_kind)
                    },
                    failure_kind: Some(failure_kind),
                    message: Some(message.to_string()),
                    model_count: stale_catalog
                        .as_ref()
                        .map_or(0, |catalog| catalog.models.len()),
                    generation: stale_catalog
                        .as_ref()
                        .map_or(generation, |catalog| catalog.generation),
                    observed_at_ms,
                },
            );
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Refreshes one credential-scoped model catalog from its provider.
    /// 从对应供应商刷新一个凭据作用域的模型目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the persisted public AI configuration and `connection_id` is exact.
    /// `configuration` 是持久化公共 AI 配置，`connection_id` 为精确连接标识。
    ///
    /// # Returns
    /// 返回值
    /// The newly installed process-local catalog.
    /// 返回新安装到进程内的目录。
    pub async fn refresh_connection_catalog(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
    ) -> Result<ConnectionModelCatalog, String> {
        let _refresh_guard = self.catalog_refresh_flight.lock().await;
        let before_image = self.capture_activation_snapshot()?;
        let provider_generation = Arc::clone(&before_image.resolved_provider_generation);
        let isolated_runtime = self.isolated_catalog_activation_runtime(
            &before_image,
            Arc::clone(&provider_generation.manifests),
        );
        let refresh = isolated_runtime
            .refresh_connection_catalog_from_generation(
                configuration,
                connection_id,
                provider_generation,
                true,
            )
            .await;
        let candidate = isolated_runtime.capture_activation_snapshot()?;
        if let Err(publication) =
            self.publish_isolated_catalog_candidate(&before_image, &candidate, None)
        {
            return Err(match refresh {
                Ok(_) => publication,
                Err(primary) => format!(
                    "Provider catalog refresh failed: {primary}; isolated failure-state publication also failed: {publication}"
                ),
            });
        }
        refresh
    }

    /// Observes one candidate catalog in a fully isolated runtime without publishing any state.
    /// 在完全隔离的运行时中观测一个候选目录，且不发布任何状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` and `connection_id` identify the candidate route; `temporary_secret`
    /// optionally overlays a not-yet-durable credential entirely in memory.
    /// `configuration` 与 `connection_id` 标识候选路由；`temporary_secret` 可选地在内存中
    /// 覆盖一份尚未持久化的凭据。
    ///
    /// # Returns
    /// 返回值
    /// The isolated typed status, raw source catalog, and controlled refresh diagnostic. Dropping
    /// or cancelling the future cannot leave a catalog or candidate secret in the main runtime.
    /// 返回隔离的类型化状态、原始来源目录与受控刷新诊断；丢弃或取消 Future 不会在主运行时
    /// 留下目录或候选秘密。
    pub async fn observe_connection_catalog_candidate(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        temporary_secret: Option<&CredentialSecretEnvelope>,
    ) -> Result<IsolatedConnectionCatalogObservation, String> {
        // beforeImage freezes every main-runtime input needed to construct an isolated reader.
        // beforeImage 冻结构建隔离读取方所需的每项主运行时输入。
        let before_image = self.capture_activation_snapshot()?;
        // providerGeneration remains retained for the entire remote observation.
        // providerGeneration 在整个远端观测期间保持持有。
        let provider_generation = Arc::clone(&before_image.resolved_provider_generation);
        // isolatedRuntime owns every mutable catalog projection and therefore needs no cleanup.
        // isolatedRuntime 持有每项可变目录投影，因此不需要清理。
        let mut isolated_runtime = self.isolated_catalog_activation_runtime(
            &before_image,
            Arc::clone(&provider_generation.manifests),
        );
        if let Some(secret) = temporary_secret {
            secret.validate()?;
            // connection supplies the single exact reference shadowed by the in-memory overlay.
            // connection 提供由内存覆盖层遮蔽的唯一精确引用。
            let connection = configuration
                .connection_graph
                .connections
                .get(connection_id)
                .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
            isolated_runtime.secrets = Arc::new(IsolatedCredentialSecretStore::new(
                Arc::clone(&self.secrets),
                connection.secret_reference.clone(),
                secret.clone(),
            ));
        }
        // refreshError remains data when discovery reaches a typed closed failure state.
        // refreshError 在发现到达类型化封闭失败状态时保留为数据。
        let refresh_error = isolated_runtime
            .refresh_connection_catalog_from_generation(
                configuration,
                connection_id,
                provider_generation,
                true,
            )
            .await
            .err();
        // status must be emitted by every accepted discovery outcome, including closed failures.
        // status 必须由每个可接受的发现结果产生，包括封闭失败。
        let status = isolated_runtime
            .catalog_status_snapshot()?
            .remove(connection_id)
            .ok_or_else(|| {
                refresh_error.clone().unwrap_or_else(|| {
                    String::from("Custom provider discovery did not produce a typed status.")
                })
            })?;
        // catalog is the exact raw source retained only by this isolated runtime.
        // catalog 是仅由该隔离运行时保留的精确原始来源。
        let catalog = isolated_runtime.staged_source_catalog(connection_id)?;
        Ok(IsolatedConnectionCatalogObservation {
            status,
            catalog,
            refresh_error,
        })
    }

    /// Refreshes one connection against one exact immutable provider generation.
    /// 针对一个精确不可变供应商代际刷新一个连接。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` and `connection_id` identify the account, `provider_generation` freezes
    /// all provider facts, and `publish_effective_catalog` controls writes to this runtime's mutable catalog.
    /// `configuration` 与 `connection_id` 标识账号，`provider_generation` 冻结全部供应商事实，
    /// `publish_effective_catalog` 控制是否写入当前运行时的可变目录。
    /// Request-visible publication remains owned by the surrounding isolated-candidate or activation transaction.
    /// 请求可见的发布仍由外层隔离候选事务或激活事务持有。
    ///
    /// # Returns
    /// 返回值
    /// The prepared effective catalog; this method alone never swaps the published provider generation.
    /// 返回已准备的有效目录；本方法本身绝不交换已发布供应商代际。
    pub(super) async fn refresh_connection_catalog_from_generation(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        provider_generation: Arc<ResolvedProviderGeneration>,
        publish_effective_catalog: bool,
    ) -> Result<ConnectionModelCatalog, String> {
        if !configuration.connection_is_enabled(connection_id) {
            return Err(format!(
                "Connection `{connection_id}` is disabled and cannot refresh its model catalog."
            ));
        }
        let observed_at_ms = current_unix_millis()?;
        let (provider, entry, connection) = self.resolve_connection_metadata_from_generation(
            configuration,
            connection_id,
            &provider_generation,
        )?;
        let manifest = provider_generation
            .manifests
            .manifest_arc(provider.id.as_str());
        let source_policy = resolved_model_source_policy(
            configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            manifest.as_deref(),
        )?;
        let generation = self
            .catalogs
            .read()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .get(connection_id)
            .map_or(1, |catalog| catalog.generation.saturating_add(1));
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                )
            })?;
        let account_model_access_enabled = credential_profile.account_model_access.is_some();
        // remoteDiscoveryEnabled requires a remote implementation authorized by the source policy
        // and the active Manifest route set. Credential records never own protocol authority.
        // remoteDiscoveryEnabled 要求远端实现同时获得来源策略与活动 Manifest 路由集合授权；
        // 凭据记录永远不持有协议权威。
        let remote_discovery_enabled = source_policy.includes_discovery()
            && entry
                .protocol_routes
                .iter()
                .filter(|route| crate::client::route_is_available_for_discovery(route))
                .any(|route| {
                    model_discovery_is_remote(route.resolved_model_discovery(entry.model_discovery))
                });
        if source_policy.includes_discovery()
            && !remote_discovery_enabled
            && !source_policy.includes_manual()
        {
            return Err(format!(
                "Provider `{}` service entry `{}` declares discovery identity without a remote discovery implementation.",
                provider.id, entry.id
            ));
        }
        // discoverySecret is loaded only for remote model discovery. Account usage may declare a
        // different credential role and therefore resolves independently below.
        // discoverySecret 仅为远端模型发现加载。账号用量可能声明不同的凭据角色，因此在下方独立解析。
        let discovery_secret = if remote_discovery_enabled {
            Some(
                self.resolve_fresh_execution_secret(
                    connection.id.as_str(),
                    connection.secret_reference.as_str(),
                )
                .await?,
            )
        } else {
            None
        };
        // sourceCatalog contains exactly the non-overlay source layers authorized by the policy.
        // sourceCatalog 仅包含策略授权的非覆盖来源层。
        let source_catalog = if remote_discovery_enabled {
            let discovery = self
                .metadata_client
                .discover_models(
                    &provider,
                    &entry,
                    &connection,
                    discovery_secret
                        .as_ref()
                        .expect("remote discovery must load an execution secret"),
                    manifest.clone(),
                    generation,
                )
                .await;
            match discovery {
                Ok(catalog) => catalog,
                Err(error) => {
                    let failure_kind = catalog_discovery_failure_kind(error.kind);
                    let error_message = error.message;
                    self.record_catalog_refresh_failure(
                        connection_id,
                        generation,
                        observed_at_ms,
                        failure_kind,
                        error_message.as_str(),
                    )?;
                    if publish_effective_catalog
                        && (source_policy.includes_manual()
                            || super::catalog_policy::has_system_manual_models_for_entry(
                                configuration,
                                provider.id.as_str(),
                                entry.id.as_str(),
                            ))
                    {
                        self.install_manual_custom_catalog_without_discovery(
                            configuration,
                            &connection,
                            generation,
                        )?;
                    }
                    return Err(error_message);
                }
            }
        } else if source_policy.includes_manifest() {
            build_manifest_identity_catalog(
                manifest.as_deref().ok_or_else(|| {
                    format!(
                        "Provider `{}` requires Manifest identity without an active Manifest.",
                        provider.id
                    )
                })?,
                &connection,
                generation,
            )?
        } else if source_policy.includes_manual() {
            manual_custom_catalog(configuration, &connection, generation).unwrap_or_else(|| {
                empty_model_source_catalog(
                    connection.id.as_str(),
                    generation,
                    MANUAL_MODEL_CATALOG_SCHEMA_REVISION,
                )
            })
        } else {
            return Err(format!(
                "Provider `{}` service entry `{}` has no authorized model identity source.",
                provider.id, entry.id
            ));
        };
        let source_model_count = source_catalog.models.len();
        let mut catalog = source_catalog.clone();
        if account_model_access_enabled {
            let usage_secret_reference =
                account_usage_secret_reference(&connection, credential_profile)?;
            let usage_secret = self
                .resolve_fresh_execution_secret(connection.id.as_str(), usage_secret_reference)
                .await?;
            let usage = self
                .metadata_client
                .query_account_usage(&entry, &connection, &usage_secret)
                .await;
            self.cache_usage_snapshot(connection_id, usage, true)?;
        }
        let account_usage = self.cached_account_usage(connection_id)?;
        if let Err(error) = apply_model_catalog_overlays_with_account_access(
            configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            account_usage.as_ref(),
            source_policy,
            manifest.as_deref(),
            &mut catalog,
        )
        .map(|_| retain_models_with_valid_limits(&mut catalog))
        .and_then(|_| validate_resolved_catalog_limits(&catalog))
        {
            self.record_catalog_refresh_failure(
                connection_id,
                generation,
                observed_at_ms,
                CatalogDiscoveryFailureKind::Failed,
                error.as_str(),
            )?;
            return Err(error);
        }
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.source_catalogs
            .write()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .replace(source_catalog);
        if publish_effective_catalog {
            self.replace_catalog_locked(catalog.clone())?;
        }
        self.catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .remove(connection_id);
        self.catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
            .insert(
                connection_id.to_string(),
                CatalogDiscoveryStatus {
                    state: if source_model_count == 0 {
                        CatalogDiscoveryState::Empty
                    } else {
                        CatalogDiscoveryState::Ready
                    },
                    failure_kind: None,
                    message: None,
                    model_count: source_model_count,
                    generation: catalog.generation,
                    observed_at_ms,
                },
            );
        self.advance_catalog_state_revision_locked()?;
        Ok(catalog)
    }
}

/// Maps the metadata client's typed failure into the public runtime status contract.
/// 将元数据客户端的强类型失败映射为公共运行时状态契约。
///
/// # Parameters
/// 参数
/// `failure` is produced at the exact HTTP or parser boundary.
/// `failure` 由精确 HTTP 或解析器边界产出。
///
/// # Returns
/// 返回值
/// The equivalent serializable catalog failure category.
/// 返回等价的可序列化目录失败分类。
fn catalog_discovery_failure_kind(
    failure: ModelDiscoveryFailureKind,
) -> CatalogDiscoveryFailureKind {
    match failure {
        ModelDiscoveryFailureKind::Unsupported => CatalogDiscoveryFailureKind::Unsupported,
        ModelDiscoveryFailureKind::AuthenticationFailed => {
            CatalogDiscoveryFailureKind::AuthenticationFailed
        }
        ModelDiscoveryFailureKind::RateLimited => CatalogDiscoveryFailureKind::RateLimited,
        ModelDiscoveryFailureKind::TemporarilyUnavailable => {
            CatalogDiscoveryFailureKind::TemporarilyUnavailable
        }
        ModelDiscoveryFailureKind::InvalidResponse => CatalogDiscoveryFailureKind::InvalidResponse,
        ModelDiscoveryFailureKind::Failed => CatalogDiscoveryFailureKind::Failed,
    }
}

/// Maps one failure kind to its non-stale user-facing state.
/// 将一种失败类型映射为非过期的面向用户状态。
///
/// # Parameters
/// 参数
/// `failure` is the closed discovery failure category.
/// `failure` 是封闭的发现失败分类。
///
/// # Returns
/// 返回值
/// The matching non-stale status without parsing diagnostic text.
/// 返回匹配的非过期状态，不解析诊断文本。
fn discovery_state_for_failure(failure: CatalogDiscoveryFailureKind) -> CatalogDiscoveryState {
    match failure {
        CatalogDiscoveryFailureKind::Unsupported => CatalogDiscoveryState::Unsupported,
        CatalogDiscoveryFailureKind::AuthenticationFailed => {
            CatalogDiscoveryState::AuthenticationFailed
        }
        CatalogDiscoveryFailureKind::RateLimited => CatalogDiscoveryState::RateLimited,
        CatalogDiscoveryFailureKind::TemporarilyUnavailable => {
            CatalogDiscoveryState::TemporarilyUnavailable
        }
        CatalogDiscoveryFailureKind::InvalidResponse => CatalogDiscoveryState::InvalidResponse,
        CatalogDiscoveryFailureKind::Failed => CatalogDiscoveryState::Failed,
    }
}

#[cfg(test)]
mod tests;

impl PublicAiRuntime {
    /// Rebuilds one effective catalog after an account-usage observation changes model access.
    /// 在账号用量观测改变模型访问权后重建一个有效目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies the authoritative overlays and `connection_id` identifies the
    /// exact credential whose source catalog must be reprojected.
    /// `configuration` 提供权威覆盖，`connection_id` 标识需要重新投影源目录的精确凭据。
    ///
    /// # Returns
    /// 返回值
    /// Success after the current source catalog has been atomically reprojected, or success with
    /// no mutation when discovery has not produced a source catalog yet.
    /// 当前源目录完成原子重新投影后返回成功；若发现尚未产生源目录，则成功但不修改状态。
    pub(super) fn rebuild_catalog_after_usage(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
    ) -> Result<(), String> {
        let (provider, entry, _) =
            self.resolve_connection_metadata(configuration, connection_id)?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let source_catalog = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection_id)
            .cloned();
        let Some(mut catalog) = source_catalog else {
            return Ok(());
        };
        let account_usage = self.cached_account_usage(connection_id)?;
        self.apply_model_catalog_overlays_with_account_access(
            configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            account_usage.as_ref(),
            &mut catalog,
        )?;
        retain_models_with_valid_limits(&mut catalog);
        validate_resolved_catalog_limits(&catalog)?;
        self.replace_catalog_locked(catalog)?;
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Installs validated manual models when provider discovery has no usable source catalog.
    /// 当供应商发现没有可用源目录时安装已校验的手工模型。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns custom definitions, `connection` selects one credential, and
    /// `generation` preserves the current refresh generation.
    /// `configuration` 持有自定义定义，`connection` 选择一个凭据，`generation` 保留当前刷新代次。
    ///
    /// # Returns
    /// 返回值
    /// Success when no fallback is needed or the manual-only effective catalog is installed.
    /// 无需回退或仅含手工模型的有效目录安装完成时返回成功。
    fn install_manual_custom_catalog_without_discovery(
        &self,
        configuration: &PersistedAiConfiguration,
        connection: &CredentialConnection,
        generation: u64,
    ) -> Result<(), String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let has_source_catalog = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection.id.as_str())
            .is_some();
        if has_source_catalog {
            return Ok(());
        }
        let Some(mut catalog) = manual_custom_catalog(configuration, connection, generation) else {
            return Ok(());
        };
        let account_usage = self.cached_account_usage(connection.id.as_str())?;
        self.apply_model_catalog_overlays_with_account_access(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            account_usage.as_ref(),
            &mut catalog,
        )?;
        self.replace_catalog_locked(catalog)?;
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }
}
