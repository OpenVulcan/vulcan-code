//! Context compaction tests.
//! 上下文压缩测试。

use super::*;
use vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionExecutionProfile;
use vulcan_agent_runtime::conversation_protocol::conversation_state_from_session_messages;
use vulcan_context_injection::{InjectionInsert, InjectionKind};
use vulcan_workbench_model::SessionTurnStats;

/// Returns whether text contains CJK characters.
/// 返回文本是否包含 CJK 字符。
fn contains_cjk_text(text: &str) -> bool {
    text.chars().any(|character| {
        let codepoint = character as u32;
        (0x4E00..=0x9FFF).contains(&codepoint)
            || (0x3400..=0x4DBF).contains(&codepoint)
            || (0xF900..=0xFAFF).contains(&codepoint)
    })
}

/// Removes the variable last-user payload from an automatic compaction continuation prompt.
/// 从自动压缩续接提示中移除可变的最后用户消息载荷。
fn auto_compaction_continuation_shell(prompt: &str) -> String {
    let Some((before_user, after_start)) = prompt.split_once("<last_user_message>") else {
        return prompt.to_string();
    };
    let Some((_, after_user)) = after_start.split_once("</last_user_message>") else {
        return prompt.to_string();
    };
    format!("{before_user}<last_user_message></last_user_message>{after_user}")
}

/// Verifies compaction draft creation prunes expired transient injections without mutating active state.
/// 验证压缩候选状态会裁剪过期临时注入且不会修改活跃状态。
#[test]
fn webui_context_compaction_draft_prunes_expired_transient_injections_transactionally() {
    let session_id = SessionId::new_v4();
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "请实现上下文压缩")
            .with_runtime_metadata("user-1", "1780000000000"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "已开始实现")
            .with_runtime_metadata("assistant-1", "1780000000001"),
    ];
    let mut active_runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&messages)
            .expect("valid replayed conversation identities"),
        live_context_budget_fingerprint: Some(String::from("budget-live")),
        live_context_assembly_fingerprint: Some(String::from("assembly-live")),
        ..SessionRuntimeStateEntry::default()
    };
    active_runtime_state.context_injection_state.apply_insert(
        InjectionInsert::single(
            InjectionKind::Transient,
            "vmm_inject",
            "Transient memory",
            "expired transient memory",
            Some(1),
        ),
        "turn-1",
    );
    let expired_item = active_runtime_state
        .context_injection_state
        .groups
        .get_mut("vmm_inject")
        .and_then(|group| group.items.values_mut().next())
        .expect("expired transient injection item should exist");
    expired_item.remaining_turns = Some(0);
    let plan = CompactionPlan {
        source_messages: vec![messages[0].clone()],
        source_until_message_id: Some(String::from("user-1")),
        source_until_turn_index: Some(0),
        retained_tail_start_turn_index: Some(1),
        compaction_id: String::from("test-compaction"),
        created_at: String::from("1780000000100"),
        policy: vulcan_context::ContextCompactionPolicy::default(),
    };
    let execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- 保留上下文压缩升级目标。",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };

    let draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        session_id,
        "section-test",
        "预算快照",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- 保留上下文压缩升级目标。",
        &execution,
    )
    .expect("draft should be built from a valid summary");

    assert_eq!(draft.ttl_prune_report.removed_item_count, 1);
    assert!(
        active_runtime_state
            .context_injection_state
            .groups
            .contains_key("vmm_inject"),
        "active runtime state must remain untouched"
    );
    assert!(
        !draft
            .runtime_state
            .context_injection_state
            .groups
            .contains_key("vmm_inject"),
        "draft runtime state should remove expired transient injections"
    );
    assert!(active_runtime_state.context_start_message_id.is_none());
    assert!(draft.runtime_state.context_start_message_id.is_some());
    assert_eq!(
        draft.checkpoint.context_budget_fingerprint.as_deref(),
        Some("budget-live")
    );
}

/// Verifies completed compaction markers are not anchored back into older visible turns.
/// 验证已完成压缩标记不会被锚回更早的可见轮次。
#[test]
fn webui_context_compaction_draft_keeps_status_markers_unanchored() {
    let session_id = SessionId::new_v4();
    let section_id = String::from("section-test");
    let section = SessionMessageEntry {
        id: section_id.clone(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("上下文正在压缩"),
        body: String::new(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_section_id": section_id.clone(),
            "context_compaction_role": "start",
            "context_compaction_state": "running",
        })),
        created_at: String::from("1780000000002"),
    };
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "用户", "压缩前问题")
            .with_runtime_metadata("user-before", "1780000000000"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "压缩前回答")
            .with_runtime_metadata("assistant-before", "1780000000001"),
        section,
        SessionMessageEntry::new(SessionMessageKind::User, "用户", "压缩后继续提问")
            .with_runtime_metadata("user-after", "1780000000003"),
    ];
    let active_runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&messages)
            .expect("valid replayed conversation identities"),
        live_context_budget_fingerprint: Some(String::from("budget-live")),
        live_context_assembly_fingerprint: Some(String::from("assembly-live")),
        ..SessionRuntimeStateEntry::default()
    };
    let plan = CompactionPlan {
        source_messages: vec![messages[0].clone()],
        source_until_message_id: Some(String::from("user-before")),
        source_until_turn_index: Some(0),
        retained_tail_start_turn_index: Some(1),
        compaction_id: String::from("test-compaction-unanchored"),
        created_at: String::from("1780000000100"),
        policy: vulcan_context::ContextCompactionPolicy::default(),
    };
    let execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- 保留上下文压缩升级目标。",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };

    let draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        session_id,
        &section_id,
        "预算快照",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- 保留上下文压缩升级目标。",
        &execution,
    )
    .expect("draft should be built from a valid summary");

    let section_records = draft
        .runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| {
            record.id == section_id
                || record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("context_compaction_section_id"))
                    .and_then(Value::as_str)
                    == Some(section_id.as_str())
        })
        .collect::<Vec<_>>();
    assert!(
        section_records.len() >= 2,
        "completed compaction should keep visible section rows"
    );
    assert!(section_records.iter().all(|record| {
        record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_insert_before_message_id"))
            .is_none()
    }));
}

/// Verifies manual compaction owns draft statistics while automatic compaction owns continuation state.
/// 验证手动压缩在候选状态中持有统计记录，而自动压缩持有续接状态。
#[test]
fn webui_context_compaction_draft_separates_manual_stats_and_auto_continuation() {
    let session_id = SessionId::new_v4();
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "User", "Old question")
            .with_runtime_metadata("user-old", "1780000000000"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "Assistant", "Old answer")
            .with_runtime_metadata("assistant-old", "1780000000001"),
        SessionMessageEntry::new(
            SessionMessageKind::User,
            "User",
            "Please continue the real task",
        )
        .with_runtime_metadata("user-latest", "1780000000002"),
    ];
    let mut active_runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&messages)
            .expect("valid replayed conversation identities"),
        live_context_budget_fingerprint: Some(String::from("budget-live")),
        live_context_assembly_fingerprint: Some(String::from("assembly-live")),
        ..SessionRuntimeStateEntry::default()
    };
    active_runtime_state
        .vmm_session_memory
        .apply_recall_batch(
            &session_id.to_string(),
            "user-old",
            &vulcan_vmm::VmmRecallBatch {
                ttl_turns: 3,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                items: vec![vulcan_vmm::ContextItem {
                    text: String::from("active-compaction-memory"),
                    memory_id: 77,
                    ..vulcan_vmm::ContextItem::default()
                }],
            },
            "1780000000003",
        )
        .expect("active VMM lease should be prepared before compaction");
    active_runtime_state
        .conversation_state
        .continuation
        .last_provider_message_id = Some(String::from("resp-before-compaction"));
    active_runtime_state
        .conversation_state
        .continuation
        .last_local_message_id = Some(String::from("assistant-before-compaction"));
    let plan = CompactionPlan {
        source_messages: vec![messages[0].clone()],
        source_until_message_id: Some(String::from("user-old")),
        source_until_turn_index: Some(0),
        retained_tail_start_turn_index: Some(1),
        compaction_id: String::from("test-auto-continuation"),
        created_at: String::from("1780000000100"),
        policy: vulcan_context::ContextCompactionPolicy::default(),
    };
    let manual_execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- Keep automatic compaction continuation semantics.",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };
    let manual_draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        session_id,
        "section-manual",
        "Budget snapshot",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- Keep automatic compaction continuation semantics.",
        &manual_execution,
    )
    .expect("manual draft should be built from a valid summary");
    assert!(
        !manual_draft
            .runtime_state
            .conversation_state
            .messages
            .iter()
            .any(|message| matches!(
                &message.payload,
                SessionConversationPayload::UserText { text, .. }
                    if text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX)
            )),
        "manual compaction must not append the automatic continuation note"
    );
    assert_eq!(
        manual_draft
            .runtime_state
            .vmm_session_memory
            .cache_epoch
            .epoch_id,
        "test-auto-continuation"
    );
    assert!(
        manual_draft
            .runtime_state
            .vmm_session_memory
            .cache_epoch
            .anchors
            .is_empty()
    );
    assert!(
        manual_draft
            .runtime_state
            .vmm_session_memory
            .cache_epoch
            .needs_active_baseline
    );
    assert!(
        manual_draft
            .runtime_state
            .conversation_state
            .continuation
            .last_provider_message_id
            .is_none()
    );
    assert!(
        manual_draft
            .runtime_state
            .conversation_state
            .continuation
            .last_local_message_id
            .is_none()
    );
    assert_eq!(
        active_runtime_state
            .vmm_session_memory
            .cache_epoch
            .anchors
            .len(),
        1,
        "draft construction must not mutate the active VMM epoch"
    );
    // manual_turn_stats is materialized inside the draft before any active-state replacement occurs.
    // manual_turn_stats 在替换任何活动状态前已物化于候选状态内部。
    let manual_turn_stats = manual_draft
        .runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
        .and_then(|record| record.metadata.as_ref())
        .and_then(|metadata| metadata.get("turn_stats"))
        .cloned()
        .and_then(|value| serde_json::from_value::<SessionTurnStats>(value).ok())
        .expect("manual compaction draft should contain canonical turn statistics");
    assert_eq!(
        manual_turn_stats.turn_kind,
        SessionTurnStatsKind::ContextCompaction
    );
    assert_eq!(manual_turn_stats.provider_call_count, 1);
    assert_eq!(manual_turn_stats.usage_segments.len(), 1);
    assert_eq!(
        manual_turn_stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::ContextCompaction
    );

    let auto_execution = PromptCompactionExecution {
        compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
            &plan,
            "## Goal\n- Keep automatic compaction continuation semantics.",
        ),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };
    let auto_draft = build_webui_context_compaction_draft(
        &active_runtime_state,
        session_id,
        "section-auto",
        "Budget snapshot",
        AUTO_CONTEXT_COMPACTION_TRIGGER,
        1,
        "## Goal\n- Keep automatic compaction continuation semantics.",
        &auto_execution,
    )
    .expect("automatic draft should be built from a valid summary");
    assert!(
        auto_draft
            .runtime_state
            .conversation_state
            .records
            .iter()
            .all(|record| record.record_kind != SessionMessageRecordKind::TurnStats),
        "automatic compaction must not append an independent turn statistics record"
    );
    let continuation_messages = auto_draft
        .runtime_state
        .conversation_state
        .messages
        .iter()
        .filter_map(|message| {
            let SessionConversationPayload::UserText { text, .. } = &message.payload else {
                return None;
            };
            text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX)
                .then_some(text.as_str())
        })
        .collect::<Vec<_>>();
    assert_eq!(continuation_messages.len(), 1);
    assert!(continuation_messages[0].contains("synthetic continuation prompt"));
    assert!(continuation_messages[0].contains("resumed provider request"));
    assert!(continuation_messages[0].contains("Do not greet"));
    assert!(continuation_messages[0].contains("Please continue the real task"));
    assert!(
        !contains_cjk_text(continuation_messages[0]),
        "model-visible automatic compaction continuation message must remain fixed English in this host-generated test path"
    );
    assert!(
        !contains_cjk_text(&auto_compaction_continuation_shell(
            continuation_messages[0]
        )),
        "model-visible automatic compaction continuation shell must remain fixed English"
    );
    assert!(
        auto_draft
            .runtime_state
            .conversation_state
            .records
            .iter()
            .all(|record| !record.body.contains(AUTO_COMPACTION_CONTINUATION_PREFIX)),
        "automatic continuation is model-visible runtime state, not a visible transcript record"
    );
    // The synthetic continuation id is the only legal target for the automatic VMM baseline.
    // 合成续接消息标识是自动 VMM 基线唯一合法的目标。
    let synthetic_continuation_id = auto_draft
        .runtime_state
        .conversation_state
        .messages
        .iter()
        .find_map(|message| match &message.payload {
            SessionConversationPayload::UserText { text, .. }
                if text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX) =>
            {
                Some(message.id.as_str())
            }
            _ => None,
        })
        .expect("automatic compaction should append one synthetic continuation id");
    let baseline_anchor = auto_draft
        .runtime_state
        .vmm_session_memory
        .cache_epoch
        .anchors
        .first()
        .expect("automatic compaction should bind one active VMM baseline");
    assert_eq!(
        baseline_anchor.kind,
        vulcan_vmm::VmmPromptAnchorKind::Baseline
    );
    assert_eq!(baseline_anchor.target_message_id, synthetic_continuation_id);
    assert!(baseline_anchor.prefix.contains("active-compaction-memory"));
    assert!(
        !auto_draft
            .runtime_state
            .vmm_session_memory
            .cache_epoch
            .needs_active_baseline
    );
}

/// Verifies repeated automatic compaction ignores previous continuation notes when finding the last user prompt.
/// 验证重复自动压缩查找最后用户提示时会忽略旧续接提示。
#[test]
fn last_runtime_user_message_text_skips_previous_auto_continuation() {
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("real-user"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("真正的最后用户请求"),
                    attachments: Vec::new(),
                },
            },
            SessionConversationMessage {
                id: String::from("old-auto-continuation"),
                payload: SessionConversationPayload::UserText {
                    text: format!("{AUTO_COMPACTION_CONTINUATION_PREFIX} stale note"),
                    attachments: Vec::new(),
                },
            },
        ],
        ..SessionConversationState::default()
    };

    assert_eq!(
        last_runtime_user_message_text(&state).as_deref(),
        Some("真正的最后用户请求")
    );
}

/// Verifies context compaction persistence keeps the complete visible transcript window.
/// 验证上下文压缩持久化会保留完整可见转录窗口。
#[test]
fn context_compaction_complete_display_messages_preserve_visible_history() {
    let session_id = SessionId::new_v4();
    let section_id = String::from("compaction-section-1");
    let mut user_entry =
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "循环调用工具");
    user_entry.id = String::from("user-1");
    let mut tool_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "", "已执行 sleep 5");
    tool_entry.id = String::from("tool-1");
    tool_entry.record_kind = SessionMessageRecordKind::ToolCallOutput;
    let mut interrupt_entry =
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "不用继续了");
    interrupt_entry.id = String::from("interrupt-1");
    interrupt_entry.metadata = Some(json!({
        "source": "user_interrupt",
        "starts_user_turn": false,
        "turn_process": true,
        "editable": false,
    }));
    let mut compaction_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "上下文已压缩", "");
    compaction_entry.id = section_id.clone();
    compaction_entry.record_kind = SessionMessageRecordKind::CompactionStatus;
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![
                user_entry.clone(),
                tool_entry.clone(),
                interrupt_entry.clone(),
                compaction_entry.clone(),
            ],
        )]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };

    let messages =
        context_compaction_complete_display_messages(&workspace_snapshot, session_id, &section_id);

    assert_eq!(messages.len(), 4);
    assert!(messages.iter().any(|message| message.id == tool_entry.id));
    assert!(
        messages
            .iter()
            .any(|message| message.id == interrupt_entry.id)
    );
    assert!(
        messages
            .iter()
            .any(|message| message.id == compaction_entry.id)
    );
}

/// Verifies compaction terminal records preserve clone-prune report metadata.
/// 验证压缩终态记录会保留克隆剪枝报告元数据。
#[test]
fn append_webui_context_compaction_end_message_preserves_input_prune_report() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let prune_report = CompactionInputPruneReport {
        mode: CompactionInputPruneMode::RequiredForWindowDowngrade,
        target_input_budget_tokens: 1024,
        safety_reserve_tokens: 128,
        original_token_estimate: 4096,
        pruned_token_estimate: 900,
        pruned_tool_output_count: 2,
        preserved_tool_output_count: 3,
        dropped_source_message_count: 4,
        truncated_source_message_count: 5,
        still_exceeds_budget: false,
    };

    let record_id = append_webui_context_compaction_end_message_to_state(
        &mut runtime_state,
        session_id,
        "section-a",
        "预算说明",
        AUTO_CONTEXT_COMPACTION_TRIGGER,
        7,
        Some(&prune_report),
    )
    .expect("valid compaction terminal record identity");

    let record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.id == record_id)
        .expect("terminal record should be appended");
    let metadata = record
        .metadata
        .as_ref()
        .expect("terminal record should carry metadata");
    assert_eq!(
        metadata
            .get("context_compaction_input_prune_report")
            .and_then(|value| value.get("mode"))
            .and_then(Value::as_str),
        Some("required_for_window_downgrade")
    );
    assert_eq!(
        metadata
            .get("context_compaction_input_prune_report")
            .and_then(|value| value.get("pruned_tool_output_count"))
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        metadata
            .get("context_compaction_provider_request_count")
            .and_then(Value::as_u64),
        Some(7)
    );
    assert_eq!(
        metadata
            .get("context_compaction_trigger")
            .and_then(Value::as_str),
        Some(AUTO_CONTEXT_COMPACTION_TRIGGER)
    );
}

/// Verifies failed auto-compaction records failure metadata and circuit-breaker progress.
/// 验证自动压缩失败会记录失败元数据并推进熔断计数。
#[test]
fn fail_webui_context_compaction_execution_records_auto_failure_metadata() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry {
        auto_compaction_consecutive_failures: 2,
        ..SessionRuntimeStateEntry::default()
    };
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "压缩测试会话",
            "",
            SessionState::Running,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Preparing)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    let section_id = append_webui_context_compaction_status_message_to_workspace(
        &mut workspace_snapshot,
        session_id,
        "预算说明",
        AUTO_CONTEXT_COMPACTION_TRIGGER,
    )
    .expect("valid compaction status record identity");
    runtime_state.conversation_state = workspace_snapshot
        .session_runtime_state
        .remove(&session_id)
        .expect("status append should create runtime state")
        .conversation_state;
    workspace_snapshot
        .session_runtime_state
        .insert(session_id, runtime_state);
    apply_webui_context_compaction_failure_to_workspace(
        &mut workspace_snapshot,
        session_id,
        SessionRunPhase::Ready,
        &section_id,
        "预算说明",
        AUTO_CONTEXT_COMPACTION_TRIGGER,
        "测试失败",
    )
    .expect("valid compaction failure record identity");

    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should remain present");
    assert_eq!(runtime_state.auto_compaction_consecutive_failures, 3);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Ready)
    );
    assert!(
        workspace_snapshot
            .sessions
            .iter()
            .any(|session| session.id == session_id && session.status == SessionState::Completed)
    );
    let failed_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("context_compaction_role"))
                .and_then(Value::as_str)
                == Some("end")
        })
        .expect("failed terminal record should be appended");
    let metadata = failed_record
        .metadata
        .as_ref()
        .expect("failed terminal record should carry metadata");
    assert_eq!(
        metadata
            .get("context_compaction_state")
            .and_then(Value::as_str),
        Some("failed")
    );
    assert_eq!(
        metadata
            .get("context_compaction_trigger")
            .and_then(Value::as_str),
        Some(AUTO_CONTEXT_COMPACTION_TRIGGER)
    );
    assert_eq!(
        metadata
            .get("context_compaction_error")
            .and_then(Value::as_str),
        Some("测试失败")
    );
}
