//! Durable non-secret lifecycle records for provider-native asynchronous tools.
//! 供应商原生异步工具的可持久化非秘密生命周期记录。

use crate::{ProviderToolAdapterKind, ProviderToolExecutionResult, ProviderToolTargetSelection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
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
}

/// Non-secret recovery data required to normalize a provider task after restart.
/// 重启后归一化供应商任务所需的非秘密恢复数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderAsyncTaskRecoveryContext {
    /// No adapter-specific recovery data is required.
    /// 不需要适配器专属恢复数据。
    None,
    /// Ordered SHA-256 fingerprints of Fun-ASR provider object locators.
    /// Fun-ASR 供应商对象定位符的有序 SHA-256 指纹。
    FunAsr {
        /// Ordered hexadecimal SHA-256 values without retaining signed object URLs.
        /// 不保留签名对象 URL 的有序十六进制 SHA-256 值。
        source_url_sha256: Vec<String>,
    },
}

/// Immutable non-secret route affinity for one asynchronous provider task.
/// 单个异步供应商任务的不可变非秘密路由亲和性。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        {
            let mut records = self
                .records
                .lock()
                .map_err(|_| String::from("The provider task recovery lock is poisoned."))?;
            *records = loaded.records;
        }
        *self
            .revision
            .lock()
            .map_err(|_| String::from("The provider task revision lock is poisoned."))? =
            loaded.revision;
        *self
            .directory
            .lock()
            .map_err(|_| String::from("The provider task directory lock is poisoned."))? =
            Some(directory.to_path_buf());
        self.persist()?;
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
        let _mutation_guard = self
            .mutation_lock
            .lock()
            .map_err(|_| String::from("The provider task mutation lock is poisoned."))?;
        let execution_id = record.execution_id.clone();
        let previous_records = {
            let mut records = self
                .records
                .lock()
                .map_err(|_| String::from("The provider task recovery lock is poisoned."))?;
            if let Some(current) = records.get(execution_id.as_str()) {
                validate_provider_task_replacement(current, &record)?;
            }
            let previous_records = records.clone();
            let now_unix_ms = record.updated_at_unix_ms;
            records.insert(execution_id.clone(), record);
            prune_terminal_records(&mut records, now_unix_ms, Some(execution_id.as_str()));
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
        Ok(())
    }

    /// Publishes the complete map using one unique temporary file and atomic rename.
    /// 使用唯一临时文件和原子重命名发布完整映射。
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
        let identity = Uuid::new_v4();
        let temporary = directory.join(format!(".snapshot-{next_revision:020}-{identity}.tmp"));
        let published = directory.join(format!("snapshot-{next_revision:020}-{identity}.json"));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("Provider task recovery temporary file failed: {error}"))?;
        file.write_all(bytes.as_slice())
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Provider task recovery snapshot write failed: {error}"))?;
        drop(file);
        fs::rename(&temporary, &published)
            .map_err(|error| format!("Provider task recovery publish failed: {error}"))?;
        *revision = next_revision;
        cleanup_old_snapshots(&directory, &published);
        Ok(())
    }
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

/// Loads the newest complete strict snapshot or an empty version-one state.
/// 加载最新完整严格快照，或返回空的第一版状态。
fn load_latest_snapshot(directory: &Path) -> Result<ProviderTaskRecoverySnapshot, String> {
    let mut snapshots = fs::read_dir(directory)
        .map_err(|error| format!("Provider task recovery directory read failed: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("snapshot-") && name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    snapshots.sort();
    let Some(path) = snapshots.pop() else {
        return Ok(ProviderTaskRecoverySnapshot {
            version: PROVIDER_TASK_RECOVERY_VERSION,
            revision: 0,
            records: BTreeMap::new(),
        });
    };
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "Provider task recovery snapshot `{}` could not be read: {error}",
            path.display()
        )
    })?;
    let snapshot: ProviderTaskRecoverySnapshot =
        serde_json::from_slice(bytes.as_slice()).map_err(|error| {
            format!(
                "Provider task recovery snapshot `{}` is invalid: {error}",
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
    Ok(snapshot)
}

/// Best-effort removes obsolete snapshots after the authoritative replacement is durable.
/// 在权威替代快照持久化后尽力移除过期快照。
fn cleanup_old_snapshots(directory: &Path, retained: &Path) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("[provider-task] obsolete recovery snapshots could not be scanned: {error}");
            return;
        }
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => {
                eprintln!(
                    "[provider-task] one obsolete recovery snapshot entry could not be read: {error}"
                );
                continue;
            }
        };
        if path != retained
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("snapshot-") && name.ends_with(".json"))
            && let Err(error) = fs::remove_file(&path)
        {
            eprintln!(
                "[provider-task] obsolete recovery snapshot `{}` could not be removed: {error}",
                path.display()
            );
        }
    }
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
                adapter: ProviderToolAdapterKind::AlibabaWanVideoGenerate,
            },
            provider_task_id: Some(String::from("task-one")),
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Queued,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            deadline_unix_ms: 2,
            cancellation_supported: true,
            error: None,
            result: None,
        }
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
        let document = fs::read_to_string(
            fs::read_dir(&directory)
                .expect("directory should be readable")
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "json")
                })
                .expect("one snapshot should exist"),
        )
        .expect("snapshot should be readable");
        assert!(!document.contains("secret"));
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
            }]),
        });
        store
            .replace(completed)
            .expect("completed record should persist without its result");

        let snapshot_path = fs::read_dir(&directory)
            .expect("directory should be readable")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .expect("one snapshot should exist");
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

        let snapshot_path = fs::read_dir(&directory)
            .expect("directory should be readable")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .expect("one published snapshot should remain");
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
            .replace(cancellation.clone())
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
        mutated.recovery = ProviderAsyncTaskRecoveryContext::FunAsr {
            source_url_sha256: vec![String::from("unexpected")],
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
}
