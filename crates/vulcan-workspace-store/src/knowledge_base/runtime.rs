//! Knowledge-base lifecycle, import, authorization, and BM25 runtime.
//! 知识库生命周期、导入、授权与 BM25 运行时。

mod authorization;
mod display;
mod file_selection;
mod fts;
mod import;
mod library;
mod operations;
mod package_archive;
mod package_lifecycle;
mod raw_corpus;
mod retrieval;
mod storage;
mod vector;

use display::recovered_display_index_status;
use library::{LibraryRow, library_name_key, library_summary_from_row, validate_library_name};
use raw_corpus::CountRow;
use vector::recovered_vector_index_status;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

use storage::{
    KnowledgeManifest, metadata_is_link_or_reparse, persist_tempfile_atomically,
    validate_library_id,
};

use operations::{
    ActiveKnowledgeOperation, KNOWLEDGE_OPERATION_CANCELLED_ERROR, PreparedKnowledgeOperation,
    queued_operation_summary, require_operation_not_cancelled,
};

// Preserve the same model authority for all relocated domain implementations.
// 为全部迁移后的领域实现保留同一模型权威。
use super::models;

use super::schema::{KNOWLEDGE_FTS_INDEX, KNOWLEDGE_SCHEMA_VERSION};
use crate::data_plane::HostRuntimeHandle;
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use uuid::Uuid;

/// Maximum user-visible warning length.
/// 单条用户可见告警的最大长度。
const MAX_WARNING_CHARS: usize = 1_000;
/// Maximum federated search result size.
/// 联合检索结果的最大数量。
const MAX_SEARCH_LIMIT: u32 = 200;
/// Reciprocal-rank-fusion constant used across physical libraries.
/// 跨物理知识库使用的倒数排名融合常量。
const RRF_K: f64 = 60.0;
/// Prefix reserved for host-owned package extraction directories removed during startup recovery.
/// 宿主持有的知识库包解压目录保留前缀，启动恢复期间会清理这些目录。
const KNOWLEDGE_PACKAGE_INSPECTION_TEMP_PREFIX: &str = ".package-inspect-";

/// Maximum accepted JSON metadata entry size.
/// 可接受的 JSON 元数据条目最大字节数。
const MAX_KNOWLEDGE_PACKAGE_JSON_BYTES: u64 = 1024 * 1024;

/// Host-native knowledge runtime sharing the in-process storage runtime and application registry.
/// 共享进程内存储运行时与应用注册表的宿主原生知识库运行时。
#[derive(Clone)]
pub struct KnowledgeBaseRuntime {
    /// Long-lived host data-plane handle.
    /// 长生命周期宿主数据面句柄。
    host_runtime: HostRuntimeHandle,
    /// Per-library exclusive mutation locks.
    /// 每知识库独占变更锁。
    mutation_locks: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
    /// Serializes binding activation and protects the shared active-binding caches.
    /// 串行化绑定激活并保护共享活动绑定缓存。
    binding_activation_lock: Arc<Mutex<()>>,
    /// Library identifiers whose host SQLite binding is already active.
    /// 宿主 SQLite 绑定已经激活的知识库标识符。
    active_bindings: Arc<Mutex<BTreeSet<String>>>,
    /// Library identifiers whose derived display SQLite binding is already active.
    /// 派生显示库 SQLite 绑定已经激活的知识库标识符。
    active_display_bindings: Arc<Mutex<BTreeSet<String>>>,
    /// Library identifiers whose derived LanceDB binding is already active.
    /// 派生 LanceDB 绑定已经激活的知识库标识符。
    active_vector_bindings: Arc<Mutex<BTreeSet<String>>>,
    /// Exact active knowledge operations, including derived builds, with cooperative cancellation.
    /// 支持协作取消的精确活动知识库操作，包括派生构建。
    active_operations: Arc<Mutex<BTreeMap<String, ActiveKnowledgeOperation>>>,
}

/// Process-local knowledge runtime state shared by every wrapper over one host handle.
/// 由同一宿主句柄的所有包装运行时共享的进程内知识库状态。
#[derive(Default)]
pub(crate) struct KnowledgeBaseRuntimeSharedState {
    /// Per-library exclusive mutation locks.
    /// 每知识库独占变更锁。
    mutation_locks: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
    /// Serializes binding activation and protects the shared active-binding caches.
    /// 串行化绑定激活并保护共享活动绑定缓存。
    binding_activation_lock: Arc<Mutex<()>>,
    /// Library identifiers whose host SQLite binding is already active.
    /// 宿主 SQLite 绑定已经激活的知识库标识符。
    active_bindings: Arc<Mutex<BTreeSet<String>>>,
    /// Library identifiers whose derived display SQLite binding is already active.
    /// 派生显示库 SQLite 绑定已经激活的知识库标识符。
    active_display_bindings: Arc<Mutex<BTreeSet<String>>>,
    /// Library identifiers whose derived LanceDB binding is already active.
    /// 派生 LanceDB 绑定已经激活的知识库标识符。
    active_vector_bindings: Arc<Mutex<BTreeSet<String>>>,
    /// Exact active knowledge operations, including derived builds, with cooperative cancellation.
    /// 支持协作取消的精确活动知识库操作，包括派生构建。
    active_operations: Arc<Mutex<BTreeMap<String, ActiveKnowledgeOperation>>>,
}

impl KnowledgeBaseRuntime {
    /// Creates a knowledge runtime from the established application data plane.
    /// 从已建立的应用数据面创建知识库运行时。
    /// `host_runtime` is the established host handle that owns shared knowledge state.
    /// `host_runtime` 是持有共享知识库状态的既有宿主句柄。
    /// Returns a wrapper reusing that exact host runtime, locks, caches, and active-operation map.
    /// 返回复用同一宿主运行时、锁、缓存和活动任务映射的包装运行时。
    pub fn new(host_runtime: HostRuntimeHandle) -> Self {
        // sharedState ensures independently reconstructed AI and WebUI runtimes coordinate the same bindings and operations.
        // sharedState 确保独立重建的 AI 与 WebUI 运行时协调同一组绑定和操作。
        let shared_state = host_runtime.knowledge_runtime_state.clone();
        Self {
            host_runtime,
            mutation_locks: shared_state.mutation_locks.clone(),
            binding_activation_lock: shared_state.binding_activation_lock.clone(),
            active_bindings: shared_state.active_bindings.clone(),
            active_display_bindings: shared_state.active_display_bindings.clone(),
            active_vector_bindings: shared_state.active_vector_bindings.clone(),
            active_operations: shared_state.active_operations.clone(),
        }
    }

    /// Marks abandoned queued or running knowledge operations as interrupted during service startup.
    /// 服务启动时将遗留的排队或运行中知识库操作标记为已中断。
    /// Uses the established host runtime; no independent recovery state is allocated.
    /// 使用既有宿主运行时，不分配独立恢复状态。
    /// Returns success after job, replacement, derived-state, and orphan cleanup recovery in order.
    /// 按顺序完成任务、覆盖、派生状态及孤立目录清理恢复后返回成功。
    pub async fn recover_interrupted_jobs(&self) -> Result<(), String> {
        local_sql::execute_script_async(
            &self.host_runtime.local,
            LogicalDatabase::AppState,
                "UPDATE knowledge_import_jobs SET state = 'interrupted', phase = 'interrupted', error_message = 'service restarted before knowledge operation completion', finished_at_ms = ? WHERE state IN ('queued', 'running');",
                vec![int(unix_epoch_millis()?)],
            ).await
            .map_err(|error| format!("恢复知识库操作状态失败：{error}"))?;
        self.recover_interrupted_replacements().await?;
        self.recover_interrupted_derived_index_states().await?;
        self.cleanup_orphan_package_directories().await?;
        Ok(())
    }

    /// Reconciles derived-index lifecycle flags after their executor disappeared during restart.
    /// 在重启导致派生索引执行器消失后协调其生命周期标记。
    ///
    /// Returns: success after interrupted active libraries receive explicit continuation or rebuild states.
    /// 返回值：被中断的活动资料库获得明确继续或重建状态后返回成功。
    /// Uses current active registry rows after replacement recovery; errors propagate to startup.
    /// 使用覆盖恢复之后的当前活动注册记录；错误向启动流程传播。
    async fn recover_interrupted_derived_index_states(&self) -> Result<(), String> {
        // Libraries are read after replacement recovery so status writes target the authoritative registry rows.
        // 在覆盖恢复之后读取资料库，确保状态写入指向权威注册记录。
        let libraries = self.list_libraries(None).await?;
        for library in libraries
            .into_iter()
            .filter(|library| library.lifecycle_state == "active")
        {
            if let Some(recovered_status) = recovered_display_index_status(
                library.settings.display_index.status,
                library.settings.display_index.active_generation.is_some(),
            ) {
                // A published generation remains usable while the retained pending generation is completed.
                // 已发布代次在保留的待完成代次补齐期间仍然可用。
                let recovery_message =
                    if library.settings.display_index.pending_generation.is_some() {
                        "服务重启中断了显示库生成；已生成文档均已保留，可以继续生成"
                    } else {
                        "服务重启中断了显示库生成；尚未建立可恢复代次，请重新生成"
                    };
                self.set_display_index_status(
                    &library.id,
                    recovered_status,
                    Some(String::from(recovery_message)),
                )
                .await?;
            }
            if let Some(recovered_status) = recovered_vector_index_status(
                library.settings.vector_index.status,
                library.settings.vector_index.active_generation.is_some(),
            ) {
                // Vector builds do not yet retain a resumable generation, so restart exposes an explicit rebuild state.
                // 向量构建尚未保留可恢复代次，因此重启后显式进入重建状态。
                self.set_vector_index_status(
                    &library.id,
                    recovered_status,
                    Some(String::from("服务重启中断了向量索引生成，请重新生成")),
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Returns the per-library mutation guard after validating its UUID.
    /// 在验证 UUID 后返回每知识库变更锁守卫。
    /// `library_id` is the canonical UUID selecting the shared exclusive mutation domain.
    /// `library_id` 是选择共享独占变更域的规范 UUID。
    /// Returns the owned library lock guard, or an invalid-identity error.
    /// 返回知识库锁的自有守卫，或非法身份错误。
    async fn library_mutation_guard(
        &self,
        library_id: &str,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
        validate_library_id(library_id)?;
        let lock = {
            let mut locks = self.mutation_locks.lock().await;
            locks
                .entry(library_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        Ok(lock.lock_owned().await)
    }
}

/// Validates one generation identity used to isolate derived display data.
/// 校验用于隔离派生显示数据的一份代次身份。
/// `generation` is the exact identity shared by display and vector generation tables.
/// `generation` 是显示与向量代次表共享的精确身份。
/// Returns success only for the canonical lowercase UUID form; otherwise returns an explicit error.
/// 仅对规范小写 UUID 格式返回成功，否则返回明确错误。
fn validate_generation_id(generation: &str) -> Result<(), String> {
    let parsed = Uuid::parse_str(generation)
        .map_err(|error| format!("显示库代次标识符不是有效 UUID：{error}"))?;
    if parsed.to_string() != generation {
        return Err(String::from("显示库代次必须使用规范小写 UUID 格式。"));
    }
    Ok(())
}

/// Converts a non-negative SQLite integer into a public unsigned value.
/// 将非负 SQLite 整数转换为公共无符号值。
/// `value` is the signed SQLite count; `field` names the failing diagnostic.
/// `value` 是 SQLite 有符号计数；`field` 标识失败诊断。
/// Returns the nonnegative count or an explicit negative-value error.
/// 返回非负计数，或明确的负值错误。
fn nonnegative_u64(value: i64, field: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("知识库字段 {field} 包含非法负值：{value}"))
}

/// Returns a typed SQLite text value.
/// 返回类型化 SQLite 文本值。
/// Consumes `value` and returns the same text as a typed SQLite parameter.
/// 消费 `value`，并将同一文本返回为类型化 SQLite 参数。
fn text(value: impl Into<String>) -> SqliteValue {
    SqliteValue::Text(value.into())
}

/// Returns a typed SQLite integer value.
/// 返回类型化 SQLite 整数值。
/// Returns `value` unchanged as a signed 64-bit SQLite parameter.
/// 将 `value` 原样返回为有符号64位 SQLite 参数。
fn int(value: i64) -> SqliteValue {
    SqliteValue::Integer(value)
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
/// Reads the system wall clock without arguments; returns Unix milliseconds or a range/epoch error.
/// 无参数读取系统墙钟；返回 Unix 毫秒，或范围/纪元错误。
fn unix_epoch_millis() -> Result<i64, String> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("系统时间早于 Unix 纪元：{error}"))?
        .as_millis();
    i64::try_from(value).map_err(|_| String::from("当前 Unix 毫秒时间戳超出 i64 范围。"))
}

/// Truncates text by Unicode scalar count without splitting UTF-8.
/// 按 Unicode 标量数量截断文本且不拆分 UTF-8。
/// `value` is the source text; `maximum` bounds Unicode scalar values rather than bytes.
/// `value` 是来源文本；`maximum` 限制 Unicode 标量而非字节数量。
/// Returns an owned prefix without cutting a UTF-8 scalar.
/// 返回不切断 UTF-8 标量的自有前缀。
fn truncate_chars(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

#[cfg(test)]
mod tests;
