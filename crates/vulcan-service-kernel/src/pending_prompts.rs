//! Pending prompt queue service adapters for the WebUI bridge.
//! WebUI 桥接层待发送提示词队列服务适配器。

use std::collections::BTreeMap;

use vulcan_agent_runtime::{
    attachment_asset_index::{acquire_staged_attachment_ref, release_staged_attachment_ref},
    provider_config::{ProviderConfigurationFile, ReasoningEffortLevel},
};
use vulcan_core::types::ConversationAttachment;
use vulcan_service_api::{
    PendingPromptSummary, PendingPromptsChangedDelta, ServiceEvent, ServiceEventEmitter,
    WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_session_core::v1::config::QueuedInput;
use vulcan_workbench_model::{
    PENDING_PROMPT_QUEUE_LIMIT, PendingPromptBusyLaneContext, PendingPromptDispatchContext,
    PendingPromptModelReference, PendingPromptPanelEntry, PendingPromptPanelEntryState,
    PendingPromptRuntimeOptions, PendingSessionPromptSource, SessionId, SessionRuntimeStateEntry,
    SessionTranscriptUiState, pending_prompt_lane_is_busy,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use super::{
    AgentRuntimeBridge, PendingPromptProcessMirror, ServiceBridgeState, SharedServiceBridgeState,
    active_reasoning_selection_effort, composer_attachment_summary_from_core,
    current_unix_millis_text, emit_service_event_with_trace, owning_project_id_for_session,
    parse_reasoning_effort, public_ai_workbench_selection,
    replace_conversation_scoped_projection_entry, session_has_pending_plan_artifact,
    session_process_public_ai_selection,
};

/// Queued-prompt Hook facts dispatched only after the authoritative workspace commit.
/// 仅在权威工作区提交后分发的排队提示词 Hook 事实。
#[derive(Debug, Clone)]
pub(super) struct QueuedPendingPromptHookEvent {
    /// Session whose logical turn receives the queued timeline item.
    /// 逻辑回合接收排队时间线条目的会话。
    session_id: SessionId,
    /// Stable queued prompt identity used for timeline deduplication.
    /// 用于时间线去重的稳定排队提示词标识。
    prompt_entry_id: String,
    /// Authoritative project ownership captured with the workspace draft.
    /// 随工作区草稿捕获的权威项目归属。
    project_id: Option<String>,
    /// Authoritative workspace path included in the Hook payload.
    /// 包含在 Hook 载荷中的权威工作区路径。
    workspace_path: String,
    /// Exact queue source controlling the user-event kind.
    /// 控制用户事件类型的精确队列来源。
    source: PendingSessionPromptSource,
    /// User-visible queued prompt content.
    /// 用户可见的排队提示词内容。
    prompt: String,
}

/// Records one queued prompt in the conversation Hook runtime after workspace commit.
/// 在工作区提交后把单条排队提示词记录到会话 Hook 运行时。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns the Hook runtime and `event` contains committed queue facts.
/// `agent_runtime` 持有 Hook 运行时，`event` 包含已提交队列事实。
///
/// # Returns
/// 返回值
/// Success after timeline mutation and notification dispatch, or an explicit Hook lock error.
/// 时间线修改与通知分发后返回成功，或返回显式 Hook 锁错误。
pub(super) fn dispatch_queued_pending_prompt_hook(
    agent_runtime: &AgentRuntimeBridge,
    event: QueuedPendingPromptHookEvent,
) -> Result<(), String> {
    // hook_runtime may create or extend a logical turn only after the queue is authoritative.
    // hook_runtime 仅在队列成为权威状态后创建或扩展逻辑回合。
    let mut hook_runtime = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
    hook_runtime.record_queued_user_prompt(
        event.session_id,
        &event.prompt_entry_id,
        event.project_id,
        event.workspace_path,
        event.source,
        &event.prompt,
    );
    Ok(())
}

/// Returns whether a submitted prompt must enter the visible queue instead of starting immediately.
/// 判断提交的提示词是否必须进入可见队列而不是立即启动。
pub(super) fn session_has_blocking_system_prompt_work(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    pending_prompt_ui_state: Option<&SessionTranscriptUiState>,
    has_active_stream_task: bool,
    has_active_run: bool,
    has_precheck_progress: bool,
) -> bool {
    // An active stream owner is busy even when its kernel marker is between phases.
    // 即使内核标记正处于阶段切换中，活动流所有者仍表示会话繁忙。
    if has_active_stream_task {
        return true;
    }
    if session_has_pending_plan_artifact(workspace_snapshot, session_id) {
        return true;
    }
    let dispatch_context =
        webui_pending_prompt_dispatch_context(workspace_snapshot, session_id, has_active_run);
    let empty_pending_prompt_ui_state = SessionTranscriptUiState::default();
    pending_prompt_lane_is_busy(
        pending_prompt_ui_state.unwrap_or(&empty_pending_prompt_ui_state),
        PendingPromptBusyLaneContext {
            dispatch: dispatch_context,
            has_precheck_progress,
        },
    )
}

/// Builds the shared pending-prompt dispatch context from WebUI-owned runtime facts.
/// 从 WebUI 持有的运行态事实构建共享待发送提示词分发上下文。
pub(super) fn webui_pending_prompt_dispatch_context(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    has_active_run: bool,
) -> PendingPromptDispatchContext {
    PendingPromptDispatchContext {
        has_running_subagents: false,
        has_active_run,
        run_phase: workspace_snapshot
            .session_run_phases
            .get(&session_id)
            .copied(),
        has_active_compaction: false,
    }
}

/// Appends one submitted prompt to the visible pending queue using the current service runtime options.
/// 使用当前服务运行时选项将单条提交提示词追加到可见待发送队列。
///
/// # Parameters
/// 参数
/// `service_state` owns the non-serializable process mirror, `agent_runtime` supplies runtime
/// options, and `session_id` selects the session.
/// `service_state` 持有不可序列化的进程镜像，`agent_runtime` 提供运行时选项，`session_id`
/// 选择会话。
/// `client_action_id` reuses the already durable browser action as the process-local row identity.
/// `client_action_id` 复用已经可靠落库的浏览器动作作为进程内行身份。
/// The remaining parameters contain the authoritative workspace path, prompt content, attachments, and execution controls.
/// 其余参数包含权威工作区路径、提示词内容、附件与执行控制。
///
/// # Returns
/// 返回值
/// Returns Hook facts after the process mirror has accepted the row, or an explicit mirror/lease
/// error. This function never writes `SessionRuntimeStateEntry`.
/// 在进程镜像接纳该行后返回 Hook 事实，或明确的镜像/租约错误；该函数绝不写入
/// `SessionRuntimeStateEntry`。
pub(super) fn enqueue_webui_pending_prompt_from_submission(
    service_state: &mut ServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    workspace_path: &str,
    client_action_id: &str,
    prompt: String,
    hidden_model_guidance: Option<String>,
    attachments: Vec<ConversationAttachment>,
    reasoning_effort: Option<&str>,
    model_selection: Option<&crate::conversation_submission::SubmissionModelSelection>,
) -> Result<QueuedPendingPromptHookEvent, String> {
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    let project_id = owning_project_id_for_session(workspace_snapshot, session_id);
    let entry = build_webui_pending_prompt_panel_entry(
        client_action_id,
        prompt,
        hidden_model_guidance,
        attachments,
        current_pending_prompt_model_reference(
            agent_runtime,
            session_id,
            workspace_snapshot.session_runtime_state.get(&session_id),
            model_selection,
        )?,
        pending_prompt_runtime_options_from_submission(
            &agent_runtime.provider_configuration,
            reasoning_effort,
        )?,
    );
    let queue_scope = pending_prompt_attachment_staging_scope(&entry.id)?;
    acquire_staged_attachment_ref(workspace_path, &queue_scope, &entry.attachments)?;
    match enqueue_webui_pending_prompt_panel_entry(
        &mut service_state.pending_prompt_process_mirrors,
        session_id,
        project_id,
        workspace_path,
        entry.clone(),
    ) {
        Ok(event) => Ok(event),
        Err(error) => {
            // queueLeaseRollback prevents a failed queue admission from retaining an invisible asset owner.
            // queueLeaseRollback 防止失败的队列准入保留不可见的资产所有者。
            if let Err(rollback_error) =
                release_staged_attachment_ref(workspace_path, &queue_scope, &entry.attachments)
            {
                return Err(format!(
                    "{error}; failed to roll back pending attachment lease: {rollback_error}"
                ));
            }
            Err(error)
        }
    }
}

/// Builds the deterministic transient attachment owner for one visible pending-prompt row.
/// 为一条可见待发送提示词行构建确定性的瞬时附件所有者。
///
/// # Parameters
/// 参数
/// `panel_entry_id` is the stable visible queue-row identifier.
/// `panel_entry_id` 是稳定的可见队列行标识。
///
/// # Returns
/// 返回值
/// The internal-only asset-index scope, or an explicit invalid-identity error.
/// 返回仅供内部资产索引使用的作用域，或明确的身份无效错误。
pub(super) fn pending_prompt_attachment_staging_scope(
    panel_entry_id: &str,
) -> Result<String, String> {
    if panel_entry_id.chars().any(char::is_control) {
        return Err(String::from(
            "Pending prompt attachment owner identity is invalid.",
        ));
    }
    let panel_entry_id = panel_entry_id.trim();
    if panel_entry_id.is_empty() || panel_entry_id.len() > 256 {
        return Err(String::from(
            "Pending prompt attachment owner identity is invalid.",
        ));
    }
    Ok(format!("pending-prompt:v1:{panel_entry_id}"))
}

/// Builds one WebUI-originated visible pending prompt entry.
/// 构建一条来自 WebUI 的可见待发送提示词条目。
fn build_webui_pending_prompt_panel_entry(
    client_action_id: &str,
    prompt: String,
    hidden_model_guidance: Option<String>,
    attachments: Vec<ConversationAttachment>,
    model_reference: Option<PendingPromptModelReference>,
    runtime_options: PendingPromptRuntimeOptions,
) -> PendingPromptPanelEntry {
    let created_at = current_unix_millis_text();
    PendingPromptPanelEntry {
        // The process-local compatibility row reuses the durable browser action identity so
        // retries cannot create a second Hook item.
        // 进程内兼容行复用可靠的浏览器动作身份，确保重试不会创建第二条 Hook 记录。
        id: client_action_id.to_string(),
        preview: pending_prompt_preview_text(&prompt, &attachments),
        prompt,
        hidden_model_guidance,
        attachments,
        model_reference,
        runtime_options,
        state: PendingPromptPanelEntryState::Ready,
        created_at,
    }
}

/// Appends one visible pending prompt entry to the selected session queue and produces queued user-event facts.
/// 将一条可见待发送提示词追加到选中会话队列，并生成排队用户事件事实。
///
/// # Parameters
/// 参数
/// `mirrors` owns non-serializable queue state, `session_id` selects the session,
/// `project_id` is the already resolved project identity, and `workspace_path` is its
/// authoritative path.
/// `mirrors` 持有不可序列化的队列状态，`session_id` 选择会话，`project_id` 是已解析的项目
/// 身份，`workspace_path` 是其权威路径。
/// `entry` is the fully resolved visible prompt row to append.
/// `entry` 是要追加的完整可见提示词行。
///
/// # Returns
/// 返回值
/// Returns the Hook event facts aligned with the appended entry, or a queue-capacity error.
/// 返回与已追加条目对齐的 Hook 事件事实，或队列容量错误。
fn enqueue_webui_pending_prompt_panel_entry(
    mirrors: &mut BTreeMap<SessionId, PendingPromptProcessMirror>,
    session_id: SessionId,
    project_id: Option<String>,
    workspace_path: &str,
    entry: PendingPromptPanelEntry,
) -> Result<QueuedPendingPromptHookEvent, String> {
    let mirror = mirrors.entry(session_id).or_default();
    if mirror.ui_state.pending_prompt_panel_queue.len() >= PENDING_PROMPT_QUEUE_LIMIT {
        return Err(String::from(
            "待发送消息队列已满，最多保留 10 条，请等待前面的消息完成后再继续追加。",
        ));
    }
    let entry_id = entry.id.clone();
    let prompt = entry.prompt.clone();
    mirror.ui_state.pending_prompt_panel_queue.push(entry);
    Ok(QueuedPendingPromptHookEvent {
        session_id,
        prompt_entry_id: entry_id,
        project_id,
        workspace_path: workspace_path.to_string(),
        source: PendingSessionPromptSource::UserFollowup,
        prompt,
    })
}

/// Captures the exact selected or currently active public model identity for one queued prompt.
/// 为排队提示词捕获精确选定的或当前激活的公共模型身份。
///
/// # Parameters
/// 参数
/// `agent_runtime` resolves the authoritative public model and credential catalog.
/// `agent_runtime` 解析权威公共模型与凭据目录。
/// `session_id` and `runtime_state` identify the queue owner's immutable route lease.
/// `session_id` 与 `runtime_state` 标识队列所有者的不可变路由租约。
/// `model_selection` is the optional composer selection that must take precedence over the active session selection.
/// `model_selection` 是可选的输入区选择，必须优先于当前会话选择。
///
/// # Returns
/// 返回值
/// Returns a queue model reference when the active public selection is available, or an explicit selection error.
/// 当前公共选择可用时返回队列模型引用，显式选择无效时返回明确错误。
fn current_pending_prompt_model_reference(
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    runtime_state: Option<&SessionRuntimeStateEntry>,
    model_selection: Option<&crate::conversation_submission::SubmissionModelSelection>,
) -> Result<Option<PendingPromptModelReference>, String> {
    let configuration = agent_runtime.public_ai_runtime.configuration_snapshot()?;
    let selection = match model_selection {
        Some(model_selection) => {
            // matchingLease keeps a queued follow-up on the same frozen route even after its model
            // disappears from the current hot-updated catalog.
            // matchingLease 使排队跟进消息在模型从热更新后的当前目录消失时仍使用同一冻结路由。
            let matching_lease = session_process_public_ai_selection(
                agent_runtime.public_ai_runtime.as_ref(),
                session_id,
                runtime_state,
            )?
            .filter(|selection| {
                selection.route.provider.id == model_selection.provider_id
                    && selection.route.connection.id == model_selection.connection_id
                    && selection.model.id == model_selection.model_id
            });
            match matching_lease {
                Some(selection) => selection,
                None => agent_runtime
                    .public_ai_runtime
                    .resolve_connection_model_selection(
                        &configuration,
                        model_selection.connection_id.as_str(),
                        model_selection.provider_id.as_str(),
                        model_selection.model_id.as_str(),
                    )?,
            }
        }
        None => match public_ai_workbench_selection(
            &configuration,
            agent_runtime.public_ai_runtime.as_ref(),
            Some(session_id),
            runtime_state,
        )? {
            Some(selection) => selection,
            None => return Ok(None),
        },
    };
    Ok(Some(PendingPromptModelReference {
        connection_id: selection.route.connection.id,
        provider_id: selection.route.provider.id,
        model_id: selection.model.id,
        provider_label: selection.route.provider.display_name,
        model_name: selection.model.display_name,
    }))
}

/// Captures runtime controls for one queued WebUI prompt from the shared service router input.
/// 从共享服务路由输入捕获单条 WebUI 排队提示词的运行控制参数。
fn pending_prompt_runtime_options_from_submission(
    provider_configuration: &ProviderConfigurationFile,
    reasoning_effort: Option<&str>,
) -> Result<PendingPromptRuntimeOptions, String> {
    Ok(PendingPromptRuntimeOptions {
        reasoning_effort: reasoning_effort
            .and_then(parse_reasoning_effort)
            .or_else(|| active_reasoning_selection_effort(provider_configuration))
            .unwrap_or(ReasoningEffortLevel::Off),
        temperature: None,
        web_search_override: Default::default(),
        web_search_provider_id: None,
    })
}

/// Clones the runtime bridge with the queued prompt's locked public model identity.
/// 使用排队提示词锁定的公共模型身份克隆运行时桥接。
pub(super) fn agent_runtime_for_pending_prompt(
    agent_runtime: &AgentRuntimeBridge,
    prompt_entry: &vulcan_workbench_model::PendingSessionPromptEntry,
) -> AgentRuntimeBridge {
    let mut next_runtime = agent_runtime.clone();
    next_runtime.provider_id_override = prompt_entry
        .model_reference
        .as_ref()
        .map(|reference| reference.provider_id.clone());
    next_runtime.model_id_override = prompt_entry
        .model_reference
        .as_ref()
        .map(|reference| reference.model_id.clone());
    next_runtime.connection_id_override =
        prompt_entry.model_reference.as_ref().and_then(|reference| {
            (!reference.connection_id.is_empty()).then(|| reference.connection_id.clone())
        });
    next_runtime
}

/// Converts one persisted visible pending-prompt entry into a WebUI summary.
/// 将单条持久化可见待发送提示词转换为 WebUI 摘要。
pub(super) fn pending_prompt_summary_from_panel_entry(
    entry: &PendingPromptPanelEntry,
) -> PendingPromptSummary {
    PendingPromptSummary {
        id: entry.id.clone(),
        prompt: entry.prompt.clone(),
        preview: entry.preview.clone(),
        attachments: entry
            .attachments
            .iter()
            .map(composer_attachment_summary_from_core)
            .collect(),
        state: match entry.state {
            PendingPromptPanelEntryState::Ready => String::from("ready"),
            PendingPromptPanelEntryState::Inserted => String::from("inserted"),
        },
        created_at: entry.created_at.clone(),
    }
}

/// Converts visible pending prompt panel entries into WebUI summaries.
/// 将可见待发送提示词面板条目转换为 WebUI 摘要集合。
pub(super) fn pending_prompt_summaries_from_process_mirror(
    mirror: &PendingPromptProcessMirror,
) -> Vec<PendingPromptSummary> {
    mirror
        .ui_state
        .pending_prompt_panel_queue
        .iter()
        .map(pending_prompt_summary_from_panel_entry)
        .collect()
}

/// Rebuilds the non-serializable queue mirror from authoritative V1 queued inputs.
/// 从权威 V1 排队输入重建不可序列化的队列镜像。
///
/// `queued_inputs` preserves durable FIFO order and `resolve_attachments` resolves each input's
/// ordered project resources without consulting legacy runtime state.
/// `queued_inputs` 保留可靠 FIFO 顺序，`resolve_attachments` 为每条输入解析有序项目资源且不读取旧运行态。
pub(super) fn pending_prompt_process_mirror_from_v1_queued_inputs<ResolveAttachments>(
    queued_inputs: &[QueuedInput],
    mut resolve_attachments: ResolveAttachments,
) -> Result<PendingPromptProcessMirror, String>
where
    ResolveAttachments: FnMut(&QueuedInput) -> Result<Vec<ConversationAttachment>, String>,
{
    let mut mirror = PendingPromptProcessMirror::default();
    for queued_input in queued_inputs {
        let attachments = resolve_attachments(queued_input)?;
        mirror
            .ui_state
            .pending_prompt_panel_queue
            .push(PendingPromptPanelEntry {
                id: queued_input.client_action_id.as_str().to_string(),
                preview: pending_prompt_preview_text(&queued_input.text, &attachments),
                prompt: queued_input.text.clone(),
                hidden_model_guidance: None,
                attachments,
                model_reference: None,
                runtime_options: PendingPromptRuntimeOptions::default(),
                state: PendingPromptPanelEntryState::Ready,
                created_at: queued_input.created_at_ms.to_string(),
            });
    }
    Ok(mirror)
}

/// Emits the current visible pending prompt queue for one session through a workbench delta.
/// 通过工作台增量发送单个会话当前可见的待发送提示词队列。
pub(super) fn emit_pending_prompts_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<u64, String> {
    let conversation_id = session_id.to_string();
    let (event, revision) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let pending_prompts = guard
            .pending_prompt_process_mirrors
            .get(&session_id)
            .map(pending_prompt_summaries_from_process_mirror)
            .unwrap_or_default();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let revision = guard.workbench_generation;
        guard.workbench.revision = revision;
        // delta is the authoritative pending-prompt projection for this conversation.
        // delta 是该会话的权威待发送提示词投影。
        let delta = PendingPromptsChangedDelta {
            conversation_id,
            pending_prompts,
        };
        if guard.workbench.selected_conversation_id.as_deref()
            == Some(delta.conversation_id.as_str())
        {
            guard.workbench.pending_prompts = delta.pending_prompts.clone();
        }
        replace_conversation_scoped_projection_entry(
            &mut guard.workbench.pending_prompts_by_conversation,
            delta.conversation_id.as_str(),
            (!delta.pending_prompts.is_empty()).then(|| delta.clone()),
            |entry| entry.conversation_id.as_str(),
        );
        (
            WorkbenchDeltaEnvelope {
                generation: revision,
                delta: WorkbenchDelta::PendingPromptsChanged(delta),
            },
            revision,
        )
    };
    emit_service_event_with_trace(
        app,
        "workbench_pending_prompts_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(revision)
}

/// Builds one compact single-line preview used by the pending prompt queue strip.
/// 构建待发送提示词队列条使用的单行紧凑预览。
pub(super) fn pending_prompt_preview_text(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> String {
    let trimmed_prompt = prompt.trim();
    let attachment_prefix = if attachments.is_empty() {
        String::new()
    } else {
        format!("{} 个附件 · ", attachments.len())
    };
    let text = if trimmed_prompt.is_empty() {
        String::from("附件消息")
    } else {
        let mut normalized_prompt = String::new();
        // Collapse all whitespace classes into single spaces without materializing a token list.
        // 不落地中间词元列表，直接把所有空白字符折叠为单个空格。
        for segment in trimmed_prompt.split_whitespace() {
            if !normalized_prompt.is_empty() {
                normalized_prompt.push(' ');
            }
            normalized_prompt.push_str(segment);
        }
        normalized_prompt
    };
    let preview = format!("{attachment_prefix}{text}");
    if preview.chars().count() <= 48 {
        preview
    } else {
        format!("{}...", preview.chars().take(48).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_core::types::ConversationAttachmentKind;
    use vulcan_session_core::v1::ids::{ClientActionId, PendingInputId, ResourceId};

    /// Builds one minimal pending-prompt preview attachment.
    /// 构建一个最小待发送提示词预览附件。
    fn preview_test_attachment() -> ConversationAttachment {
        ConversationAttachment {
            kind: ConversationAttachmentKind::Image,
            file_name: String::from("chart.png"),
            local_path: String::from("C:/tmp/chart.png"),
            resource_path: None,
            source_path: None,
            asset_id: None,
            byte_len: Some(128),
            source_sha256: None,
            delivery_mode: None,
            preview_local_path: None,
            mime_type: String::from("image/png"),
        }
    }

    /// Verifies one visible pending row maps to exactly one stable transient asset owner.
    /// 验证一条可见待发送行只会映射到一个稳定的瞬时资产所有者。
    #[test]
    fn pending_prompt_attachment_scope_is_deterministic() {
        assert_eq!(
            pending_prompt_attachment_staging_scope(" row-a ")
                .expect("bounded row identity should be valid"),
            "pending-prompt:v1:row-a"
        );
        assert!(pending_prompt_attachment_staging_scope("").is_err());
        assert!(pending_prompt_attachment_staging_scope("row\na").is_err());
    }

    /// Verifies pending prompt previews collapse whitespace without changing visible words.
    /// 验证待发送提示词预览会折叠空白且不改变可见词语。
    #[test]
    fn pending_prompt_preview_collapses_whitespace_without_token_collection() {
        assert_eq!(
            pending_prompt_preview_text("  hello\t\n  world  ", &[]),
            "hello world"
        );
    }

    /// Verifies attachment-only pending prompt previews keep the canonical label.
    /// 验证仅包含附件的待发送提示词预览会保留规范标签。
    #[test]
    fn pending_prompt_preview_uses_attachment_label_for_empty_prompt() {
        assert_eq!(
            pending_prompt_preview_text("   ", &[preview_test_attachment()]),
            "1 个附件 · 附件消息"
        );
    }

    /// Verifies restart mirror projection preserves durable FIFO identity and ordered resources.
    /// 验证重启镜像投影会保留可靠 FIFO 身份与有序资源。
    #[test]
    fn v1_queued_inputs_rebuild_process_mirror_without_legacy_runtime_state() {
        let queued_inputs = vec![
            QueuedInput {
                client_action_id: ClientActionId::try_new("client-action-1")
                    .expect("first client action should be valid"),
                input_id: PendingInputId::try_new(11).expect("first pending input should be valid"),
                text: String::from("first queued prompt"),
                resource_ids: vec![
                    ResourceId::try_new(101).expect("first resource should be valid"),
                    ResourceId::try_new(102).expect("second resource should be valid"),
                ],
                created_at_ms: 1_787_000_000_001,
            },
            QueuedInput {
                client_action_id: ClientActionId::try_new("client-action-2")
                    .expect("second client action should be valid"),
                input_id: PendingInputId::try_new(12)
                    .expect("second pending input should be valid"),
                text: String::from("second queued prompt"),
                resource_ids: vec![
                    ResourceId::try_new(103).expect("third resource should be valid"),
                ],
                created_at_ms: 1_787_000_000_002,
            },
        ];
        let mirror =
            pending_prompt_process_mirror_from_v1_queued_inputs(&queued_inputs, |queued_input| {
                Ok(queued_input
                    .resource_ids
                    .iter()
                    .map(|resource_id| ConversationAttachment {
                        kind: ConversationAttachmentKind::File,
                        file_name: format!("resource-{}.txt", resource_id.value()),
                        local_path: format!("D:/project/resource-{}.txt", resource_id.value()),
                        resource_path: None,
                        source_path: None,
                        asset_id: None,
                        byte_len: None,
                        source_sha256: None,
                        delivery_mode: None,
                        preview_local_path: None,
                        mime_type: String::from("text/plain"),
                    })
                    .collect())
            })
            .expect("V1 queued inputs should rebuild the process mirror");

        let rows = &mirror.ui_state.pending_prompt_panel_queue;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "client-action-1");
        assert_eq!(rows[0].prompt, "first queued prompt");
        assert_eq!(rows[0].created_at, "1787000000001");
        assert_eq!(
            rows[0]
                .attachments
                .iter()
                .map(|attachment| attachment.file_name.as_str())
                .collect::<Vec<_>>(),
            ["resource-101.txt", "resource-102.txt"]
        );
        assert_eq!(rows[1].id, "client-action-2");
        assert_eq!(rows[1].prompt, "second queued prompt");
        assert_eq!(rows[1].attachments[0].file_name, "resource-103.txt");
        assert!(
            rows.iter()
                .all(|row| row.state == PendingPromptPanelEntryState::Ready)
        );
    }
}

#[cfg(test)]
mod provider_default_tests {
    use super::*;

    /// Verifies queued prompts do not invent a reasoning effort when no selection exists.
    /// 验证排队提示词在没有选择时不会臆造推理强度。
    #[test]
    fn queued_prompt_without_selection_preserves_provider_reasoning_default() {
        let options = pending_prompt_runtime_options_from_submission(
            &ProviderConfigurationFile::default(),
            None,
        )
        .expect("provider-default queued prompt options should resolve");

        assert_eq!(options.reasoning_effort, ReasoningEffortLevel::Off);
        assert!(options.temperature.is_none());
    }
}
