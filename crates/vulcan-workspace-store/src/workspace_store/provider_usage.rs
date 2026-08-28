//! Controller-backed provider attempt, token-usage, quota, and rate-limit facts.
//! 由控制器承载的供应商 attempt、Token 用量、额度与限流事实。

use super::decode_query_rows;
use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle, SYSTEM_STATISTICS_SPACE_ID};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};
use vldb_controller_client::ControllerSqliteValue;
use vulcan_ai_contract::UsageBillingDetail;

/// Current native provider-usage schema revision.
/// 当前原生供应商用量 Schema 修订号。
pub const PROVIDER_USAGE_SCHEMA_REVISION: i64 = 7;

/// Process-local serialization lock for attempt insertion followed by daily rebuild.
/// attempt 插入后每日重建使用的进程内串行锁。
static PROVIDER_USAGE_WRITE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

/// Acquires the process-local integrity gate shared by core and trusted plugin statistics writes.
/// 获取由内核与受信插件统计写入共享的进程内一致性门禁。
///
/// # Returns
/// 返回值
/// Returns a guard that serializes multi-step attempt aggregation and plugin maintenance writes.
/// 返回一个将多步调用聚合与插件维护写入串行化的守卫。
pub async fn lock_provider_usage_writes() -> tokio::sync::MutexGuard<'static, ()> {
    PROVIDER_USAGE_WRITE_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// Metadata key proving that native usage facts were copied out of the application database.
/// 证明原生用量事实已经从应用数据库迁出的元数据键。
const APPLICATION_PROVIDER_USAGE_MIGRATION_KEY: &str = "application_database_migration_completed";

/// Maximum rows copied from one legacy application table per Controller batch.
/// 每个 Controller 批次从旧应用表复制的最大行数。
const APPLICATION_PROVIDER_USAGE_MIGRATION_PAGE_SIZE: i64 = 500;

/// Tables copied byte-for-value before daily aggregates are rebuilt in the independent database.
/// 在独立数据库中重建每日聚合前按值复制的表。
const APPLICATION_PROVIDER_USAGE_MIGRATION_TABLES: &[&str] = &[
    "provider_usage_attempts_v3",
    "provider_account_usage_snapshots_v1",
    "provider_rate_limit_snapshots_v1",
    "provider_usage_meta",
];

/// Returns the bootstrap-scoped binding for the independent system-statistics database.
/// 返回独立系统统计数据库的单次引导作用域绑定。
///
/// # Parameters
/// 参数
/// `controller` is the active application Controller handle.
/// `controller` 是当前活动的应用 Controller 句柄。
///
/// # Returns
/// 返回值
/// Returns the exact binding identifier enabled during data-plane bootstrap.
/// 返回数据平面引导期间启用的精确绑定标识符。
fn system_statistics_sqlite_binding_id(controller: &ControllerRuntimeHandle) -> &str {
    controller.system_statistics_sqlite_binding_id.as_str()
}

/// Persists one physical provider attempt independently from a logical request.
/// 独立于逻辑请求持久化一次物理供应商 attempt。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUsageAttemptRecord {
    /// Stable logical request identity shared by retries and failovers.
    /// 由重试与故障转移共享的稳定逻辑请求标识。
    pub logical_request_id: String,
    /// Stable physical attempt identity used for idempotency.
    /// 用于幂等的稳定物理 attempt 标识。
    pub attempt_id: String,
    /// Zero-based retry attempt index.
    /// 从零开始的重试 attempt 索引。
    pub attempt_index: i64,
    /// Zero-based failover index.
    /// 从零开始的故障转移索引。
    pub failover_index: Option<i64>,
    /// Server-bound consumer identity.
    /// 服务端绑定的消费者标识。
    pub consumer_id: String,
    /// Stable operation category such as conversation, compact, or warmup.
    /// 稳定操作类别，例如对话、压缩或预热。
    pub operation: String,
    /// Stable provider identity.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Stable service-entry identity.
    /// 稳定服务入口标识。
    pub service_entry_id: String,
    /// Stable credential connection identity without secret material.
    /// 不含秘密材料的稳定凭据连接标识。
    pub connection_id: String,
    /// Frozen route identity.
    /// 冻结路由标识。
    pub route_id: Option<String>,
    /// Exact protocol profile identity.
    /// 精确协议 Profile 标识。
    pub protocol_profile_id: String,
    /// Frozen route/profile revision.
    /// 冻结路由/Profile 修订号。
    pub route_revision: Option<String>,
    /// Transport used by this attempt.
    /// 当前 attempt 使用的传输。
    pub transport: Option<String>,
    /// Requested public model identity.
    /// 请求的公共模型标识。
    pub requested_model: String,
    /// Model mapped by the resolved route.
    /// 已解析路由映射的模型。
    pub mapped_model: String,
    /// Provider-returned model identity when reported.
    /// 供应商报告时返回的模型标识。
    pub response_model: Option<String>,
    /// Provider-returned response identity when reported.
    /// 供应商报告时返回的响应标识。
    pub response_id: Option<String>,
    /// Provider request identity reported by the transport headers.
    /// 传输响应头报告的供应商请求标识。
    pub provider_request_id: Option<String>,
    /// Dispatch timestamp in Unix milliseconds.
    /// 分派 Unix 毫秒时间戳。
    pub dispatched_at_ms: Option<i64>,
    /// Response-header timestamp in Unix milliseconds.
    /// 响应 Header Unix 毫秒时间戳。
    pub headers_at_ms: Option<i64>,
    /// First semantic output timestamp in Unix milliseconds.
    /// 首个语义输出 Unix 毫秒时间戳。
    pub first_semantic_at_ms: Option<i64>,
    /// Terminal timestamp in Unix milliseconds.
    /// 终态 Unix 毫秒时间戳。
    pub terminal_at_ms: i64,
    /// Physical terminal status.
    /// 物理终态状态。
    pub status: String,
    /// Whether the physical request was dispatched.
    /// 物理请求是否已分派。
    pub dispatched: bool,
    /// Whether the dispatched flag is known rather than a legacy placeholder.
    /// dispatched 标记是否为已知事实而非历史占位值。
    pub dispatched_known: bool,
    /// Whether semantic output was observed.
    /// 是否观察到语义输出。
    pub semantic_output_observed: bool,
    /// Whether semantic-output observation is known rather than a legacy placeholder.
    /// 语义输出观察标记是否为已知事实而非历史占位值。
    pub semantic_output_observed_known: bool,
    /// Whether automatic replay remained safe at terminal time.
    /// 终态时是否仍可安全自动重放。
    pub replay_safe: Option<bool>,
    /// Optional HTTP status.
    /// 可选 HTTP 状态码。
    pub http_status: Option<i64>,
    /// Optional WebSocket close code.
    /// 可选 WebSocket 关闭码。
    pub websocket_close_code: Option<i64>,
    /// Stable error category.
    /// 稳定错误类别。
    pub error_category: Option<String>,
    /// Stable provider error code.
    /// 稳定供应商错误码。
    pub error_code: Option<String>,
    /// Usage availability: missing, partial, or final.
    /// 用量可用性：缺失、部分或终态。
    pub usage_availability: String,
    /// Total input tokens when reported.
    /// 报告时的输入 Token 总量。
    pub input_tokens: Option<i64>,
    /// Uncached input tokens when reported or explicitly derived.
    /// 报告或显式推导时的非缓存输入 Token。
    pub uncached_input_tokens: Option<i64>,
    /// Cache-read input tokens.
    /// 缓存读取输入 Token。
    pub cache_read_tokens: Option<i64>,
    /// Cache-write input tokens.
    /// 缓存写入输入 Token。
    pub cache_write_tokens: Option<i64>,
    /// Output tokens.
    /// 输出 Token。
    pub output_tokens: Option<i64>,
    /// Reasoning output tokens.
    /// 推理输出 Token。
    pub reasoning_tokens: Option<i64>,
    /// Audio input tokens.
    /// 音频输入 Token。
    pub audio_input_tokens: Option<i64>,
    /// Audio output tokens.
    /// 音频输出 Token。
    pub audio_output_tokens: Option<i64>,
    /// Provider-reported total tokens.
    /// 供应商报告的 Token 合计。
    pub total_tokens: Option<i64>,
    /// Frozen usage extraction profile identity.
    /// 冻结用量提取 Profile 标识。
    pub usage_profile_id: Option<String>,
    /// Frozen usage extraction profile revision.
    /// 冻结用量提取 Profile 修订号。
    pub usage_profile_revision: Option<i64>,
    /// JSON object containing exact source paths.
    /// 包含精确来源路径的 JSON 对象。
    pub usage_source_paths_json: Option<String>,
    /// JSON array containing validation flags.
    /// 包含校验标记的 JSON 数组。
    pub usage_validation_flags_json: Option<String>,
    /// JSON array containing typed provider billing partitions.
    /// 包含强类型供应商计费分区的 JSON 数组。
    pub provider_billing_details_json: Option<String>,
    /// Calendar date used by daily aggregation.
    /// 每日聚合使用的日历日期。
    pub calendar_date: String,
}

/// Persists one provider account balance, quota, or subscription snapshot.
/// 持久化一条供应商账户余额、额度或套餐快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAccountUsageSnapshotRecord {
    /// Stable snapshot identity.
    /// 稳定快照标识。
    pub snapshot_id: String,
    /// Provider identity.
    /// 供应商标识。
    pub provider_id: String,
    /// Credential connection identity.
    /// 凭据连接标识。
    pub connection_id: String,
    /// Snapshot category such as balance, quota, or token_plan.
    /// 快照类别，例如 balance、quota 或 token_plan。
    pub snapshot_kind: String,
    /// Exact endpoint profile revision.
    /// 精确端点 Profile 修订号。
    pub endpoint_profile_revision: String,
    /// Bounded normalized snapshot JSON.
    /// 有界归一化快照 JSON。
    pub payload_json: String,
    /// Observation timestamp in Unix milliseconds.
    /// 观测 Unix 毫秒时间戳。
    pub observed_at_ms: i64,
}

/// Persists one allowlisted rate-limit header snapshot.
/// 持久化一条允许列表内的限流 Header 快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRateLimitSnapshotRecord {
    /// Stable snapshot identity.
    /// 稳定快照标识。
    pub snapshot_id: String,
    /// Owning physical attempt identity.
    /// 所属物理 attempt 标识。
    pub attempt_id: String,
    /// Bounded normalized allowlisted header JSON.
    /// 有界归一化允许列表 Header JSON。
    pub headers_json: String,
    /// Optional retry delay in milliseconds.
    /// 可选重试延迟毫秒数。
    pub retry_after_ms: Option<i64>,
    /// Observation timestamp in Unix milliseconds.
    /// 观测 Unix 毫秒时间戳。
    pub observed_at_ms: i64,
}

/// Carries one native daily provider-usage row for the settings dashboard.
/// 承载设置仪表盘使用的一条原生供应商每日用量行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageDailySummary {
    /// Host-local calendar date.
    /// 宿主本地自然日。
    pub calendar_date: String,
    /// Stable provider identity.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Stable service entry identity.
    /// 稳定服务入口标识。
    pub service_entry_id: String,
    /// Server-bound consumer identity.
    /// 服务端绑定消费者标识。
    pub consumer_id: String,
    /// Stable operation category.
    /// 稳定操作类别。
    pub operation: String,
    /// Exact mapped model identity.
    /// 精确映射模型标识。
    pub model_id: String,
    /// Physical attempt count.
    /// 真实 attempt 数量。
    pub attempt_count: i64,
    /// Successful physical attempt count.
    /// 成功真实 attempt 数量。
    pub successful_attempt_count: i64,
    /// Attempts containing provider-reported usage.
    /// 包含供应商报告用量的 attempt 数量。
    pub reported_usage_attempt_count: i64,
    /// Known normalized token totals.
    /// 已知规范化 Token 合计。
    pub input_tokens: Option<i64>,
    /// Known uncached input-token total.
    /// 已知非缓存输入 Token 合计。
    pub uncached_input_tokens: Option<i64>,
    /// Known cache-read token total.
    /// 已知缓存读取 Token 合计。
    pub cache_read_tokens: Option<i64>,
    /// Known cache-write token total.
    /// 已知缓存写入 Token 合计。
    pub cache_write_tokens: Option<i64>,
    /// Known output-token total.
    /// 已知输出 Token 合计。
    pub output_tokens: Option<i64>,
    /// Known reasoning-token total.
    /// 已知推理 Token 合计。
    pub reasoning_tokens: Option<i64>,
    /// Known complete token total.
    /// 已知完整 Token 合计。
    pub total_tokens: Option<i64>,
    /// Monotonic native aggregate revision.
    /// 单调递增原生聚合修订号。
    pub aggregate_revision: i64,
}

/// Carries one recent physical attempt with exact token-usage provenance.
/// 承载一条带有精确 Token 用量来源的近期物理 attempt。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageAttemptSummary {
    /// Stable physical attempt identity.
    /// 稳定物理 attempt 标识。
    pub attempt_id: String,
    /// Server-bound consumer identity.
    /// 服务端绑定的消费者标识。
    pub consumer_id: String,
    /// Stable operation category.
    /// 稳定操作类别。
    pub operation: String,
    /// Provider and service-entry identities.
    /// 供应商与服务入口标识。
    pub provider_id: String,
    /// Stable service-entry identity.
    /// 稳定服务入口标识。
    pub service_entry_id: String,
    /// Exact mapped model identity.
    /// 精确映射模型标识。
    pub model_id: String,
    /// Physical HTTP/SSE or Responses WebSocket transport when observed.
    /// 已观测到的物理 HTTP/SSE 或 Responses WebSocket 传输。
    pub transport: Option<String>,
    /// Optional HTTP status observed at termination.
    /// 终止时观测到的可选 HTTP 状态。
    pub http_status: Option<i64>,
    /// Optional WebSocket close code observed at termination.
    /// 终止时观测到的可选 WebSocket 关闭码。
    pub websocket_close_code: Option<i64>,
    /// Stable provider error category when the attempt failed.
    /// attempt 失败时的稳定供应商错误类别。
    pub error_category: Option<String>,
    /// Stable provider error code when the attempt failed.
    /// attempt 失败时的稳定供应商错误代码。
    pub error_code: Option<String>,
    /// Physical terminal status.
    /// 物理终态状态。
    pub status: String,
    /// Usage availability: missing, partial, or final.
    /// 用量可用性：缺失、部分或终态。
    pub usage_availability: String,
    /// Provider-reported complete token total when known.
    /// 已知时由供应商报告的完整 Token 合计。
    pub total_tokens: Option<i64>,
    /// Total input tokens when known.
    /// 已知时的输入 Token 总量。
    pub input_tokens: Option<i64>,
    /// Uncached input tokens when known.
    /// 已知时的非缓存输入 Token。
    pub uncached_input_tokens: Option<i64>,
    /// Cache-read input tokens when known.
    /// 已知时的缓存读取输入 Token。
    pub cache_read_tokens: Option<i64>,
    /// Cache-write input tokens when known.
    /// 已知时的缓存写入输入 Token。
    pub cache_write_tokens: Option<i64>,
    /// Output tokens when known.
    /// 已知时的输出 Token。
    pub output_tokens: Option<i64>,
    /// Reasoning output tokens when known.
    /// 已知时的推理输出 Token。
    pub reasoning_tokens: Option<i64>,
    /// Audio input tokens when known.
    /// 已知时的音频输入 Token。
    pub audio_input_tokens: Option<i64>,
    /// Audio output tokens when known.
    /// 已知时的音频输出 Token。
    pub audio_output_tokens: Option<i64>,
    /// Typed provider-specific billing partitions.
    /// 强类型供应商专属计费分区。
    pub billing_details: Vec<UsageBillingDetail>,
    /// Terminal timestamp in Unix milliseconds.
    /// 终态 Unix 毫秒时间戳。
    pub terminal_at_ms: i64,
}

/// Carries one latest native account-usage snapshot for the settings dashboard.
/// 承载设置仪表盘使用的一条最新原生账户用量快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountUsageSummary {
    /// Stable provider and credential identities.
    /// 稳定供应商与凭据身份。
    pub provider_id: String,
    /// Stable credential connection identity.
    /// 稳定凭据连接标识。
    pub connection_id: String,
    /// Snapshot category and endpoint revision.
    /// 快照类别与端点修订号。
    pub snapshot_kind: String,
    /// Exact content-addressed endpoint profile revision.
    /// 精确内容寻址端点 Profile 修订号。
    pub endpoint_profile_revision: String,
    /// Normalized account snapshot payload.
    /// 规范化账户快照载荷。
    pub payload: serde_json::Value,
    /// Observation timestamp in Unix milliseconds.
    /// 观测 Unix 毫秒时间戳。
    pub observed_at_ms: i64,
}

/// Carries native provider token usage and account facts for user inspection.
/// 承载供用户查看的原生供应商 Token 用量与账户事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageDashboard {
    /// Dashboard generation timestamp.
    /// 仪表盘生成时间戳。
    pub generated_at_ms: i64,
    /// Native schema and aggregate revisions.
    /// 原生 Schema 与聚合修订号。
    pub schema_revision: i64,
    /// Monotonic native aggregate revision.
    /// 单调递增原生聚合修订号。
    pub aggregate_revision: i64,
    /// Native storage health state.
    /// 原生存储健康状态。
    pub health_status: String,
    /// Recent daily rows in reverse chronological order.
    /// 按时间倒序排列的近期每日行。
    pub daily: Vec<ProviderUsageDailySummary>,
    /// Recent physical attempts with exact token partitions.
    /// 带有精确 Token 分区的近期物理 attempt。
    pub recent_attempts: Vec<ProviderUsageAttemptSummary>,
    /// Latest account snapshot for each connection and category.
    /// 每个连接与类别的最新账户快照。
    pub accounts: Vec<ProviderAccountUsageSummary>,
}

/// Ensures all native provider usage tables exist in the system-statistics binding.
/// 确保系统统计绑定中存在全部原生供应商用量表。
pub async fn ensure_provider_usage_schema(
    controller: &ControllerRuntimeHandle,
) -> Result<(), String> {
    ensure_provider_usage_schema_at(
        controller,
        SYSTEM_STATISTICS_SPACE_ID,
        system_statistics_sqlite_binding_id(controller),
    )
    .await
}

/// Ensures the Provider usage schema at one exact host-owned Controller binding.
/// 在一个精确的宿主持有 Controller 绑定上确保 Provider 用量 Schema。
///
/// # Parameters
/// 参数
/// `controller` supplies the connected Controller client, while `space_id` and `binding_id`
/// identify the exact database initialized by the host.
/// `controller` 提供已连接的 Controller 客户端，`space_id` 与 `binding_id` 标识宿主初始化的精确数据库。
///
/// # Returns
/// 返回值
/// Returns unit after schema creation and additive migrations complete.
/// Schema 创建和增量迁移完成后返回空值。
async fn ensure_provider_usage_schema_at(
    controller: &ControllerRuntimeHandle,
    space_id: &str,
    binding_id: &str,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_script_typed(
            space_id,
            binding_id,
            r#"
CREATE TABLE IF NOT EXISTS provider_usage_attempts_v3 (
    attempt_id TEXT PRIMARY KEY,
    logical_request_id TEXT NOT NULL,
    attempt_index INTEGER NOT NULL,
    failover_index INTEGER,
    consumer_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    service_entry_id TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    route_id TEXT,
    protocol_profile_id TEXT NOT NULL,
    route_revision TEXT,
    transport TEXT,
    requested_model TEXT NOT NULL,
    mapped_model TEXT NOT NULL,
    response_model TEXT,
    response_id TEXT,
    provider_request_id TEXT,
    dispatched_at_ms INTEGER,
    headers_at_ms INTEGER,
    first_semantic_at_ms INTEGER,
    terminal_at_ms INTEGER NOT NULL,
    status TEXT NOT NULL,
    dispatched INTEGER NOT NULL,
    dispatched_known INTEGER NOT NULL DEFAULT 1,
    semantic_output_observed INTEGER NOT NULL,
    semantic_output_observed_known INTEGER NOT NULL DEFAULT 1,
    replay_safe INTEGER,
    http_status INTEGER,
    websocket_close_code INTEGER,
    error_category TEXT,
    error_code TEXT,
    usage_availability TEXT NOT NULL,
    input_tokens INTEGER,
    uncached_input_tokens INTEGER,
    cache_read_tokens INTEGER,
    cache_write_tokens INTEGER,
    output_tokens INTEGER,
    reasoning_tokens INTEGER,
    audio_input_tokens INTEGER,
    audio_output_tokens INTEGER,
    total_tokens INTEGER,
    usage_profile_id TEXT,
    usage_profile_revision INTEGER,
    usage_source_paths_json TEXT,
    usage_validation_flags_json TEXT,
    provider_billing_details_json TEXT,
    calendar_date TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_provider_usage_attempts_v3_logical
    ON provider_usage_attempts_v3(logical_request_id, attempt_index, failover_index);
CREATE INDEX IF NOT EXISTS idx_provider_usage_attempts_v3_date
    ON provider_usage_attempts_v3(calendar_date, provider_id, consumer_id, operation);
CREATE INDEX IF NOT EXISTS idx_provider_usage_attempts_v3_connection
    ON provider_usage_attempts_v3(connection_id, terminal_at_ms);
CREATE INDEX IF NOT EXISTS idx_provider_usage_attempts_v3_terminal
    ON provider_usage_attempts_v3(terminal_at_ms DESC, attempt_id DESC);

CREATE TABLE IF NOT EXISTS provider_usage_daily_v3 (
    calendar_date TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    service_entry_id TEXT NOT NULL,
    consumer_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    model_id TEXT NOT NULL,
    attempt_count INTEGER NOT NULL,
    successful_attempt_count INTEGER NOT NULL,
    reported_usage_attempt_count INTEGER NOT NULL,
    input_tokens INTEGER,
    uncached_input_tokens INTEGER,
    cache_read_tokens INTEGER,
    cache_write_tokens INTEGER,
    output_tokens INTEGER,
    reasoning_tokens INTEGER,
    total_tokens INTEGER,
    aggregate_revision INTEGER NOT NULL,
    PRIMARY KEY (
        calendar_date, provider_id, service_entry_id, consumer_id,
        operation, model_id
    )
);

CREATE TABLE IF NOT EXISTS provider_account_usage_snapshots_v1 (
    snapshot_id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    snapshot_kind TEXT NOT NULL,
    endpoint_profile_revision TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_provider_account_usage_latest
    ON provider_account_usage_snapshots_v1(connection_id, snapshot_kind, observed_at_ms);

CREATE TABLE IF NOT EXISTS provider_rate_limit_snapshots_v1 (
    snapshot_id TEXT PRIMARY KEY,
    attempt_id TEXT NOT NULL,
    headers_json TEXT NOT NULL,
    retry_after_ms INTEGER,
    observed_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_provider_rate_limit_attempt
    ON provider_rate_limit_snapshots_v1(attempt_id, observed_at_ms);

CREATE TABLE IF NOT EXISTS provider_usage_meta (
    meta_key TEXT PRIMARY KEY,
    meta_value TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
INSERT OR IGNORE INTO provider_usage_meta(meta_key, meta_value, updated_at_ms)
VALUES ('schema_revision', '0', 0);
INSERT OR IGNORE INTO provider_usage_meta(meta_key, meta_value, updated_at_ms)
VALUES ('aggregate_revision', '0', 0);
INSERT OR IGNORE INTO provider_usage_meta(meta_key, meta_value, updated_at_ms)
VALUES ('health_status', 'ready', 0);
INSERT OR IGNORE INTO provider_usage_meta(meta_key, meta_value, updated_at_ms)
VALUES ('application_database_migration_completed', '0', 0);
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("初始化原生供应商用量 Schema 失败：{error}"))?;
    migrate_provider_usage_v2_to_v3(controller, space_id, binding_id).await?;
    ensure_provider_usage_attempt_columns(controller, space_id, binding_id).await?;
    Ok(())
}

/// Migrates legacy price-bearing usage tables into the token-only v3 schema and removes them.
/// 将旧版含价格用量表迁移到仅含 Token 的 v3 Schema，并删除旧表。
///
/// # Parameters
/// 参数
/// `controller`, `space_id`, and `binding_id` identify one exact initialized SQLite binding.
/// `controller`、`space_id` 与 `binding_id` 标识一个精确且已初始化的 SQLite Binding。
///
/// # Returns
/// 返回值
/// Returns after non-price facts are copied transactionally and every price-bearing table is gone.
/// 在非价格事实完成事务复制且全部含价格表被删除后返回。
async fn migrate_provider_usage_v2_to_v3(
    controller: &ControllerRuntimeHandle,
    space_id: &str,
    binding_id: &str,
) -> Result<(), String> {
    let table_result = controller
        .client
        .query_sqlite_json_typed(
            space_id,
            binding_id,
            "SELECT COUNT(*) AS table_count FROM sqlite_master WHERE type = 'table' AND name = 'provider_usage_attempts_v2';",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("检查旧版供应商用量表失败：{error}"))?;
    let source_exists = decode_query_rows::<ProviderUsageTableCountRow>(&table_result.json_data)?
        .into_iter()
        .next()
        .is_some_and(|row| row.table_count == 1);
    if !source_exists {
        controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "DROP TABLE IF EXISTS provider_usage_daily_v2; DROP TABLE IF EXISTS provider_pricing_profiles_v1;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("清理旧版供应商价格表失败：{error}"))?;
        return Ok(());
    }

    let column_result = controller
        .client
        .query_sqlite_json_typed(
            space_id,
            binding_id,
            "PRAGMA table_info(provider_usage_attempts_v2);",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("读取旧版供应商用量列失败：{error}"))?;
    let columns = decode_query_rows::<ProviderUsageColumnRow>(&column_result.json_data)?
        .into_iter()
        .map(|column| column.name)
        .collect::<BTreeSet<_>>();
    let required_columns = [
        "attempt_id",
        "logical_request_id",
        "attempt_index",
        "consumer_id",
        "operation",
        "provider_id",
        "service_entry_id",
        "connection_id",
        "protocol_profile_id",
        "requested_model",
        "mapped_model",
        "terminal_at_ms",
        "status",
        "dispatched",
        "semantic_output_observed",
        "usage_availability",
        "calendar_date",
    ];
    for required_column in required_columns {
        if !columns.contains(required_column) {
            return Err(format!(
                "旧版供应商用量表缺少必需列 `{required_column}`，拒绝猜测迁移"
            ));
        }
    }
    let optional_column = |name: &str, fallback: &str| {
        if columns.contains(name) {
            name.to_string()
        } else {
            fallback.to_string()
        }
    };
    let migration = format!(
        r#"
BEGIN IMMEDIATE;
INSERT OR IGNORE INTO provider_usage_attempts_v3 (
    attempt_id, logical_request_id, attempt_index, failover_index,
    consumer_id, operation, provider_id, service_entry_id, connection_id,
    route_id, protocol_profile_id, route_revision, transport,
    requested_model, mapped_model, response_model, response_id, provider_request_id,
    dispatched_at_ms, headers_at_ms, first_semantic_at_ms, terminal_at_ms,
    status, dispatched, dispatched_known, semantic_output_observed,
    semantic_output_observed_known, replay_safe, http_status, websocket_close_code,
    error_category, error_code, usage_availability, input_tokens,
    uncached_input_tokens, cache_read_tokens, cache_write_tokens, output_tokens,
    reasoning_tokens, audio_input_tokens, audio_output_tokens, total_tokens,
    usage_profile_id, usage_profile_revision, usage_source_paths_json,
    usage_validation_flags_json, provider_billing_details_json, calendar_date
)
SELECT
    attempt_id, logical_request_id, attempt_index, failover_index,
    consumer_id, operation, provider_id, service_entry_id, connection_id,
    route_id, protocol_profile_id, route_revision, transport,
    requested_model, mapped_model, response_model, response_id, {provider_request_id},
    dispatched_at_ms, headers_at_ms, first_semantic_at_ms, terminal_at_ms,
    status, dispatched, {dispatched_known}, semantic_output_observed,
    {semantic_output_observed_known}, replay_safe, http_status, websocket_close_code,
    error_category, error_code, usage_availability, input_tokens,
    uncached_input_tokens, cache_read_tokens, cache_write_tokens, output_tokens,
    reasoning_tokens, audio_input_tokens, audio_output_tokens, total_tokens,
    usage_profile_id, usage_profile_revision, usage_source_paths_json,
    usage_validation_flags_json, {provider_billing_details_json}, calendar_date
FROM provider_usage_attempts_v2;
DROP TABLE provider_usage_attempts_v2;
DROP TABLE IF EXISTS provider_usage_daily_v2;
DROP TABLE IF EXISTS provider_pricing_profiles_v1;
COMMIT;
"#,
        provider_request_id = optional_column("provider_request_id", "NULL"),
        dispatched_known = optional_column("dispatched_known", "1"),
        semantic_output_observed_known = optional_column("semantic_output_observed_known", "1"),
        provider_billing_details_json = optional_column("provider_billing_details_json", "NULL"),
    );
    controller
        .client
        .execute_sqlite_script_typed(space_id, binding_id, migration.as_str(), Vec::new())
        .await
        .map_err(|error| format!("迁移 Token-only 供应商用量表失败：{error}"))?;
    Ok(())
}

/// Decodes one SQLite table-existence count.
/// 解码一个 SQLite 表存在性计数。
#[derive(Debug, Deserialize)]
struct ProviderUsageTableCountRow {
    /// Number of tables matching the exact queried name.
    /// 与精确查询名称匹配的表数量。
    table_count: i64,
}

/// Applies additive attempt-table migrations after inspecting the live Controller binding.
/// 检查实时控制器绑定后应用 attempt 表的增量迁移。
async fn ensure_provider_usage_attempt_columns(
    controller: &ControllerRuntimeHandle,
    space_id: &str,
    binding_id: &str,
) -> Result<(), String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            space_id,
            binding_id,
            "PRAGMA table_info(provider_usage_attempts_v3);",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("读取原生供应商用量列失败：{error}"))?;
    let columns = decode_query_rows::<ProviderUsageColumnRow>(&result.json_data)?;
    if !columns
        .iter()
        .any(|column| column.name == "provider_request_id")
    {
        controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "ALTER TABLE provider_usage_attempts_v3 ADD COLUMN provider_request_id TEXT;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("迁移供应商请求标识列失败：{error}"))?;
    }
    if !columns
        .iter()
        .any(|column| column.name == "dispatched_known")
    {
        controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "ALTER TABLE provider_usage_attempts_v3 ADD COLUMN dispatched_known INTEGER NOT NULL DEFAULT 1;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("迁移供应商分派事实状态列失败：{error}"))?;
    }
    if !columns
        .iter()
        .any(|column| column.name == "semantic_output_observed_known")
    {
        controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "ALTER TABLE provider_usage_attempts_v3 ADD COLUMN semantic_output_observed_known INTEGER NOT NULL DEFAULT 1;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("迁移供应商语义输出事实状态列失败：{error}"))?;
    }
    if !columns
        .iter()
        .any(|column| column.name == "provider_billing_details_json")
    {
        controller
            .client
            .execute_sqlite_script_typed(
                space_id,
                binding_id,
                "ALTER TABLE provider_usage_attempts_v3 ADD COLUMN provider_billing_details_json TEXT;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("迁移供应商计费明细列失败：{error}"))?;
    }
    // revision_update derives the persisted schema value from the public constant to prevent drift.
    // revision_update 从公开常量生成持久化 Schema 值，防止两处版本漂移。
    let revision_update = format!(
        "UPDATE provider_usage_meta SET meta_value = '{}' WHERE meta_key = 'schema_revision';",
        PROVIDER_USAGE_SCHEMA_REVISION
    );
    controller
        .client
        .execute_sqlite_script_typed(space_id, binding_id, revision_update.as_str(), Vec::new())
        .await
        .map_err(|error| format!("更新供应商用量 Schema 修订失败：{error}"))?;
    Ok(())
}

/// Decodes one SQLite table-info row used by additive schema migrations.
/// 解码增量 Schema 迁移使用的一行 SQLite 表信息。
#[derive(Debug, Deserialize)]
struct ProviderUsageColumnRow {
    /// Exact column name returned by SQLite.
    /// SQLite 返回的精确列名。
    name: String,
}

/// Migrates native Provider usage facts from the historical application database exactly once.
/// 将原生 Provider 用量事实从历史应用数据库精确迁移一次。
///
/// # Parameters
/// 参数
/// `controller` owns both the historical application binding and the new system-statistics binding.
/// `controller` 同时持有历史应用绑定和新的系统统计绑定。
///
/// # Returns
/// 返回值
/// Returns unit after all durable facts are copied, aggregates are rebuilt, and the cutover marker is saved.
/// 所有持久事实复制、聚合重建并保存切换标记后返回空值。
pub async fn migrate_application_provider_usage(
    controller: &ControllerRuntimeHandle,
) -> Result<(), String> {
    ensure_provider_usage_schema(controller).await?;
    if application_provider_usage_migration_completed(controller).await? {
        return Ok(());
    }
    let source_exists = application_provider_usage_source_exists(controller).await?;
    if source_exists {
        // source_schema is upgraded in place before copying so every declared current column has deterministic semantics.
        // source_schema 在复制前原地升级，使每个当前声明列都具有确定语义。
        ensure_provider_usage_schema_at(
            controller,
            APP_SPACE_ID,
            controller.sqlite_binding_id.as_str(),
        )
        .await?;
        for table in APPLICATION_PROVIDER_USAGE_MIGRATION_TABLES {
            migrate_application_provider_usage_table(controller, table).await?;
        }
        // schema_revision is host-owned and must not remain at an older source value after metadata copying.
        // schema_revision 由宿主持有，复制元数据后不得停留在旧来源值。
        ensure_provider_usage_schema(controller).await?;
        rebuild_provider_usage_daily(controller, current_provider_usage_unix_millis()).await?;
    }
    persist_application_provider_usage_migration_completed(controller).await
}

/// Reads the durable cutover marker from the independent system-statistics database.
/// 从独立系统统计数据库读取持久切换标记。
async fn application_provider_usage_migration_completed(
    controller: &ControllerRuntimeHandle,
) -> Result<bool, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            "SELECT CAST(meta_value AS INTEGER) AS completed FROM provider_usage_meta WHERE meta_key = ?;",
            vec![ControllerSqliteValue::String(String::from(
                APPLICATION_PROVIDER_USAGE_MIGRATION_KEY,
            ))],
        )
        .await
        .map_err(|error| format!("读取应用统计数据库迁移状态失败：{error}"))?;
    Ok(
        decode_query_rows::<ProviderUsageMigrationCompletedRow>(&result.json_data)?
            .into_iter()
            .next()
            .map(|row| row.completed == 1)
            .unwrap_or(false),
    )
}

/// Reports whether the historical application database contains native Provider usage tables.
/// 报告历史应用数据库是否包含原生 Provider 用量表。
async fn application_provider_usage_source_exists(
    controller: &ControllerRuntimeHandle,
) -> Result<bool, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            controller.sqlite_binding_id.as_str(),
            "SELECT COUNT(*) AS completed FROM sqlite_master WHERE type = 'table' AND name IN ('provider_usage_attempts_v2', 'provider_usage_attempts_v3');",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("检查应用统计数据库迁移来源失败：{error}"))?;
    Ok(
        decode_query_rows::<ProviderUsageMigrationCompletedRow>(&result.json_data)?
            .into_iter()
            .next()
            .map(|row| row.completed > 0)
            .unwrap_or(false),
    )
}

/// Copies one allowlisted Provider usage table by stable SQLite rowid pages.
/// 按稳定 SQLite rowid 分页复制一张允许列表中的 Provider 用量表。
async fn migrate_application_provider_usage_table(
    controller: &ControllerRuntimeHandle,
    table: &str,
) -> Result<(), String> {
    if !APPLICATION_PROVIDER_USAGE_MIGRATION_TABLES.contains(&table) {
        return Err(format!("拒绝迁移未声明的 Provider 用量表 `{table}`。"));
    }
    let source_columns = load_provider_usage_table_columns(
        controller,
        APP_SPACE_ID,
        controller.sqlite_binding_id.as_str(),
        table,
    )
    .await?;
    let target_columns = load_provider_usage_table_columns(
        controller,
        SYSTEM_STATISTICS_SPACE_ID,
        system_statistics_sqlite_binding_id(controller),
        table,
    )
    .await?;
    let source_column_names = source_columns
        .into_iter()
        .map(|column| column.name)
        .collect::<BTreeSet<_>>();
    let columns = target_columns
        .into_iter()
        .map(|column| column.name)
        .filter(|column| source_column_names.contains(column))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Err(format!("Provider 用量表 `{table}` 没有可迁移列。"));
    }
    let quoted_table = quote_sqlite_identifier(table)?;
    let quoted_columns = columns
        .iter()
        .map(|column| quote_sqlite_identifier(column))
        .collect::<Result<Vec<_>, _>>()?;
    let select_sql = format!(
        "SELECT rowid AS __source_rowid, {} FROM {} WHERE rowid > ? ORDER BY rowid LIMIT ?;",
        quoted_columns.join(", "),
        quoted_table
    );
    let placeholders = std::iter::repeat_n("?", columns.len())
        .collect::<Vec<_>>()
        .join(", ");
    let insert_sql = if table == "provider_usage_meta" {
        format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT(meta_key) DO UPDATE SET meta_value = excluded.meta_value, updated_at_ms = excluded.updated_at_ms;",
            quoted_table,
            quoted_columns.join(", "),
            placeholders
        )
    } else {
        format!(
            "INSERT OR IGNORE INTO {} ({}) VALUES ({});",
            quoted_table,
            quoted_columns.join(", "),
            placeholders
        )
    };
    let mut source_rowid = 0_i64;
    loop {
        let result = controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                controller.sqlite_binding_id.as_str(),
                select_sql.as_str(),
                vec![
                    ControllerSqliteValue::Int64(source_rowid),
                    ControllerSqliteValue::Int64(APPLICATION_PROVIDER_USAGE_MIGRATION_PAGE_SIZE),
                ],
            )
            .await
            .map_err(|error| format!("读取应用 Provider 用量表 `{table}` 失败：{error}"))?;
        let rows = serde_json::from_str::<Vec<Map<String, Value>>>(&result.json_data)
            .map_err(|error| format!("解析应用 Provider 用量表 `{table}` 分页失败：{error}"))?;
        if rows.is_empty() {
            break;
        }
        let mut parameter_sets = Vec::with_capacity(rows.len());
        for row in &rows {
            let rowid = row
                .get("__source_rowid")
                .and_then(Value::as_i64)
                .ok_or_else(|| format!("应用 Provider 用量表 `{table}` 缺少有效 rowid。"))?;
            source_rowid = rowid;
            parameter_sets.push(
                columns
                    .iter()
                    .map(|column| {
                        row.get(column).ok_or_else(|| {
                            format!("应用 Provider 用量表 `{table}` 缺少列 `{column}`。")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(json_value_to_controller_sqlite_value)
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        controller
            .client
            .execute_sqlite_batch_typed(
                SYSTEM_STATISTICS_SPACE_ID,
                system_statistics_sqlite_binding_id(controller),
                insert_sql.as_str(),
                parameter_sets,
            )
            .await
            .map_err(|error| format!("写入独立 Provider 用量表 `{table}` 失败：{error}"))?;
    }
    Ok(())
}

/// Loads ordered column metadata for one allowlisted migration table.
/// 加载一张允许列表迁移表的有序列元数据。
async fn load_provider_usage_table_columns(
    controller: &ControllerRuntimeHandle,
    space_id: &str,
    binding_id: &str,
    table: &str,
) -> Result<Vec<ProviderUsageColumnRow>, String> {
    let quoted_table = quote_sqlite_identifier(table)?;
    let result = controller
        .client
        .query_sqlite_json_typed(
            space_id,
            binding_id,
            format!("PRAGMA table_info({quoted_table});"),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("读取 Provider 用量表 `{table}` 列失败：{error}"))?;
    decode_query_rows::<ProviderUsageColumnRow>(&result.json_data)
}

/// Persists the completed native application-database migration boundary.
/// 持久化原生应用数据库迁移完成边界。
async fn persist_application_provider_usage_migration_completed(
    controller: &ControllerRuntimeHandle,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_script_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            "UPDATE provider_usage_meta SET meta_value = '1', updated_at_ms = ? WHERE meta_key = ?;",
            vec![
            ControllerSqliteValue::Int64(current_provider_usage_unix_millis()),
                ControllerSqliteValue::String(String::from(
                    APPLICATION_PROVIDER_USAGE_MIGRATION_KEY,
                )),
            ],
        )
        .await
        .map_err(|error| format!("保存应用统计数据库迁移状态失败：{error}"))?;
    Ok(())
}

/// Converts one Controller JSON row scalar back into the typed SQLite parameter protocol.
/// 将一个 Controller JSON 行标量转换回类型化 SQLite 参数协议。
fn json_value_to_controller_sqlite_value(value: &Value) -> Result<ControllerSqliteValue, String> {
    match value {
        Value::Null => Ok(ControllerSqliteValue::Null),
        Value::Bool(value) => Ok(ControllerSqliteValue::Bool(*value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(ControllerSqliteValue::Int64(value))
            } else if let Some(value) = value.as_f64() {
                Ok(ControllerSqliteValue::Float64(value))
            } else {
                Err(String::from("SQLite JSON 数值超出受支持范围。"))
            }
        }
        Value::String(value) => Ok(ControllerSqliteValue::String(value.clone())),
        Value::Array(_) | Value::Object(_) => {
            Err(String::from("SQLite 行包含不受支持的复合 JSON 值。"))
        }
    }
}

/// Quotes one verified portable SQLite identifier.
/// 引用一个经过验证的可移植 SQLite 标识符。
fn quote_sqlite_identifier(identifier: &str) -> Result<String, String> {
    if identifier.is_empty()
        || !identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!("SQLite 标识符 `{identifier}` 无效。"));
    }
    Ok(format!("\"{identifier}\""))
}

/// One persisted legacy-migration completion row.
/// 一条持久化的旧迁移完成状态行。
#[derive(Debug, Deserialize)]
struct ProviderUsageMigrationCompletedRow {
    /// Exact zero-or-one completion flag.
    /// 精确的零或一完成标记。
    completed: i64,
}

/// Loads the bounded native provider usage dashboard from the application VLDB binding.
/// 从应用 VLDB 绑定加载有界原生供应商用量仪表盘。
pub async fn load_provider_usage_dashboard(
    controller: &ControllerRuntimeHandle,
) -> Result<ProviderUsageDashboard, String> {
    ensure_provider_usage_schema(controller).await?;
    let daily_result = controller
        .client
        .query_sqlite_json_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            r#"
SELECT calendar_date AS calendarDate, provider_id AS providerId,
       service_entry_id AS serviceEntryId, consumer_id AS consumerId, operation,
       model_id AS modelId, attempt_count AS attemptCount,
       successful_attempt_count AS successfulAttemptCount,
       reported_usage_attempt_count AS reportedUsageAttemptCount,
       input_tokens AS inputTokens, uncached_input_tokens AS uncachedInputTokens,
       cache_read_tokens AS cacheReadTokens, cache_write_tokens AS cacheWriteTokens,
       output_tokens AS outputTokens, reasoning_tokens AS reasoningTokens,
       total_tokens AS totalTokens, aggregate_revision AS aggregateRevision
 FROM provider_usage_daily_v3
 ORDER BY calendar_date DESC, provider_id, service_entry_id, consumer_id, operation, model_id
LIMIT 500;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("读取原生供应商每日用量失败：{error}"))?;
    let daily = decode_query_rows::<ProviderUsageDailySummary>(&daily_result.json_data)?;
    let recent_attempts_result = controller
        .client
        .query_sqlite_json_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            r#"
SELECT attempts.attempt_id, attempts.consumer_id, attempts.operation,
       attempts.provider_id, attempts.service_entry_id,
       attempts.mapped_model AS model_id, attempts.transport,
       attempts.http_status, attempts.websocket_close_code,
       attempts.error_category, attempts.error_code, attempts.status,
       attempts.usage_availability, attempts.input_tokens,
       attempts.uncached_input_tokens, attempts.cache_read_tokens,
       attempts.cache_write_tokens, attempts.output_tokens,
       attempts.reasoning_tokens, attempts.audio_input_tokens,
       attempts.audio_output_tokens, attempts.total_tokens,
       attempts.provider_billing_details_json, attempts.terminal_at_ms
 FROM provider_usage_attempts_v3 AS attempts
ORDER BY attempts.terminal_at_ms DESC, attempts.attempt_id DESC
LIMIT 200;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("读取原生供应商近期 attempt 失败：{error}"))?;
    let recent_attempt_rows =
        decode_query_rows::<ProviderUsageAttemptDashboardRow>(&recent_attempts_result.json_data)?;
    let recent_attempts = recent_attempt_rows
        .into_iter()
        .map(ProviderUsageAttemptDashboardRow::decode)
        .collect::<Result<Vec<_>, _>>()?;
    let meta_result = controller
        .client
        .query_sqlite_json_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            r#"
SELECT
    COALESCE(MAX(CASE WHEN meta_key = 'aggregate_revision' THEN CAST(meta_value AS INTEGER) END), 0) AS aggregate_revision,
    COALESCE(MAX(CASE WHEN meta_key = 'health_status' THEN meta_value END), 'unknown') AS health_status
FROM provider_usage_meta;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("读取原生供应商用量元数据失败：{error}"))?;
    let meta = decode_query_rows::<ProviderUsageDashboardMetaRow>(&meta_result.json_data)?
        .into_iter()
        .next()
        .unwrap_or(ProviderUsageDashboardMetaRow {
            aggregate_revision: 0,
            health_status: String::from("unknown"),
        });
    let accounts_result = controller
        .client
        .query_sqlite_json_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            r#"
SELECT provider_id, connection_id, snapshot_kind, endpoint_profile_revision,
       payload_json, observed_at_ms
FROM (
    SELECT provider_id, connection_id, snapshot_kind, endpoint_profile_revision,
           payload_json, observed_at_ms,
           ROW_NUMBER() OVER (
               PARTITION BY connection_id, snapshot_kind
               ORDER BY observed_at_ms DESC, snapshot_id DESC
           ) AS row_number
    FROM provider_account_usage_snapshots_v1
)
WHERE row_number = 1
ORDER BY provider_id, connection_id, snapshot_kind
LIMIT 200;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("读取原生供应商账户用量失败：{error}"))?;
    let account_rows =
        decode_query_rows::<ProviderAccountUsageDashboardRow>(&accounts_result.json_data)?;
    let accounts = account_rows
        .into_iter()
        .map(ProviderAccountUsageDashboardRow::decode)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProviderUsageDashboard {
        generated_at_ms: current_provider_usage_unix_millis(),
        schema_revision: PROVIDER_USAGE_SCHEMA_REVISION,
        aggregate_revision: meta.aggregate_revision,
        health_status: meta.health_status,
        daily,
        recent_attempts,
        accounts,
    })
}

/// Decodes one recent attempt row whose provider billing partitions remain JSON in SQLite.
/// 解码一条供应商计费分区在 SQLite 中保持 JSON 的近期 attempt 行。
#[derive(Debug, Deserialize)]
struct ProviderUsageAttemptDashboardRow {
    /// Stable physical attempt identity.
    /// 稳定物理 attempt 标识。
    attempt_id: String,
    /// Server-bound consumer and operation identities.
    /// 服务端绑定的消费者与操作标识。
    consumer_id: String,
    operation: String,
    /// Provider, service-entry, and model identities.
    /// 供应商、服务入口与模型标识。
    provider_id: String,
    service_entry_id: String,
    model_id: String,
    /// Exact observed physical transport.
    /// 精确观测到的物理传输。
    transport: Option<String>,
    /// Optional terminal HTTP status.
    /// 可选终态 HTTP 状态。
    http_status: Option<i64>,
    /// Optional terminal WebSocket close code.
    /// 可选终态 WebSocket 关闭码。
    websocket_close_code: Option<i64>,
    /// Optional terminal error category.
    /// 可选终态错误类别。
    error_category: Option<String>,
    /// Optional terminal error code.
    /// 可选终态错误代码。
    error_code: Option<String>,
    /// Physical status and usage completeness.
    /// 物理状态与用量完整性。
    status: String,
    usage_availability: String,
    /// Optional normalized token partitions.
    /// 可选的规范化 Token 分区。
    input_tokens: Option<i64>,
    uncached_input_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    output_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    audio_input_tokens: Option<i64>,
    audio_output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    /// Optional serialized provider billing partitions.
    /// 可选序列化供应商计费分区。
    provider_billing_details_json: Option<String>,
    /// Terminal timestamp.
    /// 终态时间戳。
    terminal_at_ms: i64,
}

impl ProviderUsageAttemptDashboardRow {
    /// Decodes provider billing JSON without inventing absent partitions.
    /// 解码供应商计费 JSON，且不虚构缺失分区。
    fn decode(self) -> Result<ProviderUsageAttemptSummary, String> {
        let billing_details = match self.provider_billing_details_json {
            Some(json) => serde_json::from_str::<Vec<UsageBillingDetail>>(&json)
                .map_err(|error| format!("原生供应商计费明细 JSON 无效：{error}"))?,
            None => Vec::new(),
        };
        Ok(ProviderUsageAttemptSummary {
            attempt_id: self.attempt_id,
            consumer_id: self.consumer_id,
            operation: self.operation,
            provider_id: self.provider_id,
            service_entry_id: self.service_entry_id,
            model_id: self.model_id,
            transport: self.transport,
            http_status: self.http_status,
            websocket_close_code: self.websocket_close_code,
            error_category: self.error_category,
            error_code: self.error_code,
            status: self.status,
            usage_availability: self.usage_availability,
            input_tokens: self.input_tokens,
            uncached_input_tokens: self.uncached_input_tokens,
            cache_read_tokens: self.cache_read_tokens,
            cache_write_tokens: self.cache_write_tokens,
            output_tokens: self.output_tokens,
            reasoning_tokens: self.reasoning_tokens,
            audio_input_tokens: self.audio_input_tokens,
            audio_output_tokens: self.audio_output_tokens,
            total_tokens: self.total_tokens,
            billing_details,
            terminal_at_ms: self.terminal_at_ms,
        })
    }
}

/// Decodes one dashboard account row whose payload remains JSON in SQLite.
/// 解码一条载荷在 SQLite 中保持 JSON 的仪表盘账户行。
#[derive(Debug, Deserialize)]
struct ProviderAccountUsageDashboardRow {
    /// Stable provider and connection identities.
    /// 稳定供应商与连接身份。
    provider_id: String,
    connection_id: String,
    /// Snapshot category and endpoint revision.
    /// 快照类别与端点修订号。
    snapshot_kind: String,
    endpoint_profile_revision: String,
    /// Serialized normalized payload.
    /// 已序列化规范载荷。
    payload_json: String,
    /// Observation timestamp.
    /// 观测时间戳。
    observed_at_ms: i64,
}

impl ProviderAccountUsageDashboardRow {
    /// Decodes the bounded payload into the public dashboard record.
    /// 将有界载荷解码为公开仪表盘记录。
    fn decode(self) -> Result<ProviderAccountUsageSummary, String> {
        let payload = serde_json::from_str(&self.payload_json)
            .map_err(|error| format!("原生账户用量快照 JSON 无效：{error}"))?;
        Ok(ProviderAccountUsageSummary {
            provider_id: self.provider_id,
            connection_id: self.connection_id,
            snapshot_kind: self.snapshot_kind,
            endpoint_profile_revision: self.endpoint_profile_revision,
            payload,
            observed_at_ms: self.observed_at_ms,
        })
    }
}

/// Carries scalar native dashboard metadata returned by SQLite.
/// 承载 SQLite 返回的原生仪表盘标量元数据。
#[derive(Debug, Deserialize)]
struct ProviderUsageDashboardMetaRow {
    /// Monotonic aggregate revision.
    /// 单调聚合修订号。
    aggregate_revision: i64,
    /// Native persistence health state.
    /// 原生持久化健康状态。
    health_status: String,
}

/// Returns the current Unix timestamp in milliseconds for dashboard snapshots.
/// 返回仪表盘快照使用的当前 Unix 毫秒时间戳。
fn current_provider_usage_unix_millis() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

/// Persists one physical attempt idempotently with unknown values stored as SQL NULL.
/// 将一次物理 attempt 幂等持久化，并将未知值保存为 SQL NULL。
pub async fn persist_provider_usage_attempt(
    controller: &ControllerRuntimeHandle,
    record: &ProviderUsageAttemptRecord,
) -> Result<(), String> {
    validate_attempt(record)?;
    ensure_provider_usage_schema(controller).await?;
    persist_provider_usage_attempt_rows(controller, std::slice::from_ref(record)).await
}

/// Persists an idempotent batch of physical attempts and rebuilds daily aggregates once.
/// 幂等持久化一批物理 attempt，并仅重建一次每日聚合。
///
/// # Parameters
/// 参数
/// `controller` owns the system-statistics binding, `records` are immutable attempt facts, and `updated_at_ms` timestamps the aggregate revision.
/// `controller` 持有系统统计绑定，`records` 是不可变 attempt 事实，`updated_at_ms` 标记聚合修订时间。
///
/// # Returns
/// 返回值
/// Success only after every validated row and the exact aggregate rebuild complete.
/// 仅在所有已校验行与精确聚合重建均完成后返回成功。
pub async fn persist_provider_usage_attempt_batch_and_rebuild_daily(
    controller: &ControllerRuntimeHandle,
    records: &[ProviderUsageAttemptRecord],
    updated_at_ms: i64,
) -> Result<(), String> {
    if records.is_empty() {
        return Ok(());
    }
    for record in records {
        validate_attempt(record)?;
    }
    let _guard = lock_provider_usage_writes().await;
    ensure_provider_usage_schema(controller).await?;
    persist_provider_usage_attempt_rows(controller, records).await?;
    rebuild_provider_usage_daily(controller, updated_at_ms).await
}

/// Writes already validated attempt rows through one Controller batch.
/// 通过单个 Controller 批次写入已经校验的 attempt 行。
///
/// # Parameters
/// 参数
/// `controller` owns the system-statistics binding and `records` contains validated immutable rows.
/// `controller` 持有系统统计绑定，`records` 包含已校验的不可变行。
///
/// # Returns
/// 返回值
/// Success after the idempotent batch statement completes.
/// 幂等批处理语句完成后返回成功。
async fn persist_provider_usage_attempt_rows(
    controller: &ControllerRuntimeHandle,
    records: &[ProviderUsageAttemptRecord],
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            r#"
INSERT OR IGNORE INTO provider_usage_attempts_v3 (
    attempt_id, logical_request_id, attempt_index, failover_index, consumer_id,
    operation, provider_id, service_entry_id, connection_id, route_id,
    protocol_profile_id, route_revision, transport, requested_model, mapped_model,
    response_model, response_id, provider_request_id, dispatched_at_ms, headers_at_ms, first_semantic_at_ms,
    terminal_at_ms, status, dispatched, dispatched_known, semantic_output_observed,
    semantic_output_observed_known, replay_safe,
    http_status, websocket_close_code, error_category, error_code, usage_availability,
    input_tokens, uncached_input_tokens, cache_read_tokens, cache_write_tokens,
    output_tokens, reasoning_tokens, audio_input_tokens, audio_output_tokens, total_tokens,
    usage_profile_id, usage_profile_revision, usage_source_paths_json,
    usage_validation_flags_json, provider_billing_details_json, calendar_date
) SELECT
    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
    ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
    ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44,
    ?45, ?46, ?47, ?48
WHERE NOT EXISTS (
    SELECT 1
    FROM provider_usage_attempts_v3 AS existing
    WHERE existing.logical_request_id = ?2
      AND existing.attempt_index = ?3
      AND COALESCE(existing.failover_index, -1) = COALESCE(?4, -1)
);
"#,
            records.iter().map(|record| vec![
                text(record.attempt_id.clone()),
                text(record.logical_request_id.clone()),
                integer(record.attempt_index),
                optional_integer(record.failover_index),
                text(record.consumer_id.clone()),
                text(record.operation.clone()),
                text(record.provider_id.clone()),
                text(record.service_entry_id.clone()),
                text(record.connection_id.clone()),
                optional_text(record.route_id.clone()),
                text(record.protocol_profile_id.clone()),
                optional_text(record.route_revision.clone()),
                optional_text(record.transport.clone()),
                text(record.requested_model.clone()),
                text(record.mapped_model.clone()),
                optional_text(record.response_model.clone()),
                optional_text(record.response_id.clone()),
                optional_text(record.provider_request_id.clone()),
                optional_integer(record.dispatched_at_ms),
                optional_integer(record.headers_at_ms),
                optional_integer(record.first_semantic_at_ms),
                integer(record.terminal_at_ms),
                text(record.status.clone()),
                boolean(record.dispatched),
                boolean(record.dispatched_known),
                boolean(record.semantic_output_observed),
                boolean(record.semantic_output_observed_known),
                optional_boolean(record.replay_safe),
                optional_integer(record.http_status),
                optional_integer(record.websocket_close_code),
                optional_text(record.error_category.clone()),
                optional_text(record.error_code.clone()),
                text(record.usage_availability.clone()),
                optional_integer(record.input_tokens),
                optional_integer(record.uncached_input_tokens),
                optional_integer(record.cache_read_tokens),
                optional_integer(record.cache_write_tokens),
                optional_integer(record.output_tokens),
                optional_integer(record.reasoning_tokens),
                optional_integer(record.audio_input_tokens),
                optional_integer(record.audio_output_tokens),
                optional_integer(record.total_tokens),
                optional_text(record.usage_profile_id.clone()),
                optional_integer(record.usage_profile_revision),
                optional_text(record.usage_source_paths_json.clone()),
                optional_text(record.usage_validation_flags_json.clone()),
                optional_text(record.provider_billing_details_json.clone()),
                text(record.calendar_date.clone()),
            ]).collect(),
        )
        .await
        .map_err(|error| format!("写入供应商 attempt 事实失败：{error}"))?;
    Ok(())
}

/// Persists one physical attempt and rebuilds exact daily aggregates under one ordered writer lock.
/// 在同一个有序写入锁下持久化一次物理 attempt 并重建精确每日聚合。
///
/// # Parameters
/// 参数
/// `controller` owns the existing system-statistics SQLite binding, `record` is the immutable attempt, and `updated_at_ms` timestamps the aggregate revision.
/// `controller` 持有既有系统统计 SQLite 绑定，`record` 是不可变 attempt，`updated_at_ms` 标记聚合修订时间。
///
/// # Returns
/// 返回值
/// Success only after both the idempotent fact write and exact aggregate rebuild complete.
/// 仅在幂等事实写入与精确聚合重建均完成后返回成功。
pub async fn persist_provider_usage_attempt_and_rebuild_daily(
    controller: &ControllerRuntimeHandle,
    record: &ProviderUsageAttemptRecord,
    updated_at_ms: i64,
) -> Result<(), String> {
    let _guard = lock_provider_usage_writes().await;
    persist_provider_usage_attempt(controller, record).await?;
    rebuild_provider_usage_daily(controller, updated_at_ms).await
}

/// Persists one account usage snapshot independently from request token usage.
/// 独立于请求 Token 用量持久化一条账户用量快照。
pub async fn persist_provider_account_usage_snapshot(
    controller: &ControllerRuntimeHandle,
    record: &ProviderAccountUsageSnapshotRecord,
) -> Result<(), String> {
    validate_non_empty_fields(&[
        ("snapshot_id", &record.snapshot_id),
        ("provider_id", &record.provider_id),
        ("connection_id", &record.connection_id),
        ("snapshot_kind", &record.snapshot_kind),
        (
            "endpoint_profile_revision",
            &record.endpoint_profile_revision,
        ),
        ("payload_json", &record.payload_json),
    ])?;
    ensure_provider_usage_schema(controller).await?;
    controller
        .client
        .execute_sqlite_batch_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            r#"
INSERT OR IGNORE INTO provider_account_usage_snapshots_v1 (
    snapshot_id, provider_id, connection_id, snapshot_kind,
    endpoint_profile_revision, payload_json, observed_at_ms
)
SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7
WHERE NOT EXISTS (
    SELECT 1
    FROM provider_account_usage_snapshots_v1 AS latest
    WHERE latest.snapshot_id = (
        SELECT snapshot_id
        FROM provider_account_usage_snapshots_v1
        WHERE connection_id = ?3 AND snapshot_kind = ?4
        ORDER BY observed_at_ms DESC, snapshot_id DESC
        LIMIT 1
    )
      AND latest.endpoint_profile_revision = ?5
      AND latest.payload_json = ?6
);
"#,
            vec![vec![
                text(record.snapshot_id.clone()),
                text(record.provider_id.clone()),
                text(record.connection_id.clone()),
                text(record.snapshot_kind.clone()),
                text(record.endpoint_profile_revision.clone()),
                text(record.payload_json.clone()),
                integer(record.observed_at_ms),
            ]],
        )
        .await
        .map_err(|error| format!("写入供应商账户用量快照失败：{error}"))?;
    Ok(())
}

/// Persists one allowlisted rate-limit snapshot independently from usage.
/// 独立于用量持久化一条允许列表内的限流快照。
pub async fn persist_provider_rate_limit_snapshot(
    controller: &ControllerRuntimeHandle,
    record: &ProviderRateLimitSnapshotRecord,
) -> Result<(), String> {
    validate_non_empty_fields(&[
        ("snapshot_id", &record.snapshot_id),
        ("attempt_id", &record.attempt_id),
        ("headers_json", &record.headers_json),
    ])?;
    ensure_provider_usage_schema(controller).await?;
    controller
        .client
        .execute_sqlite_batch_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            r#"
INSERT OR IGNORE INTO provider_rate_limit_snapshots_v1 (
    snapshot_id, attempt_id, headers_json, retry_after_ms, observed_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5);
"#,
            vec![vec![
                text(record.snapshot_id.clone()),
                text(record.attempt_id.clone()),
                text(record.headers_json.clone()),
                optional_integer(record.retry_after_ms),
                integer(record.observed_at_ms),
            ]],
        )
        .await
        .map_err(|error| format!("写入供应商限流快照失败：{error}"))?;
    Ok(())
}

/// One immutable attempt row read for exact daily aggregation.
/// 为精确每日聚合读取的一条不可变 attempt 记录。
#[derive(Debug, Deserialize)]
struct ProviderUsageAggregateInputRow {
    /// Host-local calendar date.
    /// 宿主本地自然日。
    calendar_date: String,
    /// Stable provider identity.
    /// 稳定供应商标识。
    provider_id: String,
    /// Stable service-entry identity.
    /// 稳定服务入口标识。
    service_entry_id: String,
    /// Stable consumer identity.
    /// 稳定消费者标识。
    consumer_id: String,
    /// Stable operation category.
    /// 稳定操作类别。
    operation: String,
    /// Effective model identity.
    /// 生效模型标识。
    mapped_model: String,
    /// Physical terminal status.
    /// 物理终态。
    status: String,
    /// Usage availability state.
    /// 用量可用状态。
    usage_availability: String,
    /// Optional input tokens.
    /// 可选输入 Token。
    input_tokens: Option<i64>,
    /// Optional uncached input tokens.
    /// 可选非缓存输入 Token。
    uncached_input_tokens: Option<i64>,
    /// Optional cache-read tokens.
    /// 可选缓存读取 Token。
    cache_read_tokens: Option<i64>,
    /// Optional cache-write tokens.
    /// 可选缓存写入 Token。
    cache_write_tokens: Option<i64>,
    /// Optional output tokens.
    /// 可选输出 Token。
    output_tokens: Option<i64>,
    /// Optional reasoning tokens.
    /// 可选推理 Token。
    reasoning_tokens: Option<i64>,
    /// Optional total tokens.
    /// 可选总 Token。
    total_tokens: Option<i64>,
}

/// Exact grouping key for one daily usage row.
/// 一条每日用量记录的精确分组键。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProviderUsageDailyKey {
    /// Host-local calendar date.
    /// 宿主本地自然日。
    calendar_date: String,
    /// Stable provider identity.
    /// 稳定供应商标识。
    provider_id: String,
    /// Stable service-entry identity.
    /// 稳定服务入口标识。
    service_entry_id: String,
    /// Stable consumer identity.
    /// 稳定消费者标识。
    consumer_id: String,
    /// Stable operation category.
    /// 稳定操作类别。
    operation: String,
    /// Effective model identity.
    /// 生效模型标识。
    model_id: String,
}

/// Exact counters and token totals for one daily grouping key.
/// 一个每日分组键对应的精确计数与 Token 合计。
#[derive(Debug)]
struct ProviderUsageDailyAggregate {
    /// Immutable grouping key.
    /// 不可变分组键。
    key: ProviderUsageDailyKey,
    /// Number of physical attempts.
    /// 物理 attempt 数量。
    attempt_count: i64,
    /// Number of successful attempts.
    /// 成功 attempt 数量。
    successful_attempt_count: i64,
    /// Number of attempts with partial or final usage.
    /// 带部分或终态用量的 attempt 数量。
    reported_usage_attempt_count: i64,
    /// Optional exact input-token sum.
    /// 可选精确输入 Token 合计。
    input_tokens: Option<i64>,
    /// Optional exact uncached-input sum.
    /// 可选精确非缓存输入 Token 合计。
    uncached_input_tokens: Option<i64>,
    /// Optional exact cache-read sum.
    /// 可选精确缓存读取合计。
    cache_read_tokens: Option<i64>,
    /// Optional exact cache-write sum.
    /// 可选精确缓存写入合计。
    cache_write_tokens: Option<i64>,
    /// Optional exact output-token sum.
    /// 可选精确输出 Token 合计。
    output_tokens: Option<i64>,
    /// Optional exact reasoning-token sum.
    /// 可选精确推理 Token 合计。
    reasoning_tokens: Option<i64>,
    /// Optional exact total-token sum.
    /// 可选精确总 Token 合计。
    total_tokens: Option<i64>,
}

impl ProviderUsageDailyAggregate {
    /// Creates an empty aggregate for one exact key.
    /// 为一个精确键创建空聚合。
    fn new(key: ProviderUsageDailyKey) -> Self {
        Self {
            key,
            attempt_count: 0,
            successful_attempt_count: 0,
            reported_usage_attempt_count: 0,
            input_tokens: None,
            uncached_input_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            output_tokens: None,
            reasoning_tokens: None,
            total_tokens: None,
        }
    }

    /// Applies one immutable attempt row using checked integer arithmetic.
    /// 使用受检整数运算应用一条不可变 attempt 记录。
    fn apply(&mut self, row: ProviderUsageAggregateInputRow) -> Result<(), String> {
        self.attempt_count = self
            .attempt_count
            .checked_add(1)
            .ok_or_else(|| String::from("provider daily attempt count overflow"))?;
        if row.status == "succeeded" {
            self.successful_attempt_count = self
                .successful_attempt_count
                .checked_add(1)
                .ok_or_else(|| String::from("provider daily success count overflow"))?;
        }
        match row.usage_availability.as_str() {
            "partial" | "final" => {
                self.reported_usage_attempt_count = self
                    .reported_usage_attempt_count
                    .checked_add(1)
                    .ok_or_else(|| String::from("provider daily usage count overflow"))?;
            }
            "missing" => {}
            value => return Err(format!("unknown provider usage availability: {value}")),
        }
        checked_add_optional_i64(&mut self.input_tokens, row.input_tokens, "input_tokens")?;
        checked_add_optional_i64(
            &mut self.uncached_input_tokens,
            row.uncached_input_tokens,
            "uncached_input_tokens",
        )?;
        checked_add_optional_i64(
            &mut self.cache_read_tokens,
            row.cache_read_tokens,
            "cache_read_tokens",
        )?;
        checked_add_optional_i64(
            &mut self.cache_write_tokens,
            row.cache_write_tokens,
            "cache_write_tokens",
        )?;
        checked_add_optional_i64(&mut self.output_tokens, row.output_tokens, "output_tokens")?;
        checked_add_optional_i64(
            &mut self.reasoning_tokens,
            row.reasoning_tokens,
            "reasoning_tokens",
        )?;
        checked_add_optional_i64(&mut self.total_tokens, row.total_tokens, "total_tokens")?;
        Ok(())
    }

    /// Renders one validated parameter-free INSERT inside the rebuild transaction.
    /// 在重建事务中渲染一条已校验的无参数 INSERT。
    fn insert_sql(&self) -> Result<String, String> {
        Ok(format!(
            "INSERT INTO provider_usage_daily_v3 (calendar_date, provider_id, service_entry_id, consumer_id, operation, model_id, attempt_count, successful_attempt_count, reported_usage_attempt_count, input_tokens, uncached_input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, reasoning_tokens, total_tokens, aggregate_revision) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, 1);",
            sql_text_literal(&self.key.calendar_date)?,
            sql_text_literal(&self.key.provider_id)?,
            sql_text_literal(&self.key.service_entry_id)?,
            sql_text_literal(&self.key.consumer_id)?,
            sql_text_literal(&self.key.operation)?,
            sql_text_literal(&self.key.model_id)?,
            self.attempt_count,
            self.successful_attempt_count,
            self.reported_usage_attempt_count,
            sql_optional_i64(self.input_tokens),
            sql_optional_i64(self.uncached_input_tokens),
            sql_optional_i64(self.cache_read_tokens),
            sql_optional_i64(self.cache_write_tokens),
            sql_optional_i64(self.output_tokens),
            sql_optional_i64(self.reasoning_tokens),
            sql_optional_i64(self.total_tokens),
        ))
    }
}

/// Aggregates immutable attempt token facts with checked integer arithmetic.
/// 使用受检整数运算聚合不可变 attempt Token 事实。
fn aggregate_provider_usage_rows(
    rows: Vec<ProviderUsageAggregateInputRow>,
) -> Result<BTreeMap<ProviderUsageDailyKey, ProviderUsageDailyAggregate>, String> {
    let mut aggregates = BTreeMap::new();
    for row in rows {
        let key = ProviderUsageDailyKey {
            calendar_date: row.calendar_date.clone(),
            provider_id: row.provider_id.clone(),
            service_entry_id: row.service_entry_id.clone(),
            consumer_id: row.consumer_id.clone(),
            operation: row.operation.clone(),
            model_id: row.mapped_model.clone(),
        };
        aggregates
            .entry(key.clone())
            .or_insert_with(|| ProviderUsageDailyAggregate::new(key))
            .apply(row)?;
    }
    Ok(aggregates)
}

/// Adds one optional integer into an optional exact sum with overflow detection.
/// 将一个可选整数加入可选精确合计并检测溢出。
fn checked_add_optional_i64(
    target: &mut Option<i64>,
    value: Option<i64>,
    field: &str,
) -> Result<(), String> {
    if let Some(value) = value {
        *target = Some(match *target {
            Some(current) => current
                .checked_add(value)
                .ok_or_else(|| format!("provider daily {field} overflow"))?,
            None => value,
        });
    }
    Ok(())
}

/// Escapes one trusted database value as a SQLite text literal and rejects embedded NUL bytes.
/// 将一个可信数据库值转义为 SQLite 文本字面量并拒绝内嵌 NUL 字节。
fn sql_text_literal(value: &str) -> Result<String, String> {
    if value.contains('\0') {
        return Err(String::from(
            "provider daily key contains an embedded NUL byte",
        ));
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

/// Renders one optional integer as a parameter-free SQLite literal.
/// 将一个可选整数渲染为无参数 SQLite 字面量。
fn sql_optional_i64(value: Option<i64>) -> String {
    value.map_or_else(|| String::from("NULL"), |value| value.to_string())
}

/// Rebuilds daily aggregates transactionally from immutable attempt facts.
/// 从不可变 attempt 事实事务性重建每日聚合。
pub async fn rebuild_provider_usage_daily(
    controller: &ControllerRuntimeHandle,
    updated_at_ms: i64,
) -> Result<(), String> {
    ensure_provider_usage_schema(controller).await?;
    let result = controller
        .client
        .query_sqlite_json_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            r#"
SELECT
    calendar_date, provider_id, service_entry_id, consumer_id, operation,
    mapped_model, status, usage_availability,
    input_tokens, uncached_input_tokens, cache_read_tokens, cache_write_tokens,
    output_tokens, reasoning_tokens, total_tokens
FROM provider_usage_attempts_v3
ORDER BY calendar_date, provider_id, service_entry_id, consumer_id, operation,
         mapped_model, attempt_id;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("读取供应商 attempt 聚合事实失败：{error}"))?;
    let rows = decode_query_rows::<ProviderUsageAggregateInputRow>(&result.json_data)?;
    let aggregates = aggregate_provider_usage_rows(rows)?;
    let mut script = String::from("BEGIN IMMEDIATE;\nDELETE FROM provider_usage_daily_v3;\n");
    for aggregate in aggregates.values() {
        script.push_str(aggregate.insert_sql()?.as_str());
        script.push('\n');
    }
    script.push_str(
        format!(
            r#"INSERT INTO provider_usage_meta(meta_key, meta_value, updated_at_ms)
VALUES ('aggregate_revision', '1', {updated_at_ms})
ON CONFLICT(meta_key) DO UPDATE SET
    meta_value = CAST(CAST(provider_usage_meta.meta_value AS INTEGER) + 1 AS TEXT),
    updated_at_ms = excluded.updated_at_ms;
COMMIT;
"#
        )
        .as_str(),
    );
    controller
        .client
        .execute_sqlite_script_typed(
            SYSTEM_STATISTICS_SPACE_ID,
            system_statistics_sqlite_binding_id(controller),
            script.as_str(),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("重建供应商每日用量聚合失败：{error}"))?;
    Ok(())
}

/// Validates one physical attempt before any SQL is sent.
/// 在发送任何 SQL 前校验一次物理 attempt。
fn validate_attempt(record: &ProviderUsageAttemptRecord) -> Result<(), String> {
    validate_non_empty_fields(&[
        ("logical_request_id", &record.logical_request_id),
        ("attempt_id", &record.attempt_id),
        ("consumer_id", &record.consumer_id),
        ("operation", &record.operation),
        ("provider_id", &record.provider_id),
        ("service_entry_id", &record.service_entry_id),
        ("connection_id", &record.connection_id),
        ("protocol_profile_id", &record.protocol_profile_id),
        ("requested_model", &record.requested_model),
        ("mapped_model", &record.mapped_model),
        ("status", &record.status),
        ("usage_availability", &record.usage_availability),
        ("calendar_date", &record.calendar_date),
    ])?;
    if record.attempt_index < 0 || record.failover_index.is_some_and(|value| value < 0) {
        return Err(String::from("供应商 attempt 索引不能为负数。"));
    }
    if !matches!(
        record.usage_availability.as_str(),
        "missing" | "partial" | "final"
    ) {
        return Err(String::from(
            "供应商 attempt 的 usage_availability 必须是 missing、partial 或 final。",
        ));
    }
    Ok(())
}

/// Validates a list of required non-empty string fields.
/// 校验一组必填非空字符串字段。
fn validate_non_empty_fields(fields: &[(&str, &String)]) -> Result<(), String> {
    for (label, value) in fields {
        if value.trim().is_empty() {
            return Err(format!("供应商用量事实缺少非空字段 `{label}`。"));
        }
    }
    Ok(())
}

/// Converts one string into a controller SQLite value.
/// 将一个字符串转换为控制器 SQLite 值。
fn text(value: String) -> ControllerSqliteValue {
    ControllerSqliteValue::String(value)
}

/// Converts one integer into a controller SQLite value.
/// 将一个整数转换为控制器 SQLite 值。
fn integer(value: i64) -> ControllerSqliteValue {
    ControllerSqliteValue::Int64(value)
}

/// Converts one boolean into an integer SQLite value.
/// 将一个布尔值转换为整数 SQLite 值。
fn boolean(value: bool) -> ControllerSqliteValue {
    ControllerSqliteValue::Int64(i64::from(value))
}

/// Converts one optional integer into SQL NULL or an integer value.
/// 将一个可选整数转换为 SQL NULL 或整数值。
fn optional_integer(value: Option<i64>) -> ControllerSqliteValue {
    value.map(integer).unwrap_or(ControllerSqliteValue::Null)
}

/// Converts one optional boolean into SQL NULL or an integer value.
/// 将一个可选布尔值转换为 SQL NULL 或整数值。
fn optional_boolean(value: Option<bool>) -> ControllerSqliteValue {
    value.map(boolean).unwrap_or(ControllerSqliteValue::Null)
}

/// Converts one optional string into SQL NULL or a text value.
/// 将一个可选字符串转换为 SQL NULL 或文本值。
fn optional_text(value: Option<String>) -> ControllerSqliteValue {
    value.map(text).unwrap_or(ControllerSqliteValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one minimal valid physical attempt fixture.
    /// 构建一个最小有效物理 attempt fixture。
    fn attempt_fixture() -> ProviderUsageAttemptRecord {
        ProviderUsageAttemptRecord {
            logical_request_id: String::from("logical-1"),
            attempt_id: String::from("attempt-1"),
            attempt_index: 0,
            failover_index: Some(0),
            consumer_id: String::from("webui"),
            operation: String::from("conversation"),
            provider_id: String::from("openai"),
            service_entry_id: String::from("api"),
            connection_id: String::from("connection-1"),
            route_id: Some(String::from("responses")),
            protocol_profile_id: String::from("openai_codex_responses"),
            route_revision: Some(String::from("revision-1")),
            transport: Some(String::from("http_sse")),
            requested_model: String::from("coding"),
            mapped_model: String::from("gpt-5"),
            response_model: None,
            response_id: None,
            provider_request_id: None,
            dispatched_at_ms: Some(1),
            headers_at_ms: Some(2),
            first_semantic_at_ms: None,
            terminal_at_ms: 3,
            status: String::from("succeeded"),
            dispatched: true,
            dispatched_known: true,
            semantic_output_observed: false,
            semantic_output_observed_known: true,
            replay_safe: Some(true),
            http_status: Some(200),
            websocket_close_code: None,
            error_category: None,
            error_code: None,
            usage_availability: String::from("missing"),
            input_tokens: None,
            uncached_input_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            output_tokens: None,
            reasoning_tokens: None,
            audio_input_tokens: None,
            audio_output_tokens: None,
            total_tokens: None,
            usage_profile_id: None,
            usage_profile_revision: None,
            usage_source_paths_json: None,
            usage_validation_flags_json: None,
            provider_billing_details_json: None,
            calendar_date: String::from("2026-08-06"),
        }
    }

    #[test]
    fn attempt_validation_preserves_missing_usage() {
        let record = attempt_fixture();
        validate_attempt(&record).expect("missing usage should remain valid");
        assert_eq!(record.input_tokens, None);
        assert_eq!(record.usage_availability, "missing");
    }

    #[test]
    fn attempt_validation_rejects_ambiguous_usage_availability() {
        let mut record = attempt_fixture();
        record.usage_availability = String::from("zero");
        let error = validate_attempt(&record).expect_err("ambiguous availability must fail");
        assert!(error.contains("usage_availability"));
    }

    /// Verifies dashboard rows retain all token partitions and typed provider usage details.
    /// 验证仪表盘行保留全部 Token 分区与强类型供应商用量明细。
    #[test]
    fn attempt_dashboard_row_decodes_token_partitions() {
        let row = ProviderUsageAttemptDashboardRow {
            attempt_id: String::from("attempt-1"),
            consumer_id: String::from("vmm"),
            operation: String::from("compact"),
            provider_id: String::from("alibaba"),
            service_entry_id: String::from("model-studio"),
            model_id: String::from("qwen-plus"),
            transport: Some(String::from("http_sse")),
            http_status: Some(200),
            websocket_close_code: None,
            error_category: None,
            error_code: None,
            status: String::from("succeeded"),
            usage_availability: String::from("final"),
            input_tokens: Some(10),
            uncached_input_tokens: Some(7),
            cache_read_tokens: Some(3),
            cache_write_tokens: Some(1),
            output_tokens: Some(20),
            reasoning_tokens: Some(4),
            audio_input_tokens: None,
            audio_output_tokens: None,
            total_tokens: Some(30),
            provider_billing_details_json: Some(String::from(
                r#"[{"billing_type":"response_api","input_tokens":10,"output_tokens":20,"total_tokens":30,"text_tokens":8,"image_tokens":2,"reasoning_tokens":4}]"#,
            )),
            terminal_at_ms: 10,
        };

        let summary = row.decode().expect("typed billing details should decode");
        assert_eq!(summary.billing_details.len(), 1);
        assert_eq!(summary.transport.as_deref(), Some("http_sse"));
        assert_eq!(summary.http_status, Some(200));
        assert_eq!(summary.billing_details[0].billing_type, "response_api");
        assert_eq!(summary.billing_details[0].image_tokens, Some(2));
        assert_eq!(summary.uncached_input_tokens, Some(7));
        assert_eq!(summary.cache_read_tokens, Some(3));
        assert_eq!(summary.cache_write_tokens, Some(1));
        assert_eq!(summary.reasoning_tokens, Some(4));
    }

    /// Verifies corrupt persisted billing JSON is explicit instead of silently disappearing.
    /// 验证损坏的持久化计费 JSON 会显式报错而非静默消失。
    #[test]
    fn attempt_dashboard_row_rejects_corrupt_billing_json() {
        let mut row = ProviderUsageAttemptDashboardRow {
            attempt_id: String::from("attempt-1"),
            consumer_id: String::from("webui"),
            operation: String::from("conversation"),
            provider_id: String::from("alibaba"),
            service_entry_id: String::from("model-studio"),
            model_id: String::from("qwen-plus"),
            transport: None,
            http_status: None,
            websocket_close_code: None,
            error_category: None,
            error_code: None,
            status: String::from("succeeded"),
            usage_availability: String::from("final"),
            input_tokens: Some(1),
            uncached_input_tokens: Some(1),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
            output_tokens: Some(0),
            reasoning_tokens: Some(0),
            audio_input_tokens: None,
            audio_output_tokens: None,
            total_tokens: Some(1),
            provider_billing_details_json: None,
            terminal_at_ms: 10,
        };
        row.provider_billing_details_json = Some(String::from("{}"));

        let error = row.decode().expect_err("non-array billing JSON must fail");
        assert!(error.contains("计费明细 JSON 无效"));
    }

    /// Verifies daily aggregation sums token facts without any price dimension.
    /// 验证每日聚合只汇总 Token 事实且不存在任何价格维度。
    #[test]
    fn daily_aggregation_preserves_exact_token_totals() {
        let rows = vec![aggregate_input_row(), aggregate_input_row()];

        let aggregates = aggregate_provider_usage_rows(rows).expect("rows should aggregate");
        let aggregate = aggregates.values().next().expect("aggregate should exist");
        assert_eq!(aggregate.attempt_count, 2);
        assert_eq!(aggregate.successful_attempt_count, 2);
        assert_eq!(aggregate.reported_usage_attempt_count, 2);
        assert_eq!(aggregate.input_tokens, Some(20));
        assert_eq!(aggregate.cache_read_tokens, Some(4));
        assert_eq!(aggregate.output_tokens, Some(6));
        assert_eq!(aggregate.total_tokens, Some(26));
    }

    /// Builds one exact daily-aggregation input fixture.
    /// 构建一条精确每日聚合输入夹具。
    fn aggregate_input_row() -> ProviderUsageAggregateInputRow {
        ProviderUsageAggregateInputRow {
            calendar_date: String::from("2026-08-06"),
            provider_id: String::from("openai"),
            service_entry_id: String::from("api"),
            consumer_id: String::from("main-agent"),
            operation: String::from("conversation"),
            mapped_model: String::from("gpt-5"),
            status: String::from("succeeded"),
            usage_availability: String::from("final"),
            input_tokens: Some(10),
            uncached_input_tokens: Some(8),
            cache_read_tokens: Some(2),
            cache_write_tokens: None,
            output_tokens: Some(3),
            reasoning_tokens: Some(1),
            total_tokens: Some(13),
        }
    }
}
