use super::*;

/// Current schema version for recoverable runtime/VMM checkpoint markers.
/// 可恢复运行态与 VMM 检查点标记的当前结构版本。
const VMM_CHECKPOINT_TRANSACTION_SCHEMA_VERSION: u32 = 1;

/// Complete target snapshot required to finish an interrupted runtime/VMM checkpoint.
/// 完成中断运行态与 VMM 检查点所需的完整目标快照。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct VmmCheckpointTransaction {
    /// Checkpoint marker schema version.
    /// 检查点标记结构版本。
    schema_version: u32,
    /// Exact session owning every target artifact.
    /// 拥有全部目标工件的精确会话。
    session_id: String,
    /// Complete target VMM state stored outside `state.json`.
    /// 保存在 `state.json` 之外的完整目标 VMM 状态。
    target_vmm_state: vulcan_vmm::VmmSessionMemoryState,
    /// Whether this transaction owns publication or conflict detection for `vmm.json`.
    /// 本事务是否负责发布 `vmm.json` 或检测其冲突。
    #[serde(default = "default_publish_vmm_state")]
    publish_vmm_state: bool,
    /// Complete target state document with VMM fields intentionally omitted.
    /// 明确省略 VMM 字段的完整目标状态文档。
    target_state_document: SegmentedSessionStateDocument,
    /// Runtime snapshot used to synchronize the Provider-facing runtime log.
    /// 用于同步 Provider 可见运行时日志的运行态快照。
    target_runtime_state: SessionRuntimeStateEntry,
    /// Unix-millis text captured before the marker is published.
    /// 发布标记前捕获的 Unix 毫秒文本。
    created_at: String,
}

/// Returns the compatibility intent for checkpoint markers created before the field existed.
/// 返回该字段出现前创建的检查点标记所使用的兼容发布意图。
///
/// # Returns
/// 返回值
/// `true` because historical markers coordinated VMM publication whenever the feature was enabled.
/// 返回 `true`，因为历史标记会在功能启用时协调 VMM 发布。
fn default_publish_vmm_state() -> bool {
    true
}

/// Returns the exact checkpoint-marker path below one segmented session directory.
/// 返回单个分段会话目录下的精确检查点标记路径。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved owning session directory.
/// `session_dir` 是已解析的所属会话目录。
///
/// # Returns
/// 返回值
/// The sibling `vmm.checkpoint.json` path.
/// 返回同级 `vmm.checkpoint.json` 路径。
fn vmm_checkpoint_transaction_path(session_dir: &std::path::Path) -> std::path::PathBuf {
    session_dir.join(SEGMENTED_VMM_CHECKPOINT_FILE)
}

/// Publishes one runtime-state, VMM-state, and runtime-log checkpoint behind a recoverable marker.
/// 在可恢复标记保护下发布运行态、VMM 状态与运行时日志检查点。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory receiving all artifacts.
/// `session_dir` 是接收全部工件的精确分段会话目录。
/// `session_id` is the canonical owner written into every document.
/// `session_id` 是写入每份文档的规范所有者。
/// `runtime_state` is the complete in-memory checkpoint candidate.
/// `runtime_state` 是完整的内存检查点候选。
///
/// # Returns
/// 返回值
/// Success only after every target artifact is durable and the marker is removed.
/// 仅当全部目标工件持久化且标记已删除后返回成功。
pub(in crate::workspace_store) fn persist_segmented_runtime_checkpoint_transactionally(
    session_dir: &std::path::Path,
    session_id: &str,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    // state_document is serialized before the marker so construction failures cannot expose a transaction.
    // state_document 在标记前完成序列化构造，因此构造失败不会暴露事务。
    let state_document = state_document_from_runtime_state(session_id, runtime_state)?;
    // publish_vmm_state freezes rollout intent so crash recovery never guesses from a later environment.
    // publish_vmm_state 冻结灰度发布意图，使崩溃恢复绝不根据后续环境进行猜测。
    let publish_vmm_state = vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
        || runtime_state.vmm_session_memory.requires_persistence();
    // transaction retains both documents and the log source needed for deterministic forward recovery.
    // transaction 保留确定性向前恢复所需的两份文档与日志来源。
    let transaction = VmmCheckpointTransaction {
        schema_version: VMM_CHECKPOINT_TRANSACTION_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        target_vmm_state: runtime_state.vmm_session_memory.clone(),
        publish_vmm_state,
        target_state_document: state_document,
        target_runtime_state: runtime_state.clone(),
        created_at: current_unix_millis_text(),
    };
    validate_vmm_checkpoint_transaction(&transaction, session_id)?;
    if let Err(error) =
        write_json_file_atomic(&vmm_checkpoint_transaction_path(session_dir), &transaction)
    {
        record_vmm_store_lifecycle_event_best_effort(
            session_dir,
            &transaction.created_at,
            vulcan_context::RuntimeVmmLifecycleAction::WriteFailed,
            Some(&transaction.target_vmm_state),
            "vmm_checkpoint_marker_write_failed",
        );
        return Err(error);
    }
    // failure_state and failure_timestamp remain available if forward publication fails.
    // failure_state 与 failure_timestamp 在向前发布失败时仍然可用于审计。
    let failure_state = transaction.target_vmm_state.clone();
    let failure_timestamp = transaction.created_at.clone();
    // failure_publish_vmm_state preserves the marker's exact ownership intent after transaction consumption.
    // failure_publish_vmm_state 在事务被消费后保留标记的精确归属意图。
    let failure_publish_vmm_state = transaction.publish_vmm_state;
    if let Err(error) = execute_vmm_checkpoint_transaction(session_dir, transaction) {
        if let Err(conflict_error) = discard_unrecoverable_vmm_checkpoint_marker(
            session_dir,
            session_id,
            &failure_state,
            failure_publish_vmm_state,
        ) {
            eprintln!(
                "[vulcan-workspace-store] failed to inspect rejected VMM checkpoint marker: {conflict_error}"
            );
        }
        record_vmm_store_lifecycle_event_best_effort(
            session_dir,
            &failure_timestamp,
            vulcan_context::RuntimeVmmLifecycleAction::WriteFailed,
            Some(&failure_state),
            "vmm_checkpoint_publication_failed",
        );
        return Err(error);
    }
    Ok(())
}

/// Completes an interrupted runtime/VMM checkpoint before session state is loaded.
/// 在加载会话状态前完成一份中断的运行态与 VMM 检查点。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory inspected for a marker.
/// `session_dir` 是检查标记的精确分段会话目录。
/// `expected_session_id` is the canonical directory owner.
/// `expected_session_id` 是该目录的规范所有者。
///
/// # Returns
/// 返回值
/// `true` after a marker is replayed, `false` for clean absence, or an explicit recovery error.
/// 重放标记后返回 `true`，标记干净缺失时返回 `false`，否则返回明确恢复错误。
pub(in crate::workspace_store) fn recover_segmented_runtime_checkpoint_transaction(
    session_dir: &std::path::Path,
    expected_session_id: &str,
) -> Result<bool, String> {
    // transaction is read before state hydration so recovered state becomes the only loaded snapshot.
    // transaction 在状态水合前读取，使恢复后的状态成为唯一加载快照。
    let Some(transaction) = read_optional_json::<VmmCheckpointTransaction>(
        &vmm_checkpoint_transaction_path(session_dir),
    )?
    else {
        return Ok(false);
    };
    validate_vmm_checkpoint_transaction(&transaction, expected_session_id)?;
    // recovered_vmm_state and recovery_timestamp survive transaction consumption for audit.
    // recovered_vmm_state 与 recovery_timestamp 在事务被消费后仍用于审计。
    let recovered_vmm_state = transaction.target_vmm_state.clone();
    let recovery_timestamp = transaction.created_at.clone();
    // recovered_publish_vmm_state is the persisted VMM ownership decision for this exact checkpoint.
    // recovered_publish_vmm_state 是该精确检查点持久化的 VMM 归属决策。
    let recovered_publish_vmm_state = transaction.publish_vmm_state;
    if discard_unrecoverable_vmm_checkpoint_marker(
        session_dir,
        expected_session_id,
        &recovered_vmm_state,
        recovered_publish_vmm_state,
    )? {
        record_vmm_store_lifecycle_event_best_effort(
            session_dir,
            &recovery_timestamp,
            vulcan_context::RuntimeVmmLifecycleAction::FileRecovered,
            Some(&recovered_vmm_state),
            "superseded_vmm_checkpoint_marker_discarded",
        );
        return Ok(true);
    }
    execute_vmm_checkpoint_transaction(session_dir, transaction)?;
    record_vmm_store_lifecycle_event_best_effort(
        session_dir,
        &recovery_timestamp,
        vulcan_context::RuntimeVmmLifecycleAction::FileRecovered,
        Some(&recovered_vmm_state),
        "interrupted_vmm_checkpoint_replayed_forward",
    );
    Ok(true)
}

/// Removes one checkpoint marker only when durable VMM state proves its target cannot commit.
/// 仅在持久化 VMM 状态证明目标无法提交时移除检查点标记。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory owning all artifacts.
/// `session_dir` 是拥有全部工件的精确分段会话目录。
/// `expected_session_id` is the canonical session owner.
/// `expected_session_id` 是规范会话所有者。
/// `target_vmm_state` is the rejected or pending checkpoint target.
/// `target_vmm_state` 是被拒绝或待处理的检查点目标。
/// `publish_vmm_state` is the persisted decision that this transaction owns `vmm.json`.
/// `publish_vmm_state` 是本事务拥有 `vmm.json` 的持久化决策。
///
/// # Returns
/// 返回值
/// `true` only after a newer or divergent equal durable revision is proven and the marker is removed.
/// 仅在证明持久状态版本更高或同版本分歧且标记已移除后返回 `true`。
fn discard_unrecoverable_vmm_checkpoint_marker(
    session_dir: &std::path::Path,
    expected_session_id: &str,
    target_vmm_state: &vulcan_vmm::VmmSessionMemoryState,
    publish_vmm_state: bool,
) -> Result<bool, String> {
    if !publish_vmm_state
        || !durable_vmm_state_conflicts_with_checkpoint(
            session_dir,
            expected_session_id,
            target_vmm_state,
        )?
    {
        return Ok(false);
    }
    match std::fs::remove_file(vmm_checkpoint_transaction_path(session_dir)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(to_store_error(error)),
    }
}

/// Validates one checkpoint marker before it can mutate durable session artifacts.
/// 在检查点标记修改持久化会话工件前执行校验。
///
/// # Parameters
/// 参数
/// `transaction` is the complete marker candidate.
/// `transaction` 是完整标记候选。
/// `expected_session_id` is the exact owning session identity.
/// `expected_session_id` 是精确的所属会话身份。
///
/// # Returns
/// 返回值
/// Success only for a supported schema and mutually consistent artifact ownership.
/// 仅当结构版本受支持且工件归属相互一致时返回成功。
fn validate_vmm_checkpoint_transaction(
    transaction: &VmmCheckpointTransaction,
    expected_session_id: &str,
) -> Result<(), String> {
    if transaction.schema_version != VMM_CHECKPOINT_TRANSACTION_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported VMM checkpoint transaction schema version {}.",
            transaction.schema_version
        ));
    }
    if transaction.session_id != expected_session_id {
        return Err(format!(
            "VMM checkpoint owner mismatch: expected `{expected_session_id}`, found `{}`.",
            transaction.session_id
        ));
    }
    if transaction.target_state_document.session_id != expected_session_id {
        return Err(format!(
            "VMM checkpoint state owner mismatch: expected `{expected_session_id}`, found `{}`.",
            transaction.target_state_document.session_id
        ));
    }
    // reconstructed_state proves the marker's state document and runtime-log source share one snapshot.
    // reconstructed_state 证明标记中的状态文档与运行时日志来源属于同一快照。
    let reconstructed_state =
        state_document_from_runtime_state(expected_session_id, &transaction.target_runtime_state)?;
    if reconstructed_state != transaction.target_state_document {
        return Err(String::from(
            "VMM checkpoint state document does not match its runtime-state source snapshot.",
        ));
    }
    if !transaction.publish_vmm_state && transaction.target_vmm_state.requires_persistence() {
        return Err(String::from(
            "VMM checkpoint without publication ownership contains a non-pristine VMM target.",
        ));
    }
    if transaction.target_vmm_state.requires_persistence() {
        transaction.target_vmm_state.validate(expected_session_id)
    } else {
        Ok(())
    }
}

/// Replays one validated checkpoint in deterministic forward order and removes its marker last.
/// 按确定性向前顺序重放一份已校验检查点，并在最后删除标记。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact directory receiving the target artifacts.
/// `session_dir` 是接收目标工件的精确目录。
/// `transaction` is the validated complete target snapshot.
/// `transaction` 是已校验的完整目标快照。
///
/// # Returns
/// 返回值
/// Success only after VMM state, state document, and runtime log all match the target.
/// 仅当 VMM 状态、状态文档与运行时日志全部匹配目标后返回成功。
fn execute_vmm_checkpoint_transaction(
    session_dir: &std::path::Path,
    transaction: VmmCheckpointTransaction,
) -> Result<(), String> {
    if transaction.publish_vmm_state {
        write_vmm_state(
            session_dir,
            transaction.session_id.as_str(),
            &transaction.target_vmm_state,
        )?;
    }
    write_state(session_dir, &transaction.target_state_document)?;
    // target_runtime_state receives the separately persisted VMM state because serde intentionally skips it.
    // target_runtime_state 接收独立持久化的 VMM 状态，因为 serde 会明确跳过该字段。
    let mut target_runtime_state = transaction.target_runtime_state;
    target_runtime_state.vmm_session_memory = transaction.target_vmm_state;
    sync_segmented_runtime_log_for_state(session_dir, &target_runtime_state)?;
    match std::fs::remove_file(vmm_checkpoint_transaction_path(session_dir)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(to_store_error(error)),
    }
}

/// Mirrors a runtime-state checkpoint into the segmented session state file without touching append-only JSONL segments.
/// 将运行时状态检查点镜像到分段会话状态文件，且不触碰追加式 JSONL 分段。
pub(super) async fn persist_segmented_session_runtime_state_checkpoint(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<(), String> {
    let session_id_text = session_id.to_string();
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
/// # 参数
/// `session_dir` is the segmented session directory that owns the runtime files.
/// `session_dir` 是拥有运行时文件的分段会话目录。
/// `runtime_state` is the canonical runtime state to mirror into provider-facing JSONL.
/// `runtime_state` 是要镜像到 Provider 可见 JSONL 的规范运行态。
///
/// # Returns
/// # 返回值
/// Returns `Ok(())` after the runtime log is synchronized.
/// 运行时日志同步完成后返回 `Ok(())`。
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

/// Persists one complete session snapshot into segmented transcript and runtime-state files.
/// 将一份完整会话快照持久化到分段转录与运行态文件。
///
/// # Parameters
/// # 参数
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
/// `runtime_state` optionally supplies the runtime checkpoint and provider-facing log state.
/// `runtime_state` 可选提供运行态检查点与 Provider 可见日志状态。
///
/// # Returns
/// # 返回值
/// Returns success only after transcript reconciliation and every supplied runtime artifact are durable.
/// 仅在转录校正与全部已提供运行态工件均完成耐久化后返回成功。
pub(super) async fn persist_segmented_session_update(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
    messages: &[SessionMessageEntry],
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Result<(), String> {
    // locator is the single filesystem identity derived from the authoritative session owner and lifecycle state.
    // locator 是从权威会话归属与生命周期状态派生的唯一文件系统身份。
    let locator = segmented_session_locator_from_summary(session, project_id, archived);
    // session_id_text is the canonical textual session identity stored in segmented metadata and state files.
    // session_id_text 是写入分段元数据与状态文件的规范会话文本身份。
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

    /// Verifies an ordinary runtime checkpoint succeeds before a session has ever acquired VMM state.
    /// 验证会话从未获得 VMM 状态时，普通运行态检查点仍可成功。
    #[test]
    fn pristine_vmm_checkpoint_persists_non_vmm_runtime_state() {
        // root owns the exact ordinary-session persistence fixture.
        // root 持有精确的普通会话持久化夹具。
        let root = std::env::temp_dir().join(format!(
            "vulcan-pristine-vmm-checkpoint-{}",
            uuid::Uuid::new_v4()
        ));
        // session_id is the canonical owner even though its VMM snapshot remains unowned and pristine.
        // session_id 是规范所有者，但其 VMM 快照仍保持无归属纯净状态。
        let session_id = "session-pristine-vmm";
        // session_dir supplies normal metadata required by runtime-log synchronization.
        // session_dir 提供运行时日志同步所需的正常元数据。
        let session_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id: session_id.to_string(),
                project_tag: String::from("checkpoint-project"),
                created_date: String::from("2026-07-20"),
                created_at: String::from("1"),
                project_id: Some(String::from("checkpoint-project")),
                title: Some(String::from("pristine VMM checkpoint")),
                title_source: Some(String::from("test")),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("segmented session should be created");
        // runtime_state contains ordinary metadata and the untouched default VMM snapshot.
        // runtime_state 包含普通元数据与未触碰的默认 VMM 快照。
        let runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("ordinary-provider")),
            updated_at: String::from("2"),
            ..SessionRuntimeStateEntry::default()
        };

        persist_segmented_runtime_checkpoint_transactionally(
            &session_dir,
            session_id,
            &runtime_state,
        )
        .expect("ordinary checkpoint should accept pristine VMM state");

        // durable_state proves non-VMM runtime metadata still reached state.json.
        // durable_state 证明非 VMM 运行态元数据仍已写入 state.json。
        let durable_state = read_state(&session_dir)
            .expect("durable state should read")
            .expect("durable state should exist");
        assert_eq!(
            durable_state.runtime_state["active_provider_id"],
            serde_json::Value::String(String::from("ordinary-provider"))
        );
        assert!(!session_dir.join("vmm.json").exists());
        assert!(!vmm_checkpoint_transaction_path(&session_dir).exists());
        std::fs::remove_dir_all(&root).expect("remove exact test directory");
    }

    /// Verifies a marker left after the VMM publish deterministically completes state and log publication.
    /// 验证在 VMM 发布后遗留的标记会确定性完成状态与日志发布。
    #[test]
    fn interrupted_vmm_checkpoint_recovers_forward_and_removes_marker() {
        // root is a unique, exact temporary directory owned only by this test.
        // root 是仅由本测试拥有的唯一精确临时目录。
        let root = std::env::temp_dir().join(format!(
            "vulcan-vmm-checkpoint-recovery-{}",
            uuid::Uuid::new_v4()
        ));
        // session_id is shared by every transaction artifact under test.
        // session_id 是本测试中全部事务工件共享的会话标识。
        let session_id = "session-checkpoint";
        // session_dir supplies the normal segmented metadata required by runtime-log synchronization.
        // session_dir 提供运行时日志同步所需的正常分段元数据。
        let session_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id: session_id.to_string(),
                project_tag: String::from("checkpoint-project"),
                created_date: String::from("2026-07-20"),
                created_at: String::from("1"),
                project_id: Some(String::from("checkpoint-project")),
                title: Some(String::from("checkpoint recovery")),
                title_source: Some(String::from("test")),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("segmented session should be created");
        // old_runtime_state proves state.json is advanced only by transaction recovery.
        // old_runtime_state 证明 state.json 仅由事务恢复向前推进。
        let old_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("old-provider")),
            updated_at: String::from("1"),
            ..SessionRuntimeStateEntry::default()
        };
        // old_state_document is the durable pre-transaction runtime metadata.
        // old_state_document 是事务前的持久化运行时元数据。
        let old_state_document = state_document_from_runtime_state(session_id, &old_runtime_state)
            .expect("old state document should serialize");
        write_state(&session_dir, &old_state_document).expect("old state should persist");

        // target_vmm_state owns one durable lease and therefore requires independent persistence.
        // target_vmm_state 拥有一条持久化租约，因此需要独立持久化。
        let mut target_vmm_state = vulcan_vmm::VmmSessionMemoryState::default();
        target_vmm_state
            .apply_recall_batch(
                session_id,
                "user-1",
                &vulcan_vmm::VmmRecallBatch {
                    ttl_turns: 2,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Recovered memory."),
                    items: vec![vulcan_vmm::ContextItem {
                        text: String::from("recover-me"),
                        score: 0.9,
                        turn_id: 1,
                        has_dialogue: true,
                        created_datetime: String::from("2026-07-20"),
                        memory_id: 7,
                    }],
                },
                "2",
            )
            .expect("target VMM lease should be valid");
        // target_runtime_state supplies the complete forward-recovery target.
        // target_runtime_state 提供完整的向前恢复目标。
        let target_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("new-provider")),
            vmm_session_memory: target_vmm_state.clone(),
            updated_at: String::from("2"),
            ..SessionRuntimeStateEntry::default()
        };
        // target_state_document mirrors the target runtime metadata without duplicating VMM state.
        // target_state_document 镜像目标运行时元数据且不重复保存 VMM 状态。
        let target_state_document =
            state_document_from_runtime_state(session_id, &target_runtime_state)
                .expect("target state document should serialize");
        // transaction is the exact durable recovery intent published before individual artifacts.
        // transaction 是在各个独立工件前发布的精确持久化恢复意图。
        let transaction = VmmCheckpointTransaction {
            schema_version: VMM_CHECKPOINT_TRANSACTION_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            target_vmm_state: target_vmm_state.clone(),
            publish_vmm_state: true,
            target_state_document,
            target_runtime_state,
            created_at: String::from("2"),
        };
        // marker_path remains present while the simulated process publishes only VMM state.
        // marker_path 在模拟进程仅发布 VMM 状态期间保持存在。
        let marker_path = vmm_checkpoint_transaction_path(&session_dir);
        write_json_file_atomic(&marker_path, &transaction)
            .expect("checkpoint marker should persist");
        write_vmm_state(&session_dir, session_id, &target_vmm_state)
            .expect("first transaction artifact should persist");

        assert!(
            recover_segmented_runtime_checkpoint_transaction(&session_dir, session_id)
                .expect("interrupted transaction should recover")
        );
        assert!(!marker_path.exists());
        // recovered_state proves the second transaction artifact reached the exact target.
        // recovered_state 证明第二个事务工件已达到精确目标。
        let recovered_state = read_state(&session_dir)
            .expect("recovered state should read")
            .expect("recovered state should exist");
        assert_eq!(
            recovered_state.runtime_state["active_provider_id"],
            serde_json::Value::String(String::from("new-provider"))
        );
        // recovered_vmm proves recovery accepted the idempotently pre-published VMM revision.
        // recovered_vmm 证明恢复接受了已经幂等发布的 VMM 版本。
        let recovered_vmm: vulcan_vmm::VmmSessionMemoryState =
            serde_json::from_slice(&std::fs::read(session_dir.join("vmm.json")).expect("read VMM"))
                .expect("parse VMM");
        assert_eq!(recovered_vmm, target_vmm_state);
        std::fs::remove_dir_all(&root).expect("remove exact test directory");
    }

    /// Verifies a checkpoint superseded by a newer durable VMM revision is discarded without rollback.
    /// 验证被更新持久化 VMM 版本取代的检查点会被丢弃且不会回滚状态。
    #[test]
    fn superseded_vmm_checkpoint_marker_is_discarded_without_state_rollback() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-vmm-checkpoint-superseded-{}",
            uuid::Uuid::new_v4()
        ));
        let session_id = "session-superseded";
        let session_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id: session_id.to_string(),
                project_tag: String::from("checkpoint-project"),
                created_date: String::from("2026-07-20"),
                created_at: String::from("1"),
                project_id: Some(String::from("checkpoint-project")),
                title: Some(String::from("superseded checkpoint")),
                title_source: Some(String::from("test")),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("segmented session should be created");
        let mut stale_vmm_state = vulcan_vmm::VmmSessionMemoryState::default();
        let batch = vulcan_vmm::VmmRecallBatch {
            ttl_turns: 2,
            injection_tag: String::from("VMM_MEMORY"),
            injection_description: String::from("Relevant memories."),
            items: vec![vulcan_vmm::ContextItem {
                text: String::from("stable-memory"),
                memory_id: 7,
                ..vulcan_vmm::ContextItem::default()
            }],
        };
        stale_vmm_state
            .apply_recall_batch(session_id, "user-1", &batch, "1")
            .expect("stale target should be valid");
        let mut durable_vmm_state = stale_vmm_state.clone();
        durable_vmm_state
            .apply_recall_batch(session_id, "user-2", &batch, "2")
            .expect("durable refresh should advance the revision");
        write_vmm_state(&session_dir, session_id, &durable_vmm_state)
            .expect("newer durable VMM state should persist");
        let durable_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("durable-provider")),
            vmm_session_memory: durable_vmm_state.clone(),
            updated_at: String::from("2"),
            ..SessionRuntimeStateEntry::default()
        };
        let durable_state_document =
            state_document_from_runtime_state(session_id, &durable_runtime_state)
                .expect("durable state document should serialize");
        write_state(&session_dir, &durable_state_document)
            .expect("durable state document should persist");
        let stale_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("stale-provider")),
            vmm_session_memory: stale_vmm_state.clone(),
            updated_at: String::from("1"),
            ..SessionRuntimeStateEntry::default()
        };
        let transaction = VmmCheckpointTransaction {
            schema_version: VMM_CHECKPOINT_TRANSACTION_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            target_vmm_state: stale_vmm_state,
            publish_vmm_state: true,
            target_state_document: state_document_from_runtime_state(
                session_id,
                &stale_runtime_state,
            )
            .expect("stale state document should serialize"),
            target_runtime_state: stale_runtime_state,
            created_at: String::from("3"),
        };
        let marker_path = vmm_checkpoint_transaction_path(&session_dir);
        write_json_file_atomic(&marker_path, &transaction)
            .expect("stale checkpoint marker should persist");

        assert!(
            recover_segmented_runtime_checkpoint_transaction(&session_dir, session_id)
                .expect("superseded checkpoint should be discarded")
        );
        assert!(!marker_path.exists());
        let recovered_state = read_state(&session_dir)
            .expect("durable state should read")
            .expect("durable state should exist");
        assert_eq!(
            recovered_state.runtime_state["active_provider_id"],
            serde_json::Value::String(String::from("durable-provider"))
        );
        let recovered_vmm: vulcan_vmm::VmmSessionMemoryState =
            serde_json::from_slice(&std::fs::read(session_dir.join("vmm.json")).expect("read VMM"))
                .expect("parse VMM");
        assert_eq!(recovered_vmm, durable_vmm_state);
        std::fs::remove_dir_all(&root).expect("remove exact test directory");
    }

    /// Verifies a pristine stale checkpoint cannot erase or poison a session with durable VMM state.
    /// 验证纯净陈旧检查点无法擦除或阻塞已经具有持久 VMM 状态的会话。
    #[test]
    fn pristine_checkpoint_superseded_by_durable_vmm_is_discarded() {
        // root owns one exact session fixture containing newer independent VMM state.
        // root 持有一份包含更新独立 VMM 状态的精确会话夹具。
        let root = std::env::temp_dir().join(format!(
            "vulcan-pristine-checkpoint-superseded-{}",
            uuid::Uuid::new_v4()
        ));
        // session_id is shared by durable artifacts and the stale checkpoint marker.
        // session_id 由持久化工件与陈旧检查点标记共享。
        let session_id = "session-pristine-superseded";
        // session_dir supplies normal segmented ownership metadata.
        // session_dir 提供正常的分段归属元数据。
        let session_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id: session_id.to_string(),
                project_tag: String::from("checkpoint-project"),
                created_date: String::from("2026-07-20"),
                created_at: String::from("1"),
                project_id: Some(String::from("checkpoint-project")),
                title: Some(String::from("pristine checkpoint superseded")),
                title_source: Some(String::from("test")),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("segmented session should be created");
        // durable_vmm_state represents a recall published after the stale ordinary snapshot was captured.
        // durable_vmm_state 表示在陈旧普通快照捕获后发布的一次召回。
        let mut durable_vmm_state = vulcan_vmm::VmmSessionMemoryState::default();
        durable_vmm_state
            .apply_recall_batch(
                session_id,
                "user-1",
                &vulcan_vmm::VmmRecallBatch {
                    ttl_turns: 2,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![vulcan_vmm::ContextItem {
                        text: String::from("durable-memory"),
                        memory_id: 17,
                        ..vulcan_vmm::ContextItem::default()
                    }],
                },
                "2",
            )
            .expect("durable VMM fixture should be valid");
        write_vmm_state(&session_dir, session_id, &durable_vmm_state)
            .expect("durable VMM fixture should persist");
        // durable_runtime_state is the non-VMM metadata that recovery must not roll back.
        // durable_runtime_state 是恢复不得回滚的非 VMM 元数据。
        let durable_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("durable-provider")),
            vmm_session_memory: durable_vmm_state.clone(),
            updated_at: String::from("2"),
            ..SessionRuntimeStateEntry::default()
        };
        // durable_state_document establishes the already-published runtime metadata baseline.
        // durable_state_document 建立已经发布的运行态元数据基线。
        let durable_state_document =
            state_document_from_runtime_state(session_id, &durable_runtime_state)
                .expect("durable state document should serialize");
        write_state(&session_dir, &durable_state_document)
            .expect("durable state document should persist");
        // stale_runtime_state predates all VMM ownership and must therefore lose to vmm.json.
        // stale_runtime_state 早于全部 VMM 归属，因此必须让位于 vmm.json。
        let stale_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("stale-provider")),
            updated_at: String::from("1"),
            ..SessionRuntimeStateEntry::default()
        };
        // transaction models an interrupted old whole-session writer with a pristine VMM target.
        // transaction 模拟一名以纯净 VMM 为目标且被中断的旧整会话写入者。
        let transaction = VmmCheckpointTransaction {
            schema_version: VMM_CHECKPOINT_TRANSACTION_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            target_vmm_state: vulcan_vmm::VmmSessionMemoryState::default(),
            publish_vmm_state: true,
            target_state_document: state_document_from_runtime_state(
                session_id,
                &stale_runtime_state,
            )
            .expect("stale state document should serialize"),
            target_runtime_state: stale_runtime_state,
            created_at: String::from("3"),
        };
        // marker_path is the only stale artifact recovery should remove.
        // marker_path 是恢复应删除的唯一陈旧工件。
        let marker_path = vmm_checkpoint_transaction_path(&session_dir);
        write_json_file_atomic(&marker_path, &transaction)
            .expect("pristine stale checkpoint marker should persist");

        assert!(
            recover_segmented_runtime_checkpoint_transaction(&session_dir, session_id)
                .expect("pristine superseded checkpoint should be discarded")
        );
        assert!(!marker_path.exists());
        // recovered_state proves stale non-VMM metadata never reached state.json.
        // recovered_state 证明陈旧非 VMM 元数据从未写入 state.json。
        let recovered_state = read_state(&session_dir)
            .expect("durable state should read")
            .expect("durable state should remain");
        assert_eq!(recovered_state, durable_state_document);
        // recovered_vmm proves the newer independent snapshot remains exact.
        // recovered_vmm 证明更新的独立快照保持精确不变。
        let recovered_vmm: vulcan_vmm::VmmSessionMemoryState =
            serde_json::from_slice(&std::fs::read(session_dir.join("vmm.json")).expect("read VMM"))
                .expect("parse VMM");
        assert_eq!(recovered_vmm, durable_vmm_state);
        std::fs::remove_dir_all(&root).expect("remove exact test directory");
    }

    /// Verifies a checkpoint created with VMM publication disabled still recovers non-VMM runtime state.
    /// 验证在 VMM 发布关闭时创建的检查点仍会恢复非 VMM 运行态。
    #[test]
    fn non_publishing_checkpoint_recovers_while_preserving_durable_vmm() {
        // root owns one exact rollback-mode recovery fixture.
        // root 持有一份精确的回滚模式恢复夹具。
        let root = std::env::temp_dir().join(format!(
            "vulcan-non-publishing-vmm-checkpoint-{}",
            uuid::Uuid::new_v4()
        ));
        // session_id is shared by the intentionally preserved VMM file and non-VMM checkpoint.
        // session_id 由刻意保留的 VMM 文件与非 VMM 检查点共享。
        let session_id = "session-non-publishing-checkpoint";
        // session_dir supplies the normal metadata required by runtime-log synchronization.
        // session_dir 提供运行时日志同步所需的正常元数据。
        let session_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id: session_id.to_string(),
                project_tag: String::from("checkpoint-project"),
                created_date: String::from("2026-07-20"),
                created_at: String::from("1"),
                project_id: Some(String::from("checkpoint-project")),
                title: Some(String::from("non-publishing checkpoint")),
                title_source: Some(String::from("test")),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("segmented session should be created");
        // durable_vmm_state models state retained on disk while the rollout switch is disabled.
        // durable_vmm_state 模拟灰度开关关闭期间保留在磁盘上的状态。
        let mut durable_vmm_state = vulcan_vmm::VmmSessionMemoryState::new(session_id, "1")
            .expect("valid durable VMM fixture");
        durable_vmm_state.revision = 1;
        write_vmm_state(&session_dir, session_id, &durable_vmm_state)
            .expect("retained VMM state should persist");
        // target_runtime_state contains the ordinary metadata that an interrupted checkpoint must recover.
        // target_runtime_state 包含中断检查点必须恢复的普通元数据。
        let target_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("updated-provider")),
            updated_at: String::from("2"),
            ..SessionRuntimeStateEntry::default()
        };
        // target_state_document is the exact non-VMM state publication target.
        // target_state_document 是精确的非 VMM 状态发布目标。
        let target_state_document =
            state_document_from_runtime_state(session_id, &target_runtime_state)
                .expect("target state document should serialize");
        // transaction freezes the disabled publication intent instead of consulting the recovery environment.
        // transaction 冻结禁用发布意图，而不是查询恢复时环境。
        let transaction = VmmCheckpointTransaction {
            schema_version: VMM_CHECKPOINT_TRANSACTION_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            target_vmm_state: vulcan_vmm::VmmSessionMemoryState::default(),
            publish_vmm_state: false,
            target_state_document: target_state_document.clone(),
            target_runtime_state,
            created_at: String::from("2"),
        };
        // marker_path simulates a crash immediately after the recovery intent became durable.
        // marker_path 模拟恢复意图刚持久化后立即崩溃。
        let marker_path = vmm_checkpoint_transaction_path(&session_dir);
        write_json_file_atomic(&marker_path, &transaction)
            .expect("non-publishing checkpoint marker should persist");

        assert!(
            recover_segmented_runtime_checkpoint_transaction(&session_dir, session_id)
                .expect("non-publishing checkpoint should replay forward")
        );
        assert!(!marker_path.exists());
        // recovered_state proves non-VMM runtime metadata was not discarded as a false VMM conflict.
        // recovered_state 证明非 VMM 运行态元数据未被误判为 VMM 冲突而丢弃。
        let recovered_state = read_state(&session_dir)
            .expect("recovered state should read")
            .expect("recovered state should exist");
        assert_eq!(recovered_state, target_state_document);
        // recovered_vmm proves the rollback-mode file remained byte-semantically unchanged.
        // recovered_vmm 证明回滚模式文件保持字节语义不变。
        let recovered_vmm: vulcan_vmm::VmmSessionMemoryState =
            serde_json::from_slice(&std::fs::read(session_dir.join("vmm.json")).expect("read VMM"))
                .expect("parse VMM");
        assert_eq!(recovered_vmm, durable_vmm_state);
        std::fs::remove_dir_all(&root).expect("remove exact test directory");
    }

    /// Verifies an internally inconsistent checkpoint marker is rejected before any durable artifact changes.
    /// 验证内部不一致的检查点标记会在任何持久化工件变化前被拒绝。
    #[test]
    fn mismatched_vmm_checkpoint_state_document_is_rejected_before_mutation() {
        // root is an exact isolated directory whose durable artifacts can be compared before and after recovery.
        // root 是一个精确隔离目录，可用于比较恢复前后的持久化工件。
        let root = std::env::temp_dir().join(format!(
            "vulcan-vmm-checkpoint-mismatch-{}",
            uuid::Uuid::new_v4()
        ));
        // session_id is the canonical owner shared by the directory and transaction.
        // session_id 是目录与事务共享的规范所有者。
        let session_id = "session-checkpoint-mismatch";
        // session_dir provides the normal segmented metadata and state paths.
        // session_dir 提供正常的分段元数据与状态路径。
        let session_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id: session_id.to_string(),
                project_tag: String::from("checkpoint-project"),
                created_date: String::from("2026-07-20"),
                created_at: String::from("1"),
                project_id: Some(String::from("checkpoint-project")),
                title: Some(String::from("checkpoint mismatch")),
                title_source: Some(String::from("test")),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("segmented session should be created");
        // durable_runtime_state is the baseline that invalid recovery must leave untouched.
        // durable_runtime_state 是无效恢复必须保持不变的持久基线。
        let durable_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("durable-provider")),
            updated_at: String::from("1"),
            ..SessionRuntimeStateEntry::default()
        };
        // durable_state_document is published before the invalid marker is introduced.
        // durable_state_document 在引入无效标记前完成发布。
        let durable_state_document =
            state_document_from_runtime_state(session_id, &durable_runtime_state)
                .expect("durable state document should serialize");
        write_state(&session_dir, &durable_state_document)
            .expect("durable baseline should persist");
        // target_vmm_state is valid independently so only cross-document consistency can reject the marker.
        // target_vmm_state 本身有效，确保只能由跨文档一致性拒绝标记。
        let target_vmm_state = vulcan_vmm::VmmSessionMemoryState::new(session_id, "2")
            .expect("valid target VMM fixture");
        // target_runtime_state is the purported source snapshot stored in the marker.
        // target_runtime_state 是标记中声明的来源快照。
        let target_runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("target-provider")),
            vmm_session_memory: target_vmm_state.clone(),
            updated_at: String::from("2"),
            ..SessionRuntimeStateEntry::default()
        };
        // mismatched_state_document starts from the source and is then deliberately corrupted at one typed field.
        // mismatched_state_document 从来源构造，随后在一个类型化字段上被刻意篡改。
        let mut mismatched_state_document =
            state_document_from_runtime_state(session_id, &target_runtime_state)
                .expect("target state document should serialize");
        mismatched_state_document.runtime_state["active_provider_id"] =
            serde_json::Value::String(String::from("tampered-provider"));
        // transaction carries mutually inconsistent target artifacts and must never be replayed.
        // transaction 携带相互不一致的目标工件，绝不能被重放。
        let transaction = VmmCheckpointTransaction {
            schema_version: VMM_CHECKPOINT_TRANSACTION_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            target_vmm_state,
            publish_vmm_state: true,
            target_state_document: mismatched_state_document,
            target_runtime_state,
            created_at: String::from("2"),
        };
        // marker_path remains as diagnostic evidence after validation rejects the transaction.
        // marker_path 在校验拒绝事务后仍作为诊断证据保留。
        let marker_path = vmm_checkpoint_transaction_path(&session_dir);
        write_json_file_atomic(&marker_path, &transaction)
            .expect("invalid checkpoint marker should be staged for recovery test");

        // error captures the explicit consistency failure without consuming its forensic marker.
        // error 捕获明确的一致性失败，同时不消耗其取证标记。
        let error = recover_segmented_runtime_checkpoint_transaction(&session_dir, session_id)
            .expect_err("mismatched checkpoint must be rejected");

        assert!(error.contains("does not match its runtime-state source snapshot"));
        assert!(marker_path.exists());
        assert!(!session_dir.join("vmm.json").exists());
        // recovered_state proves validation happened before state.json mutation.
        // recovered_state 证明校验发生在 state.json 修改之前。
        let recovered_state = read_state(&session_dir)
            .expect("durable state should read")
            .expect("durable state should remain");
        assert_eq!(recovered_state, durable_state_document);
        std::fs::remove_dir_all(&root).expect("remove exact test directory");
    }
}
