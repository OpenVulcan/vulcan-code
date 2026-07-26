use super::*;

/// Historical marker name previously used to couple ordinary runtime checkpoints with VMM state.
/// 历史上用于耦合普通运行时检查点与 VMM 状态的标记文件名。
const HISTORICAL_VMM_CHECKPOINT_FILE: &str = "vmm.checkpoint.json";

/// Persists one ordinary runtime checkpoint without reading or writing VMM state.
/// 持久化一个普通运行时检查点，且不读取或写入 VMM 状态。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory receiving ordinary runtime artifacts.
/// `session_dir` 是接收普通运行时工件的精确分段会话目录。
/// `session_id` is the canonical owner written into `state.json`.
/// `session_id` 是写入 `state.json` 的规范所有者。
/// `runtime_state` is the ordinary runtime state without Core VMM ownership.
/// `runtime_state` 是不含 Core VMM 所有权的普通运行时状态。
///
/// # Returns
/// 返回值
/// Success after `state.json` and the provider-facing runtime log are synchronized.
/// 在 `state.json` 与面向 Provider 的运行时日志同步后返回成功。
pub(in crate::workspace_store) fn persist_segmented_runtime_checkpoint_transactionally(
    session_dir: &std::path::Path,
    session_id: &str,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    // state_document is derived only from ordinary runtime state.
    // state_document 仅从普通运行时状态派生。
    let state_document = state_document_from_runtime_state(session_id, runtime_state)?;
    write_state(session_dir, &state_document)?;
    sync_segmented_runtime_log_for_state(session_dir, runtime_state)
}

/// Rejects obsolete runtime/VMM checkpoint markers instead of replaying historical state.
/// 拒绝旧版运行时与 VMM 检查点标记，而不是重放历史状态。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory inspected during load.
/// `session_dir` 是加载期间检查的精确分段会话目录。
/// `expected_session_id` identifies the session used in the rejection diagnostic.
/// `expected_session_id` 标识拒绝诊断中使用的会话。
///
/// # Returns
/// 返回值
/// Success for clean absence, otherwise an explicit unsupported-history error.
/// 干净缺失时返回成功，否则返回明确的不支持历史数据错误。
pub(in crate::workspace_store) fn reject_historical_segmented_runtime_checkpoint(
    session_dir: &std::path::Path,
    expected_session_id: &str,
) -> Result<(), String> {
    // marker_path is retained only for strict detection of the removed persistence contract.
    // marker_path 仅用于严格检测已经移除的持久化契约。
    let marker_path = session_dir.join(HISTORICAL_VMM_CHECKPOINT_FILE);
    if marker_path.try_exists().map_err(to_store_error)? {
        return Err(format!(
            "Session `{expected_session_id}` contains an unsupported historical VMM checkpoint marker `{}`; automatic replay and migration are disabled.",
            marker_path.display()
        ));
    }
    Ok(())
}

/// Mirrors a runtime-state checkpoint into the segmented session state file without touching append-only JSONL segments.
/// 将运行时状态检查点镜像到分段会话状态文件，且不触碰追加式 JSONL 分段。
pub(super) async fn persist_segmented_session_runtime_state_checkpoint(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    // session_id_text is the canonical persisted session identity.
    // session_id_text 是规范的持久化会话身份。
    let session_id_text = session_id.to_string();
    // session_dir resolves the only existing active or archived session directory.
    // session_dir 解析唯一存在的活动或归档会话目录。
    let Some(session_dir) =
        find_segmented_session_directory_by_id(&controller.data_root, &session_id_text)?
    else {
        return Ok(());
    };
    persist_segmented_runtime_checkpoint_transactionally(
        &session_dir,
        &session_id_text,
        runtime_state,
    )
}

/// Syncs one segmented runtime log using the active compaction-aware persistence policy.
/// 使用当前压缩感知持久化策略同步单个分段运行时日志。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory that owns the runtime files.
/// `session_dir` 是拥有运行时文件的分段会话目录。
/// `runtime_state` is the canonical runtime state to mirror into provider-facing JSONL.
/// `runtime_state` 是要镜像到 Provider 可见 JSONL 的规范运行态。
///
/// # Returns
/// 返回值
/// Success after the runtime log is synchronized.
/// 运行时日志同步完成后返回成功。
pub(in crate::workspace_store) fn sync_segmented_runtime_log_for_state(
    session_dir: &std::path::Path,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    if let Some(compaction_id) = runtime_state.last_compaction_id.as_deref() {
        sync_runtime_log_from_compacted_conversation_state(
            session_dir,
            &runtime_state.conversation_state,
            &runtime_state.updated_at,
            compaction_id,
        )?;
    } else {
        sync_runtime_log_from_conversation_state(
            session_dir,
            &runtime_state.conversation_state,
            &runtime_state.updated_at,
        )?;
    }
    Ok(())
}

/// Persists one complete session snapshot into segmented transcript and ordinary runtime files.
/// 将一份完整会话快照持久化到分段转录与普通运行时文件。
///
/// # Parameters
/// 参数
/// `controller` owns the segmented data root used for session directory resolution.
/// `controller` 持有用于解析会话目录的分段数据根。
/// `session` supplies the authoritative session identity and timestamps.
/// `session` 提供权威会话身份与时间戳。
/// `project_id` identifies the session owner used by the filesystem locator.
/// `project_id` 标识文件系统定位器使用的会话归属项目。
/// `archived` selects the active or archived segmented directory tree.
/// `archived` 选择活动或归档分段目录树。
/// `messages` is the complete transcript snapshot to append or physically reconcile.
/// `messages` 是需要追加或物理校正的完整转录快照。
/// `runtime_state` optionally supplies the ordinary runtime checkpoint.
/// `runtime_state` 可选提供普通运行时检查点。
///
/// # Returns
/// 返回值
/// Success only after transcript reconciliation and every supplied runtime artifact are durable.
/// 仅在转录校正与全部已提供运行时工件均完成持久化后返回成功。
pub(super) async fn persist_segmented_session_update(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
    messages: &[SessionMessageEntry],
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Result<(), String> {
    // locator is the single filesystem identity derived from session ownership and lifecycle state.
    // locator 是从会话归属与生命周期状态派生的唯一文件系统身份。
    let locator = segmented_session_locator_from_summary(session, project_id, archived);
    // session_id_text is the canonical textual session identity.
    // session_id_text 是规范的会话文本身份。
    let session_id_text = session.id.to_string();
    // session_dir is created or resolved before transcript reconciliation begins.
    // session_dir 会在转录校正开始前完成创建或解析。
    let session_dir =
        ensure_segmented_session_directory_for_locator(controller, &session_id_text, &locator)?;
    append_segmented_session_message_update(&session_dir, messages)?;
    if let Some(runtime_state) = runtime_state {
        persist_segmented_runtime_checkpoint_transactionally(
            &session_dir,
            &session_id_text,
            runtime_state,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies ordinary runtime persistence cannot rewrite or remove the Core VMM document.
    /// 验证普通运行时持久化不能改写或移除 Core VMM 文档。
    #[test]
    fn ordinary_runtime_checkpoint_does_not_touch_vmm_document() {
        // directory owns the isolated ordinary and VMM persistence files.
        // directory 拥有隔离的普通持久化文件与 VMM 持久化文件。
        let directory = tempfile::tempdir().expect("create runtime checkpoint directory");
        // vmm_path is the Core-owned file that ordinary persistence must leave byte-identical.
        // vmm_path 是普通持久化必须保持逐字节不变的 Core 所有文件。
        let vmm_path = directory.path().join("vmm.json");
        // vmm_bytes represent an opaque controller document from the ordinary writer's perspective.
        // vmm_bytes 表示普通写入器视角下不透明的控制器文档。
        let vmm_bytes = br#"{"controller_kind":"core_session_vmm_controller","opaque":true}"#;
        std::fs::write(&vmm_path, vmm_bytes).expect("seed opaque VMM document");
        // runtime_state is the minimal ordinary checkpoint payload.
        // runtime_state 是最小普通检查点载荷。
        let runtime_state = SessionRuntimeStateEntry::default();

        persist_segmented_runtime_checkpoint_transactionally(
            directory.path(),
            "session-a",
            &runtime_state,
        )
        .expect("persist ordinary runtime checkpoint");

        assert_eq!(
            std::fs::read(&vmm_path).expect("read VMM document after ordinary persistence"),
            vmm_bytes
        );
    }

    /// Verifies ordinary runtime loading rejects the removed coupled VMM checkpoint contract.
    /// 验证普通运行态加载会拒绝已经移除的 VMM 耦合检查点契约。
    #[test]
    fn historical_runtime_vmm_checkpoint_is_rejected_unchanged() {
        // directory owns the isolated unsupported marker.
        // directory 拥有隔离的不支持标记。
        let directory = tempfile::tempdir().expect("create historical checkpoint directory");
        // marker_path is the exact historical artifact that must remain untouched for diagnosis.
        // marker_path 是必须保持不变以供诊断的精确历史工件。
        let marker_path = directory.path().join(HISTORICAL_VMM_CHECKPOINT_FILE);
        // marker_bytes are opaque because the removed format must never be parsed or replayed.
        // marker_bytes 是不透明内容，因为已移除格式绝不能被解析或重放。
        let marker_bytes = b"{\"schema_version\":1}\n";
        std::fs::write(&marker_path, marker_bytes).expect("write historical checkpoint marker");

        // error proves the ordinary loader uses fail-stop rejection rather than recovery.
        // error 证明普通加载器采用故障停止拒绝，而不是恢复。
        let error = reject_historical_segmented_runtime_checkpoint(directory.path(), "session-a")
            .expect_err("historical checkpoint must be rejected");

        assert!(error.contains("unsupported historical VMM checkpoint marker"));
        assert_eq!(
            std::fs::read(&marker_path).expect("read unchanged historical checkpoint"),
            marker_bytes
        );
    }
}
