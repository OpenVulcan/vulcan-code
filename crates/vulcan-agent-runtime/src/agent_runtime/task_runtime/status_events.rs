use super::*;

/// Converts a task execution status into the persisted TaskSession status.
/// 将任务执行状态转换为持久化 TaskSession 状态。
pub(super) fn task_session_status_from_execution_status(
    status: TaskExecutionStatus,
) -> TaskSessionStatus {
    match status {
        TaskExecutionStatus::Running => TaskSessionStatus::Running,
        TaskExecutionStatus::Completed => TaskSessionStatus::Completed,
        TaskExecutionStatus::Stopped => TaskSessionStatus::Stopped,
    }
}

/// Converts a host payload status label into the persisted TaskSession status.
/// 将宿主载荷中的状态标签转换为持久化 TaskSession 状态。
pub(super) fn task_session_status_from_execution_status_text(status: &str) -> TaskSessionStatus {
    match status {
        "running" => TaskSessionStatus::Running,
        "completed" => TaskSessionStatus::Completed,
        "stopped" => TaskSessionStatus::Stopped,
        "failed" => TaskSessionStatus::Failed,
        _ => TaskSessionStatus::Running,
    }
}

/// Resolves a persisted task status without letting stale active updates reopen terminal work.
/// 解析持久化任务状态，避免迟到的活跃状态重新打开已终止任务。
pub(super) fn resolve_persisted_task_session_status(
    current_status: TaskSessionStatus,
    incoming_status: TaskSessionStatus,
) -> TaskSessionStatus {
    if current_status == TaskSessionStatus::Archived
        && incoming_status != TaskSessionStatus::Archived
    {
        return TaskSessionStatus::Archived;
    }
    if matches!(
        current_status,
        TaskSessionStatus::Completed | TaskSessionStatus::Stopped
    ) && incoming_status.is_active()
    {
        return current_status;
    }
    incoming_status
}

/// Maps TaskSession status into the Vulcan-native subagent registry status.
/// 将 TaskSession 状态映射为 Vulcan 原生子代理注册表状态。
///
/// # Parameters
/// 参数
/// `status` is the TaskSession status.
/// `status` 是 TaskSession 状态。
///
/// # Returns
/// 返回值
/// The matching subagent registry status.
/// 匹配的子代理注册表状态。
pub(super) fn subagent_status_from_task_session_status(
    status: TaskSessionStatus,
) -> SubagentStatus {
    match status {
        TaskSessionStatus::Created => SubagentStatus::Created,
        TaskSessionStatus::Running => SubagentStatus::Running,
        TaskSessionStatus::WaitingChildren => SubagentStatus::WaitingChildren,
        TaskSessionStatus::WaitingFollowup => SubagentStatus::WaitingFollowup,
        TaskSessionStatus::Completed => SubagentStatus::Completed,
        TaskSessionStatus::Failed => SubagentStatus::Failed,
        TaskSessionStatus::Stopped => SubagentStatus::Stopped,
        TaskSessionStatus::Compacting => SubagentStatus::Compacting,
        TaskSessionStatus::Archived => SubagentStatus::Archived,
    }
}

/// Returns the stable lowercase name for one TaskSession status.
/// 返回某个 TaskSession 状态的稳定小写名称。
pub(super) fn status_name_for_task_session_status(status: TaskSessionStatus) -> &'static str {
    match status {
        TaskSessionStatus::Created => "created",
        TaskSessionStatus::Running => "running",
        TaskSessionStatus::WaitingChildren => "waiting_children",
        TaskSessionStatus::WaitingFollowup => "waiting_followup",
        TaskSessionStatus::Completed => "completed",
        TaskSessionStatus::Failed => "failed",
        TaskSessionStatus::Stopped => "stopped",
        TaskSessionStatus::Compacting => "compacting",
        TaskSessionStatus::Archived => "archived",
    }
}

/// Converts a task status into a parent-side invocation event kind.
/// 将任务状态转换为父级侧调用事件类型。
pub(super) fn task_invocation_event_kind_for_status(
    status: TaskSessionStatus,
) -> TaskInvocationEventKind {
    match status {
        TaskSessionStatus::Running => TaskInvocationEventKind::TaskStarted,
        TaskSessionStatus::Completed => TaskInvocationEventKind::TaskCompleted,
        TaskSessionStatus::Failed => TaskInvocationEventKind::TaskFailed,
        TaskSessionStatus::Stopped => TaskInvocationEventKind::TaskStopped,
        TaskSessionStatus::Compacting => TaskInvocationEventKind::TaskCompacted,
        TaskSessionStatus::Created
        | TaskSessionStatus::WaitingChildren
        | TaskSessionStatus::WaitingFollowup
        | TaskSessionStatus::Archived => TaskInvocationEventKind::TaskCheckpoint,
    }
}

/// Converts a task status into a node-local event kind.
/// 将任务状态转换为节点本地事件类型。
pub(super) fn task_session_event_kind_for_status(status: TaskSessionStatus) -> SessionEventKind {
    match status {
        TaskSessionStatus::Completed | TaskSessionStatus::Stopped => SessionEventKind::TaskFeedback,
        TaskSessionStatus::Failed => SessionEventKind::RuntimeMarker,
        _ => SessionEventKind::RuntimeMarker,
    }
}

/// Builds the direct parent reference for one task record.
/// 为一条任务记录构建直接父级引用。
pub(super) fn parent_ref_for_task_record(record: &TaskSessionRecord) -> ParentSessionRef {
    let root_session_id = SessionNodeId::from(record.origin_session_id);
    record
        .parent_task_id
        .map(|parent_task_session_id| ParentSessionRef::TaskSession {
            root_session_id,
            parent_task_session_id: SessionNodeId::from(parent_task_session_id),
        })
        .unwrap_or(ParentSessionRef::MainSession { root_session_id })
}

/// Converts one concrete controller configuration into a session effective-config snapshot.
/// 将一份具体控制器配置转换为会话有效配置快照。
pub(super) fn core_controller_config_to_effective_config(
    controller_config: &CoreControllerConfig,
) -> SessionEffectiveConfig {
    let mut config = SessionEffectiveConfig {
        provider_model: ProviderModelSnapshot {
            provider_id: Some(controller_config.provider_id.clone()),
            model_id: Some(controller_config.model_id.clone()),
            reasoning_effort: None,
            execution_mode: None,
        },
        ..Default::default()
    };
    config.context_compaction.auto_compaction_enabled = true;
    if !controller_config.system_prompt.trim().is_empty() {
        config
            .system_prompt_layers
            .push(controller_config.system_prompt.clone());
    }
    config
}

/// Builds the append-only task state event payload.
/// 构建追加式任务状态事件载荷。
pub(super) fn task_state_event_payload(
    record: &TaskSessionRecord,
    invocation: &TaskExecutionRequest,
    result_text: Option<&str>,
    error_text: Option<&str>,
    status: TaskSessionStatus,
    timestamp_ms: u64,
    outcome: Option<&TaskExecutionOutcome>,
) -> Value {
    let subagent_mailbox_event = build_subagent_mailbox_event(
        invocation,
        status,
        timestamp_ms,
        result_text,
        error_text,
        outcome,
    );
    json!({
        "task_id": record.public_task_id,
        (SUBAGENT_TASK_ID_KEY): record.task_id.to_string(),
        "root_session_id": record.origin_session_id.to_string(),
        "direct_parent_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": record.parent_task_id.map(|value| value.to_string()),
        "display_title": record.display_title,
        "description": record.description,
        "subagent": record.subagent,
        "prompt": invocation.prompt,
        "result": result_text,
        "error": error_text,
        "provider_turns_executed": outcome.map(|value| value.provider_turns_executed).unwrap_or(0),
        "tool_calls_processed": outcome.map(|value| value.tool_calls_processed).unwrap_or(0),
        "final_assistant_message_id": outcome.and_then(|value| value.final_assistant_message_id.clone()),
        "subagent_mailbox_event": subagent_mailbox_event,
    })
}

/// Builds a session-boundary mailbox event for one task-session lifecycle transition.
/// 为一次子代理会话生命周期变化构建 会话边界 mailbox 事件。
pub(super) fn build_subagent_mailbox_event(
    invocation: &TaskExecutionRequest,
    status: TaskSessionStatus,
    timestamp_ms: u64,
    result_text: Option<&str>,
    error_text: Option<&str>,
    outcome: Option<&TaskExecutionOutcome>,
) -> SubagentMailboxEvent {
    SubagentMailboxEvent {
        parent_thread_id: SessionNodeId::from(invocation.current_session_id),
        parent_turn_id: None,
        child_thread_id: SessionNodeId::from(invocation.task_id),
        kind: subagent_mailbox_kind_from_task_status(status),
        timestamp_ms,
        payload: build_subagent_mailbox_payload(
            invocation,
            status,
            result_text,
            error_text,
            outcome,
        ),
    }
}

/// Builds the structured payload stored in session-boundary subagent mailbox events.
/// 构建 会话边界子代理 mailbox 事件中保存的结构化载荷。
pub(super) fn build_subagent_mailbox_payload(
    invocation: &TaskExecutionRequest,
    status: TaskSessionStatus,
    result_text: Option<&str>,
    error_text: Option<&str>,
    outcome: Option<&TaskExecutionOutcome>,
) -> Value {
    let agent_kind = subagent_kind_for_invocation(&invocation.invocation_config);
    let prompt_source = subagent_prompt_source_for_task_config(
        invocation.invocation_config.subagent_profile_id.as_deref(),
    );
    let debug = subagent_runtime_debug_payload(&invocation.invocation_config);
    json!({
        "child_thread_id": invocation.task_id.to_string(),
        "public_id": invocation.public_task_id,
        "parent_thread_id": invocation.current_session_id.to_string(),
        "root_session_id": invocation.root_session_id.to_string(),
        "parent_task_id": invocation.parent_task_id.map(|value| value.to_string()),
        "parent_turn_id": Value::Null,
        "agent_kind": agent_kind,
        "prompt_source": prompt_source,
        "debug": debug,
        "status": subagent_status_from_task_status(status),
        "task_status": status,
        "description": invocation.description,
        "display_title": invocation.display_title,
        "depth": invocation.depth,
        "prompt": invocation.prompt,
        "result_text": result_text,
        "error_text": error_text,
        "provider_turns_executed": outcome.map(|value| value.provider_turns_executed).unwrap_or(0),
        "tool_calls_processed": outcome.map(|value| value.tool_calls_processed).unwrap_or(0),
        "final_assistant_message_id": outcome.and_then(|value| value.final_assistant_message_id.clone()),
    })
}

/// Maps a TaskSession invocation into the subagent mailbox kind contract.
/// 将 TaskSession 调用映射到 会话子代理类型契约。
pub(super) fn subagent_kind_for_invocation(
    invocation: &TaskInvocationConfig,
) -> SubagentMailboxAgentKind {
    if invocation
        .subagent_profile_id
        .as_deref()
        .is_some_and(|profile_id| !profile_id.trim().is_empty())
    {
        SubagentMailboxAgentKind::AgentProfile
    } else {
        SubagentMailboxAgentKind::ParentClone
    }
}

/// Maps one task-session record into the Vulcan-native subagent kind.
/// 将一条任务会话记录映射为 Vulcan 原生子代理类型。
///
/// # Parameters
/// 参数
/// `record` is the task-session record being mirrored.
/// `record` 是正在镜像的任务会话记录。
///
/// # Returns
/// 返回值
/// The subagent kind stored in the native registry.
/// 写入原生注册表的子代理类型。
pub(super) fn subagent_kind_for_task_record(record: &TaskSessionRecord) -> SubagentKind {
    subagent_kind_for_task_config(
        record.invocation_config.subagent_profile_id.as_deref(),
        record.invocation_config.system_prompt.as_deref(),
    )
}

/// Maps task configuration identity fields into the Vulcan-native subagent kind.
/// 将任务配置身份字段映射为 Vulcan 原生子代理类型。
///
/// # Parameters
/// 参数
/// `profile_id` is the optional catalog-backed subagent profile identifier.
/// `profile_id` 是可选的目录驱动子代理配置标识。
/// `system_prompt` is the optional custom replacement system prompt.
/// `system_prompt` 是可选的自定义替换系统提示词。
///
/// # Returns
/// 返回值
/// The native registry kind implied by the explicit profile or custom prompt configuration.
/// 返回由显式配置或自定义提示词配置推导出的原生注册表类型。
pub(super) fn subagent_kind_for_task_config(
    profile_id: Option<&str>,
    system_prompt: Option<&str>,
) -> SubagentKind {
    if profile_id.is_some_and(|profile_id| !profile_id.trim().is_empty()) {
        return SubagentKind::AgentMd;
    }
    if system_prompt.is_some_and(|system_prompt| !system_prompt.trim().is_empty()) {
        return SubagentKind::Custom;
    }
    SubagentKind::Clone
}

/// Resolves a prompt-source label for one task-session record.
/// 解析一条任务会话记录的提示词来源标签。
///
/// # Parameters
/// 参数
/// `record` is the task-session record being mirrored.
/// `record` 是正在镜像的任务会话记录。
///
/// # Returns
/// 返回值
/// The stable source marker for this subagent task.
/// 该子代理任务的稳定提示词来源标记。
pub(super) fn subagent_prompt_source_for_task_record(record: &TaskSessionRecord) -> String {
    subagent_prompt_source_for_task_config(record.invocation_config.subagent_profile_id.as_deref())
}

/// Resolves a prompt-source label from task configuration identity.
/// 从任务配置身份解析提示词来源标签。
///
/// # Parameters
/// 参数
/// `profile_id` is the optional catalog-backed subagent profile identifier.
/// `profile_id` 是可选的目录驱动子代理配置标识。
///
/// # Returns
/// 返回值
/// `agent_md:{profile_id}` for catalog-backed agents, otherwise `core`.
/// 对目录驱动代理返回 `agent_md:{profile_id}`，否则返回 `core`。
pub(super) fn subagent_prompt_source_for_task_config(profile_id: Option<&str>) -> String {
    profile_id
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty())
        .map(|profile_id| format!("agent_md:{profile_id}"))
        .unwrap_or_else(|| String::from("core"))
}

/// Maps a persisted TaskSession status into the subagent mailbox status contract.
/// 将持久化 TaskSession 状态映射到 会话子代理状态契约。
pub(super) fn subagent_status_from_task_status(status: TaskSessionStatus) -> SubagentMailboxStatus {
    match status {
        TaskSessionStatus::Completed => SubagentMailboxStatus::Completed,
        TaskSessionStatus::Failed => SubagentMailboxStatus::Failed,
        TaskSessionStatus::Stopped => SubagentMailboxStatus::Stopped,
        TaskSessionStatus::Created
        | TaskSessionStatus::Running
        | TaskSessionStatus::WaitingChildren
        | TaskSessionStatus::WaitingFollowup
        | TaskSessionStatus::Compacting
        | TaskSessionStatus::Archived => SubagentMailboxStatus::Running,
    }
}

/// Maps a TaskSession status into the parent mailbox event kind.
/// 将 TaskSession 状态映射到 父级 mailbox 事件类型。
pub(super) fn subagent_mailbox_kind_from_task_status(
    status: TaskSessionStatus,
) -> SubagentMailboxEventKind {
    match status {
        TaskSessionStatus::Completed => SubagentMailboxEventKind::Completed,
        TaskSessionStatus::Failed => SubagentMailboxEventKind::Failed,
        TaskSessionStatus::Stopped => SubagentMailboxEventKind::Stopped,
        TaskSessionStatus::Created | TaskSessionStatus::Running => {
            SubagentMailboxEventKind::Started
        }
        TaskSessionStatus::WaitingChildren
        | TaskSessionStatus::WaitingFollowup
        | TaskSessionStatus::Compacting
        | TaskSessionStatus::Archived => SubagentMailboxEventKind::Message,
    }
}

/// Builds a compact result summary suitable for task-session metadata.
/// 构建适合 TaskSession 元数据使用的紧凑结果摘要。
pub(super) fn task_result_summary(result_text: &str) -> Option<String> {
    let summary = result_text.trim();
    if summary.is_empty() {
        return None;
    }
    const MAX_SUMMARY_CHARS: usize = 240;
    let mut chars = summary.chars();
    let clipped: String = chars.by_ref().take(MAX_SUMMARY_CHARS).collect();
    if chars.next().is_some() {
        Some(format!("{clipped}..."))
    } else {
        Some(clipped)
    }
}
