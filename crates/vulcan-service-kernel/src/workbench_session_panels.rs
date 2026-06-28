//! workbench session panels helpers.
//! workbench session panels 辅助逻辑。

use super::*;

/// Projects session todolist state into the WebUI snapshot.
/// 将会话 todolist 状态投影到 WebUI 快照。
///
/// # Parameters
/// 参数
/// `runtime_state` is the selected session runtime state when available.
/// `runtime_state` 是可用时的当前选中会话运行态。
///
/// # Returns
/// 返回值
/// A todolist summary when the selected session has visible items.
/// 当前选中会话存在可见条目时返回 todolist 摘要。
pub(crate) fn session_todolist_summary_from_runtime_state(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Option<SessionTodolistSummary> {
    let todolist = &runtime_state?.todolist;
    if todolist.is_empty() || todolist.items.is_empty() {
        return None;
    }
    let items = todolist
        .items
        .iter()
        .map(|item| SessionTodolistItemSummary {
            step: item.step.clone(),
            status: session_todolist_item_status_key(item.status).to_string(),
        })
        .collect::<Vec<_>>();
    let completed_count = todolist
        .items
        .iter()
        .filter(|item| item.status == SessionTodolistItemStatus::Completed)
        .count();
    Some(SessionTodolistSummary {
        explanation: todolist.explanation.clone(),
        total_count: items.len(),
        completed_count,
        items,
        updated_at: todolist.updated_at.clone(),
    })
}

/// Converts selected-session Plan mode runtime state into a WebUI summary.
/// 将当前选中会话的计划模式运行态转换为 WebUI 摘要。
///
/// # Parameters
/// 参数
/// `runtime_state` is the optional selected-session runtime entry.
/// `runtime_state` 是可选的当前选中会话运行态条目。
///
/// # Returns
/// 返回值
/// The Plan mode summary when the selected session is in Plan mode or has a plan artifact.
/// 当选中会话处于计划模式或拥有计划产物时返回计划模式摘要。
pub(crate) fn session_plan_mode_summary_from_runtime_state(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Option<SessionPlanModeSummary> {
    let runtime_state = runtime_state?;
    let execution_mode = runtime_state
        .active_work_mode
        .as_deref()
        .unwrap_or("build")
        .trim()
        .to_string();
    let is_plan = execution_mode.eq_ignore_ascii_case("plan");
    if !is_plan && runtime_state.plan_mode.is_empty() {
        return None;
    }
    Some(SessionPlanModeSummary {
        execution_mode,
        prompt_injected: runtime_state.plan_mode.prompt_injected,
        artifact: runtime_state
            .plan_mode
            .artifact
            .as_ref()
            .map(session_plan_artifact_summary_from_state),
    })
}

/// Converts one persisted Plan artifact state into a WebUI summary.
/// 将单个持久化计划产物状态转换为 WebUI 摘要。
///
/// # Parameters
/// 参数
/// `artifact` is the persisted artifact payload.
/// `artifact` 是持久化的产物载荷。
///
/// # Returns
/// 返回值
/// A WebUI-facing artifact summary with stable field names.
/// 字段名称稳定的 WebUI 产物摘要。
pub(crate) fn session_plan_artifact_summary_from_state(
    artifact: &SessionPlanArtifactState,
) -> SessionPlanArtifactSummary {
    SessionPlanArtifactSummary {
        name: artifact.name.clone(),
        path: artifact.path.clone(),
        content: artifact.content.clone(),
        updated_at: artifact.updated_at.clone(),
    }
}

/// Returns the stable WebUI key for one todolist item status.
/// 返回单个 todolist 条目状态的稳定 WebUI 键。
///
/// # Parameters
/// 参数
/// `status` is the internal todolist item lifecycle status.
/// `status` 是内部 todolist 条目生命周期状态。
///
/// # Returns
/// 返回值
/// The snake_case status key consumed by WebUI.
/// WebUI 消费的 snake_case 状态键。
pub(crate) fn session_todolist_item_status_key(status: SessionTodolistItemStatus) -> &'static str {
    match status {
        SessionTodolistItemStatus::Pending => "pending",
        SessionTodolistItemStatus::InProgress => "in_progress",
        SessionTodolistItemStatus::Completed => "completed",
    }
}

/// Projects the oldest pending ask-tool request from the shared runtime for WebUI display.
/// 从共享运行时投影最早的待处理 ask 工具请求供 WebUI 展示。
pub(crate) fn pending_question_snapshot_from_agent_runtime(
    agent_runtime: Option<&AgentRuntimeBridge>,
) -> Option<PendingQuestionSnapshot> {
    let question_runtime = agent_runtime.map(|runtime| runtime.question_runtime.as_ref())?;
    let mut pending = question_runtime.list_pending().ok()?;
    let request = pending.drain(..).next()?;
    Some(PendingQuestionSnapshot {
        id: request.id,
        session_id: request.session_id.to_string(),
        sequence: request.sequence,
        questions: request
            .questions
            .into_iter()
            .map(|question| {
                let multiple = question.allows_multiple();
                PendingQuestionPromptSummary {
                    question: question.question,
                    header: question.header,
                    options: question
                        .options
                        .into_iter()
                        .map(|option| PendingQuestionOptionSummary {
                            label: option.label,
                            description: option.description,
                        })
                        .collect(),
                    multiple,
                }
            })
            .collect(),
    })
}

/// Builds the selected-session context injection side-panel snapshot.
/// 构建当前选中会话的上下文注入侧面板快照。
pub(crate) fn context_injection_snapshot_from_runtime_state(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Option<ContextInjectionSnapshot> {
    let runtime_state = runtime_state?;
    let dynamic_cards = context_injection_cards_to_summary(context_injection_panel_cards_for_kind(
        &runtime_state.context_injection_state,
        Some(InjectionKind::Transient),
    ));
    let static_profile = static_vmm_profile_snapshot(runtime_state);
    let mut static_cards = Vec::new();
    static_cards.extend(context_injection_cards_to_summary(
        context_injection_panel_cards_for_kind(
            &runtime_state.context_injection_state,
            Some(InjectionKind::Persistent),
        ),
    ));

    Some(ContextInjectionSnapshot {
        dynamic_cards,
        static_profile,
        static_cards,
    })
}

/// Converts shared injection panel cards into service API DTOs.
/// 将共享注入面板卡片转换为 service API DTO。
pub(crate) fn context_injection_cards_to_summary(
    cards: Vec<InjectionPanelCard>,
) -> Vec<ContextInjectionCardSummary> {
    cards
        .into_iter()
        .map(|card| ContextInjectionCardSummary {
            tag: card.tag,
            description: card.description,
            items: card
                .items
                .into_iter()
                .map(|item| ContextInjectionItemSummary {
                    dedupe_key: item.dedupe_key,
                    memory_id: item
                        .memory_id
                        .or_else(|| context_injection_memory_id_from_detail(&item.detail)),
                    turn_id: item.turn_id,
                    score_label: item.score_label,
                    has_dialogue: item.has_dialogue,
                    created_datetime: item.created_datetime,
                    updated_at: item.updated_at,
                    content: item.content,
                    detail: item.detail,
                    turn_label: item.turn_label,
                    ttl_label: item.ttl_label,
                    source_display: item.source_display,
                })
                .collect(),
        })
        .collect()
}

/// Extracts a durable VMM memory id from rendered VMM panel detail text.
/// 从已渲染的 VMM 面板详情文本中提取长期 VMM 记忆标识。
pub(crate) fn context_injection_memory_id_from_detail(detail: &str) -> Option<u64> {
    detail.lines().find_map(|line| {
        let value = line.trim().strip_prefix("记忆 ID：")?;
        value
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|memory_id| *memory_id > 0)
    })
}

/// Builds the same compact multi-line preview used by the WebUI panel.
/// 构建与 WebUI 面板一致的多行紧凑预览。
pub(crate) fn compact_panel_preview(content: &str) -> String {
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(7)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() > 6 {
        lines.truncate(6);
        lines.push(String::from("..."));
    }
    lines.join("\n")
}
