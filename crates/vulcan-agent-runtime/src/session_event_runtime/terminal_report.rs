use super::*;

/// Applies the terminal execution report to one background conversation state.
/// 将终态执行报告应用到单个后台会话状态。
pub fn apply_terminal_execution_report_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) -> SessionTerminalReportApplyResult {
    let is_success = report.error.is_none();
    if let Some(conversation_state) = report.conversation_state.clone() {
        apply_checkpoint_conversation_state(runtime_state, conversation_state, now);
    }
    if let Some(usage) = report.last_successful_provider_usage.clone() {
        runtime_state.last_provider_usage = Some(usage.clone());
        runtime_state.last_successful_provider_usage = Some(usage);
    }
    if report.provider_request_count > 0 {
        runtime_state.last_safe_provider_request_count = Some(report.provider_request_count);
    }
    if is_success {
        complete_open_tool_records(
            runtime_state,
            session_id,
            now,
            "failed",
            "Tool call ended before returning a result.",
        );
    } else {
        complete_open_tool_records(
            runtime_state,
            session_id,
            now,
            "failed",
            "Tool call failed or was interrupted before returning a result.",
        );
    }
    let closing_record_id = if is_success {
        ensure_terminal_success_records(runtime_state, session_id, now, report);
        ensure_terminal_success_assistant_message(runtime_state, session_id, now, report);
        append_terminal_summary_record(runtime_state, session_id, now, report)
    } else {
        append_terminal_error_record(runtime_state, session_id, now, report);
        append_terminal_terminated_record(
            runtime_state,
            session_id,
            now,
            report
                .error
                .as_deref()
                .unwrap_or("真实执行失败，但未返回具体错误信息。"),
        )
    };
    let status = if is_success {
        SessionTurnStatsStatus::Completed
    } else {
        SessionTurnStatsStatus::Failed
    };
    append_terminal_turn_stats_record(
        runtime_state,
        session_id,
        now,
        &closing_record_id,
        status,
        Some(report.provider_request_count),
        SessionTurnStatsKind::UserTurn,
        report
            .cumulative_successful_provider_usage
            .as_ref()
            .or(report.last_successful_provider_usage.as_ref()),
        report.embedded_turn_usage_segments.clone(),
    );
    runtime_state.updated_at = now.to_string();
    SessionTerminalReportApplyResult {
        is_success,
        closing_record_id,
    }
}

/// Result produced after applying one terminal report.
/// 应用一份终态报告后产出的结果。
#[derive(Debug, Clone)]
pub struct SessionTerminalReportApplyResult {
    /// Whether the terminal report represents a successful run.
    /// 终态报告是否表示一次成功运行。
    pub is_success: bool,
    /// Hidden summary or terminated record that closes the logical turn.
    /// 闭合逻辑轮次的隐藏摘要或终止记录。
    pub closing_record_id: String,
}

/// Ensures a successful terminal report has visible thinking and output records.
/// 确保成功终态报告拥有可见的思考与输出记录。
fn ensure_terminal_success_records(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) {
    if !current_turn_has_visible_record(runtime_state, SessionMessageRecordKind::ThinkingOutput)
        && let Some(thinking_output) = report
            .thinking_output
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        let record_id = build_session_record_message_id(
            session_id,
            now,
            runtime_state.conversation_state.records.len(),
        );
        let thinking_record = SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ThinkingOutput,
            visible: true,
            title: String::from("思考"),
            body: thinking_output.to_string(),
            metadata: Some(json!({ "collapsed_by_default": true })),
            created_at: now.to_string(),
        };
        let insert_index = current_turn_first_visible_record_index(
            runtime_state,
            SessionMessageRecordKind::AssistantOutput,
        )
        .unwrap_or(runtime_state.conversation_state.records.len());
        runtime_state
            .conversation_state
            .records
            .insert(insert_index, thinking_record);
        runtime_state
            .ui_state
            .collapsed_message_ids
            .insert(record_id);
    }
    if !current_turn_has_visible_record(runtime_state, SessionMessageRecordKind::AssistantOutput) {
        let assistant_output = report.assistant_output.clone().unwrap_or_else(|| {
            String::from("The model completed the run but did not return visible text output.")
        });
        let record_id = build_session_record_message_id(
            session_id,
            now,
            runtime_state.conversation_state.records.len(),
        );
        append_record(
            &mut runtime_state.conversation_state,
            SessionConversationRecord {
                id: record_id,
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: assistant_output,
                metadata: None,
                created_at: now.to_string(),
            },
        );
    }
}

/// Ensures successful terminal output participates in future provider-visible history.
/// 确保成功终态输出会参与后续 Provider 可见历史。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state that owns canonical messages.
/// `runtime_state` 是拥有规范消息的会话运行态。
/// `session_id` selects the session receiving the assistant output.
/// `session_id` 选择接收助手输出的会话。
/// `now` is the current Unix-millis timestamp text.
/// `now` 是当前 Unix 毫秒时间戳文本。
/// `report` contains the terminal provider output.
/// `report` 包含终态 Provider 输出。
fn ensure_terminal_success_assistant_message(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) {
    let Some(assistant_output) = report
        .assistant_output
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    if runtime_state
        .conversation_state
        .messages
        .iter_mut()
        .rev()
        .any(|message| terminal_assistant_message_accepts_usage(message, assistant_output, report))
    {
        return;
    }
    append_terminal_assistant_text_message(
        runtime_state,
        session_id,
        now,
        assistant_output,
        report,
    );
}

/// Returns whether the message is the terminal assistant output and attaches missing usage.
/// 判断消息是否为终态助手输出，并在缺失时补入用量信息。
///
/// # Parameters
/// 参数
/// `message` is one mutable canonical conversation message from the current session.
/// `message` 是当前会话中的一条可变规范会话消息。
/// `assistant_output` is the terminal assistant text reported by the runtime.
/// `assistant_output` 是运行时报告的终态助手文本。
/// `report` carries the authoritative latest successful provider usage for this run.
/// `report` 携带本次运行最近一次成功 Provider 调用的权威用量。
///
/// # Returns
/// 返回值
/// Returns `true` when this message is the terminal assistant output.
/// 当该消息就是终态助手输出时返回 `true`。
fn terminal_assistant_message_accepts_usage(
    message: &mut SessionConversationMessage,
    assistant_output: &str,
    report: &ExecutionRunReport,
) -> bool {
    let SessionConversationPayload::Assistant { text, usage, .. } = &mut message.payload else {
        return false;
    };
    if text.trim() != assistant_output {
        return false;
    }
    if usage.is_none() {
        *usage = report.last_successful_provider_usage.clone();
    }
    true
}

/// Appends the terminal assistant protocol message with the run usage attached.
/// 追加携带本次运行用量的终态助手协议消息。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state that owns canonical messages.
/// `runtime_state` 是拥有规范消息的会话运行态。
/// `session_id` selects the session receiving the assistant output.
/// `session_id` 选择接收助手输出的会话。
/// `now` is the current Unix-millis timestamp text.
/// `now` 是当前 Unix 毫秒时间戳文本。
/// `assistant_output` is the normalized terminal assistant text.
/// `assistant_output` 是规范化后的终态助手文本。
/// `report` carries the latest successful provider usage for this run.
/// `report` 携带本次运行最近一次成功 Provider 用量。
fn append_terminal_assistant_text_message(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    assistant_output: &str,
    report: &ExecutionRunReport,
) {
    runtime_state
        .conversation_state
        .messages
        .push(SessionConversationMessage {
            id: format!("session-{session_id}-protocol-assistant-{now}"),
            payload: SessionConversationPayload::Assistant {
                text: assistant_output.to_string(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: report.last_successful_provider_usage.clone(),
                tool_calls: Vec::new(),
            },
        });
}

/// Appends the same hidden summary record produced by the shared session-derived skill chain.
/// 追加与共享会话派生技能链一致的隐藏摘要记录。
fn append_terminal_summary_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) -> String {
    let derived_record =
        SessionDerivedSkillChain::default().run_summary_skill(SummarySkillInput { report });
    append_derived_record(runtime_state, session_id, now, derived_record)
}

/// Appends a visible terminal error row for a failed execution report.
/// 为失败的执行报告追加一条可见终态错误行。
fn append_terminal_error_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    report: &ExecutionRunReport,
) {
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id,
            kind: SessionMessageKind::Error,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("执行失败"),
            body: report
                .error
                .clone()
                .unwrap_or_else(|| String::from("真实执行失败，但未返回具体错误信息。")),
            metadata: None,
            created_at: now.to_string(),
        },
    );
}

/// Appends the hidden terminal marker used to close failed or stopped turns.
/// 追加用于闭合失败或中止轮次的隐藏终止标记。
fn append_terminal_terminated_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    error: &str,
) -> String {
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnTerminated,
            visible: false,
            title: String::from("Turn Terminated"),
            body: format!("The run terminated without a summary. Error detail: {error}"),
            metadata: Some(json!({ "preview": "" })),
            created_at: now.to_string(),
        },
    );
    record_id
}

/// Appends one visible turn-statistics row using the shared statistics builder.
/// 使用共享统计构建器追加一条可见轮次统计行。
fn append_terminal_turn_stats_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    closing_record_id: &str,
    status: SessionTurnStatsStatus,
    provider_request_count_override: Option<usize>,
    turn_kind: SessionTurnStatsKind,
    provider_usage_fallback: Option<&vulcan_core::types::ProviderUsageSnapshot>,
    usage_segments: Vec<SessionTurnUsageSegment>,
) -> Option<String> {
    let mut turn_stats = build_session_turn_stats_from_state_with_segments(
        &runtime_state.conversation_state,
        closing_record_id,
        status,
        provider_request_count_override,
        turn_kind,
        Vec::new(),
    )?;
    merge_terminal_provider_usage_fallback(
        &mut turn_stats,
        provider_request_count_override,
        provider_usage_fallback,
    );
    merge_terminal_explicit_usage_segments(&mut turn_stats, usage_segments);
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::TurnStats,
            visible: true,
            title: format!("Turn Stats · {}", turn_stats.status.label()),
            body: turn_stats.detail_body(),
            metadata: Some(json!({
                "turn_stats": turn_stats,
                "turn_closing_record_id": closing_record_id,
            })),
            created_at: now.to_string(),
        },
    );
    Some(record_id)
}

/// Merges authoritative provider usage when transcript messages did not retain token usage.
/// 当转录消息没有保留 Token 用量时，合并终态报告中的权威 Provider 用量。
fn merge_terminal_provider_usage_fallback(
    turn_stats: &mut SessionTurnStats,
    provider_request_count_override: Option<usize>,
    provider_usage_fallback: Option<&vulcan_core::types::ProviderUsageSnapshot>,
) {
    if terminal_turn_stats_has_token_usage(turn_stats) {
        return;
    }
    let Some(provider_usage) = provider_usage_fallback else {
        return;
    };
    remove_empty_assistant_usage_segments(turn_stats);
    let provider_call_count = provider_request_count_override
        .filter(|count| *count > 0)
        .unwrap_or(1);
    let mut segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::AssistantResponse,
        provider_call_count,
        Some(provider_usage),
    );
    if turn_stats.provider_call_count > 0 {
        segment.provider_call_count = 0;
    }
    merge_usage_segment_into_turn_stats(turn_stats, &segment);
    turn_stats.usage_segments.push(segment);
}

/// Removes call-only assistant segments before inserting the authoritative provider-usage segment.
/// 插入权威 Provider 用量分段前，移除只有调用次数的空助手分段。
fn remove_empty_assistant_usage_segments(turn_stats: &mut SessionTurnStats) {
    turn_stats.usage_segments.retain(|segment| {
        !(segment.kind == SessionTurnUsageSegmentKind::AssistantResponse
            && segment.input_tokens == 0
            && segment.output_tokens == 0
            && segment.reasoning_tokens == 0
            && segment.total_tokens == 0
            && segment.tool_call_count == 0
            && segment.cached_input_tokens.is_none()
            && segment.cache_creation_input_tokens.is_none())
    });
}

/// Merges explicit embedded usage segments after the normal assistant-response usage is settled.
/// 在常规助手响应统计稳定后，合并显式嵌入的用量分段。
fn merge_terminal_explicit_usage_segments(
    turn_stats: &mut SessionTurnStats,
    usage_segments: Vec<SessionTurnUsageSegment>,
) {
    for segment in usage_segments {
        merge_explicit_usage_segment_into_turn_stats(turn_stats, &segment);
        turn_stats.usage_segments.push(segment);
    }
}

/// Returns whether one terminal statistics snapshot already contains concrete token usage.
/// 判断终态统计快照是否已经包含明确 Token 用量。
fn terminal_turn_stats_has_token_usage(turn_stats: &SessionTurnStats) -> bool {
    turn_stats.input_tokens > 0
        || turn_stats.output_tokens > 0
        || turn_stats.reasoning_tokens > 0
        || turn_stats.total_tokens > 0
        || turn_stats
            .cached_input_tokens
            .is_some_and(|cached_input_tokens| cached_input_tokens > 0)
        || turn_stats
            .cache_creation_input_tokens
            .is_some_and(|cache_creation_input_tokens| cache_creation_input_tokens > 0)
}

/// Appends one hidden derived session record with the same metadata shape used by the workbench.
/// 以工作台使用的相同元数据结构追加一条隐藏派生会话记录。
fn append_derived_record(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    now: &str,
    record: SessionDerivedRecord,
) -> String {
    let record_id = build_session_record_message_id(
        session_id,
        now,
        runtime_state.conversation_state.records.len(),
    );
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: record_id.clone(),
            kind: record.kind,
            record_kind: record.record_kind,
            visible: false,
            title: record.title,
            body: record.body,
            metadata: Some(json!({ "preview": record.preview })),
            created_at: now.to_string(),
        },
    );
    record_id
}

/// Returns whether the current logical turn already has a visible record of the requested kind.
/// 返回当前逻辑轮次是否已经拥有指定类型的可见记录。
fn current_turn_has_visible_record(
    runtime_state: &SessionRuntimeStateEntry,
    record_kind: SessionMessageRecordKind,
) -> bool {
    let records = &runtime_state.conversation_state.records;
    let boundary_index = records
        .iter()
        .rposition(|record| {
            matches!(
                record.record_kind,
                SessionMessageRecordKind::SummaryOutput
                    | SessionMessageRecordKind::TurnTerminated
                    | SessionMessageRecordKind::TurnStats
            )
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    records[boundary_index..]
        .iter()
        .any(|record| record.visible && record.record_kind == record_kind)
}

/// Finds the first visible record of a kind in the current logical turn.
/// 查找当前逻辑轮次中指定类型的第一条可见记录。
fn current_turn_first_visible_record_index(
    runtime_state: &SessionRuntimeStateEntry,
    record_kind: SessionMessageRecordKind,
) -> Option<usize> {
    let records = &runtime_state.conversation_state.records;
    let boundary_index = records
        .iter()
        .rposition(|record| {
            record.kind == SessionMessageKind::User
                || record.record_kind == SessionMessageRecordKind::TurnStats
                || record.record_kind == SessionMessageRecordKind::TurnTerminated
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    records[boundary_index..]
        .iter()
        .position(|record| record.visible && record.record_kind == record_kind)
        .map(|index| boundary_index + index)
}
