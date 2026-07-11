//! goal todolist tests.
//! goal todolist tests。

use super::*;
use crate::pending_prompts::clear_stale_session_goal_pending_prompts;
use crate::session_goal_lifecycle::session_goal_execution_context;
use vulcan_workbench_model::PendingPromptRuntimeOptions;

/// Verifies todolist host results are projected into session runtime todolist state.
/// 验证 todolist 宿主结果会投影为会话运行态 todolist 状态。
#[test]
fn todolist_state_from_tool_finished_event_parses_host_result() {
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "todolist finished",
        "Todolist updated",
    )
    .with_metadata(Some(json!({
        "tool_name": "todolist",
        "tool_call_id": "call-todo",
        "host_result": {
            "kind": "todolist",
            "payload": {
                "explanation": "同步任务列表",
                "items": [
                    { "step": "阅读参考实现", "status": "completed" },
                    { "step": "接入右侧面板", "status": "in_progress" },
                    { "step": "补充验证", "status": "pending" }
                ]
            }
        }
    })));

    let state = todolist_state_from_tool_finished_event(&event, "2026-06-17T00:00:00Z")
        .expect("todo host result should parse");

    assert_eq!(state.explanation.as_deref(), Some("同步任务列表"));
    assert_eq!(state.updated_at.as_deref(), Some("2026-06-17T00:00:00Z"));
    assert_eq!(state.items.len(), 3);
    assert_eq!(state.items[0].status, SessionTodolistItemStatus::Completed);
    assert_eq!(state.items[1].status, SessionTodolistItemStatus::InProgress);
    assert_eq!(state.items[2].status, SessionTodolistItemStatus::Pending);
}

/// Verifies user-prompt edit gating blocks applied file changes and child TaskSession cards.
/// 验证用户输入编辑门禁会阻断已应用文件变更与子代理 TaskSession 卡片。
#[test]
fn user_prompt_edit_gate_blocks_applied_changes_and_task_sessions() {
    let first_user = SessionMessageEntry::new(SessionMessageKind::User, "", "first")
        .with_runtime_metadata("user-1", "1780000000000");
    let mut applied_change = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "调用 vulcan_file_edit 工具",
        "applied",
    )
    .with_runtime_metadata("tool-change", "1780000000001");
    applied_change.record_kind = SessionMessageRecordKind::ToolResultOutput;
    applied_change.metadata = Some(json!({
        "host_result": {
            "kind": "change_set",
            "payload": {
                "mode": "applied"
            }
        }
    }));
    let second_user = SessionMessageEntry::new(SessionMessageKind::User, "", "second")
        .with_runtime_metadata("user-2", "1780000000002");
    let task_message = test_spawn_agent_tool_message(
        SessionId::new_v4(),
        "completed",
        SessionMessageRecordKind::ToolResultOutput,
    );
    let third_user = SessionMessageEntry::new(SessionMessageKind::User, "", "third")
        .with_runtime_metadata("user-3", "1780000000003");

    let ids = editable_user_prompt_message_ids_from_entries(&[
        first_user,
        applied_change,
        second_user,
        task_message,
        third_user,
    ]);

    assert!(!ids.contains("user-1"));
    assert!(!ids.contains("user-2"));
    assert!(ids.contains("user-3"));
}

/// Verifies preview-only change sets do not block user-prompt editing.
/// 验证仅预览的变更集不会阻断用户输入编辑。
#[test]
fn user_prompt_edit_gate_allows_preview_only_change_sets() {
    let user = SessionMessageEntry::new(SessionMessageKind::User, "", "first")
        .with_runtime_metadata("user-preview", "1780000000000");
    let mut preview_change = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "调用 vulcan_file_edit 工具",
        "preview",
    )
    .with_runtime_metadata("tool-preview", "1780000000001");
    preview_change.record_kind = SessionMessageRecordKind::ToolResultOutput;
    preview_change.metadata = Some(json!({
        "host_result": {
            "kind": "change_set",
            "payload": {
                "mode": "preview"
            }
        }
    }));

    let ids = editable_user_prompt_message_ids_from_entries(&[user, preview_change]);

    assert!(ids.contains("user-preview"));
}

/// Verifies interrupt prompt records are folded into the turn and cannot be edited.
/// 验证插队提示词记录会折叠进轮次过程且不可编辑。
#[test]
fn user_prompt_edit_gate_rejects_interrupt_prompt_records() {
    let regular_user = SessionMessageEntry::new(SessionMessageKind::User, "", "regular")
        .with_runtime_metadata("user-regular", "1780000000000");
    let mut interrupt_user = SessionMessageEntry::new(SessionMessageKind::User, "", "interrupt")
        .with_runtime_metadata("user-interrupt", "1780000000001");
    interrupt_user.metadata = Some(json!({
        "source": "user_interrupt",
        "starts_user_turn": false,
        "turn_process": true,
        "collapsed_by_default": true,
        "editable": false,
    }));

    let ids = editable_user_prompt_message_ids_from_entries(&[regular_user, interrupt_user]);

    assert!(ids.contains("user-regular"));
    assert!(!ids.contains("user-interrupt"));
}

/// Verifies malformed todolist host results are rejected as a whole.
/// 验证格式错误的 todolist 宿主结果会整体拒绝。
#[test]
fn todolist_state_from_tool_finished_event_rejects_invalid_items() {
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "todolist finished",
        "Todolist updated",
    )
    .with_metadata(Some(json!({
        "host_result": {
            "kind": "todolist",
            "payload": {
                "items": [
                    { "step": "有效条目", "status": "completed" },
                    { "step": "", "status": "pending" }
                ]
            }
        }
    })));

    assert!(todolist_state_from_tool_finished_event(&event, "2026-06-17T00:00:00Z").is_none());
}

/// Verifies WebUI todolist summaries preserve status keys and completed counts.
/// 验证 WebUI todolist 摘要会保留状态键与完成数量。
#[test]
fn session_todolist_summary_counts_completed_items() {
    let runtime_state = SessionRuntimeStateEntry {
        todolist: SessionTodolistState {
            explanation: Some(String::from("审核修复")),
            items: vec![
                SessionTodolistItem {
                    step: String::from("完成菜单接入"),
                    status: SessionTodolistItemStatus::Completed,
                },
                SessionTodolistItem {
                    step: String::from("补充测试"),
                    status: SessionTodolistItemStatus::InProgress,
                },
                SessionTodolistItem {
                    step: String::from("等待复核"),
                    status: SessionTodolistItemStatus::Pending,
                },
            ],
            updated_at: Some(String::from("2026-06-17T00:00:00Z")),
        },
        ..SessionRuntimeStateEntry::default()
    };

    let summary = session_todolist_summary_from_runtime_state(Some(&runtime_state))
        .expect("summary should exist");

    assert_eq!(summary.explanation.as_deref(), Some("审核修复"));
    assert_eq!(summary.total_count, 3);
    assert_eq!(summary.completed_count, 1);
    assert_eq!(summary.items[0].step, "完成菜单接入");
    assert_eq!(summary.items[0].status, "completed");
    assert_eq!(summary.items[1].step, "补充测试");
    assert_eq!(summary.items[1].status, "in_progress");
    assert_eq!(summary.items[2].step, "等待复核");
    assert_eq!(summary.items[2].status, "pending");
    assert_eq!(summary.updated_at.as_deref(), Some("2026-06-17T00:00:00Z"));
}

/// Verifies Goal tool host results preserve updated, cleared, and unrelated event states.
/// 验证 Goal 工具宿主结果会保留更新、清理与无关事件三种状态。
#[test]
fn goal_summary_from_tool_finished_event_preserves_three_states() {
    let session_id = SessionId::new_v4();
    let goal_id = Uuid::new_v4();
    let updated_event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "goal finished",
        "Goal updated",
    )
    .with_metadata(Some(json!({
        "host_result": {
            "kind": "session_goal",
            "payload": {
                "goal": {
                    "sessionId": session_id.to_string(),
                    "goalId": goal_id.to_string(),
                    "objective": "完成三态测试",
                    "status": "active",
                    "tokenBudget": 1200,
                    "tokensUsed": 34,
                    "usageTokens": 35,
                    "apiCallCount": 2,
                    "toolCallCount": 3,
                    "inputTokens": 100,
                    "cachedInputTokens": 10,
                    "outputTokens": 24,
                    "totalTokens": 124,
                    "timeUsedSeconds": 7,
                    "createdAtMs": 1000,
                    "updatedAtMs": 2000
                }
            }
        }
    })));
    let cleared_event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "goal cleared",
        "Goal cleared",
    )
    .with_metadata(Some(json!({
        "host_result": {
            "kind": "session_goal",
            "payload": {
                "goal": null
            }
        }
    })));
    let unrelated_event = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "goal started",
        "Goal started",
    )
    .with_metadata(Some(json!({
        "host_result": {
            "kind": "session_goal",
            "payload": {
                "goal": null
            }
        }
    })));

    let Some(Some(summary)) = goal_summary_from_tool_finished_event(&updated_event)
        .expect("valid updated goal event should parse")
    else {
        panic!("updated goal event should parse to a summary");
    };

    assert_eq!(summary.session_id, session_id.to_string());
    assert_eq!(summary.goal_id, goal_id.to_string());
    assert_eq!(summary.objective, "完成三态测试");
    assert_eq!(summary.status, SessionGoalStatus::Active);
    assert_eq!(summary.token_budget, Some(1200));
    assert_eq!(summary.tokens_used, 34);
    assert_eq!(summary.usage_tokens, 35);
    assert_eq!(
        goal_summary_from_tool_finished_event(&cleared_event),
        Ok(Some(None))
    );
    assert_eq!(
        goal_summary_from_tool_finished_event(&unrelated_event),
        Ok(None)
    );
}

/// Verifies declared goal host results reject malformed payloads instead of disappearing silently.
/// 验证已声明的目标宿主结果会拒绝畸形载荷，而不是静默消失。
#[test]
fn goal_summary_from_tool_finished_event_rejects_malformed_goal_payload() {
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "goal finished",
        "Goal updated",
    )
    .with_metadata(Some(json!({
        "host_result": {
            "kind": "session_goal",
            "payload": {
                "goal": {
                    "sessionId": "not-a-session-id"
                }
            }
        }
    })));

    let error = goal_summary_from_tool_finished_event(&event)
        .expect_err("malformed declared goal projection must fail");

    assert_eq!(
        error,
        "session_goal host result contains invalid field `sessionId`: expected UUID string"
    );
}

/// Verifies goal projection diagnostics identify a missing required field exactly.
/// 验证目标投影诊断会准确指出缺失的必填字段。
#[test]
fn goal_summary_from_tool_finished_event_reports_missing_required_field() {
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "goal finished",
        "Goal updated",
    )
    .with_metadata(Some(json!({
        "host_result": {
            "kind": "session_goal",
            "payload": {
                "goal": { "sessionId": SessionId::new_v4().to_string() }
            }
        }
    })));

    let error = goal_summary_from_tool_finished_event(&event)
        .expect_err("missing goalId must fail with field context");

    assert_eq!(
        error,
        "session_goal host result contains missing required field `goalId`"
    );
}

/// Verifies optional live counters default only when omitted and reject invalid supplied values.
/// 验证可选实时计数仅在省略时使用默认值，并拒绝已提供的无效取值。
#[test]
fn goal_summary_from_tool_finished_event_validates_optional_live_counter() {
    let base_goal = json!({
        "sessionId": SessionId::new_v4().to_string(),
        "goalId": Uuid::new_v4().to_string(),
        "objective": "验证可选计数",
        "status": "active",
        "tokenBudget": null,
        "tokensUsed": 12,
        "timeUsedSeconds": 3,
        "createdAtMs": 100,
        "updatedAtMs": 200
    });
    let build_event = |goal: Value| {
        RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "goal finished",
            "Goal updated",
        )
        .with_metadata(Some(json!({
            "host_result": {
                "kind": "session_goal",
                "payload": { "goal": goal }
            }
        })))
    };

    let Some(Some(summary)) =
        goal_summary_from_tool_finished_event(&build_event(base_goal.clone()))
            .expect("omitted optional counters should use zero defaults")
    else {
        panic!("valid goal projection should produce a summary");
    };
    assert_eq!(summary.usage_tokens, 12);
    assert_eq!(summary.api_call_count, 0);
    assert_eq!(summary.total_tokens, 0);
    let mut invalid_goal = base_goal;
    invalid_goal["apiCallCount"] = json!(-1);
    let error = goal_summary_from_tool_finished_event(&build_event(invalid_goal.clone()))
        .expect_err("negative optional count must not silently become zero");
    assert_eq!(
        error,
        "session_goal host result contains invalid field `apiCallCount`: expected non-negative integer fitting usize or null"
    );
    invalid_goal["apiCallCount"] = json!(0);
    invalid_goal["totalTokens"] = json!("12");
    let type_error = goal_summary_from_tool_finished_event(&build_event(invalid_goal))
        .expect_err("string token count must not silently become zero");
    assert_eq!(
        type_error,
        "session_goal host result contains invalid field `totalTokens`: expected non-negative integer or null"
    );
}

/// Verifies persisted goal status and positive-budget range contracts are enforced.
/// 验证持久化目标状态与正预算范围契约会被强制执行。
#[test]
fn goal_summary_from_tool_finished_event_rejects_invalid_status_and_budget_range() {
    let mut goal = json!({
        "sessionId": SessionId::new_v4().to_string(),
        "goalId": Uuid::new_v4().to_string(),
        "objective": "验证字段范围",
        "status": "unknown",
        "tokenBudget": null,
        "tokensUsed": 0,
        "timeUsedSeconds": 0,
        "createdAtMs": 100,
        "updatedAtMs": 100
    });
    let build_event = |goal: Value| {
        RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "goal finished",
            "Goal updated",
        )
        .with_metadata(Some(json!({
            "host_result": {
                "kind": "session_goal",
                "payload": { "goal": goal }
            }
        })))
    };

    let status_error = goal_summary_from_tool_finished_event(&build_event(goal.clone()))
        .expect_err("unknown persisted status must fail");
    assert_eq!(
        status_error,
        "session_goal host result contains invalid field `status`: expected active, paused, blocked, usage_limited, budget_limited, or complete"
    );
    goal["status"] = json!("active");
    goal["tokenBudget"] = json!(0);
    let budget_error = goal_summary_from_tool_finished_event(&build_event(goal.clone()))
        .expect_err("zero token budget must fail");
    assert_eq!(
        budget_error,
        "session_goal host result contains invalid field `tokenBudget`: expected positive integer or null"
    );
    goal["tokenBudget"] = Value::Null;
    goal["objective"] = json!(
        "x".repeat(vulcan_workspace_store::workspace_store::MAX_THREAD_GOAL_OBJECTIVE_CHARS + 1)
    );
    let objective_error = goal_summary_from_tool_finished_event(&build_event(goal))
        .expect_err("oversized objective must fail the shared store contract");
    assert_eq!(
        objective_error,
        "session_goal host result contains invalid field `objective`: expected string with at most 4000 characters"
    );
}

/// Verifies a goal projection cannot route one runtime event into another session's goal cache.
/// 验证目标投影不能把当前运行时事件路由到另一个会话的目标缓存。
#[test]
fn runtime_event_goal_projection_rejects_cross_session_ownership_before_commit() {
    let runtime_session_id = SessionId::new_v4();
    let foreign_session_id = SessionId::new_v4();
    let service_state = test_service_state_with_parent_session(runtime_session_id);
    let workspace = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot should exist");
    let original_messages = workspace.session_messages.clone();
    let original_runtime_state = workspace.session_runtime_state.clone();
    let original_hydrated_sessions = workspace.hydrated_session_ids.clone();
    let original_selected_session = workspace.selected_session;
    let state = Arc::new(Mutex::new(service_state));
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "goal finished",
        "Goal updated",
    )
    .with_metadata(Some(json!({
        "host_result": {
            "kind": "session_goal",
            "payload": {
                "goal": {
                    "sessionId": foreign_session_id.to_string(),
                    "goalId": Uuid::new_v4().to_string(),
                    "objective": "不得污染其他会话",
                    "status": "active",
                    "tokenBudget": null,
                    "tokensUsed": 0,
                    "timeUsedSeconds": 0,
                    "createdAtMs": 1,
                    "updatedAtMs": 1
                }
            }
        }
    })));

    let error = append_runtime_event_projection(&app, &state, runtime_session_id, &event)
        .expect_err("cross-session goal projection must fail preflight");

    assert!(error.contains("session_goal host result belongs to session"));
    let guard = state.lock().expect("service state lock");
    assert!(guard.session_goal_summaries.is_empty());
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot should remain");
    assert_eq!(
        serde_json::to_value(&workspace.session_messages)
            .expect("retained session messages should serialize"),
        serde_json::to_value(&original_messages)
            .expect("original session messages should serialize")
    );
    assert_eq!(
        serde_json::to_value(&workspace.session_runtime_state)
            .expect("retained runtime state should serialize"),
        serde_json::to_value(&original_runtime_state)
            .expect("original runtime state should serialize")
    );
    assert_eq!(workspace.hydrated_session_ids, original_hydrated_sessions);
    assert_eq!(workspace.selected_session, original_selected_session);
}

/// Verifies the active-goal continuation prompt keeps Codex's full completion and blocked audits.
/// 验证活跃目标续跑提示词保留 Codex 的完整完成审计与阻塞审计。
#[test]
fn goal_continuation_prompt_keeps_codex_audit_details() {
    let prompt = render_goal_continuation_context(&SessionGoalSummary {
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-1"),
        objective: String::from("完成目标模式"),
        status: SessionGoalStatus::Active,
        token_budget: Some(10_000),
        tokens_used: 1_234,
        usage_tokens: 1_234,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 56,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    });

    assert!(prompt.contains("<objective>\n完成目标模式\n</objective>"));
    assert!(prompt.contains("Tokens remaining: 8766"));
    assert!(prompt.contains("Derive concrete requirements"));
    assert!(prompt.contains("The audit must prove completion"));
    assert!(prompt.contains("same blocking condition"));
    assert!(prompt.contains("original/user-triggered turn"));
    assert!(prompt.contains("truly at an impasse"));
    assert!(prompt.contains("call update_goal with status \"blocked\""));
}

/// Verifies prompt rendering keeps the durable goal identity in the execution binding.
/// 验证提示词渲染会在执行绑定中保留持久目标身份。
#[test]
fn goal_execution_context_binds_prompt_to_goal_identity() {
    let goal = SessionGoalSummary {
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-bound-to-run"),
        objective: String::from("验证目标身份绑定"),
        status: SessionGoalStatus::Active,
        token_budget: None,
        tokens_used: 0,
        usage_tokens: 0,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 0,
        created_at_ms: 1_000,
        updated_at_ms: 1_000,
    };

    let context = session_goal_execution_context(&goal).expect("active goal context");

    assert_eq!(context.goal_id, goal.goal_id);
    assert!(context.prompt_context.contains("验证目标身份绑定"));
}

/// Verifies retiring one Goal stage removes live and crash-restored guidance owned by that stage.
/// 验证退役 Goal 阶段会移除该阶段拥有的实时与崩溃恢复引导。
#[test]
fn retired_goal_stage_clears_live_and_restored_guidance() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    let workspace = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_default();
    runtime_state
        .ui_state
        .pending_prompt_queue
        .push(PendingSessionPromptEntry {
            id: String::from("old-guidance"),
            prompt: String::from("old stage"),
            hidden_model_guidance: None,
            preview: String::from("old stage"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            source: PendingSessionPromptSource::GoalContinuation,
            task_id: Some(String::from("goal-stage-old")),
            panel_entry_id: Some(String::from("old-guidance")),
            created_at: String::from("1"),
        });
    runtime_state
        .ui_state
        .pending_prompt_queue
        .push(PendingSessionPromptEntry {
            id: String::from("new-guidance"),
            prompt: String::from("new stage"),
            hidden_model_guidance: None,
            preview: String::from("new stage"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            source: PendingSessionPromptSource::GoalContinuation,
            task_id: Some(String::from("goal-stage-new")),
            panel_entry_id: Some(String::from("new-guidance")),
            created_at: String::from("2"),
        });
    service_state
        .budget_limit_reported_goal_ids
        .insert(String::from("goal-stage-old"));
    let state = Arc::new(Mutex::new(service_state));

    let removed = clear_session_goal_stage_runtime_state(&state, session_id, "goal-stage-old")
        .expect("retired stage cleanup");

    assert_eq!(removed, 1);
    let guard = state.lock().expect("service state lock");
    assert!(
        !guard
            .budget_limit_reported_goal_ids
            .contains("goal-stage-old")
    );
    let queue = &guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state")
        .ui_state
        .pending_prompt_queue;
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].task_id.as_deref(), Some("goal-stage-new"));
    drop(guard);

    let mut guard = state.lock().expect("service state lock");
    guard
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot")
        .session_runtime_state
        .get_mut(&session_id)
        .expect("runtime state")
        .ui_state
        .pending_prompt_queue
        .push(PendingSessionPromptEntry {
            id: String::from("restored-old-guidance"),
            prompt: String::from("restored old stage"),
            hidden_model_guidance: None,
            preview: String::from("restored old stage"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            source: PendingSessionPromptSource::GoalContinuation,
            task_id: Some(String::from("goal-stage-old")),
            panel_entry_id: Some(String::from("restored-old-guidance")),
            created_at: String::from("3"),
        });
    drop(guard);

    assert_eq!(
        clear_stale_session_goal_pending_prompts(&state, session_id, Some("goal-stage-new"))
            .expect("dispatch-time cleanup"),
        1
    );
    let guard = state.lock().expect("service state lock");
    let queue = &guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state")
        .ui_state
        .pending_prompt_queue;
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].task_id.as_deref(), Some("goal-stage-new"));
}

/// Verifies objective-updated steering mirrors Codex's superseding-objective prompt.
/// 验证目标更新引导会对齐 Codex 的新目标覆盖旧目标提示词。
#[test]
fn goal_objective_updated_prompt_supersedes_previous_goal() {
    let prompt = render_goal_objective_updated_context(&SessionGoalSummary {
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-1"),
        objective: String::from("完成修改后的目标"),
        status: SessionGoalStatus::Active,
        token_budget: Some(10_000),
        tokens_used: 1_234,
        usage_tokens: 1_234,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 56,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    });

    assert!(prompt.contains("edited by the user"));
    assert!(prompt.contains("supersedes any previous thread goal objective"));
    assert!(prompt.contains("<untrusted_objective>\n完成修改后的目标\n</untrusted_objective>"));
    assert!(prompt.contains("Tokens remaining: 8766"));
    assert!(
        prompt.contains("Do not call update_goal unless the updated goal is actually complete.")
    );
}

/// Verifies budget-limited goals render a stage wrap-up instead of a completion instruction.
/// 验证预算耗尽目标会渲染阶段收尾提示而不是完成指令。
#[test]
fn goal_budget_limited_prompt_waits_for_user_resume() {
    let prompt = render_goal_budget_limited_context(&SessionGoalSummary {
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-budget"),
        objective: String::from("完成预算测试"),
        status: SessionGoalStatus::BudgetLimited,
        token_budget: Some(2_000),
        tokens_used: 2_345,
        usage_tokens: 2_345,
        api_call_count: 1,
        tool_call_count: 2,
        input_tokens: 1_000,
        cached_input_tokens: 100,
        output_tokens: 1_445,
        total_tokens: 2_445,
        time_used_seconds: 60,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    });

    assert!(prompt.contains("stage wrap-up, not goal completion"));
    assert!(prompt.contains("Wait for the user to decide whether to continue"));
    assert!(prompt.contains("Do not continue the next stage automatically"));
    assert!(prompt.contains("Do not call update_goal unless the goal is actually complete."));
}

/// Verifies stopped budget-limited goals do not inject ordinary prompt goal context.
/// 验证已预算耗尽的停止目标不会注入普通提示词目标上下文。
#[test]
fn budget_limited_goal_is_not_regular_prompt_context() {
    let goal = SessionGoalSummary {
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-budget"),
        objective: String::from("完成预算测试"),
        status: SessionGoalStatus::BudgetLimited,
        token_budget: Some(2_000),
        tokens_used: 2_345,
        usage_tokens: 2_345,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 60,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    };

    assert!(render_session_goal_context_for_prompt(&goal).is_none());
    assert_eq!(
        saved_goal_status_after_edit(ThreadGoalStatus::BudgetLimited),
        ThreadGoalStatus::BudgetLimited
    );
}

/// Verifies legacy hidden Goal prompts are stripped from restored user-visible messages.
/// 验证历史隐藏 Goal 提示会从恢复后的用户可见消息中剥离。
#[test]
fn filters_legacy_goal_internal_user_prompts_from_session_messages() {
    let messages = vec![
        SessionMessageEntry::new(
            SessionMessageKind::User,
            "用户指令",
            "Continue working toward the active thread goal.",
        ),
        SessionMessageEntry::new(
            SessionMessageKind::User,
            "用户指令",
            "The active thread goal has reached its token budget.\n\nBudget:",
        ),
        SessionMessageEntry::new(
            SessionMessageKind::User,
            "用户指令",
            "The active thread goal objective was edited by the user.",
        ),
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "真实用户消息"),
    ];

    let filtered = filter_legacy_goal_internal_user_messages(messages);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].body, "真实用户消息");
}
