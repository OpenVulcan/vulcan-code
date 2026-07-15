//! Pending prompt queue service adapters for the WebUI bridge.
//! WebUI 桥接层待发送提示词队列服务适配器。

use vulcan_agent_runtime::{
    provider_config::{
        self, PersistedAiToolConfiguration, ProviderConfigurationFile, ProviderRuntimeKind,
        ReasoningEffortLevel,
    },
    workbench_ai_capability_router::effective_workbench_disabled_ai_capability_kinds,
};
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
    owning_project_id_for_session, parse_reasoning_effort,
    replace_conversation_scoped_projection_entry, session_ai_tool_configuration_or_default,
    session_has_pending_plan_artifact,
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

/// Returns whether a runtime state has queued pending prompt work.
/// 返回运行态是否存在已排队的待发送提示词工作。
pub(super) fn runtime_state_has_queued_pending_prompt_work(
    runtime_state: &SessionRuntimeStateEntry,
) -> bool {
    !runtime_state.ui_state.pending_prompt_queue.is_empty()
        || !runtime_state.ui_state.pending_prompt_panel_queue.is_empty()
}

/// Returns whether a submitted prompt must enter the visible queue instead of starting immediately.
/// 判断提交的提示词是否必须进入可见队列而不是立即启动。
pub(super) fn session_has_blocking_system_prompt_work(
    workspace_snapshot: &WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
    has_active_run: bool,
    has_precheck_progress: bool,
) -> bool {
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return has_active_run;
    };
    if session_has_pending_plan_artifact(workspace_snapshot, session_id) {
        return true;
    }
    let dispatch_context = webui_pending_prompt_dispatch_context(
        workspace_snapshot,
        provider_configuration,
        session_id,
        has_active_run,
    );
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
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
    has_active_run: bool,
) -> PendingPromptDispatchContext {
    PendingPromptDispatchContext {
        uses_anthropic_runtime: session_uses_anthropic_runtime(
            workspace_snapshot,
            provider_configuration,
            session_id,
        ),
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
    disabled_ai_capability_kinds: Option<&[String]>,
) -> Result<QueuedPendingPromptHookEvent, String> {
    let session_ai_tool_configuration = session_ai_tool_configuration_or_default(
        workspace_snapshot,
        session_id,
        &agent_runtime.ai_tool_configuration,
    );
    let entry = build_webui_pending_prompt_panel_entry(
        prompt,
        hidden_model_guidance,
        attachments,
        current_pending_prompt_model_reference(&agent_runtime.provider_configuration),
        pending_prompt_runtime_options_from_submission(
            &agent_runtime.provider_configuration,
            session_ai_tool_configuration,
            reasoning_effort,
            disabled_ai_capability_kinds,
        )?,
    );
    enqueue_webui_pending_prompt_panel_entry(workspace_snapshot, session_id, workspace_path, entry)
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

/// Captures the active provider/model identity for one queued WebUI prompt.
/// 为单条 WebUI 排队提示词捕获当前激活的供应商与模型身份。
fn current_pending_prompt_model_reference(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<PendingPromptModelReference> {
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    Some(PendingPromptModelReference {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        provider_label: provider_config::provider_summary(provider_id),
        model_name: provider_config::model_summary(provider_id, model_id, provider_configuration),
        openrouter_endpoint_route: (provider_id == "openrouter")
            .then(|| {
                provider_config::openrouter_endpoint_route_for_model(
                    provider_configuration,
                    model_id,
                )
            })
            .flatten(),
    })
}

/// Captures runtime controls for one queued WebUI prompt from the shared service router input.
/// 从共享服务路由输入捕获单条 WebUI 排队提示词的运行控制参数。
fn pending_prompt_runtime_options_from_submission(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    reasoning_effort: Option<&str>,
    disabled_ai_capability_kinds: Option<&[String]>,
) -> Result<PendingPromptRuntimeOptions, String> {
    Ok(PendingPromptRuntimeOptions {
        reasoning_effort: reasoning_effort
            .and_then(parse_reasoning_effort)
            .or_else(|| active_reasoning_selection_effort(provider_configuration))
            .unwrap_or(ReasoningEffortLevel::Medium),
        temperature: None,
        web_search_override: Default::default(),
        web_search_provider_id: None,
        disabled_ai_capability_kinds: effective_workbench_disabled_ai_capability_kinds(
            ai_tool_configuration,
            disabled_ai_capability_kinds,
        )?,
    })
}

/// Clones provider configuration and overrides only the queued prompt's locked model identity.
/// 克隆 Provider 配置，并仅覆盖排队提示词锁定的模型身份。
pub(super) fn provider_configuration_for_model_reference(
    provider_configuration: &ProviderConfigurationFile,
    model_reference: Option<&PendingPromptModelReference>,
) -> ProviderConfigurationFile {
    let mut next_configuration = provider_configuration.clone();
    if let Some(model_reference) = model_reference {
        next_configuration.model = Some(format!(
            "{}/{}",
            model_reference.provider_id, model_reference.model_id
        ));
        if model_reference.provider_id == "openrouter" {
            provider_config::set_openrouter_endpoint_route_for_model(
                &mut next_configuration,
                &model_reference.model_id,
                model_reference.openrouter_endpoint_route.clone(),
            );
        }
    }
    next_configuration
}

/// Clones the runtime bridge with the queued prompt's locked provider configuration.
/// 使用排队提示词锁定的 Provider 配置克隆运行时桥接。
pub(super) fn agent_runtime_for_pending_prompt(
    agent_runtime: &AgentRuntimeBridge,
    prompt_entry: &vulcan_workbench_model::PendingSessionPromptEntry,
) -> AgentRuntimeBridge {
    let mut next_runtime = agent_runtime.clone();
    next_runtime.provider_configuration = provider_configuration_for_model_reference(
        &agent_runtime.provider_configuration,
        prompt_entry.model_reference.as_ref(),
    );
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
/// Returns one post-commit Hook event for a new insertion, `None` for an idempotent route, or a routing error.
/// 新插入时返回一条提交后 Hook 事件，幂等路由时返回 `None`，失败时返回路由错误。
pub(super) fn insert_webui_pending_prompt_into_internal_queue(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    workspace_path: &str,
    entry_id: &str,
) -> Result<Option<QueuedPendingPromptHookEvent>, String> {
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
    Ok(Some(QueuedPendingPromptHookEvent {
        session_id,
        prompt_entry_id: entry_id.to_string(),
        project_id,
        workspace_path: workspace_path.to_string(),
        source: PendingSessionPromptSource::UserInterrupt,
        prompt: internal_entry.prompt,
    }))
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

/// Returns whether one session should use Anthropic-specific queue dispatch ordering.
/// 返回单个会话是否应使用 Anthropic 专属队列分发顺序。
fn session_uses_anthropic_runtime(
    workspace_snapshot: &WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
) -> bool {
    if let Some(active_provider_id) = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_provider_id.as_deref())
    {
        return provider_config::supported_provider_by_id(active_provider_id)
            .is_some_and(|definition| definition.runtime == ProviderRuntimeKind::Anthropic);
    }
    provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
        .and_then(|(provider_id, _)| provider_config::supported_provider_by_id(provider_id))
        .is_some_and(|definition| definition.runtime == ProviderRuntimeKind::Anthropic)
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
            source_path: None,
            asset_id: None,
            byte_len: Some(128),
            preview_local_path: None,
            mime_type: String::from("image/png"),
        }
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
}
