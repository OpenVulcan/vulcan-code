//! Strict persistence adapter for Core-owned per-session VMM state.
//! Core 所有的每会话 VMM 状态严格持久化适配器。

use super::*;
use vulcan_vmm::{VMM_SESSION_MEMORY_MAX_BYTES, VmmSessionMemoryState};

/// Independent VMM state file stored at the session-directory root.
/// 保存在会话目录根部的独立 VMM 状态文件。
pub(super) const SEGMENTED_VMM_STATE_FILE: &str = "vmm.json";

/// Historical migration marker rejected by the Core-only format.
/// Core 独占格式拒绝的历史迁移标记。
pub(super) const SEGMENTED_VMM_MIGRATION_FILE: &str = "vmm.migration.json";

/// Historical migration completion marker rejected by the Core-only format.
/// Core 独占格式拒绝的历史迁移完成标记。
pub(super) const SEGMENTED_VMM_MIGRATION_COMPLETED_FILE: &str = "vmm.migration.completed.json";

/// Historical runtime/VMM checkpoint marker rejected by the Core-only format.
/// Core 独占格式拒绝的历史运行时与 VMM 检查点标记。
pub(super) const SEGMENTED_VMM_CHECKPOINT_FILE: &str = "vmm.checkpoint.json";

/// Current schema version for Core-owned VMM controller documents.
/// Core 所有的 VMM 控制器文档当前结构版本。
const CORE_VMM_CONTROLLER_DOCUMENT_SCHEMA_VERSION: u32 = 2;

/// Stable owner kind proving that a VMM document belongs to the Core session controller.
/// 证明 VMM 文档归属于 Core 会话控制器的稳定所有者类型。
const CORE_VMM_CONTROLLER_KIND: &str = "core_session_vmm_controller";

/// Strict durable document owned only by one Core session VMM controller.
/// 仅由一个 Core 会话 VMM 控制器拥有的严格持久化文档。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoreVmmControllerDocument {
    /// Durable controller-document schema version.
    /// 持久化控制器文档结构版本。
    schema_version: u32,
    /// Stable controller owner kind.
    /// 稳定的控制器所有者类型。
    controller_kind: String,
    /// Exact host session identifier owning this document.
    /// 拥有该文档的精确宿主会话标识。
    session_id: String,
    /// Complete domain state committed by the owning controller.
    /// 所属控制器提交的完整领域状态。
    state: VmmSessionMemoryState,
}

/// Returns whether one root-level file belongs to current or rejected historical VMM state.
/// 返回一个根级文件是否归属于当前或已拒绝的历史 VMM 状态。
///
/// # Parameters
/// 参数
/// `file_name` is the exact UTF-8 file name inspected during a session fork.
/// `file_name` 是会话分叉期间检查的精确 UTF-8 文件名。
///
/// # Returns
/// 返回值
/// `true` for the Core document, obsolete markers, and abandoned atomic staging files.
/// 对 Core 文档、废弃标记与遗留原子暂存文件返回 `true`。
pub(super) fn is_vmm_session_artifact_file_name(file_name: &str) -> bool {
    // artifact_names enumerate every current or historical VMM-owned root file.
    // artifact_names 枚举全部当前或历史 VMM 所有的根文件。
    let artifact_names = [
        SEGMENTED_VMM_STATE_FILE,
        SEGMENTED_VMM_MIGRATION_FILE,
        SEGMENTED_VMM_MIGRATION_COMPLETED_FILE,
        SEGMENTED_VMM_CHECKPOINT_FILE,
    ];
    artifact_names.contains(&file_name)
        || artifact_names
            .iter()
            .any(|artifact| file_name.starts_with(&format!(".{artifact}.vulcan-stage-")))
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

/// Strictly loads the Core-owned VMM controller state for one session directory.
/// 严格加载单个会话目录中由 Core 所有的 VMM 控制器状态。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory.
/// `session_dir` 是精确的分段会话目录。
/// `expected_session_id` is the exact session owner expected by Core.
/// `expected_session_id` 是 Core 期望的精确会话所有者。
///
/// # Returns
/// 返回值
/// The validated state, clean absence, or an explicit format and ownership error.
/// 返回已校验状态、干净缺失，或明确的格式与所有权错误。
pub(in crate::workspace_store) fn load_core_vmm_controller_state(
    session_dir: &Path,
    expected_session_id: &str,
) -> Result<Option<VmmSessionMemoryState>, String> {
    reject_historical_vmm_markers(session_dir, expected_session_id)?;
    // path is the only durable document location owned by the session controller.
    // path 是会话控制器拥有的唯一持久化文档位置。
    let path = vmm_state_path(session_dir);
    // guard serializes the physical read with atomic state-file publication.
    // guard 将物理读取与原子状态文件发布串行化。
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // metadata rejects oversized historical or corrupt content before JSON allocation.
    // metadata 在 JSON 分配前拒绝超限的历史或损坏内容。
    match fs::metadata(&path) {
        Ok(metadata) if metadata.len() > VMM_SESSION_MEMORY_MAX_BYTES as u64 => {
            return Err(format!(
                "Core VMM controller document `{}` exceeds the {} byte limit.",
                path.display(),
                VMM_SESSION_MEMORY_MAX_BYTES
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(to_store_error(error)),
    }
    // raw_value preserves enough structure to identify and explicitly reject the historical flat format.
    // raw_value 保留足够结构，以识别并明确拒绝历史扁平格式。
    let raw_value = read_optional_json::<Value>(&path)
        .map_err(|error| {
            format!(
                "Failed to parse Core VMM controller document `{}`: {error}",
                path.display()
            )
        })?
        .ok_or_else(|| {
            format!(
                "Core VMM controller document `{}` disappeared.",
                path.display()
            )
        })?;
    if raw_value.get("controller_kind").is_none()
        && (raw_value.get("revision").is_some() || raw_value.get("active_leases").is_some())
    {
        return Err(format!(
            "Unsupported historical VMM state format in `{}`; automatic migration is disabled.",
            path.display()
        ));
    }
    // document is decoded only after the historical format check so the diagnostic is intentional.
    // document 仅在历史格式检查后解码，使诊断具有明确含义。
    let document: CoreVmmControllerDocument =
        serde_json::from_value(raw_value).map_err(to_store_error)?;
    validate_core_vmm_controller_document(&document, expected_session_id)?;
    Ok(Some(document.state))
}

/// Atomically persists one state owned by the exact Core session VMM controller.
/// 原子持久化一份由精确 Core 会话 VMM 控制器拥有的状态。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory.
/// `session_dir` 是精确的分段会话目录。
/// `expected_session_id` is the exact owning session identifier.
/// `expected_session_id` 是精确的所属会话标识。
/// `state` is the complete candidate already serialized by its controller.
/// `state` 是已经由其控制器串行化的完整候选状态。
///
/// # Returns
/// 返回值
/// Success only after the candidate is durably published or confirmed idempotent.
/// 仅在候选状态持久发布或确认幂等后返回成功。
pub(in crate::workspace_store) fn write_core_vmm_controller_state(
    session_dir: &Path,
    expected_session_id: &str,
    state: &VmmSessionMemoryState,
) -> Result<(), String> {
    reject_historical_vmm_markers(session_dir, expected_session_id)?;
    state.validate(expected_session_id)?;
    // document binds the domain state to the new Core-only persistence contract.
    // document 将领域状态绑定到新的 Core 独占持久化契约。
    let document = CoreVmmControllerDocument {
        schema_version: CORE_VMM_CONTROLLER_DOCUMENT_SCHEMA_VERSION,
        controller_kind: String::from(CORE_VMM_CONTROLLER_KIND),
        session_id: expected_session_id.to_string(),
        state: state.clone(),
    };
    // encoded is the exact byte representation published to `vmm.json`.
    // encoded 是发布到 `vmm.json` 的精确字节表示。
    let mut encoded = serde_json::to_vec_pretty(&document).map_err(to_store_error)?;
    encoded.push(b'\n');
    if encoded.len() > VMM_SESSION_MEMORY_MAX_BYTES {
        return Err(format!(
            "Core VMM controller state for session `{expected_session_id}` is {} bytes and exceeds the {} byte limit.",
            encoded.len(),
            VMM_SESSION_MEMORY_MAX_BYTES
        ));
    }
    // path is resolved once before entering the single physical file critical section.
    // path 在进入唯一物理文件临界区前只解析一次。
    let path = vmm_state_path(session_dir);
    // guard prevents interleaved reads and atomic replacements inside the storage adapter.
    // guard 防止存储适配器内的读取与原子替换交错。
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    if path.try_exists().map_err(to_store_error)? {
        // raw_value is inspected before decode to reject the old flat document without fallback.
        // raw_value 在解码前检查，以便无回退地拒绝旧扁平文档。
        let raw_value = read_optional_json::<Value>(&path)?.ok_or_else(|| {
            format!(
                "Core VMM controller document `{}` disappeared.",
                path.display()
            )
        })?;
        if raw_value.get("controller_kind").is_none() {
            return Err(format!(
                "Unsupported historical VMM state format in `{}`; automatic migration is disabled.",
                path.display()
            ));
        }
        // existing is the only durable predecessor accepted by the controller writer.
        // existing 是控制器写入器接受的唯一持久前序状态。
        let existing: CoreVmmControllerDocument =
            serde_json::from_value(raw_value).map_err(to_store_error)?;
        validate_core_vmm_controller_document(&existing, expected_session_id)?;
        if existing.state.revision > state.revision {
            return Err(format!(
                "Rejected stale Core VMM controller revision {} because durable revision {} is newer for session `{expected_session_id}`.",
                state.revision, existing.state.revision
            ));
        }
        if existing.state.revision == state.revision {
            if existing == document {
                return Ok(());
            }
            return Err(format!(
                "Rejected divergent Core VMM controller state at revision {} for session `{expected_session_id}`.",
                state.revision
            ));
        }
    }
    write_file_bytes_atomic(&path, &encoded, "Core VMM controller document")
}

/// Validates one strict Core controller document before it can become authoritative.
/// 在严格 Core 控制器文档成为权威状态前校验它。
///
/// # Parameters
/// 参数
/// `document` is the decoded durable document.
/// `document` 是已解码的持久化文档。
/// `expected_session_id` is the exact session owner requested by Core.
/// `expected_session_id` 是 Core 请求的精确会话所有者。
///
/// # Returns
/// 返回值
/// Success only when schema, owner kind, owner id, and domain state all agree.
/// 仅当结构、所有者类型、所有者标识和领域状态全部一致时返回成功。
fn validate_core_vmm_controller_document(
    document: &CoreVmmControllerDocument,
    expected_session_id: &str,
) -> Result<(), String> {
    if document.schema_version != CORE_VMM_CONTROLLER_DOCUMENT_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported Core VMM controller document schema version {}.",
            document.schema_version
        ));
    }
    if document.controller_kind != CORE_VMM_CONTROLLER_KIND {
        return Err(format!(
            "Unsupported VMM controller owner kind `{}`.",
            document.controller_kind
        ));
    }
    if document.session_id != expected_session_id {
        return Err(format!(
            "Core VMM controller owner mismatch: expected `{expected_session_id}`, found `{}`.",
            document.session_id
        ));
    }
    document.state.validate(expected_session_id)
}

/// Rejects every obsolete migration or coupled-checkpoint marker for one session.
/// 拒绝单个会话中的全部废弃迁移或耦合检查点标记。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact owning session directory.
/// `session_dir` 是精确的所属会话目录。
/// `session_id` is the owner included in the diagnostic.
/// `session_id` 是诊断中包含的所有者。
///
/// # Returns
/// 返回值
/// Success only when no historical marker exists.
/// 仅在不存在历史标记时返回成功。
fn reject_historical_vmm_markers(session_dir: &Path, session_id: &str) -> Result<(), String> {
    // marker_names are the removed formats that Core intentionally never replays.
    // marker_names 是 Core 明确不再重放的已移除格式。
    let marker_names = [
        SEGMENTED_VMM_MIGRATION_FILE,
        SEGMENTED_VMM_MIGRATION_COMPLETED_FILE,
        SEGMENTED_VMM_CHECKPOINT_FILE,
    ];
    for marker_name in marker_names {
        // marker_path is checked independently to produce the exact offending artifact.
        // marker_path 独立检查，以报告精确的违规工件。
        let marker_path = session_dir.join(marker_name);
        if marker_path.try_exists().map_err(to_store_error)? {
            return Err(format!(
                "Session `{session_id}` contains unsupported historical VMM artifact `{}`; automatic migration and replay are disabled.",
                marker_path.display()
            ));
        }
    }
    Ok(())
}

/// Removes VMM-owned artifacts after an explicit destructive history rewrite.
/// 在明确的破坏性历史重写后移除 VMM 所有工件。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact rewritten session directory.
/// `session_dir` 是被重写的精确会话目录。
///
/// # Returns
/// 返回值
/// Success after every current and historical VMM root artifact is absent.
/// 在全部当前与历史 VMM 根工件均不存在后返回成功。
pub(in crate::workspace_store) fn reset_vmm_state_for_history_rewrite(
    session_dir: &Path,
) -> Result<(), String> {
    // artifact_names are explicit to avoid any broad recursive deletion.
    // artifact_names 使用明确清单，以避免任何宽泛递归删除。
    let artifact_names = [
        SEGMENTED_VMM_STATE_FILE,
        SEGMENTED_VMM_MIGRATION_FILE,
        SEGMENTED_VMM_MIGRATION_COMPLETED_FILE,
        SEGMENTED_VMM_CHECKPOINT_FILE,
    ];
    for artifact_name in artifact_names {
        remove_exact_file_if_present(&session_dir.join(artifact_name))?;
    }
    // directory_entries remove only explicitly recognized VMM temporary or diagnostic artifacts.
    // directory_entries 仅移除明确识别的 VMM 临时或诊断工件。
    for directory_entry in fs::read_dir(session_dir).map_err(to_store_error)? {
        let directory_entry = directory_entry.map_err(to_store_error)?;
        if !directory_entry
            .file_type()
            .map_err(to_store_error)?
            .is_file()
        {
            continue;
        }
        // file_name is matched without following links or traversing child directories.
        // file_name 在不跟随链接或遍历子目录的情况下完成匹配。
        let file_name = directory_entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if is_removed_vmm_auxiliary_artifact(file_name) {
            remove_exact_file_if_present(&directory_entry.path())?;
        }
    }
    Ok(())
}

/// Returns whether one immediate file is an obsolete VMM diagnostic or staging artifact.
/// 返回一个直接文件是否属于废弃的 VMM 诊断或暂存工件。
///
/// # Parameters
/// 参数
/// `file_name` is one immediate non-directory file name.
/// `file_name` 是一个直接的非目录文件名。
///
/// # Returns
/// 返回值
/// `true` only for explicitly owned VMM auxiliary naming patterns.
/// 仅对明确归属于 VMM 的辅助命名模式返回 `true`。
fn is_removed_vmm_auxiliary_artifact(file_name: &str) -> bool {
    (file_name.starts_with("vmm.corrupt.") && file_name.ends_with(".json"))
        || file_name.starts_with(".vmm.json.vulcan-stage-")
}

/// Removes copied VMM ownership from a newly forked session target.
/// 从新分叉的会话目标中移除复制来的 VMM 所有权。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact fork target directory.
/// `session_dir` 是精确的分叉目标目录。
/// `session_id` is the fork target identity used only for ownership diagnostics.
/// `session_id` 是仅用于所有权诊断的分叉目标身份。
/// `updated_at` is the fork timestamp retained for call-site contract clarity.
/// `updated_at` 是为调用点契约清晰度保留的分叉时间戳。
///
/// # Returns
/// 返回值
/// Success after the target contains no inherited VMM controller artifact.
/// 在目标不包含继承的 VMM 控制器工件后返回成功。
pub(in crate::workspace_store) fn purge_vmm_state_from_fork_target(
    session_dir: &Path,
    session_id: &str,
    updated_at: &str,
) -> Result<(), String> {
    if session_id.trim().is_empty() || updated_at.trim().is_empty() {
        return Err(String::from(
            "Fork VMM purge requires a non-empty target session id and timestamp.",
        ));
    }
    reset_vmm_state_for_history_rewrite(session_dir)
}

/// Removes one exact file and treats clean absence as success.
/// 移除一个精确文件，并将干净缺失视为成功。
///
/// # Parameters
/// 参数
/// `path` is the exact non-recursive file target.
/// `path` 是精确的非递归文件目标。
///
/// # Returns
/// 返回值
/// Success when the file was removed or already absent.
/// 文件已移除或原本不存在时返回成功。
fn remove_exact_file_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(to_store_error(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates one isolated session directory for strict VMM adapter tests.
    /// 为严格 VMM 适配器测试创建一个隔离会话目录。
    ///
    /// # Returns
    /// 返回值
    /// The temporary directory owner.
    /// 返回临时目录所有者。
    fn temporary_session_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temporary VMM session directory")
    }

    /// Verifies the new Core envelope round-trips and preserves command receipts.
    /// 验证新的 Core 信封能够往返并保留命令回执。
    #[test]
    fn core_vmm_controller_document_round_trips() {
        // directory owns the isolated `vmm.json`.
        // directory 拥有隔离的 `vmm.json`。
        let directory = temporary_session_dir();
        // state is a valid controller candidate with one durable command receipt.
        // state 是带有一个持久命令回执的有效控制器候选状态。
        let mut state = VmmSessionMemoryState::new("session-a", "1").expect("create VMM state");
        state
            .record_committed_controller_command("command-a", state.revision, "2")
            .expect("record command receipt");

        write_core_vmm_controller_state(directory.path(), "session-a", &state)
            .expect("write Core VMM document");
        let loaded = load_core_vmm_controller_state(directory.path(), "session-a")
            .expect("load Core VMM document")
            .expect("Core VMM document should exist");

        assert_eq!(loaded, state);
    }

    /// Verifies the historical flat document is rejected without migration or quarantine.
    /// 验证历史扁平文档在不迁移、不隔离的情况下被拒绝。
    #[test]
    fn historical_flat_vmm_document_is_rejected() {
        // directory owns the isolated historical artifact.
        // directory 拥有隔离的历史工件。
        let directory = temporary_session_dir();
        // state represents the removed flat persistence format.
        // state 表示已经移除的扁平持久化格式。
        let state = VmmSessionMemoryState::new("session-a", "1").expect("create VMM state");
        write_json_file_atomic(&vmm_state_path(directory.path()), &state)
            .expect("write historical flat state");

        let error = load_core_vmm_controller_state(directory.path(), "session-a")
            .expect_err("historical state must be rejected");

        assert!(error.contains("Unsupported historical VMM state format"));
        assert!(vmm_state_path(directory.path()).exists());
    }

    /// Verifies every obsolete migration or coupled-checkpoint marker blocks Core state loading.
    /// 验证每个废弃迁移标记或耦合检查点标记都会阻止 Core 状态加载。
    #[test]
    fn historical_vmm_markers_are_rejected() {
        // marker_names enumerate the complete removed persistence contracts.
        // marker_names 枚举全部已移除的持久化契约。
        let marker_names = [
            SEGMENTED_VMM_MIGRATION_FILE,
            SEGMENTED_VMM_MIGRATION_COMPLETED_FILE,
            SEGMENTED_VMM_CHECKPOINT_FILE,
        ];
        for marker_name in marker_names {
            // directory owns one isolated historical marker per assertion.
            // directory 为每个断言独占一个历史标记。
            let directory = temporary_session_dir();
            // marker_path is the exact removed artifact under test.
            // marker_path 是当前测试的精确已移除工件。
            let marker_path = directory.path().join(marker_name);
            write_json_file_atomic(&marker_path, &serde_json::json!({"schema_version": 1}))
                .expect("write historical marker");

            // error must identify the exact unsupported artifact without modifying it.
            // error 必须标识精确的不支持工件，且不得修改该工件。
            let error = load_core_vmm_controller_state(directory.path(), "session-a")
                .expect_err("historical marker must be rejected");

            assert!(error.contains("unsupported historical VMM artifact"));
            assert!(error.contains(marker_name));
            assert!(marker_path.exists());
        }
    }

    /// Verifies the current envelope rejects unknown nested state fields instead of accepting compatibility data.
    /// 验证当前信封会拒绝未知的嵌套状态字段，而不是接受兼容数据。
    #[test]
    fn core_vmm_controller_document_rejects_unknown_state_fields() {
        // directory owns the isolated document under strict decoding.
        // directory 拥有接受严格解码的隔离文档。
        let directory = temporary_session_dir();
        // state supplies a valid current document before deliberate corruption.
        // state 在刻意破坏前提供一份有效的当前文档。
        let state = VmmSessionMemoryState::new("session-a", "1").expect("create VMM state");
        write_core_vmm_controller_state(directory.path(), "session-a", &state)
            .expect("write strict Core VMM document");
        // document_value preserves the valid envelope while adding one unsupported nested field.
        // document_value 保留有效信封，同时添加一个不受支持的嵌套字段。
        let mut document_value: Value = read_optional_json(&vmm_state_path(directory.path()))
            .expect("read strict Core VMM document")
            .expect("strict Core VMM document should exist");
        document_value["state"]["historical_writer"] = Value::Bool(true);
        write_json_file_atomic(&vmm_state_path(directory.path()), &document_value)
            .expect("write document with unknown nested state field");

        let error = load_core_vmm_controller_state(directory.path(), "session-a")
            .expect_err("unknown nested state field must be rejected");

        assert!(error.contains("unknown field `historical_writer`"));
    }

    /// Verifies the current cache-epoch format rejects omitted required fields.
    /// 验证当前缓存纪元格式会拒绝缺失的必填字段。
    #[test]
    fn core_vmm_controller_document_rejects_missing_cache_epoch_fields() {
        // directory owns the isolated strict document.
        // directory 拥有隔离的严格文档。
        let directory = temporary_session_dir();
        // state supplies every required current-format field before deliberate removal.
        // state 在刻意移除字段前提供当前格式要求的全部字段。
        let state = VmmSessionMemoryState::new("session-a", "1").expect("create VMM state");
        write_core_vmm_controller_state(directory.path(), "session-a", &state)
            .expect("write strict Core VMM document");
        // document_value removes one current field to prove old partial shapes are not accepted.
        // document_value 移除一个当前字段，以证明旧的不完整结构不会被接受。
        let mut document_value: Value = read_optional_json(&vmm_state_path(directory.path()))
            .expect("read strict Core VMM document")
            .expect("strict Core VMM document should exist");
        document_value["state"]["cache_epoch"]
            .as_object_mut()
            .expect("cache epoch must be an object")
            .remove("anchor_revision");
        write_json_file_atomic(&vmm_state_path(directory.path()), &document_value)
            .expect("write document with missing cache epoch field");

        let error = load_core_vmm_controller_state(directory.path(), "session-a")
            .expect_err("missing current cache epoch field must be rejected");

        assert!(error.contains("missing field `anchor_revision`"));
    }

    /// Verifies a Core envelope cannot be loaded through a different session identity.
    /// 验证 Core 信封不能通过不同的会话身份加载。
    #[test]
    fn core_vmm_controller_document_rejects_session_mismatch() {
        // directory owns the isolated valid document with a deliberately different owner.
        // directory 拥有一份所有者被刻意设为不同值的隔离有效文档。
        let directory = temporary_session_dir();
        // state is valid only for its declared session owner.
        // state 仅对其声明的会话所有者有效。
        let state = VmmSessionMemoryState::new("session-a", "1").expect("create VMM state");
        write_core_vmm_controller_state(directory.path(), "session-a", &state)
            .expect("write owner A document");

        let error = load_core_vmm_controller_state(directory.path(), "session-b")
            .expect_err("session mismatch must be rejected");

        assert!(error.contains("expected `session-b`"));
        assert!(error.contains("found `session-a`"));
    }

    /// Verifies malformed JSON is rejected without replacement, quarantine, or migration.
    /// 验证格式损坏的 JSON 会被拒绝，且不会被替换、隔离或迁移。
    #[test]
    fn corrupted_core_vmm_controller_document_is_rejected_unchanged() {
        // directory owns the isolated malformed controller document.
        // directory 拥有隔离的格式损坏控制器文档。
        let directory = temporary_session_dir();
        // malformed_bytes are intentionally not valid JSON.
        // malformed_bytes 被刻意设置为无效 JSON。
        let malformed_bytes = b"{\"schema_version\":2,\"controller_kind\":";
        fs::write(vmm_state_path(directory.path()), malformed_bytes)
            .expect("write malformed VMM document");

        let error = load_core_vmm_controller_state(directory.path(), "session-a")
            .expect_err("malformed document must be rejected");

        assert!(error.contains("Failed to parse Core VMM controller document"));
        assert_eq!(
            fs::read(vmm_state_path(directory.path())).expect("read unchanged malformed document"),
            malformed_bytes
        );
    }
}
