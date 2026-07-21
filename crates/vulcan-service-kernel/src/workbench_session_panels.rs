//! workbench session panels helpers.
//! workbench session panels 辅助逻辑。

use super::*;
use vulcan_agent_runtime::agent_runtime::question_runtime::QuestionRequest;
use vulcan_service_api::{ConversationAttentionSummary, ConversationSummary};

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
    let mut items = Vec::with_capacity(todolist.items.len());
    let mut completed_count = 0;
    for item in &todolist.items {
        if item.status == SessionTodolistItemStatus::Completed {
            completed_count += 1;
        }
        items.push(SessionTodolistItemSummary {
            step: item.step.clone(),
            status: session_todolist_item_status_key(item.status).to_string(),
        });
    }
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

/// Converts session VMM policy layers into a WebUI summary.
/// 将会话 VMM 策略层转换为 WebUI 摘要。
///
/// # Parameters
/// 参数
/// `global_layer` is the system-level VMM configuration layer.
/// `global_layer` 是系统级 VMM 配置层。
/// `project_layer` is the project-level VMM configuration layer.
/// `project_layer` 是项目级 VMM 配置层。
/// `runtime_state` is the optional selected-session runtime entry.
/// `runtime_state` 是可选的当前选中会话运行态条目。
///
/// # Returns
/// 返回值
/// The session VMM policy summary with final effective booleans.
/// 返回带最终布尔值的会话 VMM 策略摘要。
pub(crate) fn session_vmm_policy_summary_from_layers(
    global_layer: &vulcan_vmm::VmmConfigLayer,
    project_layer: Option<&vulcan_vmm::VmmConfigLayer>,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> SessionVmmPolicySummary {
    let session_layer = runtime_state.map(|runtime_state| &runtime_state.vmm);
    let empty_session_layer = vulcan_vmm::VmmConfigLayer::default();
    let summary_layer = session_layer.unwrap_or(&empty_session_layer);
    let effective =
        vulcan_vmm::VmmEffectiveConfig::resolve(global_layer, project_layer, session_layer);
    SessionVmmPolicySummary {
        precheck_mode: vmm_policy_mode_key(summary_layer.precheck_enabled),
        precheck_enabled: effective.precheck_enabled,
        memory_writeback_mode: vmm_policy_mode_key(summary_layer.memory_writeback_enabled),
        memory_writeback_enabled: effective.memory_writeback_enabled,
    }
}

/// Builds one retained session-addons list entry when it has visible content.
/// 当会话附属保留列表项存在可见内容时构建该条目。
///
/// # Parameters
/// 参数
/// `conversation_id` is the conversation id used by retained Workbench lists.
/// `conversation_id` 是保留工作台列表使用的会话标识。
/// `session_goal` is the optional goal summary for the session.
/// `session_goal` 是该会话的可选目标摘要。
/// `session_todolist` is the optional todolist summary for the session.
/// `session_todolist` 是该会话的可选待办摘要。
/// `session_plan_mode` is the optional Plan mode summary for the session.
/// `session_plan_mode` 是该会话的可选计划模式摘要。
/// `session_vmm_policy` is the optional non-empty session VMM policy list projection.
/// `session_vmm_policy` 是可选的非空会话 VMM 策略列表投影。
///
/// # Returns
/// 返回值
/// A retained list delta when any addon panel should be shown.
/// 当任意附属面板需要展示时返回保留列表增量。
pub(crate) fn session_addons_changed_delta_for_retained_list(
    conversation_id: String,
    session_goal: Option<SessionGoalSummary>,
    session_todolist: Option<SessionTodolistSummary>,
    session_plan_mode: Option<SessionPlanModeSummary>,
    session_vmm_policy: Option<SessionVmmPolicySummary>,
) -> Option<SessionAddonsChangedDelta> {
    if session_goal.is_none()
        && session_todolist.is_none()
        && session_plan_mode.is_none()
        && session_vmm_policy.is_none()
    {
        return None;
    }
    Some(SessionAddonsChangedDelta {
        conversation_id,
        session_goal,
        session_todolist,
        session_plan_mode,
        session_vmm_policy,
    })
}

/// Converts one optional VMM policy override into a tri-state mode key.
/// 将单个可选 VMM 策略覆盖转换为三态模式键。
///
/// # Parameters
/// 参数
/// `value` is the optional override value from a configuration layer.
/// `value` 是配置层中的可选覆盖值。
///
/// # Returns
/// 返回值
/// The stable WebUI mode key.
/// 返回稳定的 WebUI 模式键。
fn vmm_policy_mode_key(value: Option<bool>) -> String {
    match value {
        None => String::from("inherit"),
        Some(true) => String::from("enabled"),
        Some(false) => String::from("disabled"),
    }
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

/// Projects every pending ask-tool request from the shared runtime for per-session display.
/// 从共享运行时投影所有待处理 ask 工具请求，供按会话展示。
///
/// # Parameters
/// 参数
/// `agent_runtime` is the optional shared runtime bridge that owns the question runtime.
/// `agent_runtime` 是可选的共享运行时桥，持有提问运行时。
///
/// # Returns
/// 返回值
/// Returns ordered pending ask summaries, or an explicit runtime projection error.
/// 返回按运行时序号排序的待处理 ask 摘要，失败时返回明确的运行时投影错误。
pub(crate) fn pending_question_snapshots_from_agent_runtime(
    agent_runtime: Option<&AgentRuntimeBridge>,
) -> Result<Vec<PendingQuestionSnapshot>, String> {
    let Some(question_runtime) = agent_runtime.map(|runtime| runtime.question_runtime.as_ref())
    else {
        return Ok(Vec::new());
    };
    project_pending_question_requests(question_runtime.list_pending())
}

/// Converts one authoritative pending-request read into ordered WebUI snapshots.
/// 将一次权威 pending 请求读取转换为有序 WebUI 快照。
///
/// # Parameters
/// 参数
/// `pending_requests` is the exact QuestionRuntime read result to project without fallback.
/// `pending_requests` 是不得回退的精确 QuestionRuntime 读取结果。
///
/// # Returns
/// 返回值
/// Returns ordered snapshots for a successful read, or a contextualized registry-read error.
/// 读取成功时返回有序快照，失败时返回带上下文的注册表读取错误。
fn project_pending_question_requests(
    pending_requests: Result<Vec<QuestionRequest>, String>,
) -> Result<Vec<PendingQuestionSnapshot>, String> {
    // snapshots contains only facts read successfully from the authoritative question registry.
    // snapshots 仅包含从权威问题注册表成功读取的事实。
    let mut snapshots = pending_requests
        .map_err(|error| format!("failed to project pending ask requests: {error}"))?
        .into_iter()
        .map(|request| PendingQuestionSnapshot {
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
        .collect::<Vec<_>>();
    snapshots.sort_by_key(|snapshot| snapshot.sequence);
    Ok(snapshots)
}

/// Builds the highest-priority sidebar attention marker for one conversation.
/// 为单个会话构建最高优先级侧栏注意力标记。
///
/// # Parameters
/// 参数
/// `session_id` is the conversation session id.
/// `session_id` 是会话标识。
/// `pending_questions` contains all current ask requests.
/// `pending_questions` 包含当前所有 ask 请求。
/// `session_plan_mode` is the optional Plan mode projection for this session.
/// `session_plan_mode` 是该会话的可选计划模式投影。
/// `unread` indicates whether the session has unread completed activity.
/// `unread` 表示该会话是否存在未读完成活动。
/// `updated_at` is the session update timestamp used for diagnostics.
/// `updated_at` 是用于诊断的会话更新时间戳。
///
/// # Returns
/// 返回值
/// The marker clients should show in the sidebar, if any.
/// 客户端应在侧栏展示的标记；没有标记时为空。
pub(crate) fn conversation_attention_summary_for_session(
    session_id: SessionId,
    pending_questions: &[PendingQuestionSnapshot],
    session_plan_mode: Option<&SessionPlanModeSummary>,
    unread: bool,
    updated_at: Option<String>,
) -> Option<ConversationAttentionSummary> {
    if let Some(question) = pending_questions
        .iter()
        .find(|question| question.session_id == session_id.to_string())
    {
        return Some(conversation_attention_summary(
            "ask",
            90,
            Some(question.id.clone()),
            "Ask question waiting",
            updated_at,
        ));
    }
    if session_plan_mode.is_some_and(|summary| summary.execution_mode.eq_ignore_ascii_case("plan"))
    {
        return Some(conversation_attention_summary(
            "plan_confirm",
            80,
            session_plan_mode
                .and_then(|summary| summary.artifact.as_ref())
                .map(|artifact| artifact.path.clone()),
            "Plan mode waiting",
            updated_at,
        ));
    }
    if unread {
        return Some(conversation_attention_summary(
            "unread",
            10,
            None,
            "Unread activity",
            updated_at,
        ));
    }
    None
}

/// Refreshes a conversation summary with the highest-priority sidebar attention marker.
/// 使用最高优先级侧栏注意力标记刷新会话摘要。
///
/// # Parameters
/// 参数
/// `conversation` is the mutable WebUI conversation summary to update.
/// `conversation` 是要更新的可变 WebUI 会话摘要。
/// `session_id` identifies the conversation session.
/// `session_id` 标识会话。
/// `pending_questions` contains all current ask requests.
/// `pending_questions` 包含当前所有 ask 请求。
/// `session_plan_mode` is the optional Plan mode projection for this session.
/// `session_plan_mode` 是该会话的可选计划模式投影。
pub(crate) fn refresh_conversation_attention_summary(
    conversation: &mut ConversationSummary,
    session_id: SessionId,
    pending_questions: &[PendingQuestionSnapshot],
    session_plan_mode: Option<&SessionPlanModeSummary>,
) {
    // Normalize empty timestamps before they reach the WebUI diagnostic payload.
    // 在进入 WebUI 诊断载荷前归一化空时间戳。
    let updated_at =
        (!conversation.last_updated_at.is_empty()).then(|| conversation.last_updated_at.clone());
    conversation.attention = conversation_attention_summary_for_session(
        session_id,
        pending_questions,
        session_plan_mode,
        conversation.unread,
        updated_at,
    );
}

/// Replaces one retained conversation-scoped projection entry.
/// 替换单条按会话归属保留的投影条目。
///
/// # Parameters
/// 参数
/// `entries` is the retained per-conversation projection list.
/// `entries` 是按会话保留的投影列表。
/// `conversation_id` identifies the conversation whose entry should be replaced.
/// `conversation_id` 标识需要替换投影条目的会话。
/// `next_entry` is the replacement entry, or `None` when the projection should be cleared.
/// `next_entry` 是替换条目；为 `None` 时表示清除该投影。
/// `entry_conversation_id` reads the owning conversation id from an existing entry.
/// `entry_conversation_id` 从已有条目读取所属会话标识。
pub(crate) fn replace_conversation_scoped_projection_entry<Entry>(
    entries: &mut Vec<Entry>,
    conversation_id: &str,
    next_entry: Option<Entry>,
    entry_conversation_id: impl Fn(&Entry) -> &str,
) {
    entries.retain(|entry| entry_conversation_id(entry) != conversation_id);
    if let Some(entry) = next_entry {
        entries.push(entry);
    }
}

/// Builds one conversation attention marker with shared field conversion.
/// 使用共享字段转换构建单个会话注意力标记。
///
/// # Parameters
/// 参数
/// `kind` is the stable attention marker key.
/// `kind` 是稳定的注意力标记键。
/// `priority` is the marker ordering priority.
/// `priority` 是标记排序优先级。
/// `request_id` identifies the owning request or artifact when available.
/// `request_id` 在可用时标识所属请求或产物。
/// `label` is the user-facing marker label.
/// `label` 是面向用户的标记标签。
/// `updated_at` carries the source session update timestamp.
/// `updated_at` 携带来源会话更新时间。
///
/// # Returns
/// 返回值
/// A conversation attention summary ready for WebUI projection.
/// 返回可用于 WebUI 投影的会话注意力摘要。
fn conversation_attention_summary(
    kind: &str,
    priority: u8,
    request_id: Option<String>,
    label: &str,
    updated_at: Option<String>,
) -> ConversationAttentionSummary {
    ConversationAttentionSummary {
        kind: kind.to_string(),
        priority,
        request_id,
        label: Some(label.to_string()),
        updated_at,
    }
}

/// Builds the selected-session context injection side-panel snapshot.
/// 构建当前选中会话的上下文注入侧面板快照。
pub(crate) fn context_injection_snapshot_from_runtime_state(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Option<ContextInjectionSnapshot> {
    let runtime_state = runtime_state?;
    let mut dynamic_panel_cards = runtime_state
        .vmm_session_memory
        .active_panel_card()
        .into_iter()
        .collect::<Vec<_>>();
    dynamic_panel_cards.extend(context_injection_panel_cards_for_kind(
        &runtime_state.context_injection_state,
        Some(InjectionKind::Transient),
    ));
    // Expired VMM residents are appended last so the WebUI presents them in a dedicated bottom region.
    // 过期 VMM 驻留项最后追加，使 WebUI 将其展示在独立的底部区域。
    dynamic_panel_cards.extend(runtime_state.vmm_session_memory.expired_panel_card());
    let dynamic_cards = context_injection_cards_to_summary(dynamic_panel_cards);
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

/// Builds one context-injection delta from optional session runtime state.
/// 从可选会话运行态构建单条上下文注入增量。
///
/// # Parameters
/// 参数
/// `session_id` identifies the conversation that owns the memory panel.
/// `session_id` 标识拥有记忆面板的会话。
/// `runtime_state` is the optional retained runtime entry for that conversation.
/// `runtime_state` 是该会话的可选保留运行态条目。
///
/// # Returns
/// 返回值
/// A delta whose `context_injection` is `None` when the runtime entry has no panel state.
/// 返回一条增量；当运行态没有面板状态时 `context_injection` 为 `None`。
pub(crate) fn context_injection_changed_delta_from_runtime_state(
    session_id: SessionId,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> ContextInjectionChangedDelta {
    ContextInjectionChangedDelta {
        conversation_id: session_id.to_string(),
        context_injection: context_injection_snapshot_from_runtime_state(runtime_state),
    }
}

/// Builds retained context injection projections for hydrated conversations.
/// 为已水合会话构建保留的上下文注入投影。
///
/// # Parameters
/// 参数
/// `workspace` provides session runtime states and hydration ownership.
/// `workspace` 提供会话运行态与水合归属信息。
///
/// # Returns
/// 返回值
/// Per-conversation memory-panel projections with empty runtime entries omitted.
/// 返回按会话归属的记忆面板投影，并省略缺少运行态的条目。
pub(crate) fn context_injection_by_conversation_from_workspace(
    workspace: &WorkspaceSnapshot,
) -> Vec<ContextInjectionChangedDelta> {
    let mut session_ids = workspace.hydrated_session_ids.clone();
    session_ids.insert(workspace.selected_session);
    session_ids
        .iter()
        .filter_map(|session_id| {
            let delta = context_injection_changed_delta_from_runtime_state(
                *session_id,
                workspace.session_runtime_state.get(session_id),
            );
            delta.context_injection.is_some().then_some(delta)
        })
        .collect()
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
    // Accumulates preview text directly instead of buffering line strings.
    // 直接累积预览文本，避免缓存行字符串。
    let mut preview = String::new();
    // Counts inspected visible lines, including the seventh truncation sentinel.
    // 统计已检查的可见行，包含用于截断判断的第七行哨兵。
    let mut inspected_lines = 0usize;

    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        inspected_lines += 1;
        if inspected_lines > 6 {
            if !preview.is_empty() {
                preview.push('\n');
            }
            preview.push_str("...");
            break;
        }
        if !preview.is_empty() {
            preview.push('\n');
        }
        preview.push_str(line);
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_service_api::PendingPromptsChangedDelta;
    use vulcan_vmm::{ContextItem, VmmRecallBatch};

    /// Verifies a pending-question registry failure cannot be projected as an empty request list.
    /// 验证待回答问题注册表失败无法被投影为空请求列表。
    #[test]
    fn pending_question_projection_preserves_registry_failure() {
        // error is the contextualized Service Kernel failure returned for an authoritative read error.
        // error 是权威读取失败时返回的带上下文 Service Kernel 错误。
        let error = project_pending_question_requests(Err(String::from(
            "The question runtime registry is locked.",
        )))
        .expect_err("registry failure must not become an empty pending-question list");

        assert!(error.contains("failed to project pending ask requests"));
        assert!(error.contains("question runtime registry is locked"));
    }

    /// Builds one pending question snapshot for attention-priority tests.
    /// 为注意力优先级测试构建单个待回答问题快照。
    fn test_pending_question(session_id: SessionId) -> PendingQuestionSnapshot {
        PendingQuestionSnapshot {
            id: String::from("question-1"),
            session_id: session_id.to_string(),
            sequence: 1,
            questions: Vec::new(),
        }
    }

    /// Builds one plan-mode summary with a request-identifying artifact path.
    /// 构建带有请求标识产物路径的计划模式摘要。
    fn test_plan_summary(execution_mode: &str) -> SessionPlanModeSummary {
        SessionPlanModeSummary {
            execution_mode: execution_mode.to_string(),
            prompt_injected: true,
            artifact: Some(SessionPlanArtifactSummary {
                name: String::from("Plan"),
                path: String::from("docs/plan/test.md"),
                content: None,
                updated_at: String::from("1780000000000"),
            }),
        }
    }

    /// Builds one conversation summary for attention refresh tests.
    /// 为注意力刷新测试构建单个会话摘要。
    fn test_conversation_summary(unread: bool, last_updated_at: &str) -> ConversationSummary {
        ConversationSummary {
            id: String::from("session-1"),
            title: String::from("Session"),
            status: String::from("idle"),
            run_phase: String::from("idle"),
            last_updated_label: String::from("now"),
            last_updated_at: last_updated_at.to_string(),
            unread,
            attention: None,
            project_id: None,
            fork_source: None,
        }
    }

    /// Verifies conversation-scoped retained projections replace and clear matching entries.
    /// 验证按会话归属保留的投影会替换和清除匹配条目。
    #[test]
    fn replace_conversation_scoped_projection_entry_replaces_and_clears_matches() {
        // entries stores retained pending-prompt projections for multiple conversations.
        // entries 保存多个会话的待发送提示词保留投影。
        let mut entries = vec![
            PendingPromptsChangedDelta {
                conversation_id: String::from("conversation-a"),
                pending_prompts: Vec::new(),
            },
            PendingPromptsChangedDelta {
                conversation_id: String::from("conversation-b"),
                pending_prompts: Vec::new(),
            },
        ];

        replace_conversation_scoped_projection_entry(
            &mut entries,
            "conversation-a",
            Some(PendingPromptsChangedDelta {
                conversation_id: String::from("conversation-a"),
                pending_prompts: Vec::new(),
            }),
            |entry| entry.conversation_id.as_str(),
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.conversation_id == "conversation-a")
                .count(),
            1
        );
        assert_eq!(
            entries.last().map(|entry| entry.conversation_id.as_str()),
            Some("conversation-a")
        );

        replace_conversation_scoped_projection_entry(
            &mut entries,
            "conversation-b",
            None,
            |entry| entry.conversation_id.as_str(),
        );

        assert!(
            entries
                .iter()
                .all(|entry| entry.conversation_id != "conversation-b")
        );
    }

    /// Verifies retained session-addons entries are omitted when all panels are empty.
    /// 验证所有面板为空时会省略保留会话附属条目。
    #[test]
    fn session_addons_changed_delta_for_retained_list_omits_empty_entry() {
        // entry stores the retained-list projection when any addon content exists.
        // entry 保存存在任意附属内容时的保留列表投影。
        let entry = session_addons_changed_delta_for_retained_list(
            String::from("conversation-a"),
            None,
            None,
            None,
            None,
        );

        assert!(entry.is_none());
    }

    /// Verifies retained session-addons entries preserve non-empty panel summaries.
    /// 验证保留会话附属条目会保留非空面板摘要。
    #[test]
    fn session_addons_changed_delta_for_retained_list_keeps_plan_entry() {
        // plan_mode identifies the addon panel that should keep the retained entry visible.
        // plan_mode 标识应保持保留条目可见的附属面板。
        let plan_mode = test_plan_summary("plan");
        // entry stores the retained-list projection built from the shared helper.
        // entry 保存共享辅助函数构建出的保留列表投影。
        let entry = session_addons_changed_delta_for_retained_list(
            String::from("conversation-a"),
            None,
            None,
            Some(plan_mode),
            None,
        )
        .expect("plan mode should produce a retained addon entry");

        assert_eq!(entry.conversation_id, "conversation-a");
        assert!(entry.session_goal.is_none());
        assert!(entry.session_todolist.is_none());
        assert_eq!(
            entry
                .session_plan_mode
                .as_ref()
                .map(|summary| summary.execution_mode.as_str()),
            Some("plan")
        );
        assert!(entry.session_vmm_policy.is_none());
    }

    /// Verifies compact panel previews trim blank lines.
    /// 验证紧凑面板预览会裁剪空行。
    #[test]
    fn compact_panel_preview_trims_blank_lines() {
        // content represents rendered panel text with indentation and blank separators.
        // content 表示带缩进和空白分隔符的已渲染面板文本。
        let content = "  first line  \n\n\tsecond line\n   \n third line ";

        assert_eq!(
            compact_panel_preview(content),
            "first line\nsecond line\nthird line"
        );
    }

    /// Verifies the seventh visible line becomes the compact-preview truncation sentinel.
    /// 验证第七条可见行会变成紧凑预览的截断哨兵。
    #[test]
    fn compact_panel_preview_uses_seventh_visible_line_as_truncation_sentinel() {
        // content contains seven visible lines so the preview should keep six lines and append dots.
        // content 包含七条可见行，因此预览应保留六条并追加省略号。
        let content = "one\ntwo\nthree\nfour\nfive\nsix\nseven";

        assert_eq!(
            compact_panel_preview(content),
            "one\ntwo\nthree\nfour\nfive\nsix\n..."
        );
    }

    /// Verifies conversation attention uses ask, then plan, then unread priority.
    /// 验证会话注意力按 ask、plan、unread 的优先级生成。
    #[test]
    fn conversation_attention_summary_uses_ask_plan_unread_priority() {
        let session_id = SessionId::new_v4();
        let updated_at = Some(String::from("1780000000000"));
        let pending_questions = vec![test_pending_question(session_id)];
        let plan_summary = test_plan_summary("plan");

        let ask_attention = conversation_attention_summary_for_session(
            session_id,
            &pending_questions,
            Some(&plan_summary),
            true,
            updated_at.clone(),
        )
        .expect("ask attention should be produced");
        assert_eq!(ask_attention.kind, "ask");
        assert_eq!(ask_attention.priority, 90);
        assert_eq!(ask_attention.request_id.as_deref(), Some("question-1"));
        assert_eq!(ask_attention.updated_at, updated_at);

        let plan_attention = conversation_attention_summary_for_session(
            session_id,
            &[],
            Some(&plan_summary),
            true,
            None,
        )
        .expect("plan attention should be produced");
        assert_eq!(plan_attention.kind, "plan_confirm");
        assert_eq!(plan_attention.priority, 80);
        assert_eq!(
            plan_attention.request_id.as_deref(),
            Some("docs/plan/test.md")
        );

        let build_summary = test_plan_summary("build");
        let unread_attention = conversation_attention_summary_for_session(
            session_id,
            &[],
            Some(&build_summary),
            true,
            None,
        )
        .expect("unread attention should be produced");
        assert_eq!(unread_attention.kind, "unread");
        assert_eq!(unread_attention.priority, 10);

        assert!(
            conversation_attention_summary_for_session(
                session_id,
                &[],
                Some(&build_summary),
                false,
                None,
            )
            .is_none()
        );
    }

    /// Verifies the shared refresh helper updates summaries and normalizes empty timestamps.
    /// 验证共享刷新辅助函数会更新摘要并归一化空时间戳。
    #[test]
    fn refresh_conversation_attention_summary_updates_summary_from_runtime_inputs() {
        // Use one stable session id for the pending ask lookup.
        // 使用一个稳定会话标识匹配待回答 ask。
        let session_id = SessionId::new_v4();
        // Keep an ask request present to verify the highest-priority branch.
        // 保留一个 ask 请求以验证最高优先级分支。
        let pending_questions = vec![test_pending_question(session_id)];
        // Keep a Plan summary present to prove ask still wins after the helper extraction.
        // 保留计划摘要以证明抽取辅助函数后 ask 仍然优先。
        let plan_summary = test_plan_summary("plan");
        // Start with an empty timestamp to verify the shared normalization behavior.
        // 使用空时间戳启动以验证共享归一化行为。
        let mut conversation = test_conversation_summary(true, "");

        refresh_conversation_attention_summary(
            &mut conversation,
            session_id,
            &pending_questions,
            Some(&plan_summary),
        );

        assert_eq!(
            conversation
                .attention
                .as_ref()
                .map(|attention| attention.kind.as_str()),
            Some("ask")
        );
        assert_eq!(
            conversation
                .attention
                .as_ref()
                .and_then(|attention| attention.updated_at.as_deref()),
            None
        );

        // Use build mode so unread is the only remaining attention source.
        // 使用 build 模式让未读状态成为唯一剩余注意力来源。
        let build_summary = test_plan_summary("build");
        // Keep a concrete timestamp to verify non-empty values are preserved.
        // 保留具体时间戳以验证非空值会被保留。
        let mut unread_conversation = test_conversation_summary(true, "1780000000000");

        refresh_conversation_attention_summary(
            &mut unread_conversation,
            session_id,
            &[],
            Some(&build_summary),
        );

        assert_eq!(
            unread_conversation
                .attention
                .as_ref()
                .map(|attention| attention.kind.as_str()),
            Some("unread")
        );
        assert_eq!(
            unread_conversation
                .attention
                .as_ref()
                .and_then(|attention| attention.updated_at.as_deref()),
            Some("1780000000000")
        );
    }

    /// Verifies active VMM memories render before the dedicated expired bottom region.
    /// 验证活动 VMM 记忆会展示在独立的底部过期区域之前。
    #[test]
    fn context_injection_snapshot_places_expired_vmm_region_last() {
        let mut runtime_state = SessionRuntimeStateEntry::default();
        // expiring_batch creates one resident that crosses its TTL boundary after the first turn.
        // expiring_batch 创建一条在首个回合后跨越 TTL 边界的驻留记忆。
        let expiring_batch = VmmRecallBatch {
            ttl_turns: 1,
            injection_tag: String::from("VMM_MEMORY"),
            injection_description: String::from("Relevant memories."),
            items: vec![ContextItem {
                text: String::from("expired-memory"),
                memory_id: 7,
                ..ContextItem::default()
            }],
        };
        runtime_state
            .vmm_session_memory
            .apply_recall_batch("session-a", "user-1", &expiring_batch, "1")
            .expect("expiring recall should succeed");
        runtime_state
            .vmm_session_memory
            .register_active_turn("turn-1", "1")
            .expect("logical turn should register");
        runtime_state
            .vmm_session_memory
            .consume_turn("turn-1", "2")
            .expect("logical turn should expire the resident");
        // active_batch adds one still-active identity without reviving the expired identity.
        // active_batch 新增一条仍活动的身份，同时不恢复已过期身份。
        let active_batch = VmmRecallBatch {
            ttl_turns: 3,
            injection_tag: String::from("VMM_MEMORY"),
            injection_description: String::from("Relevant memories."),
            items: vec![ContextItem {
                text: String::from("active-memory"),
                memory_id: 8,
                ..ContextItem::default()
            }],
        };
        runtime_state
            .vmm_session_memory
            .apply_recall_batch("session-a", "user-2", &active_batch, "3")
            .expect("active recall should succeed");

        let snapshot = context_injection_snapshot_from_runtime_state(Some(&runtime_state))
            .expect("runtime state should project a context injection snapshot");
        let tags = snapshot
            .dynamic_cards
            .iter()
            .map(|card| card.tag.as_str())
            .collect::<Vec<_>>();

        assert_eq!(tags, vec!["VMM_MEMORY", "VMM_MEMORY_EXPIRED"]);
        assert_eq!(snapshot.dynamic_cards[0].items[0].ttl_label, "剩余 3 回合");
        assert_eq!(snapshot.dynamic_cards[1].items[0].ttl_label, "已过期");
    }
}
