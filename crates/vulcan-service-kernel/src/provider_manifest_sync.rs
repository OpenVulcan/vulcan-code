//! Production wiring for the provider-manifest store and signed remote sync (plan §17.1-17.6).
//! 供应商清单存储与签名远端同步的生产接线（方案 §17.1-17.6）。
//!
//! The startup recovery chain restores persisted active/previous layers into the runtime's
//! single authoritative catalog; the sync service runs the full signed remote flow and hot
//! replaces the runtime through the same catalog. Remote endpoints and trusted signing keys
//! stay behind the external confirmation gate: an unconfigured sync service reports
//! `not_configured` instead of guessing an endpoint (plan §7.2).
//! 启动恢复链把持久化 active/previous 层恢复到运行时的唯一权威目录；同步服务执行完整
//! 签名远端流程并通过同一目录热替换运行时。远端端点与受信签名密钥保持在外部门之后：
//! 未配置的同步服务报告 `not_configured` 而不是猜测端点（方案 §7.2）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use vulcan_provider_adapters::{
    PersistedAiConfiguration, ProviderOperationalTelemetryEvent,
    ProviderOperationalTelemetryEventName, PublicAiRuntime,
    provider_operational_telemetry_snapshot, record_provider_operational_telemetry,
};
use vulcan_provider_config::manifest_store::{
    CheckStateLoad, ManifestCheckOutcome, ManifestCheckState, ManifestStoreLayout, SyncStateLoad,
};
use vulcan_provider_config::manifest_sync::{
    REMOTE_INDEX_MAX_BYTES, REMOTE_PROVIDER_MAX_BYTES, TrustedSigningKey, activate_synced_provider,
    resolve_remote_provider_url, stage_synced_provider_activation, verify_and_parse_index,
    verify_provider_bytes, verify_stored_system_manifest,
};
use vulcan_service_api::ServiceEventEmitter;

use crate::SharedServiceBridgeState;

/// Connect timeout for one remote manifest sync request.
/// 一次远端清单同步请求的连接超时。
const SYNC_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Total request timeout for one remote manifest sync request.
/// 一次远端清单同步请求的总请求超时。
const SYNC_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Accepted media type for every remote index/provider response.
/// 每个远端 Index/供应商响应接受的媒体类型。
const SYNC_ACCEPTED_CONTENT_TYPE: &str = "application/json";
/// Header advertising the exact client release requesting a compatible manifest generation.
/// 声明请求兼容 Manifest 代际的精确客户端发行版本请求头。
const SYNC_CLIENT_VERSION_HEADER: &str = "x-vulcan-client-version";
/// Header advertising the only provider-manifest schema understood by this client.
/// 声明当前客户端唯一支持的 Provider Manifest Schema 请求头。
const SYNC_MANIFEST_SCHEMA_HEADER: &str = "x-vulcan-manifest-schema";
/// Header advertising the maximum provider-manifest engine understood by this client.
/// 声明当前客户端支持的最高 Provider Manifest 引擎版本请求头。
const SYNC_MANIFEST_ENGINE_HEADER: &str = "x-vulcan-manifest-engine";

/// Reports whether one HTTP Content-Type value declares the exact JSON media type.
/// 判断一个 HTTP Content-Type 值是否声明精确的 JSON 媒体类型。
///
/// Parameters such as `charset=utf-8` are allowed, while prefix lookalikes such as
/// `application/jsonp` are rejected before any response bytes are buffered.
/// 允许 `charset=utf-8` 等参数，但 `application/jsonp` 等前缀仿冒值会在缓冲响应体前被拒绝。
fn is_supported_sync_content_type(value: &str) -> bool {
    value
        .split_once(';')
        .map_or(value, |(media_type, _parameters)| media_type)
        .trim()
        .eq_ignore_ascii_case(SYNC_ACCEPTED_CONTENT_TYPE)
}

/// Produces one credential- and query-free remote URL label for diagnostics.
/// 生成一条用于诊断且不含凭据与查询参数的远端 URL 标签。
///
/// # Parameters
/// 参数
/// `value` is an untrusted configured or response URL that may contain signed query material.
/// `value` 是可能包含签名查询材料的不可信配置或响应 URL。
///
/// # Returns
/// 返回值
/// A parsed URL with user information, query, and fragment removed, or one opaque marker when
/// parsing itself fails. Callers must never format the original value into an error.
/// 返回移除了用户信息、查询与片段的已解析 URL；解析失败时返回不透明标记。调用方不得把
/// 原始值格式化进错误。
fn redacted_sync_url(value: &str) -> String {
    let Ok(mut parsed) = reqwest::Url::parse(value) else {
        return String::from("<invalid-remote-url>");
    };
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}
/// Stable interval after one successful periodic synchronization.
/// 周期同步成功后的稳定间隔。
const PERIODIC_SYNC_SUCCESS_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Initial retry interval after one periodic synchronization failure.
/// 周期同步失败后的初始重试间隔。
const PERIODIC_SYNC_FAILURE_BASE: Duration = Duration::from_secs(60);
/// Maximum retry interval after repeated periodic synchronization failures.
/// 周期同步连续失败后的最大重试间隔。
const PERIODIC_SYNC_FAILURE_MAX: Duration = Duration::from_secs(60 * 60);
/// Symmetric percentage bound applied to periodic synchronization jitter.
/// 应用于周期同步抖动的对称百分比上限。
const PERIODIC_SYNC_JITTER_PERCENT: u64 = 10;
/// Maximum concrete model bindings returned in one provider status snapshot.
/// 单份供应商状态快照返回的具体模型绑定最大数量。
const MANIFEST_STATUS_MODEL_BINDING_MAX_ITEMS: usize = 8_192;
/// Maximum matching model identities summarized by one operation status.
/// 单条 operation 状态汇总的匹配模型身份最大数量。
const MANIFEST_STATUS_OPERATION_MODEL_MAX_ITEMS: usize = 64;
/// Maximum operation deltas returned by one candidate preview.
/// 单次候选预览返回的 operation 差异最大数量。
const MANIFEST_PREVIEW_OPERATION_CHANGE_MAX_ITEMS: usize = 1_024;
/// Maximum UTF-8 bytes returned for one candidate verification diagnostic.
/// 单条候选校验诊断返回的最大 UTF-8 字节数。
const MANIFEST_PREVIEW_DIAGNOSTIC_MAX_BYTES: usize = 2_048;
/// Maximum recent structured operational events exposed per provider status snapshot.
/// 每份供应商状态快照公开的最近结构化运行事件最大数量。
const MANIFEST_STATUS_TELEMETRY_MAX_ITEMS: usize = 64;

/// Drop-safe index-check telemetry guard that closes every early error path.
/// 支持安全丢弃的 Index 检查遥测守卫，用于闭合每条提前错误路径。
struct ManifestIndexCheckTelemetryGuard {
    /// Exact providers whose check has not emitted a terminal completion event yet.
    /// 尚未发出终态完成事件的精确供应商。
    pending_provider_ids: BTreeSet<String>,
    /// Stable failure kind used only for providers still pending when the guard drops.
    /// guard 释放时仅用于仍待处理供应商的稳定失败分类。
    pending_failure_kind: &'static str,
}

impl ManifestIndexCheckTelemetryGuard {
    /// Starts one provider-scoped event for every exact index-check target.
    /// 为每个精确 Index 检查目标启动一条供应商范围事件。
    ///
    /// # Parameters
    /// 参数
    /// `provider_ids` are the exact system providers covered by one network check.
    /// `provider_ids` 是一次网络检查覆盖的精确系统供应商。
    ///
    /// # Returns
    /// 返回值
    /// An armed guard that emits failed and completed events if an early return drops it.
    /// 返回一个已启用守卫；提前返回将使其发出失败与完成事件。
    fn start(provider_ids: Vec<String>) -> Self {
        for provider_id in &provider_ids {
            emit_manifest_index_event(
                ProviderOperationalTelemetryEventName::ManifestIndexCheckStarted,
                provider_id,
                None,
            );
        }
        Self {
            pending_provider_ids: provider_ids.into_iter().collect(),
            pending_failure_kind: "index_check_failed",
        }
    }

    /// Marks the shared signed Index as resolved before provider-scoped work begins.
    /// 在逐供应商工作开始前把共享签名 Index 标记为已解析。
    fn mark_index_resolved(&mut self) {
        self.pending_failure_kind = "provider_check_aborted";
    }

    /// Records that one provider reused a revalidated local active layer after HTTP 304.
    /// 记录一个供应商在 HTTP 304 后复用了重新校验的本地 active 层。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact provider whose conditional index result was not modified.
    /// `provider_id` 是条件式 Index 结果未修改的精确供应商。
    fn not_modified(&self, provider_id: &str) {
        emit_manifest_index_event(
            ProviderOperationalTelemetryEventName::ManifestIndexCheckNotModified,
            provider_id,
            None,
        );
    }

    /// Completes every provider check from the exact closed sync reports.
    /// 根据精确封闭同步报告完成每个供应商检查。
    ///
    /// # Parameters
    /// 参数
    /// `reports` contain one terminal provider-scoped outcome each.
    /// `reports` 每条包含一个供应商范围终态结果。
    fn complete(&mut self, reports: &[ProviderSyncReport]) {
        for report in reports {
            self.complete_provider(report);
        }
    }

    /// Completes one provider without changing the terminal state of any later provider.
    /// 完成一个供应商，且不改变任何后续供应商的终态。
    ///
    /// # Parameters
    /// 参数
    /// `report` is the exact closed provider-scoped synchronization result.
    /// `report` 是精确封闭的逐供应商同步结果。
    fn complete_provider(&mut self, report: &ProviderSyncReport) {
        if self
            .pending_provider_ids
            .remove(report.provider_id.as_str())
        {
            emit_manifest_index_event(
                ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted,
                report.provider_id.as_str(),
                report.failure_kind.as_deref(),
            );
        }
    }

    /// Fails one provider explicitly while leaving all unprocessed providers pending.
    /// 显式终止一个供应商，同时让全部未处理供应商保持待处理。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is exact and `failure_kind` is a stable non-secret category.
    /// `provider_id` 为精确身份，`failure_kind` 是稳定且非秘密的分类。
    fn fail_provider(&mut self, provider_id: &str, failure_kind: &'static str) {
        if self.pending_provider_ids.remove(provider_id) {
            emit_manifest_index_event(
                ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed,
                provider_id,
                Some(failure_kind),
            );
            emit_manifest_index_event(
                ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted,
                provider_id,
                Some(failure_kind),
            );
        }
    }
}

impl Drop for ManifestIndexCheckTelemetryGuard {
    /// Emits a typed failure and matching completion for every unclosed provider check.
    /// 为每个未闭合供应商检查发出类型化失败及匹配完成事件。
    fn drop(&mut self) {
        for provider_id in &self.pending_provider_ids {
            emit_manifest_index_event(
                ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed,
                provider_id,
                Some(self.pending_failure_kind),
            );
            emit_manifest_index_event(
                ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted,
                provider_id,
                Some(self.pending_failure_kind),
            );
        }
    }
}

/// Emits one provider-scoped index event without retaining the configured index URL.
/// 发出一条供应商范围 Index 事件，且不保留已配置 Index URL。
///
/// # Parameters
/// 参数
/// `event_name` is a closed index lifecycle point; `provider_id` is exact and
/// `failure_kind` is a stable non-secret category.
/// `event_name` 是封闭 Index 生命周期点；`provider_id` 是精确标识，`failure_kind` 是稳定
/// 非秘密分类。
fn emit_manifest_index_event(
    event_name: ProviderOperationalTelemetryEventName,
    provider_id: &str,
    failure_kind: Option<&str>,
) {
    let mut event = ProviderOperationalTelemetryEvent::new(event_name);
    event.provider_id = Some(provider_id.to_string());
    event.failure_kind = failure_kind.map(str::to_string);
    emit_manifest_operational_event(event);
}

/// Emits one authenticated candidate lifecycle event without bytes, URLs, or signatures.
/// 发出一条不含字节、URL 或签名的认证候选生命周期事件。
///
/// # Parameters
/// 参数
/// `event_name` is a closed candidate point; provider, revision, and digest come from the signed
/// index; `failure_kind` and compiler-owned `failure_path` are present only for rejection.
/// `event_name` 是封闭候选点；供应商、修订与摘要来自签名 Index；`failure_kind` 与编译器
/// 权威 `failure_path` 仅拒绝时存在。
fn emit_provider_candidate_event(
    event_name: ProviderOperationalTelemetryEventName,
    provider_id: &str,
    revision: u64,
    sha256: &str,
    failure_kind: Option<&str>,
    failure_path: Option<&str>,
) {
    let mut event = ProviderOperationalTelemetryEvent::new(event_name);
    event.provider_id = Some(provider_id.to_string());
    event.manifest_revision = Some(revision);
    event.manifest_sha256 = Some(sha256.to_string());
    event.failure_kind = failure_kind.map(str::to_string);
    event.failure_path = failure_path.map(str::to_string);
    emit_manifest_operational_event(event);
}

/// Closed terminal state for one post-staging manifest activation transaction.
/// 单个候选暂存后 Manifest 激活事务的封闭终态。
enum ManifestActivationTelemetryTerminal<'a> {
    /// Every file, runtime, configuration, and Workbench projection committed.
    /// 每个文件、运行时、配置与工作台投影均已提交。
    Activated {
        /// Exact active source layer.
        /// 精确活动来源层。
        source: &'a str,
        /// Optional authenticated active digest.
        /// 可选认证活动摘要。
        sha256: Option<&'a str>,
    },
    /// Every before-image was restored after a post-activation failure.
    /// 激活后失败已恢复每个前镜像。
    RolledBack {
        /// Stable typed publication failure category.
        /// 稳定类型化发布失败分类。
        failure_kind: &'a str,
    },
}

/// Builds exactly one mutually exclusive activation terminal event.
/// 构建恰好一个相互排斥的激活终态事件。
///
/// # Parameters
/// 参数
/// `provider_id`, `revision`, and `generation` are exact transaction identities; `terminal`
/// selects either committed activation or compensated rollback.
/// `provider_id`、`revision` 与 `generation` 是精确事务身份；`terminal` 只能选择已提交激活
/// 或已补偿回滚之一。
///
/// # Returns
/// 返回值
/// One structurally valid event whose name cannot represent both terminal states.
/// 返回一条结构有效事件，其名称不可能同时表示两个终态。
fn manifest_activation_terminal_event(
    provider_id: &str,
    revision: u64,
    generation: u64,
    terminal: ManifestActivationTelemetryTerminal<'_>,
) -> ProviderOperationalTelemetryEvent {
    let mut event = match terminal {
        ManifestActivationTelemetryTerminal::Activated { source, sha256 } => {
            let mut event = ProviderOperationalTelemetryEvent::new(
                ProviderOperationalTelemetryEventName::ProviderManifestActivated,
            );
            event.manifest_source = Some(source.to_string());
            event.manifest_sha256 = sha256.map(str::to_string);
            event
        }
        ManifestActivationTelemetryTerminal::RolledBack { failure_kind } => {
            let mut event = ProviderOperationalTelemetryEvent::new(
                ProviderOperationalTelemetryEventName::ProviderManifestPostActivationRolledBack,
            );
            event.failure_kind = Some(failure_kind.to_string());
            event
        }
    };
    event.provider_id = Some(provider_id.to_string());
    event.manifest_revision = Some(revision);
    event.manifest_generation = Some(generation);
    event
}

/// Returns the stable source-layer label shared by telemetry and status contracts.
/// 返回遥测与状态契约共享的稳定来源层标签。
///
/// # Parameters
/// 参数
/// `source` is the exact compiled Manifest source layer.
/// `source` 是精确编译 Manifest 来源层。
///
/// # Returns
/// 返回值
/// One closed lowercase source identifier.
/// 返回一个封闭小写来源标识。
fn manifest_source_layer_label(
    source: vulcan_provider_adapters::ManifestSourceLayer,
) -> &'static str {
    match source {
        vulcan_provider_adapters::ManifestSourceLayer::Bundled => "bundled",
        vulcan_provider_adapters::ManifestSourceLayer::Active => "active",
        vulcan_provider_adapters::ManifestSourceLayer::Custom => "custom",
        vulcan_provider_adapters::ManifestSourceLayer::Previous => "previous",
    }
}

/// Emits one trusted fallback selection without retaining invalid candidate bytes or paths.
/// 发出一次可信回退选择，且不保留无效候选字节或路径。
///
/// # Parameters
/// 参数
/// Provider, revision, generation, source, and digest identify the selected trusted layer;
/// `failure_kind` explains why a newer layer was not used.
/// 供应商、修订、代际、来源与摘要标识选中的可信层；`failure_kind` 解释未使用更新层的原因。
fn emit_manifest_fallback_event(
    provider_id: &str,
    revision: u64,
    generation: u64,
    source: &str,
    sha256: Option<&str>,
    failure_kind: &str,
) {
    let mut event = ProviderOperationalTelemetryEvent::new(
        ProviderOperationalTelemetryEventName::ProviderManifestFallbackSelected,
    );
    event.provider_id = Some(provider_id.to_string());
    event.manifest_revision = Some(revision);
    event.manifest_generation = Some(generation);
    event.manifest_source = Some(source.to_string());
    event.manifest_sha256 = sha256.map(str::to_string);
    event.failure_kind = Some(failure_kind.to_string());
    emit_manifest_operational_event(event);
}

/// Emits one provider-local isolation when no trusted executable layer remains at startup.
/// 在启动时没有剩余可信可执行层时发出一条逐供应商隔离事件。
///
/// # Parameters
/// 参数
/// `provider_id` is the isolated identity, `generation` is the still-running catalog generation,
/// and `failure_kind` is the closed reason that prevented recovery.
/// `provider_id` 是被隔离身份，`generation` 是仍在运行的目录代次，`failure_kind` 是阻止
/// 恢复的封闭原因。
fn emit_manifest_isolated_event(provider_id: &str, generation: u64, failure_kind: &str) {
    let mut event = ProviderOperationalTelemetryEvent::new(
        ProviderOperationalTelemetryEventName::ProviderManifestIsolated,
    );
    event.provider_id = Some(provider_id.to_string());
    event.manifest_generation = Some(generation);
    event.manifest_source = Some(String::from("isolated"));
    event.failure_kind = Some(failure_kind.to_string());
    emit_manifest_operational_event(event);
}

/// Records one internally constructed manifest event without changing synchronization outcome.
/// 记录一条内部构建的 Manifest 事件，且不改变同步结果。
///
/// # Parameters
/// 参数
/// `event` contains only authoritative, bounded, non-secret identities.
/// `event` 仅包含权威、有界、非秘密身份。
fn emit_manifest_operational_event(event: ProviderOperationalTelemetryEvent) {
    if let Err(error) = record_provider_operational_telemetry(event) {
        eprintln!("[provider-manifest-sync] structured telemetry was rejected: {error}");
    }
}

/// Starts asynchronous startup synchronization and its periodic retry loop when configured.
/// 在已配置时启动异步启动同步及其周期重试循环。
///
/// The first attempt runs asynchronously without delaying service readiness. Later attempts use
/// a stable success interval or bounded exponential failure backoff with symmetric jitter. The
/// exact task is registered on the injected service so application shutdown can cancel it.
/// 第一次尝试异步执行，不延迟服务就绪。后续尝试使用稳定成功间隔，或带对称抖动的有界
/// 指数失败退避。精确任务注册到注入服务，使应用关闭时可以取消。
///
/// # Parameters
/// 参数
/// `state` owns the injected sync service and public AI runtime; `app` publishes Workbench changes.
/// `state` 持有注入的同步服务与公共 AI 运行时；`app` 发布 Workbench 变化。
///
/// # Returns
/// 返回值
/// `true` when a task was started, `false` when external URL/key configuration is absent.
/// 启动任务时返回 `true`；外部 URL/密钥配置缺失时返回 `false`。
pub(crate) fn start_provider_manifest_sync_monitor(
    state: SharedServiceBridgeState,
    app: ServiceEventEmitter,
) -> Result<bool, String> {
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock provider manifest sync state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    if !service.is_configured() {
        return Ok(false);
    }
    let weak_state = Arc::downgrade(&state);
    let handle = tokio::spawn(async move {
        let mut consecutive_failures = 0_u32;
        loop {
            let Some(state) = weak_state.upgrade() else {
                break;
            };
            let result =
                crate::public_ai_settings::service_sync_provider_manifests(app.clone(), state)
                    .await;
            match result {
                Ok(reports) if reports.iter().all(|report| report.outcome != "rejected") => {
                    consecutive_failures = 0;
                }
                Ok(reports) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    let rejected = reports
                        .iter()
                        .filter(|report| report.outcome == "rejected")
                        .map(|report| report.provider_id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    eprintln!(
                        "[provider-manifest-sync] periodic synchronization rejected providers: {rejected}"
                    );
                }
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    eprintln!("[provider-manifest-sync] periodic synchronization failed: {error}");
                }
            }
            let delay = periodic_sync_delay(
                consecutive_failures,
                current_unix_millis() ^ u64::from(consecutive_failures),
            );
            tokio::time::sleep(delay).await;
        }
    });
    service.install_periodic_task(handle.abort_handle())?;
    drop(handle);
    Ok(true)
}

/// Calculates one bounded periodic synchronization delay.
/// 计算一次有界周期同步延迟。
///
/// # Parameters
/// 参数
/// `consecutive_failures` selects success cadence or exponential retry; `jitter_seed` selects
/// one deterministic point inside the symmetric jitter range.
/// `consecutive_failures` 选择成功周期或指数重试；`jitter_seed` 在对称抖动范围内选择一个
/// 确定性位置。
///
/// # Returns
/// 返回值
/// A delay inside ±10% of the bounded base interval.
/// 位于有界基础间隔 ±10% 内的延迟。
fn periodic_sync_delay(consecutive_failures: u32, jitter_seed: u64) -> Duration {
    let base_seconds = if consecutive_failures == 0 {
        PERIODIC_SYNC_SUCCESS_INTERVAL.as_secs()
    } else {
        let exponent = consecutive_failures.saturating_sub(1).min(6);
        PERIODIC_SYNC_FAILURE_BASE
            .as_secs()
            .saturating_mul(1_u64 << exponent)
            .min(PERIODIC_SYNC_FAILURE_MAX.as_secs())
    };
    let jitter_span = base_seconds.saturating_mul(PERIODIC_SYNC_JITTER_PERCENT) / 100;
    let jitter_width = jitter_span.saturating_mul(2).saturating_add(1);
    let jitter_offset = jitter_seed % jitter_width;
    Duration::from_secs(
        base_seconds
            .saturating_sub(jitter_span)
            .saturating_add(jitter_offset),
    )
}

/// Builds the dedicated secure HTTP client for manifest sync (plan §R7.4).
/// 构建清单同步专用安全 HTTP 客户端（方案 §R7.4）。
///
/// Redirects are disabled so a remote answer can never deliver bytes from a second origin;
/// connect and total request timeouts bound every network read.
/// 禁用重定向，使远端响应绝不可能从第二个源交付字节；连接与总请求超时约束每次网络读取。
pub(crate) fn build_sync_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(SYNC_CONNECT_TIMEOUT)
        .timeout(SYNC_REQUEST_TIMEOUT)
        .build()
        .map_err(|error| format!("Failed to build the manifest sync HTTP client: {error}"))
}

/// One startup restore report for one system provider.
/// 一个系统供应商的一次启动恢复报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemManifestRestoreReport {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Restored manifest revision.
    /// 恢复的清单修订号。
    pub revision: u64,
    /// Restored layer identity: `active`, `previous`, or `bundled`.
    /// 恢复层级标识：`active`、`previous` 或 `bundled`。
    pub layer: &'static str,
    /// Provider-local reason for a Previous fallback, absent for a normal Active restore.
    /// 回退到 Previous 的逐供应商原因；正常 Active 恢复时不存在。
    pub fallback_failure_kind: Option<String>,
}

/// Removes every untrusted persisted layer before discarding one unreadable transaction journal.
/// 在丢弃一份无法读取的事务日志前移除全部不可信持久化层。
///
/// # Parameters
/// 参数
/// `store` owns the provider files and `provider_id` is one trusted bundled identity.
/// `store` 持有供应商文件，`provider_id` 是一个可信内置身份。
///
/// # Returns
/// 返回值
/// Success only when Active, Previous, sync/check state, and the unreadable journal are absent.
/// 仅当 Active、Previous、同步/检查状态与不可读日志全部不存在时返回成功。
fn discard_unrecoverable_system_manifest_transaction(
    store: &ManifestStoreLayout,
    provider_id: &str,
) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Err(error) = store.remove_active(provider_id) {
        failures.push(format!("active cleanup failed: {error}"));
    }
    if let Err(error) = store.remove_previous(provider_id) {
        failures.push(format!("previous cleanup failed: {error}"));
    }
    if let Err(error) = store.remove_sync_state(provider_id) {
        failures.push(format!("sync-state cleanup failed: {error}"));
    }
    if let Err(error) = store.remove_check_state(provider_id) {
        failures.push(format!("check-state cleanup failed: {error}"));
    }
    if failures.is_empty() {
        store.discard_system_transaction_journal(provider_id)
    } else {
        Err(failures.join("; "))
    }
}

/// Restores every persisted system manifest from the host store into the runtime catalog.
/// 从宿主存储把全部持久化系统清单恢复到运行时目录。
///
/// The store's validated active layer wins; an invalid active layer is quarantined and the
/// previous layer is rolled back and restored; providers without usable persisted layers
/// keep the bundled baseline (plan §17.5).
/// 存储中已校验的 active 层优先；无效 active 层被隔离并回滚恢复 previous 层；
/// 无可使用持久化层的供应商保持内置基线（方案 §17.5）。
///
/// # Parameters
/// 参数
/// `runtime` owns the single authoritative catalog and `app_data_root` is the host-resolved
/// application data directory.
/// `runtime` 持有唯一权威目录，`app_data_root` 是宿主解析的应用数据目录。
///
/// # Returns
/// 返回值
/// One report per provider whose persisted layer replaced the bundled baseline.
/// 每个以持久化层替换内置基线的供应商返回一条报告。
pub fn restore_system_manifests_from_store(
    runtime: &PublicAiRuntime,
    app_data_root: &Path,
) -> Result<Vec<SystemManifestRestoreReport>, String> {
    let store = ManifestStoreLayout::new(app_data_root);
    let provider_ids = runtime.system_manifest_provider_ids()?;
    let mut reports = Vec::new();
    for provider_id in provider_ids {
        let bundled_revision = runtime
            .system_manifest(&provider_id)
            .map(|manifest| manifest.revision)
            .unwrap_or(0);
        if store.recover_system_transaction(&provider_id).is_err() {
            // cleanupResult removes every possibly half-rotated persisted authority before the
            // unreadable journal is discarded; the provider then remains on its independent
            // bundled baseline.
            // cleanupResult 在丢弃不可读日志前移除每份可能只旋转一半的持久化权威；随后该
            // 供应商保留其独立内置基线。
            let cleanup_result =
                discard_unrecoverable_system_manifest_transaction(&store, &provider_id);
            let failure_kind = if cleanup_result.is_ok() {
                "transaction_recovery_failed"
            } else {
                "transaction_recovery_cleanup_failed"
            };
            if let Some(manifest) = runtime.system_manifest(&provider_id) {
                emit_manifest_fallback_event(
                    &provider_id,
                    bundled_revision,
                    runtime.catalog_generation(),
                    "bundled",
                    manifest.snapshot_provenance.sha256.as_deref(),
                    failure_kind,
                );
            } else {
                emit_manifest_isolated_event(
                    &provider_id,
                    runtime.catalog_generation(),
                    failure_kind,
                );
            }
            continue;
        }
        let mut restored: Option<SystemManifestRestoreReport> = None;
        let mut fallback_failure_kind: Option<&'static str> = None;
        // An invalid generated state cannot authenticate Active or Previous. Remove it on a
        // best-effort basis and continue with the independent bundled baseline; one provider's
        // corrupt bookkeeping must not abort application startup (plan §17.4/§17.5).
        // 无效生成状态无法认证 Active 或 Previous。尽力移除后继续使用独立内置基线；一个
        // 供应商损坏的簿记不得中止应用启动（方案 §17.4/§17.5）。
        let state = match store.load_sync_state(&provider_id) {
            Ok(state) => state,
            Err(_error) => {
                fallback_failure_kind = Some("sync_state_invalid");
                if store.remove_sync_state(&provider_id).is_err() {
                    fallback_failure_kind = Some("sync_state_cleanup_failed");
                }
                vulcan_provider_config::manifest_store::SyncStateLoad::Absent
            }
        };
        let active_text = match store.read_active(&provider_id) {
            Ok(text) => text,
            Err(_error) => {
                fallback_failure_kind = Some("active_unreadable");
                None
            }
        };
        // Active bytes without authenticated state are not executable authority. Remove the
        // orphan on a best-effort provider-local basis so later status reads do not mistake it
        // for a recoverable layer.
        // 没有认证状态的 Active 字节不是可执行权威。按供应商尽力移除孤儿文件，避免后续
        // 状态读取把它误认为可恢复层。
        if matches!(
            &state,
            vulcan_provider_config::manifest_store::SyncStateLoad::Absent
        ) && active_text.is_some()
        {
            fallback_failure_kind = Some("active_state_missing");
            if store.remove_active(&provider_id).is_err() {
                fallback_failure_kind = Some("active_cleanup_failed");
            }
        }
        if matches!(
            &state,
            vulcan_provider_config::manifest_store::SyncStateLoad::Present(_)
        ) && active_text.is_none()
        {
            fallback_failure_kind = Some("active_missing");
        }
        if let (Some(text), vulcan_provider_config::manifest_store::SyncStateLoad::Present(state)) =
            (active_text, &state)
        {
            let expected = vulcan_provider_config::manifest_store::ManifestVersionState {
                revision: state.revision,
                sha256: state.sha256.clone(),
            };
            match verify_stored_system_manifest(
                &provider_id,
                &text,
                vulcan_provider_adapters::ManifestSourceLayer::Active,
                &expected,
            ) {
                Ok(resolved) => {
                    runtime.replace_system_manifest(&provider_id, resolved)?;
                    restored = Some(SystemManifestRestoreReport {
                        provider_id: provider_id.clone(),
                        revision: expected.revision,
                        layer: "active",
                        fallback_failure_kind: None,
                    });
                }
                Err(_error) => {
                    fallback_failure_kind = Some("active_invalid");
                    if store
                        .quarantine(
                            &provider_id,
                            expected.revision,
                            "active_invalid",
                            text.as_bytes(),
                        )
                        .is_err()
                    {
                        fallback_failure_kind = Some("active_invalid_quarantine_failed");
                    }
                }
            }
        }
        if restored.is_none()
            && let vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) = &state
            && let Some(previous) = state.previous.as_ref()
        {
            let previous_text = match store.read_previous(&provider_id) {
                Ok(text) => text,
                Err(_error) => {
                    fallback_failure_kind = Some("previous_unreadable");
                    None
                }
            };
            if let Some(text) = previous_text {
                match verify_stored_system_manifest(
                    &provider_id,
                    &text,
                    vulcan_provider_adapters::ManifestSourceLayer::Previous,
                    previous,
                ) {
                    Ok(resolved) => {
                        // File and state identities are swapped before runtime publication. Any
                        // failure restores both layers so startup never leaves a half rollback.
                        // 在运行时发布前交换文件与状态身份。任意失败都会恢复两层，使启动绝不
                        // 留下半回滚状态。
                        let restored_state = recovered_previous_manifest_sync_state(state)?;
                        match store.capture_system_snapshot(&provider_id) {
                            Ok(store_snapshot) => {
                                // storeRecovery mutates only this provider's four-file transaction.
                                // Any local failure is compensated to the exact before-image and
                                // leaves the runtime on its bundled baseline.
                                // storeRecovery 只修改该供应商的四文件事务。任一本地失败都会
                                // 补偿回精确前镜像，并使运行时继续使用内置基线。
                                let store_recovery = (|| -> Result<(), String> {
                                    store.rollback(&provider_id)?;
                                    store.remove_previous(&provider_id)?;
                                    store.save_sync_state(&provider_id, &restored_state)
                                })();
                                if store_recovery.is_err() {
                                    fallback_failure_kind = Some("previous_recovery_failed");
                                    if store
                                        .restore_system_snapshot(&provider_id, &store_snapshot)
                                        .is_err()
                                    {
                                        fallback_failure_kind =
                                            Some("previous_recovery_compensation_failed");
                                    }
                                } else if let Err(error) =
                                    runtime.replace_system_manifest(&provider_id, resolved)
                                {
                                    // Runtime publication failures indicate a shared in-memory
                                    // invariant failure. Restore provider-local files, then abort
                                    // startup because the catalog authority itself is unusable.
                                    // 运行时发布失败表示共享内存不变量失效。先恢复逐供应商文件，
                                    // 再中止启动，因为目录权威本身已不可用。
                                    let store_restore = store
                                        .restore_system_snapshot(&provider_id, &store_snapshot);
                                    return Err(format!(
                                        "Provider `{provider_id}` failed to publish recovered Previous manifest: {error}; store restore: {store_restore:?}"
                                    ));
                                } else {
                                    restored = Some(SystemManifestRestoreReport {
                                        provider_id: provider_id.clone(),
                                        revision: previous.revision,
                                        layer: "previous",
                                        fallback_failure_kind: Some(
                                            fallback_failure_kind
                                                .unwrap_or("active_unavailable")
                                                .to_string(),
                                        ),
                                    });
                                }
                            }
                            Err(_error) => {
                                fallback_failure_kind = Some("previous_recovery_snapshot_failed");
                            }
                        }
                    }
                    Err(_error) => {
                        fallback_failure_kind = Some("previous_invalid");
                        if store
                            .quarantine(
                                &provider_id,
                                previous.revision,
                                "previous_invalid",
                                text.as_bytes(),
                            )
                            .is_err()
                        {
                            fallback_failure_kind = Some("previous_invalid_quarantine_failed");
                        }
                    }
                }
            }
        }
        if let Some(report) = restored {
            reports.push(report);
        } else if let Some(manifest) = runtime.system_manifest(&provider_id) {
            if let Some(failure_kind) = fallback_failure_kind {
                emit_manifest_fallback_event(
                    &provider_id,
                    bundled_revision,
                    runtime.catalog_generation(),
                    "bundled",
                    manifest.snapshot_provenance.sha256.as_deref(),
                    failure_kind,
                );
            }
        } else {
            emit_manifest_isolated_event(
                &provider_id,
                runtime.catalog_generation(),
                fallback_failure_kind.unwrap_or("bundled_baseline_invalid"),
            );
        }
    }
    Ok(reports)
}

/// Emits startup restore events only after the complete provider generation is published.
/// 仅在完整供应商代际发布后发出启动恢复事件。
///
/// # Parameters
/// 参数
/// `runtime` owns the newly published immutable generation and `reports` are the exact staged
/// restore outcomes that participated in that activation.
/// `runtime` 持有新发布的不可变代际，`reports` 是参与该次激活的精确暂存恢复结果。
///
/// # Returns
/// 返回值
/// Success after every Previous fallback identity matches the published manifest; otherwise an
/// explicit startup invariant failure and no incorrect event for the mismatched report.
/// 每个 Previous 回退身份均与已发布 Manifest 匹配后返回成功；否则返回明确的启动不变量错误，
/// 且不会为不匹配报告发出错误事件。
pub(crate) fn emit_published_startup_manifest_restore_events(
    runtime: &PublicAiRuntime,
    reports: &[SystemManifestRestoreReport],
) -> Result<(), String> {
    for report in reports.iter().filter(|report| report.layer == "previous") {
        let manifest = runtime.system_manifest(report.provider_id.as_str()).ok_or_else(|| {
            format!(
                "Provider `{}` Previous restore is absent from the published startup generation.",
                report.provider_id
            )
        })?;
        if manifest.revision != report.revision
            || manifest.snapshot_provenance.source_layer
                != vulcan_provider_adapters::ManifestSourceLayer::Previous
        {
            return Err(format!(
                "Provider `{}` Previous restore identity does not match the published startup generation.",
                report.provider_id
            ));
        }
        let failure_kind = report.fallback_failure_kind.as_deref().ok_or_else(|| {
            format!(
                "Provider `{}` Previous restore has no fallback failure kind.",
                report.provider_id
            )
        })?;
        emit_manifest_fallback_event(
            report.provider_id.as_str(),
            manifest.revision,
            runtime.catalog_generation(),
            "previous",
            manifest.snapshot_provenance.sha256.as_deref(),
            failure_kind,
        );
    }
    Ok(())
}

/// Builds the authenticated sync state produced by swapping Active and Previous layers.
/// 构建交换 Active 与 Previous 层后产生的可信同步状态。
///
/// # Parameters
/// 参数
/// `state` identifies the current Active version and its authenticated Previous version.
/// `state` 标识当前 Active 版本及其已认证 Previous 版本。
///
/// # Returns
/// 返回值
/// A state whose Active identity is the former Previous and whose Previous identity is the
/// former Active. Conditional HTTP validators are cleared because they belong to the newer
/// remote revision and must not make an older rollback look current.
/// 返回以前一 Previous 为 Active、以前一 Active 为 Previous 的状态。条件 HTTP 验证器
/// 属于较新远端修订，必须清除，避免让旧回滚版本看似仍为当前版本。
fn swapped_manifest_sync_state(
    state: &vulcan_provider_config::manifest_store::ManifestSyncState,
) -> Result<vulcan_provider_config::manifest_store::ManifestSyncState, String> {
    let previous = state.previous.as_ref().ok_or_else(|| {
        String::from("Authenticated Previous manifest state is unavailable for rollback.")
    })?;
    Ok(vulcan_provider_config::manifest_store::ManifestSyncState {
        revision: previous.revision,
        sha256: previous.sha256.clone(),
        previous: Some(
            vulcan_provider_config::manifest_store::ManifestVersionState {
                revision: state.revision,
                sha256: state.sha256.clone(),
            },
        ),
        etag: None,
        index_etag: None,
        last_modified: None,
        synced_at_unix_ms: state.synced_at_unix_ms,
        last_error_kind: None,
    })
}

/// Builds recovered Active state after an invalid Active falls back to authenticated Previous.
/// 在无效 Active 回退到可信 Previous 后构建恢复的 Active 状态。
///
/// The invalid former Active is never retained as an authenticated Previous identity.
/// 无效的旧 Active 绝不会作为可信 Previous 身份保留。
fn recovered_previous_manifest_sync_state(
    state: &vulcan_provider_config::manifest_store::ManifestSyncState,
) -> Result<vulcan_provider_config::manifest_store::ManifestSyncState, String> {
    let previous = state.previous.as_ref().ok_or_else(|| {
        String::from("Authenticated Previous manifest state is unavailable for recovery.")
    })?;
    Ok(vulcan_provider_config::manifest_store::ManifestSyncState {
        revision: previous.revision,
        sha256: previous.sha256.clone(),
        previous: None,
        etag: None,
        index_etag: None,
        last_modified: None,
        synced_at_unix_ms: state.synced_at_unix_ms,
        last_error_kind: None,
    })
}

/// Builds the durable state for a revalidated provider HTTP 304 response.
/// 为重新校验通过的供应商 HTTP 304 响应构建持久状态。
///
/// A not-modified check may advance the shared Index validator and clear a prior transient error,
/// but it is not a new activation and therefore must preserve the exact successful activation time.
/// 未修改检查可以推进共享 Index 验证器并清除先前瞬态错误，但它不是新激活，因此必须保留
/// 精确的成功激活时间。
///
/// # Parameters
/// 参数
/// `current` is the authenticated Active state and `index_etag` is the newly observed shared
/// signed-Index validator.
/// `current` 是已认证 Active 状态，`index_etag` 是新观察到的共享签名 Index 验证器。
///
/// # Returns
/// 返回值
/// A detached state with unchanged activation identity and timestamp.
/// 返回一份激活身份与时间戳均不变的独立状态。
fn provider_not_modified_sync_state(
    current: &vulcan_provider_config::manifest_store::ManifestSyncState,
    index_etag: Option<String>,
) -> vulcan_provider_config::manifest_store::ManifestSyncState {
    let mut next = current.clone();
    next.index_etag = index_etag;
    next.last_error_kind = None;
    next
}

/// Outcome of one production sync attempt for one provider.
/// 一个供应商的一次生产同步尝试结果。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderSyncReport {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Stable outcome key: `not_configured`, `up_to_date`, `activated`, `rejected`, or
    /// `not_pinned`.
    /// 稳定结果键：`not_configured`、`up_to_date`、`activated`、`rejected` 或 `not_pinned`。
    pub outcome: &'static str,
    /// Synced manifest revision (zero when nothing activated).
    /// 已同步清单修订号（未激活时为零）。
    pub revision: u64,
    /// Classified rejection kind when `outcome` is `rejected`.
    /// `outcome` 为 `rejected` 时的分类拒绝类型。
    pub failure_kind: Option<String>,
    /// Optional bounded and redacted public diagnostic for a provider-scoped synchronization failure.
    /// 可选的有界脱敏公开逐供应商同步失败诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
    /// Non-serialized exact before-image retained only until projection publication succeeds.
    /// 仅保留到投影发布成功、且不序列化的精确前镜像。
    #[serde(skip)]
    pub(crate) activation_snapshot: Option<ProviderManifestActivationSnapshot>,
}

/// Classifies whether a full-catalog synchronization failed before or during provider projection.
/// 区分全目录同步是在远端批次阶段还是逐供应商投影阶段失败。
#[derive(Debug)]
pub(crate) enum ProviderSyncBatchFailure {
    /// Signed-index, network, or shared remote-batch failure before a provider report is finalized.
    /// 在逐供应商报告完成前发生的签名 Index、网络或共享远端批次失败。
    Remote(String),
    /// Provider projection failure whose exact compensation could not be proven complete.
    /// 无法证明精确补偿完整的逐供应商投影失败。
    Projection(String),
}

impl From<String> for ProviderSyncBatchFailure {
    /// Classifies ordinary pre-finalization string errors as shared remote-batch failures.
    /// 将普通的完成前字符串错误分类为共享远端批次失败。
    ///
    /// # Parameters
    /// 参数
    /// `message` is the exact diagnostic returned before a provider projection handler runs.
    /// `message` 是逐供应商投影处理器运行前返回的精确诊断。
    ///
    /// # Returns
    /// 返回值
    /// A remote-batch failure retaining the original message.
    /// 保留原始消息的远端批次失败。
    fn from(message: String) -> Self {
        Self::Remote(message)
    }
}

/// Closed outcome of one read-only signed candidate preview.
/// 一次只读签名候选预览的封闭结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderManifestPreviewOutcome {
    /// Remote synchronization is intentionally not configured.
    /// 远端同步有意保持未配置。
    NotConfigured,
    /// The signed index does not pin the requested provider.
    /// 签名 Index 未固定所请求供应商。
    NotPinned,
    /// The signed candidate is exactly the authenticated active version.
    /// 签名候选与已认证 active 版本完全一致。
    UpToDate,
    /// A newer candidate verified and compiled without mutating local state.
    /// 较新候选已验证并编译，且未修改本地状态。
    Candidate,
    /// The candidate failed integrity, ordering, schema, semantic, or execution checks.
    /// 候选未通过完整性、顺序、Schema、语义或执行检查。
    Rejected,
}

/// Closed change category shared by model-binding and operation preview deltas.
/// 模型绑定与 operation 预览差异共享的封闭变更类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderManifestChangeKind {
    /// Present only in the candidate.
    /// 仅存在于候选。
    Added,
    /// Present only in the current snapshot.
    /// 仅存在于当前快照。
    Removed,
    /// Present in both snapshots with different effective semantics.
    /// 同时存在于两份快照但有效语义不同。
    Changed,
}

/// One concrete model/profile binding delta in a candidate preview.
/// 候选预览中的一条具体模型/配置档绑定差异。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderManifestModelBindingDelta {
    /// Closed change category.
    /// 封闭变更类别。
    pub change: ProviderManifestChangeKind,
    /// Current binding when it exists.
    /// 当前绑定（存在时）。
    pub current: Option<ManifestModelBindingStatus>,
    /// Candidate binding when it exists.
    /// 候选绑定（存在时）。
    pub candidate: Option<ManifestModelBindingStatus>,
}

/// One operation delta in a candidate preview.
/// 候选预览中的一条 operation 差异。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProviderManifestOperationDelta {
    /// Closed change category.
    /// 封闭变更类别。
    pub change: ProviderManifestChangeKind,
    /// Current operation status when it exists.
    /// 当前 operation 状态（存在时）。
    pub current: Option<ManifestOperationStatus>,
    /// Candidate operation status when it exists.
    /// 候选 operation 状态（存在时）。
    pub candidate: Option<ManifestOperationStatus>,
}

/// Read-only verified preview of one remotely pinned provider manifest candidate.
/// 一份远端固定供应商 Manifest 候选的只读已验证预览。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderManifestCandidatePreview {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Closed preview outcome.
    /// 封闭预览结果。
    pub outcome: ProviderManifestPreviewOutcome,
    /// Current active manifest revision when available.
    /// 当前 active Manifest 修订号（可用时）。
    pub current_revision: Option<u64>,
    /// Signed candidate revision when the index pins the provider.
    /// Index 固定供应商时的签名候选修订号。
    pub candidate_revision: Option<u64>,
    /// Signed lowercase SHA-256 digest of the candidate bytes.
    /// 候选字节的签名小写 SHA-256 摘要。
    pub candidate_sha256: Option<String>,
    /// Whether every candidate model binding is executable after compilation.
    /// 编译后每条候选模型绑定是否均可执行。
    pub executable: bool,
    /// Stable rejection category when the candidate is not executable.
    /// 候选不可执行时的稳定拒绝分类。
    pub failure_kind: Option<String>,
    /// Bounded exact verification diagnostic without credentials or response bodies.
    /// 不含凭据或响应正文的有界精确验证诊断。
    pub failure_message: Option<String>,
    /// Bounded concrete model/profile binding deltas.
    /// 有界具体模型/配置档绑定差异。
    pub model_binding_changes: Vec<ProviderManifestModelBindingDelta>,
    /// Total binding delta count before response truncation.
    /// 响应截断前的绑定差异总数。
    pub model_binding_change_total_count: usize,
    /// Whether binding deltas were truncated.
    /// 绑定差异是否被截断。
    pub model_binding_changes_truncated: bool,
    /// Bounded semantic operation deltas.
    /// 有界语义 operation 差异。
    pub operation_changes: Vec<ProviderManifestOperationDelta>,
    /// Total operation delta count before response truncation.
    /// 响应截断前的 operation 差异总数。
    pub operation_change_total_count: usize,
    /// Whether operation deltas were truncated.
    /// operation 差异是否被截断。
    pub operation_changes_truncated: bool,
}

/// Immutable local comparison baseline captured before candidate-preview network I/O.
/// 在候选预览网络 I/O 前捕获的不可变本地对比基线。
#[derive(Clone)]
pub(crate) struct ProviderManifestPreviewBaseline {
    /// Exact installed configuration paired with the captured runtime generation.
    /// 与已捕获运行时代际配对的精确已安装配置。
    configuration: PersistedAiConfiguration,
    /// Coherent active Manifest plus discovery/support/effective catalogs.
    /// 一致的 active Manifest 及发现/支持/有效目录。
    runtime_snapshot: vulcan_provider_adapters::ProviderManifestRuntimeSnapshot,
}

/// Verifies one caller-controlled identity belongs to the trusted bundled system-provider set.
/// 验证一个调用方可控身份属于可信内置系统供应商集合。
///
/// # Parameters
/// 参数
/// `runtime` owns the immutable trusted identity set and `provider_id` is untrusted input.
/// `runtime` 持有不可变可信身份集合，`provider_id` 是不可信输入。
///
/// # Returns
/// 返回值
/// Success only for an exact known identity; errors never echo the untrusted value.
/// 仅精确已知身份返回成功；错误绝不回显不可信值。
pub(crate) fn require_trusted_system_manifest_provider(
    runtime: &PublicAiRuntime,
    provider_id: &str,
) -> Result<(), String> {
    if provider_id.len() > vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN
    {
        return Err(String::from(
            "Provider manifest operation requires a trusted system provider identifier.",
        ));
    }
    let known_provider_ids = runtime.system_manifest_provider_ids()?;
    if known_provider_ids
        .iter()
        .any(|known_provider_id| known_provider_id == provider_id)
    {
        Ok(())
    } else {
        Err(String::from(
            "Provider manifest operation requires a trusted system provider identifier.",
        ))
    }
}

/// Converts one provider-scoped synchronization error into an isolated report.
/// 把一个逐供应商同步错误转换为隔离报告。
///
/// # Parameters
/// 参数
/// `provider_id` identifies the failed provider and `message` is reduced to a bounded public diagnostic.
/// `provider_id` 标识失败供应商，`message` 会收敛为有界公开诊断。
///
/// # Returns
/// 返回值
/// A rejected report that lets the remaining providers continue in the same batch.
/// 返回允许同一批次其余供应商继续执行的拒绝报告。
fn provider_sync_failure_report(provider_id: &str, message: String) -> ProviderSyncReport {
    ProviderSyncReport {
        provider_id: provider_id.to_string(),
        outcome: "rejected",
        revision: 0,
        failure_kind: Some(String::from("sync_failed")),
        failure_message: Some(bounded_public_manifest_diagnostic(message.as_str())),
        activation_snapshot: None,
    }
}

/// One typed per-provider manifest status snapshot (plan §R10).
/// 一份 typed 逐供应商清单状态快照（方案 §R10）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderManifestStatus {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Provider-local status projection error that did not abort unrelated provider statuses.
    /// 未中止无关供应商状态的逐供应商状态投影错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_error: Option<ProviderManifestCriticalErrorStatus>,
    /// Critical provider-local error when no executable trusted layer can be projected.
    /// 无法投影任何可执行可信层时的逐供应商严重错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub critical_error: Option<ProviderManifestCriticalErrorStatus>,
    /// Provider-local bundled baseline failure, even when Active/Previous recovered execution.
    /// 逐供应商内置基线失败；即使 Active/Previous 已恢复执行也会保留。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundled_failure: Option<ProviderManifestCriticalErrorStatus>,
    /// Bundled baseline revision and hash when the bundled file is readable.
    /// 内置基线修订与摘要（内置文件可读时）。
    pub bundled: Option<ManifestLayerStatus>,
    /// Active layer revision, source layer, and hash currently used by the runtime.
    /// 运行时当前使用的 active 层修订、来源层与摘要。
    pub active: Option<ManifestActiveStatus>,
    /// Persisted previous layer revision and hash when present.
    /// 持久化 previous 层修订与摘要（存在时）。
    pub previous: Option<ManifestLayerStatus>,
    /// Bounded sync-state bookkeeping when a sync transaction ever ran.
    /// 有界同步状态簿记（曾运行过同步事务时）。
    pub sync_state: Option<ManifestSyncStateView>,
    /// Independent latest-check state, including failures before the first activation.
    /// 独立的最近检查状态，包含首次激活前的失败。
    pub sync_check_state: Option<ManifestCheckStateView>,
    /// Newest-first bounded structured operational events for this provider.
    /// 该供应商按新到旧排列的有界结构化运行事件。
    pub recent_telemetry_events: Vec<ProviderOperationalTelemetryEvent>,
    /// Event count currently retained for this provider inside the process-wide ring.
    /// 进程级环中当前为该供应商保留的事件数量。
    pub recent_telemetry_total_count: usize,
    /// Whether the status response truncated the retained process-wide provider events.
    /// 状态响应是否截断了进程级已保留供应商事件。
    pub recent_telemetry_events_truncated: bool,
    /// Classified quarantined candidates for this provider.
    /// 该供应商的分类隔离候选。
    pub quarantine: Vec<vulcan_provider_config::manifest_store::QuarantineSummary>,
    /// Whether the remote sync service is configured (external gate open).
    /// 远端同步服务是否已配置（外部门开启）。
    pub sync_configured: bool,
    /// Effective model-rule facts and their field-level provenance from the active snapshot.
    /// 当前生效快照中的有效模型规则事实及其字段级来源。
    pub model_rules: Vec<ManifestModelRuleStatus>,
    /// Concrete model/profile protocol bindings resolved exclusively by the backend authority.
    /// 完全由后端权威解析的具体模型/凭据配置档协议绑定。
    pub model_bindings: Vec<ManifestModelBindingStatus>,
    /// Total concrete bindings before the bounded status projection was truncated.
    /// 有界状态投影截断前的具体绑定总数。
    pub model_binding_total_count: usize,
    /// Whether `model_bindings` was truncated by the response bound.
    /// `model_bindings` 是否被响应上限截断。
    pub model_bindings_truncated: bool,
    /// Exact authenticated layer used as the active model-binding change baseline.
    /// 用作 active 模型绑定变化基线的精确已认证层。
    pub active_model_binding_change_baseline: Option<ManifestModelBindingChangeBaselineStatus>,
    /// Bounded semantic binding deltas from the baseline to the active manifest generation.
    /// 从基线到 active Manifest 代际的有界语义绑定差异。
    pub active_model_binding_changes: Vec<ProviderManifestModelBindingDelta>,
    /// Total active binding delta count before response truncation.
    /// 响应截断前的 active 绑定差异总数。
    pub active_model_binding_change_total_count: usize,
    /// Whether active binding deltas were truncated.
    /// active 绑定差异是否被截断。
    pub active_model_binding_changes_truncated: bool,
    /// Effective operation-to-upload/tool bindings from the active snapshot.
    /// 当前生效快照中的 operation 到 upload/tool 的绑定。
    pub operations: Vec<ManifestOperationStatus>,
    /// Concrete provider-tool targets instantiated from immutable bundled templates.
    /// 从不可变内置模板实例化的具体供应商工具目标。
    pub tool_target_template_instances: Vec<ManifestToolTargetTemplateInstanceStatus>,
    /// Provenance for every active profile, upload channel, operation, and tool target.
    /// 当前生效的每个 Profile、上传通道、operation 与工具目标的来源。
    pub declaration_provenance:
        BTreeMap<String, vulcan_provider_adapters::ManifestDeclarationProvenance>,
}

/// Closed model-inventory source exposed by provider diagnostics.
/// 供应商诊断公开的封闭模型目录来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestModelSourceStatus {
    /// The identity is declared by the active Manifest catalog or an exact model rule.
    /// 模型身份由 active Manifest 目录或精确模型规则声明。
    Manifest,
    /// The identity was observed through the service entry's remote discovery contract.
    /// 模型身份通过服务入口的远端发现契约观测。
    Discovery,
    /// The identity comes from a user-owned manual model declaration.
    /// 模型身份来自用户持有的手工模型声明。
    Manual,
    /// Two or more authoritative source layers contribute to the effective identity.
    /// 两个或更多权威来源层共同贡献该有效模型身份。
    Merged,
}

/// One concrete backend-resolved model protocol binding without secrets.
/// 一条不含秘密、由后端解析的具体模型协议绑定。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ManifestModelBindingStatus {
    /// Exact credential connection whose catalog facts are projected, when configured.
    /// 被投影目录事实所属的精确凭据连接（已配置时）。
    pub connection_id: Option<String>,
    /// Exact service entry owning the model catalog identity.
    /// 拥有模型目录身份的精确服务入口。
    pub service_entry_id: String,
    /// Exact credential profile whose scoped rule was evaluated, when present.
    /// 被求值作用域规则所属的精确凭据配置档（存在时）。
    pub credential_profile_id: Option<String>,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    pub model_id: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    pub display_name: String,
    /// Closed effective identity source after manifest, discovery, and manual-model merging.
    /// Manifest、发现与手工模型合并后的封闭有效身份来源。
    pub model_source: ManifestModelSourceStatus,
    /// Exact contributing source layers in deterministic order; `merged` is never stored here.
    /// 按确定性顺序排列的精确贡献来源层；此处绝不存储 `merged`。
    pub model_source_layers: Vec<ManifestModelSourceStatus>,
    /// Effective protocol candidates in deterministic priority order.
    /// 按确定性优先级排列的有效协议候选。
    pub supported_protocols: Vec<String>,
    /// Protocol variants actually present in this connection's model catalog.
    /// 当前连接模型目录中实际存在的协议变体。
    pub available_protocols: Vec<String>,
    /// Unique effective protocol when this binding is executable.
    /// 绑定可执行时的唯一有效协议。
    pub effective_protocol: Option<String>,
    /// Stable backend-owned source label, such as `system_manifest`.
    /// 后端持有的稳定来源标签，例如 `system_manifest`。
    pub binding_source: Option<String>,
    /// Rule identifier that produced the binding; absent for a provider default.
    /// 产生绑定的规则标识；供应商默认绑定时为空。
    pub binding_rule_id: Option<String>,
    /// Manifest revision that produced the binding.
    /// 产生绑定的 Manifest 修订号。
    pub manifest_revision: u64,
    /// Effective operation references inherited by this exact model scope.
    /// 此精确模型作用域继承的有效 operation 引用。
    pub operation_refs: Vec<String>,
    /// Effective model capabilities after all published catalog overlays, when observed.
    /// 应用全部已发布目录覆盖后的有效模型能力（已观测时）。
    pub capabilities: Option<vulcan_provider_adapters::ModelCapabilities>,
    /// Field-presence evidence paired with the effective catalog capability snapshot.
    /// 与有效目录能力快照配对的字段存在性证据。
    pub capability_evidence: Option<vulcan_provider_adapters::ModelCapabilityEvidence>,
    /// Manifest field provenance for every capability contributed by the matching rule.
    /// 匹配规则贡献的每个能力字段的 Manifest 来源。
    pub capability_provenance: BTreeMap<String, vulcan_provider_adapters::ResolvedFieldProvenance>,
    /// Whether any published source/support/effective catalog currently observes this identity.
    /// 当前是否有任一已发布来源/支持/有效目录观测到该身份。
    pub catalog_observed: bool,
    /// Whether the model remains in the effective catalog after access projection.
    /// 应用访问投影后模型是否仍在有效目录中。
    pub catalog_enabled: bool,
    /// Whether the backend can build an exact protocol route for this model scope.
    /// 后端是否能为此模型作用域构建精确协议路由。
    pub executable: bool,
    /// Stable actionable diagnostic when the binding is not executable.
    /// 绑定不可执行时的稳定可操作诊断。
    pub unavailable_reason: Option<String>,
}

/// Browser-safe latest configured remote-check state for one provider.
/// 一个供应商面向浏览器安全的最近已配置远端检查状态。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestCheckStateView {
    /// Unix milliseconds when the most recent configured check finished.
    /// 最近一次已配置检查完成时的 Unix 毫秒时间。
    pub checked_at_unix_ms: u64,
    /// Unix milliseconds when activation and every projection last completed.
    /// 激活及全部投影最近一次完成时的 Unix 毫秒时间。
    pub last_successful_at_unix_ms: Option<u64>,
    /// Closed result of the most recent configured check.
    /// 最近一次已配置检查的封闭结果。
    pub outcome: ManifestCheckOutcome,
    /// Stable classified failure kind without response bodies or URLs.
    /// 不含响应正文或 URL 的稳定分类失败类型。
    pub last_error_kind: Option<String>,
}

/// One browser-safe effective model rule with exact field provenance.
/// 一条面向浏览器安全公开且带精确字段来源的有效模型规则。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestModelRuleStatus {
    /// Stable rule identifier.
    /// 稳定规则标识。
    pub rule_id: String,
    /// Exact service entry receiving this expanded rule instance.
    /// 接收此展开规则实例的精确服务入口。
    pub service_entry_id: String,
    /// Optional credential-profile scope; empty means every profile on the entry.
    /// 可选凭据配置档作用域；为空表示入口内全部配置档。
    pub credential_profile_ids: Vec<String>,
    /// Exact or longest-prefix model matcher.
    /// 精确或最长前缀模型匹配器。
    pub matcher: vulcan_provider_adapters::RuleMatcher,
    /// Effective protocol candidates in deterministic priority order.
    /// 按确定性优先级排序的有效协议候选。
    pub supported_protocols: Vec<String>,
    /// Unique effective protocol, when the rule declares an executable binding.
    /// 规则声明可执行绑定时的唯一有效协议。
    pub selected_protocol: Option<String>,
    /// Effective default-enabled state.
    /// 有效默认启用状态。
    pub default_enabled: bool,
    /// Stable model-parameter identifiers contributed by the rule.
    /// 规则贡献的稳定模型参数标识。
    pub model_parameter_ids: Vec<String>,
    /// Effective usage profile references.
    /// 有效 usage Profile 引用。
    pub usage_profile_refs: Vec<String>,
    /// Effective request profile references.
    /// 有效请求 Profile 引用。
    pub request_profile_refs: Vec<String>,
    /// Effective response/error profile references.
    /// 有效响应/错误 Profile 引用。
    pub response_profile_refs: Vec<String>,
    /// Effective operation references.
    /// 有效 operation 引用。
    pub operation_refs: Vec<String>,
    /// Number of model-specific input-resource constraints applied by this rule.
    /// 此规则应用的模型专属输入资源限制数量。
    pub input_resource_constraint_count: usize,
    /// Provenance for non-capability routing and reference fields.
    /// 非能力路由与引用字段的来源。
    pub field_provenance: BTreeMap<String, vulcan_provider_adapters::ResolvedFieldProvenance>,
    /// Provenance for protocol-independent and protocol-scoped capability fields.
    /// 协议无关及协议作用域能力字段的来源。
    pub capability_provenance: BTreeMap<String, vulcan_provider_adapters::ResolvedFieldProvenance>,
    /// Rule-level evidence provenance frozen by the manifest compiler.
    /// Manifest 编译器冻结的规则级证据来源。
    pub provenance: vulcan_provider_adapters::ResolvedRuleProvenance,
}

/// One active operation's profile, upload-channel, and tool-target closure.
/// 一个生效 operation 的 Profile、上传通道与工具目标闭包。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestOperationStatus {
    /// Stable operation identifier.
    /// 稳定 operation 标识。
    pub operation_id: String,
    /// Canonical provider-neutral operation name.
    /// 规范供应商无关 operation 名称。
    pub operation: String,
    /// Closed execution carrier selected by the manifest.
    /// Manifest 选择的封闭执行载体。
    pub carrier: vulcan_provider_adapters::provider_manifest::OperationCarrier,
    /// Input resource kinds consumed by this operation.
    /// 此 operation 消费的输入资源类别。
    pub input_resources: Vec<vulcan_ai_contract::ResourceKind>,
    /// Output resource kinds produced by this operation.
    /// 此 operation 产生的输出资源类别。
    pub output_resources: Vec<vulcan_ai_contract::ResourceKind>,
    /// Exact input MIME types accepted by referenced upload bindings/channels.
    /// 引用上传绑定/通道接受的精确输入 MIME 类型。
    pub input_mime_types: Vec<String>,
    /// Exact output MIME types declared by referenced tool targets.
    /// 引用工具目标声明的精确输出 MIME 类型。
    pub output_mime_types: Vec<String>,
    /// Referenced request-profile identifiers.
    /// 引用的请求 Profile 标识。
    pub request_profile_refs: Vec<String>,
    /// Referenced response-profile identifiers.
    /// 引用的响应 Profile 标识。
    pub response_profile_refs: Vec<String>,
    /// Referenced upload-channel identifiers.
    /// 引用的上传通道标识。
    pub upload_channel_refs: Vec<String>,
    /// Referenced provider-tool target identifiers.
    /// 引用的供应商工具目标标识。
    pub tool_target_refs: Vec<String>,
    /// Referenced immutable provider-tool template identifiers owned by this operation.
    /// 当前 operation 持有的不可变供应商工具模板标识。
    pub tool_target_template_refs: Vec<String>,
    /// Whether execution uses a start/poll asynchronous lifecycle.
    /// 执行是否使用启动/轮询异步生命周期。
    pub asynchronous: bool,
    /// Fixed successful-observation polling interval for asynchronous execution.
    /// 异步执行在成功观测后的固定轮询间隔。
    pub poll_interval_ms: Option<u64>,
    /// Maximum retry backoff after consecutive asynchronous observation failures.
    /// 连续异步观测失败后的最大重试退避时间。
    pub max_backoff_ms: Option<u64>,
    /// Whether the asynchronous lifecycle declares an explicit cancellation operation.
    /// 异步生命周期是否声明显式取消操作。
    pub cancellation_supported: bool,
    /// Maximum operation execution time in milliseconds when declared.
    /// 声明时的 operation 最大执行时间（毫秒）。
    pub timeout_ms: Option<u64>,
    /// Bounded concrete model identities whose effective rules reference this operation.
    /// 有界的具体模型身份，其有效规则引用此 operation。
    pub matching_models: Vec<ManifestOperationMatchingModelStatus>,
    /// Total concrete matching model identities before truncation.
    /// 截断前的具体匹配模型身份总数。
    pub matching_model_total_count: usize,
    /// Whether the matching model summary was truncated.
    /// 匹配模型摘要是否被截断。
    pub matching_models_truncated: bool,
}

/// One concrete model identity summarized by an operation status.
/// 一条由 operation 状态汇总的具体模型身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct ManifestOperationMatchingModelStatus {
    /// Exact service entry owning the model identity.
    /// 拥有模型身份的精确服务入口。
    pub service_entry_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    pub model_id: String,
}

/// One browser-safe concrete target instantiated from an immutable provider-tool template.
/// 一条面向浏览器安全公开、从不可变供应商工具模板实例化的具体目标。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestToolTargetTemplateInstanceStatus {
    /// Deterministic concrete target identifier.
    /// 确定性的具体目标标识。
    pub target_id: String,
    /// Immutable bundled template identifier supplying execution authority.
    /// 提供执行权限的不可变内置模板标识。
    pub template_id: String,
    /// Exact service entry whose catalog owns the model.
    /// 模型目录所属的精确服务入口。
    pub service_entry_id: String,
    /// Exact upstream model identifier supplied only as data.
    /// 仅作为数据提供的精确上游模型标识。
    pub model_id: String,
    /// User-visible catalog model name.
    /// 用户可见目录模型名称。
    pub model_display_name: String,
}

/// One immutable layer revision and content hash.
/// 一个不可变层的修订与内容摘要。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestLayerStatus {
    /// Manifest revision.
    /// 清单修订号。
    pub revision: u64,
    /// Lowercase SHA-256 hex digest of the layer bytes.
    /// 层字节的十六进制 SHA-256 摘要。
    pub sha256: String,
}

/// One bounded provider-local critical manifest diagnostic exposed to operators.
/// 一条暴露给运维人员的有界逐供应商严重 Manifest 诊断。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProviderManifestCriticalErrorStatus {
    /// Stable closed failure category suitable for UI branching and telemetry correlation.
    /// 适用于 UI 分支与遥测关联的稳定封闭失败类别。
    pub failure_kind: String,
    /// Bounded non-secret diagnostic generated from local validation or status projection.
    /// 由本地校验或状态投影产生的有界非秘密诊断。
    pub message: String,
}

/// One exact authenticated layer selected as the active model-binding diff baseline.
/// 一份被选为 active 模型绑定差异基线的精确已认证层。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestModelBindingChangeBaselineStatus {
    /// Baseline layer identity; only `previous` or `bundled` is emitted.
    /// 基线层身份；仅会输出 `previous` 或 `bundled`。
    pub layer: ManifestActiveLayer,
    /// Baseline manifest revision.
    /// 基线 Manifest 修订号。
    pub revision: u64,
    /// Lowercase SHA-256 digest of the exact baseline bytes.
    /// 精确基线字节的小写 SHA-256 摘要。
    pub sha256: String,
}

/// The active layer identity currently bound in the runtime catalog.
/// 运行时目录当前绑定的 active 层身份。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestActiveStatus {
    /// Active manifest revision.
    /// active 清单修订号。
    pub revision: u64,
    /// Source layer the active snapshot was compiled from.
    /// active 快照编译自的来源层。
    pub layer: ManifestActiveLayer,
    /// Optional content hash when the store filled it.
    /// 存储填充内容摘要时存在。
    pub sha256: Option<String>,
    /// Minimum manifest engine version verified for this active generation.
    /// 当前生效代际已验证的最低 Manifest 引擎版本。
    pub minimum_manifest_engine: u32,
    /// Whole-manifest verification status.
    /// 整份 Manifest 的验证状态。
    pub provenance_status: vulcan_provider_adapters::provider_manifest::ProvenanceStatus,
    /// Number of exact evidence sources frozen into the active snapshot.
    /// 冻结进生效快照的精确证据来源数量。
    pub evidence_source_count: usize,
}

/// Projects one immutable runtime manifest into its non-secret active-layer status.
/// 将一份不可变运行时 Manifest 投影为不含秘密的 active 层状态。
///
/// # Parameters
/// 参数
/// `manifest` is the exact compiled snapshot retained by the published generation.
/// `manifest` 是已发布代际保留的精确已编译快照。
///
/// # Returns
/// 返回值
/// Revision, source, digest, engine, and provenance facts for operator diagnostics.
/// 返回供运维诊断使用的修订、来源、摘要、引擎与来源证明事实。
fn manifest_active_status(
    manifest: &vulcan_provider_adapters::ResolvedProviderManifest,
) -> ManifestActiveStatus {
    let layer = match manifest.snapshot_provenance.source_layer {
        vulcan_provider_adapters::ManifestSourceLayer::Bundled => ManifestActiveLayer::Bundled,
        vulcan_provider_adapters::ManifestSourceLayer::Active => ManifestActiveLayer::Active,
        vulcan_provider_adapters::ManifestSourceLayer::Custom => ManifestActiveLayer::Custom,
        vulcan_provider_adapters::ManifestSourceLayer::Previous => ManifestActiveLayer::Previous,
    };
    ManifestActiveStatus {
        revision: manifest.revision,
        layer,
        sha256: manifest.snapshot_provenance.sha256.clone(),
        minimum_manifest_engine: manifest.engine_contract.minimum_manifest_engine,
        provenance_status: manifest.manifest_provenance.status,
        evidence_source_count: manifest.manifest_provenance.sources.len(),
    }
}

/// Closed set of active-layer origins exposed to the UI/CLI.
/// 暴露给 UI/CLI 的封闭 active 层来源集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestActiveLayer {
    /// Software-bundled baseline.
    /// 软件内置基线。
    Bundled,
    /// Locally activated (active/LKG) layer.
    /// 本地已激活（active/LKG）层。
    Active,
    /// User-owned custom provider manifest.
    /// 用户自有自定义供应商清单。
    Custom,
    /// Restored from the previous layer after a rollback.
    /// 回滚后从 previous 层恢复。
    Previous,
}

/// Bounded sync-state view exposed to the UI/CLI.
/// 暴露给 UI/CLI 的有界同步状态视图。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestSyncStateView {
    /// Active manifest revision.
    /// active 清单修订号。
    pub revision: u64,
    /// SHA-256 hex digest of the active bytes.
    /// active 字节的 SHA-256 十六进制摘要。
    pub sha256: String,
    /// Optional remote ETag.
    /// 可选远端 ETag。
    pub etag: Option<String>,
    /// Optional remote index ETag.
    /// 可选远端 Index ETag。
    pub index_etag: Option<String>,
    /// Optional remote Last-Modified.
    /// 可选远端 Last-Modified。
    pub last_modified: Option<String>,
    /// Last successful sync unix milliseconds.
    /// 最近一次成功同步的 Unix 毫秒时间。
    pub synced_at_unix_ms: u64,
    /// Classified failure kind of the last rejected candidate.
    /// 最近一次被拒候选的分类失败类型。
    pub last_error_kind: Option<String>,
}

/// Mutable internal inventory facts for one exact connection/model scope.
/// 单个精确连接/模型作用域的可变内部目录事实。
#[derive(Default)]
struct ManifestModelInventoryFacts {
    /// Manifest-owned display name used only when no published catalog supplies one.
    /// 仅在没有已发布目录提供名称时使用的 Manifest 显示名。
    manifest_display_name: Option<String>,
    /// Exact identity-source layers contributing to this model.
    /// 贡献当前模型的精确身份来源层。
    source_layers: BTreeSet<ManifestModelSourceStatus>,
    /// Pre-overlay discovery model for this exact connection.
    /// 当前精确连接的覆盖前发现模型。
    source_model: Option<vulcan_provider_adapters::ConnectionModel>,
    /// Access-independent post-overlay model for this exact connection.
    /// 当前精确连接独立于访问开关的覆盖后模型。
    support_model: Option<vulcan_provider_adapters::ConnectionModel>,
    /// Effective post-access model used by current request selection.
    /// 当前请求选择使用的访问后有效模型。
    effective_model: Option<vulcan_provider_adapters::ConnectionModel>,
}

/// Classifies the identity source of a service entry's pre-overlay catalog.
/// 分类服务入口覆盖前目录的身份来源。
///
/// # Parameters
/// 参数
/// `catalog` is the exact pre-overlay source snapshot whose schema marker is authoritative.
/// `catalog` 是精确的覆盖前来源快照，其 Schema 标记具有权威性。
///
/// # Returns
/// 返回值
/// The closed concrete identity source represented by this catalog.
/// 返回该目录表示的封闭具体身份来源。
fn manifest_catalog_source_layer(
    catalog: &vulcan_provider_adapters::ConnectionModelCatalog,
) -> ManifestModelSourceStatus {
    match catalog.discovery_schema_revision.as_str() {
        "provider-manifest.static-models.v1" => ManifestModelSourceStatus::Manifest,
        "manual.models.v1" => ManifestModelSourceStatus::Manual,
        _ => ManifestModelSourceStatus::Discovery,
    }
}

/// Returns one catalog model by exact connection and model identifiers.
/// 按精确连接与模型标识返回一条目录模型。
///
/// # Parameters
/// 参数
/// `catalogs`, `connection_id`, and `model_id` identify one immutable published catalog row.
/// `catalogs`、`connection_id` 与 `model_id` 标识一条不可变已发布目录记录。
///
/// # Returns
/// 返回值
/// The exact model when that catalog layer contains it.
/// 对应目录层包含该模型时返回精确模型。
fn manifest_catalog_model(
    catalogs: &BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
    connection_id: &str,
    model_id: &str,
) -> Option<vulcan_provider_adapters::ConnectionModel> {
    catalogs
        .get(connection_id)
        .and_then(|catalog| catalog.models.iter().find(|model| model.id == model_id))
        .cloned()
}

/// Resolves one closed public model-source label from exact contributing layers.
/// 从精确贡献来源层解析一个封闭公开模型来源标签。
///
/// # Parameters
/// 参数
/// `layers` is a non-empty set containing only concrete source layers.
/// `layers` 是仅包含具体来源层的非空集合。
///
/// # Returns
/// 返回值
/// The unique layer or `merged` when multiple layers contribute.
/// 返回唯一来源层；多个来源层共同贡献时返回 `merged`。
fn resolved_manifest_model_source(
    layers: &BTreeSet<ManifestModelSourceStatus>,
) -> Result<ManifestModelSourceStatus, String> {
    if layers.len() > 1 {
        return Ok(ManifestModelSourceStatus::Merged);
    }
    layers.iter().next().copied().ok_or_else(|| {
        String::from("A provider model status cannot be emitted without an attributable source.")
    })
}

/// Builds one exact connection/profile inventory before resolving protocol bindings.
/// 在解析协议绑定前构建一个精确连接/配置档目录。
///
/// # Parameters
/// 参数
/// `manifest_models` contains static identities; the three catalog maps are paired published
/// layers; `configuration`, `entry`, and `connection_id` identify the owning scope.
/// `manifest_models` 包含静态身份；三份目录映射是配对发布层；`configuration`、`entry` 与
/// `connection_id` 标识所属作用域。
///
/// # Returns
/// 返回值
/// A deterministic model-id map with exact source attribution, or a fail-closed source error.
/// 返回按模型标识确定性排序且来源精确的映射；来源无法归属时失败关闭。
// Manifest reconciliation distinguishes declaration, connection and catalog authorities.
// 清单协调区分声明、连接和目录权威。
#[expect(
    clippy::too_many_arguments,
    reason = "Manifest reconciliation distinguishes declaration, connection and catalog authorities."
)]
fn manifest_model_inventory_for_scope(
    manifest_models: &BTreeMap<String, (String, bool)>,
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    entry: &vulcan_provider_adapters::ServiceEntry,
    source_policy: vulcan_provider_adapters::ModelSourcePolicy,
    connection_id: Option<&str>,
    effective_catalogs: &BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
    model_support_catalogs: &BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
    source_catalogs: &BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
) -> Result<BTreeMap<String, ManifestModelInventoryFacts>, String> {
    // inventory begins with every Manifest-declared identity in the exact service entry.
    // inventory 从精确服务入口中每个 Manifest 声明身份开始构建。
    let mut inventory = manifest_models
        .iter()
        .map(|(model_id, (display_name, manifest_identity_declared))| {
            let mut facts = ManifestModelInventoryFacts {
                manifest_display_name: Some(display_name.clone()),
                ..ManifestModelInventoryFacts::default()
            };
            if *manifest_identity_declared {
                facts
                    .source_layers
                    .insert(ManifestModelSourceStatus::Manifest);
            }
            (model_id.clone(), facts)
        })
        .collect::<BTreeMap<_, _>>();
    let Some(connection_id) = connection_id else {
        // Rule-only exact declarations remain useful diagnostics without claiming to be an
        // observed catalog identity; Manifest is their only attributable declaration source.
        // 仅规则的 Exact 声明在未配置连接时仍是有用诊断；Manifest 是其唯一可归属声明来源。
        for facts in inventory.values_mut() {
            if facts.source_layers.is_empty() {
                facts
                    .source_layers
                    .insert(ManifestModelSourceStatus::Manifest);
            }
        }
        return Ok(inventory);
    };

    // catalogModelIds is the exact union of all paired layers for this connection.
    // catalogModelIds 是当前连接全部配对目录层的精确并集。
    let catalog_model_ids = source_catalogs
        .get(connection_id)
        .into_iter()
        .flat_map(|catalog| catalog.models.iter().map(|model| model.id.clone()))
        .chain(
            model_support_catalogs
                .get(connection_id)
                .into_iter()
                .flat_map(|catalog| catalog.models.iter().map(|model| model.id.clone())),
        )
        .chain(
            effective_catalogs
                .get(connection_id)
                .into_iter()
                .flat_map(|catalog| catalog.models.iter().map(|model| model.id.clone())),
        )
        .collect::<BTreeSet<_>>();
    for model_id in catalog_model_ids {
        // facts owns the exact catalog observations for one model identity.
        // facts 持有一个模型身份的精确目录观测。
        let facts = inventory.entry(model_id.clone()).or_default();
        facts.source_model = manifest_catalog_model(source_catalogs, connection_id, &model_id);
        facts.support_model =
            manifest_catalog_model(model_support_catalogs, connection_id, &model_id);
        facts.effective_model =
            manifest_catalog_model(effective_catalogs, connection_id, &model_id);
        if facts.source_model.is_some() {
            let source_catalog = source_catalogs.get(connection_id).ok_or_else(|| {
                format!(
                    "Source model `{model_id}` has no owning catalog for connection `{connection_id}`."
                )
            })?;
            facts
                .source_layers
                .insert(manifest_catalog_source_layer(source_catalog));
        }
        if source_policy.includes_manual()
            && configuration.custom_model(provider_id, &model_id).is_some()
        {
            facts
                .source_layers
                .insert(ManifestModelSourceStatus::Manual);
        }
    }
    // Every exact rule-only identity is still attributable to the Manifest even when this
    // configured connection has not observed it; catalog-only identities must retain a concrete
    // authorized catalog source or fail closed.
    // 即使当前已配置连接尚未观测到精确的仅规则模型，该身份仍可归因于 Manifest；仅目录
    // 身份必须保留具体的授权目录来源，否则失败关闭。
    for (model_id, facts) in &mut inventory {
        if facts.source_layers.is_empty() {
            if facts.manifest_display_name.is_some() {
                facts
                    .source_layers
                    .insert(ManifestModelSourceStatus::Manifest);
            } else {
                return Err(format!(
                    "Service entry `{}` published model `{model_id}` without an attributable authorized source.",
                    entry.id
                ));
            }
        }
    }
    Ok(inventory)
}

/// Resolves every concrete manifest-backed and runtime-catalog model binding for one snapshot.
/// 为一份快照解析每条具体 Manifest 与运行时目录模型绑定。
///
/// # Parameters
/// 参数
/// `manifest` is the immutable backend authority; `configuration` owns exact connection/manual
/// membership; the three catalog maps come from the same published runtime generation.
/// `manifest` 是不可变后端权威；`configuration` 持有精确连接/手工成员关系；三份目录映射
/// 来自同一已发布运行时代际。
///
/// # Returns
/// 返回值
/// A deterministic full list that callers bound only after deriving aggregate operation facts.
/// 返回确定性完整列表；调用方仅在派生 operation 聚合事实后施加上限。
fn manifest_model_binding_statuses(
    manifest: &vulcan_provider_adapters::ResolvedProviderManifest,
    configuration: &PersistedAiConfiguration,
    effective_catalogs: &BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
    model_support_catalogs: &BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
    source_catalogs: &BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
) -> Result<Vec<ManifestModelBindingStatus>, String> {
    // modelsByEntry merges authoritative catalog identities with exact-rule identities.
    // modelsByEntry 合并权威目录身份与精确规则身份。
    let mut models_by_entry = BTreeMap::<String, BTreeMap<String, (String, bool)>>::new();
    for (service_entry_id, models) in &manifest.catalog_models {
        let entry_models = models_by_entry.entry(service_entry_id.clone()).or_default();
        for model in models {
            entry_models.insert(model.model_id.clone(), (model.display_name.clone(), true));
        }
    }
    for rule in &manifest.rule_instances {
        if rule.matcher.kind
            != vulcan_provider_adapters::provider_manifest::matcher::MatcherKind::Exact
        {
            continue;
        }
        let display_name = rule
            .display_name
            .clone()
            .unwrap_or_else(|| rule.matcher.value.clone());
        models_by_entry
            .entry(rule.service_entry_id.clone())
            .or_default()
            .entry(rule.matcher.value.clone())
            .or_insert((display_name, false));
    }

    // statuses contains one binding per exact connection/profile/model scope, or one static
    // scope for a profile without a configured connection.
    // statuses 为每个精确连接/配置档/模型作用域包含一条绑定；未配置连接的配置档保留一条静态作用域。
    let mut statuses = Vec::new();
    for entry in &manifest.descriptor.service_entries {
        if entry.kind == vulcan_provider_adapters::ServiceEntryKind::ProviderTools {
            continue;
        }
        let source_policy = manifest.model_source_policy(entry.id.as_str())?;
        let entry_defaults = manifest.model_defaults_for_entry(entry.id.as_str())?;
        let manifest_models = models_by_entry
            .get(entry.id.as_str())
            .cloned()
            .unwrap_or_default();
        // profileIds includes declared profiles plus any persisted profile so invalid drift is visible.
        // profileIds 包含已声明配置档以及全部持久化配置档，使无效漂移可见。
        let mut profile_ids = if entry.credential_profiles.is_empty() {
            BTreeSet::from([None])
        } else {
            entry
                .credential_profiles
                .iter()
                .map(|profile| Some(profile.id.clone()))
                .collect::<BTreeSet<_>>()
        };
        for connection in configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| {
                connection.provider_id == manifest.descriptor.id
                    && connection.service_entry_id == entry.id
            })
        {
            profile_ids.insert(Some(connection.credential_profile_id.clone()));
        }
        for credential_profile_id in profile_ids {
            // connections is stable by persisted connection identifier because the graph is a BTreeMap.
            // connections 按持久化连接标识稳定排序，因为连接图使用 BTreeMap。
            let connections = configuration
                .connection_graph
                .connections
                .values()
                .filter(|connection| {
                    connection.provider_id == manifest.descriptor.id
                        && connection.service_entry_id == entry.id
                        && credential_profile_id
                            .as_deref()
                            .is_none_or(|profile_id| connection.credential_profile_id == profile_id)
                })
                .collect::<Vec<_>>();
            let connection_ids = if connections.is_empty() {
                vec![None]
            } else {
                connections
                    .iter()
                    .map(|connection| Some(connection.id.as_str()))
                    .collect::<Vec<_>>()
            };
            for connection_id in connection_ids {
                let inventory = manifest_model_inventory_for_scope(
                    &manifest_models,
                    configuration,
                    manifest.descriptor.id.as_str(),
                    entry,
                    source_policy,
                    connection_id,
                    effective_catalogs,
                    model_support_catalogs,
                    source_catalogs,
                )?;
                for (model_id, facts) in inventory {
                    let rule = manifest.rule_for_model(
                        entry.id.as_str(),
                        credential_profile_id.as_deref(),
                        model_id.as_str(),
                    );
                    let supported_protocols = rule
                        .map(|rule| rule.supported_protocols.clone())
                        .unwrap_or_else(|| entry_defaults.supported_protocols.clone());
                    let operation_refs = rule
                        .map(|rule| rule.operation_refs.clone())
                        .unwrap_or_else(|| entry_defaults.operation_refs.clone());
                    // catalogModel follows the exact published overlay precedence.
                    // catalogModel 遵循精确的已发布覆盖优先级。
                    let catalog_model = facts
                        .effective_model
                        .as_ref()
                        .or(facts.support_model.as_ref())
                        .or(facts.source_model.as_ref());
                    let display_name = catalog_model
                        .map(|model| model.display_name.clone())
                        .or(facts.manifest_display_name.clone())
                        .unwrap_or_else(|| model_id.clone());
                    let available_protocols = facts
                        .source_model
                        .iter()
                        .chain(facts.support_model.iter())
                        .chain(facts.effective_model.iter())
                        .flat_map(|model| model.protocol_variants.keys().cloned())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>();
                    let model_source = resolved_manifest_model_source(&facts.source_layers)?;
                    let model_source_layers = facts.source_layers.iter().copied().collect();
                    let catalog_observed = catalog_model.is_some();
                    let catalog_enabled = facts.effective_model.is_some();
                    let capability_provenance = rule
                        .map(|rule| rule.capability_provenance.clone())
                        .unwrap_or_default();
                    let binding_result =
                        vulcan_provider_adapters::resolve_system_model_protocol_binding(
                            manifest,
                            entry.id.as_str(),
                            credential_profile_id.as_deref(),
                            model_id.as_str(),
                        );
                    let status = match binding_result {
                        Ok(binding) => {
                            // boundVariant is mandatory whenever a published catalog observes a
                            // system model; this mirrors request execution's fail-closed guard.
                            // boundVariant 在已发布目录观测到系统模型时必需；此处镜像请求执行的失败关闭护栏。
                            let bound_variant = catalog_model.and_then(|model| {
                                model.protocol_variants.get(&binding.protocol_profile_id)
                            });
                            let missing_bound_variant = catalog_observed && bound_variant.is_none();
                            let capabilities = bound_variant
                                .map(|variant| variant.capabilities.clone())
                                .or_else(|| catalog_model.map(|model| model.capabilities.clone()))
                                .or_else(|| rule.and_then(|rule| rule.capabilities.clone()))
                                .or_else(|| entry_defaults.capabilities.clone());
                            let capability_evidence = bound_variant
                                .map(|variant| variant.capability_evidence.clone())
                                .or_else(|| {
                                    catalog_model.map(|model| model.capability_evidence.clone())
                                });
                            ManifestModelBindingStatus {
                                connection_id: connection_id.map(str::to_string),
                                service_entry_id: entry.id.clone(),
                                credential_profile_id: credential_profile_id.clone(),
                                model_id: model_id.clone(),
                                display_name,
                                model_source,
                                model_source_layers,
                                supported_protocols,
                                available_protocols,
                                effective_protocol: Some(binding.protocol_profile_id.clone()),
                                binding_source: Some(String::from("system_manifest")),
                                binding_rule_id: (!binding.rule_id.is_empty())
                                    .then_some(binding.rule_id),
                                manifest_revision: binding.manifest_revision,
                                operation_refs,
                                capabilities,
                                capability_evidence,
                                capability_provenance,
                                catalog_observed,
                                catalog_enabled,
                                executable: !missing_bound_variant,
                                unavailable_reason: missing_bound_variant.then(|| {
                                    format!(
                                        "Model `{model_id}` has no capability variant for its bound protocol `{}`; execution is blocked.",
                                        binding.protocol_profile_id
                                    )
                                }),
                            }
                        }
                        Err(error) => ManifestModelBindingStatus {
                            connection_id: connection_id.map(str::to_string),
                            service_entry_id: entry.id.clone(),
                            credential_profile_id: credential_profile_id.clone(),
                            model_id: model_id.clone(),
                            display_name,
                            model_source,
                            model_source_layers,
                            supported_protocols,
                            available_protocols,
                            effective_protocol: None,
                            binding_source: None,
                            binding_rule_id: rule.map(|rule| rule.rule_id.clone()),
                            manifest_revision: manifest.revision,
                            operation_refs,
                            capabilities: catalog_model
                                .map(|model| model.capabilities.clone())
                                .or_else(|| rule.and_then(|rule| rule.capabilities.clone()))
                                .or_else(|| entry_defaults.capabilities.clone()),
                            capability_evidence: catalog_model
                                .map(|model| model.capability_evidence.clone()),
                            capability_provenance,
                            catalog_observed,
                            catalog_enabled,
                            executable: false,
                            unavailable_reason: Some(error.to_string()),
                        },
                    };
                    statuses.push(status);
                }
            }
        }
    }
    statuses.sort_by(|left, right| {
        left.service_entry_id
            .cmp(&right.service_entry_id)
            .then_with(|| left.credential_profile_id.cmp(&right.credential_profile_id))
            .then_with(|| left.connection_id.cmp(&right.connection_id))
            .then_with(|| left.model_id.cmp(&right.model_id))
    });
    Ok(statuses)
}

/// Builds one semantically complete bounded operation status from an active manifest.
/// 从 active Manifest 构建一份语义完整且有界的 operation 状态。
///
/// # Parameters
/// 参数
/// `manifest`, `operation`, and `model_bindings` share the same immutable generation.
/// `manifest`、`operation` 与 `model_bindings` 共享同一不可变代际。
///
/// # Returns
/// 返回值
/// Carrier, resources, MIME contracts, lifecycle, uploads, targets, and matching models.
/// 返回载体、资源、MIME 契约、生命周期、上传、目标与匹配模型。
fn manifest_operation_status(
    manifest: &vulcan_provider_adapters::ResolvedProviderManifest,
    operation: &vulcan_provider_adapters::provider_manifest::OperationDeclaration,
    model_bindings: &[ManifestModelBindingStatus],
) -> ManifestOperationStatus {
    // uploadChannelRefs includes both operation-wide and field-specific channel authority.
    // uploadChannelRefs 包含 operation 全局与字段专属通道权限。
    let upload_channel_refs = operation
        .upload_channel_refs
        .iter()
        .chain(
            operation
                .upload_bindings
                .iter()
                .flat_map(|binding| binding.upload_channel_refs.iter()),
        )
        .cloned()
        .collect::<BTreeSet<_>>();
    // inputMimeTypes merges closed upload-channel and binding-level MIME declarations.
    // inputMimeTypes 合并封闭上传通道与绑定级 MIME 声明。
    let mut input_mime_types = manifest
        .upload_channels
        .iter()
        .filter(|channel| upload_channel_refs.contains(&channel.upload_channel_id))
        .flat_map(|channel| channel.accepted_mime_types.iter().cloned())
        .collect::<BTreeSet<_>>();
    for binding in &operation.upload_bindings {
        input_mime_types.extend(binding.mime_type.iter().cloned());
        input_mime_types.extend(binding.mime_types_by_extension.values().cloned());
    }
    // targetRefs is the exact provider-tool target closure owned by this operation.
    // targetRefs 是此 operation 持有的精确供应商工具目标闭包。
    let target_refs = operation
        .tool_target_refs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    // outputMimeTypes merges the exact output contracts of every referenced target.
    // outputMimeTypes 合并每个引用目标的精确输出契约。
    let mut output_mime_types = BTreeSet::new();
    for target in manifest
        .tool_targets
        .iter()
        .filter(|target| target_refs.contains(&target.target_id))
    {
        output_mime_types.extend(target.contract.output.mime_types.iter().cloned());
        for decoder in &target.contract.output.resource_decoders {
            output_mime_types.extend(decoder.mime_type.iter().cloned());
            output_mime_types.extend(decoder.mime_types_by_request_value.values().cloned());
        }
    }
    // matchingModels collapses credential-profile duplicates into concrete service/model identities.
    // matchingModels 把凭据配置档重复项折叠为具体服务/模型身份。
    let mut matching_models = model_bindings
        .iter()
        .filter(|binding| {
            binding
                .operation_refs
                .iter()
                .any(|reference| reference == &operation.operation_id)
        })
        .map(|binding| ManifestOperationMatchingModelStatus {
            service_entry_id: binding.service_entry_id.clone(),
            model_id: binding.model_id.clone(),
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    // matchingModelTotalCount preserves the full count before bounded detail truncation.
    // matchingModelTotalCount 保留有界详情截断前的完整数量。
    let matching_model_total_count = matching_models.len();
    matching_models.truncate(MANIFEST_STATUS_OPERATION_MODEL_MAX_ITEMS);
    // matchingModelsTruncated distinguishes complete and bounded summaries.
    // matchingModelsTruncated 区分完整摘要与有界摘要。
    let matching_models_truncated = matching_models.len() < matching_model_total_count;
    // asyncExecution is the frozen lifecycle contract exposed without UI-side inference.
    // asyncExecution 是直接公开的冻结生命周期契约，避免 UI 侧推断。
    let async_execution = operation.async_execution.as_ref();
    ManifestOperationStatus {
        operation_id: operation.operation_id.clone(),
        operation: operation.operation.clone(),
        carrier: operation.carrier,
        input_resources: operation.input_resources.clone(),
        output_resources: operation.output_resources.clone(),
        input_mime_types: input_mime_types.into_iter().collect(),
        output_mime_types: output_mime_types.into_iter().collect(),
        request_profile_refs: operation.request_profile_refs.clone(),
        response_profile_refs: operation.response_profile_refs.clone(),
        upload_channel_refs: upload_channel_refs.into_iter().collect(),
        tool_target_refs: operation.tool_target_refs.clone(),
        tool_target_template_refs: operation.tool_target_template_refs.clone(),
        asynchronous: async_execution.is_some(),
        poll_interval_ms: async_execution.map(|async_| async_.poll_interval_ms),
        max_backoff_ms: async_execution.map(|async_| async_.max_backoff_ms),
        cancellation_supported: async_execution.is_some_and(|async_| async_.cancellation_supported),
        timeout_ms: operation
            .timeout_ms
            .or_else(|| async_execution.map(|async_| async_.timeout_ms)),
        matching_models,
        matching_model_total_count,
        matching_models_truncated,
    }
}

/// Builds every deterministic operation status in one immutable manifest generation.
/// 构建一个不可变 Manifest 代际中的全部确定性 operation 状态。
///
/// # Parameters
/// 参数
/// `manifest` and `model_bindings` share the same compiled generation.
/// `manifest` 与 `model_bindings` 共享同一编译代际。
///
/// # Returns
/// 返回值
/// Operation statuses in stable operation-id order.
/// 返回按稳定 operation 标识排序的状态。
fn manifest_operation_statuses(
    manifest: &vulcan_provider_adapters::ResolvedProviderManifest,
    model_bindings: &[ManifestModelBindingStatus],
) -> Vec<ManifestOperationStatus> {
    // statuses contains the complete bounded manifest-owned operation inventory.
    // statuses 包含完整且有界的 Manifest 自有 operation 清单。
    let mut statuses = manifest
        .operations
        .iter()
        .map(|operation| manifest_operation_status(manifest, operation, model_bindings))
        .collect::<Vec<_>>();
    statuses.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    statuses
}

/// Removes generation-owned static Manifest catalogs before comparing two Manifest contracts.
/// 在比较两个 Manifest 契约前移除由代际持有的静态 Manifest 目录。
///
/// Static identities must be reconstructed independently from each compared Manifest. Reusing
/// the active static catalog for the baseline would make newly added models appear on both sides
/// and hide additions or retirements. Discovery and manual catalogs remain valid external evidence
/// shared by both semantic projections.
/// 静态身份必须分别从参与比较的两份 Manifest 重建。若基线复用 active 静态目录，新模型会
/// 同时出现在两侧，从而掩盖新增或退役；发现与手工目录仍作为两侧共享的有效外部证据保留。
///
/// # Parameters
/// 参数
/// `catalogs` is one coherent catalog layer captured with the active runtime generation.
/// `catalogs` 是与 active 运行时代际共同捕获的一层一致目录。
///
/// # Returns
/// 返回值
/// A deterministic catalog map containing only discovery/manual evidence.
/// 返回仅包含发现/手工证据的确定性目录映射。
fn manifest_binding_diff_external_catalogs(
    catalogs: &BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
) -> BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog> {
    catalogs
        .iter()
        .filter(|(_connection_id, catalog)| {
            manifest_catalog_source_layer(catalog) != ManifestModelSourceStatus::Manifest
        })
        .map(|(connection_id, catalog)| (connection_id.clone(), catalog.clone()))
        .collect()
}

/// Normalizes generation-only provenance before semantic model-binding comparison.
/// 在模型绑定语义比较前归一化仅属于代际的来源信息。
///
/// Manifest revision numbers prove which immutable generation produced a binding, but a pure
/// revision bump is not a model behavior change. Evidence revisions, rule identities, overridden
/// source order, routing, capabilities, availability, and execution state remain significant.
/// Manifest 修订号用于证明绑定来自哪个不可变代际，但纯修订升级并非模型行为变化。证据
/// 修订、规则身份、被覆盖来源顺序、路由、能力、可用性与执行状态仍参与比较。
///
/// # Parameters
/// 参数
/// `binding` is one complete backend-resolved status.
/// `binding` 是一条完整的后端解析状态。
///
/// # Returns
/// 返回值
/// A clone whose generation-only revision fields are replaced by a stable sentinel.
/// 返回把仅属于代际的修订字段替换为稳定哨兵后的克隆。
fn normalized_manifest_model_binding_semantics(
    binding: &ManifestModelBindingStatus,
) -> ManifestModelBindingStatus {
    // normalized preserves every effective fact while removing only generation identity.
    // normalized 保留全部有效事实，仅移除代际身份。
    let mut normalized = binding.clone();
    normalized.manifest_revision = 0;
    for provenance in normalized.capability_provenance.values_mut() {
        provenance.manifest_revision = 0;
        for overridden in &mut provenance.overridden_sources {
            overridden.manifest_revision = 0;
        }
    }
    normalized
}

/// Computes concrete model/profile binding deltas between two compiled generations.
/// 计算两个编译代际之间的具体模型/配置档绑定差异。
///
/// # Parameters
/// 参数
/// `current` and `candidate` are complete backend-resolved binding lists.
/// `current` 与 `candidate` 是完整的后端解析绑定列表。
///
/// # Returns
/// 返回值
/// A bounded delta list, complete count, and explicit truncation flag.
/// 返回有界差异列表、完整数量与显式截断标志。
fn diff_manifest_model_bindings(
    current: &[ManifestModelBindingStatus],
    candidate: &[ManifestModelBindingStatus],
) -> (Vec<ProviderManifestModelBindingDelta>, usize, bool) {
    // currentByKey indexes every exact service/profile/connection/model identity.
    // currentByKey 索引每个精确服务/配置档/连接/模型身份。
    let current_by_key = current
        .iter()
        .map(|binding| {
            (
                (
                    binding.service_entry_id.clone(),
                    binding.credential_profile_id.clone(),
                    binding.connection_id.clone(),
                    binding.model_id.clone(),
                ),
                binding,
            )
        })
        .collect::<BTreeMap<_, _>>();
    // candidateByKey indexes the same identity space for the verified candidate.
    // candidateByKey 为已验证候选索引相同身份空间。
    let candidate_by_key = candidate
        .iter()
        .map(|binding| {
            (
                (
                    binding.service_entry_id.clone(),
                    binding.credential_profile_id.clone(),
                    binding.connection_id.clone(),
                    binding.model_id.clone(),
                ),
                binding,
            )
        })
        .collect::<BTreeMap<_, _>>();
    // keys is the deterministic union required to classify additions, removals, and changes.
    // keys 是分类新增、删除与变更所需的确定性并集。
    let keys = current_by_key
        .keys()
        .chain(candidate_by_key.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    // deltas excludes identities whose full effective status stayed equal.
    // deltas 排除完整有效状态保持相等的身份。
    let mut deltas = Vec::new();
    for key in keys {
        // currentBinding and candidateBinding are the exact optional sides of one identity.
        // currentBinding 与 candidateBinding 是一个身份精确的可选两侧。
        let current_binding = current_by_key.get(&key).copied();
        let candidate_binding = candidate_by_key.get(&key).copied();
        // change is absent only when both immutable statuses are equal.
        // change 仅在两个不可变状态相等时缺失。
        let change = match (current_binding, candidate_binding) {
            (None, Some(_)) => Some(ProviderManifestChangeKind::Added),
            (Some(_), None) => Some(ProviderManifestChangeKind::Removed),
            (Some(left), Some(right))
                if normalized_manifest_model_binding_semantics(left)
                    != normalized_manifest_model_binding_semantics(right) =>
            {
                Some(ProviderManifestChangeKind::Changed)
            }
            _ => None,
        };
        if let Some(change) = change {
            deltas.push(ProviderManifestModelBindingDelta {
                change,
                current: current_binding.cloned(),
                candidate: candidate_binding.cloned(),
            });
        }
    }
    // totalCount reports the exact delta cardinality before response bounding.
    // totalCount 报告响应限界前的精确差异数量。
    let total_count = deltas.len();
    deltas.truncate(MANIFEST_STATUS_MODEL_BINDING_MAX_ITEMS);
    // truncated prevents the preview from claiming completeness after bounding.
    // truncated 防止预览在限界后声称完整。
    let truncated = deltas.len() < total_count;
    (deltas, total_count, truncated)
}

/// Computes semantic operation deltas between two compiled generations.
/// 计算两个编译代际之间的语义 operation 差异。
///
/// # Parameters
/// 参数
/// `current` and `candidate` contain complete operation status inventories.
/// `current` 与 `candidate` 包含完整 operation 状态清单。
///
/// # Returns
/// 返回值
/// A bounded delta list, complete count, and explicit truncation flag.
/// 返回有界差异列表、完整数量与显式截断标志。
fn diff_manifest_operations(
    current: &[ManifestOperationStatus],
    candidate: &[ManifestOperationStatus],
) -> (Vec<ProviderManifestOperationDelta>, usize, bool) {
    // currentById and candidateById use the stable operation id as the sole identity key.
    // currentById 与 candidateById 使用稳定 operation 标识作为唯一身份键。
    let current_by_id = current
        .iter()
        .map(|operation| (operation.operation_id.clone(), operation))
        .collect::<BTreeMap<_, _>>();
    let candidate_by_id = candidate
        .iter()
        .map(|operation| (operation.operation_id.clone(), operation))
        .collect::<BTreeMap<_, _>>();
    // ids is the deterministic union of both semantic inventories.
    // ids 是两个语义清单的确定性并集。
    let ids = current_by_id
        .keys()
        .chain(candidate_by_id.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    // deltas excludes byte-independent operation statuses that stayed semantically equal.
    // deltas 排除保持语义相等且与字节无关的 operation 状态。
    let mut deltas = Vec::new();
    for operation_id in ids {
        // currentOperation and candidateOperation are the exact optional sides of one identity.
        // currentOperation 与 candidateOperation 是一个身份精确的可选两侧。
        let current_operation = current_by_id.get(&operation_id).copied();
        let candidate_operation = candidate_by_id.get(&operation_id).copied();
        // change is absent only when both immutable statuses are equal.
        // change 仅在两个不可变状态相等时缺失。
        let change = match (current_operation, candidate_operation) {
            (None, Some(_)) => Some(ProviderManifestChangeKind::Added),
            (Some(_), None) => Some(ProviderManifestChangeKind::Removed),
            (Some(left), Some(right)) if left != right => Some(ProviderManifestChangeKind::Changed),
            _ => None,
        };
        if let Some(change) = change {
            deltas.push(ProviderManifestOperationDelta {
                change,
                current: current_operation.cloned(),
                candidate: candidate_operation.cloned(),
            });
        }
    }
    // totalCount reports the exact delta cardinality before response bounding.
    // totalCount 报告响应限界前的精确差异数量。
    let total_count = deltas.len();
    deltas.truncate(MANIFEST_PREVIEW_OPERATION_CHANGE_MAX_ITEMS);
    // truncated prevents the preview from claiming completeness after bounding.
    // truncated 防止预览在限界后声称完整。
    let truncated = deltas.len() < total_count;
    (deltas, total_count, truncated)
}

/// Truncates one diagnostic on a UTF-8 boundary before it crosses the public response bound.
/// 在诊断超过公开响应上限前按 UTF-8 边界截断。
///
/// # Parameters
/// 参数
/// `diagnostic` is a non-secret local manifest verification or projection error.
/// `diagnostic` 是非秘密的本地 Manifest 校验或投影错误。
///
/// # Returns
/// 返回值
/// The original text or a boundary-safe ellipsis-terminated prefix.
/// 返回原文本，或按边界安全且以省略号结尾的前缀。
pub(crate) fn bounded_public_manifest_diagnostic(diagnostic: &str) -> String {
    // lowercase exposes URL and credential shapes without retaining the original casing.
    // lowercase 在不保留原始大小写的情况下暴露 URL 与凭据形态。
    let lowercase = diagnostic.to_ascii_lowercase();
    if lowercase.contains("://") {
        return String::from("<redacted-url>");
    }
    if diagnostic_contains_secret_assignment(lowercase.as_str()) {
        return String::from("<redacted>");
    }
    if diagnostic_contains_absolute_path(diagnostic) {
        return String::from("<redacted-local-path>");
    }
    if diagnostic.len() <= MANIFEST_PREVIEW_DIAGNOSTIC_MAX_BYTES {
        return diagnostic.to_string();
    }
    // boundary retreats until the byte index is a valid UTF-8 character boundary.
    // boundary 后退直到字节索引成为有效 UTF-8 字符边界。
    let mut boundary = MANIFEST_PREVIEW_DIAGNOSTIC_MAX_BYTES.saturating_sub(3);
    while boundary > 0 && !diagnostic.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}...", &diagnostic[..boundary])
}

/// Detects plain-text, header-style, or JSON-style secret assignments in one diagnostic.
/// 检测诊断中的纯文本、Header 风格或 JSON 风格秘密赋值。
///
/// # Parameters
/// 参数
/// `lowercase` is the ASCII-lowercased diagnostic used only for shape detection.
/// `lowercase` 是仅用于形态检测的 ASCII 小写诊断。
///
/// # Returns
/// 返回值
/// `true` when a sensitive key is followed by an assignment delimiter or an auth scheme appears.
/// 敏感键后出现赋值分隔符或认证方案时返回 `true`。
fn diagnostic_contains_secret_assignment(lowercase: &str) -> bool {
    if lowercase.contains("bearer ") || lowercase.contains("basic ") {
        return true;
    }
    [
        "authorization",
        "api_key",
        "api-key",
        "apikey",
        "secret",
        "token",
        "signature",
        "password",
    ]
    .iter()
    .any(|sensitive_key| {
        lowercase.match_indices(sensitive_key).any(|(index, _)| {
            lowercase[index + sensitive_key.len()..]
                .trim_start_matches(|character: char| {
                    character.is_ascii_whitespace() || matches!(character, '"' | '\'' | '`')
                })
                .starts_with([':', '='])
        })
    })
}

/// Detects embedded Windows, UNC, or Unix absolute-path shapes in a public diagnostic.
/// 检测公开诊断中嵌入的 Windows、UNC 或 Unix 绝对路径形态。
///
/// # Parameters
/// 参数
/// `diagnostic` is a bounded local error before public projection.
/// `diagnostic` 是公开投影前的有界本地错误。
///
/// # Returns
/// 返回值
/// `true` when any token can disclose a machine-specific absolute path.
/// 任一词元可能泄露机器相关绝对路径时返回 `true`。
fn diagnostic_contains_absolute_path(diagnostic: &str) -> bool {
    let bytes = diagnostic.as_bytes();
    let windows_path = bytes.windows(3).any(|window| {
        window[0].is_ascii_alphabetic() && window[1] == b':' && matches!(window[2], b'\\' | b'/')
    });
    let unix_path = diagnostic
        .split(|character: char| {
            character.is_ascii_whitespace()
                || matches!(
                    character,
                    '`' | '"' | '\'' | '(' | '[' | '{' | '=' | ':' | ','
                )
        })
        .any(|token| token.len() > 1 && token.starts_with('/'));
    windows_path || diagnostic.contains("\\\\") || unix_path
}

/// Builds one candidate preview whose verification stopped before semantic diff generation.
/// 构建一份在生成语义差异前已停止校验的候选预览。
///
/// # Parameters
/// 参数
/// The parameters are the exact provider/current/candidate identities and closed failure facts.
/// 参数为精确供应商/当前/候选身份与封闭失败事实。
///
/// # Returns
/// 返回值
/// A complete typed preview with empty, explicitly complete delta sets.
/// 返回具有空且明确完整差异集合的完整类型化预览。
// Manifest reconciliation distinguishes declaration, connection and catalog authorities.
// 清单协调区分声明、连接和目录权威。
#[expect(
    clippy::too_many_arguments,
    reason = "Manifest reconciliation distinguishes declaration, connection and catalog authorities."
)]
fn manifest_preview_without_diffs(
    provider_id: &str,
    outcome: ProviderManifestPreviewOutcome,
    current_revision: Option<u64>,
    candidate_revision: Option<u64>,
    candidate_sha256: Option<String>,
    executable: bool,
    failure_kind: Option<String>,
    failure_message: Option<String>,
) -> ProviderManifestCandidatePreview {
    ProviderManifestCandidatePreview {
        provider_id: provider_id.to_string(),
        outcome,
        current_revision,
        candidate_revision,
        candidate_sha256,
        executable,
        failure_kind,
        failure_message,
        model_binding_changes: Vec::new(),
        model_binding_change_total_count: 0,
        model_binding_changes_truncated: false,
        operation_changes: Vec::new(),
        operation_change_total_count: 0,
        operation_changes_truncated: false,
    }
}

/// One typed explicit-rollback report.
/// 一份 typed 显式回滚报告。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderRollbackReport {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Revision restored from the previous layer.
    /// 从 previous 层恢复的修订号。
    pub restored_revision: u64,
    /// Non-serialized exact before-image retained until projection publication commits.
    /// 保留到投影发布提交、且不序列化的精确前镜像。
    #[serde(skip)]
    pub(crate) activation_snapshot: ProviderManifestActivationSnapshot,
}

/// Exact disk/runtime before-image used to compensate a failed projection publication.
/// 用于补偿投影发布失败的精确磁盘/运行时前镜像。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderManifestActivationSnapshot {
    /// Exact system provider owning the snapshot.
    /// 拥有该快照的精确系统供应商。
    pub(crate) provider_id: String,
    /// Exact Active, Previous, sync-state, and check-state bytes before activation.
    /// 激活前精确的 Active、Previous、同步状态与检查状态字节。
    pub(crate) store: vulcan_provider_config::manifest_store::SystemManifestStoreSnapshot,
    /// Exact staged runtime Manifest-catalog mutation, including both generation images.
    /// 精确的暂存运行时 Manifest 目录变更，包含前后两个代次镜像。
    pub(crate) staged_manifest_mutation:
        vulcan_provider_adapters::ProviderStagedManifestCatalogMutation,
}

/// Runs the signed remote manifest sync for the production runtime.
/// 为生产运行时执行签名远端清单同步。
///
/// The remote index URL and trusted signing keys are injected by the host; until they are
/// configured every sync reports `not_configured` (external confirmation gate, plan §7.2).
/// 远端 Index URL 与受信签名密钥由宿主注入；配置前每次同步报告 `not_configured`
/// （外部确认门，方案 §7.2）。
#[derive(Clone)]
pub struct ProviderManifestSyncService {
    store: ManifestStoreLayout,
    http: reqwest::Client,
    trusted_keys: Vec<TrustedSigningKey>,
    remote_index_url: Option<String>,
    /// Global single-flight guard serializing index fetches (plan §R7.8).
    /// 串行化 Index 获取的全局 single-flight 守卫（方案 §R7.8）。
    index_flight: Arc<tokio::sync::Mutex<()>>,
    /// Per-provider single-flight guards preventing duplicate activations (plan §R7.8).
    /// 防止重复激活的逐供应商 single-flight 守卫（方案 §R7.8）。
    provider_flights:
        Arc<tokio::sync::Mutex<std::collections::BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    /// Global single-writer guard spanning download, activation, projection, and compensation.
    /// 覆盖下载、激活、投影与补偿的全局单写者守卫。
    activation_flight: Arc<tokio::sync::Mutex<()>>,
    /// Abort handle for the single service-owned startup/periodic synchronization task.
    /// 服务持有的唯一启动/周期同步任务取消句柄。
    periodic_task_abort: Arc<std::sync::Mutex<Option<tokio::task::AbortHandle>>>,
}

/// Restores one staged explicit rollback when its asynchronous projection future is cancelled.
/// 在显式回滚的异步投影 Future 被取消时恢复该暂存回滚。
struct ProviderRollbackCancellationGuard<'a> {
    /// Service owning durable and staged-runtime rollback compensation.
    /// 持有持久化与暂存运行时回滚补偿的服务。
    service: &'a ProviderManifestSyncService,
    /// Runtime owning the staged manifest catalog.
    /// 持有暂存 Manifest 目录的运行时。
    runtime: &'a PublicAiRuntime,
    /// Exact rollback report containing disk/runtime before-images.
    /// 包含磁盘与运行时精确前镜像的回滚报告。
    report: &'a ProviderRollbackReport,
    /// Armed state retained only while asynchronous cancellation remains possible.
    /// 仅在仍可能发生异步取消时保持的启用状态。
    armed: bool,
}

impl<'a> ProviderRollbackCancellationGuard<'a> {
    /// Arms cancellation compensation for one exact staged rollback.
    /// 为一次精确已暂存回滚启用取消补偿。
    ///
    /// # Parameters
    /// 参数
    /// `service`, `runtime`, and `report` form the complete rollback authority.
    /// `service`、`runtime` 与 `report` 构成完整回滚权威。
    ///
    /// # Returns
    /// 返回值
    /// An armed guard that aborts the rollback if dropped before `disarm`.
    /// 返回一个已启用护栏；若在 `disarm` 前被丢弃则中止回滚。
    fn new(
        service: &'a ProviderManifestSyncService,
        runtime: &'a PublicAiRuntime,
        report: &'a ProviderRollbackReport,
    ) -> Self {
        Self {
            service,
            runtime,
            report,
            armed: true,
        }
    }

    /// Disarms automatic compensation after every asynchronous cancellation point has completed.
    /// 在全部异步取消点完成后停用自动补偿。
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ProviderRollbackCancellationGuard<'_> {
    /// Aborts the staged rollback on asynchronous cancellation.
    /// 在异步取消时中止已暂存回滚。
    fn drop(&mut self) {
        if self.armed {
            // abortResult is best-effort in Drop; a failed runtime restore keeps the Applying
            // journal for deterministic startup recovery.
            // abortResult 在 Drop 中采用尽力恢复；运行时恢复失败时保留 Applying 日志，
            // 供启动确定性恢复。
            let _abort_result = self.service.abort_rollback(self.runtime, self.report);
        }
    }
}

impl ProviderManifestSyncService {
    /// Builds one sync service with the host-owned store root and dedicated secure HTTP client.
    /// 使用宿主持有的存储根与专用安全 HTTP 客户端构建一个同步服务。
    ///
    /// # Parameters
    /// 参数
    /// `app_data_root` is the application data directory; `http` is the shared configured
    /// client used only as the base for the dedicated secure client (redirects disabled,
    /// bounded timeouts, plan §R7.4); `trusted_keys` are the bundled signing keys;
    /// `remote_index_url` is the reviewed HTTPS index endpoint, or `None` until the external
    /// gate opens.
    /// `app_data_root` 是应用数据目录；`http` 是注入的已配置客户端（生产接线传入专用
    /// 安全客户端，禁用重定向、有界超时，方案 §R7.4；测试可注入回环 TLS 客户端）；
    /// `trusted_keys` 是内置签名密钥；`remote_index_url` 是已审核的 HTTPS Index 端点，
    /// 外部门开启前为 `None`。
    pub fn new(
        app_data_root: &Path,
        http: reqwest::Client,
        trusted_keys: Vec<TrustedSigningKey>,
        remote_index_url: Option<String>,
    ) -> Self {
        // The HTTP client is injected so local E2E tests can serve the signing server over a
        // loopback TLS endpoint; production wiring passes the dedicated secure client
        // (plan §R7.13).
        // HTTP client 由外部注入，使本地 E2E 测试可以在回环 TLS 端点上提供签名服务器；
        // 生产接线传入专用安全客户端（方案 §R7.13）。
        Self {
            store: ManifestStoreLayout::new(app_data_root),
            http,
            trusted_keys,
            remote_index_url,
            index_flight: Arc::new(tokio::sync::Mutex::new(())),
            provider_flights: Arc::new(tokio::sync::Mutex::new(std::collections::BTreeMap::new())),
            activation_flight: Arc::new(tokio::sync::Mutex::new(())),
            periodic_task_abort: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Reports whether the host injected both a remote index URL and trusted signing keys.
    /// 报告宿主是否同时注入远端 Index URL 与受信签名密钥。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when an automatic synchronization task may make a meaningful attempt.
    /// 仅当自动同步任务可以进行有效尝试时返回 `true`。
    pub(crate) fn is_configured(&self) -> bool {
        self.remote_index_url.is_some() && !self.trusted_keys.is_empty()
    }

    /// Returns one conditional Index ETag only when every requested provider observed the same value.
    /// 仅当每个请求供应商都观察到同一值时返回一个条件式 Index ETag。
    ///
    /// Corrupted, absent, or divergent per-provider state disables this optimization; the normal
    /// provider-scoped processing path later reports each exact local-state failure independently.
    /// 逐供应商状态损坏、缺失或不一致时禁用该优化；常规逐供应商处理路径随后会独立报告
    /// 每个精确本地状态错误。
    ///
    /// # Parameters
    /// 参数
    /// `provider_ids` is the complete deterministic full-catalog synchronization target set.
    /// `provider_ids` 是完整且确定性的全目录同步目标集合。
    ///
    /// # Returns
    /// 返回值
    /// The shared validated ETag, or `None` when an unconditional authenticated Index fetch is required.
    /// 返回共享且已校验的 ETag；需要无条件认证 Index 获取时返回 `None`。
    fn shared_index_etag(&self, provider_ids: &[String]) -> Option<String> {
        let mut shared = None::<String>;
        for provider_id in provider_ids {
            let SyncStateLoad::Present(state) = self.store.load_sync_state(provider_id).ok()?
            else {
                return None;
            };
            let candidate = state.index_etag?;
            if shared.as_ref().is_some_and(|current| current != &candidate) {
                return None;
            }
            shared = Some(candidate);
        }
        shared
    }

    /// Returns one conditional Index ETag only when every provider also has a trusted Active layer.
    /// 仅当每个供应商同时拥有可信 Active 层时返回条件式 Index ETag。
    ///
    /// A batch-level HTTP 304 carries no signed body. Therefore the request may be conditional
    /// only when every provider can already prove the complete local before-image that the shared
    /// validator represents. Otherwise the batch performs one unconditional signed Index fetch.
    /// 批量 HTTP 304 不携带签名正文。因此，仅当每个供应商都能证明共享验证器所代表的完整
    /// 本地前镜像时，请求才允许使用条件头；否则整批只执行一次无条件签名 Index 获取。
    ///
    /// # Parameters
    /// 参数
    /// `provider_ids` is the complete deterministic provider set for this batch.
    /// `provider_ids` 是本批次完整且确定的供应商集合。
    ///
    /// # Returns
    /// 返回值
    /// The shared validator for complete local state, or `None` to force one authenticated fetch.
    /// 返回完整本地状态对应的共享验证器，或返回 `None` 以强制一次认证获取。
    fn shared_revalidated_index_etag(&self, provider_ids: &[String]) -> Option<String> {
        // sharedEtag first proves every provider persisted the same authenticated Index identity.
        // sharedEtag 首先证明每个供应商都持久化了相同的已认证 Index 身份。
        let shared_etag = self.shared_index_etag(provider_ids)?;
        for provider_id in provider_ids {
            // state is loaded again deliberately: the Active proof below must use the exact
            // revision and digest paired with the conditional request decision.
            // state 被刻意再次加载：下方 Active 证明必须使用与条件请求决策配对的精确修订与摘要。
            let SyncStateLoad::Present(state) = self.store.load_sync_state(provider_id).ok()?
            else {
                return None;
            };
            self.local_active_is_current(provider_id, &state.revision, &state.sha256)
                .ok()?;
        }
        Some(shared_etag)
    }

    /// Enters the single-writer transaction boundary shared by every synchronization trigger.
    /// 进入所有同步触发器共享的单写者事务边界。
    ///
    /// # Returns
    /// 返回值
    /// An owned guard that must stay alive through manifest projection or compensation.
    /// 返回必须持续持有到清单投影或补偿完成的所有权守卫。
    pub(crate) async fn begin_activation_transaction(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.activation_flight).lock_owned().await
    }

    /// Emits committed activation events only after every derived projection is visible.
    /// 仅在每个派生投影均可见后发出已提交激活事件。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the committed catalog generation and `reports` identify staged providers.
    /// `runtime` 持有已提交目录代际，`reports` 标识已暂存供应商。
    pub(crate) fn record_committed_activation_events(
        &self,
        runtime: &PublicAiRuntime,
        reports: &[ProviderSyncReport],
    ) {
        let generation = runtime.catalog_generation();
        for report in reports
            .iter()
            .filter(|report| report.outcome == "activated")
        {
            let Some(manifest) = runtime.system_manifest(report.provider_id.as_str()) else {
                eprintln!(
                    "[provider-manifest-sync] committed telemetry skipped missing runtime provider `{}`",
                    report.provider_id
                );
                continue;
            };
            let event = manifest_activation_terminal_event(
                report.provider_id.as_str(),
                report.revision,
                generation,
                ManifestActivationTelemetryTerminal::Activated {
                    source: manifest_source_layer_label(manifest.snapshot_provenance.source_layer),
                    sha256: manifest.snapshot_provenance.sha256.as_deref(),
                },
            );
            emit_manifest_operational_event(event);
        }
    }

    /// Emits compensated rollback events after every captured before-image has been restored.
    /// 在每个捕获的前镜像均已恢复后发出已补偿回滚事件。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the restored generation; `reports` identify staged providers and
    /// `failure_kind` is the stable post-activation failure category.
    /// `runtime` 持有已恢复代际；`reports` 标识已暂存供应商，`failure_kind` 是稳定激活后
    /// 失败分类。
    pub(crate) fn record_compensated_rollback_events(
        &self,
        runtime: &PublicAiRuntime,
        reports: &[ProviderSyncReport],
        failure_kind: &str,
    ) {
        let generation = runtime.catalog_generation();
        for report in reports
            .iter()
            .filter(|report| report.outcome == "activated")
        {
            let event = manifest_activation_terminal_event(
                report.provider_id.as_str(),
                report.revision,
                generation,
                ManifestActivationTelemetryTerminal::RolledBack { failure_kind },
            );
            emit_manifest_operational_event(event);
        }
    }

    /// Atomically records the completed check result represented by a batch of sync reports.
    /// 原子记录一批同步报告所表示的已完成检查结果。
    ///
    /// `activated` is accepted only after callers have published every runtime and Workbench
    /// projection. `not_configured` performs no remote check and therefore creates no record.
    /// 仅当调用方已发布全部运行时与 Workbench 投影后才接受 `activated`；
    /// `not_configured` 未执行远端检查，因此不创建记录。
    ///
    /// # Parameters
    /// 参数
    /// `reports` are the exact provider-scoped outcomes from one serialized sync transaction.
    /// `reports` 是一次串行同步事务产生的精确逐供应商结果。
    ///
    /// # Returns
    /// 返回值
    /// Success only after all check-state files publish, or an error after exact compensation.
    /// 仅当全部检查状态文件发布后返回成功；失败时精确补偿后返回错误。
    pub(crate) fn record_sync_check_reports(
        &self,
        reports: &[ProviderSyncReport],
    ) -> Result<(), String> {
        // entries translates closed report outcomes into the independent check-state contract.
        // entries 把封闭报告结果转换为独立检查状态契约。
        let mut entries = Vec::with_capacity(reports.len());
        for report in reports {
            // outcome preserves the exact completed remote-check result.
            // outcome 保留精确的已完成远端检查结果。
            let (outcome, failure_kind) = match report.outcome {
                "not_configured" => continue,
                "not_pinned" => (ManifestCheckOutcome::NotPinned, None),
                "up_to_date" => (ManifestCheckOutcome::UpToDate, None),
                "activated" => (ManifestCheckOutcome::Activated, None),
                "rejected" => (
                    ManifestCheckOutcome::Rejected,
                    Some(
                        report
                            .failure_kind
                            .clone()
                            .unwrap_or_else(|| String::from("sync_failed")),
                    ),
                ),
                unknown => {
                    return Err(format!(
                        "Provider `{}` returned unknown sync outcome `{unknown}`.",
                        report.provider_id
                    ));
                }
            };
            entries.push((report.provider_id.clone(), outcome, failure_kind));
        }
        self.record_sync_check_entries(&entries)
    }

    /// Atomically records one classified failure for every provider targeted by a failed batch.
    /// 为失败批次针对的每个供应商原子记录一个分类失败。
    ///
    /// # Parameters
    /// 参数
    /// `provider_ids` are exact system providers and `failure_kind` is a bounded stable id.
    /// `provider_ids` 是精确系统供应商，`failure_kind` 是有界稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// Success only after every provider observes the same finished-check timestamp transaction.
    /// 仅当每个供应商观察到同一完成检查时间事务后返回成功。
    pub(crate) fn record_sync_check_failures(
        &self,
        provider_ids: &[String],
        failure_kind: &str,
    ) -> Result<(), String> {
        // entries closes the free-form service error into one stable non-secret classification.
        // entries 把自由文本服务错误封闭为一个稳定非秘密分类。
        let entries = provider_ids
            .iter()
            .map(|provider_id| {
                (
                    provider_id.clone(),
                    ManifestCheckOutcome::Failed,
                    Some(failure_kind.to_string()),
                )
            })
            .collect::<Vec<_>>();
        self.record_sync_check_entries(&entries)
    }

    /// Publishes an exact batch of independent check-state entries with before-image rollback.
    /// 使用前镜像回滚发布一批精确的独立检查状态条目。
    ///
    /// # Parameters
    /// 参数
    /// `entries` contain provider id, closed outcome, and optional stable failure kind.
    /// `entries` 包含供应商标识、封闭结果与可选稳定失败类型。
    ///
    /// # Returns
    /// 返回值
    /// Success after all writes, or a combined primary/compensation diagnostic.
    /// 全部写入后返回成功，或返回合并的主错误与补偿诊断。
    fn record_sync_check_entries(
        &self,
        entries: &[(String, ManifestCheckOutcome, Option<String>)],
    ) -> Result<(), String> {
        // snapshots are captured for every target before the first write.
        // snapshots 在首次写入前为每个目标完成捕获。
        let snapshots = entries
            .iter()
            .map(|(provider_id, _outcome, _failure_kind)| {
                self.store
                    .capture_check_state(provider_id)
                    .map(|snapshot| (provider_id.clone(), snapshot))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // checkedAt is shared by the whole batch and cannot move behind a retained timestamp.
        // checkedAt 由整个批次共享，且不能落后于已保留时间戳。
        let checked_at_unix_ms = current_unix_millis();
        for (provider_id, outcome, failure_kind) in entries {
            if let Err(primary) = self.write_sync_check_state(
                provider_id,
                *outcome,
                failure_kind.as_deref(),
                checked_at_unix_ms,
            ) {
                // restorationFailures retains every exact compensation failure without hiding the primary.
                // restorationFailures 保留每个精确补偿失败且不隐藏主错误。
                let restoration_failures = snapshots
                    .iter()
                    .filter_map(|(snapshot_provider_id, snapshot)| {
                        self.store
                            .restore_check_state(snapshot_provider_id, snapshot)
                            .err()
                            .map(|error| format!("{snapshot_provider_id}: {error}"))
                    })
                    .collect::<Vec<_>>();
                return Err(if restoration_failures.is_empty() {
                    format!(
                        "Provider manifest check-state transaction failed for `{provider_id}`: {primary}; compensation: ok"
                    )
                } else {
                    format!(
                        "Provider manifest check-state transaction failed for `{provider_id}`: {primary}; compensation failed: {}",
                        restoration_failures.join("; ")
                    )
                });
            }
        }
        Ok(())
    }

    /// Builds and writes one provider's next independent check state.
    /// 构建并写入一个供应商的下一份独立检查状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `outcome`, `failure_kind`, and `checked_at_unix_ms` identify the exact fact.
    /// `provider_id`、`outcome`、`failure_kind` 与 `checked_at_unix_ms` 标识精确事实。
    ///
    /// # Returns
    /// 返回值
    /// Success after preserving the former successful activation unless this is a new one.
    /// 保留先前成功激活（当前为新激活时更新）并写入后返回成功。
    fn write_sync_check_state(
        &self,
        provider_id: &str,
        outcome: ManifestCheckOutcome,
        failure_kind: Option<&str>,
        checked_at_unix_ms: u64,
    ) -> Result<(), String> {
        // formerCheck contributes the independent last-successful identity and monotonic time.
        // formerCheck 提供独立的最近成功身份与单调时间。
        let former_check = match self.store.load_check_state(provider_id)? {
            CheckStateLoad::Present(state) => Some(state),
            CheckStateLoad::Absent => None,
        };
        // activeSuccess seeds migration from the transactional sync state when no check file exists.
        // activeSuccess 在检查文件不存在时从事务同步状态播种迁移值。
        let active_success = match self.store.load_sync_state(provider_id)? {
            SyncStateLoad::Present(state) => Some(state.synced_at_unix_ms),
            SyncStateLoad::Absent => None,
        };
        // effectiveCheckedAt prevents wall-clock regression from corrupting timestamp ordering.
        // effectiveCheckedAt 防止墙上时钟回退破坏时间戳顺序。
        let effective_checked_at = former_check
            .as_ref()
            .map(|state| state.checked_at_unix_ms)
            .into_iter()
            .chain(active_success)
            .fold(checked_at_unix_ms, u64::max);
        // formerSuccessful preserves the most recent proven full activation across 304 and failures.
        // formerSuccessful 在 304 与失败之间保留最近一次已证明完整激活。
        let former_successful = former_check
            .as_ref()
            .and_then(|state| state.last_successful_at_unix_ms)
            .or(active_success);
        // next is closed and contains neither remote URLs nor free-form error text.
        // next 是封闭状态，既不含远端 URL，也不含自由文本错误。
        let next = ManifestCheckState {
            checked_at_unix_ms: effective_checked_at,
            last_successful_at_unix_ms: if outcome == ManifestCheckOutcome::Activated {
                Some(effective_checked_at)
            } else {
                former_successful
            },
            outcome,
            last_error_kind: failure_kind.map(str::to_string),
        };
        self.store.save_check_state(provider_id, &next)
    }

    /// Installs the abort handle for the single service-owned periodic task.
    /// 安装服务持有的唯一周期任务取消句柄。
    ///
    /// # Parameters
    /// 参数
    /// `abort_handle` controls the exact newly spawned startup/periodic task.
    /// `abort_handle` 控制刚生成的精确启动/周期任务。
    ///
    /// # Returns
    /// 返回值
    /// Success after any former task is aborted and the new handle becomes authoritative.
    /// 中止任何旧任务并使新句柄成为权威句柄后返回成功。
    pub(crate) fn install_periodic_task(
        &self,
        abort_handle: tokio::task::AbortHandle,
    ) -> Result<(), String> {
        let mut guard = self
            .periodic_task_abort
            .lock()
            .map_err(|_| String::from("Provider manifest periodic task state is poisoned."))?;
        if let Some(previous) = guard.replace(abort_handle) {
            previous.abort();
        }
        Ok(())
    }

    /// Stops and detaches the service-owned periodic synchronization task.
    /// 停止并解除服务持有的周期同步任务。
    ///
    /// # Returns
    /// 返回值
    /// Success after the current task receives cancellation, or when no task exists.
    /// 当前任务收到取消后返回成功；没有任务时也返回成功。
    pub(crate) fn stop_periodic_task(&self) -> Result<(), String> {
        let mut guard = self
            .periodic_task_abort
            .lock()
            .map_err(|_| String::from("Provider manifest periodic task state is poisoned."))?;
        if let Some(abort_handle) = guard.take() {
            abort_handle.abort();
        }
        Ok(())
    }

    /// Restores exact disk/runtime before-images after a later projection stage fails.
    /// 在后续投影阶段失败后恢复精确磁盘/运行时前镜像。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` receives each immutable old manifest and `snapshots` owns exact file bytes.
    /// `runtime` 接收每份旧不可变清单，`snapshots` 持有精确文件字节。
    ///
    /// # Returns
    /// 返回值
    /// Success only when every affected provider is restored in both layers.
    /// 仅当每个受影响供应商在两层均完成恢复时返回成功。
    pub(crate) fn restore_activation_snapshots(
        &self,
        runtime: &PublicAiRuntime,
        snapshots: &[ProviderManifestActivationSnapshot],
    ) -> Result<(), String> {
        let mut failures = Vec::new();
        // Reverse order is required when a batch staged multiple providers: each after-image is
        // the next snapshot's before-image, so unwinding forward would violate the CAS chain.
        // 批次暂存多个供应商时必须逆序：每个后镜像都是下一快照的前镜像，正向展开会破坏
        // CAS 链。
        for snapshot in snapshots.iter().rev() {
            // runtimeRestore runs first so a failed in-process CAS keeps the durable Applying
            // journal available as the final crash-recovery authority.
            // runtimeRestore 先执行，使进程内 CAS 恢复失败时仍保留持久化 Applying 日志，
            // 作为最后的崩溃恢复权威。
            let runtime_restore =
                runtime.restore_staged_system_manifest_catalog(&snapshot.staged_manifest_mutation);
            if let Err(error) = &runtime_restore {
                failures.push(format!(
                    "{} runtime restore failed: {error}",
                    snapshot.provider_id
                ));
            }
            // storeRestore clears the journal only after runtime staging is coherent. When runtime
            // restoration fails, the exact disk before-image is still restored but the Applying
            // journal deliberately remains so startup can repeat the recovery fail-closed.
            // storeRestore 仅在运行时暂存恢复一致后清除日志。运行时恢复失败时仍恢复精确磁盘
            // 前镜像，但有意保留 Applying 日志，使启动可以再次执行失败关闭恢复。
            let store_restore = if runtime_restore.is_ok() {
                self.store
                    .abort_system_transaction(&snapshot.provider_id, &snapshot.store)
            } else {
                self.store
                    .restore_system_snapshot(&snapshot.provider_id, &snapshot.store)
            };
            if let Err(error) = store_restore {
                failures.push(format!(
                    "{} store restore failed: {error}",
                    snapshot.provider_id
                ));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    /// Commits every staged provider file transaction after all visible projections succeed.
    /// 在全部可见投影成功后提交每个已暂存供应商文件事务。
    ///
    /// # Parameters
    /// 参数
    /// `snapshots` are the exact provider-scoped before-images whose Applying journals remain open.
    /// `snapshots` 是 Applying 日志仍保持打开的精确逐供应商前镜像。
    ///
    /// # Returns
    /// 返回值
    /// Success only when every durable activation transaction reaches its commit boundary.
    /// 仅当每个持久化激活事务都到达提交边界时返回成功。
    pub(crate) fn commit_activation_snapshots(
        &self,
        snapshots: &[ProviderManifestActivationSnapshot],
    ) -> Result<(), String> {
        for snapshot in snapshots {
            self.store
                .commit_system_transaction(&snapshot.provider_id, &snapshot.store)?;
        }
        Ok(())
    }

    /// Commits one explicit rollback only after its complete runtime projection is visible.
    /// 仅在显式回滚的完整运行时投影可见后提交该回滚。
    ///
    /// # Parameters
    /// 参数
    /// `report` retains the exact transaction identity and before-image journal payload.
    /// `report` 保留精确事务身份与前镜像日志载荷。
    ///
    /// # Returns
    /// 返回值
    /// Success after startup recovery can no longer restore the former version.
    /// 启动恢复不再会恢复旧版本后返回成功。
    pub(crate) fn commit_rollback(&self, report: &ProviderRollbackReport) -> Result<(), String> {
        self.store
            .commit_system_transaction(&report.provider_id, &report.activation_snapshot.store)
    }

    /// Aborts one explicit rollback and restores its exact disk and staged-runtime before-image.
    /// 中止一次显式回滚并恢复其精确磁盘与暂存运行时前镜像。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the staged manifest catalog and `report` owns the former version.
    /// `runtime` 持有暂存 Manifest 目录，`report` 持有旧版本。
    ///
    /// # Returns
    /// 返回值
    /// Success only when the durable transaction and runtime staging catalog are both restored.
    /// 仅当持久事务与运行时暂存目录均恢复时返回成功。
    pub(crate) fn abort_rollback(
        &self,
        runtime: &PublicAiRuntime,
        report: &ProviderRollbackReport,
    ) -> Result<(), String> {
        // runtimeRestore runs first so any failed staging CAS leaves the Applying journal as
        // durable recovery authority instead of clearing the only cross-process evidence.
        // runtimeRestore 先执行，使任何暂存 CAS 恢复失败时保留 Applying 日志作为持久恢复权威，
        // 而不是清除唯一跨进程证据。
        let runtime_restore = runtime.restore_staged_system_manifest_catalog(
            &report.activation_snapshot.staged_manifest_mutation,
        );
        // storeRestore clears the journal only after runtime staging is coherent; otherwise it
        // restores bytes while deliberately retaining the Applying journal for startup recovery.
        // storeRestore 仅在运行时暂存一致后清除日志；否则恢复字节并有意保留 Applying 日志，
        // 供启动恢复。
        let store_restore = if runtime_restore.is_ok() {
            self.store
                .abort_system_transaction(&report.provider_id, &report.activation_snapshot.store)
        } else {
            self.store
                .restore_system_snapshot(&report.provider_id, &report.activation_snapshot.store)
        };
        match (store_restore, runtime_restore) {
            (Ok(()), Ok(())) => Ok(()),
            (store, runtime) => Err(format!(
                "Provider `{}` rollback compensation failed: store={store:?}; runtime={runtime:?}",
                report.provider_id
            )),
        }
    }

    /// Finalizes one rollback across the runtime generation and its externally visible projection.
    /// 跨运行时代际及其外部可见投影完成一次回滚。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the staged manifest generation; `configuration` is the exact persisted
    /// connection graph; `report` owns the durable before-image transaction; `publish_projection`
    /// installs the candidate around one mandatory commit callback; `republish_projection`
    /// republishes only the restored before-image after compensation.
    /// `runtime` 持有暂存 Manifest 代际；`configuration` 是精确持久化连接图；`report` 持有持久化
    /// 前镜像事务；`publish_projection` 围绕一次必须调用的提交回调安装候选；
    /// `republish_projection` 仅在补偿后重新发布已恢复的前镜像。
    ///
    /// # Returns
    /// 返回值
    /// The committed rollback report only after every projection succeeds. Any projection or
    /// commit failure restores the exact disk/runtime before-image and republishes it.
    /// 仅在全部投影成功后返回已提交回滚报告。任何投影或提交失败都会恢复精确磁盘/运行时前镜像
    /// 并重新发布它。
    pub(crate) async fn finalize_rollback_projection<Publish, Republish>(
        &self,
        runtime: &PublicAiRuntime,
        configuration: &PersistedAiConfiguration,
        report: ProviderRollbackReport,
        publish_projection: Publish,
        mut republish_projection: Republish,
    ) -> Result<ProviderRollbackReport, String>
    where
        Publish: FnOnce(&mut dyn FnMut() -> Result<(), String>) -> Result<(), String>,
        Republish: FnMut() -> Result<(), String>,
    {
        let provider_id = report.provider_id.clone();
        // cancellationGuard owns exact rollback compensation across every asynchronous wait.
        // cancellationGuard 在每个异步等待点期间持有精确回滚补偿责任。
        let mut cancellation_guard = ProviderRollbackCancellationGuard::new(self, runtime, &report);
        // catalogTransaction prevents usage, refresh, removal, or another runtime activation from
        // entering between rollback projection, durable commit, and any required compensation.
        // catalogTransaction 防止用量、刷新、删除或其他运行时激活插入回滚投影、持久提交与
        // 必要补偿之间。
        let catalog_transaction = runtime.begin_catalog_activation_transaction().await;
        let connection_ids = configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .map(|connection| connection.id.clone())
            .collect::<Vec<_>>();
        let runtime_before_image = match runtime.capture_activation_snapshot() {
            Ok(snapshot) => snapshot,
            Err(primary) => {
                // No asynchronous cancellation point remains before explicit compensation.
                // 显式补偿前已不存在异步取消点。
                cancellation_guard.disarm();
                // compensation restores the durable and staged rollback mutation because no
                // published runtime generation or Workbench projection exists yet.
                // compensation 恢复持久层与暂存回滚变更，因为此时尚未发布运行时代际或
                // 工作台投影。
                let compensation = self.abort_rollback(runtime, &report);
                return Err(format!(
                    "Provider `{provider_id}` rollback runtime before-image capture failed: {primary}; manifest compensation: {compensation:?}"
                ));
            }
        };
        let activation = runtime
            .activate_staged_manifest_catalogs_in_transaction(
                configuration,
                &connection_ids,
                &catalog_transaction,
            )
            .await;
        // All remaining projection, commit, and compensation steps are synchronous.
        // 剩余投影、提交与补偿步骤全部为同步操作。
        cancellation_guard.disarm();
        drop(cancellation_guard);
        let (runtime_after_image, projection) = match activation {
            Ok(after_image) => {
                // projectionResult and commitCalled leave the callback's lexical scope together,
                // so the mutable proof borrow ends without an artificial explicit drop.
                // projectionResult 与 commitCalled 一同离开回调的词法作用域，因此可变证明借用
                // 会自然结束，无需人为显式 drop。
                let (projection, commit_called) = {
                    // commitCalled proves the projection crossed the durable rollback boundary exactly once.
                    // commitCalled 证明投影恰好一次越过持久回滚边界。
                    let mut commit_called = false;
                    // projection owns the callback only for this inner scope.
                    // projection 仅在当前内部作用域持有回调。
                    let projection = {
                        // commitRollback rejects duplicate callback use and owns the journal transition.
                        // commitRollback 拒绝重复使用回调，并持有日志转换。
                        let mut commit_rollback = || {
                            if std::mem::replace(&mut commit_called, true) {
                                return Err(format!(
                                    "Provider `{provider_id}` rollback projection invoked its commit callback more than once."
                                ));
                            }
                            self.commit_rollback(&report)
                        };
                        // projection must prepare every fallible Service value before invoking commit;
                        // its state swap is required to be infallible afterward.
                        // projection 必须在调用提交前准备全部可失败 Service 值；之后的状态交换必须不可失败。
                        publish_projection(&mut commit_rollback)
                    };
                    (projection, commit_called)
                };
                let projection = if projection.is_ok() && !commit_called {
                    Err(format!(
                        "Provider `{provider_id}` rollback projection returned success without committing its activation journal."
                    ))
                } else {
                    projection
                };
                (Some(after_image), projection)
            }
            Err(error) => (None, Err(error)),
        };
        if let Err(primary) = projection {
            let compensation = self.abort_rollback(runtime, &report);
            let runtime_restore = runtime_after_image.as_ref().map_or(Ok(()), |after_image| {
                runtime.restore_activation_snapshot(&runtime_before_image, after_image)
            });
            let republish = match (
                &compensation,
                &runtime_restore,
                runtime_after_image.is_some(),
            ) {
                (Ok(()), Ok(()), true) => republish_projection(),
                (Ok(()), Ok(()), false) => Ok(()),
                _ => Err(String::from(
                    "Former provider projection was not republished because rollback compensation failed.",
                )),
            };
            return Err(format!(
                "Provider `{provider_id}` rollback projection failed: {primary}; manifest compensation: {compensation:?}; runtime compensation: {runtime_restore:?}; former-generation republication: {republish:?}"
            ));
        }
        if let Some(manifest) = runtime.system_manifest(provider_id.as_str()) {
            emit_manifest_fallback_event(
                provider_id.as_str(),
                report.restored_revision,
                runtime.catalog_generation(),
                manifest_source_layer_label(manifest.snapshot_provenance.source_layer),
                manifest.snapshot_provenance.sha256.as_deref(),
                "explicit_rollback",
            );
        } else {
            eprintln!(
                "[provider-manifest-sync] explicit rollback telemetry skipped missing runtime provider `{provider_id}`"
            );
        }
        Ok(report)
    }

    /// Synchronizes one provider from the signed remote index and hot-replaces the runtime.
    /// 从签名远端 Index 同步一个供应商并热替换运行时。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the single authoritative catalog; `provider_id` is exact.
    /// `runtime` 持有唯一权威目录；`provider_id` 为精确标识。
    ///
    /// # Returns
    /// 返回值
    /// A stable sync report; verification and ordering failures are quarantined with their
    /// classified kind and never touch the active layer.
    /// 返回稳定同步报告；校验与顺序失败按分类类型隔离，绝不触碰活动层。
    ///
    /// The method stays as the single-provider entry point exercised by local E2E tests;
    /// the production RPC path uses `sync_all` for one-index/many-provider synchronization.
    /// 该方法保留为本地 E2E 测试使用的单供应商入口；生产 RPC 路径使用 `sync_all`
    /// 进行单 Index/多供应商同步。
    #[cfg(test)]
    pub async fn sync_provider(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
    ) -> Result<ProviderSyncReport, String> {
        self.sync_provider_with_file_transaction_mode(runtime, provider_id, true)
            .await
    }

    /// Synchronizes one provider while selecting whether file activation commits immediately.
    /// 同步一个供应商，并选择文件激活是否立即提交。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the staged runtime catalog, `provider_id` is the exact trusted identity,
    /// and `commit_file_transaction` is true only for compatibility callers that do not publish
    /// Runtime and Workbench projections after this method returns.
    /// `runtime` 持有暂存运行时目录，`provider_id` 是精确可信身份；仅当兼容调用方不会在本方法
    /// 返回后继续发布 Runtime 与 Workbench 投影时，`commit_file_transaction` 才为 true。
    ///
    /// # Returns
    /// 返回值
    /// A stable provider report whose activation snapshot remains recoverable while the file
    /// transaction is staged.
    /// 返回稳定的供应商报告；文件事务处于暂存状态时，其激活快照保持可恢复。
    async fn sync_provider_with_file_transaction_mode(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
        commit_file_transaction: bool,
    ) -> Result<ProviderSyncReport, String> {
        // identityGuard bounds the retained per-provider flight map to the exact trusted bundled
        // identity set before any caller-controlled string is allocated into it.
        // identityGuard 在任何调用方可控字符串写入逐供应商 Flight Map 前，把其上限约束为
        // 精确可信内置身份集合。
        require_trusted_system_manifest_provider(runtime, provider_id)?;
        let not_configured = || ProviderSyncReport {
            provider_id: provider_id.to_string(),
            outcome: "not_configured",
            revision: 0,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: None,
        };
        let Some(index_url) = self.remote_index_url.as_ref() else {
            return Ok(not_configured());
        };
        index_origin(index_url)?;
        let mut telemetry = ManifestIndexCheckTelemetryGuard::start(vec![provider_id.to_string()]);
        // Per-provider single-flight: concurrent syncs of the same provider serialize so the
        // activation transaction can never be duplicated (plan §R7.8).
        // 逐供应商 single-flight：同一供应商的并发同步被串行化，激活事务绝不可能重复
        // （方案 §R7.8）。
        let provider_flight = self.provider_flight(provider_id).await;
        let _provider_guard = provider_flight.lock().await;
        let state = match self.store.load_sync_state(provider_id)? {
            vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) => Some(state),
            vulcan_provider_config::manifest_store::SyncStateLoad::Absent => None,
        };
        // The index fetch is globally single-flighted so manual, startup, and periodic
        // triggers never issue concurrent index requests (plan §R7.8).
        // Index 获取全局 single-flight，手动、启动与周期触发绝不并发发出 Index 请求
        // （方案 §R7.8）。
        let index_etag = state.as_ref().and_then(|state| state.index_etag.clone());
        let (index_status, index_bytes, index_etag_response, index_signature) = {
            let _index_guard = self.index_flight.lock().await;
            self.fetch_conditional(index_url, index_etag.as_deref(), REMOTE_INDEX_MAX_BYTES)
                .await?
        };
        if index_status == 304 {
            telemetry.not_modified(provider_id);
            // A 304 index is acceptable only when the local state pins a complete active file
            // that still re-verifies; otherwise the index is refetched unconditionally (plan
            // §R7.9).
            // Index 304 仅在本地状态固定一份完整且仍可重新校验的 active 文件时才可接受；
            // 否则无条件重取 Index（方案 §R7.9）。
            if let Some(current_state) = state.as_ref().filter(|state| {
                self.local_active_is_current(provider_id, &state.revision, &state.sha256)
                    .is_ok()
            }) {
                // revalidatedState preserves the request validator while clearing a stale
                // transient provider error; a 304 is not a new activation timestamp.
                // revalidatedState 保留请求验证器并清除陈旧的供应商瞬态错误；304 不是新的激活时间。
                let revalidated_state = provider_not_modified_sync_state(
                    current_state,
                    current_state.index_etag.clone(),
                );
                self.store
                    .save_sync_state(provider_id, &revalidated_state)?;
                telemetry.mark_index_resolved();
                let report = ProviderSyncReport {
                    provider_id: provider_id.to_string(),
                    outcome: "up_to_date",
                    revision: state.as_ref().map(|state| state.revision).unwrap_or(0),
                    failure_kind: None,
                    failure_message: None,
                    activation_snapshot: None,
                };
                telemetry.complete(std::slice::from_ref(&report));
                return Ok(report);
            }
            let (_index_status, index_bytes, index_etag_response, index_signature) = {
                let _index_guard = self.index_flight.lock().await;
                self.fetch_conditional(index_url, None, REMOTE_INDEX_MAX_BYTES)
                    .await?
            };
            let signature = decode_signature_header(&index_signature)?;
            let now_unix_ms = current_unix_millis() as i64;
            let index =
                verify_and_parse_index(&index_bytes, &signature, &self.trusted_keys, now_unix_ms)?;
            telemetry.mark_index_resolved();
            let report = self
                .sync_provider_from_index(
                    runtime,
                    provider_id,
                    &index,
                    index_etag_response,
                    state.as_ref(),
                    commit_file_transaction,
                )
                .await?;
            telemetry.complete(std::slice::from_ref(&report));
            return Ok(report);
        }
        let signature = decode_signature_header(&index_signature)?;
        let now_unix_ms = current_unix_millis() as i64;
        let index =
            verify_and_parse_index(&index_bytes, &signature, &self.trusted_keys, now_unix_ms)?;
        telemetry.mark_index_resolved();
        let report = self
            .sync_provider_from_index(
                runtime,
                provider_id,
                &index,
                index_etag_response,
                state.as_ref(),
                commit_file_transaction,
            )
            .await?;
        telemetry.complete(std::slice::from_ref(&report));
        Ok(report)
    }

    /// Synchronizes one signed Index and finalizes each provider before staging the next provider.
    /// 同步一份签名 Index，并在暂存下一个供应商前完成当前供应商。
    ///
    /// This production-oriented boundary preserves one-index fetch efficiency while allowing the
    /// service layer to publish or compensate each provider as an independent transaction (plan
    /// §17.4). The handler runs while the exact provider flight remains held.
    /// 该生产边界既保留单次 Index 获取效率，又允许服务层把每个供应商作为独立事务发布或补偿
    /// （方案 §17.4）。处理器运行期间会继续持有精确供应商 single-flight。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns staged manifests, `provider_ids` preserves deterministic order, and
    /// `report_handler` completes the exact provider's runtime/Workbench/check-state transaction.
    /// `runtime` 持有暂存 Manifest，`provider_ids` 保持确定顺序，`report_handler` 完成精确供应商
    /// 的运行时、工作台与检查状态事务。
    ///
    /// # Returns
    /// 返回值
    /// One finalized report per processed provider, a shared remote failure, or a provider-scoped
    /// projection failure after its handler has attempted compensation.
    /// 返回每个已处理供应商的一份完成报告、共享远端失败，或处理器已尝试补偿后的逐供应商投影失败。
    pub(crate) async fn sync_all_with_report_handler<F, Fut>(
        &self,
        runtime: &PublicAiRuntime,
        provider_ids: &[String],
        mut report_handler: F,
    ) -> Result<Vec<ProviderSyncReport>, ProviderSyncBatchFailure>
    where
        F: FnMut(ProviderSyncReport) -> Fut,
        Fut: std::future::Future<Output = Result<ProviderSyncReport, String>>,
    {
        let not_configured = |provider_id: &str| ProviderSyncReport {
            provider_id: provider_id.to_string(),
            outcome: "not_configured",
            revision: 0,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: None,
        };
        let Some(index_url) = self.remote_index_url.as_ref() else {
            return Ok(provider_ids
                .iter()
                .map(|provider_id| not_configured(provider_id))
                .collect());
        };
        index_origin(index_url)?;
        let mut telemetry = ManifestIndexCheckTelemetryGuard::start(provider_ids.to_vec());
        // sharedIndexEtag enables a real conditional periodic/manual batch check only when all
        // provider transactions agree on the exact last authenticated Index identity.
        // sharedIndexEtag 仅在全部供应商事务对上一份认证 Index 身份完全一致时，启用真正的
        // 条件式周期/手动批量检查。
        let shared_index_etag = self.shared_revalidated_index_etag(provider_ids);
        // Fetch the signed index exactly once under the global flight.
        // 在全局 single-flight 下只获取一次签名 Index。
        let (index_status, index_bytes, index_etag_response, index_signature) = {
            let _index_guard = self.index_flight.lock().await;
            self.fetch_conditional(
                index_url,
                shared_index_etag.as_deref(),
                REMOTE_INDEX_MAX_BYTES,
            )
            .await?
        };
        if index_status == 304 {
            telemetry.mark_index_resolved();
            for provider_id in provider_ids {
                telemetry.not_modified(provider_id);
            }
            // The conditional request was admitted only after every provider proved the same
            // authenticated Index identity and a complete Active layer. Each provider is
            // revalidated under its own flight; a concurrent external mutation fails that
            // provider closed and is healed by one unconditional batch fetch on the next run.
            // 条件请求仅在每个供应商都证明相同认证 Index 身份与完整 Active 层后才获准。
            // 每个供应商仍在其专属 single-flight 下重新校验；并发外部篡改会让该供应商失败封闭，
            // 并在下次运行时通过整批一次无条件获取修复。
            let mut reports = Vec::with_capacity(provider_ids.len());
            for provider_id in provider_ids {
                // Full-catalog sync must share the exact per-provider flight with manual
                // single-provider sync; otherwise startup/periodic and manual triggers can
                // commit the same provider transaction concurrently.
                // 全目录同步必须与手动单供应商同步共享同一个逐供应商 single-flight；否则
                // 启动/周期触发与手动触发可能并发提交同一供应商事务。
                let provider_flight = self.provider_flight(provider_id).await;
                let _provider_guard = provider_flight.lock().await;
                let report = self
                    .sync_provider_after_index_not_modified(provider_id)
                    .unwrap_or_else(|error| provider_sync_failure_report(provider_id, error));
                // finalizedReport crosses the provider-scoped publication boundary before the
                // next provider can mutate staged Manifest ownership.
                // finalizedReport 在下一供应商修改暂存 Manifest 所有权前越过逐供应商发布边界。
                let finalized_report = match report_handler(report).await {
                    Ok(report) => report,
                    Err(error) => {
                        telemetry.fail_provider(provider_id, "activation_publication_failed");
                        return Err(ProviderSyncBatchFailure::Projection(error));
                    }
                };
                telemetry.complete_provider(&finalized_report);
                reports.push(finalized_report);
            }
            return Ok(reports);
        }
        let signature = decode_signature_header(&index_signature)?;
        let now_unix_ms = current_unix_millis() as i64;
        let index =
            verify_and_parse_index(&index_bytes, &signature, &self.trusted_keys, now_unix_ms)?;
        telemetry.mark_index_resolved();
        let mut reports = Vec::with_capacity(provider_ids.len());
        for provider_id in provider_ids {
            // Reuse the same provider flight used by `sync_provider` so every trigger type
            // observes one serial activation transaction for this exact provider.
            // 复用 `sync_provider` 使用的同一供应商锁，使所有触发类型针对该精确供应商
            // 只观察到串行激活事务。
            let provider_flight = self.provider_flight(provider_id).await;
            let _provider_guard = provider_flight.lock().await;
            let report = match self.store.load_sync_state(provider_id) {
                Ok(state) => {
                    let state = match state {
                        vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) => {
                            Some(state)
                        }
                        vulcan_provider_config::manifest_store::SyncStateLoad::Absent => None,
                    };
                    self.sync_provider_from_index(
                        runtime,
                        provider_id,
                        &index,
                        index_etag_response.clone(),
                        state.as_ref(),
                        false,
                    )
                    .await
                    .unwrap_or_else(|error| provider_sync_failure_report(provider_id, error))
                }
                Err(error) => provider_sync_failure_report(provider_id, error),
            };
            // finalizedReport prevents a later provider failure from rolling back an already
            // committed independent provider (plan §17.4).
            // finalizedReport 防止后续供应商失败回滚已提交的独立供应商（方案 §17.4）。
            let finalized_report = match report_handler(report).await {
                Ok(report) => report,
                Err(error) => {
                    telemetry.fail_provider(provider_id, "activation_publication_failed");
                    return Err(ProviderSyncBatchFailure::Projection(error));
                }
            };
            telemetry.complete_provider(&finalized_report);
            reports.push(finalized_report);
        }
        Ok(reports)
    }

    /// Revalidates one provider after a shared batch-level Index 304.
    /// 在共享批量 Index 返回 304 后重新校验一个供应商。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact local state admitted by the shared conditional request.
    /// `provider_id` 标识共享条件请求所准入的精确本地状态。
    ///
    /// # Returns
    /// 返回值
    /// One up-to-date report after durable state refresh, or a provider-scoped fail-closed error.
    /// 返回持久状态刷新后的一份最新报告，或逐供应商失败封闭错误。
    fn sync_provider_after_index_not_modified(
        &self,
        provider_id: &str,
    ) -> Result<ProviderSyncReport, String> {
        // state must still be present because the shared conditional decision was derived from it.
        // state 必须仍然存在，因为共享条件决策正是从该状态推导而来。
        let state = match self.store.load_sync_state(provider_id)? {
            vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) => state,
            vulcan_provider_config::manifest_store::SyncStateLoad::Absent => {
                return Err(format!(
                    "Provider `{provider_id}` lost its sync state after the shared Index 304."
                ));
            }
        };
        self.local_active_is_current(provider_id, &state.revision, &state.sha256)?;
        // revalidatedState keeps the existing shared validator because HTTP 304 carries no new
        // signed body or ETag authority, while clearing a stale transient provider error.
        // revalidatedState 保留既有共享验证器，因为 HTTP 304 不携带新的签名正文或 ETag 权威，
        // 同时清除陈旧的供应商瞬态错误。
        let revalidated_state = provider_not_modified_sync_state(&state, state.index_etag.clone());
        self.store
            .save_sync_state(provider_id, &revalidated_state)?;
        Ok(ProviderSyncReport {
            provider_id: provider_id.to_string(),
            outcome: "up_to_date",
            revision: state.revision,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: None,
        })
    }

    /// Returns the per-provider single-flight guard, creating it on first use.
    /// 返回逐供应商 single-flight 守卫，首次使用时创建。
    async fn provider_flight(&self, provider_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut flights = self.provider_flights.lock().await;
        flights
            .entry(provider_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Verifies the local active layer still matches one pinned revision/hash and compiles.
    /// 校验本地 active 层仍与一个固定修订/摘要一致且可编译。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider; `revision`/`sha256` are the pinned facts.
    /// `provider_id` 选择精确供应商；`revision`/`sha256` 是固定事实。
    fn local_active_is_current(
        &self,
        provider_id: &str,
        revision: &u64,
        sha256: &str,
    ) -> Result<(), String> {
        let Some(text) = self.store.read_active(provider_id)? else {
            return Err(format!(
                "Provider `{provider_id}` has no active layer to validate."
            ));
        };
        let expected = vulcan_provider_config::manifest_store::ManifestVersionState {
            revision: *revision,
            sha256: sha256.to_string(),
        };
        verify_stored_system_manifest(
            provider_id,
            &text,
            vulcan_provider_adapters::ManifestSourceLayer::Active,
            &expected,
        )
        .map(|_| ())
        .map_err(|error| {
            format!("Provider `{provider_id}` active layer is no longer trusted: {error}")
        })
    }

    /// Synchronizes one provider from an already verified index snapshot.
    /// 从一份已校验的 Index 快照同步一个供应商。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the single authoritative catalog; `provider_id` is exact; `index` is
    /// the shared verified snapshot; `index_etag_response` is the response ETag persisted
    /// with the activation transaction; `state` is the per-provider sync state if present; and
    /// `commit_file_transaction` selects compatibility commit versus production staged publish.
    /// `runtime` 持有唯一权威目录；`provider_id` 是精确标识；`index` 是共享已验证快照；
    /// `index_etag_response` 是随激活事务持久化的响应 ETag；`state` 是逐供应商同步状态
    /// （存在时）；`commit_file_transaction` 选择兼容提交或生产暂存发布。
    async fn sync_provider_from_index(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
        index: &vulcan_provider_config::manifest_sync::ManifestRemoteIndex,
        index_etag_response: Option<String>,
        state: Option<&vulcan_provider_config::manifest_store::ManifestSyncState>,
        commit_file_transaction: bool,
    ) -> Result<ProviderSyncReport, String> {
        let Some(entry) = index.providers.get(provider_id).cloned() else {
            return Ok(ProviderSyncReport {
                provider_id: provider_id.to_string(),
                outcome: "not_pinned",
                revision: 0,
                failure_kind: None,
                failure_message: None,
                activation_snapshot: None,
            });
        };
        // The provider URL resolves strictly under the index origin.
        // 供应商 URL 严格在 Index 源下解析。
        let index_url = self
            .remote_index_url
            .as_deref()
            .ok_or_else(|| String::from("Remote index URL is not configured."))?;
        let base_origin = index_origin(index_url)?;
        let provider_url = resolve_remote_provider_url(&base_origin, &entry.relative_url)?;
        let provider_etag = state.and_then(|state| state.etag.clone());
        let (provider_status, provider_bytes, provider_etag_response, _provider_signature) = self
            .fetch_conditional(
                &provider_url,
                provider_etag.as_deref(),
                REMOTE_PROVIDER_MAX_BYTES,
            )
            .await?;
        if provider_status == 304 {
            // A provider 304 is acceptable only when the local active bytes still match the
            // index pin and re-verify; otherwise the provider is refetched unconditionally
            // (plan §R7.9).
            // 供应商 304 仅在本地 active 字节仍与 Index 固定值一致且可重新校验时才可接受；
            // 否则无条件重取供应商（方案 §R7.9）。
            let current_state = state.filter(|state| {
                state.revision == entry.revision
                    && state.sha256 == entry.sha256
                    && self
                        .local_active_is_current(provider_id, &entry.revision, &entry.sha256)
                        .is_ok()
            });
            if let Some(current_state) = current_state {
                // nextState persists the newly observed shared Index ETag even when provider
                // bytes are unchanged, so later batch checks can use a real conditional GET.
                // nextState 即使供应商字节未变化也会持久化新观察到的共享 Index ETag，使后续
                // 批量检查可以使用真正的条件 GET。
                let next_state =
                    provider_not_modified_sync_state(current_state, index_etag_response.clone());
                self.store.save_sync_state(provider_id, &next_state)?;
                return Ok(ProviderSyncReport {
                    provider_id: provider_id.to_string(),
                    outcome: "up_to_date",
                    revision: entry.revision,
                    failure_kind: None,
                    failure_message: None,
                    activation_snapshot: None,
                });
            }
            let (_, provider_bytes, provider_etag_response, _) = self
                .fetch_conditional(&provider_url, None, REMOTE_PROVIDER_MAX_BYTES)
                .await?;
            emit_provider_candidate_event(
                ProviderOperationalTelemetryEventName::ProviderCandidateDownloaded,
                provider_id,
                entry.revision,
                entry.sha256.as_str(),
                None,
                None,
            );
            let outcome = if commit_file_transaction {
                activate_synced_provider(
                    &self.store,
                    &entry,
                    &provider_bytes,
                    provider_etag_response,
                    index_etag_response,
                    None,
                )?
            } else {
                stage_synced_provider_activation(
                    &self.store,
                    &entry,
                    &provider_bytes,
                    provider_etag_response,
                    index_etag_response,
                    None,
                )?
            };
            return self.outcome_to_report(
                runtime,
                provider_id,
                entry.revision,
                entry.sha256.as_str(),
                outcome,
            );
        }
        emit_provider_candidate_event(
            ProviderOperationalTelemetryEventName::ProviderCandidateDownloaded,
            provider_id,
            entry.revision,
            entry.sha256.as_str(),
            None,
            None,
        );
        let outcome = if commit_file_transaction {
            activate_synced_provider(
                &self.store,
                &entry,
                &provider_bytes,
                provider_etag_response,
                index_etag_response,
                None,
            )?
        } else {
            stage_synced_provider_activation(
                &self.store,
                &entry,
                &provider_bytes,
                provider_etag_response,
                index_etag_response,
                None,
            )?
        };
        self.outcome_to_report(
            runtime,
            provider_id,
            entry.revision,
            entry.sha256.as_str(),
            outcome,
        )
    }

    /// Projects one sync outcome into the stable sync report.
    /// 把一个同步结果投影为稳定同步报告。
    fn outcome_to_report(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
        revision: u64,
        sha256: &str,
        outcome: vulcan_provider_config::manifest_sync::SyncOutcome,
    ) -> Result<ProviderSyncReport, String> {
        match outcome {
            vulcan_provider_config::manifest_sync::SyncOutcome::UpToDate => {
                Ok(ProviderSyncReport {
                    provider_id: provider_id.to_string(),
                    outcome: "up_to_date",
                    revision,
                    failure_kind: None,
                    failure_message: None,
                    activation_snapshot: None,
                })
            }
            vulcan_provider_config::manifest_sync::SyncOutcome::Activated {
                revision,
                manifest,
                store_snapshot,
            } => {
                // The verified snapshot replaces the runtime catalog under the short write
                // lock; activation and replacement share the exact compiled revision.
                // 已验证快照在短写锁下替换运行时目录；激活与替换共享同一编译修订。
                let staged_manifest_mutation = match runtime
                    .replace_system_manifest_transactional(provider_id, manifest)
                {
                    Ok(mutation) => mutation,
                    Err(error) => {
                        let rollback = self
                            .store
                            .abort_system_transaction(provider_id, &store_snapshot);
                        return Err(format!(
                            "Provider `{provider_id}` runtime publication failed: {error}; store rollback: {rollback:?}"
                        ));
                    }
                };
                emit_provider_candidate_event(
                    ProviderOperationalTelemetryEventName::ProviderCandidateStaged,
                    provider_id,
                    revision,
                    sha256,
                    None,
                    None,
                );
                Ok(ProviderSyncReport {
                    provider_id: provider_id.to_string(),
                    outcome: "activated",
                    revision,
                    failure_kind: None,
                    failure_message: None,
                    activation_snapshot: Some(ProviderManifestActivationSnapshot {
                        provider_id: provider_id.to_string(),
                        store: store_snapshot,
                        staged_manifest_mutation,
                    }),
                })
            }
            vulcan_provider_config::manifest_sync::SyncOutcome::Rejected {
                revision,
                failure_kind,
                failure_path,
            } => {
                emit_provider_candidate_event(
                    ProviderOperationalTelemetryEventName::ProviderCandidateRejected,
                    provider_id,
                    revision,
                    sha256,
                    Some(failure_kind.as_str()),
                    failure_path.as_deref(),
                );
                Ok(ProviderSyncReport {
                    provider_id: provider_id.to_string(),
                    outcome: "rejected",
                    revision,
                    failure_kind: Some(failure_kind),
                    failure_message: None,
                    activation_snapshot: None,
                })
            }
        }
    }

    /// Fetches one conditional GET with a strict byte bound, media-type check, and no redirect
    /// following; returns status, bytes, response ETag, and the manifest signature header.
    /// 执行一次带严格字节上限、媒体类型校验且不跟随重定向的条件 GET；返回状态、字节、
    /// 响应 ETag 与清单签名头。
    ///
    /// # Parameters
    /// 参数
    /// `url` is the exact remote URL; `if_none_match` is the persisted ETag for conditional
    /// requests; `max_bytes` is the exact cumulative byte bound for this document class
    /// (index vs provider use different limits, plan §R7.5/§R7.6).
    /// `url` 是精确远端 URL；`if_none_match` 是用于条件请求的持久化 ETag；`max_bytes` 是
    /// 该文档类的精确累计字节上限（Index 与供应商使用不同上限，方案 §R7.5/§R7.6）。
    async fn fetch_conditional(
        &self,
        url: &str,
        if_none_match: Option<&str>,
        max_bytes: u64,
    ) -> Result<(u16, Vec<u8>, Option<String>, String), String> {
        // diagnosticUrl is the only URL representation permitted in errors; the configured URL
        // may carry a signed query string that must not cross the observability boundary.
        // diagnosticUrl 是错误中唯一允许使用的 URL 表示；配置 URL 可能携带不得越过
        // 可观测性边界的签名查询串。
        let diagnostic_url = redacted_sync_url(url);
        // requested_url is the canonical endpoint identity used to detect any injected-client redirect.
        // requested_url 是用于检测任何注入客户端重定向的规范端点身份。
        let requested_url = reqwest::Url::parse(url)
            .map_err(|error| format!("Manifest sync URL `{diagnostic_url}` is invalid: {error}"))?;
        // Compatibility headers let the distribution service select one generation the current
        // client can execute; local verification remains mandatory if the server misroutes it.
        // 兼容性请求头使分发服务选择当前客户端可执行的代际；即使服务端误投，客户端仍必须独立校验。
        let mut request = self
            .http
            .get(requested_url.clone())
            .header(SYNC_CLIENT_VERSION_HEADER, env!("CARGO_PKG_VERSION"))
            .header(
                SYNC_MANIFEST_SCHEMA_HEADER,
                vulcan_provider_adapters::provider_manifest::MANIFEST_SCHEMA_VERSION,
            )
            .header(
                SYNC_MANIFEST_ENGINE_HEADER,
                vulcan_provider_adapters::provider_manifest::CURRENT_MANIFEST_ENGINE.to_string(),
            );
        if let Some(etag) = if_none_match {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(|error| {
            format!(
                "Manifest sync request failed for `{diagnostic_url}`: {}",
                error.without_url()
            )
        })?;
        if response.url() != &requested_url {
            return Err(format!(
                "Manifest sync request for `{diagnostic_url}` followed a forbidden redirect to `{}`.",
                redacted_sync_url(response.url().as_str())
            ));
        }
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok((304, Vec::new(), None, String::new()));
        }
        if !status.is_success() {
            // Redirects are never followed: the dedicated client disables them and any 3xx
            // answer fails the fetch instead of delivering a second origin's bytes (plan
            // §R7.4).
            // 绝不跟随重定向：专用客户端禁用重定向，任何 3xx 响应都使获取失败，而不是
            // 交付第二个源的字节（方案 §R7.4）。
            return Err(format!(
                "Manifest sync request for `{diagnostic_url}` failed with HTTP {}.",
                status.as_u16()
            ));
        }
        // Headers are read before the body consumes the response.
        // 头在响应体消费前读取。
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let signature = response
            .headers()
            .get("x-vulcan-manifest-signature")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        // The media type is checked before any body byte is buffered (plan §R7.5).
        // 媒体类型在缓冲任何响应体字节前校验（方案 §R7.5）。
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !is_supported_sync_content_type(content_type) {
            return Err(format!(
                "Manifest sync response for `{diagnostic_url}` has media type `{content_type}`; expected `{SYNC_ACCEPTED_CONTENT_TYPE}`."
            ));
        }
        // A declared Content-Length above the bound is rejected before buffering; an absent or
        // lying length is still caught by the streaming cumulative cap below.
        // 超过上限的已声明 Content-Length 在缓冲前被拒绝；缺失或谎报的长度仍由下方流式
        // 累计上限兜底。
        if let Some(length) = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            && length > max_bytes
        {
            return Err(format!(
                "Manifest sync response for `{diagnostic_url}` declares {length} bytes, exceeding the {max_bytes} byte bound."
            ));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| {
                format!(
                    "Manifest sync response read failed for `{diagnostic_url}`: {}",
                    error.without_url()
                )
            })?;
            // The cumulative cap fails on the next byte after the limit instead of buffering
            // the whole body (plan §R7.5).
            // 累计上限在超过上限的下一字节立即失败，而不是缓冲完整响应体（方案 §R7.5）。
            if bytes.len() as u64 + chunk.len() as u64 > max_bytes {
                return Err(format!(
                    "Manifest sync response for `{diagnostic_url}` exceeds the {max_bytes} byte bound."
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((status.as_u16(), bytes, etag, signature))
    }

    /// Builds the typed per-provider manifest status snapshot (plan §R10).
    /// 构建 typed 逐供应商清单状态快照（方案 §R10）。
    ///
    /// The snapshot exposes bundled/active/previous revision and hash, the active source
    /// layer, sync state, quarantine summaries, and whether remote sync is configured — so
    /// the UI/CLI can answer "which manifest is in use, why, and whether it rolled back"
    /// without reading logs (plan §R10 exit condition).
    /// 快照暴露 bundled/active/previous 修订与摘要、active 来源层、同步状态、隔离摘要与
    /// 远端同步是否配置——使 UI/CLI 无需读日志即可回答"当前使用哪份清单、为何选择、
    /// 是否回滚"（方案 §R10 退出条件）。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the single authoritative catalog; `provider_id` is exact.
    /// `runtime` 持有唯一权威目录；`provider_id` 为精确标识。
    pub fn provider_manifest_status(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
    ) -> Result<ProviderManifestStatus, String> {
        require_trusted_system_manifest_provider(runtime, provider_id)?;
        // configuration is retained with the runtime generation snapshot for all model facts.
        // configuration 与运行时代际快照共同保留，用于全部模型事实。
        let configuration = runtime.configuration_snapshot()?;
        // runtimeSnapshot pins Manifest, discovery, support, and effective catalogs together.
        // runtimeSnapshot 把 Manifest、发现、支持与有效目录固定在同一快照中。
        let runtime_snapshot =
            runtime.provider_manifest_runtime_snapshot(provider_id, &configuration)?;
        let active_manifest = runtime_snapshot
            .as_ref()
            .map(|snapshot| Arc::clone(&snapshot.manifest));
        let active = active_manifest
            .as_ref()
            .map(|manifest| manifest_active_status(manifest));

        // bundledFailure is retained by the immutable runtime generation when the globally
        // trusted index identified this provider but its local baseline failed verification.
        // bundledFailure 在全局可信 Index 已识别该供应商、但其本地基线未通过校验时，由
        // 不可变运行时代际保留。
        let catalog_bundled_failure = runtime.bundled_manifest_failure(provider_id)?;
        // bundledLoad independently reads and compiles a healthy baseline for layer identity and
        // first-update semantic comparison. A provider-local failure becomes status data instead
        // of aborting the aggregate status response (plan §17.5).
        // bundledLoad 独立读取并编译健康基线，用于层身份与首次更新语义比较。逐供应商失败
        // 会成为状态数据，而不是中止聚合状态响应（方案 §17.5）。
        let bundled_load = if let Some(failure) = catalog_bundled_failure.as_ref() {
            Err(failure.clone())
        } else {
            (|| {
                let bundled_bytes = vulcan_provider_adapters::provider_manifest::read_bundled_provider_manifest_bytes(
                    provider_id,
                )?;
                let bundled_text = std::str::from_utf8(&bundled_bytes).map_err(|error| {
                    format!("Bundled provider manifest `{provider_id}` is not UTF-8: {error}")
                })?;
                let bundled_manifest = vulcan_provider_adapters::provider_manifest::compile_manifest_text_with_layer(
                    bundled_text,
                    vulcan_provider_adapters::ManifestOwnership::System,
                    vulcan_provider_adapters::ManifestSourceLayer::Bundled,
                )
                .map_err(|error| {
                    format!("Bundled provider manifest `{provider_id}` is invalid: {error}")
                })?;
                if bundled_manifest.descriptor.id != provider_id {
                    return Err(format!(
                        "Bundled provider manifest `{provider_id}` declares provider `{}`.",
                        bundled_manifest.descriptor.id
                    ));
                }
                let bundled = ManifestLayerStatus {
                    revision: bundled_manifest.revision,
                    sha256: vulcan_provider_config::manifest_store::ManifestStoreLayout::sha256_hex(
                        &bundled_bytes,
                    ),
                };
                Ok((bundled, bundled_manifest))
            })()
            .map_err(|message: String| {
                vulcan_provider_adapters::provider_manifest::BundledProviderManifestFailure {
                    failure_kind: String::from("bundled_baseline_invalid"),
                    message,
                }
            })
        };
        let (bundled, bundled_manifest, bundled_failure) = match bundled_load {
            Ok((bundled, manifest)) => (Some(bundled), Some(manifest), None),
            Err(failure) => (
                None,
                None,
                Some(ProviderManifestCriticalErrorStatus {
                    failure_kind: failure.failure_kind,
                    message: bounded_public_manifest_diagnostic(failure.message.as_str()),
                }),
            ),
        };
        // criticalError is present only when the provider has no executable trusted runtime layer.
        // criticalError 仅在供应商没有可执行可信运行时层时存在。
        let critical_error = if active_manifest.is_none() {
            bundled_failure.clone()
        } else {
            None
        };

        let store = &self.store;
        let loaded_state = store.load_sync_state(provider_id)?;
        // Previous is reported only when state authenticates its exact bytes; a stale file is
        // never presented to operators as a rollback option.
        // 仅当状态认证 Previous 的精确字节时才报告该层；绝不把过期文件展示为可回滚项。
        let (previous, previous_manifest) = match &loaded_state {
            vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) => {
                match (state.previous.as_ref(), store.read_previous(provider_id)?) {
                    (Some(expected), Some(text)) => match verify_stored_system_manifest(
                        provider_id,
                        &text,
                        vulcan_provider_adapters::ManifestSourceLayer::Previous,
                        expected,
                    ) {
                        Ok(manifest) => (
                            Some(ManifestLayerStatus {
                                revision: expected.revision,
                                sha256: expected.sha256.clone(),
                            }),
                            Some(manifest),
                        ),
                        Err(_error) => (None, None),
                    },
                    _ => (None, None),
                }
            }
            vulcan_provider_config::manifest_store::SyncStateLoad::Absent => (None, None),
        };

        let sync_state = match loaded_state {
            vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) => {
                Some(ManifestSyncStateView {
                    revision: state.revision,
                    sha256: state.sha256,
                    etag: state.etag,
                    index_etag: state.index_etag,
                    last_modified: state.last_modified,
                    synced_at_unix_ms: state.synced_at_unix_ms,
                    last_error_kind: state.last_error_kind,
                })
            }
            vulcan_provider_config::manifest_store::SyncStateLoad::Absent => None,
        };
        // syncCheckState remains visible even when a rejected first candidate created no Active state.
        // syncCheckState 即使首次候选被拒且未创建 Active 状态也保持可见。
        let sync_check_state = match store.load_check_state(provider_id)? {
            CheckStateLoad::Present(state) => Some(ManifestCheckStateView {
                checked_at_unix_ms: state.checked_at_unix_ms,
                last_successful_at_unix_ms: state.last_successful_at_unix_ms,
                outcome: state.outcome,
                last_error_kind: state.last_error_kind,
            }),
            CheckStateLoad::Absent => None,
        };

        let quarantine = store.quarantine_summaries(provider_id)?;
        let model_rules = active_manifest
            .as_ref()
            .map(|manifest| {
                manifest
                    .rule_instances
                    .iter()
                    .map(|rule| ManifestModelRuleStatus {
                        rule_id: rule.rule_id.clone(),
                        service_entry_id: rule.service_entry_id.clone(),
                        credential_profile_ids: rule.credential_profile_ids.clone(),
                        matcher: rule.matcher.clone(),
                        supported_protocols: rule.supported_protocols.clone(),
                        selected_protocol: rule.selected_protocol.clone(),
                        default_enabled: rule.default_enabled,
                        model_parameter_ids: rule
                            .model_parameters
                            .iter()
                            .map(|parameter| parameter.id.clone())
                            .collect(),
                        usage_profile_refs: rule.usage_profile_refs.clone(),
                        request_profile_refs: rule.request_profile_refs.clone(),
                        response_profile_refs: rule.response_profile_refs.clone(),
                        operation_refs: rule.operation_refs.clone(),
                        input_resource_constraint_count: rule.input_resource_constraints.len(),
                        field_provenance: rule.field_provenance.clone(),
                        capability_provenance: rule.capability_provenance.clone(),
                        provenance: rule.provenance.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        // completeModelBindings remain unbounded only inside this bounded compiled manifest so
        // operation summaries can count every concrete model before response truncation.
        // completeModelBindings 仅在此有界编译 Manifest 内保持完整，使 operation 摘要能在
        // 响应截断前统计每个具体模型。
        let complete_model_bindings = match (&active_manifest, &runtime_snapshot) {
            (Some(manifest), Some(snapshot)) => manifest_model_binding_statuses(
                manifest,
                &configuration,
                &snapshot.effective_catalogs,
                &snapshot.model_support_catalogs,
                &snapshot.source_catalogs,
            )?,
            _ => Vec::new(),
        };
        // diffBaseline selects authenticated Previous first; the immutable Bundled layer is the
        // first-update fallback only when the active runtime no longer uses Bundled itself.
        // diffBaseline 优先选择已认证 Previous；仅当 active 运行时已不再使用 Bundled 时，
        // 不可变 Bundled 层才作为首次更新回退基线。
        let (active_model_binding_change_baseline, diff_baseline_manifest): (
            Option<ManifestModelBindingChangeBaselineStatus>,
            Option<&vulcan_provider_adapters::ResolvedProviderManifest>,
        ) = if let (Some(previous_status), Some(previous_manifest)) =
            (previous.as_ref(), previous_manifest.as_ref())
        {
            (
                Some(ManifestModelBindingChangeBaselineStatus {
                    layer: ManifestActiveLayer::Previous,
                    revision: previous_status.revision,
                    sha256: previous_status.sha256.clone(),
                }),
                Some(previous_manifest.as_ref()),
            )
        } else if active_manifest.as_ref().is_some_and(|manifest| {
            manifest.snapshot_provenance.source_layer
                != vulcan_provider_adapters::ManifestSourceLayer::Bundled
        }) {
            match (bundled.as_ref(), bundled_manifest.as_ref()) {
                (Some(bundled_status), Some(bundled_manifest)) => (
                    Some(ManifestModelBindingChangeBaselineStatus {
                        layer: ManifestActiveLayer::Bundled,
                        revision: bundled_status.revision,
                        sha256: bundled_status.sha256.clone(),
                    }),
                    Some(bundled_manifest),
                ),
                _ => (None, None),
            }
        } else {
            (None, None)
        };
        // activeModelBindingChanges compares independent Manifest identity projections while
        // preserving shared discovery/manual evidence from the captured runtime generation.
        // activeModelBindingChanges 比较相互独立的 Manifest 身份投影，同时保留已捕获运行时
        // 代际中的共享发现/手工证据。
        let (
            active_model_binding_changes,
            active_model_binding_change_total_count,
            active_model_binding_changes_truncated,
        ) = match (
            active_manifest.as_ref(),
            diff_baseline_manifest,
            runtime_snapshot.as_ref(),
        ) {
            (Some(active_manifest), Some(baseline_manifest), Some(snapshot)) => {
                // effectiveExternalCatalogs excludes active static identities so each Manifest
                // reconstructs its own added/retired identity set.
                // effectiveExternalCatalogs 排除 active 静态身份，使每份 Manifest 分别重建自身
                // 新增/退役身份集合。
                let effective_external_catalogs =
                    manifest_binding_diff_external_catalogs(&snapshot.effective_catalogs);
                // supportExternalCatalogs retains access-independent discovery/manual evidence.
                // supportExternalCatalogs 保留独立于访问开关的发现/手工证据。
                let support_external_catalogs =
                    manifest_binding_diff_external_catalogs(&snapshot.model_support_catalogs);
                // sourceExternalCatalogs retains the immutable external evidence layer.
                // sourceExternalCatalogs 保留不可变外部证据层。
                let source_external_catalogs =
                    manifest_binding_diff_external_catalogs(&snapshot.source_catalogs);
                // baselineBindings are reconstructed from the authenticated baseline Manifest.
                // baselineBindings 从已认证基线 Manifest 重建。
                let baseline_bindings = manifest_model_binding_statuses(
                    baseline_manifest,
                    &configuration,
                    &effective_external_catalogs,
                    &support_external_catalogs,
                    &source_external_catalogs,
                )?;
                // activeBindings use the same external evidence but the active Manifest identity.
                // activeBindings 使用相同外部证据与 active Manifest 身份。
                let active_bindings = manifest_model_binding_statuses(
                    active_manifest,
                    &configuration,
                    &effective_external_catalogs,
                    &support_external_catalogs,
                    &source_external_catalogs,
                )?;
                diff_manifest_model_bindings(&baseline_bindings, &active_bindings)
            }
            _ => (Vec::new(), 0, false),
        };
        // modelBindingTotalCount makes response truncation explicit.
        // modelBindingTotalCount 使响应截断显式化。
        let model_binding_total_count = complete_model_bindings.len();
        let operations = active_manifest
            .as_ref()
            .map(|manifest| manifest_operation_statuses(manifest, &complete_model_bindings))
            .unwrap_or_default();
        // modelBindings is the bounded browser projection derived after complete aggregates.
        // modelBindings 是在完整聚合派生后生成的有界浏览器投影。
        let mut model_bindings = complete_model_bindings;
        model_bindings.truncate(MANIFEST_STATUS_MODEL_BINDING_MAX_ITEMS);
        // modelBindingsTruncated prevents a bounded response from masquerading as full inventory.
        // modelBindingsTruncated 防止有界响应伪装成完整清单。
        let model_bindings_truncated = model_bindings.len() < model_binding_total_count;
        let tool_target_template_instances = active_manifest
            .as_ref()
            .map(|manifest| {
                manifest
                    .tool_target_template_instances
                    .values()
                    .map(|instance| ManifestToolTargetTemplateInstanceStatus {
                        target_id: instance.target_id.clone(),
                        template_id: instance.template_id.clone(),
                        service_entry_id: instance.service_entry_id.clone(),
                        model_id: instance.model_id.clone(),
                        model_display_name: instance.model_display_name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let declaration_provenance = active_manifest
            .as_ref()
            .map(|manifest| manifest.declaration_provenance.clone())
            .unwrap_or_default();
        let retained_telemetry = provider_operational_telemetry_snapshot(provider_id, usize::MAX)?;
        let recent_telemetry_total_count = retained_telemetry.len();
        let recent_telemetry_events = retained_telemetry
            .into_iter()
            .take(MANIFEST_STATUS_TELEMETRY_MAX_ITEMS)
            .collect::<Vec<_>>();
        let recent_telemetry_events_truncated =
            recent_telemetry_events.len() < recent_telemetry_total_count;

        Ok(ProviderManifestStatus {
            provider_id: provider_id.to_string(),
            status_error: None,
            critical_error,
            bundled_failure,
            bundled,
            active,
            previous,
            sync_state,
            sync_check_state,
            recent_telemetry_events,
            recent_telemetry_total_count,
            recent_telemetry_events_truncated,
            quarantine,
            sync_configured: self.remote_index_url.is_some() && !self.trusted_keys.is_empty(),
            model_rules,
            model_bindings,
            model_binding_total_count,
            model_bindings_truncated,
            active_model_binding_change_baseline,
            active_model_binding_changes,
            active_model_binding_change_total_count,
            active_model_binding_changes_truncated,
            operations,
            tool_target_template_instances,
            declaration_provenance,
        })
    }

    /// Builds the typed aggregate status snapshot for every system provider (plan §R10).
    /// 为全部系统供应商构建 typed 聚合状态快照（方案 §R10）。
    pub fn provider_manifest_status_all(
        &self,
        runtime: &PublicAiRuntime,
    ) -> Result<Vec<ProviderManifestStatus>, String> {
        let provider_ids = runtime.system_manifest_provider_ids()?;
        Ok(provider_ids
            .into_iter()
            .map(|provider_id| {
                self.provider_manifest_status(runtime, &provider_id)
                    .unwrap_or_else(|error| {
                        self.provider_manifest_status_projection_failure(
                            runtime,
                            &provider_id,
                            error,
                        )
                    })
            })
            .collect())
    }

    /// Builds a bounded provider-local fallback when detailed status projection itself fails.
    /// 在详细状态投影自身失败时构建有界的逐供应商回退。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` retains any executable immutable layer, `provider_id` is exact, and `error` is
    /// the local projection failure that must not suppress unrelated provider statuses.
    /// `runtime` 保留任何可执行不可变层，`provider_id` 为精确身份，`error` 是不得压制
    /// 无关供应商状态的本地投影失败。
    ///
    /// # Returns
    /// 返回值
    /// A structurally complete status with the known active layer and explicit unavailable fields.
    /// 返回结构完整的状态，保留已知 active 层并显式标记不可用字段。
    fn provider_manifest_status_projection_failure(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
        error: String,
    ) -> ProviderManifestStatus {
        let status_error = ProviderManifestCriticalErrorStatus {
            failure_kind: String::from("status_projection_failed"),
            message: bounded_public_manifest_diagnostic(error.as_str()),
        };
        let active = runtime
            .system_manifest(provider_id)
            .as_deref()
            .map(manifest_active_status);
        let bundled_failure = runtime
            .bundled_manifest_failure(provider_id)
            .ok()
            .flatten()
            .map(|failure| ProviderManifestCriticalErrorStatus {
                failure_kind: failure.failure_kind,
                message: bounded_public_manifest_diagnostic(failure.message.as_str()),
            });
        let critical_error = if active.is_none() {
            Some(
                bundled_failure
                    .clone()
                    .unwrap_or_else(|| status_error.clone()),
            )
        } else {
            None
        };
        let retained_telemetry = provider_operational_telemetry_snapshot(
            provider_id,
            MANIFEST_STATUS_TELEMETRY_MAX_ITEMS,
        )
        .unwrap_or_default();
        let recent_telemetry_total_count = retained_telemetry.len();
        ProviderManifestStatus {
            provider_id: provider_id.to_string(),
            status_error: Some(status_error),
            critical_error,
            bundled_failure,
            bundled: None,
            active,
            previous: None,
            sync_state: None,
            sync_check_state: None,
            recent_telemetry_events: retained_telemetry,
            recent_telemetry_total_count,
            recent_telemetry_events_truncated: false,
            quarantine: Vec::new(),
            sync_configured: self.remote_index_url.is_some() && !self.trusted_keys.is_empty(),
            model_rules: Vec::new(),
            model_bindings: Vec::new(),
            model_binding_total_count: 0,
            model_bindings_truncated: false,
            active_model_binding_change_baseline: None,
            active_model_binding_changes: Vec::new(),
            active_model_binding_change_total_count: 0,
            active_model_binding_changes_truncated: false,
            operations: Vec::new(),
            tool_target_template_instances: Vec::new(),
            declaration_provenance: BTreeMap::new(),
        }
    }

    /// Rolls back one provider's active manifest to the previous layer and hot-replaces the
    /// runtime (plan §R10 explicit local rollback).
    /// 把一个供应商的 active 清单回滚到 previous 层并热替换运行时（方案 §R10 显式本地回滚）。
    ///
    /// # Returns
    /// 返回值
    /// The restored revision, or a controlled failure when no previous layer exists.
    /// 返回恢复的修订号，或 previous 层不存在时的受控失败。
    pub fn rollback_provider_manifest(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
    ) -> Result<ProviderRollbackReport, String> {
        require_trusted_system_manifest_provider(runtime, provider_id)?;
        let current_state = match self.store.load_sync_state(provider_id)? {
            vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) => state,
            vulcan_provider_config::manifest_store::SyncStateLoad::Absent => {
                return Err(format!(
                    "Provider `{provider_id}` has no authenticated sync state to roll back."
                ));
            }
        };
        let previous = current_state.previous.as_ref().ok_or_else(|| {
            format!("Provider `{provider_id}` has no authenticated Previous version.")
        })?;
        let previous_text = self.store.read_previous(provider_id)?.ok_or_else(|| {
            format!("Provider `{provider_id}` has no Previous layer to roll back.")
        })?;
        let resolved = verify_stored_system_manifest(
            provider_id,
            &previous_text,
            vulcan_provider_adapters::ManifestSourceLayer::Previous,
            previous,
        )?;
        let restored_state = swapped_manifest_sync_state(&current_state)?;
        let store_snapshot = self.store.capture_system_snapshot(provider_id)?;
        // The file/state pair is committed before the immutable runtime publication. Every
        // failure restores the former pair, so callers never observe a successful half rollback.
        // 在不可变运行时发布前提交文件/状态对。任意失败都会恢复原有配对，使调用方绝不
        // 会观察到成功的半回滚。
        self.store
            .begin_system_transaction(provider_id, &store_snapshot)?;
        if let Err(error) = self.store.swap_active_previous(provider_id) {
            let restore = self
                .store
                .abort_system_transaction(provider_id, &store_snapshot);
            return Err(format!(
                "Provider `{provider_id}` rollback file swap failed: {error}; transaction abort: {restore:?}"
            ));
        }
        if let Err(error) = self.store.save_sync_state(provider_id, &restored_state) {
            let store_restore = self
                .store
                .abort_system_transaction(provider_id, &store_snapshot);
            return Err(format!(
                "Provider `{provider_id}` rollback state write failed: {error}; store restore: {store_restore:?}"
            ));
        }
        let staged_manifest_mutation = match runtime
            .replace_system_manifest_transactional(provider_id, resolved)
        {
            Ok(mutation) => mutation,
            Err(error) => {
                let store_restore = self
                    .store
                    .abort_system_transaction(provider_id, &store_snapshot);
                return Err(format!(
                    "Provider `{provider_id}` runtime rollback publication failed: {error}; store restore: {store_restore:?}"
                ));
            }
        };
        Ok(ProviderRollbackReport {
            provider_id: provider_id.to_string(),
            restored_revision: previous.revision,
            activation_snapshot: ProviderManifestActivationSnapshot {
                provider_id: provider_id.to_string(),
                store: store_snapshot,
                staged_manifest_mutation,
            },
        })
    }

    /// Resolves the authenticated SHA-256 identity of one currently active system manifest.
    /// 解析一份当前 active 系统 Manifest 的已认证 SHA-256 身份。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `manifest` identify the exact active runtime snapshot.
    /// `provider_id` 与 `manifest` 标识精确 active 运行时快照。
    ///
    /// # Returns
    /// 返回值
    /// A digest from snapshot provenance, matching sync state, or exact bundled bytes.
    /// 返回来自快照来源、匹配同步状态或精确内置字节的摘要。
    fn active_manifest_sha256(
        &self,
        provider_id: &str,
        manifest: &vulcan_provider_adapters::ResolvedProviderManifest,
    ) -> Result<Option<String>, String> {
        if let Some(sha256) = manifest.snapshot_provenance.sha256.clone() {
            return Ok(Some(sha256));
        }
        // syncState authenticates Active/Previous bytes only when its revision matches runtime.
        // syncState 仅在其修订与运行时匹配时认证 Active/Previous 字节。
        if let SyncStateLoad::Present(state) = self.store.load_sync_state(provider_id)?
            && state.revision == manifest.revision
        {
            return Ok(Some(state.sha256));
        }
        if manifest.snapshot_provenance.source_layer
            == vulcan_provider_adapters::ManifestSourceLayer::Bundled
        {
            // bundledPath is the immutable source file for this system provider identity.
            // bundledPath 是此系统供应商身份的不可变源文件。
            // bundledBytes are hashed exactly as shipped rather than reserialized.
            // bundledBytes 按随包精确字节求哈希，而不是重新序列化。
            let bundled_bytes =
                vulcan_provider_adapters::provider_manifest::read_bundled_provider_manifest_bytes(
                    provider_id,
                )?;
            return Ok(Some(ManifestStoreLayout::sha256_hex(&bundled_bytes)));
        }
        Ok(None)
    }

    /// Captures one coherent local baseline for a later read-only candidate preview.
    /// 为后续只读候选预览捕获一份一致本地基线。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the installed configuration and generation; `provider_id` is exact.
    /// `runtime` 持有已安装配置与代际；`provider_id` 为精确标识。
    ///
    /// # Returns
    /// 返回值
    /// An owned baseline safe to retain after transaction guards are released.
    /// 返回可在事务守卫释放后安全保留的自有基线。
    pub(crate) fn capture_preview_baseline(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
    ) -> Result<ProviderManifestPreviewBaseline, String> {
        require_trusted_system_manifest_provider(runtime, provider_id)?;
        let configuration = runtime.configuration_snapshot()?;
        let runtime_snapshot = runtime
            .provider_manifest_runtime_snapshot(provider_id, &configuration)?
            .ok_or_else(|| {
                format!("System provider `{provider_id}` is missing from the runtime catalog.")
            })?;
        Ok(ProviderManifestPreviewBaseline {
            configuration,
            runtime_snapshot,
        })
    }

    /// Downloads, verifies, compiles, and diffs one signed candidate without any local write.
    /// 下载、验证、编译并对比一个签名候选，且不执行任何本地写入。
    ///
    /// The method never calls staging, quarantine, activation, sync-state, or check-state APIs.
    /// 该方法绝不调用 staging、quarantine、activation、sync-state 或 check-state API。
    ///
    /// # Parameters
    /// 参数
    /// `baseline` owns the immutable current authority and `provider_id` selects one system provider.
    /// `baseline` 持有不可变当前权威，`provider_id` 选择一个系统供应商。
    ///
    /// # Returns
    /// 返回值
    /// A typed preview for configured candidate outcomes, or a top-level signed-index/network error.
    /// 返回已配置候选结果的类型化预览，或顶层签名 Index/网络错误。
    pub async fn preview_provider_manifest(
        &self,
        baseline: ProviderManifestPreviewBaseline,
        provider_id: &str,
    ) -> Result<ProviderManifestCandidatePreview, String> {
        // baseline is owned, so activation and settings locks are never held during network I/O.
        // baseline 由本方法持有，因此网络 I/O 期间绝不持有激活与设置锁。
        let ProviderManifestPreviewBaseline {
            configuration,
            runtime_snapshot,
        } = baseline;
        // current is required up front so arbitrary remote provider ids cannot be queried.
        // current 预先必需，使任意远端供应商标识不能被查询。
        let current = Arc::clone(&runtime_snapshot.manifest);
        // currentRevision is the immutable local comparison baseline.
        // currentRevision 是不可变本地比较基线。
        let current_revision = Some(current.revision);
        let Some(index_url) = self.remote_index_url.as_deref() else {
            return Ok(manifest_preview_without_diffs(
                provider_id,
                ProviderManifestPreviewOutcome::NotConfigured,
                current_revision,
                None,
                None,
                false,
                None,
                None,
            ));
        };
        index_origin(index_url)?;
        // indexResponse is unconditional so preview never trusts or mutates persisted ETag state.
        // indexResponse 是无条件请求，使预览绝不信任或修改持久化 ETag 状态。
        let (index_status, index_bytes, _index_etag, index_signature) = {
            let _index_guard = self.index_flight.lock().await;
            self.fetch_conditional(index_url, None, REMOTE_INDEX_MAX_BYTES)
                .await?
        };
        if index_status == 304 {
            return Err(String::from(
                "Manifest preview received HTTP 304 for an unconditional signed-index request.",
            ));
        }
        // signature is verified over the exact bounded index bytes before any JSON trust decision.
        // signature 在任何 JSON 信任决策前针对精确有界 Index 字节验证。
        let signature = decode_signature_header(&index_signature)?;
        // index is the single authenticated candidate directory snapshot used below.
        // index 是下方使用的唯一已认证候选目录快照。
        let index = verify_and_parse_index(
            &index_bytes,
            &signature,
            &self.trusted_keys,
            current_unix_millis() as i64,
        )?;
        let Some(entry) = index.providers.get(provider_id) else {
            return Ok(manifest_preview_without_diffs(
                provider_id,
                ProviderManifestPreviewOutcome::NotPinned,
                current_revision,
                None,
                None,
                false,
                None,
                None,
            ));
        };
        // baseOrigin and providerUrl enforce the same-origin relative URL contract as activation.
        // baseOrigin 与 providerUrl 执行和激活相同的同源相对 URL 契约。
        let base_origin = index_origin(index_url)?;
        let provider_url = resolve_remote_provider_url(&base_origin, &entry.relative_url)?;
        // providerResponse is unconditional and remains entirely in memory.
        // providerResponse 是无条件请求且完全保留在内存中。
        let (provider_status, provider_bytes, _provider_etag, _provider_signature) = self
            .fetch_conditional(&provider_url, None, REMOTE_PROVIDER_MAX_BYTES)
            .await?;
        if provider_status == 304 {
            return Err(String::from(
                "Manifest preview received HTTP 304 for an unconditional provider request.",
            ));
        }
        // candidate compiles through the same trust-ceiling verifier as real activation.
        // candidate 通过与真实激活相同的信任上限验证器编译。
        let candidate = match verify_provider_bytes(entry, &provider_bytes) {
            Ok(candidate) => candidate,
            Err(error) => {
                return Ok(manifest_preview_without_diffs(
                    provider_id,
                    ProviderManifestPreviewOutcome::Rejected,
                    current_revision,
                    Some(entry.revision),
                    Some(entry.sha256.clone()),
                    false,
                    Some(error.failure_kind().to_string()),
                    Some(bounded_public_manifest_diagnostic(
                        error.to_string().as_str(),
                    )),
                ));
            }
        };
        if entry.revision < current.revision {
            return Ok(manifest_preview_without_diffs(
                provider_id,
                ProviderManifestPreviewOutcome::Rejected,
                current_revision,
                Some(entry.revision),
                Some(entry.sha256.clone()),
                false,
                Some(String::from("revision_downgrade")),
                Some(format!(
                    "Signed candidate revision {} is older than active revision {}.",
                    entry.revision, current.revision
                )),
            ));
        }
        if entry.revision == current.revision {
            // currentSha256 must prove byte identity before `up_to_date` can be reported.
            // currentSha256 必须证明字节身份后才能报告 `up_to_date`。
            let current_sha256 = self.active_manifest_sha256(provider_id, &current)?;
            if current_sha256.as_deref() == Some(entry.sha256.as_str()) {
                return Ok(manifest_preview_without_diffs(
                    provider_id,
                    ProviderManifestPreviewOutcome::UpToDate,
                    current_revision,
                    Some(entry.revision),
                    Some(entry.sha256.clone()),
                    true,
                    None,
                    None,
                ));
            }
            return Ok(manifest_preview_without_diffs(
                provider_id,
                ProviderManifestPreviewOutcome::Rejected,
                current_revision,
                Some(entry.revision),
                Some(entry.sha256.clone()),
                false,
                Some(String::from("same_revision_conflict")),
                Some(String::from(
                    "Signed candidate reuses the active revision with different or unverifiable bytes.",
                )),
            ));
        }
        // currentBindings and candidateBindings are complete before any response truncation.
        // currentBindings 与 candidateBindings 在任何响应截断前保持完整。
        let current_bindings = manifest_model_binding_statuses(
            &current,
            &configuration,
            &runtime_snapshot.effective_catalogs,
            &runtime_snapshot.model_support_catalogs,
            &runtime_snapshot.source_catalogs,
        )?;
        let candidate_bindings = manifest_model_binding_statuses(
            &candidate,
            &configuration,
            &runtime_snapshot.effective_catalogs,
            &runtime_snapshot.model_support_catalogs,
            &runtime_snapshot.source_catalogs,
        )?;
        // currentOperations and candidateOperations contain complete semantic operation facts.
        // currentOperations 与 candidateOperations 包含完整语义 operation 事实。
        let current_operations = manifest_operation_statuses(&current, &current_bindings);
        let candidate_operations = manifest_operation_statuses(&candidate, &candidate_bindings);
        // modelDiff and operationDiff are computed only from backend-resolved effective facts.
        // modelDiff 与 operationDiff 仅从后端解析的有效事实计算。
        let (
            model_binding_changes,
            model_binding_change_total_count,
            model_binding_changes_truncated,
        ) = diff_manifest_model_bindings(&current_bindings, &candidate_bindings);
        let (operation_changes, operation_change_total_count, operation_changes_truncated) =
            diff_manifest_operations(&current_operations, &candidate_operations);
        // unavailableCandidate is impossible for a fully closed catalog but remains explicit.
        // unavailableCandidate 对完整封闭目录本不应出现，但仍保持显式。
        let unavailable_candidate = candidate_bindings
            .iter()
            .find(|binding| !binding.executable);
        Ok(ProviderManifestCandidatePreview {
            provider_id: provider_id.to_string(),
            outcome: if unavailable_candidate.is_some() {
                ProviderManifestPreviewOutcome::Rejected
            } else {
                ProviderManifestPreviewOutcome::Candidate
            },
            current_revision,
            candidate_revision: Some(entry.revision),
            candidate_sha256: Some(entry.sha256.clone()),
            executable: unavailable_candidate.is_none(),
            failure_kind: unavailable_candidate.map(|_| String::from("unexecutable_model_binding")),
            failure_message: unavailable_candidate
                .and_then(|binding| binding.unavailable_reason.clone())
                .map(|message| bounded_public_manifest_diagnostic(message.as_str())),
            model_binding_changes,
            model_binding_change_total_count,
            model_binding_changes_truncated,
            operation_changes,
            operation_change_total_count,
            operation_changes_truncated,
        })
    }

    /// Synchronizes one exact provider through the reviewed production flow.
    /// 通过已审核生产流程同步一个精确供应商。
    ///
    /// This is the single-provider RPC entry; it reuses the same signed remote flow as
    /// `sync_all` while scoping the report to one provider (plan §R10).
    /// 这是单供应商 RPC 入口；它复用与 `sync_all` 相同的签名远端流程，但把报告限定在
    /// 单个供应商（方案 §R10）。
    pub async fn sync_one(
        &self,
        runtime: &PublicAiRuntime,
        provider_id: &str,
    ) -> Result<ProviderSyncReport, String> {
        self.sync_provider_with_file_transaction_mode(runtime, provider_id, false)
            .await
    }
}

/// Decodes one hex-encoded Ed25519 signature header value.
/// 解码一个十六进制编码的 Ed25519 签名头值。
fn decode_signature_header(value: &str) -> Result<Vec<u8>, String> {
    if value.len() != 128 {
        return Err(String::from(
            "Remote manifest signature header must be 64 bytes encoded as 128 hex characters.",
        ));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| String::from("Remote manifest signature header is not valid hex."))
        })
        .collect()
}

/// Extracts the HTTPS origin (scheme + authority) from one reviewed index URL.
/// 从一条已审核 Index URL 提取 HTTPS 源（scheme + authority）。
fn index_origin(index_url: &str) -> Result<String, String> {
    // diagnosticUrl prevents configured signed-query material or user information from entering
    // validation errors before the endpoint has passed its trust checks.
    // diagnosticUrl 防止配置的签名查询材料或用户信息在端点通过信任校验前进入校验错误。
    let diagnostic_url = redacted_sync_url(index_url);
    // parsed_index gives every caller one URL-standard interpretation of the configured endpoint.
    // parsed_index 为所有调用方提供配置端点的唯一 URL 标准解释。
    let parsed_index = reqwest::Url::parse(index_url)
        .map_err(|error| format!("Remote index URL `{diagnostic_url}` is invalid: {error}"))?;
    if parsed_index.scheme() != "https" {
        return Err(format!(
            "Remote index URL `{diagnostic_url}` must use HTTPS."
        ));
    }
    if parsed_index.host_str().is_none()
        || !parsed_index.username().is_empty()
        || parsed_index.password().is_some()
        || index_url.contains('@')
        || index_url.contains('\\')
        || parsed_index.fragment().is_some()
    {
        return Err(format!(
            "Remote index URL `{diagnostic_url}` must have a credential-free HTTPS authority and no fragment."
        ));
    }
    Ok(parsed_index.origin().ascii_serialization())
}

/// Returns the current unix milliseconds, or zero when the system clock is unavailable.
/// 返回当前 Unix 毫秒；系统时钟不可用时返回零。
fn current_unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one valid per-provider sync state for conditional-Index tests.
    /// 为条件式 Index 测试构建一份有效逐供应商同步状态。
    ///
    /// # Parameters
    /// 参数
    /// `revision` and `digest_byte` create a distinct active identity; `index_etag` is the exact
    /// shared remote Index identity under test.
    /// `revision` 与 `digest_byte` 创建不同活动身份；`index_etag` 是待测的精确共享远端
    /// Index 身份。
    ///
    /// # Returns
    /// 返回值
    /// One structurally valid bounded state accepted by the manifest store.
    /// 返回一份 Manifest Store 接受的结构有效有界状态。
    fn conditional_index_state(
        revision: u64,
        digest_byte: char,
        index_etag: &str,
    ) -> vulcan_provider_config::manifest_store::ManifestSyncState {
        vulcan_provider_config::manifest_store::ManifestSyncState {
            revision,
            sha256: digest_byte.to_string().repeat(64),
            previous: None,
            etag: None,
            index_etag: Some(index_etag.to_string()),
            last_modified: None,
            synced_at_unix_ms: revision,
            last_error_kind: None,
        }
    }

    /// Verifies a provider 304 updates validators without inventing a new activation time.
    /// 验证供应商 304 只更新验证器，不虚构新的激活时间。
    #[test]
    fn provider_not_modified_preserves_successful_activation_time() {
        let mut current = conditional_index_state(7, 'a', "\"index-old\"");
        current.synced_at_unix_ms = 123_456;
        current.last_error_kind = Some(String::from("transient_failure"));

        let next = provider_not_modified_sync_state(&current, Some(String::from("\"index-new\"")));

        assert_eq!(next.revision, current.revision);
        assert_eq!(next.sha256, current.sha256);
        assert_eq!(next.synced_at_unix_ms, 123_456);
        assert_eq!(next.index_etag.as_deref(), Some("\"index-new\""));
        assert!(next.last_error_kind.is_none());
    }

    /// Verifies batch synchronization uses a conditional ETag only for complete consensus.
    /// 验证批量同步仅在状态完整一致时使用条件式 ETag。
    #[test]
    fn shared_index_etag_requires_complete_provider_consensus() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            Some(String::from("https://example.test/index.json")),
        );
        let providers = [String::from("alpha"), String::from("beta")];
        service
            .store
            .save_sync_state("alpha", &conditional_index_state(1, 'a', "\"index-7\""))
            .expect("alpha state should save");
        assert_eq!(
            service.shared_index_etag(&providers),
            None,
            "an absent provider state must force an unconditional fetch"
        );
        service
            .store
            .save_sync_state("beta", &conditional_index_state(2, 'b', "\"index-7\""))
            .expect("beta state should save");
        assert_eq!(
            service.shared_index_etag(&providers).as_deref(),
            Some("\"index-7\"")
        );
        service
            .store
            .save_sync_state("beta", &conditional_index_state(2, 'b', "\"index-8\""))
            .expect("divergent beta state should save");
        assert_eq!(
            service.shared_index_etag(&providers),
            None,
            "divergent authenticated Index observations must not be guessed"
        );
    }

    /// Verifies a batch conditional request also requires every pinned Active layer to revalidate.
    /// 验证批量条件请求还要求每个固定的 Active 层均可重新校验。
    #[test]
    fn shared_revalidated_index_etag_requires_complete_active_layers() {
        // directory isolates the durable manifest store used by this proof.
        // directory 隔离本次证明所使用的持久 Manifest 存储。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // service has no live signing keys because this test exercises only local admission.
        // service 不需要实时签名密钥，因为本测试只执行本地准入判断。
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            Some(String::from("https://example.test/index.json")),
        );
        // providerId selects one real bundled system manifest accepted by the strict compiler.
        // providerId 选择一份可被严格编译器接受的真实内置系统 Manifest。
        let provider_id = "alibaba";
        // activeText is the exact trusted local document whose revision and digest enter state.
        // activeText 是修订与摘要写入状态的精确可信本地文档。
        let active_text = std::fs::read_to_string(
            vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                .join(format!("{provider_id}.json")),
        )
        .expect("bundled provider manifest should read");
        // revision is read from the authoritative document rather than duplicated in the test.
        // revision 从权威文档读取，而不是在测试中重复声明。
        let revision = serde_json::from_str::<serde_json::Value>(&active_text)
            .expect("bundled manifest should parse")["revision"]
            .as_u64()
            .expect("bundled manifest revision should be unsigned");
        // state pins the exact Active bytes and one shared authenticated Index validator.
        // state 固定精确 Active 字节与一个共享认证 Index 验证器。
        let mut state = conditional_index_state(revision, 'a', "\"index-7\"");
        state.sha256 = ManifestStoreLayout::sha256_hex(active_text.as_bytes());
        service
            .store
            .save_sync_state(provider_id, &state)
            .expect("sync state should save");
        let providers = [provider_id.to_string()];
        assert_eq!(
            service.shared_revalidated_index_etag(&providers),
            None,
            "state without its Active document must force one unconditional batch fetch"
        );

        service
            .store
            .stage_candidate(provider_id, revision, active_text.as_bytes())
            .expect("candidate should stage");
        service
            .store
            .activate(provider_id, revision)
            .expect("candidate should activate");
        assert_eq!(
            service.shared_revalidated_index_etag(&providers).as_deref(),
            Some("\"index-7\"")
        );

        // Corrupting the Active bytes after state persistence must disable the 304 optimization.
        // 在状态持久化后破坏 Active 字节必须禁用 304 优化。
        std::fs::write(service.store.active_path(provider_id), b"{}")
            .expect("active corruption should write");
        assert_eq!(service.shared_revalidated_index_etag(&providers), None);
    }

    /// Verifies sync accepts only the exact JSON media type with optional parameters.
    /// 验证同步只接受带可选参数的精确 JSON 媒体类型。
    #[test]
    fn sync_content_type_rejects_json_prefix_lookalikes() {
        assert!(is_supported_sync_content_type("application/json"));
        assert!(is_supported_sync_content_type(
            "Application/JSON; charset=utf-8"
        ));
        assert!(!is_supported_sync_content_type("application/jsonp"));
        assert!(!is_supported_sync_content_type("application/json-seq"));
        assert!(!is_supported_sync_content_type("text/json"));
    }

    /// Verifies sync diagnostics never retain credentials, signed query material, or fragments.
    /// 验证同步诊断绝不保留凭据、签名查询材料或片段。
    #[test]
    fn sync_url_diagnostics_remove_secret_bearing_components() {
        let redacted = redacted_sync_url(
            "https://user:secret@manifests.example.com/v1/index.json?signature=private#fragment",
        );
        assert_eq!(redacted, "https://manifests.example.com/v1/index.json");
        assert!(!redacted.contains("user"));
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("signature"));
        assert_eq!(
            redacted_sync_url("not a URL?signature=private"),
            "<invalid-remote-url>"
        );
    }

    /// Verifies public manifest diagnostics redact credentials and machine-local paths.
    /// 验证公开 Manifest 诊断会脱敏凭据与机器本地路径。
    #[test]
    fn public_manifest_diagnostics_are_secret_safe_and_utf8_bounded() {
        assert_eq!(
            bounded_public_manifest_diagnostic("failed to read C:\\Users\\operator\\manifest.json"),
            "<redacted-local-path>"
        );
        assert_eq!(
            bounded_public_manifest_diagnostic("request token=private-value failed"),
            "<redacted>"
        );
        assert_eq!(
            bounded_public_manifest_diagnostic(
                r#"provider rejected {"authorization":"private-value"}"#
            ),
            "<redacted>"
        );
        assert_eq!(
            bounded_public_manifest_diagnostic("failed to read /home/operator/manifest.json"),
            "<redacted-local-path>"
        );
        assert_eq!(
            bounded_public_manifest_diagnostic("https://user:secret@example.test/index.json"),
            "<redacted-url>"
        );
        let oversized = "清单".repeat(MANIFEST_PREVIEW_DIAGNOSTIC_MAX_BYTES);
        let bounded = bounded_public_manifest_diagnostic(oversized.as_str());
        assert!(bounded.len() <= MANIFEST_PREVIEW_DIAGNOSTIC_MAX_BYTES);
        assert!(bounded.ends_with("..."));
        assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
        // report proves the serialized sync DTO reuses the same public diagnostic boundary.
        // report 证明序列化同步 DTO 复用同一公开诊断边界。
        let report = provider_sync_failure_report(
            "deepseek",
            String::from("failed to read C:\\Users\\operator\\active.json"),
        );
        assert_eq!(
            report.failure_message.as_deref(),
            Some("<redacted-local-path>")
        );
    }

    /// Verifies activation and compensated rollback are structurally mutually exclusive terminals.
    /// 验证激活与已补偿回滚在结构上是相互排斥的终态。
    #[test]
    fn activation_telemetry_terminal_states_are_mutually_exclusive() {
        // activated is the exact event emitted only after every projection commits.
        // activated 是仅在全部投影提交后发出的精确事件。
        let activated = manifest_activation_terminal_event(
            "deepseek",
            7,
            11,
            ManifestActivationTelemetryTerminal::Activated {
                source: "active",
                sha256: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            },
        );
        // rolledBack is the exact event emitted only after every before-image is restored.
        // rolledBack 是仅在全部前镜像恢复后发出的精确事件。
        let rolled_back = manifest_activation_terminal_event(
            "deepseek",
            7,
            12,
            ManifestActivationTelemetryTerminal::RolledBack {
                failure_kind: "workbench_projection_failed",
            },
        );

        activated
            .validate()
            .expect("activation event should validate");
        rolled_back
            .validate()
            .expect("rollback event should validate");
        assert_eq!(
            activated.event_name,
            ProviderOperationalTelemetryEventName::ProviderManifestActivated
        );
        assert_eq!(activated.manifest_source.as_deref(), Some("active"));
        assert!(activated.failure_kind.is_none());
        assert_eq!(
            rolled_back.event_name,
            ProviderOperationalTelemetryEventName::ProviderManifestPostActivationRolledBack
        );
        assert!(rolled_back.manifest_source.is_none());
        assert_eq!(
            rolled_back.failure_kind.as_deref(),
            Some("workbench_projection_failed")
        );
        assert_ne!(
            activated.event_name.as_str(),
            rolled_back.event_name.as_str()
        );
    }

    /// Verifies an early index failure closes with one stable typed failure and no configured URL.
    /// 验证提前 Index 失败以一个稳定类型化失败闭合，且不包含配置 URL。
    #[test]
    fn index_telemetry_guard_closes_early_failure_without_remote_url() {
        // providerId isolates this process-wide history assertion from every production provider.
        // providerId 将本进程级历史断言与所有生产供应商隔离。
        let provider_id = "telemetry-index-guard-contract-test";
        {
            // guard deliberately drops before normal completion to exercise the error boundary.
            // guard 故意在正常完成前释放，用于验证错误边界。
            let _guard = ManifestIndexCheckTelemetryGuard::start(vec![provider_id.to_string()]);
        }
        // events are newest-first and contain completed, failed, then started.
        // events 按新到旧排列，依次包含 completed、failed 与 started。
        let events = provider_operational_telemetry_snapshot(provider_id, 8)
            .expect("telemetry snapshot should read");
        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].event_name,
            ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted
        );
        assert_eq!(
            events[0].failure_kind.as_deref(),
            Some("index_check_failed")
        );
        assert_eq!(
            events[1].event_name,
            ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed
        );
        assert_eq!(
            events[1].failure_kind.as_deref(),
            Some("index_check_failed")
        );
        let serialized = serde_json::to_string(&events).expect("events should serialize");
        assert!(!serialized.contains("://"));
        assert!(!serialized.to_ascii_lowercase().contains("authorization"));
    }

    /// Verifies a later batch abort never reclassifies an already completed provider check.
    /// 验证后续批次中止绝不会重新分类已经完成的供应商检查。
    #[test]
    fn index_telemetry_guard_preserves_completed_provider_terminal_state() {
        // completedProviderId and pendingProviderId isolate both terminal branches in global history.
        // completedProviderId 与 pendingProviderId 在全局历史中隔离两条终态分支。
        let completed_provider_id = "telemetry-completed-provider-contract-test";
        let pending_provider_id = "telemetry-pending-provider-contract-test";
        {
            // guard resolves the shared index, completes the first provider, then simulates abort.
            // guard 解析共享 Index、完成首个供应商，随后模拟批次中止。
            let mut guard = ManifestIndexCheckTelemetryGuard::start(vec![
                completed_provider_id.to_string(),
                pending_provider_id.to_string(),
            ]);
            guard.mark_index_resolved();
            guard.complete_provider(&ProviderSyncReport {
                provider_id: completed_provider_id.to_string(),
                outcome: "up_to_date",
                revision: 1,
                failure_kind: None,
                failure_message: None,
                activation_snapshot: None,
            });
        }
        let completed_events = provider_operational_telemetry_snapshot(completed_provider_id, 8)
            .expect("completed provider telemetry should read");
        assert!(completed_events.iter().all(|event| {
            event.event_name != ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed
        }));
        assert!(completed_events.iter().any(|event| {
            event.event_name == ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted
                && event.failure_kind.is_none()
        }));

        let pending_events = provider_operational_telemetry_snapshot(pending_provider_id, 8)
            .expect("pending provider telemetry should read");
        assert!(pending_events.iter().any(|event| {
            event.event_name == ProviderOperationalTelemetryEventName::ManifestIndexCheckFailed
                && event.failure_kind.as_deref() == Some("provider_check_aborted")
        }));
        assert!(pending_events.iter().any(|event| {
            event.event_name == ProviderOperationalTelemetryEventName::ManifestIndexCheckCompleted
                && event.failure_kind.as_deref() == Some("provider_check_aborted")
        }));
    }

    /// Verifies candidate rejection telemetry retains the compiler-owned field path as data.
    /// 验证候选拒绝遥测把编译器权威字段路径作为结构化数据保留。
    #[test]
    fn candidate_rejection_telemetry_retains_typed_compiler_path() {
        // providerId isolates this process-wide event from every production provider.
        // providerId 将这条进程级事件与所有生产供应商隔离。
        let provider_id = "telemetry-candidate-path-contract-test";
        emit_provider_candidate_event(
            ProviderOperationalTelemetryEventName::ProviderCandidateRejected,
            provider_id,
            19,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Some("schema_semantic_failed"),
            Some("$.model_rules[0].selected_protocol"),
        );
        // event is the exact newest rejection emitted above.
        // event 是上方发出的精确最新拒绝事件。
        let event = provider_operational_telemetry_snapshot(provider_id, 1)
            .expect("candidate telemetry snapshot should read")
            .into_iter()
            .next()
            .expect("candidate rejection event should exist");
        assert_eq!(
            event.event_name,
            ProviderOperationalTelemetryEventName::ProviderCandidateRejected
        );
        assert_eq!(
            event.failure_kind.as_deref(),
            Some("schema_semantic_failed")
        );
        assert_eq!(
            event.failure_path.as_deref(),
            Some("$.model_rules[0].selected_protocol")
        );
    }

    /// Verifies status-facing operational history remains bounded under repeated observations.
    /// 验证面向状态的运行历史在重复观测下保持有界。
    #[test]
    fn status_telemetry_window_is_bounded() {
        // providerId isolates the bounded ring query from other provider tests.
        // providerId 将有界环查询与其他供应商测试隔离。
        let provider_id = "telemetry-status-window-contract-test";
        for _ in 0..(MANIFEST_STATUS_TELEMETRY_MAX_ITEMS + 8) {
            emit_manifest_index_event(
                ProviderOperationalTelemetryEventName::ManifestIndexCheckNotModified,
                provider_id,
                None,
            );
        }
        // events applies the exact status response cap even when more observations exist.
        // events 即使存在更多观测，也应用精确状态响应上限。
        let events = provider_operational_telemetry_snapshot(
            provider_id,
            MANIFEST_STATUS_TELEMETRY_MAX_ITEMS,
        )
        .expect("bounded telemetry snapshot should read");
        assert_eq!(events.len(), MANIFEST_STATUS_TELEMETRY_MAX_ITEMS);
    }

    /// Supplies one immutable API key to signed manifest catalog activation tests.
    /// 为签名清单目录激活测试提供一个不可变 API Key。
    #[derive(Clone)]
    struct SignedE2eSecretStore {
        /// Exact versioned test secret returned for every reference.
        /// 为每个引用返回的精确版本化测试秘密。
        secret: vulcan_provider_adapters::CredentialSecretEnvelope,
        /// One-shot barrier armed immediately before an in-flight generation-freeze assertion.
        /// 在执行中代际冻结断言前立即启用的一次性屏障。
        block_next_read: Arc<std::sync::atomic::AtomicBool>,
        /// Signals that the request has resolved its immutable route and reached secret loading.
        /// 表示请求已解析不可变路由并到达秘密加载阶段。
        read_blocked: Arc<tokio::sync::Notify>,
        /// Releases the one request paused after immutable route resolution.
        /// 释放在不可变路由解析后暂停的唯一请求。
        read_release: Arc<tokio::sync::Notify>,
    }

    impl SignedE2eSecretStore {
        /// Builds one deterministic revision-one API key store.
        /// 构建一个确定性的第一修订版 API Key 存储。
        ///
        /// # Returns
        /// 返回值
        /// A store suitable for static provider discovery without real credentials.
        /// 返回适用于静态供应商发现且不含真实凭据的存储。
        fn new() -> Self {
            Self {
                secret: vulcan_provider_adapters::CredentialSecretEnvelope {
                    version: 1,
                    revision: 1,
                    material: vulcan_provider_adapters::CredentialSecretMaterial::ApiKey {
                        value: String::from("sk-sp-signed-manifest-e2e"),
                    },
                },
                block_next_read: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                read_blocked: Arc::new(tokio::sync::Notify::new()),
                read_release: Arc::new(tokio::sync::Notify::new()),
            }
        }

        /// Arms the next secret read as the deterministic in-flight request boundary.
        /// 将下一次秘密读取设置为确定性的执行中请求边界。
        fn arm_next_read_barrier(&self) {
            self.block_next_read
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }

        /// Waits until the armed request has captured its immutable provider generation.
        /// 等待已启用屏障的请求捕获其不可变供应商代际。
        async fn wait_until_read_blocked(&self) {
            self.read_blocked.notified().await;
        }

        /// Releases the request paused at the secret-read boundary.
        /// 释放暂停在秘密读取边界的请求。
        fn release_blocked_read(&self) {
            self.read_release.notify_one();
        }
    }

    #[async_trait::async_trait]
    impl vulcan_provider_adapters::CredentialSecretResolver for SignedE2eSecretStore {
        /// Returns the immutable fixture secret for every opaque reference.
        /// 为每个不透明引用返回不可变夹具秘密。
        async fn read_optional_secret(
            &self,
            _secret_reference: &str,
        ) -> Result<Option<vulcan_provider_adapters::CredentialSecretEnvelope>, String> {
            if self
                .block_next_read
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                self.read_blocked.notify_one();
                self.read_release.notified().await;
            }
            Ok(Some(self.secret.clone()))
        }
    }

    #[async_trait::async_trait]
    impl vulcan_provider_adapters::CredentialSecretStore for SignedE2eSecretStore {
        /// Rejects mutation because the E2E fixture is immutable.
        /// 拒绝变更，因为 E2E 夹具不可变。
        async fn create_secret(
            &self,
            _secret_reference: &str,
            _secret: &vulcan_provider_adapters::CredentialSecretEnvelope,
        ) -> Result<(), String> {
            Err(String::from("The signed E2E secret store is immutable."))
        }

        /// Rejects replacement because the E2E fixture is immutable.
        /// 拒绝替换，因为 E2E 夹具不可变。
        async fn replace_secret(
            &self,
            _secret_reference: &str,
            _expected_revision: u64,
            _secret: &vulcan_provider_adapters::CredentialSecretEnvelope,
        ) -> Result<(), String> {
            Err(String::from("The signed E2E secret store is immutable."))
        }

        /// Rejects deletion because the E2E fixture is immutable.
        /// 拒绝删除，因为 E2E 夹具不可变。
        async fn delete_secret(&self, _secret_reference: &str) -> Result<(), String> {
            Err(String::from("The signed E2E secret store is immutable."))
        }

        /// Rejects restoration because the E2E fixture is immutable.
        /// 拒绝恢复，因为 E2E 夹具不可变。
        async fn restore_secret(
            &self,
            _secret_reference: &str,
            _secret: &vulcan_provider_adapters::CredentialSecretEnvelope,
        ) -> Result<(), String> {
            Err(String::from("The signed E2E secret store is immutable."))
        }
    }

    /// Builds one Alibaba manifest revision with exact additional static models.
    /// 构建一份包含精确新增静态模型的 Alibaba 清单修订版。
    ///
    /// # Parameters
    /// 参数
    /// `revision` is the signed remote revision, `model_ids` and their bounded data-only rules are
    /// appended to Model Studio, `media_model_ids` bind reviewed media templates, and
    /// `unauthorized_host` injects
    /// one trust-ceiling violation when true.
    /// `revision` 是签名远端修订号，`model_ids` 及其有界纯数据规则被追加到 Model Studio，
    /// `media_model_ids` 绑定已审核媒体模板；`unauthorized_host` 为真时注入一个信任上限违规项。
    ///
    /// # Returns
    /// 返回值
    /// Canonical test JSON bytes ready for digest pinning and remote activation.
    /// 返回可直接进行摘要固定与远端激活的规范测试 JSON 字节。
    fn alibaba_manifest_e2e_bytes(
        revision: u64,
        model_ids: &[&str],
        media_model_ids: &[&str],
        unauthorized_host: bool,
    ) -> Vec<u8> {
        let text = std::fs::read_to_string(
            vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                .join("alibaba.json"),
        )
        .expect("bundled Alibaba manifest must exist");
        let mut document =
            serde_json::from_str::<serde_json::Value>(&text).expect("manifest should parse");
        document["revision"] = serde_json::Value::from(revision);
        let services = document["services"]
            .as_array_mut()
            .expect("Alibaba services must be an array");
        let model_studio = services
            .iter_mut()
            .find(|service| service["service_entry_id"] == "model-studio-cn")
            .expect("Alibaba Model Studio entry must exist");
        let models = model_studio["model_catalog"]["models"]
            .as_array_mut()
            .expect("Alibaba static model catalog must be an array");
        for model_id in model_ids {
            models.push(serde_json::json!({
                "model_id": model_id,
                "display_name": model_id
            }));
        }
        if unauthorized_host {
            model_studio["base_url"] =
                serde_json::Value::String(String::from("https://unauthorized.example.test"));
        }
        // base_rule is reviewed executable authority; each remote rule changes identity and exact
        // model matching data only.
        // base_rule 是已审核可执行权限；每条远端规则只修改身份与精确模型匹配数据。
        let base_rule = document["model_rules"]
            .as_array()
            .expect("Alibaba model rules must be an array")
            .iter()
            .find(|rule| {
                rule["matcher"]["value"] == "qwen3.7-plus"
                    && rule["scope"]["service_entry_ids"]
                        .as_array()
                        .is_some_and(|services| {
                            services.iter().any(|service| service == "model-studio-cn")
                        })
            })
            .cloned()
            .expect("Alibaba reviewed Qwen conversation rule must exist");
        // rules receives one exact bounded rule for every newly published conversation model.
        // rules 为每个新发布对话模型接收一条精确有界规则。
        let rules = document["model_rules"]
            .as_array_mut()
            .expect("Alibaba model rules must be mutable");
        for model_id in model_ids {
            let mut rule = base_rule.clone();
            rule["rule_id"] = serde_json::Value::String(format!("signed-e2e-{model_id}"));
            rule["matcher"]["value"] = serde_json::Value::String((*model_id).to_string());
            rules.push(rule);
        }
        // bindings is the independent media/tool model catalog and intentionally does not
        // pollute the conversation model catalog.
        // bindings 是独立的媒体/工具模型目录，刻意不污染对话模型目录。
        let bindings = document["tool_target_template_bindings"]
            .as_array_mut()
            .expect("Alibaba template bindings must be an array");
        for model_id in media_model_ids {
            bindings.push(serde_json::json!({
                "service_entry_id": "model-studio-cn",
                "model_id": model_id,
                "display_name": format!("{model_id} Generate"),
                "template_refs": ["alibaba.model-studio-cn.qwen-image-generate.v1"]
            }));
        }
        let bytes = serde_json::to_vec(&document).expect("manifest should serialize");
        let text = std::str::from_utf8(&bytes).expect("manifest should remain UTF-8");
        vulcan_provider_adapters::compile_manifest_text_with_layer(
            text,
            vulcan_provider_adapters::ManifestOwnership::System,
            vulcan_provider_adapters::ManifestSourceLayer::Active,
        )
        .expect("generated signed E2E manifest must compile");
        bytes
    }

    /// Starts one two-request signed HTTPS endpoint and returns its configured sync service.
    /// 启动一个处理两次请求的签名 HTTPS 端点，并返回已配置的同步服务。
    ///
    /// # Parameters
    /// 参数
    /// `app_data_root` owns durable sync state, `provider_id` and `revision` identify the pin,
    /// `provider_bytes` are the exact downloaded bytes, and `index_revision` distinguishes runs.
    /// `app_data_root` 持有持久同步状态，`provider_id` 与 `revision` 标识固定版本，
    /// `provider_bytes` 是精确下载字节，`index_revision` 区分各次运行。
    ///
    /// # Returns
    /// 返回值
    /// The configured service plus the server task that completes after Index and provider reads.
    /// 返回已配置服务，以及在 Index 与供应商读取后结束的服务器任务。
    async fn signed_https_sync_service(
        app_data_root: &Path,
        provider_id: &str,
        revision: u64,
        provider_bytes: Vec<u8>,
        index_revision: u64,
    ) -> (ProviderManifestSyncService, tokio::task::JoinHandle<()>) {
        use sha2::Digest;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind signed E2E loopback");
        let index_url = format!(
            "https://{}/index.json",
            listener.local_addr().expect("loopback address")
        );
        let digest = sha2::Sha256::digest(&provider_bytes);
        let digest_hex = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut seed = [0_u8; 32];
        seed[..10].copy_from_slice(b"signed-e2e");
        let key_id = String::from("signed-e2e-key");
        let now = current_unix_millis() as i64;
        let index = vulcan_provider_config::manifest_sync::ManifestRemoteIndex {
            schema_version: vulcan_provider_config::manifest_sync::INDEX_SCHEMA_VERSION.to_string(),
            index_revision,
            issued_at_unix_ms: now - 1_000,
            expires_at_unix_ms: now + 60_000,
            signing_key_id: key_id.clone(),
            providers: std::collections::BTreeMap::from([(
                provider_id.to_string(),
                vulcan_provider_config::manifest_sync::RemoteProviderEntry {
                    provider_id: provider_id.to_string(),
                    revision,
                    relative_url: format!("/{provider_id}.json"),
                    sha256: digest_hex,
                    bytes: provider_bytes.len() as u64,
                    minimum_engine_contract: 1,
                },
            )]),
        };
        let index_bytes = serde_json::to_vec(&index).expect("index should encode");
        let signature =
            vulcan_provider_config::manifest_sync::sign_index_document(&index_bytes, &seed)
                .expect("index should sign");
        let signature_hex = signature
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();

        let mut cert_params =
            rcgen::CertificateParams::new(Vec::<String>::new()).expect("certificate params");
        cert_params.subject_alt_names = vec![rcgen::SanType::IpAddress(std::net::IpAddr::V4(
            std::net::Ipv4Addr::LOCALHOST,
        ))];
        let key_pair = rcgen::KeyPair::generate().expect("certificate key");
        let generated = cert_params.self_signed(&key_pair).expect("certificate");
        let tls_config = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![generated.der().clone()],
                tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der())
                    .expect("private key der"),
            )
            .expect("TLS config");
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));
        let provider_path = format!("/{provider_id}.json");
        let server = tokio::spawn(async move {
            for _request_index in 0..2 {
                let (stream, _peer) = listener.accept().await.expect("accept signed request");
                let mut stream = acceptor.accept(stream).await.expect("TLS accept");
                let mut head = Vec::new();
                let mut byte = [0_u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    let read = stream.read(&mut byte).await.expect("request read");
                    if read == 0 {
                        break;
                    }
                    head.push(byte[0]);
                }
                let head_text = String::from_utf8_lossy(&head);
                // Every index and provider request must advertise the same executable client
                // boundary so remote delivery cannot depend on an implicit release guess.
                // 每次 Index 与供应商请求都必须声明同一可执行客户端边界，远端分发不能依赖隐式版本猜测。
                let lower_head = head_text.to_ascii_lowercase();
                assert!(lower_head.contains(&format!(
                    "{SYNC_CLIENT_VERSION_HEADER}: {}",
                    env!("CARGO_PKG_VERSION")
                )));
                assert!(lower_head.contains(&format!(
                    "{SYNC_MANIFEST_SCHEMA_HEADER}: {}",
                    vulcan_provider_adapters::provider_manifest::MANIFEST_SCHEMA_VERSION
                )));
                assert!(lower_head.contains(&format!(
                    "{SYNC_MANIFEST_ENGINE_HEADER}: {}",
                    vulcan_provider_adapters::provider_manifest::CURRENT_MANIFEST_ENGINE
                )));
                let path = head_text.split_whitespace().nth(1).unwrap_or("/");
                let (headers, body) = if path == "/index.json" {
                    (
                        format!(
                            "Content-Type: application/json\r\nx-vulcan-manifest-signature: {signature_hex}\r\nETag: \"index-{index_revision}\"\r\n"
                        ),
                        index_bytes.as_slice(),
                    )
                } else if path == provider_path {
                    (
                        format!(
                            "Content-Type: application/json\r\nETag: \"provider-{revision}\"\r\n"
                        ),
                        provider_bytes.as_slice(),
                    )
                } else {
                    (String::new(), &[][..])
                };
                let status = if path == "/index.json" || path == provider_path {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .expect("response head write");
                stream.write_all(body).await.expect("response body write");
                let _ = stream.shutdown().await;
            }
        });
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("signed E2E client");
        let trusted = TrustedSigningKey {
            key_id,
            public_key: vulcan_provider_config::manifest_sync::public_key_for_seed(&seed),
        };
        (
            ProviderManifestSyncService::new(app_data_root, client, vec![trusted], Some(index_url)),
            server,
        )
    }

    /// Starts a reviewed-host HTTPS loopback for two generated-image requests.
    /// 为两次文生图请求启动使用已审核主机名的 HTTPS 回环服务。
    ///
    /// The injected client resolves only `dashscope.aliyuncs.com` to the ephemeral listener and
    /// trusts only this fixture certificate. The production manifest endpoint remains unchanged,
    /// so the test exercises the exact authority admitted by the bundled trust ceiling.
    /// 注入客户端只把 `dashscope.aliyuncs.com` 解析到临时监听器，并且只信任当前夹具证书。
    /// 生产 Manifest 端点保持不变，因此测试执行的是内置信任上限准入的精确权限。
    ///
    /// # Returns
    /// 返回值
    /// The bounded HTTP client, a receiver for exact request paths/bodies, and the server task.
    /// 返回有界 HTTP 客户端、精确请求路径/正文接收器与服务器任务。
    async fn provider_tool_https_fixture() -> (
        reqwest::Client,
        tokio::sync::mpsc::UnboundedReceiver<(String, serde_json::Value)>,
        tokio::task::JoinHandle<()>,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
        // listener owns the only socket to which the reviewed production hostname is resolved.
        // listener 持有已审核生产主机名所解析到的唯一套接字。
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind provider-tool E2E loopback");
        // listener_address freezes the ephemeral port before the server task takes ownership.
        // listener_address 在服务器任务取得所有权前冻结临时端口。
        let listener_address = listener
            .local_addr()
            .expect("provider-tool loopback address");
        // certificate_parameters binds the certificate to the exact reviewed manifest hostname.
        // certificate_parameters 把证书绑定到 Manifest 已审核的精确主机名。
        let certificate_parameters =
            rcgen::CertificateParams::new(vec![String::from("dashscope.aliyuncs.com")])
                .expect("provider-tool certificate params");
        // certificate_key is unique fixture key material and never leaves this test process.
        // certificate_key 是唯一夹具密钥材料，绝不离开当前测试进程。
        let certificate_key = rcgen::KeyPair::generate().expect("provider-tool certificate key");
        // generated_certificate supplies both the rustls chain and the isolated client trust root.
        // generated_certificate 同时提供 rustls 证书链与隔离的客户端信任根。
        let generated_certificate = certificate_parameters
            .self_signed(&certificate_key)
            .expect("provider-tool certificate");
        // client_certificate is the sole additional trust anchor for the injected client.
        // client_certificate 是注入客户端唯一额外信任锚。
        let client_certificate =
            reqwest::Certificate::from_der(generated_certificate.der().as_ref())
                .expect("provider-tool client certificate");
        // tls_configuration rejects client certificates and serves only the generated chain.
        // tls_configuration 拒绝客户端证书且只提供生成的证书链。
        let tls_configuration = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![generated_certificate.der().clone()],
                tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(
                    certificate_key.serialize_der(),
                )
                .expect("provider-tool private key DER"),
            )
            .expect("provider-tool TLS config");
        // tls_acceptor is shared across the two deterministic request connections.
        // tls_acceptor 在两次确定性请求连接之间共享。
        let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_configuration));
        // request channel preserves exact request order for old/new generation assertions.
        // 请求通道为旧/新代际断言保留精确请求顺序。
        let (request_sender, request_receiver) = tokio::sync::mpsc::unbounded_channel();
        // server terminates after exactly two provider-tool requests.
        // server 在恰好两次供应商工具请求后终止。
        let server = tokio::spawn(async move {
            for _request_index in 0..2 {
                // accepted_stream is one reviewed-host TLS transport.
                // accepted_stream 是一条已审核主机 TLS 传输。
                let (accepted_stream, _peer) = listener
                    .accept()
                    .await
                    .expect("accept provider-tool request");
                // tls_stream owns the fully authenticated request/response exchange.
                // tls_stream 持有完成认证的请求/响应交换。
                let mut tls_stream = tls_acceptor
                    .accept(accepted_stream)
                    .await
                    .expect("accept provider-tool TLS");
                // request_head accumulates bytes only through the HTTP header terminator.
                // request_head 只累积到 HTTP 头终止符为止。
                let mut request_head = Vec::new();
                // next_byte is the exact one-byte framing buffer used by this bounded fixture.
                // next_byte 是该有界夹具使用的精确单字节分帧缓冲区。
                let mut next_byte = [0_u8; 1];
                while !request_head.ends_with(b"\r\n\r\n") {
                    // read_count distinguishes a closed connection from one received byte.
                    // read_count 区分已关闭连接与收到的一个字节。
                    let read_count = tls_stream
                        .read(&mut next_byte)
                        .await
                        .expect("read provider-tool request head");
                    if read_count == 0 {
                        break;
                    }
                    request_head.push(next_byte[0]);
                }
                // request_head_text is used only for bounded request-line and content-length parsing.
                // request_head_text 仅用于有界解析请求行与 Content-Length。
                let request_head_text = String::from_utf8(request_head)
                    .expect("provider-tool request head must be UTF-8");
                // request_path captures the exact endpoint path selected from the frozen target.
                // request_path 捕获从冻结目标选择的精确端点路径。
                let request_path = request_head_text
                    .split_whitespace()
                    .nth(1)
                    .expect("provider-tool request path")
                    .to_string();
                // content_length is the exact JSON body length declared by reqwest.
                // content_length 是 reqwest 声明的精确 JSON 正文长度。
                let content_length = request_head_text
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .expect("provider-tool content length");
                // request_body_bytes receives exactly the declared request body and no next frame.
                // request_body_bytes 恰好接收声明的请求正文且不会读取下一帧。
                let mut request_body_bytes = vec![0_u8; content_length];
                tls_stream
                    .read_exact(&mut request_body_bytes)
                    .await
                    .expect("read provider-tool request body");
                // request_body is the projected provider-native JSON body under test.
                // request_body 是被测的供应商原生投影 JSON 正文。
                let request_body = serde_json::from_slice::<serde_json::Value>(&request_body_bytes)
                    .expect("provider-tool request body must be JSON");
                // response_model echoes the exact generation-owned upstream model into the result URL.
                // response_model 把代际持有的精确上游模型回显到结果 URL。
                let response_model = request_body["model"]
                    .as_str()
                    .expect("provider-tool request model")
                    .to_string();
                request_sender
                    .send((request_path, request_body))
                    .expect("provider-tool request receiver must remain open");
                // response_body is a valid Alibaba image envelope consumed by the reviewed profile.
                // response_body 是由已审核 Profile 消费的有效阿里云图片响应封装。
                let response_body = serde_json::to_vec(&serde_json::json!({
                    "request_id": format!("request-{response_model}"),
                    "output": {
                        "choices": [{
                            "message": {
                                "content": [{
                                    "image": format!("https://dashscope.aliyuncs.com/{response_model}.png")
                                }]
                            }
                        }]
                    }
                }))
                .expect("provider-tool response should encode");
                // response_head closes the connection after one complete JSON response.
                // response_head 在一个完整 JSON 响应后关闭连接。
                let response_head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response_body.len()
                );
                tls_stream
                    .write_all(response_head.as_bytes())
                    .await
                    .expect("write provider-tool response head");
                tls_stream
                    .write_all(&response_body)
                    .await
                    .expect("write provider-tool response body");
                let _ = tls_stream.shutdown().await;
            }
        });
        // client disables ambient proxies and resolves only the reviewed host to this fixture.
        // client 禁用环境代理，并且只把已审核主机解析到当前夹具。
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .add_root_certificate(client_certificate)
            .resolve("dashscope.aliyuncs.com", listener_address)
            .build()
            .expect("provider-tool E2E client");
        (client, request_receiver, server)
    }

    /// Returns the revision carried by the currently bundled DeepSeek manifest.
    /// 返回当前随包 DeepSeek 清单携带的修订号。
    fn bundled_deepseek_revision() -> u64 {
        let text = std::fs::read_to_string(
            vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                .join("deepseek.json"),
        )
        .expect("bundled deepseek manifest must exist");
        serde_json::from_str::<serde_json::Value>(&text).expect("manifest should parse")["revision"]
            .as_u64()
            .expect("bundled revision must be an unsigned integer")
    }

    /// Loads the bundled DeepSeek document with one exact test revision.
    /// 加载带有精确测试修订号的随包 DeepSeek 文档。
    fn deepseek_manifest_with_revision(revision: u64) -> String {
        let text = std::fs::read_to_string(
            vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                .join("deepseek.json"),
        )
        .expect("bundled deepseek manifest must exist");
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("manifest should parse");
        value["revision"] = serde_json::json!(revision);
        serde_json::to_string(&value).expect("manifest should serialize")
    }

    /// Loads one DeepSeek revision and appends an exact model rule for status-delta tests.
    /// 加载一个 DeepSeek 修订版，并为状态差异测试追加一条精确模型规则。
    ///
    /// # Parameters
    /// 参数
    /// `revision` is the candidate Manifest revision and `model_id` is the exact new identity.
    /// `revision` 是候选 Manifest 修订号；`model_id` 是精确的新身份。
    ///
    /// # Returns
    /// 返回值
    /// Valid system-manifest JSON containing the additional exact model rule.
    /// 返回包含新增精确模型规则的有效系统 Manifest JSON。
    fn deepseek_manifest_with_exact_model(revision: u64, model_id: &str) -> String {
        // value owns the revision-adjusted bundled document.
        // value 持有已调整修订号的随包文档。
        let mut value = serde_json::from_str::<serde_json::Value>(
            deepseek_manifest_with_revision(revision).as_str(),
        )
        .expect("revision-adjusted DeepSeek manifest should parse");
        // rules is the exact mutable model-rule inventory.
        // rules 是精确的可变模型规则清单。
        let rules = value["model_rules"]
            .as_array_mut()
            .expect("DeepSeek model rules must be an array");
        // rule clones a complete exact-rule contract and changes only its stable identity.
        // rule 克隆一份完整精确规则契约，并只改变其稳定身份。
        let mut rule = rules
            .iter()
            .find(|rule| rule["matcher"]["kind"] == "exact")
            .cloned()
            .expect("DeepSeek must include an exact model rule");
        rule["rule_id"] = serde_json::Value::String(format!("status-{model_id}"));
        rule["display_name"] = serde_json::Value::String(String::from("Status Delta Model"));
        rule["matcher"]["value"] = serde_json::Value::String(model_id.to_string());
        rules.push(rule);
        serde_json::to_string(&value).expect("status-delta manifest should serialize")
    }

    /// Activates one DeepSeek test revision through the authenticated sync transaction.
    /// 通过可信同步事务激活一个 DeepSeek 测试修订。
    fn activate_trusted_deepseek(store: &ManifestStoreLayout, revision: u64, text: &str) {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(text.as_bytes());
        let entry = vulcan_provider_config::manifest_sync::RemoteProviderEntry {
            provider_id: String::from("deepseek"),
            revision,
            relative_url: String::from("/deepseek.json"),
            sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
            bytes: text.len() as u64,
            minimum_engine_contract: 1,
        };
        let outcome = activate_synced_provider(store, &entry, text.as_bytes(), None, None, None)
            .expect("trusted test manifest should activate");
        assert!(
            matches!(
                outcome,
                vulcan_provider_config::manifest_sync::SyncOutcome::Activated { .. }
            ),
            "test activation must publish a new revision"
        );
    }

    /// Builds one isolated store with a staged and activated provider manifest.
    /// 构建一个隔离存储并 staging + 激活一份供应商清单。
    fn store_with_activated_deepseek() -> (ManifestStoreLayout, tempfile::TempDir, u64, String) {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        let revision = bundled_deepseek_revision() + 1;
        let text = deepseek_manifest_with_revision(revision);
        activate_trusted_deepseek(&store, revision, &text);
        (store, directory, revision, text)
    }

    /// Verifies automatic synchronization starts only after both external inputs exist.
    /// 验证自动同步仅在两个外部输入同时存在后启动。
    #[test]
    fn periodic_sync_configuration_gate_requires_url_and_key() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let unconfigured = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        assert!(!unconfigured.is_configured());
        let configured = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            vec![TrustedSigningKey {
                key_id: String::from("test-key"),
                public_key: [7_u8; 32],
            }],
            Some(String::from("https://updates.example/index.json")),
        );
        assert!(configured.is_configured());
    }

    /// Verifies an untrusted RPC identity cannot allocate a retained provider flight.
    /// 验证不可信 RPC 身份无法分配被保留的供应商 Flight。
    #[tokio::test]
    async fn single_provider_sync_rejects_unknown_identity_before_flight_allocation() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");

        let error = service
            .sync_one(&runtime, "unknown-system-provider")
            .await
            .expect_err("unknown provider identity must fail before sync configuration checks");

        assert!(error.contains("trusted system provider identifier"));
        assert!(service.provider_flights.lock().await.is_empty());
    }

    /// Verifies success cadence and exponential retry remain inside their jitter bounds.
    /// 验证成功周期与指数重试始终位于各自抖动边界内。
    #[test]
    fn periodic_sync_delay_is_bounded_and_capped() {
        let success_low = periodic_sync_delay(0, 0).as_secs();
        let success_high = periodic_sync_delay(0, 4_320).as_secs();
        assert_eq!(success_low, 19_440);
        assert_eq!(success_high, 23_760);

        let first_failure_low = periodic_sync_delay(1, 0).as_secs();
        let first_failure_high = periodic_sync_delay(1, 12).as_secs();
        assert_eq!(first_failure_low, 54);
        assert_eq!(first_failure_high, 66);

        let capped_low = periodic_sync_delay(20, 0).as_secs();
        let capped_high = periodic_sync_delay(20, 720).as_secs();
        assert_eq!(capped_low, 3_240);
        assert_eq!(capped_high, 3_960);
    }

    /// Verifies shutdown cancellation aborts the exact registered periodic task.
    /// 验证关闭取消会中止精确注册的周期任务。
    #[tokio::test]
    async fn periodic_sync_task_is_cancellable() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        let handle = tokio::spawn(std::future::pending::<()>());
        service
            .install_periodic_task(handle.abort_handle())
            .expect("periodic task should register");
        service
            .stop_periodic_task()
            .expect("periodic task should stop");
        let join_error = handle
            .await
            .expect_err("registered periodic task must be cancelled");
        assert!(join_error.is_cancelled());
    }

    /// Verifies the activation guard spans the complete cross-layer projection transaction.
    /// 验证激活守卫覆盖完整的跨层投影事务。
    #[tokio::test]
    async fn activation_transaction_guard_is_single_writer() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = std::sync::Arc::new(ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        ));
        let first_guard = service.begin_activation_transaction().await;
        let (acquired_tx, acquired_rx) = tokio::sync::oneshot::channel();
        let service_clone = std::sync::Arc::clone(&service);
        let waiter = tokio::spawn(async move {
            let _second_guard = service_clone.begin_activation_transaction().await;
            let _ = acquired_tx.send(());
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), acquired_rx)
                .await
                .is_err(),
            "a second activation must remain blocked through the first projection window"
        );
        drop(first_guard);
        tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
            .await
            .expect("the waiting activation should resume")
            .expect("the waiting activation task should not panic");
    }

    /// Verifies the startup recovery chain restores a persisted active layer.
    /// 验证启动恢复链恢复持久化 active 层。
    #[tokio::test]
    async fn startup_restores_persisted_active_layer() {
        let (_store, directory, expected_revision, text) = store_with_activated_deepseek();
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let reports = restore_system_manifests_from_store(&runtime, directory.path())
            .expect("restore should succeed");
        runtime
            .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
            .await
            .expect("startup projection must publish the restored generation");
        let report = reports
            .iter()
            .find(|report| report.provider_id == "deepseek")
            .expect("deepseek must be restored");
        assert_eq!(report.layer, "active");
        let revision = serde_json::from_str::<serde_json::Value>(&text)
            .expect("manifest should parse")
            .get("revision")
            .and_then(|value| value.as_u64())
            .expect("revision must exist");
        assert_eq!(report.revision, revision);
        assert_eq!(revision, expected_revision);
        // The runtime catalog now carries the restored revision.
        // 运行时目录现在携带恢复的修订号。
        let runtime_revisions = runtime.system_manifest_revisions().expect("revisions");
        assert!(
            runtime_revisions
                .iter()
                .any(|(id, rev)| id == "deepseek" && *rev == revision),
            "runtime must observe the restored manifest"
        );
    }

    /// Verifies startup consumes an Applying journal before trusting Active or sync state.
    /// 验证启动会在信任 Active 或同步状态前消费 Applying 日志。
    #[tokio::test]
    async fn startup_recovers_uncommitted_activation_before_layer_selection() {
        use sha2::Digest;

        let (store, directory, previous_revision, previous_text) = store_with_activated_deepseek();
        let candidate_revision = previous_revision + 1;
        let candidate_text = deepseek_manifest_with_revision(candidate_revision);
        // digest pins the exact uncommitted candidate bytes.
        // digest 固定精确的未提交候选字节。
        let digest = sha2::Sha256::digest(candidate_text.as_bytes());
        let entry = vulcan_provider_config::manifest_sync::RemoteProviderEntry {
            provider_id: String::from("deepseek"),
            revision: candidate_revision,
            relative_url: String::from("/deepseek.json"),
            sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
            bytes: candidate_text.len() as u64,
            minimum_engine_contract: 1,
        };
        let outcome = stage_synced_provider_activation(
            &store,
            &entry,
            candidate_text.as_bytes(),
            None,
            None,
            None,
        )
        .expect("uncommitted activation should stage");
        assert!(matches!(
            outcome,
            vulcan_provider_config::manifest_sync::SyncOutcome::Activated { .. }
        ));
        assert_eq!(
            store.read_active("deepseek").expect("active should read"),
            Some(candidate_text),
            "candidate should be visible only until recovery consumes the Applying journal"
        );

        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let reports = restore_system_manifests_from_store(&runtime, directory.path())
            .expect("startup recovery should succeed");
        runtime
            .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
            .await
            .expect("recovered startup generation should publish");
        let report = reports
            .iter()
            .find(|report| report.provider_id == "deepseek")
            .expect("recovered Active layer should report");
        assert_eq!(report.layer, "active");
        assert_eq!(report.revision, previous_revision);
        assert_eq!(
            store
                .read_active("deepseek")
                .expect("recovered Active should read"),
            Some(previous_text)
        );
    }

    /// Verifies the production single-provider path leaves its file transaction recoverable.
    /// 验证生产单供应商路径会让文件事务保持可恢复状态。
    #[tokio::test]
    async fn production_single_sync_stays_applying_until_projection_finalization() {
        // directory owns the staged provider files and transaction journal.
        // directory 持有已暂存供应商文件与事务日志。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // runtime owns the staged manifest catalog before full projection publication.
        // runtime 持有完整投影发布前的暂存 Manifest 目录。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // revision is strictly newer than the immutable bundled baseline.
        // revision 严格晚于不可变内置基线。
        let revision = bundled_deepseek_revision() + 1;
        // candidate_bytes are the exact signed provider document served by the local fixture.
        // candidate_bytes 是本地夹具提供的精确签名供应商文档。
        let candidate_bytes = deepseek_manifest_with_revision(revision).into_bytes();
        // service and server provide the reviewed signed HTTPS synchronization boundary.
        // service 与 server 提供已审核的签名 HTTPS 同步边界。
        let (service, server) =
            signed_https_sync_service(directory.path(), "deepseek", revision, candidate_bytes, 1)
                .await;

        // report must retain the exact before-image needed by the projection finalizer.
        // report 必须保留投影完成器所需的精确前镜像。
        let report = service
            .sync_one(&runtime, "deepseek")
            .await
            .expect("production single sync should stage");
        assert_eq!(report.outcome, "activated");
        assert!(report.activation_snapshot.is_some());

        // store emulates process-start recovery before any projection finalizer commits the journal.
        // store 模拟任何投影完成器提交日志前的进程启动恢复。
        let store = ManifestStoreLayout::new(directory.path());
        assert!(
            store
                .recover_system_transaction("deepseek")
                .expect("Applying journal should recover"),
            "the production path must retain an Applying journal"
        );
        assert_eq!(
            store.read_active("deepseek").expect("active should read"),
            None,
            "recovery must restore the exact absent Active before-image"
        );

        server.await.expect("signed server should finish");
    }

    /// Verifies the recovery chain quarantines an invalid active layer and restores previous.
    /// 验证恢复链隔离无效 active 层并恢复 previous 层。
    #[tokio::test]
    async fn startup_restores_previous_after_invalid_active() {
        let (_store, directory, previous_revision, previous_text) = store_with_activated_deepseek();
        let store = ManifestStoreLayout::new(directory.path());
        let active_revision = previous_revision + 1;
        // Rotate the first authenticated revision into Previous by activating its successor.
        // 通过激活后一修订，把首个已认证修订轮换到 Previous。
        let second = deepseek_manifest_with_revision(active_revision);
        activate_trusted_deepseek(&store, active_revision, &second);
        // Corrupt the active layer.
        // 破坏 active 层。
        std::fs::write(store.active_path("deepseek"), br#"{"broken":true}"#)
            .expect("corrupt active should write");
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let reports = restore_system_manifests_from_store(&runtime, directory.path())
            .expect("restore should succeed");
        let report = reports
            .iter()
            .find(|report| report.provider_id == "deepseek")
            .expect("deepseek must be restored");
        assert_eq!(report.layer, "previous");
        assert_eq!(report.revision, previous_revision);
        assert!(
            store
                .quarantine_path("deepseek", active_revision, "active_invalid")
                .exists(),
            "invalid active bytes must be quarantined"
        );
        runtime
            .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
            .await
            .expect("startup projection must publish the Previous generation");
        emit_published_startup_manifest_restore_events(&runtime, &reports)
            .expect("published Previous restore telemetry should emit");
        let expected_previous_sha256 = ManifestStoreLayout::sha256_hex(previous_text.as_bytes());
        let fallback_event = provider_operational_telemetry_snapshot("deepseek", usize::MAX)
            .expect("fallback telemetry should remain readable")
            .into_iter()
            .find(|event| {
                event.event_name
                    == ProviderOperationalTelemetryEventName::ProviderManifestFallbackSelected
                    && event.manifest_revision == Some(previous_revision)
                    && event.failure_kind.as_deref() == Some("active_invalid")
            })
            .expect("previous recovery must emit one exact fallback event");
        assert_eq!(fallback_event.manifest_source.as_deref(), Some("previous"));
        assert_eq!(
            fallback_event.manifest_generation,
            Some(runtime.catalog_generation())
        );
        assert_eq!(
            fallback_event.manifest_sha256.as_deref(),
            Some(expected_previous_sha256.as_str()),
            "fallback telemetry must use the verified Previous bytes rather than the old published generation"
        );
    }

    /// Verifies corrupt generated sync state falls back per provider instead of aborting startup.
    /// 验证损坏的生成同步状态按供应商回退，而不是中止启动。
    #[test]
    fn startup_isolates_invalid_sync_state_bookkeeping() {
        // directory isolates the generated state corruption from the user's application data.
        // directory 把生成状态损坏与用户应用数据隔离。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // store owns the exact encoded state path used by production recovery.
        // store 持有生产恢复使用的精确编码状态路径。
        let store = ManifestStoreLayout::new(directory.path());
        // statePath is created under its fixed generated-state directory.
        // statePath 创建在其固定生成状态目录下。
        let state_path = store.sync_state_path("deepseek");
        std::fs::create_dir_all(state_path.parent().expect("state parent must exist"))
            .expect("state directory should create");
        std::fs::write(&state_path, br#"{"unknown":true}"#)
            .expect("invalid state fixture should write");
        // runtime starts from the independent trusted bundled baseline.
        // runtime 从独立可信内置基线启动。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let bundled_revision = runtime
            .system_manifest("deepseek")
            .expect("bundled DeepSeek must exist")
            .revision;

        let reports = restore_system_manifests_from_store(&runtime, directory.path())
            .expect("one corrupt provider state must not abort startup");
        assert!(
            reports
                .iter()
                .all(|report| report.provider_id != "deepseek"),
            "invalid state cannot authenticate a persisted restore"
        );
        assert!(
            !state_path.exists(),
            "invalid generated state must be removed"
        );
        assert_eq!(
            runtime
                .system_manifest("deepseek")
                .expect("bundled fallback must remain executable")
                .revision,
            bundled_revision
        );
    }

    /// Verifies unauthenticated Active bytes are removed and never restored during startup.
    /// 验证未认证的 Active 字节会在启动时移除且绝不会被恢复。
    #[test]
    fn startup_removes_active_layer_without_authenticated_state() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        let active_path = store.active_path("deepseek");
        std::fs::create_dir_all(active_path.parent().expect("active parent must exist"))
            .expect("active directory should create");
        let unauthenticated_revision = bundled_deepseek_revision() + 1;
        std::fs::write(
            &active_path,
            deepseek_manifest_with_revision(unauthenticated_revision),
        )
        .expect("orphan active fixture should write");
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let bundled_revision = runtime
            .system_manifest("deepseek")
            .expect("bundled DeepSeek must exist")
            .revision;

        let reports = restore_system_manifests_from_store(&runtime, directory.path())
            .expect("orphan provider bytes must not abort startup");

        assert!(!active_path.exists());
        assert!(
            reports
                .iter()
                .all(|report| report.provider_id != "deepseek")
        );
        assert_eq!(
            runtime
                .system_manifest("deepseek")
                .expect("bundled fallback must remain executable")
                .revision,
            bundled_revision
        );
    }

    /// Verifies one corrupt provider status file cannot suppress every other provider status.
    /// 验证一个损坏的供应商状态文件不能压制其他全部供应商状态。
    #[test]
    fn aggregate_status_contains_provider_local_projection_failure() {
        // directory contains one deliberately invalid independent check-state document.
        // directory 包含一份刻意无效的独立检查状态文档。
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        let check_state_path = service.store.sync_check_state_path("deepseek");
        std::fs::create_dir_all(
            check_state_path
                .parent()
                .expect("check-state parent must exist"),
        )
        .expect("check-state directory should create");
        std::fs::write(&check_state_path, br#"{"unknown":true}"#)
            .expect("invalid check state fixture should write");
        // runtime retains the executable bundled layer while detailed status projection fails.
        // runtime 在详细状态投影失败时仍保留可执行内置层。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");

        let statuses = service
            .provider_manifest_status_all(&runtime)
            .expect("aggregate status must remain available");
        let deepseek = statuses
            .iter()
            .find(|status| status.provider_id == "deepseek")
            .expect("DeepSeek status must remain present");
        assert_eq!(
            deepseek
                .status_error
                .as_ref()
                .map(|error| error.failure_kind.as_str()),
            Some("status_projection_failed")
        );
        assert!(deepseek.active.is_some());
        assert!(deepseek.critical_error.is_none());
        assert!(
            statuses
                .iter()
                .any(|status| status.provider_id == "openrouter" && status.status_error.is_none()),
            "an unrelated healthy provider must retain complete status"
        );
    }

    /// Verifies an unconfigured sync service reports `not_configured` without any network.
    /// 验证未配置的同步服务报告 `not_configured` 且不发起任何网络请求。
    #[test]
    fn unconfigured_sync_reports_not_configured() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let http = build_sync_http_client().expect("sync client");
        let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let runtime = std::sync::Arc::new(runtime);
        let report = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("async runtime")
            .block_on(service.sync_provider(&runtime, "deepseek"))
            .expect("not configured must not fail");
        assert_eq!(report.outcome, "not_configured");
    }

    /// Verifies a non-HTTPS index URL is rejected before any request.
    /// 验证非 HTTPS Index URL 在任何请求前被拒绝。
    #[test]
    fn sync_rejects_insecure_index_url() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            Some(String::from("http://insecure.example.com/index.json")),
        );
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let runtime = std::sync::Arc::new(runtime);
        let error = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("async runtime")
            .block_on(service.sync_provider(&runtime, "deepseek"))
            .expect_err("insecure index URL must fail");
        assert!(error.contains("HTTPS"), "{error}");
    }

    /// Verifies concurrent unconfigured requests return safely without network or state writes.
    /// 验证并发的未配置请求能在不触网、不写状态的前提下安全返回。
    #[tokio::test]
    async fn concurrent_unconfigured_syncs_are_side_effect_free() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = std::sync::Arc::new(ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        ));
        let runtime =
            std::sync::Arc::new(PublicAiRuntime::unavailable().expect("runtime should initialize"));
        let (first_tx, first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, second_rx) = tokio::sync::oneshot::channel();
        let service_clone = std::sync::Arc::clone(&service);
        let runtime_clone = std::sync::Arc::clone(&runtime);
        let first = tokio::spawn(async move {
            let _ = first_tx.send(());
            service_clone
                .sync_provider(&runtime_clone, "deepseek")
                .await
        });
        let service_clone = std::sync::Arc::clone(&service);
        let runtime_clone = std::sync::Arc::clone(&runtime);
        let second = tokio::spawn(async move {
            let _ = second_tx.send(());
            service_clone
                .sync_provider(&runtime_clone, "deepseek")
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), first_rx)
            .await
            .expect("first sync should start")
            .expect("first signal should send");
        tokio::time::timeout(std::time::Duration::from_secs(2), second_rx)
            .await
            .expect("second sync should start")
            .expect("second signal should send");
        let first_report = tokio::time::timeout(std::time::Duration::from_secs(5), first)
            .await
            .expect("first sync should finish")
            .expect("first task should not panic")
            .expect("first sync should succeed");
        let second_report = tokio::time::timeout(std::time::Duration::from_secs(5), second)
            .await
            .expect("second sync should finish")
            .expect("second task should not panic")
            .expect("second sync should succeed");
        // Both syncs stop before any network, store, or activation boundary.
        // 两个同步都在任何网络、存储或激活边界之前停止。
        assert_eq!(first_report.outcome, "not_configured");
        assert_eq!(second_report.outcome, "not_configured");
    }

    /// Verifies a corrupted provider state cannot abort the remaining full-catalog sync.
    /// 验证一个供应商状态损坏时不能中止其余供应商的全目录同步。
    ///
    /// Business invariant (plan §R7.7): one report is returned for every requested provider;
    /// a local failure is classified as `sync_failed`, while later providers still download,
    /// verify, and activate from the same signed Index snapshot.
    /// 业务不变量（方案 §R7.7）：每个请求的供应商都必须返回一份报告；本地失败分类为
    /// `sync_failed`，后续供应商仍从同一签名 Index 快照完成下载、校验与激活。
    #[tokio::test]
    async fn sync_all_isolates_provider_state_failure_and_continues() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        let broken_state_path = store.sync_state_path("broken-provider");
        std::fs::create_dir_all(
            broken_state_path
                .parent()
                .expect("sync-state path must have a parent"),
        )
        .expect("sync-state directory should create");
        std::fs::write(&broken_state_path, b"{not-json")
            .expect("corrupted provider state should write");

        let bundled_text = std::fs::read_to_string(
            vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                .join("alibaba.json"),
        )
        .expect("bundled Alibaba manifest must exist");
        let bundled_revision = serde_json::from_str::<serde_json::Value>(&bundled_text)
            .expect("bundled Alibaba manifest should parse")["revision"]
            .as_u64()
            .expect("bundled Alibaba revision must be an unsigned integer");
        let remote_revision = bundled_revision + 1;
        let provider_bytes = alibaba_manifest_e2e_bytes(remote_revision, &[], &[], false);
        let (service, server) = signed_https_sync_service(
            directory.path(),
            "alibaba",
            remote_revision,
            provider_bytes,
            1,
        )
        .await;
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // finalizedProviderIds records the exact production-handler order.
        // finalizedProviderIds 记录精确的生产处理器调用顺序。
        let finalized_provider_ids =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        // handlerProviderIds gives the move closure shared access to the ordered evidence.
        // handlerProviderIds 让 move 闭包共享访问有序证据。
        let handler_provider_ids = std::sync::Arc::clone(&finalized_provider_ids);
        // alibabaActivePath proves the next provider has not crossed its store boundary yet.
        // alibabaActivePath 证明下一个供应商尚未越过其存储边界。
        let alibaba_active_path = store.active_path("alibaba");
        let reports = service
            .sync_all_with_report_handler(
                &runtime,
                &[String::from("broken-provider"), String::from("alibaba")],
                move |report| {
                    // providerIds and activePath are request-local handler evidence.
                    // providerIds 与 activePath 是逐次处理器调用的局部证据。
                    let provider_ids = std::sync::Arc::clone(&handler_provider_ids);
                    let active_path = alibaba_active_path.clone();
                    async move {
                        if report.provider_id == "broken-provider" {
                            assert!(
                                !active_path.exists(),
                                "the next provider must not stage before the current report is finalized"
                            );
                        }
                        provider_ids
                            .lock()
                            .expect("handler order lock should remain available")
                            .push(report.provider_id.clone());
                        Ok(report)
                    }
                },
            )
            .await
            .expect("one provider failure must not abort the full sync");

        assert_eq!(reports.len(), 2, "every provider must receive one report");
        assert_eq!(reports[0].provider_id, "broken-provider");
        assert_eq!(reports[0].outcome, "rejected");
        assert_eq!(reports[0].failure_kind.as_deref(), Some("sync_failed"));
        assert!(
            reports[0]
                .failure_message
                .as_deref()
                .is_some_and(|message| message.contains("invalid JSON")),
            "the isolated report must retain the local state failure"
        );
        assert_eq!(reports[1].provider_id, "alibaba");
        assert_eq!(reports[1].outcome, "activated");
        assert_eq!(reports[1].revision, remote_revision);
        assert_eq!(
            *finalized_provider_ids
                .lock()
                .expect("handler order lock should remain available"),
            [String::from("broken-provider"), String::from("alibaba")],
            "every provider must finish its handler before the next provider is staged"
        );
        server.await.expect("signed server should finish");
    }

    /// Verifies candidate preview performs the full trust/compile/diff flow without any store write.
    /// 验证候选预览执行完整信任/编译/差异流程且不产生任何存储写入。
    #[tokio::test]
    async fn signed_candidate_preview_is_strictly_read_only() {
        // directory is the exact app-data root that must remain untouched by preview.
        // directory 是预览必须保持不变的精确应用数据根目录。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // store resolves every path whose absence proves no hidden staging/state mutation occurred.
        // store 解析每条路径，其缺失证明未发生隐藏 staging/状态修改。
        let store = ManifestStoreLayout::new(directory.path());
        // bundledText supplies the current trusted revision used to create a strictly newer candidate.
        // bundledText 提供用于创建严格较新候选的当前可信修订。
        let bundled_text = std::fs::read_to_string(
            vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                .join("alibaba.json"),
        )
        .expect("bundled Alibaba manifest must exist");
        // bundledRevision is the immutable current runtime baseline.
        // bundledRevision 是不可变当前运行时基线。
        let bundled_revision = serde_json::from_str::<serde_json::Value>(&bundled_text)
            .expect("bundled Alibaba manifest should parse")["revision"]
            .as_u64()
            .expect("bundled Alibaba revision must be an unsigned integer");
        // candidateRevision is strictly monotonic and therefore eligible for semantic preview.
        // candidateRevision 严格单调，因此适用于语义预览。
        let candidate_revision = bundled_revision + 1;
        // candidateBytes add one exact model/rule so the diff has a deterministic added binding.
        // candidateBytes 新增一个精确模型/规则，使差异包含确定性新增绑定。
        let candidate_bytes =
            alibaba_manifest_e2e_bytes(candidate_revision, &["qwen-preview-only"], &[], false);
        // service and server execute the same signed HTTPS index/provider reads as production.
        // service 与 server 执行和生产相同的签名 HTTPS Index/供应商读取。
        let (service, server) = signed_https_sync_service(
            directory.path(),
            "alibaba",
            candidate_revision,
            candidate_bytes,
            1,
        )
        .await;
        // runtime owns the bundled Alibaba comparison snapshot.
        // runtime 持有内置 Alibaba 比较快照。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        assert!(
            !store.manifests_root().exists(),
            "precondition: app-data manifest root must be absent"
        );
        // preview is the verified in-memory candidate diff.
        // preview 是已验证的内存候选差异。
        let baseline = service
            .capture_preview_baseline(&runtime, "alibaba")
            .expect("preview baseline should capture");
        let preview = service
            .preview_provider_manifest(baseline, "alibaba")
            .await
            .expect("candidate preview should succeed");
        assert_eq!(preview.outcome, ProviderManifestPreviewOutcome::Candidate);
        assert!(preview.executable);
        assert_eq!(preview.candidate_revision, Some(candidate_revision));
        assert!(
            preview.model_binding_changes.iter().any(|delta| {
                delta.change == ProviderManifestChangeKind::Added
                    && delta
                        .candidate
                        .as_ref()
                        .is_some_and(|binding| binding.model_id == "qwen-preview-only")
            }),
            "the new exact model binding must be visible in the backend-owned diff"
        );
        assert!(
            !store.manifests_root().exists(),
            "preview must not create staging, Active, Previous, quarantine, sync, or check state"
        );
        server.await.expect("signed server should finish");
    }

    /// R7: full signed remote sync chain over a real HTTPS loopback signing server (plan §R7.14).
    /// R7：在真实 HTTPS 回环签名服务器上执行完整签名远端同步链路（方案 §R7.14）。
    ///
    /// The local signing server serves a signed index and a pinned provider manifest; the
    /// sync service fetches both over TLS, verifies the Ed25519 signature, activates the
    /// provider, hot-replaces the runtime catalog, and short-circuits on the second
    /// conditional sync via ETag.
    /// 本地签名服务器提供签名 Index 与固定供应商清单；同步服务通过 TLS 获取两者、验签
    /// Ed25519 签名、激活供应商、热替换运行时目录，并在第二次条件同步时经 ETag 短路。
    #[tokio::test]
    async fn signed_remote_sync_e2e_over_https_loopback() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // The test build enables both rustls provider features; install one explicitly so
        // the TLS acceptor and the sync client share a deterministic provider.
        // 测试构建同时启用了两个 rustls provider 特性；显式安装一个，使 TLS 接受器与
        // 同步客户端共享确定性 provider。
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();

        let directory = tempfile::tempdir().expect("temp dir should create");

        // Deterministic trusted signing key pair.
        // 确定性受信签名密钥对。
        let mut seed = [0_u8; 32];
        seed[..5].copy_from_slice(b"e2e-1");
        let public_key = vulcan_provider_config::manifest_sync::public_key_for_seed(&seed);
        let trusted = TrustedSigningKey {
            key_id: String::from("e2e-key-1"),
            public_key,
        };

        let remote_revision = bundled_deepseek_revision() + 5;
        // A revision above the bundled trust ceiling is the exact pinned provider document.
        // 高于随包信任下限的修订作为精确固定的供应商文档。
        let provider_bytes = deepseek_manifest_with_revision(remote_revision).into_bytes();
        let digest = {
            use sha2::Digest;
            sha2::Sha256::digest(&provider_bytes)
        };
        let digest_hex = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let now = current_unix_millis() as i64;
        let index = vulcan_provider_config::manifest_sync::ManifestRemoteIndex {
            schema_version: vulcan_provider_config::manifest_sync::INDEX_SCHEMA_VERSION.to_string(),
            index_revision: 1,
            issued_at_unix_ms: now - 1000,
            expires_at_unix_ms: now + 1000,
            signing_key_id: String::from("e2e-key-1"),
            providers: std::collections::BTreeMap::from([(
                String::from("deepseek"),
                vulcan_provider_config::manifest_sync::RemoteProviderEntry {
                    provider_id: String::from("deepseek"),
                    revision: remote_revision,
                    relative_url: String::from("/deepseek.json"),
                    sha256: digest_hex,
                    bytes: provider_bytes.len() as u64,
                    minimum_engine_contract: 1,
                },
            )]),
        };
        let index_bytes = serde_json::to_vec(&index).expect("index should encode");
        let signature =
            vulcan_provider_config::manifest_sync::sign_index_document(&index_bytes, &seed)
                .expect("index should sign");
        let signature_hex = signature
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            signature_hex.len(),
            128,
            "signature header is 128 hex chars"
        );

        // Self-signed certificate for the loopback HTTPS server.
        // 回环 HTTPS 服务器的自签名证书。
        let mut cert_params =
            rcgen::CertificateParams::new(Vec::<String>::new()).expect("cert params");
        cert_params.subject_alt_names = vec![rcgen::SanType::IpAddress(std::net::IpAddr::V4(
            std::net::Ipv4Addr::LOCALHOST,
        ))];
        let key_pair = rcgen::KeyPair::generate().expect("key pair");
        let generated = cert_params.self_signed(&key_pair).expect("certificate");
        let tls_config = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![generated.der().clone()],
                tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der())
                    .expect("private key der"),
            )
            .expect("TLS config");
        let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(tls_config));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let index_url = format!(
            "https://{}/index.json",
            listener.local_addr().expect("loopback addr")
        );

        // The signing server answers: signed index 200, provider 200, then a 304 for the
        // conditional second sync; the loop ends after the three expected connections.
        // 签名服务器应答：签名 Index 200、供应商 200，随后为第二次条件同步应答 304；
        // 三次预期连接后循环结束。
        let index_bytes_for_server = index_bytes.clone();
        let provider_bytes_for_server = provider_bytes.clone();
        let signature_hex_for_server = signature_hex.clone();
        let server = tokio::spawn(async move {
            let mut handled = 0_u32;
            while handled < 3 {
                let (stream, _peer) = listener.accept().await.expect("accept");
                let mut stream = acceptor.accept(stream).await.expect("TLS accept");
                // Read the HTTP request head until the blank line.
                // 读取 HTTP 请求头直到空行。
                let mut head = Vec::new();
                let mut byte = [0_u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    let read = stream.read(&mut byte).await.expect("request read");
                    if read == 0 {
                        break;
                    }
                    head.push(byte[0]);
                }
                let head_text = String::from_utf8_lossy(&head);
                let path = head_text.split_whitespace().nth(1).unwrap_or("/");
                let conditional = head_text
                    .lines()
                    .any(|line| line.to_ascii_lowercase().starts_with("if-none-match:"));
                match (path, conditional) {
                    ("/index.json", false) => {
                        stream
                            .write_all(
                                format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"e2e-index-v1\"\r\nx-vulcan-manifest-signature: {signature_hex_for_server}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    index_bytes_for_server.len()
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("index head write");
                        stream
                            .write_all(&index_bytes_for_server)
                            .await
                            .expect("index body write");
                    }
                    ("/index.json", true) => {
                        stream
                            .write_all(
                                b"HTTP/1.1 304 Not Modified\r\nETag: \"e2e-index-v1\"\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .expect("304 write");
                    }
                    ("/deepseek.json", _) => {
                        stream
                            .write_all(
                                format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"e2e-deepseek-v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    provider_bytes_for_server.len()
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("provider head write");
                        stream
                            .write_all(&provider_bytes_for_server)
                            .await
                            .expect("provider body write");
                    }
                    _ => {
                        stream
                            .write_all(
                                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .expect("404 write");
                    }
                }
                let _ = stream.shutdown().await;
                handled += 1;
            }
        });

        // The dedicated client accepts the self-signed loopback certificate but still
        // disables redirects and keeps the bounded timeouts of the production builder.
        // 专用客户端接受回环自签名证书，但仍禁用重定向并保持生产构建器的有界超时。
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("sync client");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            client,
            vec![trusted],
            Some(index_url),
        );
        let runtime = std::sync::Arc::new(PublicAiRuntime::unavailable().expect("runtime"));

        // First sync fetches, verifies, activates, and hot-replaces the runtime.
        // 第一次同步执行获取、验签、激活并热替换运行时。
        let first = service
            .sync_provider(&runtime, "deepseek")
            .await
            .expect("first sync should succeed");
        assert_eq!(first.outcome, "activated");
        assert_eq!(first.revision, remote_revision);
        runtime
            .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
            .await
            .expect("the verified staged manifest must publish as one generation");
        assert_eq!(
            runtime
                .system_manifest_revisions()
                .expect("revisions")
                .into_iter()
                .find(|(provider_id, _)| provider_id == "deepseek")
                .expect("deepseek revision")
                .1,
            remote_revision,
            "the signed remote revision must become externally visible"
        );
        let store = ManifestStoreLayout::new(directory.path());
        assert!(
            store
                .read_active("deepseek")
                .expect("read active")
                .is_some(),
            "activated bytes must be persisted"
        );

        // Second sync reuses the persisted ETag and short-circuits on 304.
        // 第二次同步复用持久化 ETag 并在 304 时短路。
        let second = service
            .sync_provider(&runtime, "deepseek")
            .await
            .expect("second sync should succeed");
        assert_eq!(second.outcome, "up_to_date");
        assert_eq!(second.revision, remote_revision);

        tokio::time::timeout(std::time::Duration::from_secs(10), server)
            .await
            .expect("signing server should finish")
            .expect("server task should not panic");
    }

    /// Verifies signed JSON hot-adds conversation and media models, freezes in-flight execution,
    /// rejects a bad successor, and rolls back atomically.
    /// 验证签名 JSON 热新增对话与媒体模型、冻结执行中请求、拒绝坏后继版本并原子回滚。
    #[tokio::test]
    async fn signed_manifest_e2e_new_model_rejection_and_rollback() {
        // directory owns the authenticated Active/Previous/state transaction for this E2E.
        // directory 持有本次 E2E 的已认证 Active/Previous/state 事务。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // provider transport preserves the reviewed production hostname while all traffic stays local.
        // 供应商传输保留已审核生产主机名，同时把全部流量约束在本地。
        let (provider_http, mut provider_requests, provider_server) =
            provider_tool_https_fixture().await;
        // secret_store exposes the deterministic request boundary used for the freeze assertion.
        // secret_store 暴露用于冻结断言的确定性请求边界。
        let secret_store = Arc::new(SignedE2eSecretStore::new());
        // runtime uses only the fixture-owned provider transport in this cross-crate E2E.
        // runtime 在本次跨 crate E2E 中只使用夹具持有的供应商传输。
        let runtime = Arc::new(
            PublicAiRuntime::first_batch_with_http_client_for_testing(
                secret_store.clone(),
                provider_http,
            )
            .expect("runtime should initialize"),
        );
        // configuration owns both the text route and the independent media-tool route.
        // configuration 同时持有文本路由与独立媒体工具路由。
        let mut configuration = PersistedAiConfiguration::default();
        // connection_id identifies the conversation catalog receiving JSON-only text models.
        // connection_id 标识接收纯 JSON 文本模型的对话目录。
        let connection_id = String::from("signed-alibaba-account");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                vulcan_provider_adapters::CredentialConnection {
                    id: connection_id.clone(),
                    provider_id: String::from("alibaba"),
                    service_entry_id: String::from("model-studio-cn"),
                    credential_profile_id: String::from("api-key"),
                    credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                    credential_id: String::from("signed-alibaba-credential"),
                    display_name: String::from("Signed Alibaba Account"),
                    secret_reference: String::from("signed-alibaba-secret"),
                    management_secret_reference: None,
                    service_origin_override: None,
                    prompt_cache_by_protocol: std::collections::BTreeMap::new(),
                },
            )
            .expect("Alibaba connection should register");
        // media_connection_id identifies the reviewed Model Studio authority used by templates.
        // media_connection_id 标识模板使用的已审核 Model Studio 权限边界。
        let media_connection_id = String::from("signed-alibaba-media-account");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                vulcan_provider_adapters::CredentialConnection {
                    id: media_connection_id.clone(),
                    provider_id: String::from("alibaba"),
                    service_entry_id: String::from("model-studio-cn"),
                    credential_profile_id: String::from("api-key"),
                    credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                    credential_id: String::from("signed-alibaba-media-credential"),
                    display_name: String::from("Signed Alibaba Media Account"),
                    secret_reference: String::from("signed-alibaba-media-secret"),
                    management_secret_reference: None,
                    service_origin_override: None,
                    prompt_cache_by_protocol: std::collections::BTreeMap::new(),
                },
            )
            .expect("Alibaba media connection should register");
        runtime
            .refresh_connection_catalog(&configuration, &connection_id)
            .await
            .expect("bundled static catalog should refresh");
        runtime
            .refresh_connection_catalog(&configuration, &media_connection_id)
            .await
            .expect("bundled media-service catalog should refresh");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("bundled generation should publish");

        // affected_connection_ids are refreshed and published together for every revision.
        // affected_connection_ids 在每个修订版中一起刷新并发布。
        let affected_connection_ids = vec![connection_id.clone(), media_connection_id.clone()];

        let bundled_revision = runtime
            .system_manifest("alibaba")
            .expect("bundled Alibaba manifest should exist")
            .revision;
        let first_model = "qwen-signed-e2e-first";
        let second_model = "qwen-signed-e2e-second";
        // Media models belong to the independent template-binding catalog, not conversation discovery.
        // 媒体模型属于独立模板绑定目录，而非对话发现目录。
        let first_media_model = "qwen-image-signed-e2e-first";
        let second_media_model = "qwen-image-signed-e2e-second";
        let first_revision = bundled_revision + 1;
        let first_bytes =
            alibaba_manifest_e2e_bytes(first_revision, &[first_model], &[first_media_model], false);
        let first_digest = {
            use sha2::Digest;
            sha2::Sha256::digest(&first_bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        vulcan_provider_config::manifest_sync::verify_provider_bytes(
            &vulcan_provider_config::manifest_sync::RemoteProviderEntry {
                provider_id: String::from("alibaba"),
                revision: first_revision,
                relative_url: String::from("/alibaba.json"),
                sha256: first_digest,
                bytes: first_bytes.len() as u64,
                minimum_engine_contract: 1,
            },
            &first_bytes,
        )
        .expect("the first signed provider bytes must pass bundled verification");
        let (first_service, first_server) =
            signed_https_sync_service(directory.path(), "alibaba", first_revision, first_bytes, 1)
                .await;
        let first_report = first_service
            .sync_provider(&runtime, "alibaba")
            .await
            .expect("first signed sync should succeed");
        assert_eq!(
            first_report.outcome, "activated",
            "unexpected first sync report: {first_report:?}"
        );
        runtime
            .activate_staged_manifest_catalogs(&configuration, &affected_connection_ids)
            .await
            .expect("first signed generation should publish");
        first_server
            .await
            .expect("first signed server should finish");
        assert!(
            runtime.catalog_snapshot().expect("first catalog")[&connection_id]
                .models
                .iter()
                .any(|model| model.id == first_model),
            "the first signed JSON-only model must appear without a software update"
        );

        // first_text_selection proves direct declared-protocol routing without probing or downgrade.
        // first_text_selection 证明直接按声明协议路由，不存在探测或降级。
        let first_text_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                &connection_id,
                "alibaba",
                first_model,
            )
            .expect("first signed conversation model must resolve");
        assert_eq!(
            first_text_selection
                .route
                .protocol_route
                .protocol_profile_id,
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID
        );
        // first_available_target is the deterministic instance expanded from the reviewed template.
        // first_available_target 是从已审核模板确定性展开的实例。
        let first_available_target = runtime
            .available_provider_tool_targets(&configuration)
            .expect("first signed tool targets should resolve")
            .into_iter()
            .find(|available| {
                available.connection_id == media_connection_id
                    && available.target.upstream_target_id == first_media_model
            })
            .expect("first signed image-template instance must be available");
        // first_tool_selection freezes the exact credential and generated target identity.
        // first_tool_selection 冻结精确凭据与生成目标标识。
        let first_tool_selection = vulcan_provider_adapters::ProviderToolTargetSelection {
            connection_id: media_connection_id.clone(),
            target_id: first_available_target.target.target_id.clone(),
        };
        secret_store.arm_next_read_barrier();
        // old_request starts under revision one and pauses only after cloning generation-owned route data.
        // old_request 在第一修订版下启动，并且仅在克隆代际路由数据后暂停。
        let old_request = {
            // request_runtime retains the shared runtime while the task is in flight.
            // request_runtime 在任务执行期间持有共享运行时。
            let request_runtime = Arc::clone(&runtime);
            // request_configuration freezes the invocation's exact user configuration.
            // request_configuration 冻结调用的精确用户配置。
            let request_configuration = configuration.clone();
            // request_selection freezes the first generated target identifier.
            // request_selection 冻结第一份生成目标标识。
            let request_selection = first_tool_selection.clone();
            tokio::spawn(async move {
                request_runtime
                    .execute_provider_tool_target_for_owner(
                        &request_configuration,
                        vulcan_provider_adapters::ProviderToolCapabilityKind::ImageGenerate,
                        &request_selection,
                        serde_json::json!({"prompt": "draw revision one"}),
                        None,
                    )
                    .await
            })
        };
        secret_store.wait_until_read_blocked().await;

        let second_revision = bundled_revision + 2;
        let second_bytes = alibaba_manifest_e2e_bytes(
            second_revision,
            &[first_model, second_model],
            &[first_media_model, second_media_model],
            false,
        );
        let (second_service, second_server) = signed_https_sync_service(
            directory.path(),
            "alibaba",
            second_revision,
            second_bytes,
            2,
        )
        .await;
        let second_report = second_service
            .sync_provider(&runtime, "alibaba")
            .await
            .expect("second signed sync should succeed");
        assert_eq!(second_report.outcome, "activated");
        runtime
            .activate_staged_manifest_catalogs(&configuration, &affected_connection_ids)
            .await
            .expect("second signed generation should publish");
        second_server
            .await
            .expect("second signed server should finish");
        assert!(
            runtime.catalog_snapshot().expect("second catalog")[&connection_id]
                .models
                .iter()
                .any(|model| model.id == second_model),
            "the signed successor model must appear in the successor generation"
        );
        assert!(
            runtime
                .available_provider_tool_targets(&configuration)
                .expect("second signed tool targets should resolve")
                .iter()
                .any(|available| {
                    available.connection_id == media_connection_id
                        && available.target.upstream_target_id == second_media_model
                }),
            "the signed successor media model must appear as a template instance"
        );

        secret_store.release_blocked_read();
        // old_result must complete with revision one's frozen route after revision two is public.
        // old_result 必须在第二修订版发布后仍使用第一修订版冻结路由完成。
        let old_result = old_request
            .await
            .expect("old request task should join")
            .expect("old request should execute through its frozen generation");
        // old_wire_request is the exact provider-native request sent after successor publication.
        // old_wire_request 是后继版本发布后发送的精确供应商原生请求。
        let (old_path, old_wire_request) = provider_requests
            .recv()
            .await
            .expect("old provider request should be captured");
        assert_eq!(
            old_path,
            "/api/v1/services/aigc/multimodal-generation/generation"
        );
        assert_eq!(old_wire_request["model"], first_media_model);
        assert_eq!(
            old_result.upstream_response_id.as_deref(),
            Some("request-qwen-image-signed-e2e-first")
        );
        match &old_result.output {
            vulcan_provider_adapters::ProviderToolNormalizedOutput::Resources(resources) => {
                assert_eq!(resources.len(), 1);
                assert_eq!(
                    resources[0].download_url.as_deref(),
                    Some("https://dashscope.aliyuncs.com/qwen-image-signed-e2e-first.png")
                );
            }
            output => panic!("expected normalized image resources, got {output:?}"),
        }

        // second_available_target resolves only after revision two crosses activation.
        // second_available_target 仅在第二修订版越过激活边界后解析。
        let second_available_target = runtime
            .available_provider_tool_targets(&configuration)
            .expect("second signed tool targets should remain available")
            .into_iter()
            .find(|available| {
                available.connection_id == media_connection_id
                    && available.target.upstream_target_id == second_media_model
            })
            .expect("second signed image-template instance must be available");
        // second_tool_selection targets only the successor media model.
        // second_tool_selection 只指向后继版本媒体模型。
        let second_tool_selection = vulcan_provider_adapters::ProviderToolTargetSelection {
            connection_id: media_connection_id.clone(),
            target_id: second_available_target.target.target_id.clone(),
        };
        // second_result proves a newly started request observes the successor generation.
        // second_result 证明新启动请求观察到后继代际。
        let second_result = runtime
            .execute_provider_tool_target_for_owner(
                &configuration,
                vulcan_provider_adapters::ProviderToolCapabilityKind::ImageGenerate,
                &second_tool_selection,
                serde_json::json!({"prompt": "draw revision two"}),
                None,
            )
            .await
            .expect("second request should execute through the successor generation");
        // second_wire_request must contain the successor model and the same reviewed path.
        // second_wire_request 必须包含后继模型与同一已审核路径。
        let (second_path, second_wire_request) = provider_requests
            .recv()
            .await
            .expect("second provider request should be captured");
        assert_eq!(second_path, old_path);
        assert_eq!(second_wire_request["model"], second_media_model);
        assert_eq!(
            second_result.upstream_response_id.as_deref(),
            Some("request-qwen-image-signed-e2e-second")
        );
        tokio::time::timeout(std::time::Duration::from_secs(10), provider_server)
            .await
            .expect("provider-tool server should finish")
            .expect("provider-tool server task should not panic");

        let bad_revision = bundled_revision + 3;
        let bad_bytes = alibaba_manifest_e2e_bytes(
            bad_revision,
            &[first_model, second_model],
            &[first_media_model, second_media_model],
            true,
        );
        let (bad_service, bad_server) =
            signed_https_sync_service(directory.path(), "alibaba", bad_revision, bad_bytes, 3)
                .await;
        let bad_report = bad_service
            .sync_provider(&runtime, "alibaba")
            .await
            .expect("a verified-index candidate rejection should return a classified report");
        bad_server.await.expect("bad signed server should finish");
        assert_eq!(bad_report.outcome, "rejected");
        assert_eq!(
            bad_report.failure_kind.as_deref(),
            Some("trust_ceiling_failed")
        );
        assert_eq!(
            runtime
                .system_manifest("alibaba")
                .expect("second generation should remain published")
                .revision,
            second_revision,
            "a rejected candidate must not alter the published generation"
        );

        let rollback = bad_service
            .rollback_provider_manifest(&runtime, "alibaba")
            .expect("authenticated previous layer should stage for rollback");
        assert_eq!(rollback.restored_revision, first_revision);
        bad_service
            .finalize_rollback_projection(
                &runtime,
                &configuration,
                rollback,
                |commit| commit(),
                || Ok(()),
            )
            .await
            .expect("rollback catalogs and manifest should publish atomically");
        assert_eq!(
            runtime
                .system_manifest("alibaba")
                .expect("rolled-back manifest should exist")
                .revision,
            first_revision
        );
        let rolled_back = runtime.catalog_snapshot().expect("rolled-back catalog");
        assert!(
            rolled_back[&connection_id]
                .models
                .iter()
                .any(|model| model.id == first_model)
        );
        assert!(
            rolled_back[&connection_id]
                .models
                .iter()
                .all(|model| model.id != second_model),
            "rollback must remove the successor-only static model from the same generation"
        );
        // rolled_back_targets prove media instances roll back in the exact same generation.
        // rolled_back_targets 证明媒体实例在同一精确代际内回滚。
        let rolled_back_targets = runtime
            .available_provider_tool_targets(&configuration)
            .expect("rolled-back tool targets should resolve");
        assert!(rolled_back_targets.iter().any(|available| {
            available.connection_id == media_connection_id
                && available.target.upstream_target_id == first_media_model
        }));
        assert!(
            rolled_back_targets.iter().all(|available| {
                available.connection_id != media_connection_id
                    || available.target.upstream_target_id != second_media_model
            }),
            "rollback must remove the successor-only media template instance"
        );
    }

    /// R7: a 304 index without a complete local active layer forces an unconditional refetch.
    /// R7：本地 active 层不完整时，304 Index 强制无条件重取。
    ///
    /// Business invariant (plan §R7.9): 304 is acceptable only when the local state pins a
    /// complete active file that still re-verifies; a missing active file must refetch.
    /// 业务不变量（方案 §R7.9）：仅当本地状态固定一份完整且仍可重新校验的 active 文件时
    /// 304 才可接受；active 文件缺失时必须重取。
    #[test]
    fn index_304_without_local_active_forces_refetch() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        // The sync state claims an active revision but no active file exists.
        // 同步状态声明存在活动修订，但不存在 active 文件。
        store
            .save_sync_state(
                "deepseek",
                &vulcan_provider_config::manifest_store::ManifestSyncState {
                    revision: 3,
                    sha256: "a".repeat(64),
                    previous: None,
                    etag: Some(String::from("\"etag-3\"")),
                    index_etag: Some(String::from("\"index-etag-3\"")),
                    last_modified: None,
                    synced_at_unix_ms: 1000,
                    last_error_kind: None,
                },
            )
            .expect("state should save");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        // With no remote URL the service stays not_configured; the local-active validation is
        // exercised directly to prove a missing file is not "current".
        // 没有远端 URL 时服务保持 not_configured；直接执行本地 active 校验，证明缺失文件
        // 不视为 "current"。
        let error = service
            .local_active_is_current("deepseek", &3, &"a".repeat(64))
            .expect_err("missing active file must not validate as current");
        assert!(error.contains("no active layer"), "{error}");
    }

    /// Spawns one loopback HTTP server that answers a fixed number of connections.
    /// 启动一个应答固定连接次数的回环 HTTP 服务器。
    ///
    /// # Parameters
    /// 参数
    /// `responses` are the raw HTTP response heads and bodies served in accept order; the
    /// request is drained before each response so redirect-following clients can be served twice.
    /// `responses` 按 accept 顺序提供的原始 HTTP 响应头与响应体；每次响应前都会排空请求，
    /// 以便跟随重定向的客户端可被服务两次。
    ///
    /// # Returns
    /// 返回值
    /// The loopback URL of the first accepted connection.
    /// 返回首个被接受连接的回环 URL。
    fn spawn_loopback_server(responses: Vec<(String, Vec<u8>)>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind should succeed");
        let url = format!("http://{}/index.json", listener.local_addr().expect("addr"));
        std::thread::spawn(move || {
            for (head, body) in responses {
                let (mut stream, _peer) = listener.accept().expect("accept should succeed");
                let mut buffer = [0_u8; 8192];
                let _read = std::io::Read::read(&mut stream, &mut buffer);
                std::io::Write::write_all(&mut stream, head.as_bytes()).expect("head should write");
                std::io::Write::write_all(&mut stream, &body).expect("body should write");
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        });
        url
    }

    /// R7: the configured index endpoint must resolve to one unambiguous credential-free HTTPS origin.
    /// R7：配置的 Index 端点必须解析为唯一且不含凭据的 HTTPS 源。
    ///
    /// Business invariant (plan §R7.4): URL text that can change authority interpretation,
    /// carry credentials, or preserve a client-only fragment is rejected before network I/O.
    /// 业务不变量（方案 §R7.4）：可能改变 authority 解释、携带凭据或保留仅客户端片段的
    /// URL 文本必须在网络 I/O 前被拒绝。
    #[test]
    fn index_origin_rejects_ambiguous_authority() {
        // origin is the canonical scheme-and-authority identity used for provider URL joining.
        // origin 是供应商 URL 拼接使用的规范 scheme 与 authority 身份。
        let origin = index_origin("https://manifests.example.com/v1/index.json?channel=stable")
            .expect("one credential-free HTTPS URL should be accepted");
        assert_eq!(origin, "https://manifests.example.com");

        // unsafe_urls enumerate every configured-endpoint ambiguity rejected by the parser guard.
        // unsafe_urls 枚举解析器守卫拒绝的每类配置端点歧义。
        let unsafe_urls = [
            "http://manifests.example.com/v1/index.json",
            "https://user:secret@manifests.example.com/v1/index.json",
            "https://manifests.example.com/v1/index.json#candidate",
            "https://manifests.example.com\\@evil.example/v1/index.json",
            "not a URL",
        ];
        for unsafe_url in unsafe_urls {
            assert!(
                index_origin(unsafe_url).is_err(),
                "unsafe index URL must be rejected: {unsafe_url}"
            );
        }
    }

    /// R0/P0-04: remote index reads must be bounded by an actual cumulative byte limit.
    /// R0/P0-04：远端 Index 读取必须受真实累计字节上限约束。
    ///
    /// Business invariant (plan §6.7/§11.3, R7): every network body is read with a real
    /// cumulative byte cap that fails on the next byte after the limit; an oversized
    /// Content-Length must be rejected before buffering the whole body.
    /// 业务不变量（方案 §6.7/§11.3、阶段 R7）：每个网络响应体都按真实累计字节上限读取，
    /// 超过上限的下一字节立即失败；超限 Content-Length 必须在缓冲完整响应体前被拒绝。
    #[test]
    fn fetch_conditional_rejects_oversized_content_length() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            Some(String::from("https://example.test/index.json")),
        );
        let oversized = vulcan_provider_config::manifest_sync::REMOTE_INDEX_MAX_BYTES as usize + 1;
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {oversized}\r\nConnection: close\r\n\r\n"
        );
        let url = spawn_loopback_server(vec![(head, vec![b'x'; oversized])]);
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("async runtime")
            .block_on(service.fetch_conditional(
                &url,
                None,
                vulcan_provider_config::manifest_sync::REMOTE_INDEX_MAX_BYTES,
            ))
            .expect_err("an oversized remote body must be rejected by the byte bound");
        assert!(
            error.contains("bound") || error.contains("limit") || error.contains("bytes"),
            "unexpected size-limit error: {error}"
        );
    }

    /// R7: a chunked body without Content-Length still fails at the cumulative byte bound.
    /// R7：无 Content-Length 的分块响应体仍在累计字节上限处失败。
    ///
    /// Business invariant (plan §R7.5): the streaming cumulative cap is the final authority
    /// when the remote omits or lies about Content-Length.
    /// 业务不变量（方案 §R7.5）：远端省略或谎报 Content-Length 时，流式累计上限是最终权威。
    #[test]
    fn fetch_conditional_rejects_chunked_body_over_bound() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            Some(String::from("https://example.test/index.json")),
        );
        let oversized = vulcan_provider_config::manifest_sync::REMOTE_INDEX_MAX_BYTES as usize + 1;
        // No Content-Length and no Content-Type: the media-type check runs first, so the
        // fixture carries the accepted media type and only the cumulative byte cap applies.
        // 无 Content-Length 且无 Content-Type：媒体类型校验先行，因此该装置携带受接受的
        // 媒体类型，只适用累计字节上限。
        let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n";
        let url = spawn_loopback_server(vec![(head.to_string(), vec![b'x'; oversized])]);
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("async runtime")
            .block_on(service.fetch_conditional(
                &url,
                None,
                vulcan_provider_config::manifest_sync::REMOTE_INDEX_MAX_BYTES,
            ))
            .expect_err("an oversized chunked body must be rejected by the cumulative bound");
        assert!(
            error.contains("bound") || error.contains("limit") || error.contains("bytes"),
            "unexpected size-limit error: {error}"
        );
    }

    /// R0/P0-04: remote manifest reads must never follow redirects.
    /// R0/P0-04：远端清单读取绝不跟随重定向。
    ///
    /// Business invariant (plan §6.7/§11.3, R7): the sync client is built with redirects
    /// disabled and any 3xx answer fails the fetch; a redirect must never deliver the
    /// response from a second origin.
    /// 业务不变量（方案 §6.7/§11.3、阶段 R7）：同步客户端禁用重定向，任何 3xx 响应
    /// 都必须使获取失败；重定向绝不能从第二个源返回响应。
    #[test]
    fn fetch_conditional_rejects_redirect() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            Some(String::from("https://example.test/index.json")),
        );
        let redirect = "HTTP/1.1 302 Found\r\nLocation: /moved.json\r\nContent-Length: 0\r\n\r\n";
        let target = "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc";
        let url = spawn_loopback_server(vec![
            (redirect.to_string(), Vec::new()),
            (target.to_string(), Vec::new()),
        ]);
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("async runtime")
            .block_on(service.fetch_conditional(&url, None, REMOTE_PROVIDER_MAX_BYTES))
            .expect_err("a redirect answer must fail the fetch");
        assert!(
            error.contains("redirect") || error.contains("3"),
            "unexpected redirect error: {error}"
        );
    }

    /// R7: response URL identity is rechecked even when an injected client follows redirects.
    /// R7：即使注入客户端跟随重定向，也必须重新校验响应 URL 身份。
    ///
    /// Business invariant (plan §R7.4): the dedicated no-redirect client is not the only
    /// authority boundary; constructor misuse must still fail closed after the response arrives.
    /// 业务不变量（方案 §R7.4）：专用禁重定向客户端不是唯一 authority 边界；即使构造器
    /// 被误用，也必须在响应到达后保持关闭失败。
    #[test]
    fn fetch_conditional_rejects_redirect_followed_by_injected_client() {
        // directory isolates the manifest store used by this constructor-level regression test.
        // directory 隔离此构造器级回归测试使用的清单存储。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // redirecting_client deliberately violates the production constructor policy.
        // redirecting_client 故意违反生产构造器的禁重定向策略。
        let redirecting_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(2))
            .build()
            .expect("redirect-following test client should build");
        // service proves the response-identity guard survives dependency injection misuse.
        // service 证明响应身份守卫可抵御依赖注入误用。
        let service = ProviderManifestSyncService::new(
            directory.path(),
            redirecting_client,
            Vec::new(),
            Some(String::from("https://example.test/index.json")),
        );
        // redirect points the injected client at a distinct final path on the same origin.
        // redirect 将注入客户端指向同源但最终路径不同的地址。
        let redirect = "HTTP/1.1 302 Found\r\nLocation: /moved.json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        // target is otherwise a valid JSON response so only URL identity can reject it.
        // target 是其他方面均有效的 JSON 响应，确保只有 URL 身份会拒绝它。
        let target = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n";
        // url is the exact initial endpoint captured before the client follows the redirect.
        // url 是客户端跟随重定向前捕获的精确初始端点。
        let url = spawn_loopback_server(vec![
            (redirect.to_string(), Vec::new()),
            (target.to_string(), b"{}".to_vec()),
        ]);
        // error must come from the post-response URL identity guard.
        // error 必须来自响应后的 URL 身份守卫。
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("async runtime")
            .block_on(service.fetch_conditional(&url, None, REMOTE_PROVIDER_MAX_BYTES))
            .expect_err("a followed redirect must fail the response identity check");
        assert!(
            error.contains("forbidden redirect"),
            "unexpected followed-redirect error: {error}"
        );
    }

    /// Verifies the typed status snapshot exposes bundled/active/previous layers, sync state,
    /// and quarantine summaries for one provider (plan §R10).
    /// 验证 typed 状态快照暴露一个供应商的 bundled/active/previous 层、同步状态与隔离摘要
    /// （方案 §R10）。
    #[tokio::test]
    async fn provider_manifest_status_snapshot_exposes_layers() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        let active_revision = bundled_deepseek_revision() + 1;
        // addedModelId is absent from Bundled and must be marked after the first activation.
        // addedModelId 不存在于 Bundled，首次激活后必须被标记。
        let added_model_id = "deepseek-status-added";
        let text = deepseek_manifest_with_exact_model(active_revision, added_model_id);
        // Stage and activate a revision above bundled so Active and Bundled form a contrast.
        // 暂存并激活高于随包版本的修订，使 Active 与 Bundled 形成对照。
        activate_trusted_deepseek(&store, active_revision, &text);
        store
            .quarantine("deepseek", 2, "signature_invalid", b"candidate")
            .expect("quarantine should write");

        let http = build_sync_http_client().expect("sync client");
        let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // The bundled manifest is the baseline; restore the persisted active layer so the
        // runtime carries a non-bundled active snapshot.
        // 内置清单是基线；恢复持久化 active 层使运行时携带非 bundled 的 active 快照。
        restore_system_manifests_from_store(&runtime, directory.path())
            .expect("restore should succeed");
        runtime
            .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
            .await
            .expect("restored active layer must publish before status is observed");

        let status = service
            .provider_manifest_status(&runtime, "deepseek")
            .expect("status snapshot should build");
        // The snapshot is a typed serializable contract (plan §R10 RPC 要求 5).
        // 快照是 typed 可序列化契约（方案 §R10 RPC 要求 5）。
        let serialized =
            serde_json::to_value(&status).expect("status must serialize as typed value");
        assert_eq!(status.provider_id, "deepseek");
        assert!(!status.sync_configured, "sync must be unconfigured");
        let bundled = status
            .bundled
            .as_ref()
            .expect("bundled layer must be readable");
        assert!(bundled.revision >= 1, "bundled revision");
        assert_eq!(bundled.sha256.len(), 64, "sha256 hex length");
        let active = status
            .active
            .as_ref()
            .expect("active layer must be present");
        assert_eq!(active.layer, ManifestActiveLayer::Active);
        assert_eq!(active.revision, active_revision);
        // changeBaseline proves first activation compares against Bundled without inventing Previous.
        // changeBaseline 证明首次激活会与 Bundled 比较，而不会虚构 Previous。
        let change_baseline = status
            .active_model_binding_change_baseline
            .as_ref()
            .expect("first activation must retain a bundled change baseline");
        assert_eq!(change_baseline.layer, ManifestActiveLayer::Bundled);
        assert_eq!(change_baseline.revision, bundled.revision);
        assert_eq!(status.active_model_binding_change_total_count, 1);
        assert!(!status.active_model_binding_changes_truncated);
        assert!(status.active_model_binding_changes.iter().any(|delta| {
            delta.change == ProviderManifestChangeKind::Added
                && delta
                    .candidate
                    .as_ref()
                    .is_some_and(|binding| binding.model_id == added_model_id)
        }));
        assert_eq!(
            status.quarantine.len(),
            1,
            "quarantine summary must list the rejected candidate"
        );
        assert_eq!(status.quarantine[0].failure_kind, "signature_invalid");
        assert!(
            serialized.get("active").is_some(),
            "typed serialized active"
        );
        assert!(
            !status.model_rules.is_empty(),
            "active model rules must be exposed with provenance"
        );
        assert!(
            status
                .model_rules
                .iter()
                .all(|rule| rule.field_provenance.contains_key("supported_protocols")),
            "every expanded rule must explain its effective protocol candidates"
        );
        assert_eq!(
            status.model_binding_total_count,
            status.model_bindings.len(),
            "the bounded DeepSeek inventory should be complete"
        );
        assert!(
            !status.model_bindings.is_empty()
                && status.model_bindings.iter().all(|binding| {
                    binding.executable
                        && binding.effective_protocol.is_some()
                        && binding.binding_source.as_deref() == Some("system_manifest")
                        && binding.manifest_revision == active_revision
                }),
            "every concrete DeepSeek model/profile binding must be backend-resolved and executable"
        );
        assert!(
            status.operations.iter().all(|operation| {
                operation.timeout_ms.is_some()
                    || operation.carrier
                        == vulcan_provider_adapters::provider_manifest::OperationCarrier::ConversationProtocol
            }),
            "every non-conversation operation must expose its timeout contract"
        );
        // alibabaStatus exercises the asynchronous lifecycle fields that DeepSeek does not own.
        // alibabaStatus 覆盖 DeepSeek 不持有的异步生命周期字段。
        let alibaba_status = service
            .provider_manifest_status(&runtime, "alibaba")
            .expect("bundled Alibaba status should build");
        // asyncOperations are the exact backend-owned operation contracts rendered by the UI.
        // asyncOperations 是 UI 渲染的精确后端 operation 契约。
        let async_operations = alibaba_status
            .operations
            .iter()
            .filter(|operation| operation.asynchronous)
            .collect::<Vec<_>>();
        assert!(
            !async_operations.is_empty(),
            "Alibaba must expose asynchronous operations"
        );
        assert!(
            async_operations.iter().all(|operation| {
                operation
                    .poll_interval_ms
                    .is_some_and(|interval| interval > 0)
                    && operation
                        .max_backoff_ms
                        .zip(operation.poll_interval_ms)
                        .is_some_and(|(backoff, interval)| backoff >= interval)
                    && operation.timeout_ms.is_some()
            }),
            "every asynchronous operation must expose its polling and backoff contract"
        );
        assert!(
            async_operations
                .iter()
                .any(|operation| operation.cancellation_supported),
            "the status must preserve declared cancellation support"
        );
        assert!(
            status
                .declaration_provenance
                .values()
                .all(|provenance| provenance.manifest_revision == active_revision),
            "declaration provenance must use the exact active revision"
        );
        assert!(
            serialized.get("model_rules").is_some(),
            "typed serialized model rules"
        );
        assert!(
            serialized.get("model_bindings").is_some(),
            "typed serialized model bindings"
        );
        assert!(
            serialized
                .get("active_model_binding_change_baseline")
                .is_some(),
            "typed serialized active binding baseline"
        );
        assert!(
            serialized.get("active_model_binding_changes").is_some(),
            "typed serialized active binding changes"
        );
        assert!(
            serialized.get("operations").is_some(),
            "typed serialized operation status"
        );
        assert!(
            serialized.get("declaration_provenance").is_some(),
            "typed serialized declaration provenance"
        );

        // originalBinding supplies a complete effective status for semantic diff normalization.
        // originalBinding 为语义差异归一化提供一条完整有效状态。
        let original_binding = status
            .model_bindings
            .iter()
            .find(|binding| binding.model_id != added_model_id)
            .cloned()
            .expect("an existing DeepSeek binding must be present");
        // revisionOnlyBinding changes generation identity but no effective model behavior.
        // revisionOnlyBinding 只改变代际身份，不改变任何有效模型行为。
        let mut revision_only_binding = original_binding.clone();
        revision_only_binding.manifest_revision =
            revision_only_binding.manifest_revision.saturating_add(1);
        for provenance in revision_only_binding.capability_provenance.values_mut() {
            provenance.manifest_revision = provenance.manifest_revision.saturating_add(1);
            for overridden in &mut provenance.overridden_sources {
                overridden.manifest_revision = overridden.manifest_revision.saturating_add(1);
            }
        }
        // revisionOnlyDiff must stay empty so a pure revision bump cannot mark every model changed.
        // revisionOnlyDiff 必须保持为空，使纯修订升级不能把全部模型标成已变更。
        let (revision_only_diff, revision_only_total, revision_only_truncated) =
            diff_manifest_model_bindings(
                std::slice::from_ref(&original_binding),
                std::slice::from_ref(&revision_only_binding),
            );
        assert!(revision_only_diff.is_empty());
        assert_eq!(revision_only_total, 0);
        assert!(!revision_only_truncated);
        revision_only_binding.effective_protocol = Some(String::from("semantic-change.v1"));
        // semanticDiff proves effective routing still produces a changed marker.
        // semanticDiff 证明有效路由变化仍会产生 changed 标记。
        let (semantic_diff, semantic_total, semantic_truncated) = diff_manifest_model_bindings(
            std::slice::from_ref(&original_binding),
            std::slice::from_ref(&revision_only_binding),
        );
        assert_eq!(semantic_total, 1);
        assert!(!semantic_truncated);
        assert_eq!(semantic_diff[0].change, ProviderManifestChangeKind::Changed);
    }

    /// Verifies status includes discovery-only models and mirrors the runtime's exact
    /// bound-protocol variant guard for each configured connection.
    /// 验证状态包含仅发现模型，并针对每个已配置连接镜像运行时精确绑定协议变体护栏。
    #[test]
    fn provider_manifest_status_includes_discovered_models_and_available_protocols() {
        // directory isolates the status store; no remote synchronization participates.
        // directory 隔离状态存储；本测试不参与远端同步。
        let directory = tempfile::tempdir().expect("temp dir should create");
        let http = build_sync_http_client().expect("sync client");
        let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // configuration owns one exact OpenAI discovery account.
        // configuration 持有一个精确 OpenAI 发现账号。
        let mut configuration = PersistedAiConfiguration::default();
        let connection_id = String::from("openai-discovery-status");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                vulcan_provider_adapters::CredentialConnection {
                    id: connection_id.clone(),
                    provider_id: String::from("openai"),
                    service_entry_id: String::from("public-api"),
                    credential_profile_id: String::from("api-key"),
                    credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                    credential_id: String::from("openai-discovery-credential"),
                    display_name: String::from("OpenAI Discovery Status"),
                    secret_reference: String::from("openai-discovery-secret"),
                    management_secret_reference: None,
                    service_origin_override: None,
                    prompt_cache_by_protocol: BTreeMap::new(),
                },
            )
            .expect("OpenAI connection should register");
        // readyModel proves an unknown discovered model can use the Manifest default binding.
        // readyModel 证明未知发现模型可使用 Manifest 默认绑定。
        let mut ready_model = vulcan_provider_adapters::ConnectionModel {
            id: String::from("openai-dynamic-ready"),
            display_name: String::from("OpenAI Dynamic Ready"),
            capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        ready_model.protocol_variants.insert(
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID.to_string(),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
                capability_evidence: Default::default(),
                discovery_signature: String::from("openai-status-chat-signature"),
                catalog_generation: 1,
            },
        );
        // missingModel intentionally lacks the Manifest-selected Chat variant.
        // missingModel 有意缺少 Manifest 选择的 Chat 变体。
        let mut missing_model = vulcan_provider_adapters::ConnectionModel {
            id: String::from("openai-dynamic-missing"),
            display_name: String::from("OpenAI Dynamic Missing"),
            capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        missing_model.protocol_variants.insert(
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID.to_string(),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities: vulcan_provider_adapters::ModelCapabilities::default(),
                capability_evidence: Default::default(),
                discovery_signature: String::from("openai-status-responses-signature"),
                catalog_generation: 1,
            },
        );
        runtime
            .stage_source_catalog_for_testing(vulcan_provider_adapters::ConnectionModelCatalog {
                connection_id: connection_id.clone(),
                models: vec![ready_model, missing_model],
                generation: 1,
                page_count: 1,
                discovery_schema_revision: String::from("openai-status-fixture-v1"),
            })
            .expect("discovered source catalog should stage");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("discovered catalog should cross a publication boundary");
        runtime
            .replace_configuration(configuration)
            .expect("status configuration should publish");

        let status = service
            .provider_manifest_status(&runtime, "openai")
            .expect("status snapshot should build");
        let ready = status
            .model_bindings
            .iter()
            .find(|binding| {
                binding.connection_id.as_deref() == Some(connection_id.as_str())
                    && binding.model_id == "openai-dynamic-ready"
            })
            .expect("discovery-only ready model must be present");
        assert_eq!(ready.model_source, ManifestModelSourceStatus::Discovery);
        assert_eq!(
            ready.available_protocols,
            vec![vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID.to_string()]
        );
        assert_eq!(
            ready.effective_protocol.as_deref(),
            Some(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID)
        );
        assert!(ready.catalog_observed && ready.catalog_enabled && ready.executable);
        assert!(
            ready.capabilities.is_some() && ready.capability_evidence.is_some(),
            "the effective route capability and evidence must be exposed"
        );

        let missing = status
            .model_bindings
            .iter()
            .find(|binding| {
                binding.connection_id.as_deref() == Some(connection_id.as_str())
                    && binding.model_id == "openai-dynamic-missing"
            })
            .expect("discovery-only missing-variant model must be present");
        assert_eq!(missing.model_source, ManifestModelSourceStatus::Discovery);
        assert_eq!(
            missing.available_protocols,
            vec![vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID.to_string()]
        );
        assert!(
            !missing.executable,
            "missing bound variant must block execution"
        );
        assert_eq!(
            missing.unavailable_reason.as_deref(),
            Some(
                "Model `openai-dynamic-missing` has no capability variant for its bound protocol `openai-chat-completions.v1`; execution is blocked."
            )
        );
    }

    /// Verifies an unconfigured service reports `sync_configured=false` and `bundled` remains
    /// the active layer when no store state exists (plan §R10 "未配置" 状态清晰可见).
    /// 验证未配置服务报告 `sync_configured=false` 且无存储状态时 bundled 保持为 active 层
    /// （方案 §R10 "未配置" 状态清晰可见）。
    #[test]
    fn provider_manifest_status_reports_unconfigured_bundled_active() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let http = build_sync_http_client().expect("sync client");
        let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let status = service
            .provider_manifest_status(&runtime, "deepseek")
            .expect("status snapshot should build");
        assert!(!status.sync_configured, "sync must be unconfigured");
        assert!(status.sync_state.is_none(), "no sync state yet");
        assert!(status.previous.is_none(), "no previous layer yet");
        let active = status.active.expect("bundled active must be present");
        assert_eq!(
            active.layer,
            ManifestActiveLayer::Bundled,
            "without a store layer the bundled baseline is the active manifest"
        );
    }

    /// Verifies APIKEY.FAN status reflects Manifest cutover while Alibaba template instances remain visible.
    /// 验证 APIKEY.FAN 状态反映 Manifest 切换，同时保持 Alibaba 模板实例可见。
    #[test]
    fn provider_manifest_status_exposes_apikeyfan_cutover_and_template_instances() {
        // directory isolates manifest-store state so both statuses come only from bundled facts.
        // directory 隔离 Manifest 存储状态，使两份状态都只来自内置事实。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // http is required by the service even though this status-only test performs no network I/O.
        // http 是服务构造所需依赖，尽管本状态测试不会执行网络 I/O。
        let http = build_sync_http_client().expect("sync client");
        // service reads status through the same production projection used by the typed RPC.
        // service 通过 typed RPC 使用的同一生产投影读取状态。
        let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
        // runtime owns the authoritative compiled bundled generation.
        // runtime 持有权威的已编译内置代际。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");

        // apikeyfan proves the bundled Manifest is now the active binding authority.
        // apikeyfan 证明内置 Manifest 现已成为活动绑定权威。
        let apikeyfan = service
            .provider_manifest_status(&runtime, "apikeyfan")
            .expect("APIKEY.FAN status should build");
        assert!(apikeyfan.bundled.is_some());
        assert!(apikeyfan.active.is_some());

        // alibaba exposes generated targets without requiring the UI to recompute templates.
        // alibaba 公开生成目标，无需 UI 重新计算模板。
        let alibaba = service
            .provider_manifest_status(&runtime, "alibaba")
            .expect("Alibaba status should build");
        assert!(
            alibaba
                .tool_target_template_instances
                .iter()
                .any(|instance| {
                    instance.template_id == "alibaba.model-studio-cn.qwen-image-generate.v1"
                        && instance.service_entry_id == "model-studio-cn"
                        && instance.model_id == "qwen-image-2.0-pro"
                }),
            "the concrete Qwen Image template instance must be visible in typed status"
        );
    }

    /// Verifies an explicit rollback restores the previous layer and hot-replaces the runtime.
    /// 验证显式回滚恢复 previous 层并热替换运行时。
    #[tokio::test]
    async fn explicit_rollback_restores_previous_layer() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        let previous_revision = bundled_deepseek_revision() + 1;
        let active_revision = previous_revision + 1;
        let text = deepseek_manifest_with_revision(previous_revision);
        activate_trusted_deepseek(&store, previous_revision, &text);
        // Rotate the first authenticated revision into Previous by activating its successor.
        // 通过激活后一修订，把首个已认证修订轮换到 Previous。
        let second = deepseek_manifest_with_revision(active_revision);
        activate_trusted_deepseek(&store, active_revision, &second);

        let http = build_sync_http_client().expect("sync client");
        let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        restore_system_manifests_from_store(&runtime, directory.path())
            .expect("restore should succeed");
        let configuration = PersistedAiConfiguration::default();
        runtime
            .activate_staged_manifest_catalogs(&configuration, &[])
            .await
            .expect("initial active generation must publish");
        assert_eq!(
            runtime
                .system_manifest_revisions()
                .expect("revisions")
                .into_iter()
                .find(|(id, _)| id == "deepseek")
                .expect("deepseek revision")
                .1,
            active_revision,
            "the active revision must be visible before rollback"
        );

        let report = service
            .rollback_provider_manifest(&runtime, "deepseek")
            .expect("rollback should succeed");
        assert_eq!(report.restored_revision, previous_revision);
        service
            .finalize_rollback_projection(
                &runtime,
                &configuration,
                report,
                |commit| commit(),
                || Ok(()),
            )
            .await
            .expect("first rollback projection should publish and commit");
        assert_eq!(
            runtime
                .system_manifest_revisions()
                .expect("revisions")
                .into_iter()
                .find(|(id, _)| id == "deepseek")
                .expect("deepseek revision")
                .1,
            previous_revision,
            "the runtime must observe the rolled-back revision"
        );

        // A second rollback swaps back to the authenticated successor; rollback is a
        // content-addressed version toggle, not a blind copy of stale Previous bytes.
        // 第二次回滚交换回已认证后一修订；回滚是内容寻址版本切换，而不是盲目复制旧 Previous。
        let again = service
            .rollback_provider_manifest(&runtime, "deepseek")
            .expect("idempotent rollback should succeed");
        assert_eq!(again.restored_revision, active_revision);
        service
            .finalize_rollback_projection(
                &runtime,
                &configuration,
                again,
                |commit| commit(),
                || Ok(()),
            )
            .await
            .expect("second rollback projection should publish and commit");
    }

    /// Verifies a service projection/event failure restores exact disk and runtime before-images.
    /// 验证服务投影/事件失败会恢复精确磁盘与运行时前镜像。
    #[tokio::test]
    async fn rollback_projection_failure_restores_exact_before_image() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        let previous_revision = bundled_deepseek_revision() + 1;
        let active_revision = previous_revision + 1;
        let first = deepseek_manifest_with_revision(previous_revision);
        activate_trusted_deepseek(&store, previous_revision, &first);
        let second = deepseek_manifest_with_revision(active_revision);
        activate_trusted_deepseek(&store, active_revision, &second);

        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        restore_system_manifests_from_store(&runtime, directory.path())
            .expect("restore should succeed");
        let configuration = PersistedAiConfiguration::default();
        runtime
            .activate_staged_manifest_catalogs(&configuration, &[])
            .await
            .expect("initial generation should publish");
        let before = store
            .capture_system_snapshot("deepseek")
            .expect("before-image must capture");

        let report = service
            .rollback_provider_manifest(&runtime, "deepseek")
            .expect("rollback staging should succeed");
        let publish_calls = std::cell::Cell::new(0_u8);
        let error = service
            .finalize_rollback_projection(
                &runtime,
                &configuration,
                report,
                |_commit| {
                    publish_calls.set(publish_calls.get() + 1);
                    Err(String::from("injected event emission failure"))
                },
                || {
                    publish_calls.set(publish_calls.get() + 1);
                    Ok(())
                },
            )
            .await
            .expect_err("the injected projection failure must reject the rollback");
        assert!(error.contains("injected event emission failure"), "{error}");
        assert_eq!(
            publish_calls.get(),
            2,
            "the restored former generation must be republished exactly once"
        );
        assert_eq!(
            store
                .capture_system_snapshot("deepseek")
                .expect("restored snapshot must capture"),
            before,
            "Active, Previous, and state bytes must match the exact before-image"
        );
        assert_eq!(
            runtime
                .system_manifest_revisions()
                .expect("revisions")
                .into_iter()
                .find(|(id, _)| id == "deepseek")
                .expect("deepseek revision")
                .1,
            active_revision,
            "the staged runtime catalog must return to the exact active revision"
        );
    }

    /// Verifies rollback publication must invoke its durable commit callback exactly once.
    /// 验证回滚发布必须恰好调用一次持久提交回调。
    #[tokio::test]
    async fn rollback_projection_requires_exactly_one_commit_callback() {
        // directory owns the authenticated active/previous layers exercised by both failures.
        // directory 持有两种失败共同使用的已认证 active/previous 层。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // store provides exact before-image capture and rollback staging.
        // store 提供精确前镜像捕获与回滚暂存。
        let store = ManifestStoreLayout::new(directory.path());
        // previousRevision identifies the first authenticated layer.
        // previousRevision 标识首个已认证层。
        let previous_revision = bundled_deepseek_revision() + 1;
        // activeRevision identifies the currently published successor.
        // activeRevision 标识当前已发布的后继版本。
        let active_revision = previous_revision + 1;
        // previousText is the exact signed-test document rotated into Previous.
        // previousText 是被轮换到 Previous 的精确签名测试文档。
        let previous_text = deepseek_manifest_with_revision(previous_revision);
        activate_trusted_deepseek(&store, previous_revision, &previous_text);
        // activeText is the exact signed-test document visible before each rejected projection.
        // activeText 是每次被拒投影前可见的精确签名测试文档。
        let active_text = deepseek_manifest_with_revision(active_revision);
        activate_trusted_deepseek(&store, active_revision, &active_text);
        // service owns the rollback journal and compensation implementation under test.
        // service 持有本测试所验证的回滚日志与补偿实现。
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        // runtime mirrors the authenticated store before rollback staging begins.
        // runtime 在回滚暂存开始前镜像已认证存储。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        restore_system_manifests_from_store(&runtime, directory.path())
            .expect("restore should succeed");
        // configuration supplies the exact empty connection graph used by this isolated rollback.
        // configuration 提供该隔离回滚使用的精确空连接图。
        let configuration = PersistedAiConfiguration::default();
        runtime
            .activate_staged_manifest_catalogs(&configuration, &[])
            .await
            .expect("initial generation should publish");
        // before captures every durable layer before either invalid projection is attempted.
        // before 捕获两次无效投影尝试前的全部持久层。
        let before = store
            .capture_system_snapshot("deepseek")
            .expect("before-image must capture");

        // missingCommitReport stages the first rollback whose projection omits the callback.
        // missingCommitReport 暂存首个省略回调的回滚。
        let missing_commit_report = service
            .rollback_provider_manifest(&runtime, "deepseek")
            .expect("rollback staging should succeed");
        // missingCommitError must reject apparent projection success without journal commit.
        // missingCommitError 必须拒绝未提交日志却声称投影成功的结果。
        let missing_commit_error = service
            .finalize_rollback_projection(
                &runtime,
                &configuration,
                missing_commit_report,
                |_commit| Ok(()),
                || Ok(()),
            )
            .await
            .expect_err("projection success without commit must fail closed");
        assert!(
            missing_commit_error.contains("without committing its activation journal"),
            "{missing_commit_error}"
        );
        assert_eq!(
            store
                .capture_system_snapshot("deepseek")
                .expect("compensated snapshot must capture"),
            before,
            "a missing commit callback must restore the exact durable before-image"
        );

        // duplicateCommitReport stages a second rollback after exact first-attempt compensation.
        // duplicateCommitReport 在首次尝试精确补偿后暂存第二次回滚。
        let duplicate_commit_report = service
            .rollback_provider_manifest(&runtime, "deepseek")
            .expect("second rollback staging should succeed");
        // duplicateCommitError must reject a projection that crosses the commit seam twice.
        // duplicateCommitError 必须拒绝两次穿越提交接缝的投影。
        let duplicate_commit_error = service
            .finalize_rollback_projection(
                &runtime,
                &configuration,
                duplicate_commit_report,
                |commit| {
                    commit()?;
                    commit()
                },
                || Ok(()),
            )
            .await
            .expect_err("duplicate commit callback use must fail closed");
        assert!(
            duplicate_commit_error.contains("more than once"),
            "{duplicate_commit_error}"
        );
        assert_eq!(
            store
                .capture_system_snapshot("deepseek")
                .expect("twice-compensated snapshot must capture"),
            before,
            "duplicate commit use must restore the exact durable before-image"
        );
        assert_eq!(
            runtime
                .system_manifest_revisions()
                .expect("revisions")
                .into_iter()
                .find(|(id, _)| id == "deepseek")
                .expect("deepseek revision")
                .1,
            active_revision,
            "both invalid projections must leave the original runtime revision visible"
        );
    }

    /// Verifies cancellation before rollback publication restores the exact staged before-image.
    /// 验证回滚发布前取消会恢复精确暂存前镜像。
    #[tokio::test]
    async fn cancelled_rollback_projection_restores_staged_before_image() {
        // directory owns the authenticated two-version rollback store.
        // directory 持有已认证双版本回滚存储。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // store owns the exact Active, Previous, state, and transaction files.
        // store 持有精确 Active、Previous、状态与事务文件。
        let store = ManifestStoreLayout::new(directory.path());
        // previous_revision and active_revision establish a valid rollback pair.
        // previous_revision 与 active_revision 建立有效回滚版本对。
        let previous_revision = bundled_deepseek_revision() + 1;
        let active_revision = previous_revision + 1;
        // previous_text is the authenticated predecessor activated first.
        // previous_text 是先激活的已认证前驱版本。
        let previous_text = deepseek_manifest_with_revision(previous_revision);
        activate_trusted_deepseek(&store, previous_revision, &previous_text);
        // active_text rotates the predecessor into Previous.
        // active_text 把前驱版本轮换到 Previous。
        let active_text = deepseek_manifest_with_revision(active_revision);
        activate_trusted_deepseek(&store, active_revision, &active_text);
        // service owns rollback staging and cancellation compensation.
        // service 持有回滚暂存与取消补偿。
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        // runtime restores the authenticated Active authority before rollback staging.
        // runtime 在回滚暂存前恢复已认证 Active 权威。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        restore_system_manifests_from_store(&runtime, directory.path())
            .expect("active authority should restore");
        // configuration publishes the restored Active staging catalog before rollback begins.
        // configuration 在回滚开始前发布已恢复的 Active 暂存目录。
        let configuration = PersistedAiConfiguration::default();
        runtime
            .activate_staged_manifest_catalogs(&configuration, &[])
            .await
            .expect("active generation should publish");
        // before is the exact durable snapshot cancellation must recover.
        // before 是取消必须恢复的精确持久化快照。
        let before = store
            .capture_system_snapshot("deepseek")
            .expect("before-image should capture");
        // report owns the Applying transaction and staged runtime mutation.
        // report 持有 Applying 事务与暂存运行时变更。
        let report = service
            .rollback_provider_manifest(&runtime, "deepseek")
            .expect("rollback should stage");

        {
            // guard emulates cancellation while waiting for the catalog activation lock.
            // guard 模拟等待目录激活锁时发生取消。
            let _guard = ProviderRollbackCancellationGuard::new(&service, &runtime, &report);
        }

        assert_eq!(
            store
                .capture_system_snapshot("deepseek")
                .expect("restored snapshot should capture"),
            before
        );
        assert!(
            !store
                .recover_system_transaction("deepseek")
                .expect("journal probe should succeed")
        );
        assert_eq!(
            runtime
                .system_manifest("deepseek")
                .expect("restored active manifest should exist")
                .revision,
            active_revision
        );
    }

    /// Verifies runtime publication failure restores Active/state to the authenticated version.
    /// 验证运行时发布失败会把 Active/state 恢复到可信版本。
    #[test]
    fn failed_runtime_publication_rolls_back_store_and_state() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let store = ManifestStoreLayout::new(directory.path());
        let previous_revision = bundled_deepseek_revision() + 1;
        let active_revision = previous_revision + 1;
        let first = deepseek_manifest_with_revision(previous_revision);
        activate_trusted_deepseek(&store, previous_revision, &first);
        let store_snapshot = store
            .capture_system_snapshot("deepseek")
            .expect("former store snapshot must capture");
        let second = deepseek_manifest_with_revision(active_revision);
        activate_trusted_deepseek(&store, active_revision, &second);

        let openrouter_text = std::fs::read_to_string(
            vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                .join("openrouter.json"),
        )
        .expect("bundled openrouter manifest must exist");
        let wrong_manifest = Arc::new(
            vulcan_provider_adapters::compile_manifest_text_with_layer(
                &openrouter_text,
                vulcan_provider_adapters::ManifestOwnership::System,
                vulcan_provider_adapters::ManifestSourceLayer::Active,
            )
            .expect("openrouter manifest must compile"),
        );
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let error = service
            .outcome_to_report(
                &runtime,
                "deepseek",
                active_revision,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                vulcan_provider_config::manifest_sync::SyncOutcome::Activated {
                    revision: active_revision,
                    manifest: wrong_manifest,
                    store_snapshot,
                },
            )
            .expect_err("identity-mismatched runtime publication must fail");
        assert!(error.contains("store rollback: Ok"), "{error}");
        let active = store
            .read_active("deepseek")
            .expect("active read must succeed")
            .expect("active layer must remain");
        let active_revision = serde_json::from_str::<serde_json::Value>(&active)
            .expect("active manifest must parse")["revision"]
            .as_u64();
        assert_eq!(active_revision, Some(previous_revision));
        let state = match store
            .load_sync_state("deepseek")
            .expect("state read must succeed")
        {
            vulcan_provider_config::manifest_store::SyncStateLoad::Present(state) => state,
            vulcan_provider_config::manifest_store::SyncStateLoad::Absent => {
                panic!("authenticated state must remain")
            }
        };
        assert_eq!(state.revision, previous_revision);
        assert!(state.previous.is_none());
    }

    /// Verifies a rollback with no previous layer fails closed without touching the runtime.
    /// 验证无 previous 层的回滚失败关闭且不触碰运行时。
    #[test]
    fn rollback_without_previous_layer_fails_closed() {
        let directory = tempfile::tempdir().expect("temp dir should create");
        let http = build_sync_http_client().expect("sync client");
        let service = ProviderManifestSyncService::new(directory.path(), http, Vec::new(), None);
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let error = service
            .rollback_provider_manifest(&runtime, "deepseek")
            .expect_err("rollback without previous must fail");
        assert!(
            error.contains("no authenticated sync state"),
            "error: {error}"
        );
    }

    /// Verifies 304 and failed checks advance check state without replacing last successful activation.
    /// 验证 304 与失败检查推进检查状态但不替换最近成功激活。
    #[test]
    fn check_state_distinguishes_checked_from_successful_activation() {
        // directory isolates the durable check-state files from the user's application data.
        // directory 把持久化检查状态文件与用户应用数据隔离。
        let directory = tempfile::tempdir().expect("temp dir should create");
        // service owns the exact production check-state transaction implementation.
        // service 持有精确的生产检查状态事务实现。
        let service = ProviderManifestSyncService::new(
            directory.path(),
            build_sync_http_client().expect("sync client"),
            Vec::new(),
            None,
        );
        // activated is recorded only after callers would have completed all projections.
        // activated 仅在调用方已完成全部投影后记录。
        let activated = ProviderSyncReport {
            provider_id: String::from("deepseek"),
            outcome: "activated",
            revision: 2,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: None,
        };
        service
            .record_sync_check_reports(std::slice::from_ref(&activated))
            .expect("activation check should record");
        // firstState proves checked and successful initially share the activation boundary.
        // firstState 证明最近检查与最近成功最初共享激活边界。
        let first_state = match service
            .store
            .load_check_state("deepseek")
            .expect("first check state should load")
        {
            CheckStateLoad::Present(state) => state,
            CheckStateLoad::Absent => panic!("activated check state must exist"),
        };
        assert_eq!(first_state.outcome, ManifestCheckOutcome::Activated);
        assert_eq!(
            first_state.last_successful_at_unix_ms,
            Some(first_state.checked_at_unix_ms)
        );
        // upToDate models a successful conditional 304 observation with no new activation.
        // upToDate 模拟未产生新激活的成功条件 304 观测。
        let up_to_date = ProviderSyncReport {
            provider_id: String::from("deepseek"),
            outcome: "up_to_date",
            revision: 2,
            failure_kind: None,
            failure_message: None,
            activation_snapshot: None,
        };
        service
            .record_sync_check_reports(std::slice::from_ref(&up_to_date))
            .expect("304 check should record");
        // rejected models a later candidate failure with a stable non-secret classification.
        // rejected 模拟后续候选以稳定非秘密分类失败。
        let rejected = ProviderSyncReport {
            provider_id: String::from("deepseek"),
            outcome: "rejected",
            revision: 3,
            failure_kind: Some(String::from("schema_semantic_failed")),
            failure_message: None,
            activation_snapshot: None,
        };
        service
            .record_sync_check_reports(std::slice::from_ref(&rejected))
            .expect("rejected check should record");
        // finalState must advance outcome while retaining the proven activation timestamp.
        // finalState 必须推进结果并保留已证明激活时间戳。
        let final_state = match service
            .store
            .load_check_state("deepseek")
            .expect("final check state should load")
        {
            CheckStateLoad::Present(state) => state,
            CheckStateLoad::Absent => panic!("final check state must exist"),
        };
        assert_eq!(final_state.outcome, ManifestCheckOutcome::Rejected);
        assert_eq!(
            final_state.last_error_kind.as_deref(),
            Some("schema_semantic_failed")
        );
        assert_eq!(
            final_state.last_successful_at_unix_ms,
            first_state.last_successful_at_unix_ms
        );
        assert!(final_state.checked_at_unix_ms >= first_state.checked_at_unix_ms);
    }
}
