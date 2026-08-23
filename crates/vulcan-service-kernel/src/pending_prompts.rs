//! Pending prompt queue service adapters for the WebUI bridge.
//! WebUI 桥接层待发送提示词队列服务适配器。

use vulcan_agent_runtime::{
    attachment_asset_index::{acquire_staged_attachment_ref, release_staged_attachment_ref},
    provider_config::{ProviderConfigurationFile, ReasoningEffortLevel},
};
use vulcan_context::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use vulcan_core::controller::INTERJECTED_PROMPT_ADMISSION_ID_KEY;
use vulcan_core::types::ConversationAttachment;
use vulcan_service_api::{
    PendingPromptSummary, PendingPromptsChangedDelta, ServiceEvent, ServiceEventEmitter,
    WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::{
    PENDING_PROMPT_QUEUE_LIMIT, PendingPromptBusyLaneContext, PendingPromptDispatchContext,
    PendingPromptModelReference, PendingPromptPanelEntry, PendingPromptPanelEntryState,
    PendingPromptQueueRouteError, PendingPromptQueueRouteOutcome, PendingPromptRuntimeOptions,
    PendingSessionPromptEntry, PendingSessionPromptSource, SessionId, SessionRunPhase,
    SessionRuntimeStateEntry, clear_internal_pending_prompt_queue_and_restore_panel,
    enqueue_pending_prompt_internal_queue_entry, pending_prompt_lane_is_busy,
    remove_internal_pending_prompt_entries_by_source, remove_pending_prompt_panel_entry,
    route_pending_prompt_panel_entry_into_internal_queue, take_next_pending_prompt_entry,
};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, active_reasoning_selection_effort,
    composer_attachment_summary_from_core, current_unix_millis_text, emit_service_event_with_trace,
    owning_project_id_for_session, parse_reasoning_effort, public_ai_workbench_selection,
    replace_conversation_scoped_projection_entry, session_has_pending_plan_artifact,
    session_process_public_ai_selection,
};

/// Snapshot of pending prompt queue lengths used by service lifecycle diagnostics.
/// 服务生命周期诊断使用的待发送提示词队列长度快照。
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingPromptQueueStatus {
    /// Number of internal queued prompt entries ready for runtime dispatch.
    /// 可供运行时分发的内部排队提示词条目数量。
    pub(super) internal_queue_len: usize,
    /// Number of visible panel prompt entries waiting for user-guided dispatch.
    /// 等待用户引导分发的可见面板提示词条目数量。
    pub(super) visible_queue_len: usize,
    /// Current session run phase recorded by the WebUI workspace snapshot.
    /// WebUI 工作区快照记录的当前会话运行阶段。
    pub(super) run_phase: Option<SessionRunPhase>,
}

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

impl QueuedPendingPromptHookEvent {
    /// Returns the authoritative workspace path captured by queue admission.
    /// 返回队列准入捕获的权威工作区路径。
    pub(super) fn workspace_path(&self) -> &str {
        self.workspace_path.as_str()
    }
}

/// Prompt admission committed atomically with its immediate transcript projection.
/// 与立即转录投影原子提交的提示词接纳结果。
#[derive(Debug, Clone)]
pub(super) struct QueuedPendingPromptAdmission {
    /// Hook event dispatched after the queue mutation becomes authoritative.
    /// 队列变更成为权威状态后分发的 Hook 事件。
    pub(super) hook_event: QueuedPendingPromptHookEvent,
    /// Visible user row emitted immediately when queue admission succeeds.
    /// 队列接纳成功时立即发出的可见用户行。
    pub(super) message: SessionMessageEntry,
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

/// Returns whether a runtime state has internally scheduled pending prompt work.
/// 返回运行态是否存在已经进入内部调度的待发送提示词工作。
pub(super) fn runtime_state_has_queued_pending_prompt_work(
    runtime_state: &SessionRuntimeStateEntry,
) -> bool {
    // Ready panel rows remain user-controlled staging data and must not block an exact Continue claim.
    // 就绪面板行仍是用户控制的暂存数据，不能阻止精确的 Continue 领取。
    !runtime_state.ui_state.pending_prompt_queue.is_empty()
}

/// Returns whether a submitted prompt must enter the visible queue instead of starting immediately.
/// 判断提交的提示词是否必须进入可见队列而不是立即启动。
pub(super) fn session_has_blocking_system_prompt_work(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    has_active_stream_task: bool,
    has_active_run: bool,
    has_precheck_progress: bool,
) -> bool {
    // An active stream owner is busy even when its kernel marker is between phases.
    // 即使内核标记正处于阶段切换中，活动流所有者仍表示会话繁忙。
    if has_active_stream_task {
        return true;
    }
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return has_active_run;
    };
    if session_has_pending_plan_artifact(workspace_snapshot, session_id) {
        return true;
    }
    let dispatch_context =
        webui_pending_prompt_dispatch_context(workspace_snapshot, session_id, has_active_run);
    pending_prompt_lane_is_busy(
        &runtime_state.ui_state,
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

/// Reads pending prompt queue counters without mutating the workspace snapshot.
/// 在不修改工作区快照的情况下读取待发送提示词队列计数。
pub(super) fn pending_prompt_queue_status(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> PendingPromptQueueStatus {
    workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(|runtime_state| PendingPromptQueueStatus {
            internal_queue_len: runtime_state.ui_state.pending_prompt_queue.len(),
            visible_queue_len: runtime_state.ui_state.pending_prompt_panel_queue.len(),
            run_phase: workspace_snapshot
                .session_run_phases
                .get(&session_id)
                .copied(),
        })
        .unwrap_or(PendingPromptQueueStatus {
            internal_queue_len: 0,
            visible_queue_len: 0,
            run_phase: None,
        })
}

/// Appends one submitted prompt to the visible pending queue using the current service runtime options.
/// 使用当前服务运行时选项将单条提交提示词追加到可见待发送队列。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the detached queue draft, `agent_runtime` supplies runtime options, and `session_id` selects the session.
/// `workspace_snapshot` 是分离队列草稿，`agent_runtime` 提供运行时选项，`session_id` 选择会话。
/// The remaining parameters contain the authoritative workspace path, prompt content, attachments, and execution controls.
/// 其余参数包含权威工作区路径、提示词内容、附件与执行控制。
///
/// # Returns
/// 返回值
/// Returns committed-candidate Hook facts that must be dispatched only after the workspace draft is stored.
/// 返回候选提交 Hook 事实，必须仅在工作区草稿存储后分发。
pub(super) fn enqueue_webui_pending_prompt_from_submission(
    workspace_snapshot: &mut WorkspaceSnapshot,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    workspace_path: &str,
    prompt: String,
    hidden_model_guidance: Option<String>,
    attachments: Vec<ConversationAttachment>,
    reasoning_effort: Option<&str>,
    model_selection: Option<&crate::conversation_submission::SubmissionModelSelection>,
) -> Result<QueuedPendingPromptHookEvent, String> {
    let entry = build_webui_pending_prompt_panel_entry(
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
        workspace_snapshot,
        session_id,
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
    prompt: String,
    hidden_model_guidance: Option<String>,
    attachments: Vec<ConversationAttachment>,
    model_reference: Option<PendingPromptModelReference>,
    runtime_options: PendingPromptRuntimeOptions,
) -> PendingPromptPanelEntry {
    let created_at = current_unix_millis_text();
    PendingPromptPanelEntry {
        id: format!("webui-pending-panel-{}", SessionId::new_v4()),
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
/// `workspace_snapshot` owns the detached queue draft, `session_id` selects the session, and `workspace_path` is its authoritative path.
/// `workspace_snapshot` 持有分离队列草稿，`session_id` 选择会话，`workspace_path` 是其权威路径。
/// `entry` is the fully resolved visible prompt row to append.
/// `entry` 是要追加的完整可见提示词行。
///
/// # Returns
/// 返回值
/// Returns the Hook event facts aligned with the appended entry, or a queue-capacity error.
/// 返回与已追加条目对齐的 Hook 事件事实，或队列容量错误。
fn enqueue_webui_pending_prompt_panel_entry(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    workspace_path: &str,
    entry: PendingPromptPanelEntry,
) -> Result<QueuedPendingPromptHookEvent, String> {
    let now = current_unix_millis_text();
    let project_id = owning_project_id_for_session(workspace_snapshot, session_id);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    if runtime_state.ui_state.pending_prompt_panel_queue.len() >= PENDING_PROMPT_QUEUE_LIMIT {
        return Err(String::from(
            "待发送消息队列已满，最多保留 10 条，请等待前面的消息完成后再继续追加。",
        ));
    }
    let entry_id = entry.id.clone();
    let prompt = entry.prompt.clone();
    runtime_state
        .ui_state
        .pending_prompt_panel_queue
        .push(entry);
    runtime_state.updated_at = now;
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
pub(super) fn pending_prompt_summaries_from_runtime_state(
    runtime_state: &SessionRuntimeStateEntry,
) -> Vec<PendingPromptSummary> {
    runtime_state
        .ui_state
        .pending_prompt_panel_queue
        .iter()
        .map(pending_prompt_summary_from_panel_entry)
        .collect()
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
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace_snapshot| {
                workspace_snapshot.session_runtime_state.get(&session_id)
            })
            .map(pending_prompt_summaries_from_runtime_state)
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

/// Clears internal pending prompt work and restores visible panel rows.
/// 清理内部待发送提示词工作并恢复可见面板行。
pub(super) fn clear_internal_pending_prompt_queue_for_stop(
    runtime_state: &mut SessionRuntimeStateEntry,
) -> bool {
    clear_internal_pending_prompt_queue_and_restore_panel(&mut runtime_state.ui_state)
}

/// Enqueues one internal pending prompt entry and updates the runtime state timestamp.
/// 入队一条内部待发送提示词并更新运行态时间戳。
pub(super) fn enqueue_internal_pending_prompt_entry(
    runtime_state: &mut SessionRuntimeStateEntry,
    entry: PendingSessionPromptEntry,
    priority: bool,
    updated_at: String,
    error_prefix: &str,
) -> Result<(), String> {
    let _ =
        enqueue_pending_prompt_internal_queue_entry(&mut runtime_state.ui_state, entry, priority)
            .map_err(|error| format!("{error_prefix}：{error:?}"))?;
    runtime_state.updated_at = updated_at;
    Ok(())
}

/// Removes the next dispatchable queued prompt from the internal or visible WebUI queues.
/// 从 WebUI 内部或可见队列中移除下一条可分发排队提示词。
pub(super) fn take_next_webui_pending_prompt(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    priority_override: bool,
) -> Option<PendingSessionPromptEntry> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)?;
    let created_at = current_unix_millis_text();
    let internal_entry_id = format!("webui-pending-internal-{}", SessionId::new_v4());
    let entry = take_next_pending_prompt_entry(
        &mut runtime_state.ui_state,
        priority_override,
        internal_entry_id,
        created_at,
    )?;
    runtime_state.updated_at = current_unix_millis_text();
    Some(entry)
}

/// Restores a claimed pending prompt at the front of its dispatch queue after preparation fails or is cancelled.
/// 在准备失败或取消后，将已领取的待发送提示词恢复到分发队列最前端。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the detached queue snapshot being restored.
/// `workspace_snapshot` 持有正在恢复的分离队列快照。
/// `session_id` identifies the exact session queue that owns the prompt.
/// `session_id` 标识拥有该提示词的精确会话队列。
/// `entry` is the prompt removed for the current dispatch attempt.
/// `entry` 是本次分发尝试中已取出的提示词。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after front insertion, or an explicit queue-state error.
/// 成功插入队列前端后返回 `Ok(())`，否则返回明确的队列状态错误。
pub(super) fn restore_webui_pending_prompt(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    entry: PendingSessionPromptEntry,
) -> Result<(), String> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| {
            format!("Conversation `{session_id}` has no pending prompt runtime state.")
        })?;
    // A visible panel entry must return to Ready instead of being duplicated in the internal queue.
    // 可见面板条目必须恢复为 Ready，不能同时复制回内部队列。
    let restored_visible_panel = match entry.panel_entry_id.as_deref() {
        Some(panel_entry_id) => {
            if let Some(panel_entry) = runtime_state
                .ui_state
                .pending_prompt_panel_queue
                .iter_mut()
                .find(|panel_entry| panel_entry.id == panel_entry_id)
            {
                panel_entry.state = PendingPromptPanelEntryState::Ready;
                true
            } else {
                false
            }
        }
        None => false,
    };
    if !restored_visible_panel {
        enqueue_pending_prompt_internal_queue_entry(&mut runtime_state.ui_state, entry, true)
            .map_err(|error| format!("failed to restore pending prompt: {error:?}"))?;
    }
    runtime_state.updated_at = current_unix_millis_text();
    Ok(())
}

/// Removes one visible panel row after a guided pending prompt has entered runtime dispatch.
/// 在引导的待发送提示词进入运行时分发后移除一条可见面板行。
pub(super) fn remove_dispatched_pending_prompt_panel_entry(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    panel_entry_id: &str,
) {
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        let _ = remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, panel_entry_id);
        runtime_state.updated_at = current_unix_millis_text();
    }
}

/// Inserts one visible WebUI queue row into the internal session prompt queue.
/// 将一条 WebUI 可见队列行插入内部会话提示词队列。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the detached routing draft, `session_id` selects the session, and `workspace_path` is its authoritative path.
/// `workspace_snapshot` 持有分离路由草稿，`session_id` 选择会话，`workspace_path` 是其权威路径。
/// `entry_id` identifies the exact visible row routed into the internal queue.
/// `entry_id` 标识路由进内部队列的精确可见行。
///
/// # Returns
/// 返回值
/// Returns one post-commit admission with Hook and visible-row facts, `None` for an idempotent route, or a routing error.
/// 新插入时返回包含 Hook 与可见行事实的提交后接纳结果，幂等路由时返回 `None`，失败时返回路由错误。
pub(super) fn insert_webui_pending_prompt_into_internal_queue(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    workspace_path: &str,
    entry_id: &str,
) -> Result<Option<QueuedPendingPromptAdmission>, String> {
    let project_id = owning_project_id_for_session(workspace_snapshot, session_id);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("当前会话没有可引导的待发送队列。"))?;
    let created_at = current_unix_millis_text();
    let internal_entry_id = format!("webui-pending-internal-{}", SessionId::new_v4());
    let route_outcome = route_pending_prompt_panel_entry_into_internal_queue(
        &mut runtime_state.ui_state,
        entry_id,
        internal_entry_id,
        created_at,
        PendingSessionPromptSource::UserInterrupt,
    )
    .map_err(|error| match error {
        PendingPromptQueueRouteError::MissingPanelEntry => {
            String::from("没有找到待引导的待发送提示词。")
        }
        PendingPromptQueueRouteError::InternalQueueFull => {
            String::from("待发送消息队列已满，最多保留 10 条，请等待前面的消息完成后再继续追加。")
        }
    })?;
    runtime_state.updated_at = current_unix_millis_text();
    let PendingPromptQueueRouteOutcome::Inserted(internal_entry) = route_outcome else {
        return Ok(None);
    };
    let internal_entry = *internal_entry;
    let message = build_pending_prompt_admission_message(&internal_entry)?;
    workspace_snapshot
        .session_messages
        .entry(session_id)
        .or_default()
        .push(message.clone());
    Ok(Some(QueuedPendingPromptAdmission {
        hook_event: QueuedPendingPromptHookEvent {
            session_id,
            prompt_entry_id: entry_id.to_string(),
            project_id,
            workspace_path: workspace_path.to_string(),
            source: PendingSessionPromptSource::UserInterrupt,
            prompt: internal_entry.prompt,
        },
        message,
    }))
}

/// Builds the immediate visible row for one accepted main-session interrupt prompt.
/// 为一条已接纳的主会话插队提示词构建立即可见行。
///
/// # Parameters
/// 参数
/// `entry` is the authoritative internal queue entry accepted for dispatch.
/// `entry` 是已接纳等待分发的权威内部队列项。
///
/// # Returns
/// 返回值
/// A process-scoped user row carrying the stable admission identity, or an explicit attachment serialization error.
/// 返回携带稳定接纳标识的过程级用户行，或返回明确的附件序列化错误。
fn build_pending_prompt_admission_message(
    entry: &PendingSessionPromptEntry,
) -> Result<SessionMessageEntry, String> {
    let message_id = format!("pending-prompt-admission-{}", entry.id);
    let mut metadata = serde_json::json!({
        "canonical_text": entry.prompt,
        "canonical_user_text": entry.prompt,
        "source": "user_interrupt",
        "starts_user_turn": false,
        "turn_process": true,
        "collapsed_by_default": true,
        "editable": false,
        (INTERJECTED_PROMPT_ADMISSION_ID_KEY): entry.id,
    });
    if !entry.attachments.is_empty()
        && let Some(object) = metadata.as_object_mut()
    {
        let attachments = serde_json::to_value(&entry.attachments).map_err(|error| {
            format!(
                "Accepted pending prompt `{}` attachments are invalid: {error}",
                entry.id
            )
        })?;
        object.insert(String::from("attachments"), attachments.clone());
        object.insert(String::from("transcript_attachments"), attachments);
    }
    let mut message = SessionMessageEntry::new(SessionMessageKind::User, "用户指令", &entry.prompt)
        .with_runtime_metadata(message_id, entry.created_at.clone());
    message.record_kind = SessionMessageRecordKind::Transcript;
    message.metadata = Some(metadata);
    Ok(message)
}

/// Clears stale session-mailbox entries from the old WebUI pending-prompt queue.
/// 从旧 WebUI 待发送队列中清理过时的会话 mailbox 条目。
pub(super) fn clear_stale_webui_session_mailbox_pending_prompts(
    _app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<usize, String> {
    let mut removed = 0usize;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) else {
            return Ok(removed);
        };
        removed = remove_internal_pending_prompt_entries_by_source(
            &mut runtime_state.ui_state,
            PendingSessionPromptSource::SessionMailbox,
        );
        if removed > 0 {
            runtime_state.updated_at = current_unix_millis_text();
        }
    }
    Ok(removed)
}

/// Removes Goal-continuation guidance that does not belong to the durable current budget stage.
/// 移除不属于持久化当前预算阶段的 Goal 续跑引导。
///
/// # Parameters
/// 参数
/// `state` owns the retained session runtime queue.
/// `state` 持有保留的会话运行态队列。
/// `session_id` identifies the queue being validated.
/// `session_id` 标识正在校验的队列。
/// `current_goal_id` is the authoritative workspace-store stage identity, or `None` after clearing.
/// `current_goal_id` 是 workspace-store 的权威阶段身份，目标已清理时为 `None`。
///
/// # Returns
/// 返回值
/// The number of stale internal guidance entries removed, or a state-lock error.
/// 返回移除的过期内部引导数量，或状态锁错误。
pub(super) fn clear_stale_session_goal_pending_prompts(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    current_goal_id: Option<&str>,
) -> Result<usize, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(runtime_state) = guard
        .workspace_snapshot
        .as_mut()
        .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
    else {
        return Ok(0);
    };
    let before = runtime_state.ui_state.pending_prompt_queue.len();
    runtime_state.ui_state.pending_prompt_queue.retain(|entry| {
        entry.source != PendingSessionPromptSource::GoalContinuation
            || entry.task_id.as_deref() == current_goal_id
    });
    let removed = before.saturating_sub(runtime_state.ui_state.pending_prompt_queue.len());
    if removed > 0 {
        runtime_state.updated_at = current_unix_millis_text();
    }
    Ok(removed)
}

/// Builds one compact single-line preview used by the pending prompt queue strip.
/// 构建待发送提示词队列条使用的单行紧凑预览。
fn pending_prompt_preview_text(prompt: &str, attachments: &[ConversationAttachment]) -> String {
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

    /// Verifies an accepted interrupt prompt carries a stable immediate-projection identity.
    /// 验证已接纳的插队提示词会携带稳定的立即投影标识。
    #[test]
    fn pending_prompt_admission_message_is_process_scoped_and_stable() {
        let entry = PendingSessionPromptEntry {
            id: String::from("internal-entry-1"),
            prompt: String::from("立即返回结果"),
            hidden_model_guidance: None,
            preview: String::from("立即返回结果"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            source: PendingSessionPromptSource::UserInterrupt,
            task_id: None,
            panel_entry_id: Some(String::from("panel-entry-1")),
            created_at: String::from("1787000000000"),
        };

        let message =
            build_pending_prompt_admission_message(&entry).expect("admission message should build");

        assert_eq!(message.kind, SessionMessageKind::User);
        assert_eq!(message.body, "立即返回结果");
        assert_eq!(message.created_at, "1787000000000");
        let metadata = message.metadata.expect("admission metadata should exist");
        assert_eq!(metadata[INTERJECTED_PROMPT_ADMISSION_ID_KEY], entry.id);
        assert_eq!(metadata["turn_process"], true);
        assert_eq!(metadata["starts_user_turn"], false);
    }

    /// Verifies queue admission and its visible main-session row commit to the same workspace draft.
    /// 验证队列接纳及其主会话可见行会提交到同一份工作区草稿。
    #[test]
    fn interrupt_queue_admission_appends_visible_message_atomically() {
        let session_id = SessionId::new_v4();
        let mut workspace = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: Default::default(),
            archived_session_messages: Default::default(),
            session_compactions: Default::default(),
            archived_session_compactions: Default::default(),
            session_runtime_state: Default::default(),
            archived_session_runtime_state: Default::default(),
            session_run_phases: Default::default(),
            expanded_task_projects: Default::default(),
            selected_session: session_id,
            hydrated_session_ids: Default::default(),
        };
        let panel_entry = PendingPromptPanelEntry {
            id: String::from("panel-entry-1"),
            prompt: String::from("停止继续搜索，立即总结"),
            hidden_model_guidance: None,
            attachments: Vec::new(),
            preview: String::from("停止继续搜索，立即总结"),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            state: PendingPromptPanelEntryState::Ready,
            created_at: String::from("1787000000000"),
        };
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state
            .ui_state
            .pending_prompt_panel_queue
            .push(panel_entry);
        workspace
            .session_runtime_state
            .insert(session_id, runtime_state);

        let admission = insert_webui_pending_prompt_into_internal_queue(
            &mut workspace,
            session_id,
            "D:/workspace",
            "panel-entry-1",
        )
        .expect("interrupt prompt should admit")
        .expect("first admission should create a projection");

        let stored_messages = workspace
            .session_messages
            .get(&session_id)
            .expect("admission should materialize the user row");
        assert_eq!(stored_messages, &[admission.message]);
        let runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should remain available");
        assert_eq!(runtime_state.ui_state.pending_prompt_queue.len(), 1);
        assert_eq!(
            runtime_state.ui_state.pending_prompt_panel_queue[0].state,
            PendingPromptPanelEntryState::Inserted
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
