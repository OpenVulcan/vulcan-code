use serde_json::json;
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_workbench_model::{
    SessionChildUsageContribution, SessionProviderCallCompleted, SessionProviderCallIdentity,
    SessionProviderCallStarted, SessionProviderCallStatus, SessionProviderUsageEvidence,
    SessionTurnUsageGroupKind,
};

use super::*;
use crate::conversation_protocol::{SessionConversationMessage, SessionConversationToolCall};

/// Builds one visible transcript user record for turn-boundary tests.
/// 为轮次边界测试构建一条可见用户转录记录。
fn user_record(id: &str, created_at: &str, body: &str) -> SessionConversationRecord {
    SessionConversationRecord {
        id: id.to_string(),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("用户指令"),
        body: body.to_string(),
        metadata: None,
        created_at: created_at.to_string(),
    }
}

/// Builds one visible interrupt user record folded into the active turn process.
/// 构建一条折叠进当前轮次过程的可见插队用户记录。
fn interrupt_user_record(id: &str, created_at: &str, body: &str) -> SessionConversationRecord {
    let mut record = user_record(id, created_at, body);
    record.metadata = Some(json!({
        "source": "user_interrupt",
        "starts_user_turn": false,
        "turn_process": true,
        "collapsed_by_default": true,
        "editable": false,
        "canonical_text": body,
        "canonical_user_text": body,
    }));
    record
}

/// Builds one hidden summary record that closes a logical turn.
/// 构建一条用于闭合逻辑轮次的隐藏摘要记录。
fn summary_record(id: &str, created_at: &str) -> SessionConversationRecord {
    SessionConversationRecord {
        id: id.to_string(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::SummaryOutput,
        visible: false,
        title: String::from("Summary"),
        body: String::from("summary"),
        metadata: None,
        created_at: created_at.to_string(),
    }
}

/// Builds one visible turn-stats record that separates completed turns.
/// 构建一条用于分隔已完成轮次的可见统计记录。
fn turn_stats_record(id: &str, created_at: &str) -> SessionConversationRecord {
    SessionConversationRecord {
        id: id.to_string(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::TurnStats,
        visible: true,
        title: String::from("Turn Stats · completed"),
        body: String::from("stats"),
        metadata: None,
        created_at: created_at.to_string(),
    }
}

/// Builds one visible context-compaction status record with stable section metadata.
/// 构建一条带稳定分段元数据的可见上下文压缩状态记录。
fn context_compaction_record(
    id: &str,
    section_id: &str,
    role: &str,
    created_at: &str,
) -> SessionConversationRecord {
    SessionConversationRecord {
        id: id.to_string(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: format!("context_compaction.{role}"),
        body: String::new(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_section_id": section_id,
            "context_compaction_role": role,
            "context_compaction_state": "completed",
        })),
        created_at: created_at.to_string(),
    }
}

/// Builds one visible tool-result record with structured host metadata.
/// 构建一条带结构化宿主元数据的可见工具结果记录。
fn tool_result_record(id: &str, created_at: &str, metadata: Value) -> SessionConversationRecord {
    SessionConversationRecord {
        id: id.to_string(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("工具返回"),
        body: String::from("tool result"),
        metadata: Some(metadata),
        created_at: created_at.to_string(),
    }
}

/// Builds one child-agent feedback statistics record embedded in a parent turn.
/// 构建一条嵌入父回合中的智能体反馈统计记录。
fn subagent_turn_stats_record(id: &str, created_at: &str) -> SessionConversationRecord {
    let mut record = turn_stats_record(id, created_at);
    record.title = String::from("智能体统计 · worker");
    record.metadata = Some(json!({
        "turn_stats": {
            "turn_kind": "subagent_feedback",
            "status": "completed",
            "provider_call_count": 1,
            "tool_call_count": 0,
            "input_tokens": 100,
            "output_tokens": 5,
            "reasoning_tokens": 0,
            "total_tokens": 105
        }
    }));
    record
}

/// Appends one terminal physical Provider call to a ledger for Turn Stats materialization tests.
/// 为 Turn Stats 物化测试向账本追加一次终态物理 Provider 调用。
fn append_turn_stats_provider_call(
    ledger: &mut SessionProviderCallLedger,
    call_id: &str,
    owner_kind: &str,
    turn_id: &str,
    closing_record_id: &str,
    usage: ProviderUsageSnapshot,
) {
    let identity = SessionProviderCallIdentity {
        logical_request_id: format!("logical-{call_id}"),
        execution_id: format!("execution-{call_id}"),
        turn_id: Some(turn_id.to_string()),
        attempt_index: 0,
        retry_number: 0,
        owner_kind: owner_kind.to_string(),
        owner_id: format!("owner-{owner_kind}"),
        owner_name_snapshot: owner_kind.to_string(),
        operation: String::from("conversation"),
        provider_id: String::from("provider-1"),
        provider_name_snapshot: String::from("Provider One"),
        model_id: String::from("model-1"),
        model_name_snapshot: String::from("Model One"),
        connection_id: String::from("connection-1"),
        service_entry_id: String::from("public-api"),
        protocol_profile_id: String::from("responses"),
        manifest_revision: 1,
        manifest_generation: 1,
        binding_source: String::from("system_manifest"),
        binding_rule: None,
        route_revision: String::from("route-1"),
        request_profiles: Vec::new(),
        response_profiles: Vec::new(),
    };
    assert!(ledger.observe_started(SessionProviderCallStarted {
        provider_call_id: call_id.to_string(),
        identity: identity.clone(),
        prepared_at_ms: 100,
        request_assembly_fingerprint: None,
        context_generation_id: None,
        context_start_record_id: Some(String::from("user-1")),
        model_route_fingerprint: None,
        toolset_fingerprint: None,
        system_prompt_fingerprint: None,
        request_text_measurement: None,
        budget_accounting: None,
    }));
    assert!(ledger.observe_completed(SessionProviderCallCompleted {
        provider_call_id: call_id.to_string(),
        identity,
        status: SessionProviderCallStatus::Succeeded,
        provider_usage_evidence: SessionProviderUsageEvidence {
            availability: SessionProviderUsageAvailability::Final,
            provider_usage: Some(usage),
        },
        prepared_at_ms: 100,
        dispatched_at_ms: Some(101),
        first_event_at_ms: Some(102),
        first_token_at_ms: Some(103),
        completed_at_ms: 104,
        dispatch_outcome: Some(String::from("dispatched")),
        dispatch_outcome_known: Some(true),
        semantic_output_observed: Some(true),
        semantic_output_observed_known: Some(true),
        replay_safe: Some(false),
        error_category: None,
        error_code: None,
    }));
    assert_eq!(
        ledger.assign_closing_record_id_to_latest_execution_for_owner(
            closing_record_id.to_string(),
            Some(owner_kind),
        ),
        1
    );
}

/// Verifies ledger-owned calls and one inclusive direct-child contribution are each counted exactly once.
/// 验证账本直属调用与一份直接子代理包容性贡献各自只统计一次。
#[test]
fn ledger_turn_stats_avoid_direct_and_descendant_double_counting() {
    let conversation_state = SessionConversationState {
        records: vec![
            user_record("user-1", "100", "run"),
            summary_record("summary-1", "200"),
        ],
        ..SessionConversationState::default()
    };
    let mut ledger = SessionProviderCallLedger::default();
    append_turn_stats_provider_call(
        &mut ledger,
        "main-call",
        "main_agent",
        "turn-1",
        "summary-1",
        ProviderUsageSnapshot {
            input_tokens: Some(10),
            output_tokens: Some(2),
            total_tokens: Some(12),
            ..ProviderUsageSnapshot::default()
        },
    );
    assert!(
        ledger.observe_child_contribution(SessionChildUsageContribution {
            task_id: String::from("child-task"),
            execution_id: String::from("child-execution"),
            parent_turn_id: String::from("turn-1"),
            revision: 7,
            status: String::from("final"),
            inclusive_usage: Some(ProviderUsageSnapshot {
                input_tokens: Some(20),
                output_tokens: Some(3),
                total_tokens: Some(23),
                ..ProviderUsageSnapshot::default()
            }),
            provider_call_count: 2,
            unknown_usage_call_count: 0,
            source_turn_stats_record_id: Some(String::from("child-stats")),
        })
    );
    let duplicate_main = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::AssistantResponse,
        1,
        Some(&ProviderUsageSnapshot {
            input_tokens: Some(10),
            output_tokens: Some(2),
            total_tokens: Some(12),
            ..ProviderUsageSnapshot::default()
        }),
    );
    let duplicate_child = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        2,
        Some(&ProviderUsageSnapshot {
            input_tokens: Some(20),
            output_tokens: Some(3),
            total_tokens: Some(23),
            ..ProviderUsageSnapshot::default()
        }),
    );

    let stats = build_session_turn_stats_from_state_with_ledger_and_segments(
        &conversation_state,
        &ledger,
        "summary-1",
        SessionTurnStatsStatus::Completed,
        SessionTurnStatsKind::UserTurn,
        vec![duplicate_main, duplicate_child],
    )
    .expect("ledger-backed Turn Stats should materialize");

    assert_eq!(stats.provider_call_count, 3);
    assert_eq!(stats.input_tokens, 30);
    assert_eq!(stats.output_tokens, 5);
    assert_eq!(stats.total_tokens, 35);
    assert_eq!(stats.usage_segments.len(), 2);
    assert_eq!(stats.child_contribution_revision, 7);
    assert_eq!(stats.source_ledger_revision, ledger.revision);
    assert_eq!(
        stats
            .usage_groups
            .iter()
            .find(|group| group.kind == SessionTurnUsageGroupKind::Subagent)
            .map(|group| group.provider_call_count),
        Some(2)
    );
}

/// Verifies rebuilding an older Turn Stats node never consumes calls from a later turn.
/// 验证重建较早的 Turn Stats 节点时绝不会误用后续回合调用。
#[test]
fn ledger_turn_stats_rebuilds_the_exact_closing_boundary() {
    let conversation_state = SessionConversationState {
        records: vec![
            user_record("user-1", "100", "first"),
            summary_record("summary-1", "200"),
            turn_stats_record("stats-1", "201"),
            user_record("user-2", "300", "second"),
            summary_record("summary-2", "400"),
        ],
        ..SessionConversationState::default()
    };
    let mut ledger = SessionProviderCallLedger::default();
    append_turn_stats_provider_call(
        &mut ledger,
        "first-call",
        "main_agent",
        "turn-1",
        "summary-1",
        ProviderUsageSnapshot {
            input_tokens: Some(10),
            uncached_input_tokens: Some(10),
            output_tokens: Some(1),
            total_tokens: Some(11),
            ..ProviderUsageSnapshot::default()
        },
    );
    append_turn_stats_provider_call(
        &mut ledger,
        "second-call",
        "main_agent",
        "turn-2",
        "summary-2",
        ProviderUsageSnapshot {
            input_tokens: Some(20),
            uncached_input_tokens: Some(20),
            output_tokens: Some(2),
            total_tokens: Some(22),
            ..ProviderUsageSnapshot::default()
        },
    );

    let stats = build_session_turn_stats_from_state_with_ledger_and_segments(
        &conversation_state,
        &ledger,
        "summary-1",
        SessionTurnStatsStatus::Completed,
        SessionTurnStatsKind::UserTurn,
        Vec::new(),
    )
    .expect("historical Turn Stats should rebuild from its exact closing boundary");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 10);
    assert_eq!(stats.uncached_input_tokens, Some(10));
    assert_eq!(stats.output_tokens, 1);
    assert_eq!(stats.total_tokens, 11);
}

/// Builds one canonical user message with the provided local identifier and text.
/// 使用指定本地标识与文本构建一条规范用户消息。
fn user_message(id: &str, text: &str) -> SessionConversationMessage {
    SessionConversationMessage {
        id: id.to_string(),
        payload: SessionConversationPayload::UserText {
            text: text.to_string(),
            attachments: Vec::new(),
        },
    }
}

/// Builds one canonical assistant message carrying usage and one optional tool call.
/// 构建一条携带用量与可选工具调用的规范助手消息。
fn assistant_message(
    id: &str,
    usage: ProviderUsageSnapshot,
    include_tool_call: bool,
) -> SessionConversationMessage {
    SessionConversationMessage {
        id: id.to_string(),
        payload: SessionConversationPayload::Assistant {
            text: String::from("assistant output"),
            reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            usage: Some(Box::new(usage)),
            tool_calls: include_tool_call
                .then(|| SessionConversationToolCall {
                    id: String::from("tool-item-1"),
                    tool_call_id: Some(String::from("tool-call-1")),
                    tool_name: String::from("vulcan_file_list"),
                    arguments: json!({}),
                    argument_parse_error: None,
                })
                .into_iter()
                .collect(),
        },
    }
}

/// Verifies the shared usage-segment merger accumulates every statistics field.
/// 验证共享用量分段聚合器会累加所有统计字段。
#[test]
fn usage_segment_merger_accumulates_all_statistics_fields() {
    let mut stats = SessionTurnStats {
        provider_call_count: 1,
        tool_call_count: 2,
        input_tokens: 3,
        uncached_input_tokens: Some(2),
        output_tokens: 4,
        reasoning_tokens: 5,
        total_tokens: 12,
        cached_input_tokens: Some(7),
        cache_creation_input_tokens: Some(8),
        ..SessionTurnStats::default()
    };
    let segment = SessionTurnUsageSegment {
        kind: SessionTurnUsageSegmentKind::SubagentTask,
        trigger: None,
        execution_profile: None,
        provider_id: None,
        provider_name_snapshot: None,
        model_id: None,
        model_name_snapshot: None,
        provider_call_count: 10,
        tool_call_count: 20,
        input_tokens: 30,
        uncached_input_tokens: Some(20),
        output_tokens: 40,
        reasoning_tokens: 50,
        total_tokens: 120,
        cached_input_tokens: Some(70),
        cache_creation_input_tokens: Some(80),
    };

    merge_usage_segment_into_turn_stats(&mut stats, &segment);

    assert_eq!(stats.provider_call_count, 11);
    assert_eq!(stats.tool_call_count, 22);
    assert_eq!(stats.input_tokens, 33);
    assert_eq!(stats.uncached_input_tokens, Some(22));
    assert_eq!(stats.output_tokens, 44);
    assert_eq!(stats.reasoning_tokens, 55);
    assert_eq!(stats.total_tokens, 132);
    assert_eq!(stats.cached_input_tokens, Some(77));
    assert_eq!(stats.cache_creation_input_tokens, Some(88));
    assert_eq!(stats.usage_groups.len(), 1);
    assert_eq!(
        stats.usage_groups[0].kind,
        SessionTurnUsageGroupKind::Subagent
    );
    assert_eq!(stats.usage_groups[0].provider_call_count, 10);
    assert_eq!(stats.usage_groups[0].total_tokens, 120);
}

/// Verifies parent user-turn explicit child-agent segments stay out of aggregate totals.
/// 验证父级用户轮次中的显式智能体分段不会进入顶层聚合总数。
#[test]
fn explicit_subagent_segments_group_without_polluting_parent_turn_totals() {
    let mut stats = SessionTurnStats {
        turn_kind: SessionTurnStatsKind::UserTurn,
        provider_call_count: 1,
        input_tokens: 10,
        uncached_input_tokens: Some(10),
        output_tokens: 3,
        total_tokens: 13,
        ..SessionTurnStats::default()
    };
    let segment = SessionTurnUsageSegment {
        kind: SessionTurnUsageSegmentKind::SubagentTask,
        trigger: Some(String::from("worker-a")),
        execution_profile: None,
        provider_id: Some(String::from("deepseek")),
        provider_name_snapshot: Some(String::from("DeepSeek")),
        model_id: Some(String::from("deepseek-v4-pro")),
        model_name_snapshot: Some(String::from("DeepSeek V4 Pro")),
        provider_call_count: 1,
        tool_call_count: 0,
        input_tokens: 100,
        uncached_input_tokens: Some(60),
        output_tokens: 25,
        reasoning_tokens: 5,
        total_tokens: 130,
        cached_input_tokens: Some(40),
        cache_creation_input_tokens: None,
    };

    merge_explicit_usage_segment_into_turn_stats(&mut stats, &segment);

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 10);
    assert_eq!(stats.uncached_input_tokens, Some(10));
    assert_eq!(stats.output_tokens, 3);
    assert_eq!(stats.reasoning_tokens, 0);
    assert_eq!(stats.total_tokens, 13);
    assert_eq!(stats.cached_input_tokens, None);
    assert_eq!(stats.usage_groups.len(), 1);
    assert_eq!(
        stats.usage_groups[0].kind,
        SessionTurnUsageGroupKind::Subagent
    );
    assert_eq!(stats.usage_groups[0].provider_call_count, 1);
    assert_eq!(stats.usage_groups[0].input_tokens, 100);
    assert_eq!(stats.usage_groups[0].uncached_input_tokens, Some(60));
    assert_eq!(stats.usage_groups[0].total_tokens, 130);
}

/// Verifies wait_agent host-result usage is grouped into the parent turn without changing regular totals.
/// 验证 wait_agent 宿主结果用量会分组进父级回合，且不会改变常规顶层统计。
#[test]
fn wait_agent_host_usage_segments_group_without_polluting_parent_turn_totals() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "等待智能体"),
            assistant_message(
                "session-1-protocol-assistant-1001",
                ProviderUsageSnapshot {
                    input_tokens: Some(10),
                    output_tokens: Some(3),
                    total_tokens: Some(13),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-current", "1000", "等待智能体"),
            tool_result_record(
                "tool-wait-result",
                "1001",
                json!({
                    "tool_name": "wait_agent",
                    "host_result": {
                        "kind": "subagent_wait",
                        "payload": {
                            "tool": "wait_agent",
                            "agents": [{
                                "agent_id": "worker-a",
                                "subagent_task_id": "00000000-0000-0000-0000-000000000001",
                                "execution_id": "worker-a-run-1",
                                "usage_segments": [{
                                    "kind": "subagent_task",
                                    "trigger": "worker-a",
                                    "provider_call_count": 1,
                                    "tool_call_count": 0,
                                    "input_tokens": 100,
                                    "output_tokens": 25,
                                    "reasoning_tokens": 5,
                                    "total_tokens": 130,
                                    "cached_input_tokens": 40
                                }]
                            }]
                        }
                    }
                }),
            ),
            summary_record(closing_record_id, "1002"),
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 10);
    assert_eq!(stats.output_tokens, 3);
    assert_eq!(stats.reasoning_tokens, 0);
    assert_eq!(stats.total_tokens, 13);
    assert_eq!(stats.usage_groups.len(), 2);
    assert_eq!(
        stats.usage_groups[0].kind,
        SessionTurnUsageGroupKind::Regular
    );
    assert_eq!(
        stats.usage_groups[1].kind,
        SessionTurnUsageGroupKind::Subagent
    );
    assert_eq!(stats.usage_groups[1].input_tokens, 100);
    assert_eq!(stats.usage_groups[1].total_tokens, 130);
}

/// Verifies an unaligned current turn never inherits historical canonical usage or tool calls.
/// 验证无法对齐的当前轮次不会继承历史规范用量或工具调用。
#[test]
fn current_turn_stats_do_not_inherit_historical_canonical_messages() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "上一轮"),
            assistant_message(
                "session-1-protocol-assistant-1001",
                ProviderUsageSnapshot {
                    input_tokens: Some(1000),
                    output_tokens: Some(200),
                    total_tokens: Some(1200),
                    cached_input_tokens: Some(800),
                    ..ProviderUsageSnapshot::default()
                },
                true,
            ),
        ],
        records: vec![
            user_record("user-previous", "1000", "上一轮"),
            summary_record("summary-previous", "1002"),
            turn_stats_record("stats-previous", "1003"),
            user_record("user-current", "2000", "当前无工具"),
            summary_record(closing_record_id, "2002"),
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 0);
    assert_eq!(stats.tool_call_count, 0);
    assert_eq!(stats.input_tokens, 0);
    assert_eq!(stats.output_tokens, 0);
    assert_eq!(stats.total_tokens, 0);
    assert_eq!(stats.cached_input_tokens, None);
}

/// Verifies assistant-only terminal TaskSession reports can still build live usage stats.
/// 验证只有终态助手消息的智能体报告仍能构建实时用量统计。
#[test]
fn current_turn_stats_use_terminal_assistant_when_no_user_record_exists() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![assistant_message(
            "session-1-protocol-assistant-2001",
            ProviderUsageSnapshot {
                input_tokens: Some(31),
                output_tokens: Some(2),
                reasoning_tokens: Some(1),
                total_tokens: Some(34),
                cached_input_tokens: Some(7),
                ..ProviderUsageSnapshot::default()
            },
            false,
        )],
        records: vec![summary_record(closing_record_id, "2002")],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        Some(1),
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 31);
    assert_eq!(stats.output_tokens, 2);
    assert_eq!(stats.reasoning_tokens, 1);
    assert_eq!(stats.total_tokens, 34);
    assert_eq!(stats.cached_input_tokens, Some(7));
}

/// Verifies turn-triggering agent messages bound fallback stats to the current feedback turn.
/// 验证触发轮次的智能体消息会把 fallback 统计限定到当前反馈轮次。
#[test]
fn current_turn_stats_use_triggering_agent_message_when_no_user_record_exists() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "上一轮"),
            assistant_message(
                "session-1-protocol-assistant-1001",
                ProviderUsageSnapshot {
                    input_tokens: Some(1000),
                    output_tokens: Some(200),
                    total_tokens: Some(1200),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
            SessionConversationMessage {
                id: String::from("session-1-agent-message-2000"),
                payload: SessionConversationPayload::AgentMessage {
                    message_type: String::from("completed"),
                    task_name: String::from("agent-a"),
                    sender: String::from("agent-a"),
                    recipient: String::from("parent"),
                    payload: String::from("完成"),
                    trigger_turn: true,
                    metadata: None,
                },
            },
            assistant_message(
                "session-1-protocol-assistant-2001",
                ProviderUsageSnapshot {
                    input_tokens: Some(40),
                    output_tokens: Some(12),
                    total_tokens: Some(52),
                    cached_input_tokens: Some(16),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![summary_record(closing_record_id, "2002")],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        Some(1),
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 40);
    assert_eq!(stats.output_tokens, 12);
    assert_eq!(stats.total_tokens, 52);
    assert_eq!(stats.cached_input_tokens, Some(16));
}

/// Verifies display/runtime layer separation does not map the current record to the first historical user.
/// 验证显示层与运行层分离时不会把当前记录映射到首个历史用户。
#[test]
fn current_turn_stats_align_current_record_when_records_only_hold_active_turn() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "上一轮"),
            assistant_message(
                "session-1-protocol-assistant-1001",
                ProviderUsageSnapshot {
                    input_tokens: Some(1000),
                    output_tokens: Some(200),
                    total_tokens: Some(1200),
                    cached_input_tokens: Some(800),
                    ..ProviderUsageSnapshot::default()
                },
                true,
            ),
            user_message("session-1-protocol-user-2000", "当前无工具"),
            assistant_message(
                "session-1-protocol-assistant-2001",
                ProviderUsageSnapshot {
                    input_tokens: Some(40),
                    output_tokens: Some(12),
                    total_tokens: Some(52),
                    cached_input_tokens: Some(16),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-current", "2000", "当前无工具"),
            summary_record(closing_record_id, "2002"),
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 0);
    assert_eq!(stats.input_tokens, 40);
    assert_eq!(stats.output_tokens, 12);
    assert_eq!(stats.total_tokens, 52);
    assert_eq!(stats.cached_input_tokens, Some(16));
}

/// Verifies repeated user text does not let a historical user satisfy the ordinal candidate.
/// 验证重复用户文本不会让历史用户通过序号候选校验。
#[test]
fn current_turn_stats_reject_historical_candidate_with_repeated_user_text() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "继续"),
            assistant_message(
                "session-1-protocol-assistant-1001",
                ProviderUsageSnapshot {
                    input_tokens: Some(900),
                    output_tokens: Some(300),
                    total_tokens: Some(1200),
                    cached_input_tokens: Some(700),
                    ..ProviderUsageSnapshot::default()
                },
                true,
            ),
            user_message("session-1-protocol-user-2000", "继续"),
            assistant_message(
                "session-1-protocol-assistant-2001",
                ProviderUsageSnapshot {
                    input_tokens: Some(30),
                    output_tokens: Some(9),
                    total_tokens: Some(39),
                    cached_input_tokens: Some(12),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-current", "2000", "继续"),
            summary_record(closing_record_id, "2002"),
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 0);
    assert_eq!(stats.input_tokens, 30);
    assert_eq!(stats.output_tokens, 9);
    assert_eq!(stats.total_tokens, 39);
    assert_eq!(stats.cached_input_tokens, Some(12));
}

/// Verifies interleaved user prompt records stay inside the original running turn.
/// 验证中途插入的用户提示词记录仍属于原始运行轮次。
#[test]
fn current_turn_stats_keep_interleaved_user_prompts_inside_same_turn() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "上一轮"),
            assistant_message(
                "session-1-protocol-assistant-1001",
                ProviderUsageSnapshot {
                    input_tokens: Some(500),
                    output_tokens: Some(100),
                    total_tokens: Some(600),
                    cached_input_tokens: Some(400),
                    ..ProviderUsageSnapshot::default()
                },
                true,
            ),
            user_message("session-1-protocol-user-2000", "当前任务"),
            user_message("session-1-protocol-user-2001", "中途补充"),
            assistant_message(
                "session-1-protocol-assistant-2002",
                ProviderUsageSnapshot {
                    input_tokens: Some(80),
                    output_tokens: Some(20),
                    total_tokens: Some(100),
                    cached_input_tokens: Some(30),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-current", "2000", "当前任务"),
            interrupt_user_record("user-interleaved", "2001", "中途补充"),
            summary_record(closing_record_id, "2003"),
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 0);
    assert_eq!(stats.input_tokens, 80);
    assert_eq!(stats.output_tokens, 20);
    assert_eq!(stats.total_tokens, 100);
    assert_eq!(stats.cached_input_tokens, Some(30));
}

/// Verifies prior interrupt user messages do not shift the next turn's canonical alignment.
/// 验证上一轮插队用户消息不会挤偏下一轮的规范消息对齐。
#[test]
fn current_turn_stats_ignore_prior_interrupts_when_aligning_new_turn() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "上一轮"),
            user_message("session-1-protocol-user-1001", "上一轮插队"),
            assistant_message(
                "session-1-protocol-assistant-1002",
                ProviderUsageSnapshot {
                    input_tokens: Some(500),
                    output_tokens: Some(100),
                    total_tokens: Some(600),
                    cached_input_tokens: Some(400),
                    ..ProviderUsageSnapshot::default()
                },
                true,
            ),
            user_message("session-1-protocol-user-2000", "当前任务"),
            assistant_message(
                "session-1-protocol-assistant-2001",
                ProviderUsageSnapshot {
                    input_tokens: Some(70),
                    output_tokens: Some(14),
                    total_tokens: Some(84),
                    cached_input_tokens: Some(21),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-previous", "1000", "上一轮"),
            interrupt_user_record("user-previous-interrupt", "1001", "上一轮插队"),
            summary_record("summary-previous", "1003"),
            turn_stats_record("stats-previous", "1004"),
            user_record("user-current", "2000", "当前任务"),
            summary_record(closing_record_id, "2002"),
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 0);
    assert_eq!(stats.input_tokens, 70);
    assert_eq!(stats.output_tokens, 14);
    assert_eq!(stats.total_tokens, 84);
    assert_eq!(stats.cached_input_tokens, Some(21));
}

/// Verifies compacted windows can still align the current user by the shared creation timestamp.
/// 验证压缩后的窗口仍可通过共享创建时间对齐当前用户。
#[test]
fn compacted_current_turn_stats_align_by_user_record_timestamp() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-2000", "当前有输出"),
            assistant_message(
                "session-1-protocol-assistant-2001",
                ProviderUsageSnapshot {
                    input_tokens: Some(50),
                    output_tokens: Some(25),
                    total_tokens: Some(75),
                    cached_input_tokens: Some(10),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-previous", "1000", "上一轮"),
            summary_record("summary-previous", "1002"),
            turn_stats_record("stats-previous", "1003"),
            user_record("user-current", "2000", "当前有输出"),
            summary_record(closing_record_id, "2002"),
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 0);
    assert_eq!(stats.input_tokens, 50);
    assert_eq!(stats.output_tokens, 25);
    assert_eq!(stats.total_tokens, 75);
    assert_eq!(stats.cached_input_tokens, Some(10));
}

/// Verifies child-agent feedback stats do not truncate the owning parent turn duration.
/// 验证智能体反馈统计不会截断所属父回合的耗时范围。
#[test]
fn parent_turn_stats_ignore_subagent_feedback_stats_as_boundary() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "上一轮"),
            assistant_message(
                "session-1-protocol-assistant-1001",
                ProviderUsageSnapshot {
                    input_tokens: Some(1000),
                    output_tokens: Some(200),
                    total_tokens: Some(1200),
                    cached_input_tokens: Some(800),
                    ..ProviderUsageSnapshot::default()
                },
                true,
            ),
            user_message("session-1-protocol-user-2000", "当前调度智能体"),
            assistant_message(
                "session-1-protocol-assistant-2001",
                ProviderUsageSnapshot {
                    input_tokens: Some(70),
                    output_tokens: Some(14),
                    total_tokens: Some(84),
                    cached_input_tokens: Some(21),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-previous", "1000", "上一轮"),
            summary_record("summary-previous", "1500"),
            turn_stats_record("stats-previous", "1600"),
            user_record("user-current", "2000", "当前调度智能体"),
            subagent_turn_stats_record("subagent-stats-current", "5000"),
            summary_record(closing_record_id, "12000"),
        ],
        continuation: Default::default(),
    };

    let stats = build_session_turn_stats_from_state(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 70);
    assert_eq!(stats.output_tokens, 14);
    assert_eq!(stats.total_tokens, 84);
    assert_eq!(stats.cached_input_tokens, Some(21));
    assert_eq!(stats.duration_ms, Some(10_000));
}

/// Verifies embedded compaction usage segments are aggregated into a normal user turn.
/// 验证嵌入式压缩用量分段会聚合进普通用户轮次。
#[test]
fn user_turn_stats_merge_embedded_context_compaction_segment() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-2000", "当前任务"),
            assistant_message(
                "session-1-protocol-assistant-2001",
                ProviderUsageSnapshot {
                    input_tokens: Some(40),
                    output_tokens: Some(12),
                    total_tokens: Some(52),
                    cached_input_tokens: Some(16),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-current", "2000", "当前任务"),
            summary_record(closing_record_id, "2002"),
        ],
        continuation: Default::default(),
    };
    let mut compaction_segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::ContextCompaction,
        1,
        Some(&ProviderUsageSnapshot {
            input_tokens: Some(20),
            output_tokens: Some(4),
            reasoning_tokens: Some(2),
            total_tokens: Some(26),
            cached_input_tokens: Some(5),
            ..ProviderUsageSnapshot::default()
        }),
    );
    compaction_segment.trigger = Some(String::from("auto_budget"));
    compaction_segment.execution_profile = Some(String::from("cache_preserving_full_fidelity"));

    let stats = build_session_turn_stats_from_state_with_segments(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        Some(1),
        SessionTurnStatsKind::UserTurn,
        vec![compaction_segment],
    )
    .expect("turn stats should be built");

    assert_eq!(stats.turn_kind, SessionTurnStatsKind::UserTurn);
    assert_eq!(stats.provider_call_count, 2);
    assert_eq!(stats.input_tokens, 60);
    assert_eq!(stats.output_tokens, 16);
    assert_eq!(stats.reasoning_tokens, 2);
    assert_eq!(stats.total_tokens, 78);
    assert_eq!(stats.cached_input_tokens, Some(21));
    assert_eq!(stats.usage_segments.len(), 2);
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );
    assert_eq!(
        stats.usage_segments[1].kind,
        SessionTurnUsageSegmentKind::ContextCompaction
    );
    assert_eq!(stats.usage_groups.len(), 2);
    assert_eq!(
        stats.usage_groups[0].kind,
        SessionTurnUsageGroupKind::Regular
    );
    assert_eq!(stats.usage_groups[0].total_tokens, 52);
    assert_eq!(
        stats.usage_groups[1].kind,
        SessionTurnUsageGroupKind::ContextCompaction
    );
    assert_eq!(stats.usage_groups[1].total_tokens, 26);
}

/// Verifies assistant provider retries remain visible when compaction usage is embedded.
/// 验证嵌入压缩用量时仍能保留助手响应的 Provider 重试次数。
#[test]
fn user_turn_stats_apply_provider_override_to_assistant_segment_with_compaction_segment() {
    let closing_record_id = "summary-current";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-2000", "当前任务"),
            assistant_message(
                "session-1-protocol-assistant-2001",
                ProviderUsageSnapshot {
                    input_tokens: Some(40),
                    output_tokens: Some(12),
                    total_tokens: Some(52),
                    cached_input_tokens: Some(16),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("user-current", "2000", "当前任务"),
            summary_record(closing_record_id, "2002"),
        ],
        continuation: Default::default(),
    };
    let compaction_segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::ContextCompaction,
        1,
        Some(&ProviderUsageSnapshot {
            input_tokens: Some(20),
            output_tokens: Some(4),
            total_tokens: Some(24),
            ..ProviderUsageSnapshot::default()
        }),
    );

    let stats = build_session_turn_stats_from_state_with_segments(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        Some(2),
        SessionTurnStatsKind::UserTurn,
        vec![compaction_segment],
    )
    .expect("turn stats should be built");

    assert_eq!(stats.provider_call_count, 3);
    assert_eq!(stats.usage_segments.len(), 2);
    assert_eq!(stats.usage_segments[0].provider_call_count, 2);
    assert_eq!(stats.usage_segments[1].provider_call_count, 1);
    assert_eq!(
        stats
            .usage_segments
            .iter()
            .map(|segment| segment.provider_call_count)
            .sum::<usize>(),
        stats.provider_call_count
    );
}

/// Verifies failed assistant requests remain represented when compaction usage is embedded.
/// 验证嵌入压缩用量时失败的助手请求仍会保留统计分段。
#[test]
fn user_turn_stats_keep_failed_assistant_request_segment_with_compaction_segment() {
    let closing_record_id = "terminated-current";
    let conversation_state = SessionConversationState {
        messages: vec![user_message("session-1-protocol-user-2000", "当前任务")],
        records: vec![
            user_record("user-current", "2000", "当前任务"),
            SessionConversationRecord {
                id: closing_record_id.to_string(),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::TurnTerminated,
                visible: true,
                title: String::from("执行失败"),
                body: String::from("Provider request failed before assistant usage was recorded."),
                metadata: None,
                created_at: String::from("2003"),
            },
        ],
        continuation: Default::default(),
    };
    let compaction_segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::ContextCompaction,
        1,
        Some(&ProviderUsageSnapshot {
            input_tokens: Some(20),
            output_tokens: Some(4),
            total_tokens: Some(24),
            ..ProviderUsageSnapshot::default()
        }),
    );

    let stats = build_session_turn_stats_from_state_with_segments(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Failed,
        Some(2),
        SessionTurnStatsKind::UserTurn,
        vec![compaction_segment],
    )
    .expect("turn stats should be built");

    assert_eq!(stats.status, SessionTurnStatsStatus::Failed);
    assert_eq!(stats.provider_call_count, 3);
    assert_eq!(stats.usage_segments.len(), 2);
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );
    assert_eq!(stats.usage_segments[0].provider_call_count, 2);
    assert_eq!(stats.usage_segments[0].total_tokens, 0);
    assert_eq!(
        stats.usage_segments[1].kind,
        SessionTurnUsageSegmentKind::ContextCompaction
    );
    assert_eq!(
        stats
            .usage_segments
            .iter()
            .map(|segment| segment.provider_call_count)
            .sum::<usize>(),
        stats.provider_call_count
    );
}

/// Verifies manual compaction can produce an independent context-compaction turn without a user prompt.
/// 验证手动压缩可以在没有用户提示词的情况下生成独立上下文压缩轮次。
#[test]
fn context_compaction_turn_stats_can_be_built_from_explicit_segment_only() {
    let closing_record_id = "compaction-end";
    let conversation_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![SessionConversationRecord {
            id: closing_record_id.to_string(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::CompactionStatus,
            visible: true,
            title: String::from("context_compaction.completed"),
            body: String::new(),
            metadata: None,
            created_at: String::from("3000"),
        }],
        continuation: Default::default(),
    };
    let compaction_segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::ContextCompaction,
        1,
        Some(&ProviderUsageSnapshot {
            input_tokens: Some(100),
            output_tokens: Some(30),
            total_tokens: Some(130),
            cache_creation_input_tokens: Some(10),
            ..ProviderUsageSnapshot::default()
        }),
    );

    let stats = build_session_turn_stats_from_state_with_segments(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
        SessionTurnStatsKind::ContextCompaction,
        vec![compaction_segment],
    )
    .expect("context-compaction stats should be built");

    assert_eq!(stats.turn_kind, SessionTurnStatsKind::ContextCompaction);
    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 100);
    assert_eq!(stats.output_tokens, 30);
    assert_eq!(stats.total_tokens, 130);
    assert_eq!(stats.cache_creation_input_tokens, Some(10));
    assert_eq!(stats.usage_segments.len(), 1);
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::ContextCompaction
    );
    assert_eq!(stats.usage_groups.len(), 1);
    assert_eq!(
        stats.usage_groups[0].kind,
        SessionTurnUsageGroupKind::ContextCompaction
    );
    assert_eq!(stats.usage_groups[0].cache_creation_input_tokens, Some(10));
}

/// Verifies standalone manual compaction stats do not inherit the previous assistant response.
/// 验证独立手动压缩统计不会继承上一轮助手响应消耗。
#[test]
fn context_compaction_turn_stats_ignore_previous_assistant_usage() {
    let closing_record_id = "compaction-end";
    let conversation_state = SessionConversationState {
        messages: vec![
            user_message("session-1-protocol-user-1000", "上一轮任务"),
            assistant_message(
                "session-1-protocol-assistant-1001",
                ProviderUsageSnapshot {
                    input_tokens: Some(40),
                    output_tokens: Some(12),
                    reasoning_tokens: Some(3),
                    total_tokens: Some(55),
                    cached_input_tokens: Some(16),
                    ..ProviderUsageSnapshot::default()
                },
                false,
            ),
        ],
        records: vec![
            user_record("previous-user", "1000", "上一轮任务"),
            turn_stats_record("previous-stats", "2000"),
            context_compaction_record("compaction-start", "section-1", "start", "3000"),
            context_compaction_record(closing_record_id, "section-1", "end", "3040"),
        ],
        continuation: Default::default(),
    };
    let compaction_segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::ContextCompaction,
        1,
        Some(&ProviderUsageSnapshot {
            input_tokens: Some(20),
            output_tokens: Some(4),
            reasoning_tokens: Some(2),
            total_tokens: Some(26),
            cached_input_tokens: Some(5),
            ..ProviderUsageSnapshot::default()
        }),
    );

    let stats = build_session_turn_stats_from_state_with_segments(
        &conversation_state,
        closing_record_id,
        SessionTurnStatsStatus::Completed,
        None,
        SessionTurnStatsKind::ContextCompaction,
        vec![compaction_segment],
    )
    .expect("context-compaction stats should be built");

    assert_eq!(stats.turn_kind, SessionTurnStatsKind::ContextCompaction);
    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 0);
    assert_eq!(stats.input_tokens, 20);
    assert_eq!(stats.output_tokens, 4);
    assert_eq!(stats.reasoning_tokens, 2);
    assert_eq!(stats.total_tokens, 26);
    assert_eq!(stats.cached_input_tokens, Some(5));
    assert_eq!(stats.duration_ms, Some(40));
    assert_eq!(stats.usage_segments.len(), 1);
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::ContextCompaction
    );
    assert_eq!(stats.usage_groups.len(), 1);
    assert_eq!(
        stats.usage_groups[0].kind,
        SessionTurnUsageGroupKind::ContextCompaction
    );
}
