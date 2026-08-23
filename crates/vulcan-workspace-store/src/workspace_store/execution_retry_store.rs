//! Memory-first retry projection and single-file interrupted snapshot storage.
//! 内存优先的重试投影与单文件中断快照存储。

use sha2::{Digest, Sha256};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, MutexGuard};
use vulcan_workbench_model::{
    EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION, ExecutionRetryPendingAction,
    ExecutionRetryPreparationRouteSelection, ExecutionRetrySnapshot, ExecutionRetrySnapshotRef,
    ExecutionRetryToolProgress, ExecutionRetryToolState,
};

/// The only durable retry snapshot file below one interrupted session.
/// 单个已中断会话下唯一的持久化重试快照文件。
const EXECUTION_RETRY_FILE: &str = "retry.json";

/// Fixed number of retry-file serialization channels shared by all exact session paths.
/// 全部精确会话路径共享的固定重试文件串行化通道数量。
const EXECUTION_RETRY_FILE_LOCK_SHARDS: usize = 256;

/// Bounded path-keyed retry-file locks that avoid one global filesystem critical section.
/// 有界的路径键控重试文件锁，避免形成单个全局文件系统临界区。
static EXECUTION_RETRY_FILE_LOCKS: LazyLock<[Mutex<()>; EXECUTION_RETRY_FILE_LOCK_SHARDS]> =
    LazyLock::new(|| std::array::from_fn(|_| Mutex::new(())));

/// Owns strict read, write, validation, and deletion of retry sidecars for one session directory.
/// 负责单个会话目录中重试 sidecar 的严格读写、校验与删除。
#[derive(Debug, Clone)]
pub struct ExecutionRetrySnapshotStore {
    /// Exact segmented main-session directory.
    /// 精确的分段主会话目录。
    session_directory: PathBuf,
}

impl ExecutionRetrySnapshotStore {
    /// Creates a retry snapshot store rooted at one exact segmented session directory.
    /// 创建一个以精确分段会话目录为根的重试快照存储。
    ///
    /// # Parameters
    /// 参数
    /// `session_directory` is the validated main-session directory selected by workspace storage.
    /// `session_directory` 是工作区存储选定的已校验主会话目录。
    ///
    /// # Returns
    /// 返回值
    /// A path-bound store that performs no I/O until an operation is invoked.
    /// 返回路径绑定的存储；在调用具体操作前不执行 I/O。
    pub fn new(session_directory: impl Into<PathBuf>) -> Self {
        Self {
            session_directory: session_directory.into(),
        }
    }

    /// Projects one complete in-memory retry snapshot into a lightweight transient reference.
    /// 将一份完整内存重试快照投影为轻量临时引用。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the complete exact execution payload retained by live service memory.
    /// `snapshot` 是由实时服务内存保留的完整精确执行载荷。
    ///
    /// # Returns
    /// 返回值
    /// A digest-bound reference without any filesystem mutation.
    /// 返回不产生任何文件系统变更的摘要绑定引用。
    pub fn project(snapshot: &ExecutionRetrySnapshot) -> Result<ExecutionRetrySnapshotRef, String> {
        validate_snapshot_identity(snapshot)?;
        execution_retry_reference_from_encoded(snapshot, None)
    }

    /// Loads the single exact interrupted-file reference when an earlier crash left it without a retained state pointer.
    /// 当早期崩溃留下没有保留状态指针的中断文件时，加载其唯一精确引用。
    ///
    /// # Returns
    /// 返回值
    /// The validated snapshot and digest-bound reference for the fixed file, `None` when absent, or an explicit integrity error.
    /// 固定文件存在时返回经过校验的快照与摘要绑定引用；缺失时返回 `None`；完整性异常时返回明确错误。
    pub fn load_interrupted_reference_if_present(
        &self,
    ) -> Result<Option<(ExecutionRetrySnapshot, ExecutionRetrySnapshotRef)>, String> {
        // retryPath is resolved directly from the exact session root without directory discovery.
        // retryPath 直接由精确会话根目录解析，不执行目录发现。
        let retry_path = self.retry_path();
        // fileGuard prevents replacement or cleanup from interleaving with orphan-byte capture.
        // fileGuard 防止替换或清理与孤立正文字节捕获交错。
        let _file_guard = lock_exact_retry_file(&retry_path)?;
        // metadata rejects symbolic links before the exact body is opened.
        // metadata 在打开精确正文前拒绝符号链接。
        match fs::symlink_metadata(&retry_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Refusing to load symbolic-link execution retry snapshot: {}",
                    retry_path.display()
                ));
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(format!(
                    "Execution retry snapshot path is not a regular file: {}",
                    retry_path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "Failed to inspect execution retry snapshot `{}`: {error}",
                    retry_path.display()
                ));
            }
        }
        // encoded is read once because reference construction hashes the same validated bytes.
        // encoded 只读取一次，因为引用构造会对同一份已校验字节计算摘要。
        let encoded = fs::read(&retry_path).map_err(|error| {
            format!(
                "Failed to read execution retry snapshot `{}`: {error}",
                retry_path.display()
            )
        })?;
        let snapshot = serde_json::from_slice::<ExecutionRetrySnapshot>(&encoded)
            .map_err(|error| format!("Failed to decode execution retry snapshot: {error}"))?;
        validate_snapshot_identity(&snapshot)?;
        let reference = execution_retry_reference_from_encoded(&snapshot, Some(&encoded))?;
        Ok(Some((snapshot, reference)))
    }

    /// Persists one final interrupted retry boundary to the single exact `retry.json` file.
    /// 将一个最终中断重试边界持久化到唯一精确的 `retry.json` 文件。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the final recoverable state after stop or abnormal termination is classified.
    /// `snapshot` 是停止或异常终止完成分类后的最终可恢复状态。
    ///
    /// # Returns
    /// 返回值
    /// A digest-bound durable reference only after exact-file publication succeeds.
    /// 仅在精确文件发布成功后返回摘要绑定的持久化引用。
    pub fn write_interrupted(
        &self,
        snapshot: &ExecutionRetrySnapshot,
    ) -> Result<ExecutionRetrySnapshotRef, String> {
        validate_snapshot_identity(snapshot)?;
        let encoded = serde_json::to_vec(snapshot)
            .map_err(|error| format!("Failed to serialize execution retry snapshot: {error}"))?;
        let reference = execution_retry_reference_from_encoded(snapshot, Some(&encoded))?;
        let retry_path = self.retry_path();
        // fileGuard serializes only sessions sharing this fixed path shard and never owns service state.
        // fileGuard 仅串行化共享当前固定路径分片的会话，且绝不持有服务状态。
        let _file_guard = lock_exact_retry_file(&retry_path)?;
        write_exact_retry_file(&retry_path, &encoded)?;
        Ok(reference)
    }

    /// Persists one terminal interruption when the unique retry file is absent or belongs to an older generation.
    /// 当唯一重试文件不存在或属于更旧代际时持久化一个终止中断边界。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the final in-memory retry body owned by the exact terminal transaction.
    /// `snapshot` 是由精确终止事务拥有的最终内存重试正文。
    ///
    /// # Returns
    /// 返回值
    /// A durable reference after creating or replacing an older exact file, or a stale-writer error when the current/newer generation already owns publication.
    /// 创建文件或替换更旧精确文件后返回可靠引用；若当前或更新代际已经拥有发布权，则返回陈旧写入者错误。
    pub fn write_interrupted_if_absent(
        &self,
        snapshot: &ExecutionRetrySnapshot,
    ) -> Result<ExecutionRetrySnapshotRef, String> {
        validate_snapshot_identity(snapshot)?;
        // encoded is computed before the file lock so serialization never extends the path-critical section.
        // encoded 在文件锁之前计算，避免序列化扩大路径临界区。
        let encoded = serde_json::to_vec(snapshot)
            .map_err(|error| format!("Failed to serialize execution retry snapshot: {error}"))?;
        // reference binds the exact bytes that this terminal transaction intends to publish.
        // reference 绑定当前终止事务准备发布的精确字节。
        let reference = execution_retry_reference_from_encoded(snapshot, Some(&encoded))?;
        // retryPath is the sole durable recovery body for the owning session.
        // retryPath 是所属会话唯一的可靠恢复正文。
        let retry_path = self.retry_path();
        // fileGuard makes stale-generation inspection and direct publication one indivisible path transaction.
        // fileGuard 使陈旧代际检查与直接发布构成不可分割的路径事务。
        let _file_guard = lock_exact_retry_file(&retry_path)?;
        match fs::read(&retry_path) {
            Ok(existing_encoded) => {
                // An exact duplicate is an idempotent observation of the already durable winner.
                // 精确重复正文表示已经可靠发布的获胜者被幂等观察。
                if existing_encoded == encoded {
                    return Ok(reference);
                }
                // existingSnapshot proves whether the one fixed file is an obsolete body left by
                // an earlier accepted turn; unreadable bytes are never overwritten speculatively.
                // existingSnapshot 用于证明唯一固定文件是否为更早已接纳回合遗留的过期正文；
                // 不可读取的字节绝不会被投机覆盖。
                let existing_snapshot = serde_json::from_slice::<ExecutionRetrySnapshot>(
                    &existing_encoded,
                )
                .map_err(|error| {
                    format!(
                        "Failed to decode existing execution retry snapshot `{}` before terminal publication: {error}",
                        retry_path.display()
                    )
                })?;
                validate_snapshot_identity(&existing_snapshot)?;
                if existing_snapshot.session_id != snapshot.session_id {
                    return Err(format!(
                        "Execution retry snapshot `{}` was not written because exact path `{}` belongs to another session.",
                        snapshot.snapshot_id,
                        retry_path.display()
                    ));
                }
                if existing_snapshot.generation >= snapshot.generation {
                    return Err(format!(
                        "Execution retry snapshot `{}` was not written because exact path `{}` already contains generation `{}`; another terminal writer already owns publication.",
                        snapshot.snapshot_id,
                        retry_path.display(),
                        existing_snapshot.generation
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Failed to inspect execution retry snapshot `{}` before terminal publication: {error}",
                    retry_path.display()
                ));
            }
        }
        write_exact_retry_file(&retry_path, &encoded)?;
        Ok(reference)
    }

    /// Reads and strictly validates one exact digest-bound retry snapshot.
    /// 读取并严格校验一份精确摘要绑定的重试快照。
    ///
    /// # Parameters
    /// 参数
    /// `reference` is the lightweight state reference loaded from the owning session.
    /// `reference` 是从所属会话加载的轻量状态引用。
    ///
    /// # Returns
    /// 返回值
    /// The exact decoded snapshot only when schema, identity, size, digest, generation, and state all match.
    /// 仅当 Schema、身份、大小、摘要、代际与状态全部一致时返回精确解码快照。
    pub fn read(
        &self,
        reference: &ExecutionRetrySnapshotRef,
    ) -> Result<ExecutionRetrySnapshot, String> {
        if reference.schema_version != EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported execution retry reference schema version `{}`; expected `{}`.",
                reference.schema_version, EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION
            ));
        }
        validate_sha256_hex(reference.content_sha256.as_str())?;
        let retry_path = self.retry_path();
        // fileGuard prevents an exact file replacement from interleaving with byte capture.
        // fileGuard 防止精确文件替换与字节捕获交错。
        let _file_guard = lock_exact_retry_file(&retry_path)?;
        let encoded = match fs::read(&retry_path) {
            Ok(encoded) => encoded,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(format!(
                    "Failed to read execution retry snapshot `{}` because exact path `{}` does not exist.",
                    reference.snapshot_id,
                    retry_path.display()
                ));
            }
            Err(error) => {
                return Err(format!(
                    "Failed to read execution retry snapshot `{}`: {error}",
                    retry_path.display()
                ));
            }
        };
        let actual_bytes = u64::try_from(encoded.len()).map_err(|_| {
            String::from("Execution retry snapshot size exceeds the supported u64 range.")
        })?;
        if actual_bytes != reference.content_bytes
            || sha256_hex(&encoded) != reference.content_sha256
        {
            return Err(String::from(
                "Execution retry snapshot byte length mismatch or digest mismatch.",
            ));
        }
        let snapshot = serde_json::from_slice::<ExecutionRetrySnapshot>(&encoded)
            .map_err(|error| format!("Failed to decode execution retry snapshot: {error}"))?;
        validate_snapshot_identity(&snapshot)?;
        let (retryable, requires_manual_confirmation, blocked_reason) =
            snapshot_safety_summary(&snapshot);
        let pending_action_label = pending_action_label(&snapshot.pending_action);
        let preparation_route_selection = preparation_route_selection(&snapshot.pending_action);
        let canonical_dispatch_sha256 = snapshot_canonical_dispatch_sha256(&snapshot)?;
        let error_summary = snapshot.error.as_ref().map(|error| error.message.clone());
        if snapshot.snapshot_id != reference.snapshot_id
            || snapshot.generation != reference.generation
            || snapshot.state != reference.state
            || snapshot.logical_execution_id != reference.logical_execution_id
            || snapshot.claimed_attempt_id != reference.claimed_attempt_id
            || pending_action_label != reference.pending_action_label
            || snapshot.provider_route != reference.provider_route
            || preparation_route_selection != reference.preparation_route_selection
            || snapshot.context_generation_id != reference.context_generation_id
            || canonical_dispatch_sha256 != reference.canonical_dispatch_sha256
            || retryable != reference.retryable
            || requires_manual_confirmation != reference.requires_manual_confirmation
            || blocked_reason != reference.blocked_reason
            || error_summary != reference.error_summary
        {
            return Err(String::from(
                "Execution retry snapshot identity, safety summary, or lifecycle state does not match its reference.",
            ));
        }
        Ok(snapshot)
    }

    /// Deletes the single retry file after the owning main-state reference is durably cleared.
    /// 在所属主状态引用被可靠清除后删除唯一重试文件。
    ///
    /// # Parameters
    /// 参数
    /// `reference` is the last durable owner used to validate deletion intent.
    /// `reference` 是用于校验删除意图的最后一个可靠所有者。
    ///
    /// # Returns
    /// 返回值
    /// Success when the referenced body is absent, removed, or already replaced by another valid snapshot.
    /// 引用正文不存在、已删除或已被另一份有效快照替换时返回成功。
    pub fn delete(&self, reference: &ExecutionRetrySnapshotRef) -> Result<(), String> {
        validate_snapshot_id(reference.snapshot_id.as_str())?;
        validate_sha256_hex(reference.content_sha256.as_str())?;
        let retry_path = self.retry_path();
        // fileGuard keeps digest validation and removal atomic relative to a replacement interruption.
        // fileGuard 使摘要校验与删除相对于后续中断替换保持原子性。
        let _file_guard = lock_exact_retry_file(&retry_path)?;
        let encoded = match fs::read(&retry_path) {
            Ok(encoded) => encoded,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "Failed to read execution retry snapshot before deletion `{}`: {error}",
                    retry_path.display()
                ));
            }
        };
        let actual_bytes = u64::try_from(encoded.len()).map_err(|_| {
            String::from("Execution retry snapshot size exceeds the supported u64 range.")
        })?;
        if actual_bytes != reference.content_bytes
            || sha256_hex(&encoded) != reference.content_sha256
        {
            // current_snapshot distinguishes a valid superseding owner from corrupted bytes. A stale
            // exact-reference delete is already satisfied when another valid owner occupies retry.json.
            // current_snapshot 区分有效替代所有者与损坏字节；当另一有效所有者占用 retry.json 时，
            // 过期精确引用的删除意图已经满足。
            let current_snapshot = serde_json::from_slice::<ExecutionRetrySnapshot>(&encoded)
                .map_err(|error| {
                    format!(
                        "Execution retry snapshot changed before deletion and the current exact file is invalid: {error}"
                    )
                })?;
            validate_snapshot_identity(&current_snapshot).map_err(|error| {
                format!(
                    "Execution retry snapshot changed before deletion and the current exact file has invalid identity: {error}"
                )
            })?;
            return Ok(());
        }
        fs::remove_file(&retry_path).map_err(|error| {
            format!(
                "Failed to remove execution retry snapshot `{}`: {error}",
                retry_path.display()
            )
        })?;
        synchronize_retry_parent_directory(&retry_path)
    }

    /// Returns the only durable retry path below the exact session root.
    /// 返回精确会话根目录下唯一的持久化重试路径。
    ///
    /// # Returns
    /// 返回值
    /// The deterministic session-root `retry.json` path without directory discovery.
    /// 不进行目录发现即可确定的会话根目录 `retry.json` 路径。
    fn retry_path(&self) -> PathBuf {
        self.session_directory.join(EXECUTION_RETRY_FILE)
    }
}

/// Locks the bounded serialization channel selected by one exact retry path.
/// 锁定一个精确重试路径选择的有界串行化通道。
///
/// # Parameters
/// 参数
/// `path` is the deterministic session-root retry file path.
/// `path` 是确定的会话根目录重试文件路径。
///
/// # Returns
/// 返回值
/// A process-wide shard guard, or an explicit poison diagnostic.
/// 返回进程级分片锁守卫，或明确的锁中毒诊断。
fn lock_exact_retry_file(path: &std::path::Path) -> Result<MutexGuard<'static, ()>, String> {
    // hasher maps the full platform path into one bounded synchronization channel.
    // hasher 把完整平台路径映射到一个有界同步通道。
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    let shard_index = (hasher.finish() as usize) % EXECUTION_RETRY_FILE_LOCK_SHARDS;
    EXECUTION_RETRY_FILE_LOCKS[shard_index]
        .lock()
        .map_err(|error| format!("Execution retry file lock was poisoned: {error}"))
}

/// Writes bytes only to the fixed retry path and never creates a sibling staging snapshot.
/// 仅把字节写入固定重试路径，绝不创建同级暂存快照。
///
/// # Parameters
/// 参数
/// `path` is the exact session-root `retry.json` path.
/// `path` 是会话根目录下精确的 `retry.json` 路径。
/// `encoded` is the complete serialized interrupted boundary.
/// `encoded` 是完整序列化后的中断边界。
///
/// # Returns
/// 返回值
/// Returns success only after the single file handle has completed `sync_all`.
/// 仅在唯一文件句柄完成 `sync_all` 后返回成功。
fn write_exact_retry_file(path: &std::path::Path, encoded: &[u8]) -> Result<(), String> {
    // retryFilePreexisted distinguishes content replacement from first directory-entry publication.
    // retryFilePreexisted 区分正文替换与首次目录项发布。
    let retry_file_preexisted = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "Refusing to replace symbolic-link execution retry snapshot: {}",
                path.display()
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!(
                "Execution retry snapshot path is not a regular file: {}",
                path.display()
            ));
        }
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(format!(
                "Failed to inspect execution retry snapshot `{}`: {error}",
                path.display()
            ));
        }
    };
    // writeResult captures the complete open, truncate, write, and durability operation.
    // writeResult 捕获完整的打开、截断、写入与耐久化操作。
    let write_result = (|| -> std::io::Result<()> {
        // file is the only filesystem handle used for the interrupted retry boundary.
        // file 是中断重试边界使用的唯一文件系统句柄。
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.write_all(encoded)?;
        file.sync_all()
    })();
    if let Err(error) = write_result {
        // A failed direct write can only leave an invalid replacement; remove that exact file so it
        // cannot masquerade as the previous durable boundary after restart.
        // 直接写入失败只会留下无效替换文件；精确删除该文件，避免重启后伪装成旧持久边界。
        let _ = fs::remove_file(path);
        return Err(format!(
            "Failed to write interrupted execution retry snapshot `{}`: {error}",
            path.display()
        ));
    }
    if !retry_file_preexisted && let Err(error) = synchronize_retry_parent_directory(path) {
        // A first publication without a durable directory entry cannot own a reliable recovery reference.
        // 首次发布若没有可靠目录项，就不能拥有可靠恢复引用。
        let _ = fs::remove_file(path);
        return Err(format!(
            "Failed to synchronize interrupted execution retry snapshot directory `{}`: {error}",
            path.display()
        ));
    }
    Ok(())
}

/// Synchronizes the parent directory entry for a newly created retry file on POSIX.
/// 在 POSIX 上同步新建重试文件的父目录项。
///
/// # Parameters
/// 参数
/// `path` is the newly created exact session-root `retry.json` path.
/// `path` 是刚创建的会话根目录精确 `retry.json` 路径。
///
/// # Returns
/// 返回值
/// Success after directory metadata is durable, or immediately on platforms without directory sync.
/// 目录元数据可靠落盘后返回成功；不支持目录同步的平台立即返回成功。
#[cfg(unix)]
fn synchronize_retry_parent_directory(path: &std::path::Path) -> Result<(), String> {
    // parent is the pre-existing session directory that owns the single retry entry.
    // parent 是拥有唯一重试目录项的既有会话目录。
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let directory = fs::File::open(parent).map_err(|error| {
        format!(
            "Failed to open retry snapshot parent directory `{}`: {error}",
            parent.display()
        )
    })?;
    directory.sync_all().map_err(|error| {
        format!(
            "Failed to synchronize retry snapshot parent directory `{}`: {error}",
            parent.display()
        )
    })
}

/// Completes retry parent publication on platforms where file synchronization owns the directory entry.
/// 在文件同步即负责目录项发布的平台完成重试父目录发布。
///
/// # Parameters
/// 参数
/// `path` is the newly created exact retry path and is intentionally unused on this platform.
/// `path` 是刚创建的精确重试路径，在当前平台上有意不使用。
///
/// # Returns
/// 返回值
/// Immediate success because the platform has no POSIX directory synchronization contract.
/// 立即返回成功，因为当前平台没有 POSIX 目录同步契约。
#[cfg(not(unix))]
fn synchronize_retry_parent_directory(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

/// Builds a lightweight reference from a live body or exact interrupted bytes.
/// 根据实时正文或精确中断字节构建轻量引用。
///
/// # Parameters
/// 参数
/// `snapshot` is the validated in-memory state and `encoded` is present only for a real interruption.
/// `snapshot` 是已校验的内存状态，`encoded` 仅在真实中断时存在。
///
/// # Returns
/// 返回值
/// A complete lightweight reference whose durable body size is zero until interruption persistence.
/// 返回完整轻量引用；在中断持久化前，其持久正文大小为零。
fn execution_retry_reference_from_encoded(
    snapshot: &ExecutionRetrySnapshot,
    encoded: Option<&[u8]>,
) -> Result<ExecutionRetrySnapshotRef, String> {
    // canonicalDispatchSha256 reuses the already computed complete Core digest.
    // canonicalDispatchSha256 复用已经计算完成的完整 Core 摘要。
    let canonical_dispatch_sha256 = snapshot_canonical_dispatch_sha256(snapshot)?;
    // contentIdentity is durable only when a real interruption supplies complete encoded bytes.
    // 仅当真实中断提供完整编码字节时，contentIdentity 才是持久身份。
    let (content_sha256, content_bytes) = match encoded {
        Some(encoded) => {
            let content_bytes = u64::try_from(encoded.len()).map_err(|_| {
                String::from("Execution retry snapshot size exceeds the supported u64 range.")
            })?;
            (sha256_hex(encoded), content_bytes)
        }
        None => (
            transient_execution_retry_reference_sha256(snapshot, &canonical_dispatch_sha256),
            0,
        ),
    };
    // replaySafety is copied into the lightweight reference so UI admission never guesses from ledger state alone.
    // replaySafety 写入轻量引用，使 UI 准入不会只根据账本状态猜测。
    let (retryable, requires_manual_confirmation, blocked_reason) =
        snapshot_safety_summary(snapshot);
    // preparationRouteSelection mirrors the exact pre-resolution selection only for preparation replay.
    // preparationRouteSelection 仅为准备阶段重放镜像精确的解析前选择。
    let preparation_route_selection = preparation_route_selection(&snapshot.pending_action);
    Ok(ExecutionRetrySnapshotRef {
        schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
        snapshot_id: snapshot.snapshot_id.clone(),
        generation: snapshot.generation,
        state: snapshot.state,
        content_sha256,
        content_bytes,
        logical_execution_id: snapshot.logical_execution_id.clone(),
        pending_action_label: pending_action_label(&snapshot.pending_action).to_string(),
        provider_route: snapshot.provider_route.clone(),
        preparation_route_selection,
        context_generation_id: snapshot.context_generation_id.clone(),
        canonical_dispatch_sha256,
        retryable,
        requires_manual_confirmation,
        blocked_reason,
        claimed_attempt_id: snapshot.claimed_attempt_id.clone(),
        error_summary: snapshot.error.as_ref().map(|error| error.message.clone()),
        updated_at: snapshot.updated_at.clone(),
    })
}

/// Computes a bounded process-local CAS digest without serializing the complete retry body.
/// 在不序列化完整重试正文的情况下计算有界进程内 CAS 摘要。
///
/// # Parameters
/// 参数
/// `snapshot` supplies lifecycle identity and `canonical_dispatch_sha256` supplies canonical state identity.
/// `snapshot` 提供生命周期身份，`canonical_dispatch_sha256` 提供规范状态身份。
///
/// # Returns
/// 返回值
/// A lowercase SHA-256 digest that changes with every owned live transition.
/// 返回随每个受控实时迁移变化的小写 SHA-256 摘要。
fn transient_execution_retry_reference_sha256(
    snapshot: &ExecutionRetrySnapshot,
    canonical_dispatch_sha256: &str,
) -> String {
    let mut digest = Sha256::new();
    update_transient_digest_field(&mut digest, snapshot.snapshot_id.as_bytes());
    digest.update(snapshot.generation.to_le_bytes());
    let state_code = match snapshot.state {
        vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen => 0_u8,
        vulcan_workbench_model::ExecutionRetrySnapshotState::Claimed => 1_u8,
        vulcan_workbench_model::ExecutionRetrySnapshotState::Consumed => 2_u8,
        vulcan_workbench_model::ExecutionRetrySnapshotState::Invalidated => 3_u8,
    };
    digest.update([state_code]);
    update_transient_digest_field(&mut digest, snapshot.logical_execution_id.as_bytes());
    update_transient_digest_field(
        &mut digest,
        pending_action_label(&snapshot.pending_action).as_bytes(),
    );
    update_transient_digest_field(&mut digest, canonical_dispatch_sha256.as_bytes());
    update_transient_digest_field(
        &mut digest,
        snapshot
            .claimed_attempt_id
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    update_transient_digest_field(&mut digest, snapshot.updated_at.as_bytes());
    update_transient_digest_field(
        &mut digest,
        snapshot
            .error
            .as_ref()
            .map(|error| error.message.as_str())
            .unwrap_or_default()
            .as_bytes(),
    );
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Adds one length-delimited field to a transient retry digest.
/// 向临时重试摘要添加一个带长度边界的字段。
///
/// # Parameters
/// 参数
/// `digest` is the active SHA-256 state and `value` is one exact field body.
/// `digest` 是活动 SHA-256 状态，`value` 是一个精确字段正文。
fn update_transient_digest_field(digest: &mut Sha256, value: &[u8]) {
    digest.update(value.len().to_le_bytes());
    digest.update(value);
}

/// Validates strict snapshot schema and content-independent identity fields.
/// 校验严格快照 Schema 与内容无关身份字段。
fn validate_snapshot_identity(snapshot: &ExecutionRetrySnapshot) -> Result<(), String> {
    if snapshot.schema_version != EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported execution retry snapshot schema version `{}`; expected `{}`.",
            snapshot.schema_version, EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION
        ));
    }
    validate_snapshot_id(snapshot.snapshot_id.as_str())?;
    if snapshot.session_id.trim().is_empty()
        || snapshot.logical_execution_id.trim().is_empty()
        || snapshot.physical_attempt_id.trim().is_empty()
    {
        return Err(String::from(
            "Execution retry snapshot is missing a required stable identity or digest.",
        ));
    }
    validate_sha256_hex(snapshot.canonical_digest.as_str())?;
    if snapshot
        .user_followup
        .as_ref()
        .is_some_and(|followup| !followup.is_structurally_valid())
    {
        return Err(String::from(
            "Execution retry snapshot contains an invalid user follow-up boundary.",
        ));
    }
    Ok(())
}

/// Derives the safety summary copied into the lightweight reference.
/// 推导写入轻量引用的安全摘要。
fn snapshot_safety_summary(snapshot: &ExecutionRetrySnapshot) -> (bool, bool, Option<String>) {
    let pending_action_supported = matches!(
        &snapshot.pending_action,
        ExecutionRetryPendingAction::ResumePreparation { .. }
            | ExecutionRetryPendingAction::DispatchProviderRequest { .. }
            | ExecutionRetryPendingAction::ExecuteToolBatch { .. }
            | ExecutionRetryPendingAction::PrepareNextProviderRequest
            | ExecutionRetryPendingAction::FinalizeAssistant
    );
    let action_boundary_blocked_reason = pending_action_boundary_blocked_reason(snapshot);
    let blocked_reason = if !pending_action_supported {
        Some(String::from("pending_action_executor_unavailable"))
    } else {
        action_boundary_blocked_reason
    };
    let retryable = blocked_reason.is_none();
    let requires_manual_confirmation = blocked_reason.is_some();
    (retryable, requires_manual_confirmation, blocked_reason)
}

/// Returns the exact pre-resolution route selection mirrored by one preparation action.
/// 返回一个准备动作镜像的精确解析前路由选择。
///
/// # Parameters
/// 参数
/// `action` is the complete frozen pending action stored in the sidecar.
/// `action` 是 sidecar 中保存的完整冻结待执行动作。
///
/// # Returns
/// 返回值
/// A present selection for `ResumePreparation`, otherwise `None`.
/// 对 `ResumePreparation` 返回存在的选择，其他动作返回 `None`。
fn preparation_route_selection(
    action: &ExecutionRetryPendingAction,
) -> Option<ExecutionRetryPreparationRouteSelection> {
    match action {
        ExecutionRetryPendingAction::ResumePreparation { input } => {
            Some(ExecutionRetryPreparationRouteSelection {
                active_provider_id: input.active_provider_id.clone(),
                active_connection_id: input.active_connection_id.clone(),
                active_model_id: input.active_model_id.clone(),
            })
        }
        _ => None,
    }
}

/// Returns a stable structural blocker when the frozen action lacks its required route boundary.
/// 当冻结动作缺少所需路由边界时返回稳定的结构性阻断原因。
///
/// # Parameters
/// 参数
/// `snapshot` is the complete sidecar whose action and controller boundary are inspected together.
/// `snapshot` 是同时检查动作与控制器边界的完整 sidecar。
///
/// # Returns
/// 返回值
/// A stable WebUI-localized reason, or `None` when the action boundary is complete.
/// 返回稳定的 WebUI 本地化原因；动作边界完整时返回 `None`。
fn pending_action_boundary_blocked_reason(snapshot: &ExecutionRetrySnapshot) -> Option<String> {
    match &snapshot.pending_action {
        ExecutionRetryPendingAction::DispatchProviderRequest { .. }
            if snapshot.provider_route.is_none() =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
            ))
        }
        ExecutionRetryPendingAction::PrepareNextProviderRequest
            if snapshot.provider_route.is_none() =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
            ))
        }
        ExecutionRetryPendingAction::ExecuteToolBatch { .. }
            if snapshot.provider_route.is_none() =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
            ))
        }
        ExecutionRetryPendingAction::FinalizeAssistant if snapshot.provider_route.is_none() => {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_ROUTE_MISSING",
            ))
        }
        ExecutionRetryPendingAction::ExecuteToolBatch { tools } => {
            execute_tool_batch_boundary_blocked_reason(snapshot, tools)
        }
        ExecutionRetryPendingAction::PrepareNextProviderRequest
            if !vulcan_core::controller::conversation_snapshot_has_seeded_provider_input(
                &snapshot.controller_snapshot,
            ) =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_NEXT_PROVIDER_BOUNDARY_MISSING",
            ))
        }
        ExecutionRetryPendingAction::FinalizeAssistant
            if !matches!(
                snapshot
                    .controller_snapshot
                    .messages
                    .last()
                    .map(|message| &message.payload),
                Some(vulcan_core::types::ConversationPayload::Assistant { tool_calls, .. })
                    if tool_calls.is_empty()
            ) =>
        {
            Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_FINAL_ASSISTANT_BOUNDARY_MISSING",
            ))
        }
        _ => None,
    }
}

/// Validates an exact retained assistant tool batch and every per-tool recovery slot.
/// 校验精确保留的助手工具批次及每个逐工具恢复槽位。
fn execute_tool_batch_boundary_blocked_reason(
    snapshot: &ExecutionRetrySnapshot,
    tools: &[ExecutionRetryToolProgress],
) -> Option<String> {
    let Some(vulcan_core::types::ConversationPayload::Assistant {
        tool_calls: assistant_calls,
        ..
    }) = snapshot
        .controller_snapshot
        .messages
        .last()
        .map(|message| &message.payload)
    else {
        return Some(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING",
        ));
    };
    if assistant_calls.is_empty() || assistant_calls.len() != tools.len() {
        return Some(String::from(
            "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BOUNDARY_MISSING",
        ));
    }

    let mut observed_tool_call_ids = std::collections::HashSet::new();
    for (assistant_call, progress) in assistant_calls.iter().zip(tools) {
        // expectedToolCallId uses the same canonical resolver as normal execution and crash-gap reconstruction.
        // expectedToolCallId 使用与正常执行及崩溃窗口重建相同的规范解析器。
        let expected_tool_call_id = vulcan_core::controller::resolved_tool_call_id(assistant_call);
        // An unresolved binding is valid only for a proven-unstarted slot. The mandatory
        // BatchPrepared durability barrier must replace it before any Started transition.
        // 未解析绑定仅对已证明未启动的槽位有效。强制 BatchPrepared 可靠屏障必须在任何
        // Started 迁移前填充该绑定。
        let binding_fingerprint_valid =
            match (&progress.state, progress.binding_fingerprint.as_deref()) {
                (ExecutionRetryToolState::NotStarted, None) => true,
                (_, Some(binding_fingerprint)) => {
                    binding_fingerprint.len() == 64
                        && binding_fingerprint
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }
                (_, None) => false,
            };
        if !observed_tool_call_ids.insert(progress.request.tool_call_id.as_str())
            || progress.request.tool_call_id != expected_tool_call_id
            || progress.request.tool_name != assistant_call.tool_name
            || progress.request.arguments != assistant_call.arguments
            || !binding_fingerprint_valid
        {
            return Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH",
            ));
        }

        let result_matches_request = progress.result.as_ref().is_none_or(|result| {
            result.tool_call_id == progress.request.tool_call_id
                && result.tool_name == progress.request.tool_name
        });
        let state_matches_result = match (&progress.state, &progress.result) {
            (
                ExecutionRetryToolState::NotStarted
                | ExecutionRetryToolState::Running
                | ExecutionRetryToolState::OutcomeUnknown,
                None,
            ) => true,
            (ExecutionRetryToolState::Completed, Some(result)) => !result.is_error,
            (ExecutionRetryToolState::Failed, Some(result)) => result.is_error,
            _ => false,
        };
        if !result_matches_request || !state_matches_result {
            return Some(String::from(
                "ERR_CONVERSATION_CONTINUE_RETRY_TOOL_RESULT_MISMATCH",
            ));
        }
    }
    None
}

/// Computes the claim-equivalent digest of provider-visible messages and continuation state.
/// 计算 Provider 可见消息与 continuation 状态的领取等价摘要。
///
/// # Parameters
/// 参数
/// `snapshot` owns the frozen canonical controller state.
/// `snapshot` 持有冻结的规范控制器状态。
///
/// # Returns
/// 返回值
/// The cached lowercase SHA-256 digest, or an exact validation error.
/// 返回缓存的小写 SHA-256 摘要，或精确校验错误。
fn snapshot_canonical_dispatch_sha256(snapshot: &ExecutionRetrySnapshot) -> Result<String, String> {
    validate_sha256_hex(snapshot.canonical_digest.as_str())?;
    Ok(snapshot.canonical_digest.clone())
}

/// Rejects path separators and non-UUID snapshot identities before filesystem resolution.
/// 在文件系统解析前拒绝路径分隔符与非 UUID 快照身份。
fn validate_snapshot_id(snapshot_id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(snapshot_id)
        .map(|_| ())
        .map_err(|_| String::from("Execution retry snapshot id must be an exact UUID."))
}

/// Rejects malformed or path-like digest text before resolving a content-addressed body.
/// 在解析内容寻址正文前拒绝格式错误或路径式摘要文本。
fn validate_sha256_hex(content_sha256: &str) -> Result<(), String> {
    if content_sha256.len() == 64
        && content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(String::from(
        "Execution retry snapshot digest must be exactly 64 lowercase hexadecimal characters.",
    ))
}

/// Returns the stable lightweight label for one exact pending action.
/// 返回一个精确待执行动作的稳定轻量标签。
fn pending_action_label(action: &ExecutionRetryPendingAction) -> &'static str {
    match action {
        ExecutionRetryPendingAction::ResumePreparation { .. } => "resume_preparation",
        ExecutionRetryPendingAction::DispatchProviderRequest { .. } => "dispatch_provider_request",
        ExecutionRetryPendingAction::ExecuteToolBatch { .. } => "execute_tool_batch",
        ExecutionRetryPendingAction::PrepareNextProviderRequest => "prepare_next_provider_request",
        ExecutionRetryPendingAction::FinalizeAssistant => "finalize_assistant",
    }
}

/// Computes a lowercase SHA-256 digest for exact serialized bytes.
/// 为精确序列化字节计算小写 SHA-256 摘要。
fn sha256_hex(encoded: &[u8]) -> String {
    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    //! Tests for memory-first retry projection and strict single-file interruption storage.
    //! 内存优先重试投影与严格单文件中断存储测试。

    use super::*;
    use vulcan_ai_contract::{AiErrorCategory, AiExecutionError, DispatchOutcome};
    use vulcan_workbench_model::{ExecutionRetrySnapshotState, SessionProviderRouteLeaseIdentity};

    /// Builds one complete minimal retry snapshot for storage contract tests.
    /// 为存储契约测试构建一份完整的最小重试快照。
    fn test_snapshot(snapshot_id: String) -> ExecutionRetrySnapshot {
        // controllerSnapshot ends at the exact completed tool boundary required by the pending action.
        // controllerSnapshot 结束于待执行动作要求的精确已完成工具边界。
        let controller_snapshot = vulcan_core::types::ConversationSnapshot {
            messages: vec![vulcan_core::types::ConversationMessage {
                id: String::from("tool-result-1"),
                payload: vulcan_core::types::ConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("tool-call-1")),
                    is_error: false,
                    text: String::from("completed tool result"),
                },
            }],
            ..vulcan_core::types::ConversationSnapshot::default()
        };
        ExecutionRetrySnapshot {
            schema_version: EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id,
            session_id: uuid::Uuid::from_u128(42).to_string(),
            generation: 3,
            state: ExecutionRetrySnapshotState::Frozen,
            logical_execution_id: String::from("logical-execution-1"),
            logical_turn_id: Some(String::from("turn-1")),
            next_provider_attempt_index: 0,
            physical_attempt_id: String::from("attempt-1"),
            claimed_attempt_id: None,
            provider_route: Some(SessionProviderRouteLeaseIdentity {
                schema_version: 1,
                provider_id: String::from("provider-a"),
                connection_id: String::from("connection-a"),
                model_id: String::from("model-a"),
                protocol_profile_id: String::from("openai-responses.v1"),
                manifest_revision: 1,
                manifest_generation: 1,
                catalog_revision: String::from("catalog-a"),
                route_revision: String::from("route-a"),
            }),
            provider_service_entry_id: Some(String::from("entry-a")),
            credential_profile_id: Some(String::from("credential-a")),
            controller_config: Some(vulcan_core::types::ControllerConfig::default()),
            controller_snapshot,
            pending_action: ExecutionRetryPendingAction::PrepareNextProviderRequest,
            user_followup: None,
            error: None,
            canonical_digest: "a".repeat(64),
            context_generation_id: Some(String::from("context-generation-a")),
            created_at: String::from("1000"),
            updated_at: String::from("1000"),
        }
    }

    /// Builds one valid interrupted assistant tool batch with an exact not-started recovery slot.
    /// 构建一个有效的中断助手工具批次，并包含精确的未启动恢复槽位。
    fn test_execute_tool_batch_snapshot(snapshot_id: String) -> ExecutionRetrySnapshot {
        // snapshot reuses the complete route and identity fixture before replacing its canonical tail.
        // snapshot 在替换规范尾部前复用完整的路由与身份测试夹具。
        let mut snapshot = test_snapshot(snapshot_id);
        // request is the exact model-issued invocation retained by the recovery slot.
        // request 是恢复槽位保留的模型原始精确调用。
        let request = vulcan_core::types::ToolInvocationRequest {
            tool_call_id: String::from("tool-item-1"),
            tool_name: String::from("read_file"),
            arguments: serde_json::json!({"path": "README.md"}),
        };
        snapshot.controller_snapshot.messages = vec![vulcan_core::types::ConversationMessage {
            id: String::from("assistant-tool-batch-1"),
            payload: vulcan_core::types::ConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: Some(String::from("provider-message-1")),
                provider_message_state: None,
                usage: None,
                tool_calls: vec![vulcan_core::types::ConversationToolCall {
                    id: String::from("tool-item-1"),
                    // The fixture proves storage resolves a blank Provider id exactly like normal Core execution.
                    // 该测试夹具证明存储层与 Core 正常执行使用同一空白 Provider 标识解析规则。
                    tool_call_id: Some(String::from("   ")),
                    tool_name: request.tool_name.clone(),
                    arguments: request.arguments.clone(),
                }],
            },
        }];
        snapshot.pending_action = ExecutionRetryPendingAction::ExecuteToolBatch {
            tools: vec![ExecutionRetryToolProgress {
                request,
                result: None,
                binding_fingerprint: Some("a".repeat(64)),
                state: ExecutionRetryToolState::NotStarted,
            }],
        };
        snapshot
    }

    /// Verifies atomic publication round-trips through a digest-bound lightweight reference.
    /// 验证原子发布能够通过摘要绑定轻量引用完整往返。
    #[test]
    fn retry_snapshot_round_trips_through_digest_reference() {
        // tempRoot owns the exact isolated session directory used by this test.
        // tempRoot 持有当前测试使用的精确隔离会话目录。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store binds every sidecar operation to the isolated session directory.
        // store 将每个 sidecar 操作绑定到隔离会话目录。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());

        let reference = store
            .write_interrupted(&snapshot)
            .expect("snapshot should persist");
        let restored = store.read(&reference).expect("snapshot should restore");

        // sidecarPath proves every interrupted boundary targets the single deterministic filename.
        // sidecarPath 证明每个中断边界都指向唯一确定的文件名。
        let sidecar_path = store.retry_path();
        assert_eq!(
            sidecar_path.file_name().and_then(std::ffi::OsStr::to_str),
            Some(EXECUTION_RETRY_FILE)
        );
        assert!(sidecar_path.is_file());

        assert_eq!(restored.snapshot_id, snapshot.snapshot_id);
        assert_eq!(restored.generation, snapshot.generation);
        assert_eq!(restored.canonical_digest, snapshot.canonical_digest);
        assert!(reference.retryable);
        assert!(!reference.requires_manual_confirmation);
        assert_eq!(reference.blocked_reason, None);
        assert_eq!(reference.provider_route, snapshot.provider_route);
        assert_eq!(reference.preparation_route_selection, None);
        assert_eq!(
            reference.context_generation_id,
            snapshot.context_generation_id
        );
        assert_eq!(reference.canonical_dispatch_sha256.len(), 64);
        assert_eq!(reference.content_sha256.len(), 64);
        assert!(reference.content_bytes > 0);
    }

    /// Verifies a normal live checkpoint remains memory-only and cannot advertise durable Continue.
    /// 验证正常实时检查点仅保留在内存中，且不能宣称持久 Continue。
    #[test]
    fn live_retry_projection_does_not_create_retry_json() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());

        let reference = ExecutionRetrySnapshotStore::project(&snapshot)
            .expect("live snapshot should project without persistence");

        assert_eq!(reference.content_bytes, 0);
        assert!(reference.live_boundary_safety_proven());
        assert!(!reference.replay_safety_proven());
        assert!(!store.retry_path().exists());
    }

    /// Verifies exact orphan reconciliation reads only the fixed file and preserves its digest identity.
    /// 验证精确孤立文件协调只读取固定文件，并保留其摘要身份。
    #[test]
    fn orphan_retry_reference_loads_from_the_exact_fixed_file() {
        // tempRoot owns the one deterministic session-root retry path.
        // tempRoot 持有唯一确定的会话根目录重试路径。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        assert_eq!(
            store
                .load_interrupted_reference_if_present()
                .expect("missing exact file should be a valid absence")
                .map(|(_, reference)| reference),
            None
        );
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let durable_reference = store
            .write_interrupted(&snapshot)
            .expect("interrupted body should persist");

        assert_eq!(
            store
                .load_interrupted_reference_if_present()
                .expect("fixed interrupted body should load")
                .map(|(_, reference)| reference),
            Some(durable_reference)
        );
    }

    /// Verifies a stale terminal writer cannot overwrite the unique retry body published by the winner.
    /// 验证陈旧终止写入者不能覆盖获胜写入者发布的唯一重试正文。
    #[test]
    fn terminal_retry_publication_rejects_a_second_writer() {
        // tempRoot isolates the fixed retry path shared by both simulated terminal writers.
        // tempRoot 隔离两个模拟终止写入者共享的固定重试路径。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store owns the absence check and direct write under one exact-path lock.
        // store 在同一精确路径锁下拥有不存在检查与直接写入。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // winnerSnapshot is the first terminal body allowed to claim the absent file.
        // winnerSnapshot 是首个获准领取不存在文件的终止正文。
        let winner_snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // staleSnapshot differs so any accidental overwrite is observable by its digest and identity.
        // staleSnapshot 保持不同，以便通过摘要与身份观察任何意外覆盖。
        let mut stale_snapshot = winner_snapshot.clone();
        stale_snapshot.snapshot_id = uuid::Uuid::new_v4().to_string();
        stale_snapshot.updated_at = String::from("4000");

        let winner_reference = store
            .write_interrupted_if_absent(&winner_snapshot)
            .expect("first terminal writer should publish the absent file");
        let stale_error = store
            .write_interrupted_if_absent(&stale_snapshot)
            .expect_err("second terminal writer must not replace the winner");

        assert!(stale_error.contains("another terminal writer already owns publication"));
        assert_eq!(
            store
                .read(&winner_reference)
                .expect("winner body must remain readable")
                .snapshot_id,
            winner_snapshot.snapshot_id
        );
    }

    /// Verifies a newly interrupted generation replaces one obsolete fixed-file body without creating a sibling snapshot.
    /// 验证新中断代际会替换固定文件中的旧正文，且不会创建同级快照。
    #[test]
    fn terminal_retry_publication_replaces_an_older_generation() {
        // tempRoot isolates the one deterministic retry file used by both generations.
        // tempRoot 隔离两个代际共同使用的唯一确定重试文件。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store owns exact-path replacement and digest validation.
        // store 负责精确路径替换与摘要校验。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // obsoleteSnapshot models the unreferenced durable body from the previously accepted turn.
        // obsoleteSnapshot 模拟上一个已接纳回合留下且不再被引用的可靠正文。
        let obsolete_snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let obsolete_reference = store
            .write_interrupted(&obsolete_snapshot)
            .expect("obsolete generation should initially persist");
        // interruptedSnapshot is the process-owned checkpoint of the newer accepted turn.
        // interruptedSnapshot 是更新已接纳回合在进程内拥有的检查点。
        let mut interrupted_snapshot = obsolete_snapshot.clone();
        interrupted_snapshot.snapshot_id = uuid::Uuid::new_v4().to_string();
        interrupted_snapshot.generation += 1;
        interrupted_snapshot.updated_at = String::from("5000");

        let interrupted_reference = store
            .write_interrupted_if_absent(&interrupted_snapshot)
            .expect("newer terminal generation should replace the obsolete fixed body");

        assert!(
            store.read(&obsolete_reference).is_err(),
            "the obsolete digest must no longer resolve"
        );
        assert_eq!(
            store
                .read(&interrupted_reference)
                .expect("newer interrupted body must remain readable")
                .snapshot_id,
            interrupted_snapshot.snapshot_id
        );
        assert_eq!(
            store
                .retry_path()
                .file_name()
                .and_then(std::ffi::OsStr::to_str),
            Some(EXECUTION_RETRY_FILE),
            "replacement must retain the one fixed retry filename"
        );
    }

    /// Verifies every Core-authorized Provider seed produces a replay-safe PrepareNext reference.
    /// 验证 Core 授权的每种 Provider 播种输入都会生成可安全重放的 PrepareNext 引用。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if storage projection narrows the shared Core boundary contract.
    /// 此测试无返回值；若存储投影缩窄共享 Core 边界契约，则测试失败。
    #[test]
    fn prepare_next_reference_accepts_every_core_provider_seed() {
        // providerSeeds enumerate the complete shared Core contract used by runtime interventions.
        // providerSeeds 枚举运行时干预使用的完整共享 Core 契约。
        let provider_seeds = vec![
            vulcan_core::types::ConversationPayload::UserText {
                text: String::from("continue with the user supplement"),
                attachments: Vec::new(),
            },
            vulcan_core::types::ConversationPayload::AgentMessage {
                message_type: String::from("message"),
                task_name: String::from("/root"),
                sender: String::from("/root/reviewer"),
                recipient: String::from("/root"),
                payload: String::from("continue after mailbox delivery"),
                trigger_turn: true,
                metadata: None,
            },
            vulcan_core::types::ConversationPayload::ToolResult {
                tool_call_id: Some(String::from("tool-call-1")),
                is_error: false,
                text: String::from("completed tool result"),
            },
        ];

        for (seed_index, payload) in provider_seeds.into_iter().enumerate() {
            // tempRoot isolates the immutable sidecar body for this exact seed kind.
            // tempRoot 隔离当前精确播种输入类型的不可变 sidecar 正文。
            let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
            // store derives the lightweight replay proof from the complete body.
            // store 从完整正文派生轻量重放证明。
            let store = ExecutionRetrySnapshotStore::new(temp_root.path());
            // snapshot replaces only the canonical tail while preserving the valid route and action fixture.
            // snapshot 仅替换规范尾部，同时保留有效路由与动作夹具。
            let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
            snapshot.controller_snapshot.messages = vec![vulcan_core::types::ConversationMessage {
                id: format!("provider-seed-{seed_index}"),
                payload,
            }];

            let reference = store
                .write_interrupted(&snapshot)
                .expect("authorized Provider seed should persist");

            assert!(reference.retryable);
            assert!(!reference.requires_manual_confirmation);
            assert_eq!(reference.blocked_reason, None);
            assert!(reference.replay_safety_proven());
        }
    }

    /// Verifies a successful lifecycle cleanup removes the sidecar instead of retaining a consumed snapshot.
    /// 验证成功生命周期清理会删除 sidecar，而不是保留已消费快照。
    #[test]
    fn retry_snapshot_delete_removes_persisted_sidecar() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let reference = store
            .write_interrupted(&snapshot)
            .expect("snapshot should persist");
        let path = store.retry_path();

        store
            .delete(&reference)
            .expect("successful cleanup should delete the sidecar");

        assert!(!path.exists());
    }

    /// Verifies changed sidecar bytes fail closed instead of being decoded as a retry candidate.
    /// 验证被修改的 sidecar 字节会失败关闭，而不是被解码为重试候选。
    #[test]
    fn retry_snapshot_rejects_digest_mismatch() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let reference = store
            .write_interrupted(&snapshot)
            .expect("snapshot should persist");
        // path is the only deterministic interrupted retry file.
        // path 是唯一确定的中断重试文件。
        let path = store.retry_path();
        let mut bytes = fs::read(&path).expect("snapshot bytes should exist");
        bytes.push(b' ');
        fs::write(&path, bytes).expect("test corruption should be written");

        let error = store
            .read(&reference)
            .expect_err("changed bytes must fail closed");

        assert!(error.contains("byte length mismatch") || error.contains("digest mismatch"));
    }

    /// Verifies a later interruption overwrites the only durable body and invalidates the prior digest.
    /// 验证后续中断会覆盖唯一持久正文，并使旧摘要失效。
    #[test]
    fn retry_snapshot_replacement_overwrites_the_only_durable_body() {
        // tempRoot owns the one durable interruption boundary for this exact session.
        // tempRoot 持有当前精确会话唯一的持久化中断边界。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store always replaces the same deterministic retry file.
        // store 始终替换同一个确定的重试文件。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // snapshot changes lifecycle and generation while preserving the logical snapshot identity.
        // snapshot 在保留逻辑快照身份的同时改变生命周期与代际。
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let prior_reference = store
            .write_interrupted(&snapshot)
            .expect("prior interrupted body should persist");
        snapshot.generation += 1;
        snapshot.state = ExecutionRetrySnapshotState::Claimed;
        snapshot.claimed_attempt_id = Some(String::from("attempt-2"));
        snapshot.updated_at = String::from("2000");
        let replacement_reference = store
            .write_interrupted(&snapshot)
            .expect("replacement interrupted body should persist");

        assert_ne!(
            prior_reference.content_sha256,
            replacement_reference.content_sha256
        );
        assert!(
            store.read(&prior_reference).is_err(),
            "the overwritten interruption must never retain a second body"
        );
        assert_eq!(
            store
                .read(&replacement_reference)
                .expect("replacement reference must be readable")
                .generation,
            4
        );
        store
            .delete(&replacement_reference)
            .expect("consumption should delete the only durable body");
        assert!(store.read(&replacement_reference).is_err());
    }

    /// Verifies stale cleanup cannot delete a newer interruption that replaced the same fixed file.
    /// 验证过期清理不能删除替换同一固定文件的更新中断边界。
    #[test]
    fn stale_retry_snapshot_deletion_preserves_newer_exact_body() {
        // tempRoot owns the one fixed retry path shared by both generations.
        // tempRoot 持有两个代际共享的唯一固定重试路径。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // staleSnapshot is superseded before its delayed cleanup reaches the file lock.
        // staleSnapshot 在延迟清理取得文件锁前已经被替换。
        let stale_snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let stale_reference = store
            .write_interrupted(&stale_snapshot)
            .expect("stale interruption should initially persist");
        // newerSnapshot publishes a different exact digest at the same deterministic path.
        // newerSnapshot 在同一确定路径发布不同的精确摘要。
        let mut newer_snapshot = stale_snapshot;
        newer_snapshot.snapshot_id = uuid::Uuid::new_v4().to_string();
        newer_snapshot.generation += 1;
        newer_snapshot.updated_at = String::from("3000");
        let newer_reference = store
            .write_interrupted(&newer_snapshot)
            .expect("newer interruption should replace the fixed file");

        store
            .delete(&stale_reference)
            .expect("stale cleanup should be an idempotent no-op for a newer valid body");

        assert_eq!(
            store
                .read(&newer_reference)
                .expect("newer exact body must remain readable")
                .snapshot_id,
            newer_snapshot.snapshot_id
        );
    }

    /// Verifies stale cleanup still reports corruption when the replacement file is not a valid snapshot.
    /// 验证替代文件不是有效快照时，过期清理仍会报告损坏。
    #[test]
    fn stale_retry_snapshot_deletion_rejects_corrupted_replacement_body() {
        // tempRoot owns the exact fixed retry path used by the corruption fixture.
        // tempRoot 持有损坏夹具使用的精确固定重试路径。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let reference = store
            .write_interrupted(&snapshot)
            .expect("initial interruption should persist");
        fs::write(store.retry_path(), b"{invalid-json")
            .expect("test should replace the exact retry file with corrupted bytes");

        let error = store
            .delete(&reference)
            .expect_err("corrupted replacement must remain an explicit cleanup error");

        assert!(error.contains("current exact file is invalid"));
        assert!(store.retry_path().is_file());
    }

    /// Verifies a lightweight reference cannot remain readable after its exact sidecar disappears.
    /// 验证精确 sidecar 消失后，轻量引用不能继续被读取。
    #[test]
    fn retry_snapshot_rejects_missing_sidecar_body() {
        // tempRoot owns the isolated segmented-session directory for the missing-body case.
        // tempRoot 持有缺失正文场景使用的隔离分段会话目录。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store publishes and reads the same digest-bound sidecar identity.
        // store 发布并读取同一个摘要绑定 sidecar 身份。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // snapshot is the complete body removed after its reference is published.
        // snapshot 是引用发布后被移除的完整正文。
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // reference remains in main state after the simulated external body loss.
        // reference 在模拟外部正文丢失后仍保留在主状态中。
        let reference = store
            .write_interrupted(&snapshot)
            .expect("snapshot should persist");
        store
            .delete(&reference)
            .expect("test setup should remove the exact sidecar");

        let error = store
            .read(&reference)
            .expect_err("a missing sidecar body must fail closed");

        assert!(error.contains("Failed to read execution retry snapshot"));
    }

    /// Verifies a lightweight safety summary cannot diverge from the sidecar body.
    /// 验证轻量安全摘要不能与 sidecar 正文发生分歧。
    #[test]
    fn retry_snapshot_rejects_reference_safety_drift() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        let mut reference = store
            .write_interrupted(&snapshot)
            .expect("snapshot should persist");
        reference.pending_action_label = String::from("finalize_assistant");

        let error = store
            .read(&reference)
            .expect_err("reference safety drift must fail closed");

        assert!(error.contains("safety summary"));
    }

    /// Verifies an omitted lightweight route cannot bypass the exact sidecar route identity.
    /// 验证轻量引用缺失路由时不能绕过 sidecar 的精确路由身份。
    #[test]
    fn retry_snapshot_rejects_reference_route_omission() {
        // tempRoot isolates the exact digest-bound body used by this route-drift case.
        // tempRoot 隔离当前路由漂移场景使用的精确摘要绑定正文。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store derives the authoritative lightweight route from the persisted body.
        // store 从持久化正文推导权威轻量路由。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // snapshot contains the frozen provider route that Continue must preserve.
        // snapshot 包含 Continue 必须保留的冻结供应商路由。
        let snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // reference simulates a truncated main-state pointer with its route removed.
        // reference 模拟路由字段被移除的截断主状态指针。
        let mut reference = store
            .write_interrupted(&snapshot)
            .expect("snapshot should persist");
        reference.provider_route = None;

        let error = store
            .read(&reference)
            .expect_err("route omission must fail closed");

        assert!(error.contains("safety summary"));
    }

    /// Verifies a frozen Provider request remains replayable after earlier tool effects were canonically retained.
    /// 验证先前工具影响已规范保留后，冻结的 Provider 请求仍可重放。
    #[test]
    fn retry_snapshot_allows_provider_replay_after_retained_tool_side_effect() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // request represents a new Provider boundary after earlier tool results already entered canonical state.
        // request 表示先前工具结果已经进入规范状态后的新 Provider 边界。
        let request =
            serde_json::from_value::<vulcan_ai_contract::AiExecutionRequest>(serde_json::json!({
                "operation": "conversation_respond",
                "model_id": "model-a"
            }))
            .expect("minimal provider request should decode through defaulted fields");
        snapshot.pending_action = ExecutionRetryPendingAction::DispatchProviderRequest {
            request: Box::new(request),
        };
        snapshot.error = Some(AiExecutionError {
            category: AiErrorCategory::Unclassified,
            subcategory: Some(String::from("provider_specific_tool_state")),
            stage: String::from("tool_execution"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("tool outcome is unknown"),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: true,
            side_effect_observed: true,
            terminal: false,
            retryable: false,
            retry_after: None,
        });

        let reference = store
            .write_interrupted(&snapshot)
            .expect("interrupted snapshot should persist");

        assert!(reference.retryable);
        assert!(!reference.requires_manual_confirmation);
        assert_eq!(reference.blocked_reason, None);
    }

    /// Verifies an unknown Provider dispatch remains retryable when no tool side effect was observed.
    /// 验证未观察到工具副作用时，分派结果未知的 Provider 请求仍可重试。
    #[test]
    fn retry_snapshot_allows_unknown_provider_dispatch_without_tool_side_effect() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        // request is the exact logical Provider request that Continue may reissue as a new physical attempt.
        // request 是 Continue 可作为新物理尝试重新发出的精确逻辑 Provider 请求。
        let request =
            serde_json::from_value::<vulcan_ai_contract::AiExecutionRequest>(serde_json::json!({
                "operation": "conversation_respond",
                "model_id": "model-a"
            }))
            .expect("minimal provider request should decode through defaulted fields");
        snapshot.pending_action = ExecutionRetryPendingAction::DispatchProviderRequest {
            request: Box::new(request),
        };
        snapshot.error = Some(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport_interrupted")),
            stage: String::from("provider_request"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("dispatch outcome is unknown"),
            dispatch_outcome: DispatchOutcome::Unknown,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
            retry_after: None,
        });

        let reference = store
            .write_interrupted(&snapshot)
            .expect("unknown dispatch snapshot should persist for audit");

        assert!(reference.retryable);
        assert!(!reference.requires_manual_confirmation);
        assert_eq!(reference.blocked_reason, None);
        assert!(reference.replay_safety_proven());
    }

    /// Verifies retained tool-batch and final-assistant actions are both claimable when exact.
    /// 验证保留工具批次与最终助手动作在边界精确时均可领取。
    #[test]
    fn retry_snapshot_supports_tool_batch_and_final_assistant_actions() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // toolSnapshot represents a Provider-completed batch whose tool body has not started.
        // toolSnapshot 表示 Provider 已完成但工具主体尚未启动的批次。
        let tool_snapshot = test_execute_tool_batch_snapshot(uuid::Uuid::new_v4().to_string());
        let tool_reference = store
            .write_interrupted(&tool_snapshot)
            .expect("exact tool batch should persist");
        assert!(tool_reference.replay_safety_proven());
        assert_eq!(tool_reference.blocked_reason, None);

        // finalSnapshot represents a tool-free assistant tail already proven by the Provider.
        // finalSnapshot 表示已由 Provider 证明的无工具助手尾部。
        let mut final_snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        final_snapshot.controller_snapshot.messages =
            vec![vulcan_core::types::ConversationMessage {
                id: String::from("assistant-final-1"),
                payload: vulcan_core::types::ConversationPayload::Assistant {
                    text: String::from("completed answer"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("provider-message-final-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }];
        final_snapshot.pending_action = ExecutionRetryPendingAction::FinalizeAssistant;
        let final_reference = store
            .write_interrupted(&final_snapshot)
            .expect("exact final assistant boundary should persist");
        assert!(final_reference.replay_safety_proven());
        assert_eq!(final_reference.blocked_reason, None);
    }

    /// Verifies a changed tool binding is retained for audit but exposed as a precise blocker.
    /// 验证变化后的工具绑定会保留用于审计，但投影为精确阻断原因。
    #[test]
    fn retry_snapshot_blocks_changed_tool_binding_with_exact_reason() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_execute_tool_batch_snapshot(uuid::Uuid::new_v4().to_string());
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } = &mut snapshot.pending_action
        else {
            panic!("tool batch fixture must retain tool progress");
        };
        tools[0].binding_fingerprint = Some(String::from("changed"));

        let reference = store
            .write_interrupted(&snapshot)
            .expect("corrupt tool binding should persist for audit");

        assert!(!reference.retryable);
        assert!(reference.requires_manual_confirmation);
        assert_eq!(
            reference.blocked_reason.as_deref(),
            Some("ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH")
        );
        assert!(!reference.replay_safety_proven());
    }

    /// Verifies an unresolved binding is claimable only while its exact tool slot is proven unstarted.
    /// 验证未解析绑定仅在精确工具槽位已证明未启动时才可领取。
    #[test]
    fn retry_snapshot_allows_unprepared_binding_only_for_not_started_tool() {
        // tempRoot owns the isolated immutable sidecars for both state classifications.
        // tempRoot 持有两种状态分类使用的隔离不可变 sidecar。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store applies the production structural blocker calculation.
        // store 应用生产环境的结构性阻断计算。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // notStarted retains the exact assistant call while deferring binding freeze to BatchPrepared.
        // notStarted 保留精确助手调用，并把绑定冻结延迟到 BatchPrepared。
        let mut not_started = test_execute_tool_batch_snapshot(uuid::Uuid::new_v4().to_string());
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } =
            &mut not_started.pending_action
        else {
            panic!("tool batch fixture must retain tool progress");
        };
        tools[0].binding_fingerprint = None;

        let not_started_reference = store
            .write_interrupted(&not_started)
            .expect("unprepared not-started slot should persist");
        assert!(not_started_reference.replay_safety_proven());
        assert_eq!(not_started_reference.blocked_reason, None);

        // running proves a body cannot cross Started without one exact frozen binding.
        // running 证明工具主体不能在缺少精确冻结绑定时跨过 Started。
        let mut running = not_started;
        running.snapshot_id = uuid::Uuid::new_v4().to_string();
        let ExecutionRetryPendingAction::ExecuteToolBatch { tools } = &mut running.pending_action
        else {
            panic!("tool batch fixture must retain tool progress");
        };
        tools[0].state = ExecutionRetryToolState::Running;

        let running_reference = store
            .write_interrupted(&running)
            .expect("invalid running slot should persist for audit");
        assert!(!running_reference.replay_safety_proven());
        assert_eq!(
            running_reference.blocked_reason.as_deref(),
            Some("ERR_CONVERSATION_CONTINUE_RETRY_TOOL_BINDING_MISMATCH")
        );
    }

    /// Verifies unknown schemas and path-like identities are rejected before publication.
    /// 验证未知 Schema 与路径式身份会在发布前被拒绝。
    #[test]
    fn retry_snapshot_rejects_unknown_schema_and_invalid_identity() {
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        snapshot.schema_version = EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION + 1;
        assert!(store.write_interrupted(&snapshot).is_err());

        snapshot.schema_version = EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION;
        snapshot.snapshot_id = String::from("../escape");
        assert!(store.write_interrupted(&snapshot).is_err());
    }

    /// Verifies a malformed durable user follow-up is rejected before any sidecar is published.
    /// 验证格式错误的持久用户补充消息会在发布任何 sidecar 前被拒绝。
    #[test]
    fn retry_snapshot_rejects_invalid_user_followup() {
        // tempRoot owns the isolated directory that must remain free of an invalid sidecar.
        // tempRoot 持有不得写入无效 sidecar 的隔离目录。
        let temp_root = tempfile::tempdir().expect("temporary session directory should exist");
        // store applies the same structural validation to every write and subsequent read.
        // store 对每次写入及后续读取应用相同的结构校验。
        let store = ExecutionRetrySnapshotStore::new(temp_root.path());
        // snapshot carries an empty visible prompt that cannot form a real transcript record.
        // snapshot 携带无法形成真实转录记录的空白可见提示词。
        let mut snapshot = test_snapshot(uuid::Uuid::new_v4().to_string());
        snapshot.user_followup = Some(vulcan_workbench_model::ExecutionRetryUserFollowup {
            visible_prompt: String::from("   "),
            provider_prompt: String::from("provider prompt"),
            client_submission_id: String::from("submission-1"),
        });

        let error = store
            .write_interrupted(&snapshot)
            .expect_err("invalid user follow-up must fail closed");

        assert!(error.contains("invalid user follow-up boundary"));
    }
}
