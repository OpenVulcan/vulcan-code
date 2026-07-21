//! Independent VMM session-state persistence, migration transactions, and recovery.
//! 独立 VMM 会话状态持久化、迁移事务与恢复。

use super::*;
use crate::workspace_store::session_addon_store::{
    read_session_addon_state, write_session_addon_state,
};
use vulcan_context::{
    RuntimeLogEvent, RuntimeVmmLifecycleAction, RuntimeVmmLifecycleEvent,
    SessionConversationPayload,
};
use vulcan_vmm::{
    VMM_SESSION_MEMORY_MAX_BYTES, VMM_SESSION_MEMORY_SCHEMA_VERSION, VmmSessionMemoryState,
    render_vmm_context_item_model, take_legacy_vmm_injection_items,
};
use vulcan_workbench_model::{SessionAddonNamespace, SessionAddonState};

/// Independent VMM state file stored at the session-directory root.
/// 保存在会话目录根部的独立 VMM 状态文件。
pub(super) const SEGMENTED_VMM_STATE_FILE: &str = "vmm.json";

/// Durable marker used only while converting legacy generic injection state.
/// 仅在转换旧版通用注入状态期间使用的持久化标记。
pub(super) const SEGMENTED_VMM_MIGRATION_FILE: &str = "vmm.migration.json";

/// Durable completion marker that prevents cleaned legacy VMM data from being imported again.
/// 防止已清理旧版 VMM 数据再次被导入的持久化完成标记。
pub(super) const SEGMENTED_VMM_MIGRATION_COMPLETED_FILE: &str = "vmm.migration.completed.json";

/// Durable marker coordinating one runtime-state and VMM checkpoint publication.
/// 协调单次运行态与 VMM 检查点发布的持久化标记。
pub(in crate::workspace_store) const SEGMENTED_VMM_CHECKPOINT_FILE: &str = "vmm.checkpoint.json";

/// Returns whether one root-level file belongs exclusively to VMM state, recovery, or diagnostics.
/// 返回一个根级文件是否仅归属于 VMM 状态、恢复或诊断。
///
/// # Parameters
/// 参数
/// `file_name` is the exact UTF-8 file name inspected during a session fork.
/// `file_name` 是会话分叉期间检查的精确 UTF-8 文件名。
///
/// # Returns
/// 返回值
/// `true` for canonical VMM artifacts, quarantine evidence, and abandoned atomic staging files.
/// 对规范 VMM 工件、隔离证据与遗留原子暂存文件返回 `true`。
pub(super) fn is_vmm_session_artifact_file_name(file_name: &str) -> bool {
    // canonical_artifacts enumerate every published VMM document owned by one session directory.
    // canonical_artifacts 枚举单个会话目录拥有的全部已发布 VMM 文档。
    let canonical_artifacts = [
        SEGMENTED_VMM_STATE_FILE,
        SEGMENTED_VMM_MIGRATION_FILE,
        SEGMENTED_VMM_MIGRATION_COMPLETED_FILE,
        SEGMENTED_VMM_CHECKPOINT_FILE,
    ];
    canonical_artifacts.contains(&file_name)
        || (file_name.starts_with("vmm.corrupt.") && file_name.ends_with(".json"))
        || canonical_artifacts
            .iter()
            .any(|artifact| file_name.starts_with(&format!(".{artifact}.vulcan-stage-")))
}

/// Current schema version for the explicit migration transaction marker.
/// 显式迁移事务标记的当前结构版本。
const VMM_MIGRATION_TRANSACTION_SCHEMA_VERSION: u32 = 1;

/// Progress stage of one resumable VMM legacy-state migration.
/// 一次可恢复 VMM 旧状态迁移的进度阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum VmmMigrationStage {
    /// Before any target artifact has been published.
    /// 尚未发布任何目标工件。
    Prepared,
    /// After the target `vmm.json` has been published.
    /// 目标 `vmm.json` 已发布。
    VmmWritten,
    /// After the cleaned `state.json` has been published.
    /// 清理后的 `state.json` 已发布。
    StateWritten,
    /// After the cleaned `addons.json` has been published.
    /// 清理后的 `addons.json` 已发布。
    AddonWritten,
    /// After every target artifact is durable and only marker removal remains.
    /// 全部目标工件均已持久化，仅剩删除标记。
    Completed,
}

/// Complete before-and-after state required to finish an interrupted migration deterministically.
/// 确定性完成中断迁移所需的完整前后状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VmmMigrationTransaction {
    /// Migration marker schema version.
    /// 迁移标记结构版本。
    schema_version: u32,
    /// Exact session owning every artifact in this transaction.
    /// 拥有该事务全部工件的精确会话。
    session_id: String,
    /// Latest durable migration progress stage.
    /// 最近一次持久化迁移进度阶段。
    stage: VmmMigrationStage,
    /// VMM state observed before migration, when one already existed.
    /// 迁移前观察到的 VMM 状态（若已存在）。
    previous_vmm_state: Option<VmmSessionMemoryState>,
    /// Exact VMM state that must exist after migration.
    /// 迁移后必须存在的精确 VMM 状态。
    target_vmm_state: VmmSessionMemoryState,
    /// Segmented state document observed before migration.
    /// 迁移前观察到的分段状态文档。
    previous_state_document: Option<SegmentedSessionStateDocument>,
    /// Cleaned segmented state document that must exist after migration.
    /// 迁移后必须存在的清理后分段状态文档。
    target_state_document: SegmentedSessionStateDocument,
    /// Addon document observed before migration.
    /// 迁移前观察到的附属状态文档。
    previous_addon_state: Option<SessionAddonState>,
    /// Cleaned addon document that must exist after migration when applicable.
    /// 适用时迁移后必须存在的清理后附属状态文档。
    target_addon_state: Option<SessionAddonState>,
    /// Unix-millis text captured when the transaction was prepared.
    /// 准备事务时捕获的 Unix 毫秒文本。
    created_at: String,
}

/// Durable proof that one session has completed legacy VMM cleanup.
/// 单个会话已完成旧版 VMM 清理的持久化证明。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VmmMigrationCompletion {
    /// Completion-marker schema version.
    /// 完成标记结构版本。
    schema_version: u32,
    /// Exact session whose legacy data was cleaned.
    /// 已清理旧数据的精确会话。
    session_id: String,
    /// VMM revision published by the completed migration transaction.
    /// 已完成迁移事务发布的 VMM 版本。
    target_vmm_revision: u64,
    /// Unix-millis text captured when cleanup completed.
    /// 清理完成时捕获的 Unix 毫秒文本。
    completed_at: String,
}

/// Result of loading `vmm.json`, including whether corrupt evidence was isolated.
/// 加载 `vmm.json` 的结果，包括是否隔离了损坏证据。
#[derive(Debug)]
struct LoadedVmmState {
    /// Valid state when the file existed or recovery created a degraded replacement.
    /// 文件存在或恢复创建降级替代状态时的有效状态。
    state: Option<VmmSessionMemoryState>,
    /// Whether legacy state must be discarded instead of imported.
    /// 是否必须丢弃而不是导入旧版状态。
    degraded: bool,
}

/// Returns the independent VMM state path for one exact session directory.
/// 返回一个精确会话目录对应的独立 VMM 状态路径。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved segmented session directory.
/// `session_dir` 是已解析的分段会话目录。
///
/// # Returns
/// 返回值
/// The sibling `vmm.json` path beneath that session directory.
/// 返回该会话目录下的同级 `vmm.json` 路径。
pub(super) fn vmm_state_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_VMM_STATE_FILE)
}

/// Returns whether one session directory contains independent VMM or migration artifacts.
/// 返回一个会话目录是否包含独立 VMM 或迁移工件。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory to inspect.
/// `session_dir` 是需要检查的精确分段会话目录。
///
/// # Returns
/// 返回值
/// `true` when `vmm.json` or its migration marker exists, otherwise an inspection error.
/// 当 `vmm.json` 或其迁移标记存在时返回 `true`，否则返回检查错误。
pub(in crate::workspace_store) fn has_vmm_session_artifacts(
    session_dir: &Path,
) -> Result<bool, String> {
    let vmm_exists = vmm_state_path(session_dir)
        .try_exists()
        .map_err(to_store_error)?;
    let migration_exists = vmm_migration_path(session_dir)
        .try_exists()
        .map_err(to_store_error)?;
    let migration_completed = vmm_migration_completion_path(session_dir)
        .try_exists()
        .map_err(to_store_error)?;
    let checkpoint_exists = session_dir
        .join(SEGMENTED_VMM_CHECKPOINT_FILE)
        .try_exists()
        .map_err(to_store_error)?;
    Ok(vmm_exists || migration_exists || migration_completed || checkpoint_exists)
}

/// Returns the resumable migration-marker path for one exact session directory.
/// 返回一个精确会话目录对应的可恢复迁移标记路径。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved segmented session directory.
/// `session_dir` 是已解析的分段会话目录。
///
/// # Returns
/// 返回值
/// The sibling `vmm.migration.json` path beneath that session directory.
/// 返回该会话目录下的同级 `vmm.migration.json` 路径。
fn vmm_migration_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_VMM_MIGRATION_FILE)
}

/// Returns the durable completed-migration marker path for one exact session directory.
/// 返回一个精确会话目录对应的持久化迁移完成标记路径。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved segmented session directory.
/// `session_dir` 是已解析的分段会话目录。
///
/// # Returns
/// 返回值
/// The sibling `vmm.migration.completed.json` path beneath that session directory.
/// 返回该会话目录下的同级 `vmm.migration.completed.json` 路径。
fn vmm_migration_completion_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_VMM_MIGRATION_COMPLETED_FILE)
}

/// Reads and validates independent VMM state, isolating corrupt evidence without legacy fallback.
/// 读取并验证独立 VMM 状态，在不回退旧状态的情况下隔离损坏证据。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `expected_session_id` is the exact directory owner expected by the loader.
/// `expected_session_id` 是加载器按目录期望的精确所有者。
/// `updated_at` is used only for the newly initialized degraded replacement state.
/// `updated_at` 仅用于新初始化的降级替代状态。
///
/// # Returns
/// 返回值
/// Valid state, clean absence, or a degraded replacement after evidence isolation.
/// 返回有效状态、干净缺失，或隔离证据后的降级替代状态。
fn load_vmm_state_with_recovery(
    session_dir: &Path,
    expected_session_id: &str,
    updated_at: &str,
) -> Result<LoadedVmmState, String> {
    let path = vmm_state_path(session_dir);
    let read_result = {
        let _guard = segmented_state_file_io_lock()
            .lock()
            .map_err(to_store_error)?;
        // Reject oversized documents before JSON allocation and parse.
        // 在 JSON 分配与解析前拒绝超限文档。
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > VMM_SESSION_MEMORY_MAX_BYTES as u64 => Err(format!(
                "VMM state `{}` exceeds the {} byte limit.",
                path.display(),
                VMM_SESSION_MEMORY_MAX_BYTES
            )),
            Ok(_) => read_optional_json::<VmmSessionMemoryState>(&path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(to_store_error(error)),
        }
    };
    match read_result {
        Ok(None) => Ok(LoadedVmmState {
            state: None,
            degraded: false,
        }),
        Ok(Some(state)) if state.schema_version != VMM_SESSION_MEMORY_SCHEMA_VERSION => {
            Err(format!(
                "Unsupported VMM session memory schema version {}.",
                state.schema_version
            ))
        }
        Ok(Some(state)) if state.session_id != expected_session_id => Err(format!(
            "VMM session memory owner mismatch: expected `{expected_session_id}`, found `{}`.",
            state.session_id
        )),
        Ok(Some(state)) => match state.validate(expected_session_id) {
            Ok(()) => Ok(LoadedVmmState {
                state: Some(state),
                degraded: false,
            }),
            Err(error) => {
                quarantine_corrupt_vmm_file(&path)?;
                let mut recovered = VmmSessionMemoryState::new(expected_session_id, updated_at)?;
                recovered.recovery_notice = Some(format!(
                    "The previous VMM state failed validation and was isolated: {error}"
                ));
                Ok(LoadedVmmState {
                    state: Some(recovered),
                    degraded: true,
                })
            }
        },
        Err(error) => {
            quarantine_corrupt_vmm_file(&path)?;
            let mut recovered = VmmSessionMemoryState::new(expected_session_id, updated_at)?;
            recovered.recovery_notice = Some(format!(
                "The previous VMM state could not be parsed and was isolated: {error}"
            ));
            Ok(LoadedVmmState {
                state: Some(recovered),
                degraded: true,
            })
        }
    }
}

/// Writes one independent VMM snapshot atomically while rejecting stale or divergent revisions.
/// 原子写入一份独立 VMM 快照，同时拒绝陈旧或同版本分歧写入。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `expected_session_id` is the exact persisted owner.
/// `expected_session_id` 是精确的持久化所有者。
/// `state` is the complete overwrite snapshot.
/// `state` 是完整覆盖式快照。
///
/// # Returns
/// 返回值
/// Success after an atomic publish, an idempotent equal write, or confirmed clean absence.
/// 原子发布、幂等相同写入或确认文件干净缺失后返回成功。
pub(in crate::workspace_store) fn write_vmm_state(
    session_dir: &Path,
    expected_session_id: &str,
    state: &VmmSessionMemoryState,
) -> Result<(), String> {
    let path = vmm_state_path(session_dir);
    // encoded is the exact pretty-printed byte representation that will become vmm.json.
    // encoded 是最终成为 vmm.json 的精确美化打印字节表示。
    let encoded = encode_vmm_state_document(
        state,
        expected_session_id,
        VMM_SESSION_MEMORY_MAX_BYTES as u64,
    )?;
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    if !state.requires_persistence() {
        if path.try_exists().map_err(to_store_error)? {
            return Err(format!(
                "Rejected pristine VMM snapshot because durable state already exists for session `{expected_session_id}`."
            ));
        }
        return Ok(());
    }
    state.validate(expected_session_id)?;
    if let Some(existing) = read_optional_json::<VmmSessionMemoryState>(&path)? {
        existing.validate(expected_session_id)?;
        if existing.revision > state.revision {
            return Err(format!(
                "Rejected stale VMM state revision {} because durable revision {} is newer for session `{expected_session_id}`.",
                state.revision, existing.revision
            ));
        }
        if existing.revision == state.revision {
            if existing == *state {
                return Ok(());
            }
            return Err(format!(
                "Rejected divergent VMM state at revision {} for session `{expected_session_id}`.",
                state.revision
            ));
        }
    }
    write_file_bytes_atomic(&path, &encoded, "VMM state document")
}

/// Encodes one VMM document exactly as persisted and enforces the physical file-size limit.
/// 按实际持久化格式精确编码 VMM 文档，并执行物理文件大小限制。
///
/// # Parameters
/// 参数
/// `state` is the complete VMM snapshot to encode.
/// `state` 是需要编码的完整 VMM 快照。
/// `expected_session_id` labels size-limit diagnostics with the exact owner.
/// `expected_session_id` 使用精确所有者标记大小限制诊断。
/// `max_bytes` is the maximum accepted physical document length including its trailing newline.
/// `max_bytes` 是包含末尾换行符在内允许的最大物理文档长度。
///
/// # Returns
/// 返回值
/// The complete publishable bytes, or an explicit serialization or size-limit error.
/// 返回可完整发布的字节，或明确的序列化与大小限制错误。
fn encode_vmm_state_document(
    state: &VmmSessionMemoryState,
    expected_session_id: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    // encoded uses the same pretty JSON and trailing newline contract as every durable JSON document.
    // encoded 使用与全部持久 JSON 文档相同的美化 JSON 与末尾换行符契约。
    let mut encoded = serde_json::to_vec_pretty(state).map_err(to_store_error)?;
    encoded.push(b'\n');
    // serialized_len measures the actual bytes written rather than a smaller compact surrogate.
    // serialized_len 测量实际写入字节，而不是更小的紧凑替代表示。
    let serialized_len = encoded.len() as u64;
    if serialized_len > max_bytes {
        return Err(format!(
            "VMM state for session `{expected_session_id}` is {serialized_len} bytes and exceeds the {max_bytes} byte limit."
        ));
    }
    Ok(encoded)
}

/// Returns whether durable VMM state makes one checkpoint target permanently unrecoverable.
/// 返回持久化 VMM 状态是否使某个检查点目标永久无法恢复。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `expected_session_id` is the exact persisted owner.
/// `expected_session_id` 是精确的持久化所有者。
/// `target` is the complete checkpoint VMM target inspected without mutation.
/// `target` 是只读检查的完整 VMM 检查点目标。
///
/// # Returns
/// 返回值
/// `true` when durable state supersedes a pristine target, has a newer revision, or diverges at the same revision.
/// 当持久状态取代纯净目标、版本更新或同版本分歧时返回 `true`。
pub(in crate::workspace_store) fn durable_vmm_state_conflicts_with_checkpoint(
    session_dir: &Path,
    expected_session_id: &str,
    target: &VmmSessionMemoryState,
) -> Result<bool, String> {
    if target.is_pristine() {
        // Any validated durable state supersedes an unowned snapshot that has never persisted VMM data.
        // 任意已验证持久状态都会取代一份从未持久化 VMM 数据的无归属快照。
        let _guard = segmented_state_file_io_lock()
            .lock()
            .map_err(to_store_error)?;
        // durable_state is read under the same overwrite-only file gate used by VMM publication.
        // durable_state 在 VMM 发布所用的同一个覆盖式文件闸门下读取。
        let durable_state =
            read_optional_json::<VmmSessionMemoryState>(&vmm_state_path(session_dir))?;
        if let Some(durable_state) = durable_state.as_ref() {
            durable_state.validate(expected_session_id)?;
        }
        return Ok(durable_state.is_some());
    }
    Ok(durable_vmm_state_superseding_target(session_dir, expected_session_id, target)?.is_some())
}

/// Reads a validated durable VMM snapshot only when it supersedes one transaction target.
/// 仅在已验证持久 VMM 快照取代某个事务目标时读取并返回该快照。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `expected_session_id` is the exact persisted owner.
/// `expected_session_id` 是精确的持久化所有者。
/// `target` is the complete transaction target inspected without mutation.
/// `target` 是只读检查的完整事务目标。
///
/// # Returns
/// 返回值
/// A newer or divergent equal durable snapshot, otherwise clean absence.
/// 返回更新或同版本分歧的持久快照，否则返回干净缺失。
fn durable_vmm_state_superseding_target(
    session_dir: &Path,
    expected_session_id: &str,
    target: &VmmSessionMemoryState,
) -> Result<Option<VmmSessionMemoryState>, String> {
    target.validate(expected_session_id)?;
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let Some(durable) = read_optional_json::<VmmSessionMemoryState>(&vmm_state_path(session_dir))?
    else {
        return Ok(None);
    };
    durable.validate(expected_session_id)?;
    Ok((durable.revision > target.revision
        || (durable.revision == target.revision && durable != *target))
        .then_some(durable))
}

/// Removes independent VMM state only for an explicit rollback, edit, or history rewrite.
/// 仅在显式回滚、编辑或历史改写时移除独立 VMM 状态。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact rewritten session directory.
/// `session_dir` 是被改写的精确会话目录。
///
/// # Returns
/// 返回值
/// Success after the state and any unfinished migration transaction are absent.
/// 状态文件与任何未完成迁移事务均缺失后返回成功。
pub(in crate::workspace_store) fn reset_vmm_state_for_history_rewrite(
    session_dir: &Path,
) -> Result<(), String> {
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    remove_exact_file_if_present(&vmm_state_path(session_dir))?;
    remove_exact_file_if_present(&vmm_migration_path(session_dir))?;
    remove_exact_file_if_present(&session_dir.join(SEGMENTED_VMM_CHECKPOINT_FILE))
}

/// Hydrates independent VMM state and migrates exact legacy renderer-owned items transactionally.
/// 水合独立 VMM 状态，并以事务方式迁移精确归属于旧渲染器的条目。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory.
/// `session_dir` 是精确的分段会话目录。
/// `session_id` is the exact host session owner.
/// `session_id` 是精确的宿主会话所有者。
/// `runtime_state` is the already-loaded state after addon application.
/// `runtime_state` 是应用附属状态后已经加载的运行态。
///
/// # Returns
/// 返回值
/// `true` when migration, recovery, or VMM hydration produced durable session state.
/// 迁移、恢复或 VMM 水合产出持久化会话状态时返回 `true`。
pub(in crate::workspace_store) fn hydrate_vmm_session_state(
    session_dir: &Path,
    session_id: &str,
    runtime_state: &mut SessionRuntimeStateEntry,
) -> Result<bool, String> {
    let recovered_migration = recover_vmm_migration_transaction(session_dir, session_id)?;
    if !vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled {
        // Legacy VMM generic injections are removed only from memory while the new state path is rolled back.
        // 新状态路径回滚期间，旧版 VMM 通用注入仅从内存中移除。
        take_legacy_vmm_injection_items(&mut runtime_state.context_injection_state);
        runtime_state.vmm_session_memory = VmmSessionMemoryState::default();
        runtime_state.conversation_state.continuation = Default::default();
        if recovered_migration {
            let updated_at = if runtime_state.updated_at.trim().is_empty() {
                current_unix_millis_text()
            } else {
                runtime_state.updated_at.clone()
            };
            record_vmm_store_lifecycle_event_best_effort(
                session_dir,
                &updated_at,
                RuntimeVmmLifecycleAction::FileRecovered,
                None,
                "interrupted_vmm_migration_replayed_while_feature_disabled",
            );
        }
        return Ok(false);
    }
    // A valid completion marker is authoritative proof that old generic VMM data must not revive.
    // 有效完成标记是禁止旧通用 VMM 数据复活的权威证明。
    let migration_completed = read_vmm_migration_completion(session_dir, session_id)?.is_some();
    let updated_at = if runtime_state.updated_at.trim().is_empty() {
        current_unix_millis_text()
    } else {
        runtime_state.updated_at.clone()
    };
    let loaded = load_vmm_state_with_recovery(session_dir, session_id, &updated_at)?;
    let previous_vmm_state = loaded.state.clone();
    // legacy_extraction distinguishes physical cleanup from the subset eligible for lease import.
    // legacy_extraction 区分物理清理与可导入租约的有效子集。
    let legacy_extraction =
        take_legacy_vmm_injection_items(&mut runtime_state.context_injection_state);
    // had_legacy_items remains true for invalid owned records so cleanup becomes durable.
    // had_legacy_items 对无效归属记录仍为真，使清理能够持久化。
    let had_legacy_items = legacy_extraction.removed_owned_item_count > 0;
    // legacy_items contains only structurally complete, non-expired candidates.
    // legacy_items 仅包含结构完整且未到期的候选项。
    let legacy_items = legacy_extraction.lease_items;
    let mut vmm_state = loaded.state.unwrap_or_else(VmmSessionMemoryState::default);
    // imported_legacy records only the deterministic first migration from legacy generic state.
    // imported_legacy 仅记录从旧通用状态执行的确定性首次迁移。
    let mut imported_legacy = false;

    if had_legacy_items && !migration_completed && !loaded.degraded && previous_vmm_state.is_none()
    {
        let target_message_id = latest_canonical_user_message_id(runtime_state);
        vmm_state.import_legacy_items(
            session_id,
            target_message_id.as_deref(),
            legacy_items,
            &updated_at,
        )?;
        imported_legacy = true;
    }

    if had_legacy_items || loaded.degraded {
        runtime_state.conversation_state.continuation = Default::default();
        if vmm_state.is_pristine() {
            vmm_state = VmmSessionMemoryState::new(session_id, &updated_at)?;
        }
        let transaction = prepare_vmm_migration_transaction(
            session_dir,
            session_id,
            previous_vmm_state,
            vmm_state.clone(),
            runtime_state,
            &updated_at,
        )?;
        execute_vmm_migration_transaction(session_dir, transaction)?;
    } else if vmm_state.requires_persistence() {
        write_vmm_state(session_dir, session_id, &vmm_state)?;
    }

    runtime_state.vmm_session_memory = vmm_state;
    let recovered_anchor_targets = recover_missing_vmm_anchor_targets(runtime_state, &updated_at)?;
    if recovered_anchor_targets {
        // The checkpoint marker couples the rebuilt VMM epoch with continuation invalidation so a crash cannot expose only one side.
        // 检查点标记将重建后的 VMM 纪元与 continuation 失效绑定，确保崩溃不会只暴露其中一侧。
        crate::workspace_store::persistence::persist_segmented_runtime_checkpoint_transactionally(
            session_dir,
            session_id,
            runtime_state,
        )?;
    }
    if imported_legacy {
        record_vmm_store_lifecycle_event_best_effort(
            session_dir,
            &updated_at,
            RuntimeVmmLifecycleAction::LegacyMigrated,
            Some(&runtime_state.vmm_session_memory),
            "deterministic_legacy_vmm_state_migration",
        );
    } else if had_legacy_items {
        record_vmm_store_lifecycle_event_best_effort(
            session_dir,
            &updated_at,
            RuntimeVmmLifecycleAction::LegacyMigrated,
            Some(&runtime_state.vmm_session_memory),
            "deterministic_invalid_legacy_vmm_cleanup",
        );
    }
    if recovered_migration {
        record_vmm_store_lifecycle_event_best_effort(
            session_dir,
            &updated_at,
            RuntimeVmmLifecycleAction::FileRecovered,
            Some(&runtime_state.vmm_session_memory),
            "interrupted_vmm_migration_replayed_forward",
        );
    }
    if loaded.degraded {
        record_vmm_store_lifecycle_event_best_effort(
            session_dir,
            &updated_at,
            RuntimeVmmLifecycleAction::FileQuarantined,
            Some(&runtime_state.vmm_session_memory),
            "invalid_vmm_state_quarantined",
        );
    }
    if recovered_anchor_targets {
        record_vmm_store_lifecycle_event_best_effort(
            session_dir,
            &updated_at,
            RuntimeVmmLifecycleAction::FileRecovered,
            Some(&runtime_state.vmm_session_memory),
            "missing_anchor_targets_rebuilt_cache_epoch",
        );
    }
    Ok(runtime_state.vmm_session_memory.requires_persistence() || had_legacy_items)
}

/// Removes every VMM session artifact and exact legacy VMM injection from a fork target.
/// 从分叉目标中移除全部 VMM 会话工件与精确旧版 VMM 注入。
///
/// # Parameters
/// 参数
/// `session_dir` is the unpublished fork target directory.
/// `session_dir` 是尚未发布的分叉目标目录。
/// `session_id` is the exact target-session owner.
/// `session_id` 是精确的目标会话所有者。
/// `updated_at` is the fork timestamp used for cleaned addon revisions.
/// `updated_at` 是用于清理后附属状态版本的分叉时间戳。
///
/// # Returns
/// 返回值
/// Success after VMM files and renderer-owned legacy items are physically absent.
/// VMM 文件与渲染器归属的旧版条目均物理消失后返回成功。
pub(in crate::workspace_store) fn purge_vmm_state_from_fork_target(
    session_dir: &Path,
    session_id: &str,
    updated_at: &str,
) -> Result<(), String> {
    remove_exact_file_if_present(&vmm_state_path(session_dir))?;
    remove_exact_file_if_present(&vmm_migration_path(session_dir))?;
    remove_exact_file_if_present(&vmm_migration_completion_path(session_dir))?;
    remove_exact_file_if_present(&session_dir.join(SEGMENTED_VMM_CHECKPOINT_FILE))?;
    if let Some(mut state_document) = read_state(session_dir)?
        && !state_document.runtime_state.is_null()
    {
        let mut runtime_state = serde_json::from_value::<SessionRuntimeStateEntry>(
            state_document.runtime_state.clone(),
        )
        .map_err(|error| format!("Failed to parse fork target runtime state: {error}"))?;
        take_legacy_vmm_injection_items(&mut runtime_state.context_injection_state);
        runtime_state.vmm_session_memory = VmmSessionMemoryState::default();
        state_document.runtime_state =
            serde_json::to_value(runtime_state).map_err(to_store_error)?;
        state_document.session_id = session_id.to_string();
        state_document.updated_at = updated_at.to_string();
        write_state(session_dir, &state_document)?;
    }
    if let Some(addon_state) = read_session_addon_state(session_dir)?
        && let Some(cleaned_addon_state) = cleaned_vmm_addon_state(Some(addon_state), updated_at)?
    {
        write_session_addon_state(session_dir, &cleaned_addon_state)?;
    }
    Ok(())
}

/// Prepares a complete before-and-after migration transaction without publishing target artifacts.
/// 在不发布目标工件的情况下准备完整前后迁移事务。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `session_id` is the exact persisted owner.
/// `session_id` 是精确的持久化所有者。
/// `previous_vmm_state` is the valid pre-migration VMM snapshot when present.
/// `previous_vmm_state` 是存在时有效的迁移前 VMM 快照。
/// `target_vmm_state` is the complete post-migration VMM snapshot.
/// `target_vmm_state` 是完整的迁移后 VMM 快照。
/// `runtime_state` is the already-cleaned runtime candidate.
/// `runtime_state` 是已经清理的运行态候选。
/// `updated_at` is the transaction timestamp.
/// `updated_at` 是事务时间戳。
///
/// # Returns
/// 返回值
/// A complete forward-replay transaction containing exact before and after documents.
/// 返回包含精确前后文档、可向前重放的完整事务。
fn prepare_vmm_migration_transaction(
    session_dir: &Path,
    session_id: &str,
    previous_vmm_state: Option<VmmSessionMemoryState>,
    target_vmm_state: VmmSessionMemoryState,
    runtime_state: &SessionRuntimeStateEntry,
    updated_at: &str,
) -> Result<VmmMigrationTransaction, String> {
    let previous_state_document = read_state(session_dir)?;
    let target_state_document = state_document_from_runtime_state(session_id, runtime_state)?;
    let previous_addon_state = read_session_addon_state(session_dir)?;
    let target_addon_state = cleaned_vmm_addon_state(previous_addon_state.clone(), updated_at)?;
    Ok(VmmMigrationTransaction {
        schema_version: VMM_MIGRATION_TRANSACTION_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        stage: VmmMigrationStage::Prepared,
        previous_vmm_state,
        target_vmm_state,
        previous_state_document,
        target_state_document,
        previous_addon_state,
        target_addon_state,
        created_at: updated_at.to_string(),
    })
}

/// Executes or resumes one migration by replaying exact target documents in fixed order.
/// 按固定顺序重放精确目标文档，以执行或恢复一次迁移。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `transaction` contains the complete target state and latest progress stage.
/// `transaction` 包含完整目标状态与最近进度阶段。
///
/// # Returns
/// 返回值
/// Success only after every target document is durable and the marker is removed.
/// 仅在全部目标文档持久化且标记被删除后返回成功。
fn execute_vmm_migration_transaction(
    session_dir: &Path,
    mut transaction: VmmMigrationTransaction,
) -> Result<(), String> {
    if let Some(durable_state) = durable_vmm_state_superseding_target(
        session_dir,
        &transaction.session_id,
        &transaction.target_vmm_state,
    )? {
        // Newer durable VMM state wins while legacy state and addon cleanup still complete forward.
        // 更新的持久 VMM 状态胜出，同时旧状态与 addon 清理仍继续向前完成。
        transaction.target_vmm_state = durable_state;
    }
    validate_vmm_migration_transaction(&transaction)?;
    write_vmm_migration_transaction(session_dir, &transaction)?;
    write_vmm_state(
        session_dir,
        &transaction.session_id,
        &transaction.target_vmm_state,
    )?;
    transaction.stage = VmmMigrationStage::VmmWritten;
    write_vmm_migration_transaction(session_dir, &transaction)?;
    write_state(session_dir, &transaction.target_state_document)?;
    transaction.stage = VmmMigrationStage::StateWritten;
    write_vmm_migration_transaction(session_dir, &transaction)?;
    if let Some(addon_state) = transaction.target_addon_state.as_ref() {
        write_session_addon_state(session_dir, addon_state)?;
    }
    transaction.stage = VmmMigrationStage::AddonWritten;
    write_vmm_migration_transaction(session_dir, &transaction)?;
    write_vmm_migration_completion(
        session_dir,
        &VmmMigrationCompletion {
            schema_version: VMM_MIGRATION_TRANSACTION_SCHEMA_VERSION,
            session_id: transaction.session_id.clone(),
            target_vmm_revision: transaction.target_vmm_state.revision,
            completed_at: transaction.created_at.clone(),
        },
    )?;
    transaction.stage = VmmMigrationStage::Completed;
    write_vmm_migration_transaction(session_dir, &transaction)?;
    remove_exact_file_if_present(&vmm_migration_path(session_dir))
}

/// Completes an interrupted migration marker before any legacy state can be applied again.
/// 在任何旧状态可能再次应用前完成一份中断的迁移标记。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `expected_session_id` is the exact loader owner.
/// `expected_session_id` 是加载器期望的精确所有者。
///
/// # Returns
/// 返回值
/// Success after clean absence or deterministic forward completion.
/// 标记干净缺失或确定性向前完成后返回成功。
fn recover_vmm_migration_transaction(
    session_dir: &Path,
    expected_session_id: &str,
) -> Result<bool, String> {
    let path = vmm_migration_path(session_dir);
    let Some(transaction) = read_optional_json::<VmmMigrationTransaction>(&path)? else {
        return Ok(false);
    };
    if transaction.session_id != expected_session_id {
        return Err(format!(
            "VMM migration owner mismatch: expected `{expected_session_id}`, found `{}`.",
            transaction.session_id
        ));
    }
    execute_vmm_migration_transaction(session_dir, transaction)?;
    Ok(true)
}

/// Validates a migration marker before it is replayed against session files.
/// 在迁移标记重放到会话文件前对其进行验证。
///
/// # Parameters
/// 参数
/// `transaction` is the complete marker candidate.
/// `transaction` 是完整标记候选。
///
/// # Returns
/// 返回值
/// Success only for supported schema and consistent target ownership.
/// 仅在结构版本受支持且目标归属一致时返回成功。
fn validate_vmm_migration_transaction(transaction: &VmmMigrationTransaction) -> Result<(), String> {
    if transaction.schema_version != VMM_MIGRATION_TRANSACTION_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported VMM migration transaction schema version {}.",
            transaction.schema_version
        ));
    }
    transaction
        .target_vmm_state
        .validate(&transaction.session_id)?;
    if transaction.target_state_document.session_id != transaction.session_id {
        return Err(String::from(
            "VMM migration target state document belongs to a different session.",
        ));
    }
    // target_runtime_state proves a persisted migration marker cannot reintroduce renderer-owned legacy items.
    // target_runtime_state 证明持久化迁移标记无法重新引入渲染器归属的旧版条目。
    let mut target_runtime_state = serde_json::from_value::<SessionRuntimeStateEntry>(
        transaction.target_state_document.runtime_state.clone(),
    )
    .map_err(|error| format!("Invalid VMM migration target runtime state: {error}"))?;
    // legacy_runtime_items must be empty because forward replay publishes this document verbatim.
    // legacy_runtime_items 必须为空，因为向前重放会原样发布该文档。
    let legacy_runtime_items =
        take_legacy_vmm_injection_items(&mut target_runtime_state.context_injection_state);
    if legacy_runtime_items.removed_owned_item_count > 0 {
        return Err(String::from(
            "VMM migration target state document still contains legacy VMM injection items.",
        ));
    }
    if let Some(addon_state) = transaction.target_addon_state.as_ref()
        && addon_state.session_id.to_string() != transaction.session_id
    {
        return Err(String::from(
            "VMM migration target addon document belongs to a different session.",
        ));
    }
    if let Some(addon_state) = transaction.target_addon_state.as_ref() {
        // recleaned_addon_state must remain byte-semantically equal when the migration target is clean.
        // recleaned_addon_state 在迁移目标已清理时必须保持字节语义相等。
        let recleaned_addon_state =
            cleaned_vmm_addon_state(Some(addon_state.clone()), &transaction.created_at)?;
        if recleaned_addon_state.as_ref() != Some(addon_state) {
            return Err(String::from(
                "VMM migration target addon document still contains legacy VMM injection items.",
            ));
        }
    }
    Ok(())
}

/// Writes one complete migration marker atomically.
/// 原子写入一份完整迁移标记。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `transaction` is the complete marker snapshot.
/// `transaction` 是完整标记快照。
///
/// # Returns
/// 返回值
/// Success after the marker is atomically published.
/// 标记原子发布后返回成功。
fn write_vmm_migration_transaction(
    session_dir: &Path,
    transaction: &VmmMigrationTransaction,
) -> Result<(), String> {
    write_json_file_atomic(&vmm_migration_path(session_dir), transaction)
}

/// Reads and validates the durable completed-migration marker for one session.
/// 读取并验证单个会话的持久化迁移完成标记。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `expected_session_id` is the exact loader owner.
/// `expected_session_id` 是加载器期望的精确所有者。
///
/// # Returns
/// 返回值
/// A valid completion marker or clean absence.
/// 返回有效完成标记或干净缺失。
fn read_vmm_migration_completion(
    session_dir: &Path,
    expected_session_id: &str,
) -> Result<Option<VmmMigrationCompletion>, String> {
    let Some(completion) =
        read_optional_json::<VmmMigrationCompletion>(&vmm_migration_completion_path(session_dir))?
    else {
        return Ok(None);
    };
    if completion.schema_version != VMM_MIGRATION_TRANSACTION_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported VMM migration completion schema version {}.",
            completion.schema_version
        ));
    }
    if completion.session_id != expected_session_id {
        return Err(format!(
            "VMM migration completion owner mismatch: expected `{expected_session_id}`, found `{}`.",
            completion.session_id
        ));
    }
    Ok(Some(completion))
}

/// Atomically writes the durable proof that legacy VMM cleanup completed.
/// 原子写入旧版 VMM 清理已完成的持久化证明。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `completion` is the complete validated marker payload.
/// `completion` 是完整且已验证的标记载荷。
///
/// # Returns
/// 返回值
/// Success after the completion marker is atomically published.
/// 完成标记原子发布后返回成功。
fn write_vmm_migration_completion(
    session_dir: &Path,
    completion: &VmmMigrationCompletion,
) -> Result<(), String> {
    write_json_file_atomic(&vmm_migration_completion_path(session_dir), completion)
}

/// Removes exact VMM renderer-owned items from one addon context namespace.
/// 从一个附属状态上下文命名空间中移除精确归属于 VMM 渲染器的条目。
///
/// # Parameters
/// 参数
/// `addon_state` is the optional original addon document.
/// `addon_state` 是可选的原始附属状态文档。
/// `updated_at` is used when replacing the cleaned namespace.
/// `updated_at` 用于替换清理后命名空间。
///
/// # Returns
/// 返回值
/// The exact cleaned addon candidate while preserving every non-VMM namespace.
/// 返回保留全部非 VMM 命名空间的精确清理后附属状态候选。
fn cleaned_vmm_addon_state(
    addon_state: Option<SessionAddonState>,
    updated_at: &str,
) -> Result<Option<SessionAddonState>, String> {
    let Some(mut addon_state) = addon_state else {
        return Ok(None);
    };
    let Some(mut context_state) = addon_state
        .typed_namespace::<vulcan_context_injection::ContextInjectionState>(
            SessionAddonNamespace::ContextInjection,
        )
    else {
        return Ok(Some(addon_state));
    };
    // extraction count includes invalid exact-owned records that must also be removed durably.
    // 提取计数包含同样必须被持久清理的无效精确归属记录。
    let extraction = take_legacy_vmm_injection_items(&mut context_state);
    if extraction.removed_owned_item_count == 0 {
        return Ok(Some(addon_state));
    }
    if context_state.is_empty() {
        addon_state.remove_namespace(SessionAddonNamespace::ContextInjection);
    } else {
        addon_state.set_typed_namespace(
            SessionAddonNamespace::ContextInjection,
            &context_state,
            updated_at.to_string(),
        );
    }
    Ok(Some(addon_state))
}

/// Returns the latest exact canonical user-message identifier without positional substitution.
/// 在不进行位置替代的情况下返回最近一条精确规范用户消息标识。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the canonical conversation window inspected for migration.
/// `runtime_state` 拥有迁移时检查的规范会话窗口。
///
/// # Returns
/// 返回值
/// The latest user-text message id, or `None` when no exact target exists.
/// 返回最近一条用户文本消息标识；不存在精确目标时返回 `None`。
fn latest_canonical_user_message_id(runtime_state: &SessionRuntimeStateEntry) -> Option<String> {
    runtime_state
        .conversation_state
        .messages
        .iter()
        .rev()
        .find(|message| matches!(message.payload, SessionConversationPayload::UserText { .. }))
        .map(|message| message.id.clone())
}

/// Recovers a cache epoch when persisted anchors no longer exist in canonical history.
/// 当持久化锚点已不存在于规范历史时恢复缓存纪元。
///
/// # Parameters
/// 参数
/// `runtime_state` owns both canonical messages and independently hydrated VMM state.
/// `runtime_state` 同时拥有规范消息与已独立水合的 VMM 状态。
/// `updated_at` is the recovery timestamp.
/// `updated_at` 是恢复时间戳。
///
/// # Returns
/// 返回值
/// `true` when the invalid epoch was discarded and continuation was cleared.
/// 丢弃无效纪元并清空 continuation 时返回 `true`。
fn recover_missing_vmm_anchor_targets(
    runtime_state: &mut SessionRuntimeStateEntry,
    updated_at: &str,
) -> Result<bool, String> {
    let canonical_message_ids = runtime_state
        .conversation_state
        .messages
        .iter()
        .map(|message| message.id.as_str())
        .collect::<BTreeSet<_>>();
    let missing_targets = runtime_state
        .vmm_session_memory
        .cache_epoch
        .anchors
        .iter()
        .filter(|anchor| !canonical_message_ids.contains(anchor.target_message_id.as_str()))
        .map(|anchor| anchor.target_message_id.clone())
        .collect::<BTreeSet<_>>();
    if missing_targets.is_empty() {
        return Ok(false);
    }
    let recovery_epoch_id = format!("recovery-{updated_at}");
    runtime_state
        .vmm_session_memory
        .begin_cache_epoch(&recovery_epoch_id, None, updated_at)?;
    runtime_state.vmm_session_memory.recovery_notice = Some(format!(
        "Discarded a VMM cache epoch because {} exact anchor target(s) were absent from canonical history.",
        missing_targets.len()
    ));
    runtime_state.conversation_state.continuation = Default::default();
    runtime_state.updated_at = updated_at.to_string();
    Ok(true)
}

/// Appends one body-free VMM persistence lifecycle event without blocking state recovery on audit failure.
/// 追加一条不含正文的 VMM 持久化生命周期事件，且审计失败不会阻断状态恢复。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory owning the runtime log.
/// `session_dir` 是拥有运行时日志的精确分段会话目录。
/// `timestamp` is the transition timestamp used by the runtime event line.
/// `timestamp` 是运行时事件行使用的转换时间戳。
/// `action` is the exact persistence or migration lifecycle action.
/// `action` 是精确的持久化或迁移生命周期动作。
/// `state` is the validated VMM snapshot after the transition when available.
/// `state` 是可用时转换后的已验证 VMM 快照。
/// `reason` is a stable diagnostic code containing no memory body.
/// `reason` 是不包含记忆正文的稳定诊断代码。
pub(in crate::workspace_store) fn record_vmm_store_lifecycle_event_best_effort(
    session_dir: &Path,
    timestamp: &str,
    action: RuntimeVmmLifecycleAction,
    state: Option<&VmmSessionMemoryState>,
    reason: &str,
) {
    // memory_keys and semantic_content_hashes preserve only stable audit identities.
    // memory_keys 与 semantic_content_hashes 仅保留稳定审计身份。
    let memory_keys = state
        .map(|state| state.active_leases.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let semantic_content_hashes = state
        .map(|state| {
            state
                .active_leases
                .iter()
                .map(|(key, lease)| (key.clone(), lease.semantic_content_hash.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    // body_bytes is an aggregate diagnostic and never stores the rendered content itself.
    // body_bytes 是聚合诊断值，绝不保存渲染正文自身。
    let body_bytes = state
        .map(|state| {
            state
                .active_leases
                .values()
                .map(|lease| render_vmm_context_item_model(&lease.item).len())
                .sum::<usize>()
        })
        .unwrap_or(0);
    let revision = state.map(|state| state.revision).unwrap_or(0);
    let event = RuntimeVmmLifecycleEvent {
        event_id: format!("vmm-store-{reason}-{revision}-{timestamp}"),
        turn_id: None,
        action,
        cache_epoch_id: state.map(|state| state.cache_epoch.epoch_id.clone()),
        anchor_revision: state.map(|state| state.cache_epoch.anchor_revision),
        completed_turn_sequence: state
            .map(|state| state.completed_turn_sequence)
            .unwrap_or(0),
        memory_keys,
        semantic_content_hashes,
        body_bytes,
        reason: Some(reason.to_string()),
    };
    if let Err(error) =
        record_runtime_audit_event(session_dir, timestamp, RuntimeLogEvent::VmmLifecycle(event))
    {
        eprintln!(
            "[vulcan-workspace-store] failed to record body-free VMM lifecycle audit: {error}"
        );
    }
}

/// Moves a corrupt VMM document to an exact sibling diagnostic path.
/// 将损坏的 VMM 文档移动到精确的同级诊断路径。
///
/// # Parameters
/// 参数
/// `path` is the exact corrupt `vmm.json` path.
/// `path` 是精确的损坏 `vmm.json` 路径。
///
/// # Returns
/// 返回值
/// Success after evidence is isolated, including an already-absent source.
/// 证据隔离后返回成功；源文件已经缺失也视为成功。
fn quarantine_corrupt_vmm_file(path: &Path) -> Result<(), String> {
    // quarantine_timestamp is generated locally so arbitrary persisted business timestamps cannot create invalid file names.
    // quarantine_timestamp 在本地生成，避免任意持久化业务时间文本产生非法文件名。
    let quarantine_timestamp = current_unix_millis_text();
    // diagnostic_path uses only host-generated numeric and UUID components and is safe on every supported filesystem.
    // diagnostic_path 仅使用宿主生成的数字与 UUID 组成部分，可安全用于全部受支持文件系统。
    let diagnostic_path = path.with_file_name(format!(
        "vmm.corrupt.{quarantine_timestamp}.{}.json",
        Uuid::new_v4()
    ));
    match fs::rename(path, &diagnostic_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to isolate corrupt VMM state `{}` as `{}`: {error}",
            path.display(),
            diagnostic_path.display()
        )),
    }
}

/// Removes one exact file while treating authoritative absence as success.
/// 移除一个精确文件，并将权威缺失视为成功。
///
/// # Parameters
/// 参数
/// `path` is the exact non-recursive file target.
/// `path` 是精确的非递归文件目标。
///
/// # Returns
/// 返回值
/// Success after removal or `NotFound`, otherwise a path-aware failure.
/// 移除完成或文件不存在时返回成功，否则返回包含路径的失败。
fn remove_exact_file_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Failed to remove `{}`: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vulcan_context::{SessionConversationMessage, SessionConversationState};
    use vulcan_context_injection::{
        ContextInjectionState, InjectionCompactionPolicy, InjectionKind, InjectionScope,
        InjectionStateGroup, InjectionStateItem,
    };
    use vulcan_vmm::{VMM_PRECHECK_MEMORY_RENDERER_ID, VmmRecallBatch, VmmSessionMemoryState};

    /// Builds one temporary segmented session directory owned by a test.
    /// 为测试构建一个独占的临时分段会话目录。
    fn temporary_session_dir() -> PathBuf {
        let directory = std::env::temp_dir().join(format!("vulcan-vmm-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("temporary session directory should be created");
        directory
    }

    /// Builds one canonical user message for persistence and migration tests.
    /// 为持久化与迁移测试构建一条规范用户消息。
    fn user_message(id: &str) -> SessionConversationMessage {
        SessionConversationMessage {
            id: id.to_string(),
            payload: SessionConversationPayload::UserText {
                text: String::from("question"),
                attachments: Vec::new(),
            },
        }
    }

    /// Builds one legacy generic injection state with exact VMM ownership and an optional non-VMM item.
    /// 构建一份具有精确 VMM 归属并可选包含非 VMM 条目的旧版通用注入状态。
    fn legacy_injection_state(include_non_vmm: bool) -> ContextInjectionState {
        // Legacy items are keyed exactly as the old generic injection document persisted them.
        // 旧版条目严格按照旧通用注入文档的持久化键组织。
        let mut items = BTreeMap::from([(
            String::from("vmm-hash"),
            InjectionStateItem {
                content: String::from("legacy-memory"),
                content_hash: String::from("vmm-hash"),
                ttl_turns: Some(3),
                remaining_turns: Some(2),
                last_consumed_turn_id: None,
                compaction_policy: InjectionCompactionPolicy::Keep,
                source_display: Some(String::from("VMM")),
                renderer_id: Some(String::from(VMM_PRECHECK_MEMORY_RENDERER_ID)),
                metadata: Some(json!({
                    "text": "legacy-memory",
                    "score": 0.5,
                    "turn_id": 9,
                    "has_dialogue": true,
                    "created_datetime": "2026-07-20",
                    "memory_id": 11
                })),
                dedupe_key: Some(String::from("memory:11")),
                created_turn_marker: Some(String::from("turn-1")),
                created_at: String::from("1"),
                updated_at: String::from("2"),
            },
        )]);
        if include_non_vmm {
            items.insert(
                String::from("other-hash"),
                InjectionStateItem {
                    content: String::from("keep-non-vmm"),
                    content_hash: String::from("other-hash"),
                    ttl_turns: Some(4),
                    remaining_turns: Some(4),
                    last_consumed_turn_id: None,
                    compaction_policy: InjectionCompactionPolicy::Keep,
                    source_display: Some(String::from("Other")),
                    renderer_id: Some(String::from("other.renderer")),
                    metadata: None,
                    dedupe_key: Some(String::from("other")),
                    created_turn_marker: Some(String::from("turn-1")),
                    created_at: String::from("1"),
                    updated_at: String::from("1"),
                },
            );
        }
        ContextInjectionState {
            groups: BTreeMap::from([(
                String::from("VMM_MEMORY"),
                InjectionStateGroup {
                    tag: String::from("VMM_MEMORY"),
                    description: String::from("Relevant memories."),
                    kind: InjectionKind::Transient,
                    scope: InjectionScope::Session,
                    priority: -50,
                    compaction_policy: InjectionCompactionPolicy::Keep,
                    items,
                },
            )]),
        }
    }

    /// Verifies independent state is atomic, session-owned, and rejects stale revisions.
    /// 验证独立状态具备原子性和会话归属，并拒绝陈旧版本。
    #[test]
    fn independent_vmm_state_rejects_stale_revision() {
        let directory = temporary_session_dir();
        let mut state =
            VmmSessionMemoryState::new("session-a", "1").expect("valid VMM session fixture");
        state.revision = 2;
        write_vmm_state(&directory, "session-a", &state).expect("new state should be written");
        let mut stale = state.clone();
        stale.revision = 1;

        let error = write_vmm_state(&directory, "session-a", &stale)
            .expect_err("stale state should be rejected");

        assert!(error.contains("stale VMM state revision"));
        fs::remove_dir_all(&directory).expect("temporary session directory should be removed");
    }

    /// Verifies independent VMM state round-trips and stale pristine snapshots cannot delete it.
    /// 验证独立 VMM 状态可精确往返，且陈旧纯净快照无法删除它。
    #[test]
    fn independent_vmm_state_round_trips_and_rejects_stale_pristine_snapshot() {
        let directory = temporary_session_dir();
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![vulcan_vmm::ContextItem {
                        text: String::from("round-trip-memory"),
                        memory_id: 9,
                        ..vulcan_vmm::ContextItem::default()
                    }],
                },
                "1",
            )
            .expect("recall should create durable state");
        write_vmm_state(&directory, "session-a", &state).expect("state should be written");

        let loaded =
            load_vmm_state_with_recovery(&directory, "session-a", "2").expect("state should load");

        assert_eq!(loaded.state, Some(state));
        assert!(!loaded.degraded);
        let pristine_error =
            write_vmm_state(&directory, "session-a", &VmmSessionMemoryState::default())
                .expect_err("stale pristine state must not remove a newer independent file");
        assert!(pristine_error.contains("Rejected pristine VMM snapshot"));
        assert!(vmm_state_path(&directory).exists());
        reset_vmm_state_for_history_rewrite(&directory)
            .expect("explicit history rewrite should remove the independent file");
        assert!(!vmm_state_path(&directory).exists());
        fs::remove_dir_all(&directory).expect("temporary session directory should be removed");
    }

    /// Verifies the VMM file limit measures the exact pretty JSON bytes instead of compact JSON.
    /// 验证 VMM 文件限制测量精确美化 JSON 字节，而不是紧凑 JSON。
    #[test]
    fn vmm_state_size_limit_uses_exact_persisted_encoding() {
        // state contains enough structured fields for pretty JSON to be strictly larger than compact JSON.
        // state 包含足够的结构化字段，使美化 JSON 严格大于紧凑 JSON。
        let mut state =
            VmmSessionMemoryState::new("session-a", "1").expect("valid VMM session fixture");
        state.revision = 1;
        // compact_len is the old surrogate measurement that omitted whitespace and the trailing newline.
        // compact_len 是旧替代测量值，它省略了空白与末尾换行符。
        let compact_len = serde_json::to_vec(&state)
            .expect("compact VMM state should serialize")
            .len() as u64;
        // exact_len is the physical representation length used by vmm.json publication.
        // exact_len 是 vmm.json 发布所用物理表示的长度。
        let exact_len = serde_json::to_vec_pretty(&state)
            .expect("pretty VMM state should serialize")
            .len() as u64
            + 1;
        assert!(exact_len > compact_len);

        // error proves a limit that would admit compact JSON still rejects the larger physical document.
        // error 证明一个会接纳紧凑 JSON 的限制仍会拒绝更大的物理文档。
        let error = encode_vmm_state_document(&state, "session-a", exact_len - 1)
            .expect_err("physical document above the exact limit must be rejected");
        assert!(error.contains(&format!("is {exact_len} bytes")));
        // encoded proves the same physical document is admitted at its exact inclusive boundary.
        // encoded 证明同一物理文档可在精确包含式边界上被接纳。
        let encoded = encode_vmm_state_document(&state, "session-a", exact_len)
            .expect("physical document at the exact limit should be accepted");
        assert_eq!(encoded.len() as u64, exact_len);
        assert_eq!(encoded.last(), Some(&b'\n'));
    }

    /// Verifies an oversized write is rejected before replacing the previous complete file.
    /// 验证超限写入会在替换旧完整文件之前被拒绝。
    #[test]
    fn oversized_vmm_state_rejection_preserves_previous_file() {
        let directory = temporary_session_dir();
        let mut previous =
            VmmSessionMemoryState::new("session-a", "1").expect("valid previous VMM fixture");
        previous.revision = 1;
        write_vmm_state(&directory, "session-a", &previous)
            .expect("previous state should be written");
        // oversized is deliberately corrupted after one valid transition so the writer must still defend the durable file boundary.
        // oversized 在一次有效转换后被刻意破坏，用于验证写入器仍会防守持久文件边界。
        let mut oversized = VmmSessionMemoryState::default();
        oversized
            .apply_recall_batch(
                "session-a",
                "user-1",
                &VmmRecallBatch {
                    ttl_turns: 2,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::new(),
                    items: vec![vulcan_vmm::ContextItem {
                        text: String::from("bounded"),
                        memory_id: 1,
                        ..vulcan_vmm::ContextItem::default()
                    }],
                },
                "2",
            )
            .expect("valid aggregate fixture should be created first");
        oversized.revision = 2;
        oversized.cache_epoch.anchors[0].prefix = "x".repeat(VMM_SESSION_MEMORY_MAX_BYTES);

        let error = write_vmm_state(&directory, "session-a", &oversized)
            .expect_err("oversized state should be rejected");
        let durable = read_optional_json::<VmmSessionMemoryState>(&vmm_state_path(&directory))
            .expect("previous file should remain readable")
            .expect("previous file should remain present");

        assert!(error.contains("exceeds"));
        assert_eq!(durable, previous);
        fs::remove_dir_all(&directory).expect("temporary session directory should be removed");
    }

    /// Verifies malformed JSON is quarantined while owner mismatches are rejected without mutation.
    /// 验证格式错误 JSON 会被隔离，而所有者不匹配会在不修改文件的情况下被拒绝。
    #[test]
    fn corrupt_vmm_state_is_quarantined_but_owner_mismatch_is_rejected() {
        let corrupt_directory = temporary_session_dir();
        fs::write(vmm_state_path(&corrupt_directory), b"{broken")
            .expect("corrupt fixture should be written");

        let recovered =
            load_vmm_state_with_recovery(&corrupt_directory, "session-a", "2026-07-20T10:11:12Z")
                .expect(
                    "corrupt state should be isolated with an arbitrary persisted timestamp format",
                );
        let quarantined_count = fs::read_dir(&corrupt_directory)
            .expect("quarantine directory should be readable")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("vmm.corrupt.")
            })
            .count();

        assert!(recovered.degraded);
        assert_eq!(quarantined_count, 1);
        assert!(!vmm_state_path(&corrupt_directory).exists());

        let mismatch_directory = temporary_session_dir();
        let mut foreign =
            VmmSessionMemoryState::new("session-foreign", "1").expect("valid foreign VMM fixture");
        foreign.revision = 1;
        write_json_file_atomic(&vmm_state_path(&mismatch_directory), &foreign)
            .expect("foreign fixture should be written without owner validation");

        let error = load_vmm_state_with_recovery(&mismatch_directory, "session-a", "2")
            .expect_err("owner mismatch should be rejected");

        assert!(error.contains("owner mismatch"));
        assert!(vmm_state_path(&mismatch_directory).exists());
        fs::remove_dir_all(&corrupt_directory)
            .expect("temporary corrupt directory should be removed");
        fs::remove_dir_all(&mismatch_directory)
            .expect("temporary mismatch directory should be removed");
    }

    /// Verifies legacy state migrates once, cleans addons physically, and cannot revive after completion.
    /// 验证旧状态只迁移一次、附属状态被物理清理，且完成后不能复活。
    #[test]
    fn legacy_migration_cleans_addons_and_completion_marker_blocks_revival() {
        let directory = temporary_session_dir();
        let session_uuid = Uuid::new_v4();
        let session_id = session_uuid.to_string();
        let legacy_state = legacy_injection_state(true);
        let mut runtime_state = SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: vec![user_message("user-1")],
                ..SessionConversationState::default()
            },
            context_injection_state: legacy_state.clone(),
            updated_at: String::from("10"),
            ..SessionRuntimeStateEntry::default()
        };
        // The addon contains both the legacy namespace and an unrelated namespace that must survive.
        // 附属状态同时包含旧版命名空间和必须保留的无关命名空间。
        let mut addon_state = SessionAddonState::new(session_uuid);
        assert!(addon_state.set_typed_namespace(
            SessionAddonNamespace::ContextInjection,
            &legacy_state,
            String::from("10")
        ));
        assert!(addon_state.set_json_namespace(
            SessionAddonNamespace::RequestPreview,
            json!({"keep": true}),
            String::from("10")
        ));
        write_session_addon_state(&directory, &addon_state)
            .expect("legacy addon fixture should be written");

        hydrate_vmm_session_state(&directory, &session_id, &mut runtime_state)
            .expect("legacy VMM state should migrate");

        let persisted_addon = read_session_addon_state(&directory)
            .expect("cleaned addon should be readable")
            .expect("cleaned addon should remain present");
        let cleaned_context = persisted_addon
            .typed_namespace::<ContextInjectionState>(SessionAddonNamespace::ContextInjection)
            .expect("non-VMM context injection should remain");
        let state_text = fs::read_to_string(directory.join(SEGMENTED_STATE_FILE))
            .expect("cleaned state document should be readable");

        assert!(
            runtime_state
                .vmm_session_memory
                .active_leases
                .contains_key("memory:11")
        );
        assert_eq!(
            runtime_state.vmm_session_memory.active_leases["memory:11"].expires_after_turn_sequence,
            2
        );
        assert_eq!(cleaned_context.groups["VMM_MEMORY"].items.len(), 1);
        assert!(
            cleaned_context.groups["VMM_MEMORY"]
                .items
                .contains_key("other-hash")
        );
        assert!(
            persisted_addon
                .namespaces
                .contains_key(&SessionAddonNamespace::RequestPreview)
        );
        assert!(vmm_migration_completion_path(&directory).exists());
        assert!(!vmm_migration_path(&directory).exists());
        assert!(!state_text.contains("vmm_session_memory"));
        assert!(!state_text.contains("legacy-memory"));

        // Removing vmm.json simulates an old backup restoring legacy state after migration completed.
        // 删除 vmm.json 用于模拟迁移完成后旧备份恢复旧状态的场景。
        fs::remove_file(vmm_state_path(&directory)).expect("migrated VMM file should be removed");
        let mut resurrected_runtime = SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: vec![user_message("user-1")],
                ..SessionConversationState::default()
            },
            context_injection_state: legacy_injection_state(false),
            updated_at: String::from("20"),
            ..SessionRuntimeStateEntry::default()
        };

        hydrate_vmm_session_state(&directory, &session_id, &mut resurrected_runtime)
            .expect("completion marker should clean but never re-import restored legacy data");

        assert!(
            resurrected_runtime
                .vmm_session_memory
                .active_leases
                .is_empty()
        );
        assert!(resurrected_runtime.context_injection_state.is_empty());
        fs::remove_dir_all(&directory).expect("temporary session directory should be removed");
    }

    /// Verifies an invalid exact-owned legacy item is durably removed without becoming an active lease.
    /// 验证无效的精确归属旧版条目会被持久删除且不会成为活动租约。
    #[test]
    fn invalid_only_legacy_state_is_cleaned_without_lease_revival() {
        // directory receives the cleanup transaction and durable completion proof.
        // directory 接收清理事务与持久化完成证明。
        let directory = temporary_session_dir();
        // session_id is the canonical owner used by the new empty VMM state.
        // session_id 是新空 VMM 状态使用的规范所有者。
        let session_id = Uuid::new_v4().to_string();
        // invalid_legacy_state starts from an exact-owned fixture and removes its persisted TTL.
        // invalid_legacy_state 从精确归属夹具构建，并移除其持久化 TTL。
        let mut invalid_legacy_state = legacy_injection_state(false);
        invalid_legacy_state
            .groups
            .get_mut("VMM_MEMORY")
            .expect("legacy group should exist")
            .items
            .get_mut("vmm-hash")
            .expect("legacy item should exist")
            .remaining_turns = None;
        // runtime_state contains no other data that could conceal whether cleanup was durably committed.
        // runtime_state 不包含其他可能掩盖清理是否持久提交的数据。
        let mut runtime_state = SessionRuntimeStateEntry {
            context_injection_state: invalid_legacy_state,
            updated_at: String::from("10"),
            ..SessionRuntimeStateEntry::default()
        };

        hydrate_vmm_session_state(&directory, &session_id, &mut runtime_state)
            .expect("invalid exact-owned legacy state should be cleaned");

        assert!(runtime_state.context_injection_state.is_empty());
        assert!(runtime_state.vmm_session_memory.active_leases.is_empty());
        assert_eq!(runtime_state.vmm_session_memory.revision, 1);
        runtime_state
            .vmm_session_memory
            .validate(&session_id)
            .expect("invalid-only cleanup must publish a valid positive-revision VMM document");
        assert!(vmm_state_path(&directory).exists());
        assert!(vmm_migration_completion_path(&directory).exists());
        // state_text proves the malformed legacy payload cannot revive on the next load.
        // state_text 证明格式异常的旧载荷无法在下一次加载时复活。
        let state_text = fs::read_to_string(directory.join(SEGMENTED_STATE_FILE))
            .expect("cleaned state document should be readable");
        assert!(!state_text.contains("legacy-memory"));
        assert!(!state_text.contains(VMM_PRECHECK_MEMORY_RENDERER_ID));
        fs::remove_dir_all(&directory).expect("temporary session directory should be removed");
    }

    /// Verifies a prepared migration transaction is deterministically completed on restart.
    /// 验证准备阶段的迁移事务会在重启时被确定性完成。
    #[test]
    fn interrupted_migration_transaction_resumes_forward_to_completion() {
        let directory = temporary_session_dir();
        let session_uuid = Uuid::new_v4();
        let session_id = session_uuid.to_string();
        let runtime_state = SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: vec![user_message("user-1")],
                ..SessionConversationState::default()
            },
            updated_at: String::from("10"),
            ..SessionRuntimeStateEntry::default()
        };
        let mut target_vmm_state = VmmSessionMemoryState::default();
        target_vmm_state
            .apply_recall_batch(
                &session_id,
                "user-1",
                &VmmRecallBatch {
                    ttl_turns: 2,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![vulcan_vmm::ContextItem {
                        text: String::from("transaction-memory"),
                        memory_id: 12,
                        ..vulcan_vmm::ContextItem::default()
                    }],
                },
                "10",
            )
            .expect("target migration state should be built");
        let transaction = prepare_vmm_migration_transaction(
            &directory,
            &session_id,
            None,
            target_vmm_state.clone(),
            &runtime_state,
            "10",
        )
        .expect("migration transaction should be prepared");
        write_vmm_migration_transaction(&directory, &transaction)
            .expect("prepared marker should be durable before simulated restart");
        // A newer valid VMM revision may commit before recovery reaches the stale migration marker.
        // 在恢复处理陈旧迁移标记前，更新的有效 VMM 版本可能已经提交。
        let mut newer_vmm_state = target_vmm_state.clone();
        newer_vmm_state
            .apply_recall_batch(
                &session_id,
                "user-2",
                &VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![vulcan_vmm::ContextItem {
                        text: String::from("transaction-memory"),
                        memory_id: 12,
                        ..vulcan_vmm::ContextItem::default()
                    }],
                },
                "11",
            )
            .expect("newer VMM state should refresh the existing lease");
        write_vmm_state(&directory, &session_id, &newer_vmm_state)
            .expect("newer VMM state should persist before recovery");

        recover_vmm_migration_transaction(&directory, &session_id)
            .expect("restart should finish the prepared transaction");

        let durable_vmm = read_optional_json::<VmmSessionMemoryState>(&vmm_state_path(&directory))
            .expect("durable VMM state should be readable")
            .expect("durable VMM state should exist");
        assert_eq!(durable_vmm, newer_vmm_state);
        assert!(directory.join(SEGMENTED_STATE_FILE).exists());
        assert!(vmm_migration_completion_path(&directory).exists());
        assert!(!vmm_migration_path(&directory).exists());
        fs::remove_dir_all(&directory).expect("temporary session directory should be removed");
    }

    /// Verifies migration-marker validation rejects a target state that could revive legacy VMM injections.
    /// 验证迁移标记校验会拒绝可能复活旧版 VMM 注入的目标状态。
    #[test]
    fn migration_transaction_rejects_unclean_legacy_target_state() {
        // directory owns the exact before-and-after documents used to prepare the invalid marker.
        // directory 持有用于准备无效标记的精确前后文档。
        let directory = temporary_session_dir();
        // session_id is a valid canonical owner for both VMM and addon schemas.
        // session_id 是 VMM 与附属状态结构共用的有效规范所有者。
        let session_id = Uuid::new_v4().to_string();
        // runtime_state deliberately retains one renderer-owned legacy item in its migration target.
        // runtime_state 刻意在迁移目标中保留一条渲染器归属的旧版条目。
        let runtime_state = SessionRuntimeStateEntry {
            context_injection_state: legacy_injection_state(false),
            updated_at: String::from("10"),
            ..SessionRuntimeStateEntry::default()
        };
        // target_vmm_state is independently valid so the legacy-cleanliness guard is the rejecting condition.
        // target_vmm_state 本身有效，确保拒绝条件来自旧版数据清理防护。
        let mut target_vmm_state =
            VmmSessionMemoryState::new(&session_id, "10").expect("valid target VMM fixture");
        target_vmm_state.revision = 1;
        // transaction models a corrupt or manually altered prepared marker.
        // transaction 模拟损坏或被手工修改的准备阶段标记。
        let transaction = prepare_vmm_migration_transaction(
            &directory,
            &session_id,
            None,
            target_vmm_state,
            &runtime_state,
            "10",
        )
        .expect("invalid migration transaction fixture should prepare");

        // error proves the marker is rejected before any forward-replay write is attempted.
        // error 证明标记会在尝试任何向前重放写入前被拒绝。
        let error = validate_vmm_migration_transaction(&transaction)
            .expect_err("unclean migration target must be rejected");

        assert!(error.contains("still contains legacy VMM injection items"));
        assert!(!vmm_state_path(&directory).exists());
        assert!(!directory.join(SEGMENTED_STATE_FILE).exists());
        fs::remove_dir_all(&directory).expect("temporary session directory should be removed");
    }

    /// Verifies a missing anchor target discards only the epoch and preserves active leases.
    /// 验证锚点目标缺失时仅丢弃纪元并保留活动租约。
    #[test]
    fn missing_anchor_target_resets_epoch_and_continuation() {
        let mut runtime_state = SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: vec![user_message("user-1")],
                ..SessionConversationState::default()
            },
            ..SessionRuntimeStateEntry::default()
        };
        runtime_state
            .vmm_session_memory
            .apply_recall_batch(
                "session-a",
                "missing-user",
                &VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![vulcan_vmm::ContextItem {
                        text: String::from("memory"),
                        memory_id: 9,
                        ..vulcan_vmm::ContextItem::default()
                    }],
                },
                "1",
            )
            .expect("recall should build an invalid persisted target for the test");
        runtime_state
            .conversation_state
            .continuation
            .last_provider_message_id = Some(String::from("resp-1"));
        runtime_state
            .conversation_state
            .continuation
            .last_local_message_id = Some(String::from("assistant-1"));

        assert!(
            recover_missing_vmm_anchor_targets(&mut runtime_state, "2")
                .expect("missing target should recover")
        );
        assert!(
            runtime_state
                .vmm_session_memory
                .cache_epoch
                .anchors
                .is_empty()
        );
        assert!(
            runtime_state
                .vmm_session_memory
                .cache_epoch
                .needs_active_baseline
        );
        assert!(
            runtime_state
                .vmm_session_memory
                .active_leases
                .contains_key("memory:9")
        );
        assert!(
            runtime_state
                .conversation_state
                .continuation
                .last_provider_message_id
                .is_none()
        );
        assert!(
            runtime_state
                .conversation_state
                .continuation
                .last_local_message_id
                .is_none()
        );
    }
}
