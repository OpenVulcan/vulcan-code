//! Context compaction draft helpers.
//! 上下文压缩 draft 辅助逻辑。

use super::*;
use crate::PersistedPromptOutputStyleSelection;
use crate::{apply_output_style_snapshot, resolve_output_style_snapshot};

/// Candidate compaction state prepared before it is atomically committed.
/// 原子提交前准备好的候选压缩状态。
pub(crate) struct WebuiContextCompactionDraft {
    /// Baseline that must still match before the draft may replace active state.
    /// 候选状态替换活跃状态前必须仍然匹配的基线。
    pub(crate) baseline: WebuiContextCompactionBaseline,
    /// Fully rewritten runtime state candidate.
    /// 已完整改写的运行时状态候选。
    pub(crate) runtime_state: SessionRuntimeStateEntry,
    /// Checkpoint that should be appended only after the runtime state is accepted.
    /// 仅在运行时状态被接受后追加的检查点。
    pub(crate) checkpoint: vulcan_context::SessionCompactionEntry,
    /// Optional runtime audit event produced when persistent injections are folded.
    /// 持久注入被折叠时产出的可选运行时审计事件。
    pub(crate) injection_merge_event: Option<vulcan_context::RuntimeInjectionMergedEvent>,
    /// Timestamp used for the optional injection merge audit event.
    /// 可选注入合并审计事件使用的时间戳。
    pub(crate) injection_merge_timestamp: String,
    /// Report describing expired transient injection pruning.
    /// 描述过期临时注入裁剪情况的报告。
    pub(crate) ttl_prune_report: InjectionPruneReport,
}

/// Appends the visible start boundary for one WebUI context-compaction section.
/// 为 WebUI 上下文压缩分段追加可见开始边界。
pub(crate) fn append_webui_context_compaction_status_message_to_workspace(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    budget_note: &str,
    trigger: &str,
) -> Result<String, String> {
    let now = current_unix_millis_text();
    let mut runtime_state = workspace
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .unwrap_or_else(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.running"),
        body: String::new(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_state": "running",
            "context_compaction_trigger": trigger,
            "context_compaction_budget_note": budget_note,
        })),
        created_at: String::new(),
    };
    let message_id =
        append_webui_session_message_entry_to_state(&mut runtime_state, session_id, entry)?;
    set_webui_context_compaction_section_metadata_in_state(
        &mut runtime_state,
        &message_id,
        &message_id,
        "start",
        "running",
        Some(budget_note),
        trigger,
    );
    let runtime_state_snapshot = runtime_state.clone();
    workspace
        .session_runtime_state
        .insert(session_id, runtime_state);
    merge_runtime_state_records_into_display_messages(
        workspace,
        session_id,
        &runtime_state_snapshot,
    );
    sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    Ok(message_id)
}

/// Appends model-produced summary output into the visible compaction body row.
/// 将模型生成的总结输出追加到可见压缩正文行。
pub(crate) fn append_webui_context_compaction_output_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    output_message_id: &mut Option<String>,
    delta: &str,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        if output_message_id.is_none() {
            let entry = SessionMessageEntry {
                id: String::new(),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionStatus,
                visible: true,
                title: String::from("context_compaction.summary"),
                body: String::new(),
                metadata: Some(json!({
                    "context_compaction_status": true,
                    "context_compaction_section_id": section_id,
                    "context_compaction_role": "body",
                    "context_compaction_state": "running",
                    "context_compaction_trigger": trigger,
                    "context_compaction_budget_note": budget_note,
                })),
                created_at: String::new(),
            };
            *output_message_id = Some(append_webui_session_message_entry_to_state(
                runtime_state,
                session_id,
                entry,
            )?);
        }
        let message_id = output_message_id
            .as_deref()
            .ok_or_else(|| String::from("context compaction output message id is missing"))?;
        append_record_body_delta(&mut runtime_state.conversation_state, message_id, delta);
        runtime_state.updated_at = now;
        let runtime_state_snapshot = runtime_state.clone();
        merge_runtime_state_records_into_display_messages(
            workspace,
            session_id,
            &runtime_state_snapshot,
        );
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Requesting);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    }
    emit_context_compaction_message_window_delta(app, state, session_id)
}

/// Appends one visible progress line to the running context-compaction status section.
/// 向正在运行的上下文压缩状态分段追加一条可见进度行。
pub(crate) fn append_webui_context_compaction_progress(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    progress: &PromptCompactionProgressEvent,
) -> Result<(), String> {
    let progress_stage = progress.stage.trim();
    if progress_stage.is_empty() {
        return Ok(());
    }
    let line = progress_stage.to_string();
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        let existing_progress =
            runtime_state
                .conversation_state
                .records
                .iter_mut()
                .find(|record| {
                    record
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("context_compaction_section_id"))
                        .and_then(Value::as_str)
                        == Some(section_id)
                        && record
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.get("context_compaction_role"))
                            .and_then(Value::as_str)
                            == Some("progress")
                });
        match existing_progress {
            Some(record) => {
                if !record.body.lines().any(|existing| existing == line) {
                    if !record.body.trim().is_empty() {
                        record.body.push('\n');
                    }
                    record.body.push_str(&line);
                }
                if let Some(metadata) = record.metadata.as_mut().and_then(Value::as_object_mut) {
                    metadata.insert(
                        String::from("context_compaction_progress_stage"),
                        json!(progress.stage),
                    );
                    metadata.insert(String::from("context_compaction_state"), json!("running"));
                }
            }
            None => {
                let entry = SessionMessageEntry {
                    id: String::new(),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::CompactionStatus,
                    visible: true,
                    title: String::from("context_compaction.progress"),
                    body: line,
                    metadata: Some(json!({
                        "context_compaction_status": true,
                        "context_compaction_section_id": section_id,
                        "context_compaction_role": "progress",
                        "context_compaction_state": "running",
                        "context_compaction_trigger": trigger,
                        "context_compaction_budget_note": budget_note,
                        "context_compaction_progress_stage": progress.stage,
                    })),
                    created_at: String::new(),
                };
                let message_id =
                    append_webui_session_message_entry_to_state(runtime_state, session_id, entry)?;
                set_webui_context_compaction_section_metadata_in_state(
                    runtime_state,
                    &message_id,
                    section_id,
                    "progress",
                    "running",
                    Some(budget_note),
                    trigger,
                );
            }
        }
        runtime_state.updated_at = now;
        let runtime_state_snapshot = runtime_state.clone();
        merge_runtime_state_records_into_display_messages(
            workspace,
            session_id,
            &runtime_state_snapshot,
        );
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Requesting);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    }
    emit_context_compaction_message_window_delta(app, state, session_id)
}

/// Maximum user-message characters embedded into the automatic compaction continuation note.
/// 自动压缩续接提示中嵌入的用户消息最大字符数。
const AUTO_COMPACTION_CONTINUATION_LAST_USER_MAX_CHARS: usize = 4_000;

/// Stable prefix for model-visible automatic compaction continuation notes.
/// 模型可见自动压缩续接提示的稳定前缀。
pub(crate) const AUTO_COMPACTION_CONTINUATION_PREFIX: &str =
    "Automatic context compaction was applied while the current task was in progress.";

/// Appends one model-visible continuation note after automatic context compaction.
/// 在自动上下文压缩后追加一条模型可见的续接提示。
///
/// # Returns
/// 返回值
/// Returns the exact synthetic user-message id after append, `None` when no real user exists, or an identity failure.
/// 追加后返回精确合成用户消息标识；不存在真实用户时返回 `None`，身份异常时返回失败。
pub(crate) fn append_auto_context_compaction_continuation_message(
    draft_runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    created_at: &str,
    active_runtime_state: &SessionRuntimeStateEntry,
) -> Result<Option<String>, String> {
    let Some(last_user_message) =
        last_runtime_user_message_text(&active_runtime_state.conversation_state)
    else {
        return Ok(None);
    };
    let last_user_message = truncate_auto_compaction_last_user_message(&last_user_message);
    let message_id = build_session_record_message_id(
        session_id,
        created_at,
        draft_runtime_state.conversation_state.messages.len(),
    );
    let text = format!(
        "{AUTO_COMPACTION_CONTINUATION_PREFIX}\n\nThis is a synthetic continuation prompt for the resumed provider request, not a new user request. Continue the interrupted work from the retained context immediately. Do not greet, recap the compaction, or ask the user to restate the task. The last real user message before compaction was:\n<last_user_message>\n{last_user_message}\n</last_user_message>"
    );
    vulcan_context::append_user_text_message(
        &mut draft_runtime_state.conversation_state,
        message_id.clone(),
        text,
        Vec::new(),
    )
    .map_err(|error| format!("Invalid automatic continuation message identity: {error}"))?;
    Ok(Some(message_id))
}

/// Returns the latest real user-text payload from one runtime conversation state.
/// 返回运行时会话状态中的最后一条真实用户文本载荷。
pub(crate) fn last_runtime_user_message_text(state: &SessionConversationState) -> Option<String> {
    state.messages.iter().rev().find_map(|message| {
        let SessionConversationPayload::UserText { text, .. } = &message.payload else {
            return None;
        };
        let text = text.trim();
        (!text.is_empty() && !text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX))
            .then(|| text.to_string())
    })
}

/// Truncates the embedded last-user message while preserving its leading content.
/// 截断嵌入的最后用户消息，同时保留其开头内容。
pub(crate) fn truncate_auto_compaction_last_user_message(message: &str) -> String {
    let char_count = message.chars().count();
    if char_count <= AUTO_COMPACTION_CONTINUATION_LAST_USER_MAX_CHARS {
        return message.to_string();
    }
    let preserved = message
        .chars()
        .take(AUTO_COMPACTION_CONTINUATION_LAST_USER_MAX_CHARS)
        .collect::<String>();
    format!(
        "{preserved}\n[Last user message truncated after {AUTO_COMPACTION_CONTINUATION_LAST_USER_MAX_CHARS} chars for automatic context-compaction continuation.]"
    )
}

/// Builds one validated WebUI compaction draft without mutating the active runtime state.
/// 在不修改活跃运行时状态的情况下构建一份已校验的 WebUI 压缩候选状态。
///
/// # Parameters
/// # 参数
/// `active_runtime_state` is the current session runtime state used as the immutable draft source.
/// `active_runtime_state` 是作为不可变候选来源使用的当前会话运行时状态。
/// `vmm_session_memory` is the isolated Core controller candidate for the new cache epoch.
/// `vmm_session_memory` 是新缓存纪元使用的隔离 Core 控制器候选状态。
/// `session_id` identifies the session that will receive the draft if commit validation succeeds.
/// `session_id` 标识在提交校验通过后接收候选状态的会话。
/// `section_id` identifies the visible compaction status section being completed.
/// `section_id` 标识正在完成的可见压缩状态分段。
/// `budget_note` is the human-readable context budget note attached to status records.
/// `budget_note` 是附加到状态记录的人类可读上下文预算说明。
/// `provider_request_count` is the number of provider requests used to produce the summary.
/// `provider_request_count` 是生成摘要时使用的 Provider 请求数量。
/// `summary_body` is the durable summary body already extracted from the model response.
/// `summary_body` 是已从模型响应中提取出的持久摘要正文。
/// `execution` is the terminal prompt-compaction execution result carrying checkpoint metadata.
/// `execution` 是携带检查点元数据的终态提示词压缩执行结果。
///
/// # Returns
/// # 返回值
/// A complete draft that can replace active state atomically, or an error explaining why no commit should occur.
/// 返回可原子替换活跃状态的完整候选状态；或返回说明不应提交的错误。
pub(crate) fn build_webui_context_compaction_draft(
    active_runtime_state: &SessionRuntimeStateEntry,
    vmm_session_memory: &mut vulcan_vmm::VmmSessionMemoryState,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    provider_request_count: usize,
    summary_body: &str,
    execution: &PromptCompactionExecution,
    global_output_style_selection: PersistedPromptOutputStyleSelection,
) -> Result<WebuiContextCompactionDraft, String> {
    if summary_body.trim().is_empty() {
        return Err(String::from("压缩摘要为空，已拒绝提交候选状态。"));
    }
    let baseline = WebuiContextCompactionBaseline::capture(active_runtime_state);
    let mut draft_runtime_state = active_runtime_state.clone();
    let now = current_unix_millis_text();
    ensure_webui_context_compaction_summary_body_for_state(
        &mut draft_runtime_state,
        session_id,
        section_id,
        "completed",
        budget_note,
        trigger,
        summary_body,
    )?;
    let summary_entry = SessionMessageEntry {
        id: String::new(),
        kind: execution.compaction_result.summary_record.kind,
        record_kind: execution.compaction_result.summary_record.record_kind,
        visible: false,
        title: execution.compaction_result.summary_record.title.clone(),
        body: summary_body.to_string(),
        metadata: None,
        created_at: String::new(),
    };
    let summary_message_id = append_webui_session_message_entry_to_state(
        &mut draft_runtime_state,
        session_id,
        summary_entry,
    )?;
    let mut checkpoint = execution
        .compaction_result
        .materialize_checkpoint(summary_message_id.clone());
    checkpoint.context_budget_fingerprint =
        draft_runtime_state.live_context_budget_fingerprint.clone();
    checkpoint.context_assembly_fingerprint = draft_runtime_state
        .live_context_assembly_fingerprint
        .clone();
    draft_runtime_state.last_compaction_id = Some(checkpoint.id.clone());
    let output_style_snapshot = resolve_output_style_snapshot(
        draft_runtime_state
            .output_style
            .override_style_id
            .as_deref(),
        global_output_style_selection,
    )?;
    apply_output_style_snapshot(
        &mut draft_runtime_state,
        &output_style_snapshot,
        "context-compaction",
        Some(checkpoint.id.clone()),
        now.clone(),
    );
    draft_runtime_state.context_start_message_id = checkpoint.context_start_message_id.clone();
    draft_runtime_state.anchor_context_budget_fingerprint =
        checkpoint.context_budget_fingerprint.clone();
    draft_runtime_state.anchor_context_assembly_fingerprint =
        checkpoint.context_assembly_fingerprint.clone();

    let previous_persistent_fingerprint =
        persistent_injection_fingerprint(&draft_runtime_state.context_injection_state);
    let (mut pruned_injection_state, ttl_prune_report) = draft_runtime_state
        .context_injection_state
        .clone_pruned_for_compaction();
    pruned_injection_state.apply_compaction();
    let merged_baseline_fingerprint = persistent_injection_fingerprint(&pruned_injection_state);
    let source_node_ids = persistent_injection_source_node_ids(&pruned_injection_state);
    draft_runtime_state.context_injection_state = pruned_injection_state;
    let injection_merge_event = if previous_persistent_fingerprint.is_some()
        || merged_baseline_fingerprint.is_some()
        || !source_node_ids.is_empty()
    {
        Some(vulcan_context::RuntimeInjectionMergedEvent {
            turn_id: None,
            merge_id: format!("injection-merge-{session_id}-{}", checkpoint.id),
            compaction_id: Some(checkpoint.id.clone()),
            previous_persistent_fingerprint,
            merged_baseline_fingerprint,
            source_node_ids,
        })
    } else {
        None
    };

    compact_conversation_with_summary_until(
        &mut draft_runtime_state.conversation_state,
        summary_message_id.clone(),
        execution.compaction_result.summary_record.kind,
        summary_body.to_string(),
        checkpoint.created_at.clone(),
        execution
            .compaction_result
            .source_until_message_id
            .as_deref(),
        execution.compaction_result.policy,
    )
    .map_err(|error| format!("Invalid compacted conversation message identity: {error}"))?;
    let continuation_message_id = if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
        append_auto_context_compaction_continuation_message(
            &mut draft_runtime_state,
            session_id,
            &checkpoint.created_at,
            active_runtime_state,
        )?
    } else {
        None
    };
    vmm_session_memory.begin_cache_epoch(
        &checkpoint.id,
        continuation_message_id.as_deref(),
        &now,
    )?;
    compact_conversation_records_after_summary(
        &mut draft_runtime_state.conversation_state,
        &summary_message_id,
        execution.compaction_result.policy,
    );
    set_webui_context_compaction_section_metadata_in_state(
        &mut draft_runtime_state,
        section_id,
        section_id,
        "start",
        "completed",
        Some(budget_note),
        trigger,
    );
    update_webui_context_compaction_body_state(&mut draft_runtime_state, section_id, "completed");
    let terminal_record_id = append_webui_context_compaction_end_message_to_state(
        &mut draft_runtime_state,
        session_id,
        section_id,
        budget_note,
        trigger,
        provider_request_count,
        execution.input_prune_report.as_ref(),
    )?;
    if trigger != AUTO_CONTEXT_COMPACTION_TRIGGER {
        // usage_segment retains the exact manual compaction execution cost inside the candidate state.
        // usage_segment 在候选状态内部保留手动压缩执行的精确成本。
        let usage_segment = context_compaction_usage_segment_from_execution(trigger, execution);
        append_webui_session_turn_stats_to_state(
            &mut draft_runtime_state,
            session_id,
            &terminal_record_id,
            SessionTurnStatsStatus::Completed,
            None,
            SessionTurnStatsKind::ContextCompaction,
            vec![usage_segment],
        )
        .map_err(|error| format!("无法在上下文压缩候选状态中追加轮次统计：{error}"))?
        .ok_or_else(|| String::from("上下文压缩候选状态未生成轮次统计，已拒绝提交。"))?;
    }
    if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
        draft_runtime_state.auto_compaction_consecutive_failures = 0;
    }
    draft_runtime_state.updated_at = now.clone();
    Ok(WebuiContextCompactionDraft {
        baseline,
        runtime_state: draft_runtime_state,
        checkpoint,
        injection_merge_event,
        injection_merge_timestamp: now,
        ttl_prune_report,
    })
}
