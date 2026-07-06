//! goal todolist tests.
//! goal todolist tests。

use super::*;

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
                    "goalId": "goal-three-state",
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

    let Some(Some(summary)) = goal_summary_from_tool_finished_event(&updated_event) else {
        panic!("updated goal event should parse to a summary");
    };

    assert_eq!(summary.session_id, session_id.to_string());
    assert_eq!(summary.goal_id, "goal-three-state");
    assert_eq!(summary.objective, "完成三态测试");
    assert_eq!(summary.status, "active");
    assert_eq!(summary.token_budget, Some(1200));
    assert_eq!(summary.tokens_used, 34);
    assert_eq!(summary.usage_tokens, 35);
    assert_eq!(
        goal_summary_from_tool_finished_event(&cleared_event),
        Some(None)
    );
    assert_eq!(
        goal_summary_from_tool_finished_event(&unrelated_event),
        None
    );
}

/// Verifies the active-goal continuation prompt keeps Codex's full completion and blocked audits.
/// 验证活跃目标续跑提示词保留 Codex 的完整完成审计与阻塞审计。
#[test]
fn goal_continuation_prompt_keeps_codex_audit_details() {
    let prompt = render_goal_continuation_context(&SessionGoalSummary {
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-1"),
        objective: String::from("完成目标模式"),
        status: String::from("active"),
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

/// Verifies objective-updated steering mirrors Codex's superseding-objective prompt.
/// 验证目标更新引导会对齐 Codex 的新目标覆盖旧目标提示词。
#[test]
fn goal_objective_updated_prompt_supersedes_previous_goal() {
    let prompt = render_goal_objective_updated_context(&SessionGoalSummary {
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-1"),
        objective: String::from("完成修改后的目标"),
        status: String::from("active"),
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
        status: String::from("budget_limited"),
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
        status: String::from("budget_limited"),
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
