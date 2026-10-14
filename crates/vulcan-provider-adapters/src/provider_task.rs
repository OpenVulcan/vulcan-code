//! Durable non-secret lifecycle records for provider-native asynchronous tools.
//! 供应商原生异步工具的可持久化非秘密生命周期记录。

use crate::{ProviderToolAdapterKind, ProviderToolExecutionResult, ProviderToolTargetSelection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(test)]
use uuid::Uuid;

/// Exact version of the durable provider-task recovery document.
/// 供应商任务恢复文档的精确版本。
pub const PROVIDER_TASK_RECOVERY_VERSION: u32 = 1;
/// Maximum age of a retained terminal provider-task record.
/// 已保留供应商任务终态记录的最大时长。
const PROVIDER_TASK_TERMINAL_RETENTION_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
/// Maximum number of terminal task records retained after age pruning.
/// 完成按时长清理后保留的最大任务终态记录数。
const PROVIDER_TASK_MAXIMUM_TERMINAL_RECORDS: usize = 256;
/// Maximum total active and terminal task records retained in one recovery snapshot.
/// 单份恢复快照允许保留的活动与终态任务总记录上限。
const PROVIDER_TASK_MAXIMUM_RECORDS: usize = 2_048;
/// Maximum bytes accepted from or written to one recovery snapshot.
/// 单份恢复快照允许读取或写入的最大字节数。
const PROVIDER_TASK_SNAPSHOT_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Exact authoritative provider-task recovery snapshot file name.
/// 精确的权威供应商任务恢复快照文件名。
const PROVIDER_TASK_CURRENT_SNAPSHOT_FILE: &str = "snapshot.json";
/// Exact rollback provider-task recovery snapshot file name.
/// 精确的回滚供应商任务恢复快照文件名。
const PROVIDER_TASK_PREVIOUS_SNAPSHOT_FILE: &str = "snapshot.previous.json";
/// Exact same-directory staging file used during provider-task snapshot publication.
/// 供应商任务快照发布期间使用的精确同目录暂存文件名。
const PROVIDER_TASK_STAGED_SNAPSHOT_FILE: &str = ".snapshot.next.tmp";
/// Maximum characters accepted in one persisted provider-task identity field.
/// 单个持久供应商任务身份字段允许的最大字符数。
const PROVIDER_TASK_IDENTITY_MAX_CHARACTERS: usize = 2_048;
/// Maximum characters accepted in one persisted provider-task diagnostic.
/// 单个持久供应商任务诊断允许的最大字符数。
pub(crate) const PROVIDER_TASK_DIAGNOSTIC_MAX_CHARACTERS: usize = 8_192;
/// Maximum ordered recovery fingerprints retained by one asynchronous task.
/// 单个异步任务允许保留的有序恢复指纹上限。
const PROVIDER_TASK_MAXIMUM_RECOVERY_FINGERPRINTS: usize = 256;
/// Maximum retained consecutive observation failures before the counter saturates.
/// 连续观测失败计数达到此上限后饱和。
pub(crate) const PROVIDER_TASK_MAXIMUM_OBSERVATION_FAILURES: u32 = 32;

/// Provider-task phases with explicit uncertain-dispatch and cancellation intent states.
/// 带显式分派不确定与取消意图状态的供应商任务阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAsyncTaskPhase {
    /// A durable dispatch intent exists but no provider task identifier is confirmed.
    /// 已存在持久化分派意图，但尚未确认供应商任务标识。
    Dispatching,
    /// The provider confirmed a queued task.
    /// 供应商已确认排队任务。
    Queued,
    /// The provider reports that the task is running.
    /// 供应商报告任务正在运行。
    Running,
    /// A durable cancellation intent exists and must precede further polling.
    /// 已存在持久化取消意图，后续轮询前必须先处理取消。
    CancellationRequested,
    /// The provider confirmed successful completion.
    /// 供应商已确认成功完成。
    Completed,
    /// The provider or adapter produced a terminal failure.
    /// 供应商或适配器产生终态失败。
    Failed,
    /// The provider confirmed cancellation.
    /// 供应商已确认取消。
    Cancelled,
    /// Dispatch may have reached the provider but no task identity was confirmed.
    /// 分派可能已到达供应商，但未确认任务身份。
    DispatchOutcomeUnknown,
}

impl ProviderAsyncTaskPhase {
    /// Reports whether no further automatic provider action is allowed.
    /// 报告是否不再允许任何自动供应商操作。
    ///
    /// # Returns
    /// 返回值
    /// `true` for every completed, failed, cancelled, or uncertain-dispatch record.
    /// 对完成、失败、取消或分派不确定记录返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::DispatchOutcomeUnknown
        )
    }

    /// Returns the stable serialized lifecycle phase used by structured telemetry.
    /// 返回结构化遥测使用的稳定序列化生命周期阶段。
    ///
    /// # Returns
    /// 返回值
    /// One lowercase snake-case phase identifier matching the durable JSON contract.
    /// 返回一个与持久 JSON 契约一致的小写蛇形阶段标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dispatching => "dispatching",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::CancellationRequested => "cancellation_requested",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::DispatchOutcomeUnknown => "dispatch_outcome_unknown",
        }
    }
}

/// Non-secret recovery data required to normalize a provider task after restart.
/// 重启后归一化供应商任务所需的非秘密恢复数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderAsyncTaskRecoveryContext {
    /// No adapter-specific recovery data is required.
    /// 不需要适配器专属恢复数据。
    None,
    /// Ordered SHA-256 fingerprints of generic submitted identities.
    /// 通用已提交身份的有序 SHA-256 指纹。
    #[serde(rename = "ordered_sha256")]
    OrderedSha256 {
        /// Ordered hexadecimal SHA-256 values without retaining signed object URLs.
        /// 不保留签名对象 URL 的有序十六进制 SHA-256 值。
        fingerprints: Vec<String>,
    },
}

/// Immutable non-secret route affinity for one asynchronous provider task.
/// 单个异步供应商任务的不可变非秘密路由亲和性。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderAsyncTaskRoute {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Exact service-entry identifier.
    /// 精确服务入口标识。
    pub service_entry_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// Exact selected target identity.
    /// 精确选中目标身份。
    pub selection: ProviderToolTargetSelection,
    /// Exact registered adapter identity.
    /// 精确已注册适配器身份。
    pub adapter: ProviderToolAdapterKind,
    /// Manifest revision frozen at task creation; zero only for persisted pre-manifest records.
    /// 任务创建时冻结的 Manifest 修订号；仅早于 Manifest 的持久化记录为零。
    #[serde(default)]
    pub manifest_revision: u64,
    /// Frozen non-secret target snapshot captured at dispatch; recovery and poll resolve the
    /// target from this snapshot instead of reading a newer manifest (plan §R9.6). `None`
    /// only for legacy revision-zero records created before frozen manifest snapshots existed.
    /// 分派时捕获的冻结非秘密目标快照；恢复与轮询从此快照解析目标，而不读取更新的
    /// Manifest（方案 §R9.6）。仅在冻结 Manifest 快照存在前创建的旧版零修订记录为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_target: Option<crate::ProviderToolTarget>,
    /// Frozen non-secret service entry captured at dispatch; it retains the exact task base URL
    /// and service contract across manifest hot replacement. `None` is allowed only for legacy
    /// revision-zero records created before complete task-route snapshots existed.
    /// 分派时捕获的冻结非秘密服务入口；它在 Manifest 热替换后保留精确任务基础 URL 与服务
    /// 契约。仅在完整任务路由快照出现前创建的旧版零修订记录允许为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_service_entry: Option<crate::ServiceEntry>,
    /// Frozen operation identifier that authorized this asynchronous lifecycle.
    /// 授权当前异步生命周期的冻结操作标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// Frozen non-secret operation declaration used by generic recovery after hot replacement.
    /// 热替换后通用恢复使用的冻结非秘密操作声明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_operation: Option<crate::provider_manifest::OperationDeclaration>,
    /// Frozen request profiles used to build the asynchronous task start body.
    /// 用于构建异步任务启动正文的冻结请求 Profile。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frozen_request_profiles: Vec<crate::provider_manifest::RequestProjectionProfile>,
    /// Frozen response profiles used for start and every retained poll response.
    /// 用于启动响应及每次保留轮询响应的冻结响应 Profile。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frozen_response_profiles: Vec<crate::provider_manifest::ResponseProjectionProfile>,
}

/// Durable non-secret state for one provider-native asynchronous task.
/// 单个供应商原生异步任务的可持久化非秘密状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderAsyncTaskRecord {
    /// Stable host execution identifier.
    /// 稳定宿主执行标识。
    pub execution_id: String,
    /// Optional owning conversation session identifier.
    /// 可选所属会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_session_id: Option<String>,
    /// Exact immutable non-secret route.
    /// 精确不可变非秘密路由。
    pub route: ProviderAsyncTaskRoute,
    /// Confirmed provider task identifier, absent only before confirmation.
    /// 已确认供应商任务标识，仅在确认前缺失。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_task_id: Option<String>,
    /// Adapter-specific non-secret recovery context.
    /// 适配器专属非秘密恢复上下文。
    pub recovery: ProviderAsyncTaskRecoveryContext,
    /// Current lifecycle phase.
    /// 当前生命周期阶段。
    pub phase: ProviderAsyncTaskPhase,
    /// Creation time as Unix milliseconds.
    /// Unix 毫秒创建时间。
    pub created_at_unix_ms: u64,
    /// Last observed mutation time as Unix milliseconds.
    /// Unix 毫秒最后变更时间。
    pub updated_at_unix_ms: u64,
    /// Absolute task deadline as Unix milliseconds.
    /// Unix 毫秒绝对任务截止时间。
    pub deadline_unix_ms: u64,
    /// Manifest-authoritative poll interval in milliseconds.
    /// Manifest 权威声明的轮询间隔（毫秒）。
    #[serde(default = "default_provider_task_poll_interval_ms")]
    pub poll_interval_ms: u64,
    /// Consecutive transient observation failures used only for bounded retry backoff.
    /// 仅用于有界重试退避的连续瞬时观测失败次数。
    #[serde(default)]
    pub consecutive_observation_failures: u32,
    /// Whether the exact adapter has a provider cancellation endpoint.
    /// 精确适配器是否具有供应商取消端点。
    pub cancellation_supported: bool,
    /// Controlled terminal or recovery diagnostic.
    /// 受控终态或恢复诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Process-local normalized terminal result; completed tasks are re-observed after restart.
    /// 进程内归一化终态结果；完成任务会在重启后重新观测。
    #[serde(default, skip_serializing)]
    pub result: Option<ProviderToolExecutionResult>,
}

/// Returns the compatibility poll interval for records written before operation metadata existed.
/// 返回 operation 元数据出现前已写记录使用的兼容轮询间隔。
fn default_provider_task_poll_interval_ms() -> u64 {
    15_000
}

/// Computes the next bounded observation delay from one frozen asynchronous operation.
/// 根据一条冻结异步操作计算下一次有界观测延迟。
///
/// # Parameters
/// 参数
/// `record` retains the base poll interval, frozen backoff ceiling, and failure streak.
/// `record` 保留基础轮询间隔、冻结退避上限与失败连续次数。
///
/// # Returns
/// 返回值
/// Exponential backoff capped by the exact manifest declaration, or the legacy fixed interval.
/// 返回由精确 Manifest 声明封顶的指数退避；旧记录返回固定间隔。
pub(crate) fn provider_task_observation_delay_ms(record: &ProviderAsyncTaskRecord) -> u64 {
    let maximum_backoff_ms = record
        .route
        .frozen_operation
        .as_ref()
        .and_then(|operation| operation.async_execution.as_ref())
        .map_or(record.poll_interval_ms, |declaration| {
            declaration.max_backoff_ms
        });
    let multiplier = 1_u64
        .checked_shl(record.consecutive_observation_failures.min(63))
        .unwrap_or(u64::MAX);
    record
        .poll_interval_ms
        .saturating_mul(multiplier)
        .min(maximum_backoff_ms)
}

/// Truncates one public provider-task diagnostic on a Unicode scalar boundary.
/// 在 Unicode 标量边界截断一条公开供应商任务诊断。
///
/// # Parameters
/// 参数
/// `value` is already stripped of any exact in-memory credential by the caller.
/// `value` 已由调用方移除任何精确内存凭据。
///
/// # Returns
/// 返回值
/// A diagnostic that always fits the durable recovery contract.
/// 返回始终符合持久恢复契约的诊断。
pub(crate) fn bound_provider_task_diagnostic(value: &str) -> String {
    if value.chars().count() <= PROVIDER_TASK_DIAGNOSTIC_MAX_CHARACTERS {
        return value.to_string();
    }
    let retained = PROVIDER_TASK_DIAGNOSTIC_MAX_CHARACTERS.saturating_sub(3);
    let mut bounded = value.chars().take(retained).collect::<String>();
    bounded.push_str("...");
    bounded
}

/// Versioned durable task recovery snapshot.
/// 带版本的任务恢复持久化快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderTaskRecoverySnapshot {
    /// Exact contract version.
    /// 精确契约版本。
    version: u32,
    /// Monotonic publication revision used to select the newest durable snapshot.
    /// 用于选择最新持久化快照的单调发布修订号。
    revision: u64,
    /// Complete execution map keyed by stable host identifier.
    /// 按稳定宿主标识索引的完整执行映射。
    records: BTreeMap<String, ProviderAsyncTaskRecord>,
}

/// Result of one provider-confirmed asynchronous task start.
/// 一次供应商确认异步任务启动的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderAsyncTaskStart {
    /// Confirmed provider task identifier.
    /// 已确认供应商任务标识。
    pub provider_task_id: String,
    /// Adapter-specific non-secret recovery context.
    /// 适配器专属非秘密恢复上下文。
    pub recovery: ProviderAsyncTaskRecoveryContext,
    /// Whether provider cancellation is supported.
    /// 是否支持供应商取消。
    pub cancellation_supported: bool,
}

/// Result of one exact provider-task observation.
/// 一次精确供应商任务观测结果。
#[derive(Debug)]
pub(crate) enum ProviderAsyncTaskPoll {
    /// The provider still owns a non-terminal task.
    /// 供应商仍持有未终结任务。
    Pending(ProviderAsyncTaskPhase),
    /// The provider returned one normalized terminal result.
    /// 供应商返回一个归一化终态结果。
    Completed(ProviderToolExecutionResult),
    /// The provider confirmed a terminal failure.
    /// 供应商确认终态失败。
    Failed(String),
    /// The provider confirmed cancellation.
    /// 供应商确认取消。
    Cancelled,
}

/// Shared recovery registry with atomic unique-snapshot publication.
/// 使用原子唯一快照发布的共享恢复注册表。
#[derive(Debug, Clone, Default)]
pub(crate) struct ProviderTaskRecoveryStore {
    /// Process-local authoritative task records.
    /// 进程内权威任务记录。
    records: Arc<Mutex<BTreeMap<String, ProviderAsyncTaskRecord>>>,
    /// Optional durable snapshot directory.
    /// 可选持久化快照目录。
    directory: Arc<Mutex<Option<PathBuf>>>,
    /// Last successfully published durable revision.
    /// 最后一次成功发布的持久化修订号。
    revision: Arc<Mutex<u64>>,
    /// Serializes complete-map publication so revisions cannot overtake each other.
    /// 串行化完整映射发布，避免修订相互超越。
    publish_lock: Arc<Mutex<()>>,
    /// Serializes memory mutation with its corresponding durable publication.
    /// 串行化内存变更及其对应的持久发布。
    mutation_lock: Arc<Mutex<()>>,
}

impl ProviderTaskRecoveryStore {
    /// Configures a durable directory and loads the newest complete strict snapshot.
    /// 配置持久化目录并加载最新完整严格快照。
    ///
    /// # Parameters
    /// 参数
    /// `directory` is host-owned and must not be shared with model catalogs or secrets.
    /// `directory` 由宿主持有，不得与模型目录或秘密共享。
    ///
    /// # Returns
    /// 返回值
    /// The loaded active records, or an explicit format or filesystem failure.
    /// 返回已加载活动记录，或明确格式/文件系统失败。
    pub fn configure(&self, directory: &Path) -> Result<Vec<ProviderAsyncTaskRecord>, String> {
        fs::create_dir_all(directory).map_err(|error| {
            format!(
                "Provider task recovery directory `{}` could not be created: {error}",
                directory.display()
            )
        })?;
        let mut loaded = load_latest_snapshot(directory)?;
        prune_terminal_records(&mut loaded.records, current_unix_millis()?, None);
        let _mutation_guard = self
            .mutation_lock
            .lock()
            .map_err(|_| String::from("The provider task mutation lock is poisoned."))?;
        // Previous values make configuration transactional if the initial publication fails.
        // 若初始发布失败，previous values 使配置操作保持事务性。
        let previous_records = {
            let mut records = self
                .records
                .lock()
                .map_err(|_| String::from("The provider task recovery lock is poisoned."))?;
            std::mem::replace(&mut *records, loaded.records)
        };
        let previous_revision = {
            let mut revision = self
                .revision
                .lock()
                .map_err(|_| String::from("The provider task revision lock is poisoned."))?;
            std::mem::replace(&mut *revision, loaded.revision)
        };
        let previous_directory = {
            let mut configured_directory = self
                .directory
                .lock()
                .map_err(|_| String::from("The provider task directory lock is poisoned."))?;
            configured_directory.replace(directory.to_path_buf())
        };
        if let Err(error) = self.persist() {
            *self
                .records
                .lock()
                .map_err(|_| String::from("The provider task recovery lock is poisoned."))? =
                previous_records;
            *self
                .revision
                .lock()
                .map_err(|_| String::from("The provider task revision lock is poisoned."))? =
                previous_revision;
            *self
                .directory
                .lock()
                .map_err(|_| String::from("The provider task directory lock is poisoned."))? =
                previous_directory;
            return Err(error);
        }
        self.snapshot()
    }

    /// Returns all process-local records in stable key order.
    /// 按稳定键顺序返回全部进程内记录。
    ///
    /// # Returns
    /// 返回值
    /// A cloned record list suitable for recovery scheduling or status display.
    /// 返回适合恢复调度或状态展示的克隆记录列表。
    pub fn snapshot(&self) -> Result<Vec<ProviderAsyncTaskRecord>, String> {
        Ok(self
            .records
            .lock()
            .map_err(|_| String::from("The provider task recovery lock is poisoned."))?
            .values()
            .cloned()
            .collect())
    }

    /// Returns one exact task record.
    /// 返回一条精确任务记录。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the stable host task identity.
    /// `execution_id` 是稳定宿主任务身份。
    ///
    /// # Returns
    /// 返回值
    /// The cloned record, or an explicit missing-task failure.
    /// 返回克隆记录，或明确任务缺失错误。
    pub fn require(&self, execution_id: &str) -> Result<ProviderAsyncTaskRecord, String> {
        self.records
            .lock()
            .map_err(|_| String::from("The provider task recovery lock is poisoned."))?
            .get(execution_id)
            .cloned()
            .ok_or_else(|| format!("Provider task execution `{execution_id}` does not exist."))
    }

    /// Inserts or replaces one complete record and publishes a new snapshot.
    /// 插入或替换一条完整记录并发布新快照。
    ///
    /// # Parameters
    /// 参数
    /// `record` is the complete new authoritative state.
    /// `record` 是完整的新权威状态。
    ///
    /// # Returns
    /// 返回值
    /// Success only after memory and configured durable storage agree.
    /// 仅在内存与已配置持久化存储一致后返回成功。
    pub fn replace(&self, record: ProviderAsyncTaskRecord) -> Result<(), String> {
        validate_provider_task_record(&record)?;
        let _mutation_guard = self
            .mutation_lock
            .lock()
            .map_err(|_| String::from("The provider task mutation lock is poisoned."))?;
        let execution_id = record.execution_id.clone();
        let telemetry_record = record.clone();
        let mut phase_changed = true;
        let previous_records = {
            let mut records = self
                .records
                .lock()
                .map_err(|_| String::from("The provider task recovery lock is poisoned."))?;
            if let Some(current) = records.get(execution_id.as_str()) {
                validate_provider_task_replacement(current, &record)?;
                phase_changed = current.phase != record.phase;
            }
            let previous_records = records.clone();
            let now_unix_ms = record.updated_at_unix_ms;
            records.insert(execution_id.clone(), record);
            prune_terminal_records(&mut records, now_unix_ms, Some(execution_id.as_str()));
            if records.len() > PROVIDER_TASK_MAXIMUM_RECORDS {
                *records = previous_records;
                return Err(format!(
                    "Provider task recovery capacity of {PROVIDER_TASK_MAXIMUM_RECORDS} records is exhausted; active tasks were preserved."
                ));
            }
            previous_records
        };
        if let Err(error) = self.persist() {
            let mut records = self
                .records
                .lock()
                .map_err(|_| String::from("The provider task recovery lock is poisoned."))?;
            *records = previous_records;
            return Err(error);
        }
        if phase_changed {
            emit_provider_async_task_telemetry(&telemetry_record);
        }
        Ok(())
    }

    /// Publishes the complete map through fixed current, previous, and staged files.
    /// 通过固定的当前、上一版与暂存文件发布完整映射。
    ///
    /// # Returns
    /// 返回值
    /// Success when no durable directory is configured or one new complete snapshot is visible.
    /// 未配置持久化目录或新完整快照可见时返回成功。
    fn persist(&self) -> Result<(), String> {
        let _publish_guard = self
            .publish_lock
            .lock()
            .map_err(|_| String::from("The provider task publication lock is poisoned."))?;
        let directory = self
            .directory
            .lock()
            .map_err(|_| String::from("The provider task directory lock is poisoned."))?
            .clone();
        let Some(directory) = directory else {
            return Ok(());
        };
        let mut revision = self
            .revision
            .lock()
            .map_err(|_| String::from("The provider task revision lock is poisoned."))?;
        let next_revision = revision
            .checked_add(1)
            .ok_or_else(|| String::from("The provider task recovery revision is exhausted."))?;
        let snapshot = ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: next_revision,
            records: self
                .records
                .lock()
                .map_err(|_| String::from("The provider task recovery lock is poisoned."))?
                .clone(),
        };
        let bytes = serde_json::to_vec_pretty(&snapshot)
            .map_err(|error| format!("Provider task recovery JSON encoding failed: {error}"))?;
        if bytes.len() as u64 > PROVIDER_TASK_SNAPSHOT_MAX_BYTES {
            return Err(format!(
                "Provider task recovery snapshot exceeds the {PROVIDER_TASK_SNAPSHOT_MAX_BYTES} byte limit."
            ));
        }
        let temporary = directory.join(PROVIDER_TASK_STAGED_SNAPSHOT_FILE);
        let published = directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE);
        let previous = directory.join(PROVIDER_TASK_PREVIOUS_SNAPSHOT_FILE);
        match fs::symlink_metadata(&temporary) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(format!(
                    "Provider task recovery staging path `{}` is not a regular file.",
                    temporary.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Provider task recovery staging path `{}` could not be inspected: {error}",
                    temporary.display()
                ));
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("Provider task recovery temporary file failed: {error}"))?;
        // writeResult is completed before cleanup so Windows no longer holds the temporary file.
        // writeResult 在清理前完成，使 Windows 不再持有临时文件。
        let write_result = file
            .write_all(bytes.as_slice())
            .and_then(|_| file.sync_all());
        drop(file);
        if let Err(error) = write_result {
            let cleanup = fs::remove_file(&temporary);
            return Err(format!(
                "Provider task recovery snapshot write failed: {error}; temporary cleanup: {cleanup:?}"
            ));
        }
        let published_exists = match fs::symlink_metadata(&published) {
            Ok(metadata) if metadata.file_type().is_file() => true,
            Ok(_) => {
                let cleanup = fs::remove_file(&temporary);
                return Err(format!(
                    "Provider task recovery current path `{}` is not a regular file; temporary cleanup: {cleanup:?}",
                    published.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                let cleanup = fs::remove_file(&temporary);
                return Err(format!(
                    "Provider task recovery current path `{}` could not be inspected: {error}; temporary cleanup: {cleanup:?}",
                    published.display()
                ));
            }
        };
        if published_exists {
            match fs::symlink_metadata(&previous) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    fs::remove_file(&previous).map_err(|error| {
                        let cleanup = fs::remove_file(&temporary);
                        format!(
                            "Provider task recovery previous snapshot `{}` could not be replaced: {error}; temporary cleanup: {cleanup:?}",
                            previous.display()
                        )
                    })?;
                }
                Ok(_) => {
                    let cleanup = fs::remove_file(&temporary);
                    return Err(format!(
                        "Provider task recovery previous path `{}` is not a regular file; temporary cleanup: {cleanup:?}",
                        previous.display()
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    let cleanup = fs::remove_file(&temporary);
                    return Err(format!(
                        "Provider task recovery previous path `{}` could not be inspected: {error}; temporary cleanup: {cleanup:?}",
                        previous.display()
                    ));
                }
            }
            fs::rename(&published, &previous).map_err(|error| {
                let cleanup = fs::remove_file(&temporary);
                format!(
                    "Provider task recovery current snapshot could not move to the exact rollback path: {error}; temporary cleanup: {cleanup:?}"
                )
            })?;
        }
        if let Err(error) = fs::rename(&temporary, &published) {
            let rollback = if published_exists {
                fs::rename(&previous, &published)
            } else {
                Ok(())
            };
            let cleanup = fs::remove_file(&temporary);
            return Err(format!(
                "Provider task recovery publish failed: {error}; rollback: {rollback:?}; temporary cleanup: {cleanup:?}"
            ));
        }
        *revision = next_revision;
        Ok(())
    }
}

/// Builds one non-secret structured event for a manifest-authoritative asynchronous task phase.
/// 为 Manifest 权威异步任务阶段构建一条非秘密结构化事件。
///
/// # Parameters
/// 参数
/// `record` is the exact state that already passed durable task validation.
/// `record` 是已经通过持久任务校验的精确状态。
///
/// # Returns
/// 返回值
/// A complete event for modern operation-bound records, or `None` for legacy records that never
/// persisted an operation identity.
/// 对现代 operation 绑定记录返回完整事件；对从未持久化 operation 身份的旧记录返回 `None`。
fn provider_async_task_telemetry_event(
    record: &ProviderAsyncTaskRecord,
) -> Option<crate::ProviderOperationalTelemetryEvent> {
    let operation_id = record.route.operation_id.as_ref()?;
    let mut event = crate::ProviderOperationalTelemetryEvent::new(
        crate::ProviderOperationalTelemetryEventName::ProviderAsyncTaskStateChanged,
    );
    event.provider_id = Some(record.route.provider_id.clone());
    event.service_entry_id = Some(record.route.service_entry_id.clone());
    event.connection_id = Some(record.route.connection_id.clone());
    event.manifest_revision = Some(record.route.manifest_revision);
    event.operation_id = Some(operation_id.clone());
    event.tool_target_id = Some(record.route.selection.target_id.clone());
    event.task_execution_id = Some(record.execution_id.clone());
    event.task_phase = Some(record.phase.as_str().to_string());
    event.request_profiles = record
        .route
        .frozen_request_profiles
        .iter()
        .map(|profile| crate::ProviderOperationalProfileRevision {
            profile_id: profile.profile_id.clone(),
            revision: profile.revision,
        })
        .collect();
    event.response_profiles = record
        .route
        .frozen_response_profiles
        .iter()
        .map(|profile| crate::ProviderOperationalProfileRevision {
            profile_id: profile.profile_id.clone(),
            revision: profile.revision,
        })
        .collect();
    event.failure_kind = match record.phase {
        ProviderAsyncTaskPhase::Failed => Some(String::from("provider_task_failed")),
        ProviderAsyncTaskPhase::Cancelled => Some(String::from("provider_task_cancelled")),
        ProviderAsyncTaskPhase::DispatchOutcomeUnknown => {
            Some(String::from("dispatch_outcome_unknown"))
        }
        _ => None,
    };
    Some(event)
}

/// Emits one distinct durable asynchronous task phase after successful state publication.
/// 在状态成功发布后发出一个不同的持久异步任务阶段。
///
/// # Parameters
/// 参数
/// `record` is the successfully persisted authoritative task state.
/// `record` 是已成功持久化的权威任务状态。
///
/// # Returns
/// 返回值
/// Returns no value because legacy omission or telemetry diagnostics cannot change task outcome.
/// 不返回值，因为旧记录缺失或遥测诊断不能改变任务结果。
fn emit_provider_async_task_telemetry(record: &ProviderAsyncTaskRecord) {
    let Some(event) = provider_async_task_telemetry_event(record) else {
        return;
    };
    if let Err(error) = crate::record_provider_operational_telemetry(event) {
        eprintln!("[provider-task] structured telemetry was rejected: {error}");
    }
}

/// Validates one complete task record before it enters memory or durable recovery state.
/// 在一条完整任务记录进入内存或持久化恢复状态前进行校验。
///
/// # Parameters
/// 参数
/// `record` is the complete non-secret task snapshot supplied by creation or recovery.
/// `record` 是由创建或恢复流程提供的完整非秘密任务快照。
///
/// # Returns
/// 返回值
/// Success only when identity affinity is self-consistent and every manifest-backed task retains
/// its complete dispatch-frozen route; otherwise an explicit fail-closed diagnostic.
/// 仅当身份亲和性自洽且每个 Manifest 驱动任务都保留完整分派冻结路由时成功；否则返回明确的
/// 封闭失败诊断。
fn validate_provider_task_record(record: &ProviderAsyncTaskRecord) -> Result<(), String> {
    validate_provider_task_identity("execution identifier", record.execution_id.as_str())?;
    if let Some(owner_session_id) = record.owner_session_id.as_deref() {
        validate_provider_task_identity("owner session identifier", owner_session_id)?;
    }
    validate_provider_task_identity("provider identifier", record.route.provider_id.as_str())?;
    validate_provider_task_identity(
        "service entry identifier",
        record.route.service_entry_id.as_str(),
    )?;
    validate_provider_task_identity("connection identifier", record.route.connection_id.as_str())?;
    validate_provider_task_identity(
        "selected connection identifier",
        record.route.selection.connection_id.as_str(),
    )?;
    validate_provider_task_identity(
        "selected target identifier",
        record.route.selection.target_id.as_str(),
    )?;
    if let Some(operation_id) = record.route.operation_id.as_deref() {
        validate_provider_task_identity("operation identifier", operation_id)?;
    }
    if let Some(provider_task_id) = record.provider_task_id.as_deref() {
        validate_provider_task_identity("provider task identifier", provider_task_id)?;
    }
    if let Some(error) = record.error.as_deref()
        && error.chars().count() > PROVIDER_TASK_DIAGNOSTIC_MAX_CHARACTERS
    {
        return Err(format!(
            "Provider task execution `{}` has a diagnostic exceeding the {PROVIDER_TASK_DIAGNOSTIC_MAX_CHARACTERS} character limit.",
            record.execution_id
        ));
    }
    if record.route.connection_id != record.route.selection.connection_id {
        return Err(format!(
            "Provider task execution `{}` has inconsistent connection affinity.",
            record.execution_id
        ));
    }
    if record.updated_at_unix_ms < record.created_at_unix_ms {
        return Err(format!(
            "Provider task execution `{}` has an update timestamp earlier than its creation timestamp.",
            record.execution_id
        ));
    }
    let lifecycle_duration_ms = record
        .deadline_unix_ms
        .checked_sub(record.created_at_unix_ms)
        .ok_or_else(|| {
            format!(
                "Provider task execution `{}` has a deadline earlier than its creation timestamp.",
                record.execution_id
            )
        })?;
    if !(crate::provider_manifest::MIN_ASYNC_POLL_INTERVAL_MS
        ..=crate::provider_manifest::MAX_ASYNC_POLL_OR_BACKOFF_MS)
        .contains(&record.poll_interval_ms)
        || lifecycle_duration_ms < record.poll_interval_ms
        || lifecycle_duration_ms > crate::provider_manifest::MAX_ASYNC_TIMEOUT_MS
    {
        return Err(format!(
            "Provider task execution `{}` has an out-of-bounds poll interval or lifecycle deadline.",
            record.execution_id
        ));
    }
    if record.consecutive_observation_failures > PROVIDER_TASK_MAXIMUM_OBSERVATION_FAILURES
        || (record.consecutive_observation_failures > 0 && record.error.is_none())
    {
        return Err(format!(
            "Provider task execution `{}` has inconsistent observation failure state.",
            record.execution_id
        ));
    }
    if matches!(
        record.phase,
        ProviderAsyncTaskPhase::Dispatching | ProviderAsyncTaskPhase::DispatchOutcomeUnknown
    ) && record.provider_task_id.is_some()
    {
        return Err(format!(
            "Provider task execution `{}` cannot retain a provider task identifier in phase `{:?}`.",
            record.execution_id, record.phase
        ));
    }
    if matches!(
        record.phase,
        ProviderAsyncTaskPhase::Queued
            | ProviderAsyncTaskPhase::Running
            | ProviderAsyncTaskPhase::CancellationRequested
            | ProviderAsyncTaskPhase::Completed
            | ProviderAsyncTaskPhase::Cancelled
    ) && record.provider_task_id.is_none()
    {
        return Err(format!(
            "Provider task execution `{}` in phase `{:?}` is missing its confirmed provider task identifier.",
            record.execution_id, record.phase
        ));
    }
    validate_provider_task_recovery_context(record)?;
    if record.route.manifest_revision > 0 && record.route.frozen_target.is_none() {
        return Err(format!(
            "Provider task execution `{}` at manifest revision {} is missing its dispatch-frozen target snapshot.",
            record.execution_id, record.route.manifest_revision
        ));
    }
    if record.route.manifest_revision > 0 && record.route.frozen_service_entry.is_none() {
        return Err(format!(
            "Provider task execution `{}` at manifest revision {} is missing its dispatch-frozen service-entry snapshot.",
            record.execution_id, record.route.manifest_revision
        ));
    }
    if let Some(frozen_target) = record.route.frozen_target.as_ref()
        && (frozen_target.provider_id != record.route.provider_id
            || frozen_target.target_id != record.route.selection.target_id
            || frozen_target.service_entry_id != record.route.service_entry_id
            || frozen_target.execution.adapter != record.route.adapter
            || frozen_target.execution.execution_kind
                != crate::ProviderToolExecutionKind::HttpJsonAsyncPoll)
    {
        return Err(format!(
            "Provider task execution `{}` has a frozen target that does not match its immutable route affinity.",
            record.execution_id
        ));
    }
    if let Some(frozen_entry) = record.route.frozen_service_entry.as_ref()
        && frozen_entry.id != record.route.service_entry_id
    {
        return Err(format!(
            "Provider task execution `{}` has a frozen service entry that does not match its immutable route affinity.",
            record.execution_id
        ));
    }
    if record.route.manifest_revision > 0 {
        validate_manifest_backed_provider_task(record)?;
    } else if let Some(operation) = record.route.frozen_operation.as_ref() {
        validate_frozen_provider_task_operation(record, operation)?;
    }
    Ok(())
}

/// Validates one bounded non-empty identity retained by durable task recovery.
/// 校验持久任务恢复保留的一个有界非空身份。
///
/// # Parameters
/// 参数
/// `label` scopes the diagnostic and `value` is the exact retained identity.
/// `label` 限定诊断范围，`value` 是精确保留的身份。
///
/// # Returns
/// 返回值
/// Success only for trimmed, control-free identities within the public persistence bound.
/// 仅对位于公开持久化边界内、无控制字符且已去除首尾空白的身份返回成功。
fn validate_provider_task_identity(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().any(char::is_control)
        || value.chars().count() > PROVIDER_TASK_IDENTITY_MAX_CHARACTERS
    {
        return Err(format!(
            "Provider task recovery {label} must be non-empty, trimmed, control-free, and within {PROVIDER_TASK_IDENTITY_MAX_CHARACTERS} characters."
        ));
    }
    Ok(())
}

/// Validates bounded non-secret adapter recovery metadata.
/// 校验有界且不含秘密的适配器恢复元数据。
///
/// # Parameters
/// 参数
/// `record` owns the exact recovery context written after task confirmation.
/// `record` 持有任务确认后写入的精确恢复上下文。
///
/// # Returns
/// 返回值
/// Success only for a bounded ordered list of canonical lowercase SHA-256 fingerprints.
/// 仅对有界且由规范小写 SHA-256 指纹组成的有序列表返回成功。
fn validate_provider_task_recovery_context(record: &ProviderAsyncTaskRecord) -> Result<(), String> {
    let ProviderAsyncTaskRecoveryContext::OrderedSha256 { fingerprints } = &record.recovery else {
        return Ok(());
    };
    if fingerprints.is_empty()
        || fingerprints.len() > PROVIDER_TASK_MAXIMUM_RECOVERY_FINGERPRINTS
        || fingerprints.iter().any(|fingerprint| {
            fingerprint.len() != 64
                || !fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    {
        return Err(format!(
            "Provider task execution `{}` has invalid ordered SHA-256 recovery fingerprints.",
            record.execution_id
        ));
    }
    Ok(())
}

/// Requires every modern manifest-backed task to retain its complete frozen operation contract.
/// 要求每个现代 Manifest 驱动任务保留完整冻结操作契约。
///
/// # Parameters
/// 参数
/// `record` is a task whose nonzero manifest revision forbids legacy recovery reconstruction.
/// `record` 是一条非零 Manifest 修订禁止旧式恢复重建的任务。
///
/// # Returns
/// 返回值
/// Success only when target, operation, profiles, timing, and cancellation authority are closed.
/// 仅当目标、操作、Profile、时间与取消权限全部闭合时返回成功。
fn validate_manifest_backed_provider_task(record: &ProviderAsyncTaskRecord) -> Result<(), String> {
    let operation_id = record.route.operation_id.as_deref().ok_or_else(|| {
        format!(
            "Provider task execution `{}` is missing its frozen operation identifier.",
            record.execution_id
        )
    })?;
    let operation = record.route.frozen_operation.as_ref().ok_or_else(|| {
        format!(
            "Provider task execution `{}` is missing its frozen operation declaration.",
            record.execution_id
        )
    })?;
    if operation.operation_id != operation_id {
        return Err(format!(
            "Provider task execution `{}` has inconsistent frozen operation identity.",
            record.execution_id
        ));
    }
    let frozen_entry = record.route.frozen_service_entry.as_ref().ok_or_else(|| {
        format!(
            "Provider task execution `{}` is missing its frozen service-entry declaration.",
            record.execution_id
        )
    })?;
    let frozen_target = record.route.frozen_target.as_ref().ok_or_else(|| {
        format!(
            "Provider task execution `{}` is missing its frozen target declaration.",
            record.execution_id
        )
    })?;
    crate::provider_manifest::validate_frozen_async_task_route_contract(
        record.route.provider_id.as_str(),
        frozen_entry,
        frozen_target,
        operation,
    )
    .map_err(|error| {
        format!(
            "Provider task execution `{}` has an invalid frozen route contract: {error}",
            record.execution_id
        )
    })?;
    validate_frozen_provider_task_operation(record, operation)
}

/// Validates one retained operation against the task route and lifecycle fields.
/// 根据任务路由与生命周期字段校验一条保留操作。
///
/// # Parameters
/// 参数
/// `record` owns the durable route and `operation` is its dispatch-frozen declaration.
/// `record` 持有持久路由，`operation` 是其分派时冻结的声明。
///
/// # Returns
/// 返回值
/// Success only when the asynchronous operation exactly explains every retained runtime field.
/// 仅当异步操作可精确解释每个保留运行时字段时返回成功。
fn validate_frozen_provider_task_operation(
    record: &ProviderAsyncTaskRecord,
    operation: &crate::provider_manifest::OperationDeclaration,
) -> Result<(), String> {
    if record.route.operation_id.as_deref() != Some(operation.operation_id.as_str())
        || !operation
            .tool_target_refs
            .iter()
            .any(|target_id| target_id == &record.route.selection.target_id)
    {
        return Err(format!(
            "Provider task execution `{}` has a frozen operation outside its target affinity.",
            record.execution_id
        ));
    }
    let async_execution = operation.async_execution.as_ref().ok_or_else(|| {
        format!(
            "Provider task execution `{}` has no frozen asynchronous lifecycle declaration.",
            record.execution_id
        )
    })?;
    let maximum_task_deadline_unix_ms = record
        .created_at_unix_ms
        .checked_add(async_execution.timeout_ms)
        .ok_or_else(|| {
            format!(
                "Provider task execution `{}` has an overflowing frozen asynchronous deadline.",
                record.execution_id
            )
        })?;
    if async_execution.generic_json.is_none()
        || async_execution.poll_interval_ms != record.poll_interval_ms
        || async_execution.max_backoff_ms < async_execution.poll_interval_ms
        || async_execution.max_backoff_ms > crate::provider_manifest::MAX_ASYNC_POLL_OR_BACKOFF_MS
        || async_execution.timeout_ms < async_execution.poll_interval_ms
        || async_execution.timeout_ms > crate::provider_manifest::MAX_ASYNC_TIMEOUT_MS
        // The parent operation deadline starts before credential and upload preparation. A task
        // may therefore retain less than its manifest lifecycle timeout, but it may never extend it.
        // 父 operation 截止时间早于凭据与上传准备开始。因此任务可保留短于 Manifest 生命周期
        // 超时的预算，但绝不能延长该预算。
        || record.deadline_unix_ms > maximum_task_deadline_unix_ms
        || (record.cancellation_supported && !async_execution.cancellation_supported)
    {
        return Err(format!(
            "Provider task execution `{}` has lifecycle state inconsistent with its frozen operation.",
            record.execution_id
        ));
    }
    let request_profiles_match = operation.request_profile_refs.len()
        == record.route.frozen_request_profiles.len()
        && operation
            .request_profile_refs
            .iter()
            .zip(record.route.frozen_request_profiles.iter())
            .all(|(profile_id, profile)| profile_id == &profile.profile_id);
    let response_profiles_match = operation.response_profile_refs.len()
        == record.route.frozen_response_profiles.len()
        && operation
            .response_profile_refs
            .iter()
            .zip(record.route.frozen_response_profiles.iter())
            .all(|(profile_id, profile)| profile_id == &profile.profile_id);
    if !request_profiles_match || !response_profiles_match {
        return Err(format!(
            "Provider task execution `{}` has frozen profiles inconsistent with its operation references.",
            record.execution_id
        ));
    }
    Ok(())
}

/// Validates immutable affinity and monotonic lifecycle movement for one task replacement.
/// 验证一次任务替换的不可变亲和性与单调生命周期变化。
///
/// # Parameters
/// 参数
/// `current` is the authoritative retained record and `replacement` is one caller snapshot.
/// `current` 是权威保留记录，`replacement` 是一份调用方快照。
///
/// # Returns
/// 返回值
/// Success only when concurrent polling or cancellation cannot regress authoritative state.
/// 仅在并发轮询或取消不会回退权威状态时返回成功。
fn validate_provider_task_replacement(
    current: &ProviderAsyncTaskRecord,
    replacement: &ProviderAsyncTaskRecord,
) -> Result<(), String> {
    if current.execution_id != replacement.execution_id
        || current.owner_session_id != replacement.owner_session_id
        || current.route != replacement.route
        || current.created_at_unix_ms != replacement.created_at_unix_ms
        || current.deadline_unix_ms != replacement.deadline_unix_ms
        || current.poll_interval_ms != replacement.poll_interval_ms
    {
        return Err(format!(
            "Provider task execution `{}` attempted to change immutable recovery affinity.",
            current.execution_id
        ));
    }
    if let Some(current_task_id) = current.provider_task_id.as_deref()
        && replacement.provider_task_id.as_deref() != Some(current_task_id)
    {
        return Err(format!(
            "Provider task execution `{}` attempted to replace its confirmed provider task identifier.",
            current.execution_id
        ));
    }
    if current.provider_task_id.is_some()
        && (current.recovery != replacement.recovery
            || current.cancellation_supported != replacement.cancellation_supported)
    {
        return Err(format!(
            "Provider task execution `{}` attempted to change confirmed recovery metadata.",
            current.execution_id
        ));
    }
    if !provider_task_phase_transition_is_monotonic(current.phase, replacement.phase) {
        return Err(format!(
            "Provider task execution `{}` rejected stale phase transition from `{:?}` to `{:?}`.",
            current.execution_id, current.phase, replacement.phase
        ));
    }
    Ok(())
}

/// Reports whether one provider-task phase transition preserves terminal and cancellation intent.
/// 报告一次供应商任务阶段转换是否保留终态与取消意图。
///
/// # Parameters
/// 参数
/// `current` is the authoritative phase and `replacement` is the proposed next phase.
/// `current` 是权威阶段，`replacement` 是建议的下一阶段。
///
/// # Returns
/// 返回值
/// `true` only for same-phase updates or forward lifecycle transitions.
/// 仅对同阶段更新或向前生命周期转换返回 `true`。
fn provider_task_phase_transition_is_monotonic(
    current: ProviderAsyncTaskPhase,
    replacement: ProviderAsyncTaskPhase,
) -> bool {
    if current == replacement {
        return true;
    }
    match current {
        ProviderAsyncTaskPhase::Dispatching => matches!(
            replacement,
            ProviderAsyncTaskPhase::Queued
                | ProviderAsyncTaskPhase::CancellationRequested
                | ProviderAsyncTaskPhase::Failed
                | ProviderAsyncTaskPhase::Cancelled
                | ProviderAsyncTaskPhase::DispatchOutcomeUnknown
        ),
        ProviderAsyncTaskPhase::Queued => matches!(
            replacement,
            ProviderAsyncTaskPhase::Running
                | ProviderAsyncTaskPhase::CancellationRequested
                | ProviderAsyncTaskPhase::Completed
                | ProviderAsyncTaskPhase::Failed
                | ProviderAsyncTaskPhase::Cancelled
                | ProviderAsyncTaskPhase::DispatchOutcomeUnknown
        ),
        ProviderAsyncTaskPhase::Running => matches!(
            replacement,
            ProviderAsyncTaskPhase::CancellationRequested
                | ProviderAsyncTaskPhase::Completed
                | ProviderAsyncTaskPhase::Failed
                | ProviderAsyncTaskPhase::Cancelled
                | ProviderAsyncTaskPhase::DispatchOutcomeUnknown
        ),
        ProviderAsyncTaskPhase::CancellationRequested => matches!(
            replacement,
            ProviderAsyncTaskPhase::Completed
                | ProviderAsyncTaskPhase::Failed
                | ProviderAsyncTaskPhase::Cancelled
                | ProviderAsyncTaskPhase::DispatchOutcomeUnknown
        ),
        ProviderAsyncTaskPhase::Completed
        | ProviderAsyncTaskPhase::Failed
        | ProviderAsyncTaskPhase::Cancelled
        | ProviderAsyncTaskPhase::DispatchOutcomeUnknown => false,
    }
}

/// Removes expired terminal records and bounds the remaining terminal history.
/// 删除已过期终态记录并限制剩余终态历史数量。
///
/// # Parameters
/// 参数
/// `records` is the authoritative complete map, `now_unix_ms` is host time, and
/// `protected_execution_id` identifies the just-mutated record that must survive this pass.
/// `records` 是权威完整映射，`now_unix_ms` 是宿主时间，
/// `protected_execution_id` 标识本轮必须保留的刚变更记录。
///
/// # Returns
/// 返回值
/// This function mutates the map in place and never removes active records.
/// 本函数原地修改映射且绝不删除活动记录。
fn prune_terminal_records(
    records: &mut BTreeMap<String, ProviderAsyncTaskRecord>,
    now_unix_ms: u64,
    protected_execution_id: Option<&str>,
) {
    records.retain(|execution_id, record| {
        !record.phase.is_terminal()
            || protected_execution_id == Some(execution_id.as_str())
            || now_unix_ms.saturating_sub(record.updated_at_unix_ms)
                <= PROVIDER_TASK_TERMINAL_RETENTION_MS
    });
    let mut terminal_records = records
        .iter()
        .filter(|(_, record)| record.phase.is_terminal())
        .map(|(execution_id, record)| (record.updated_at_unix_ms, execution_id.clone()))
        .collect::<Vec<_>>();
    terminal_records.sort();
    let removal_count = terminal_records
        .len()
        .saturating_sub(PROVIDER_TASK_MAXIMUM_TERMINAL_RECORDS);
    for (_, execution_id) in terminal_records
        .into_iter()
        .filter(|(_, execution_id)| protected_execution_id != Some(execution_id.as_str()))
        .take(removal_count)
    {
        records.remove(execution_id.as_str());
    }
}

/// Returns the current Unix timestamp in milliseconds for retention decisions.
/// 返回用于保留策略判断的当前 Unix 毫秒时间戳。
///
/// # Returns
/// 返回值
/// A saturating millisecond timestamp or an explicit clock failure.
/// 返回饱和毫秒时间戳或显式时钟失败。
fn current_unix_millis() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Provider task recovery clock is invalid: {error}"))?;
    Ok(duration.as_millis().try_into().unwrap_or(u64::MAX))
}

/// Loads the newest complete strict snapshot from two exact recovery paths.
/// 从两个精确恢复路径加载最新的完整严格快照。
fn load_latest_snapshot(directory: &Path) -> Result<ProviderTaskRecoverySnapshot, String> {
    let current =
        load_provider_task_snapshot_file(&directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE))?;
    let previous =
        load_provider_task_snapshot_file(&directory.join(PROVIDER_TASK_PREVIOUS_SNAPSHOT_FILE))?;
    match (current, previous) {
        // Missing fixed files mean no recoverable Provider task exists. Startup must not
        // enumerate legacy random-name files or infer an authoritative record from siblings.
        // 固定文件缺失表示不存在可恢复的 Provider 任务。启动不得枚举旧随机名称文件，
        // 也不得从同级文件推断权威记录。
        (None, None) => Ok(ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: 0,
            records: BTreeMap::new(),
        }),
        (Some(snapshot), None) | (None, Some(snapshot)) => Ok(snapshot),
        (Some(current), Some(previous)) => {
            if current.revision == previous.revision && current != previous {
                return Err(format!(
                    "Provider task recovery current and previous snapshots disagree at revision {}.",
                    current.revision
                ));
            }
            if current.revision >= previous.revision {
                Ok(current)
            } else {
                Ok(previous)
            }
        }
    }
}

/// Loads and validates one exact provider-task snapshot file when it exists.
/// 在存在时加载并校验一个精确的供应商任务快照文件。
///
/// # Parameters
/// 参数
/// `path` is either the fixed current or fixed rollback snapshot path.
/// `path` 是固定当前快照路径或固定回滚快照路径。
///
/// # Returns
/// 返回值
/// A validated snapshot, clean absence, or an explicit bounded storage error.
/// 返回已校验快照、干净缺失或明确的有界存储错误。
fn load_provider_task_snapshot_file(
    path: &Path,
) -> Result<Option<ProviderTaskRecoverySnapshot>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Provider task recovery snapshot `{}` could not be inspected: {error}",
                path.display()
            ));
        }
    };
    if !metadata.file_type().is_file() {
        return Err(format!(
            "Provider task recovery snapshot `{}` is not a regular file.",
            path.display()
        ));
    }
    if metadata.len() > PROVIDER_TASK_SNAPSHOT_MAX_BYTES {
        return Err(format!(
            "Provider task recovery snapshot `{}` exceeds the {PROVIDER_TASK_SNAPSHOT_MAX_BYTES} byte limit.",
            path.display()
        ));
    }
    let file = fs::File::open(path).map_err(|error| {
        format!(
            "Provider task recovery snapshot `{}` could not be opened: {error}",
            path.display()
        )
    })?;
    let mut limited = file.take(PROVIDER_TASK_SNAPSHOT_MAX_BYTES.saturating_add(1));
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).map_err(|error| {
        format!(
            "Provider task recovery snapshot `{}` could not be read: {error}",
            path.display()
        )
    })?;
    if bytes.len() as u64 > PROVIDER_TASK_SNAPSHOT_MAX_BYTES {
        return Err(format!(
            "Provider task recovery snapshot `{}` changed while reading and exceeds the {PROVIDER_TASK_SNAPSHOT_MAX_BYTES} byte limit.",
            path.display()
        ));
    }
    let snapshot_value: serde_json::Value =
        serde_json::from_slice(bytes.as_slice()).map_err(|error| {
            format!(
                "Provider task recovery snapshot `{}` is invalid: {error}",
                path.display()
            )
        })?;
    crate::provider_manifest::validate_runtime_json_bounds(
        &snapshot_value,
        "Provider task recovery snapshot",
    )?;
    let snapshot: ProviderTaskRecoverySnapshot =
        serde_json::from_value(snapshot_value).map_err(|error| {
            format!(
                "Provider task recovery snapshot `{}` has an invalid structure: {error}",
                path.display()
            )
        })?;
    if snapshot.version != PROVIDER_TASK_RECOVERY_VERSION {
        return Err(format!(
            "Unsupported provider task recovery version {} in `{}`.",
            snapshot.version,
            path.display()
        ));
    }
    if snapshot.revision == 0 {
        return Err(format!(
            "Provider task recovery snapshot `{}` cannot declare revision zero.",
            path.display()
        ));
    }
    if snapshot.records.len() > PROVIDER_TASK_MAXIMUM_RECORDS {
        return Err(format!(
            "Provider task recovery snapshot `{}` exceeds the {PROVIDER_TASK_MAXIMUM_RECORDS} record limit.",
            path.display()
        ));
    }
    for (execution_id, record) in &snapshot.records {
        if execution_id != &record.execution_id {
            return Err(format!(
                "Provider task recovery snapshot `{}` maps execution `{execution_id}` to record `{}`.",
                path.display(),
                record.execution_id
            ));
        }
        validate_provider_task_record(record).map_err(|error| {
            format!(
                "Provider task recovery snapshot `{}` contains an invalid record: {error}",
                path.display()
            )
        })?;
    }
    Ok(Some(snapshot))
}

#[cfg(test)]
mod tests {
    //! Strict recovery-store tests.
    //! 严格恢复存储测试。

    use super::*;
    use crate::{ProviderToolGeneratedResource, ProviderToolNormalizedOutput};
    /// Builds one deterministic task record without provider secrets.
    /// 构建一条不含供应商秘密的确定性任务记录。
    fn record(execution_id: &str) -> ProviderAsyncTaskRecord {
        ProviderAsyncTaskRecord {
            execution_id: execution_id.to_string(),
            owner_session_id: Some(String::from("session-one")),
            route: ProviderAsyncTaskRoute {
                provider_id: String::from("alibaba"),
                service_entry_id: String::from("model-studio-cn"),
                connection_id: String::from("connection-one"),
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("connection-one"),
                    target_id: String::from("wan2.7-t2v"),
                },
                adapter: ProviderToolAdapterKind::GenericJson,
                manifest_revision: 0,
                frozen_target: None,
                frozen_service_entry: None,
                operation_id: None,
                frozen_operation: None,
                frozen_request_profiles: vec![crate::provider_manifest::RequestProjectionProfile {
                    profile_id: String::from("operation.request.v1"),
                    revision: 3,
                    steps: Vec::new(),
                }],
                frozen_response_profiles: vec![
                    crate::provider_manifest::ResponseProjectionProfile {
                        profile_id: String::from("operation.response.v1"),
                        revision: 4,
                        steps: Vec::new(),
                        transient_reasoning_status: None,
                    },
                ],
            },
            provider_task_id: Some(String::from("task-one")),
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Queued,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            deadline_unix_ms: 60_001,
            poll_interval_ms: 15_000,
            consecutive_observation_failures: 0,
            cancellation_supported: true,
            error: None,
            result: None,
        }
    }

    /// Builds one complete modern manifest-backed asynchronous recovery record.
    /// 构建一条完整的现代 Manifest 驱动异步恢复记录。
    fn manifest_record(execution_id: &str) -> ProviderAsyncTaskRecord {
        let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
        let manifest = catalog
            .manifest("alibaba")
            .expect("bundled Alibaba manifest should exist");
        let target = manifest
            .tool_targets
            .iter()
            .find(|target| {
                target.execution.execution_kind
                    == crate::ProviderToolExecutionKind::HttpJsonAsyncPoll
            })
            .cloned()
            .expect("bundled Alibaba manifest should expose one asynchronous target");
        let operation = manifest
            .operations
            .iter()
            .find(|operation| {
                operation
                    .tool_target_refs
                    .iter()
                    .any(|target_id| target_id == &target.target_id)
            })
            .cloned()
            .expect("asynchronous target should have one owning operation");
        let frozen_service_entry = manifest
            .descriptor
            .service_entries
            .iter()
            .find(|entry| entry.id == target.service_entry_id)
            .cloned()
            .expect("asynchronous target should have one owning service entry");
        let async_execution = operation
            .async_execution
            .as_ref()
            .expect("asynchronous operation should retain its lifecycle");
        let task_timeout_ms = async_execution.timeout_ms;
        let poll_interval_ms = async_execution.poll_interval_ms;
        let cancellation_supported = async_execution.cancellation_supported;
        let created_at_unix_ms = 1_000;
        ProviderAsyncTaskRecord {
            execution_id: execution_id.to_string(),
            owner_session_id: Some(String::from("session-modern")),
            route: ProviderAsyncTaskRoute {
                provider_id: target.provider_id.clone(),
                service_entry_id: target.service_entry_id.clone(),
                connection_id: String::from("connection-modern"),
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("connection-modern"),
                    target_id: target.target_id.clone(),
                },
                adapter: target.execution.adapter,
                manifest_revision: manifest.revision,
                frozen_target: Some(target),
                frozen_service_entry: Some(frozen_service_entry),
                operation_id: Some(operation.operation_id.clone()),
                frozen_request_profiles: manifest
                    .request_profiles_for_operation(Some(&operation))
                    .expect("operation request profiles should resolve"),
                frozen_response_profiles: manifest
                    .response_profiles_for_operation(Some(&operation))
                    .expect("operation response profiles should resolve"),
                frozen_operation: Some(operation),
            },
            provider_task_id: Some(String::from("provider-task-modern")),
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Queued,
            created_at_unix_ms,
            updated_at_unix_ms: created_at_unix_ms,
            deadline_unix_ms: created_at_unix_ms + task_timeout_ms,
            poll_interval_ms,
            consecutive_observation_failures: 0,
            cancellation_supported,
            error: None,
            result: None,
        }
    }

    /// Verifies modern task recovery is explained completely by its frozen operation contract.
    /// 验证现代任务恢复可由其冻结操作契约完整解释。
    #[test]
    fn manifest_task_requires_complete_frozen_lifecycle_contract() {
        let valid = manifest_record("execution-modern-valid");
        validate_provider_task_record(&valid).expect("complete modern record should validate");

        let mut missing_operation = valid.clone();
        missing_operation.execution_id = String::from("execution-modern-missing-operation");
        missing_operation.route.frozen_operation = None;
        let error = validate_provider_task_record(&missing_operation)
            .expect_err("modern recovery without an operation must fail closed");
        assert!(error.contains("frozen operation declaration"), "{error}");

        let mut missing_service_entry = valid.clone();
        missing_service_entry.execution_id = String::from("execution-modern-missing-service-entry");
        missing_service_entry.route.frozen_service_entry = None;
        let error = validate_provider_task_record(&missing_service_entry)
            .expect_err("modern recovery without a service entry must fail closed");
        assert!(error.contains("service-entry snapshot"), "{error}");

        let mut changed_poll = valid.clone();
        changed_poll.execution_id = String::from("execution-modern-changed-poll");
        changed_poll.poll_interval_ms += 1;
        let error = validate_provider_task_record(&changed_poll)
            .expect_err("poll interval drift must fail closed");
        assert!(error.contains("lifecycle state"), "{error}");

        let mut shortened_deadline = valid.clone();
        shortened_deadline.execution_id = String::from("execution-modern-shortened-deadline");
        shortened_deadline.deadline_unix_ms -= 1;
        validate_provider_task_record(&shortened_deadline)
            .expect("credential and upload preparation may consume the parent deadline");

        let mut expanded_deadline = valid.clone();
        expanded_deadline.execution_id = String::from("execution-modern-expanded-deadline");
        expanded_deadline.deadline_unix_ms += 1;
        let error = validate_provider_task_record(&expanded_deadline)
            .expect_err("persisted task deadline cannot extend the frozen lifecycle");
        assert!(error.contains("lifecycle state"), "{error}");

        let mut expanded_cancellation = valid;
        expanded_cancellation.execution_id = String::from("execution-modern-cancellation");
        if let Some(operation) = expanded_cancellation.route.frozen_operation.as_mut()
            && let Some(async_execution) = operation.async_execution.as_mut()
        {
            async_execution.cancellation_supported = false;
        }
        expanded_cancellation.cancellation_supported = true;
        let error = validate_provider_task_record(&expanded_cancellation)
            .expect_err("persisted cancellation authority cannot exceed the frozen operation");
        assert!(error.contains("lifecycle state"), "{error}");
    }

    /// Verifies durable route corruption cannot bypass Manifest transport and header authority.
    /// 验证持久路由损坏无法绕过 Manifest 的传输与请求头权限边界。
    #[test]
    fn manifest_task_revalidates_frozen_transport_authority() {
        // hostile_path would reinterpret the concatenated task URL authority without the
        // same-origin endpoint validator used during Manifest compilation.
        // hostile_path 若缺少 Manifest 编译时使用的同源端点验证器，会重新解释拼接后任务 URL
        // 的 authority。
        let mut hostile_path = manifest_record("execution-modern-hostile-path");
        let lifecycle = hostile_path
            .route
            .frozen_operation
            .as_mut()
            .and_then(|operation| operation.async_execution.as_mut())
            .and_then(|execution| execution.generic_json.as_mut())
            .expect("modern task should retain a generic lifecycle");
        lifecycle.poll_endpoint_path = String::from("@credential-exfil.example/task");
        let error = validate_provider_task_record(&hostile_path)
            .expect_err("a recovered cross-authority poll path must fail closed");
        assert!(error.contains("invalid_endpoint_path"), "{error}");

        // insecure_origin proves a system task cannot downgrade its frozen service entry to HTTP.
        // insecure_origin 证明系统任务不能把冻结服务入口降级为 HTTP。
        let mut insecure_origin = manifest_record("execution-modern-insecure-origin");
        insecure_origin
            .route
            .frozen_service_entry
            .as_mut()
            .expect("modern task should retain its service entry")
            .base_url = String::from("http://api.example.com");
        let error = validate_provider_task_record(&insecure_origin)
            .expect_err("a recovered system HTTP origin must fail closed");
        assert!(error.contains("invalid_url_scheme"), "{error}");

        // protected_header proves recovered fixed headers receive the same engine-authority check.
        // protected_header 证明恢复的固定请求头会接受同一引擎权限校验。
        let mut protected_header = manifest_record("execution-modern-protected-header");
        protected_header
            .route
            .frozen_target
            .as_mut()
            .expect("modern task should retain its target")
            .execution
            .fixed_headers
            .insert(
                String::from("host"),
                String::from("credential-exfil.example"),
            );
        let error = validate_provider_task_record(&protected_header)
            .expect_err("a recovered engine-owned header must fail closed");
        assert!(
            error.contains("invalid_provider_tool_fixed_header_name"),
            "{error}"
        );
    }

    /// Verifies recovery fingerprints and transient observation backoff remain bounded.
    /// 验证恢复指纹与瞬时观测退避保持有界。
    #[test]
    fn recovery_fingerprints_and_observation_backoff_are_bounded() {
        let mut task = manifest_record("execution-modern-backoff");
        let base = task.poll_interval_ms;
        let maximum = task
            .route
            .frozen_operation
            .as_ref()
            .and_then(|operation| operation.async_execution.as_ref())
            .map(|declaration| declaration.max_backoff_ms)
            .expect("modern operation should declare a backoff ceiling");
        assert_eq!(provider_task_observation_delay_ms(&task), base);
        task.consecutive_observation_failures = 1;
        task.error = Some(String::from("transient"));
        assert_eq!(
            provider_task_observation_delay_ms(&task),
            base.saturating_mul(2).min(maximum)
        );
        task.consecutive_observation_failures = PROVIDER_TASK_MAXIMUM_OBSERVATION_FAILURES;
        assert_eq!(provider_task_observation_delay_ms(&task), maximum);

        task.consecutive_observation_failures = 0;
        task.error = None;
        task.recovery = ProviderAsyncTaskRecoveryContext::OrderedSha256 {
            fingerprints: vec!["A".repeat(64)],
        };
        let error = validate_provider_task_record(&task)
            .expect_err("non-canonical recovery fingerprints must fail closed");
        assert!(error.contains("SHA-256"), "{error}");
    }

    /// Verifies task telemetry freezes operation, manifest, target, and profile identity without provider task ids.
    /// 验证任务遥测冻结 operation、Manifest、目标与 Profile 身份，且不包含供应商任务标识。
    #[test]
    fn async_task_telemetry_retains_only_frozen_non_secret_identity() {
        let mut task = record("execution-telemetry");
        task.route.manifest_revision = 19;
        task.route.operation_id = Some(String::from("video.generate"));
        let event = provider_async_task_telemetry_event(&task)
            .expect("operation-bound task should produce telemetry");
        event.validate().expect("task event should validate");
        assert_eq!(event.manifest_revision, Some(19));
        assert_eq!(event.operation_id.as_deref(), Some("video.generate"));
        assert_eq!(event.tool_target_id.as_deref(), Some("wan2.7-t2v"));
        assert_eq!(event.task_phase.as_deref(), Some("queued"));
        assert_eq!(event.request_profiles[0].revision, 3);
        assert_eq!(event.response_profiles[0].revision, 4);
        let serialized = serde_json::to_string(&event).expect("event should serialize");
        assert!(!serialized.contains("task-one"));
        assert!(!serialized.contains("session-one"));
    }

    /// Verifies a published snapshot reloads without retaining any secret field.
    /// 验证已发布快照可重新加载且不保留任何秘密字段。
    #[test]
    fn recovery_store_round_trips_strict_non_secret_records() {
        let directory =
            std::env::temp_dir().join(format!("vulcan-provider-task-test-{}", Uuid::new_v4()));
        let store = ProviderTaskRecoveryStore::default();
        store.configure(&directory).expect("store should configure");
        store
            .replace(record("execution-one"))
            .expect("record should persist");
        let reloaded = ProviderTaskRecoveryStore::default();
        let records = reloaded
            .configure(&directory)
            .expect("snapshot should reload");
        assert_eq!(records, vec![record("execution-one")]);
        let document = fs::read_to_string(directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE))
            .expect("snapshot should be readable");
        assert!(!document.contains("secret"));
        assert!(document.contains("operation.request.v1"));
        assert!(document.contains("operation.response.v1"));
        fs::remove_dir_all(directory).expect("temporary directory should be removable");
    }

    /// Verifies random-name legacy snapshots are ignored when exact recovery files are absent.
    /// 验证精确恢复文件缺失时会忽略随机名称旧快照。
    #[test]
    fn recovery_store_does_not_scan_legacy_snapshot_names() {
        let directory = std::env::temp_dir().join(format!(
            "vulcan-provider-task-legacy-migration-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).expect("legacy migration directory should create");
        let legacy_snapshot = ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: 7,
            records: BTreeMap::from([(
                String::from("execution-legacy"),
                record("execution-legacy"),
            )]),
        };
        let legacy_path = directory.join(format!(
            "snapshot-00000000000000000007-{}.json",
            Uuid::new_v4()
        ));
        fs::write(
            &legacy_path,
            serde_json::to_vec(&legacy_snapshot).expect("legacy snapshot should encode"),
        )
        .expect("legacy snapshot should write");

        let first_store = ProviderTaskRecoveryStore::default();
        let recovered = first_store
            .configure(&directory)
            .expect("missing exact snapshots should initialize an empty store");
        assert!(recovered.is_empty());
        assert!(
            directory
                .join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE)
                .is_file()
        );

        fs::write(&legacy_path, b"corrupted legacy file")
            .expect("legacy file should become invalid after exact initialization");
        let second_store = ProviderTaskRecoveryStore::default();
        let reloaded = second_store
            .configure(&directory)
            .expect("fixed locator must bypass corrupted legacy files");
        assert!(reloaded.is_empty());
        fs::remove_dir_all(directory).expect("legacy migration directory should be removable");
    }

    /// Verifies nonzero manifest revisions cannot recover through the legacy current-target path.
    /// 验证非零 Manifest 修订不能通过旧版当前目标路径恢复。
    #[test]
    fn manifest_task_without_frozen_target_fails_closed_on_write_and_load() {
        // invalid_record simulates a structurally valid but cross-generation recovery snapshot.
        // invalid_record 模拟结构有效但会跨代际恢复的快照。
        let mut invalid_record = record("execution-missing-frozen-target");
        invalid_record.route.manifest_revision = 7;
        // direct_store proves new in-process mutations cannot publish the invalid record.
        // direct_store 证明新的进程内变更不能发布无效记录。
        let direct_store = ProviderTaskRecoveryStore::default();
        let direct_error = direct_store
            .replace(invalid_record.clone())
            .expect_err("nonzero revision without frozen target must fail before persistence");
        assert!(direct_error.contains("dispatch-frozen target"));

        // directory owns one injected durable snapshot used to exercise startup recovery.
        // directory 持有一份注入的持久化快照，用于验证启动恢复。
        let directory =
            std::env::temp_dir().join(format!("vulcan-provider-task-invalid-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("temporary directory should create");
        // snapshot preserves valid JSON and version fields while violating only the frozen route invariant.
        // snapshot 保持 JSON 与版本字段有效，仅违反冻结路由不变量。
        let snapshot = ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: 1,
            records: BTreeMap::from([(invalid_record.execution_id.clone(), invalid_record)]),
        };
        // snapshot_path is the exact production current-snapshot locator.
        // snapshot_path 是生产环境使用的精确当前快照定位器。
        let snapshot_path = directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE);
        fs::write(
            &snapshot_path,
            serde_json::to_vec_pretty(&snapshot).expect("injected snapshot should encode"),
        )
        .expect("injected snapshot should write");
        // recovered_store must reject the semantic mismatch instead of entering the legacy path.
        // recovered_store 必须拒绝语义不匹配，而不是进入旧版路径。
        let recovered_store = ProviderTaskRecoveryStore::default();
        let recovery_error = recovered_store
            .configure(&directory)
            .expect_err("invalid durable snapshot must fail closed");
        assert!(recovery_error.contains("dispatch-frozen target"));
        fs::remove_dir_all(directory).expect("temporary directory should be removable");
    }

    /// Verifies generated bytes and temporary signed URLs never enter durable recovery snapshots.
    /// 验证生成字节与临时签名 URL 绝不会进入持久化恢复快照。
    #[test]
    fn recovery_store_excludes_process_local_terminal_results() {
        let directory =
            std::env::temp_dir().join(format!("vulcan-provider-task-test-{}", Uuid::new_v4()));
        let store = ProviderTaskRecoveryStore::default();
        store.configure(&directory).expect("store should configure");
        let mut completed = record("execution-with-result");
        completed.phase = ProviderAsyncTaskPhase::Completed;
        completed.updated_at_unix_ms =
            current_unix_millis().expect("current time should be available");
        completed.result = Some(ProviderToolExecutionResult {
            upstream_response_id: Some(String::from("task-one")),
            output: ProviderToolNormalizedOutput::Resources(vec![ProviderToolGeneratedResource {
                output_id: String::from("video-0"),
                kind: String::from("video"),
                mime_type: String::from("video/mp4"),
                bytes: Some(vec![1, 2, 3, 4]),
                download_url: Some(String::from(
                    "https://example.com/video.mp4?signature=temporary-secret",
                )),
                download_policy: Some(crate::ProviderToolResourceDownloadPolicy {
                    encoding: crate::ProviderToolResourceEncoding::HttpsUrl,
                }),
            }]),
        });
        store
            .replace(completed)
            .expect("completed record should persist without its result");

        let snapshot_path = directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE);
        let document = fs::read_to_string(snapshot_path).expect("snapshot should be readable");
        assert!(!document.contains("temporary-secret"));
        assert!(!document.contains("\"result\""));

        let reloaded = ProviderTaskRecoveryStore::default();
        let records = reloaded
            .configure(&directory)
            .expect("snapshot should reload");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].phase, ProviderAsyncTaskPhase::Completed);
        assert!(records[0].result.is_none());
        fs::remove_dir_all(directory).expect("temporary directory should be removable");
    }

    /// Verifies repeated publications advance a durable revision that survives reload.
    /// 验证重复发布会推进可在重载后保留的持久化修订号。
    #[test]
    fn recovery_store_publishes_monotonic_revisions() {
        let directory =
            std::env::temp_dir().join(format!("vulcan-provider-task-test-{}", Uuid::new_v4()));
        let store = ProviderTaskRecoveryStore::default();
        store.configure(&directory).expect("store should configure");
        store
            .replace(record("execution-one"))
            .expect("first record should persist");
        store
            .replace(record("execution-two"))
            .expect("second record should persist");

        let snapshot_path = directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE);
        let bytes = fs::read(snapshot_path).expect("snapshot should be readable");
        let snapshot: ProviderTaskRecoverySnapshot =
            serde_json::from_slice(bytes.as_slice()).expect("snapshot should decode");
        assert_eq!(snapshot.revision, 3);

        let reloaded = ProviderTaskRecoveryStore::default();
        let records = reloaded
            .configure(&directory)
            .expect("latest revision should reload");
        assert_eq!(records.len(), 2);
        assert_eq!(
            *reloaded
                .revision
                .lock()
                .expect("revision lock should be healthy"),
            4
        );
        fs::remove_dir_all(directory).expect("temporary directory should be removable");
    }

    /// Verifies exact current and rollback files select the higher embedded revision.
    /// 验证精确当前文件与回滚文件会选择较高的内嵌修订号。
    #[test]
    fn recovery_snapshot_uses_newest_exact_file_revision() {
        let directory =
            std::env::temp_dir().join(format!("vulcan-provider-task-revision-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("temporary directory should create");
        let current = ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: 2,
            records: BTreeMap::new(),
        };
        let previous = ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: 3,
            records: BTreeMap::new(),
        };
        fs::write(
            directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE),
            serde_json::to_vec(&current).expect("current snapshot should encode"),
        )
        .expect("current snapshot should write");
        fs::write(
            directory.join(PROVIDER_TASK_PREVIOUS_SNAPSHOT_FILE),
            serde_json::to_vec(&previous).expect("previous snapshot should encode"),
        )
        .expect("previous snapshot should write");

        let loaded = load_latest_snapshot(&directory).expect("exact snapshots should load");
        assert_eq!(loaded.revision, 3);
        fs::remove_dir_all(directory).expect("temporary directory should be removable");
    }

    /// Verifies failed initial publication restores the store's prior in-memory configuration.
    /// 验证初始发布失败会恢复存储先前的内存配置。
    #[test]
    fn recovery_configuration_rolls_back_after_initial_publication_failure() {
        let store = ProviderTaskRecoveryStore::default();
        store
            .replace(record("execution-before-configure"))
            .expect("unconfigured in-memory record should insert");
        let directory = std::env::temp_dir().join(format!(
            "vulcan-provider-task-configure-rollback-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).expect("temporary directory should create");
        let snapshot = ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: u64::MAX,
            records: BTreeMap::new(),
        };
        let path = directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE);
        fs::write(
            path,
            serde_json::to_vec_pretty(&snapshot).expect("snapshot should encode"),
        )
        .expect("snapshot should write");

        let error = store
            .configure(&directory)
            .expect_err("exhausted durable revision must fail publication");
        assert!(error.contains("revision is exhausted"), "{error}");
        assert_eq!(
            store
                .snapshot()
                .expect("prior memory state should remain")
                .into_iter()
                .map(|record| record.execution_id)
                .collect::<Vec<_>>(),
            vec![String::from("execution-before-configure")]
        );
        assert_eq!(
            *store
                .revision
                .lock()
                .expect("revision lock should remain healthy"),
            0
        );
        assert!(
            store
                .directory
                .lock()
                .expect("directory lock should remain healthy")
                .is_none()
        );
        fs::remove_dir_all(directory).expect("temporary directory should be removable");
    }

    /// Verifies a failed durable publication cannot leave an uncommitted memory record.
    /// 验证持久发布失败不会在内存中留下未提交记录。
    #[test]
    fn recovery_store_rolls_back_memory_after_publication_failure() {
        let directory =
            std::env::temp_dir().join(format!("vulcan-provider-task-test-{}", Uuid::new_v4()));
        let store = ProviderTaskRecoveryStore::default();
        store.configure(&directory).expect("store should configure");
        fs::remove_dir_all(&directory).expect("durable directory should be removable");

        let error = store
            .replace(record("execution-one"))
            .expect_err("publication should fail without the configured directory");
        assert!(error.contains("temporary file"));
        assert!(
            store
                .snapshot()
                .expect("memory snapshot should load")
                .is_empty()
        );
    }

    /// Verifies uncertain dispatch is terminal and cannot be mistaken for a retryable queue state.
    /// 验证分派不确定是终态且不会被误认为可重试排队状态。
    #[test]
    fn uncertain_dispatch_is_terminal() {
        assert!(ProviderAsyncTaskPhase::DispatchOutcomeUnknown.is_terminal());
        assert!(!ProviderAsyncTaskPhase::Queued.is_terminal());
    }

    /// Verifies a stale pending poll cannot overwrite durable cancellation intent.
    /// 验证过期等待轮询无法覆盖持久取消意图。
    #[test]
    fn stale_poll_cannot_regress_cancellation_intent() {
        let store = ProviderTaskRecoveryStore::default();
        let mut running = record("execution-one");
        running.phase = ProviderAsyncTaskPhase::Running;
        store
            .replace(running.clone())
            .expect("running record should be inserted");
        let mut cancellation = running.clone();
        cancellation.phase = ProviderAsyncTaskPhase::CancellationRequested;
        cancellation.updated_at_unix_ms = 2;
        store
            .replace(cancellation)
            .expect("cancellation intent should advance");
        let mut stale_poll = running;
        stale_poll.updated_at_unix_ms = 3;

        let error = store
            .replace(stale_poll)
            .expect_err("stale poll must not overwrite cancellation intent");

        assert!(error.contains("rejected stale phase transition"));
        assert_eq!(
            store
                .require("execution-one")
                .expect("authoritative record should remain")
                .phase,
            ProviderAsyncTaskPhase::CancellationRequested
        );
    }

    /// Verifies a terminal provider observation cannot be resurrected by stale cancellation.
    /// 验证供应商终态观测无法被过期取消请求复活。
    #[test]
    fn stale_cancellation_cannot_regress_terminal_state() {
        let store = ProviderTaskRecoveryStore::default();
        let queued = record("execution-one");
        store
            .replace(queued.clone())
            .expect("queued record should be inserted");
        let mut completed = queued.clone();
        completed.phase = ProviderAsyncTaskPhase::Completed;
        completed.updated_at_unix_ms = 2;
        store.replace(completed).expect("completion should advance");
        let mut stale_cancellation = queued;
        stale_cancellation.phase = ProviderAsyncTaskPhase::CancellationRequested;
        stale_cancellation.updated_at_unix_ms = 3;

        let error = store
            .replace(stale_cancellation)
            .expect_err("stale cancellation must not overwrite terminal state");

        assert!(error.contains("rejected stale phase transition"));
        assert_eq!(
            store
                .require("execution-one")
                .expect("authoritative record should remain")
                .phase,
            ProviderAsyncTaskPhase::Completed
        );
    }

    /// Verifies a confirmed provider task cannot change its recovery affinity.
    /// 验证已确认供应商任务无法更改其恢复亲和性。
    #[test]
    fn confirmed_task_rejects_recovery_affinity_changes() {
        let store = ProviderTaskRecoveryStore::default();
        let queued = record("execution-one");
        store
            .replace(queued.clone())
            .expect("queued record should be inserted");
        let mut mutated = queued;
        mutated.recovery = ProviderAsyncTaskRecoveryContext::OrderedSha256 {
            fingerprints: vec!["a".repeat(64)],
        };

        let error = store
            .replace(mutated)
            .expect_err("confirmed recovery metadata must be immutable");

        assert!(error.contains("confirmed recovery metadata"));
    }

    /// Verifies terminal history is bounded without pruning active or just-mutated records.
    /// 验证终态历史受到限制，同时不会清理活动记录或刚变更记录。
    #[test]
    fn terminal_history_is_bounded_without_pruning_active_records() {
        let mut records = BTreeMap::new();
        let active = record("active");
        records.insert(active.execution_id.clone(), active);
        for index in 0..(PROVIDER_TASK_MAXIMUM_TERMINAL_RECORDS + 8) {
            let mut terminal = record(format!("terminal-{index:03}").as_str());
            terminal.phase = ProviderAsyncTaskPhase::Completed;
            terminal.updated_at_unix_ms = u64::try_from(index + 1).unwrap_or(u64::MAX);
            records.insert(terminal.execution_id.clone(), terminal);
        }
        let protected = String::from("terminal-000");
        prune_terminal_records(
            &mut records,
            PROVIDER_TASK_TERMINAL_RETENTION_MS,
            Some(protected.as_str()),
        );
        assert!(records.contains_key("active"));
        assert!(records.contains_key(protected.as_str()));
        assert_eq!(
            records
                .values()
                .filter(|record| record.phase.is_terminal())
                .count(),
            PROVIDER_TASK_MAXIMUM_TERMINAL_RECORDS
        );
    }

    /// Verifies expired terminal records are removed while recent records remain readable.
    /// 验证过期终态记录会被删除，而近期记录仍可读取。
    #[test]
    fn expired_terminal_history_is_removed_by_age() {
        let mut expired = record("expired");
        expired.phase = ProviderAsyncTaskPhase::Failed;
        expired.updated_at_unix_ms = 1;
        let mut recent = record("recent");
        recent.phase = ProviderAsyncTaskPhase::Completed;
        recent.updated_at_unix_ms = PROVIDER_TASK_TERMINAL_RETENTION_MS;
        let mut records = BTreeMap::from([
            (expired.execution_id.clone(), expired),
            (recent.execution_id.clone(), recent),
        ]);
        prune_terminal_records(&mut records, PROVIDER_TASK_TERMINAL_RETENTION_MS + 2, None);
        assert!(!records.contains_key("expired"));
        assert!(records.contains_key("recent"));
    }

    /// Verifies oversized exact snapshots fail before allocation and unrelated files are ignored.
    /// 验证过大的精确快照会在分配前关闭失败，且无关文件会被忽略。
    #[test]
    fn recovery_snapshot_bytes_are_bounded_and_unrelated_files_are_not_discovered() {
        let oversized_directory =
            std::env::temp_dir().join(format!("vulcan-provider-task-oversized-{}", Uuid::new_v4()));
        fs::create_dir_all(&oversized_directory).expect("oversized directory should create");
        let oversized_path = oversized_directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE);
        let oversized_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&oversized_path)
            .expect("oversized sparse snapshot should create");
        oversized_file
            .set_len(PROVIDER_TASK_SNAPSHOT_MAX_BYTES + 1)
            .expect("oversized sparse snapshot should resize");
        drop(oversized_file);
        let oversized_error = load_latest_snapshot(&oversized_directory)
            .expect_err("oversized snapshot must fail before reading bytes");
        assert!(oversized_error.contains("byte limit"), "{oversized_error}");
        fs::remove_dir_all(&oversized_directory)
            .expect("oversized temporary directory should be removable");

        let exact_directory =
            std::env::temp_dir().join(format!("vulcan-provider-task-exact-{}", Uuid::new_v4()));
        fs::create_dir_all(&exact_directory).expect("exact directory should create");
        for index in 1..=65 {
            fs::write(
                exact_directory.join(format!("legacy-snapshot-{index:03}.json")),
                b"not-json",
            )
            .expect("unrelated file should write");
        }
        let exact_snapshot = ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: 7,
            records: BTreeMap::new(),
        };
        fs::write(
            exact_directory.join(PROVIDER_TASK_CURRENT_SNAPSHOT_FILE),
            serde_json::to_vec(&exact_snapshot).expect("exact snapshot should encode"),
        )
        .expect("exact snapshot should write");
        let loaded = load_latest_snapshot(&exact_directory)
            .expect("only the exact current and previous files should be read");
        assert_eq!(loaded.revision, 7);
        fs::remove_dir_all(&exact_directory)
            .expect("exact temporary directory should be removable");
    }

    /// Verifies record-cap exhaustion preserves every already active task.
    /// 验证记录容量耗尽时保留全部既有活动任务。
    #[test]
    fn recovery_record_capacity_preserves_active_tasks() {
        let store = ProviderTaskRecoveryStore::default();
        {
            let mut records = store
                .records
                .lock()
                .expect("provider task recovery lock should be healthy");
            for index in 0..PROVIDER_TASK_MAXIMUM_RECORDS {
                let task = record(format!("active-{index:04}").as_str());
                records.insert(task.execution_id.clone(), task);
            }
        }
        let error = store
            .replace(record("overflow"))
            .expect_err("a new active task beyond the total cap must fail");
        assert!(error.contains("capacity"), "{error}");
        let retained = store
            .snapshot()
            .expect("retained tasks should remain readable");
        assert_eq!(retained.len(), PROVIDER_TASK_MAXIMUM_RECORDS);
        assert!(retained.iter().all(|task| task.execution_id != "overflow"));
    }
}
