//! Core-owned asynchronous plugin environment installation tasks.
//! Core 拥有的异步插件执行环境安装任务。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use uuid::Uuid;

/// Maximum diagnostic length retained for one installation task.
/// 单个安装任务保留的诊断最大长度。
const MAX_INSTALL_ERROR_CHARS: usize = 1024;

/// Maximum terminal tasks retained for later read-only status queries.
/// 为后续只读状态查询保留的终态任务最大数量。
const MAX_RETAINED_TERMINAL_TASKS: usize = 64;

/// Exact catalog identity used to coalesce requests for one directory generation.
/// 用于合并同一目录代次请求的精确目录身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PluginEnvironmentInstallKey {
    /// Stable plugin id selected from the validated catalog.
    /// 从已校验目录选择的稳定插件标识。
    pub(crate) plugin_id: String,
    /// Canonical source package directory for this catalog entry.
    /// 此目录条目的规范化源插件包目录。
    pub(crate) package_root: PathBuf,
    /// Monotonic catalog generation associated with the entry.
    /// 与此条目关联的单调目录代次。
    pub(crate) catalog_generation: u64,
    /// Content fingerprint that identifies the package contents.
    /// 标识插件包内容的内容指纹。
    pub(crate) content_fingerprint: String,
}

/// Publicly queryable state of one Core-owned installation task.
/// 一个 Core 自有安装任务的公开可查询状态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginEnvironmentInstallTaskStatus {
    /// Stable task id returned to callers and used for later queries.
    /// 返回给调用方并用于后续查询的稳定任务标识。
    pub(crate) task_id: String,
    /// Stable plugin id selected from the catalog.
    /// 从目录选择的稳定插件标识。
    pub(crate) plugin_id: String,
    /// Non-secret readable identity of the package generation.
    /// 不包含秘密信息的可读插件包代次身份。
    pub(crate) generation: String,
    /// Lifecycle state: queued, running, succeeded, or failed.
    /// 生命周期状态：queued、running、succeeded 或 failed。
    pub(crate) state: String,
    /// Bounded machine-readable progress stage.
    /// 有界的机器可读进度阶段。
    pub(crate) stage: String,
    /// Bounded terminal error, present only for failed tasks.
    /// 有界终态错误，仅失败任务存在。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

/// Internal task record retaining the exact deduplication key.
/// 保留精确去重键的内部任务记录。
#[derive(Debug, Clone)]
struct InstallTaskRecord {
    /// Exact key that maps one task to one package directory generation.
    /// 将任务映射到单个插件包目录代次的精确键。
    key: PluginEnvironmentInstallKey,
    /// Public status projection returned by the query RPC.
    /// 由查询 RPC 返回的公开状态投影。
    status: PluginEnvironmentInstallTaskStatus,
    /// Monotonic insertion ordinal used for bounded terminal retention.
    /// 用于限制终态保留数量的单调插入序号。
    insertion_ordinal: u64,
}

/// Mutable task registry owned by one service Core.
/// 一个服务 Core 所有的可变任务登记表。
#[derive(Clone, Debug, Default)]
pub(crate) struct PluginEnvironmentInstallTaskRegistry {
    /// Process-local mutex protecting task identity and status transitions.
    /// 保护任务身份与状态转换的进程内互斥锁。
    inner: Arc<Mutex<InstallTaskRegistryState>>,
}

/// Returns the process-local registry owned by the single Core service instance.
/// 返回由单个 Core 服务实例所有的进程内登记表。
pub(crate) fn plugin_environment_install_task_registry()
-> &'static PluginEnvironmentInstallTaskRegistry {
    static REGISTRY: OnceLock<PluginEnvironmentInstallTaskRegistry> = OnceLock::new();
    REGISTRY.get_or_init(PluginEnvironmentInstallTaskRegistry::new)
}

/// Internal state for the installation task registry.
/// 插件安装任务登记表的内部状态。
#[derive(Debug, Default)]
struct InstallTaskRegistryState {
    /// Tasks indexed by stable task id.
    /// 按稳定任务标识索引的任务。
    tasks: BTreeMap<String, InstallTaskRecord>,
    /// Exact package generation keys mapped to retained task ids.
    /// 映射到已保留任务标识的精确插件包代次键。
    keys: BTreeMap<PluginEnvironmentInstallKey, String>,
    /// Next insertion ordinal.
    /// 下一个插入序号。
    next_insertion_ordinal: u64,
}

/// Result of inserting one task request into the deduplicating registry.
/// 将一个任务请求写入去重登记表后的结果。
#[derive(Debug, Clone)]
pub(crate) struct PluginEnvironmentInstallEnqueueResult {
    /// Existing or newly allocated task status.
    /// 已存在或新分配的任务状态。
    pub(crate) status: PluginEnvironmentInstallTaskStatus,
    /// True only when the caller owns starting the newly inserted task.
    /// 仅当调用方负责启动新插入任务时为真。
    pub(crate) should_start: bool,
}

impl PluginEnvironmentInstallTaskRegistry {
    /// Creates an empty Core-owned installation task registry.
    /// 创建空的 Core 自有安装任务登记表。
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Inserts one queued task or returns the retained task for the exact generation key.
    /// 插入一个排队任务，或返回精确代次键对应的已保留任务。
    ///
    /// # Parameters
    /// 参数
    /// `key` identifies the plugin and exact catalog directory generation.
    /// `key` 标识插件及精确的目录代次。
    ///
    /// # Returns
    /// 返回值
    /// Returns the status and whether a background worker must be spawned.
    /// 返回任务状态以及是否需要启动后台 worker。
    pub(crate) fn enqueue(
        &self,
        key: PluginEnvironmentInstallKey,
    ) -> Result<PluginEnvironmentInstallEnqueueResult, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| format!("插件环境安装任务登记表加锁失败：{error}"))?;
        if let Some(task_id) = guard.keys.get(&key).cloned() {
            let retained_status = guard
                .tasks
                .get(&task_id)
                .map(|record| record.status.clone());
            if let Some(status) = retained_status {
                if matches!(status.state.as_str(), "queued" | "running") {
                    return Ok(PluginEnvironmentInstallEnqueueResult {
                        status,
                        should_start: false,
                    });
                }
                // Terminal records remain queryable by task id but do not block an explicit retry.
                // 终态记录仍可按任务标识查询，但不能阻止用户显式重试。
            }
            guard.keys.remove(&key);
        }

        let task_id = Uuid::new_v4().to_string();
        let generation = format!(
            "catalog-{}:{}",
            key.catalog_generation, key.content_fingerprint
        );
        let status = PluginEnvironmentInstallTaskStatus {
            task_id: task_id.clone(),
            plugin_id: key.plugin_id.clone(),
            generation,
            state: String::from("queued"),
            stage: String::from("queued"),
            error: None,
        };
        let insertion_ordinal = guard.next_insertion_ordinal;
        guard.next_insertion_ordinal = guard.next_insertion_ordinal.wrapping_add(1);
        guard.keys.insert(key.clone(), task_id.clone());
        guard.tasks.insert(
            task_id,
            InstallTaskRecord {
                key,
                status: status.clone(),
                insertion_ordinal,
            },
        );
        Ok(PluginEnvironmentInstallEnqueueResult {
            status,
            should_start: true,
        })
    }

    /// Updates one task with a bounded state, stage, and optional diagnostic.
    /// 使用有界状态、阶段及可选诊断更新一个任务。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` is the exact task being updated; `state` and `stage` are fixed protocol values.
    /// `task_id` 是正在更新的精确任务；`state` 与 `stage` 是固定协议值。
    /// `error` is retained only for failed tasks and is bounded by character count.
    /// `error` 仅为失败任务保留，并按字符数量限制长度。
    pub(crate) fn update(
        &self,
        task_id: &str,
        state: &str,
        stage: &str,
        error: Option<&str>,
    ) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|lock_error| format!("插件环境安装任务登记表加锁失败：{lock_error}"))?;
        let record = guard
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("未找到插件环境安装任务：{task_id}"))?;
        record.status.state = state.to_string();
        record.status.stage = stage.to_string();
        record.status.error = error.map(bound_install_error);
        if matches!(state, "succeeded" | "failed") {
            trim_terminal_tasks(&mut guard);
        }
        Ok(())
    }

    /// Returns a cloned status for one stable task id.
    /// 返回一个稳定任务标识对应的状态副本。
    pub(crate) fn status(
        &self,
        task_id: &str,
    ) -> Result<Option<PluginEnvironmentInstallTaskStatus>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| format!("插件环境安装任务登记表加锁失败：{error}"))?;
        Ok(guard.tasks.get(task_id).map(|record| record.status.clone()))
    }

    /// Returns the latest retained task for one exact package generation.
    /// 返回精确插件包代次对应的最新保留任务。
    ///
    /// # Parameters
    /// 参数
    /// `key` identifies the plugin and current catalog directory generation.
    /// `key` 标识插件及当前目录代次。
    ///
    /// # Returns
    /// 返回值
    /// Returns the task status when this generation has been requested, otherwise `None`.
    /// 当此代次曾被请求时返回任务状态，否则返回 `None`。
    pub(crate) fn status_for_key(
        &self,
        key: &PluginEnvironmentInstallKey,
    ) -> Result<Option<PluginEnvironmentInstallTaskStatus>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| format!("插件环境安装任务登记表加锁失败：{error}"))?;
        let Some(task_id) = guard.keys.get(key) else {
            return Ok(None);
        };
        Ok(guard.tasks.get(task_id).map(|record| record.status.clone()))
    }
}

/// Bounds an installation diagnostic before it enters retained task state.
/// 在诊断进入保留任务状态前限制其长度。
fn bound_install_error(error: &str) -> String {
    error.chars().take(MAX_INSTALL_ERROR_CHARS).collect()
}

/// Evicts the oldest terminal records while retaining queued and running work.
/// 保留排队和运行中任务，同时淘汰最旧的终态记录。
fn trim_terminal_tasks(state: &mut InstallTaskRegistryState) {
    let mut terminal = state
        .tasks
        .values()
        .filter(|record| matches!(record.status.state.as_str(), "succeeded" | "failed"))
        .map(|record| (record.insertion_ordinal, record.status.task_id.clone()))
        .collect::<Vec<_>>();
    terminal.sort_unstable();
    while terminal.len() > MAX_RETAINED_TERMINAL_TASKS {
        let (_, task_id) = terminal.remove(0);
        if let Some(record) = state.tasks.remove(&task_id) {
            state.keys.remove(&record.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one deterministic key for registry unit tests.
    /// 为登记表单元测试构造一个确定性键。
    fn test_key(generation: u64) -> PluginEnvironmentInstallKey {
        PluginEnvironmentInstallKey {
            plugin_id: String::from("playwright"),
            package_root: PathBuf::from(r"C:\plugins\playwright"),
            catalog_generation: generation,
            content_fingerprint: format!("fingerprint-{generation}"),
        }
    }

    #[test]
    fn exact_generation_requests_are_deduplicated() {
        let registry = PluginEnvironmentInstallTaskRegistry::new();
        let first = registry.enqueue(test_key(1)).expect("enqueue first");
        let second = registry.enqueue(test_key(1)).expect("enqueue duplicate");
        assert!(first.should_start);
        assert!(!second.should_start);
        assert_eq!(first.status.task_id, second.status.task_id);
        assert_eq!(second.status.state, "queued");
    }

    #[test]
    fn different_generations_get_independent_tasks() {
        let registry = PluginEnvironmentInstallTaskRegistry::new();
        let first = registry.enqueue(test_key(1)).expect("enqueue first");
        let second = registry.enqueue(test_key(2)).expect("enqueue second");
        assert_ne!(first.status.task_id, second.status.task_id);
        assert!(second.should_start);
    }

    #[test]
    fn errors_are_bounded_and_terminal_status_is_queryable() {
        let registry = PluginEnvironmentInstallTaskRegistry::new();
        let queued = registry.enqueue(test_key(1)).expect("enqueue");
        let oversized = "错误".repeat(MAX_INSTALL_ERROR_CHARS + 20);
        registry
            .update(&queued.status.task_id, "failed", "failed", Some(&oversized))
            .expect("update");
        let status = registry
            .status(&queued.status.task_id)
            .expect("query")
            .expect("retained status");
        assert_eq!(status.state, "failed");
        assert_eq!(status.stage, "failed");
        assert_eq!(
            status.error.expect("error").chars().count(),
            MAX_INSTALL_ERROR_CHARS
        );
    }

    #[test]
    fn terminal_task_does_not_block_an_explicit_retry() {
        let registry = PluginEnvironmentInstallTaskRegistry::new();
        let first = registry.enqueue(test_key(1)).expect("enqueue first");
        registry
            .update(&first.status.task_id, "failed", "failed", Some("retryable"))
            .expect("mark failed");
        let retry = registry.enqueue(test_key(1)).expect("enqueue retry");
        assert!(retry.should_start);
        assert_ne!(first.status.task_id, retry.status.task_id);
        assert_eq!(
            registry
                .status(&first.status.task_id)
                .expect("query old")
                .expect("old retained")
                .state,
            "failed"
        );
        assert_eq!(
            registry
                .status_for_key(&test_key(1))
                .expect("query generation")
                .expect("new retained")
                .task_id,
            retry.status.task_id
        );
    }
}
