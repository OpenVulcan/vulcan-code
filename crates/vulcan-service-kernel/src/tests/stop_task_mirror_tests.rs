//! stop task mirror tests.
//! stop task mirror tests。

use super::*;
use crate::conversation_run_services::terminate_active_conversation_hook_turn_after_stop;

/// Verifies stopping an execution closes the retained Hook turn idempotently before Continue.
/// 验证停止执行会在 Continue 前幂等关闭保留的 Hook 回合。
#[test]
fn stop_closes_retained_conversation_hook_turn_idempotently() {
    // sessionId identifies the exact stopped logical conversation.
    // sessionId 标识精确的已停止逻辑会话。
    let session_id = SessionId::new_v4();
    // agentRuntime owns the real Hook runtime used by the stop service.
    // agentRuntime 持有停止服务使用的真实 Hook 运行时。
    let agent_runtime = test_agent_runtime_bridge();
    // retainedTurnId freezes the owner that the stop operation is allowed to terminate.
    // retainedTurnId 冻结停止操作获准终止的所有者。
    let retained_turn_id = {
        // hookRuntime seeds one active turn without starting Provider execution.
        // hookRuntime 在不启动 Provider 执行的情况下播种一个活动回合。
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime fixture should lock");
        hook_runtime.record_queued_user_prompt(
            session_id,
            "prompt-a",
            Some(String::from("project-a")),
            String::from("D:\\workspace"),
            PendingSessionPromptSource::UserFollowup,
            "continue this task",
        );
        hook_runtime
            .active_turn_id(session_id)
            .expect("queued prompt should open one Hook turn")
            .to_string()
    };

    assert!(
        terminate_active_conversation_hook_turn_after_stop(
            &agent_runtime,
            session_id,
            Some(String::from("project-a")),
            Some(retained_turn_id.as_str()),
            "test stop",
        )
        .expect("first stop should close the Hook turn")
    );
    assert!(
        !terminate_active_conversation_hook_turn_after_stop(
            &agent_runtime,
            session_id,
            Some(String::from("project-a")),
            Some(retained_turn_id.as_str()),
            "test repeated stop",
        )
        .expect("repeated stop should remain idempotent")
    );
    assert!(
        !agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime should remain readable")
            .has_active_turn(session_id)
    );
}

/// Verifies stop cleanup never terminates a replacement Hook turn with a different owner id.
/// 验证停止清理绝不会终止具有不同所有者标识的替代 Hook 回合。
#[test]
fn stop_preserves_replacement_conversation_hook_turn() {
    // sessionId identifies the conversation whose Hook owner changes during stop.
    // sessionId 标识停止期间 Hook 所有者发生变化的会话。
    let session_id = SessionId::new_v4();
    // agentRuntime owns the real Hook owner map used by the stop helper.
    // agentRuntime 持有停止辅助逻辑使用的真实 Hook 所有者映射。
    let agent_runtime = test_agent_runtime_bridge();
    // staleTurnId represents the owner captured by the earlier stop request.
    // staleTurnId 表示较早停止请求捕获的所有者。
    let stale_turn_id = String::from("stale-turn-id");
    // replacementTurnId is the current owner established after the stale capture.
    // replacementTurnId 是过期捕获后建立的当前所有者。
    let replacement_turn_id = {
        // hookRuntime creates the replacement owner under the production mutex.
        // hookRuntime 在生产互斥锁下创建替代所有者。
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime fixture should lock");
        hook_runtime.record_queued_user_prompt(
            session_id,
            "prompt-replacement",
            Some(String::from("project-a")),
            String::from("D:\\workspace"),
            PendingSessionPromptSource::UserFollowup,
            "replacement prompt",
        );
        hook_runtime
            .active_turn_id(session_id)
            .expect("replacement prompt should own one Hook turn")
            .to_string()
    };

    assert!(
        !terminate_active_conversation_hook_turn_after_stop(
            &agent_runtime,
            session_id,
            Some(String::from("project-a")),
            Some(stale_turn_id.as_str()),
            "stale stop",
        )
        .expect("stale stop should be rejected without mutation")
    );
    assert_eq!(
        agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime should remain readable")
            .active_turn_id(session_id),
        Some(replacement_turn_id.as_str())
    );
}

/// Seeds terminal physical Provider calls into the v2 ledger for stop-projection tests.
/// 为停止投影测试向 v2 账本播种终态物理 Provider 调用。
fn seed_terminal_provider_calls(runtime_state: &mut SessionRuntimeStateEntry, count: usize) {
    for index in 0..count {
        let provider_call_id = format!("stop-call-{index}");
        let identity = vulcan_workbench_model::SessionProviderCallIdentity {
            logical_request_id: format!("stop-logical-{index}"),
            execution_id: String::from("stop-execution"),
            turn_id: Some(String::from("stop-turn")),
            attempt_index: u32::try_from(index).expect("test attempt index should fit u32"),
            retry_number: 0,
            owner_kind: String::from("main_agent"),
            owner_id: String::from("main-agent"),
            owner_name_snapshot: String::from("Main Agent"),
            operation: String::from("conversation"),
            provider_id: String::from("provider"),
            provider_name_snapshot: String::from("Provider"),
            model_id: String::from("model"),
            model_name_snapshot: String::from("Model"),
            connection_id: String::from("connection"),
            service_entry_id: String::from("public-api"),
            protocol_profile_id: String::from("openai_responses"),
            manifest_revision: 1,
            manifest_generation: 1,
            binding_source: String::from("system_manifest"),
            binding_rule: Some(String::from("stop-rule")),
            route_revision: String::from("route-revision"),
            request_profiles: Vec::new(),
            response_profiles: Vec::new(),
        };
        assert!(runtime_state.provider_call_ledger.observe_started(
            vulcan_workbench_model::SessionProviderCallStarted {
                provider_call_id: provider_call_id.clone(),
                identity: identity.clone(),
                prepared_at_ms: i64::try_from(index).expect("test timestamp should fit i64"),
                request_assembly_fingerprint: None,
                context_generation_id: None,
                context_start_record_id: None,
                model_route_fingerprint: None,
                toolset_fingerprint: None,
                system_prompt_fingerprint: None,
                request_text_measurement: None,
                budget_accounting: None,
            }
        ));
        assert!(runtime_state.provider_call_ledger.observe_completed(
            vulcan_workbench_model::SessionProviderCallCompleted {
                provider_call_id,
                identity,
                status: vulcan_workbench_model::SessionProviderCallStatus::Cancelled,
                provider_usage_evidence:
                    vulcan_workbench_model::SessionProviderUsageEvidence::default(),
                prepared_at_ms: i64::try_from(index).expect("test timestamp should fit i64"),
                dispatched_at_ms: Some(1),
                first_event_at_ms: None,
                first_token_at_ms: None,
                completed_at_ms: 2,
                dispatch_outcome: Some(String::from("dispatched")),
                dispatch_outcome_known: Some(true),
                semantic_output_observed: Some(false),
                semantic_output_observed_known: Some(true),
                replay_safe: Some(true),
                error_category: Some(String::from("host_cancelled")),
                error_code: Some(String::from("stopped")),
            }
        ));
    }
}

/// Builds one minimal retained Running session for stop-state convergence tests.
/// 为停止状态收敛测试构建一个最小的保留 Running 会话。
fn running_stop_workspace_snapshot(
    session_id: SessionId,
    run_phase: SessionRunPhase,
) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "停止收敛测试",
            "正在运行",
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
        session_runtime_state: BTreeMap::from([(session_id, SessionRuntimeStateEntry::default())]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, run_phase)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    }
}

/// Verifies a live stream task is sufficient to terminalize a session whose kernel turn is missing.
/// 验证实时流任务足以终结内核回合已经缺失的会话。
#[test]
fn webui_stop_session_work_uses_stream_owner_without_kernel_turn() {
    let session_id = SessionId::new_v4();
    let mut workspace_snapshot =
        running_stop_workspace_snapshot(session_id, SessionRunPhase::Requesting);

    let did_stop = stop_webui_session_work(&mut workspace_snapshot, session_id, false, true, false)
        .expect("stream-owned stop should converge every session status surface");

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    assert_eq!(
        workspace_session_summary_for_session(&workspace_snapshot, session_id)
            .map(|session| session.status),
        Some(SessionState::Blocked)
    );
}

/// Verifies Stop repairs a retained Running summary after both live owners have already disappeared.
/// 验证两个实时所有者均已消失后，Stop 仍会修复残留的 Running 会话摘要。
#[test]
fn webui_stop_session_work_repairs_orphaned_running_summary() {
    let session_id = SessionId::new_v4();
    let mut workspace_snapshot =
        running_stop_workspace_snapshot(session_id, SessionRunPhase::Stopped);

    let did_stop =
        stop_webui_session_work(&mut workspace_snapshot, session_id, false, false, false)
            .expect("orphaned Running summary should be repaired idempotently");

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    assert_eq!(
        workspace_session_summary_for_session(&workspace_snapshot, session_id)
            .map(|session| session.status),
        Some(SessionState::Blocked)
    );
}

/// Verifies WebUI stop projects the same stopped turn records as the WebUI transcript chain.
/// 验证 WebUI 停止流程投影与 WebUI 转录链条一致的已停止轮次记录。
#[test]
fn webui_stop_session_work_appends_stopped_turn_projection_once() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    seed_terminal_provider_calls(&mut runtime_state, 2);
    let user_entry =
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "运行一个长任务")
            .with_runtime_metadata("user-1", "1000");
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: user_entry.id.clone(),
            kind: user_entry.kind,
            record_kind: user_entry.record_kind,
            visible: user_entry.visible,
            title: user_entry.title.clone(),
            body: user_entry.body.clone(),
            metadata: user_entry.metadata.clone(),
            created_at: user_entry.created_at.clone(),
        },
    )
    .expect("valid stopped-turn record identity");

    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "停止测试",
                "正在运行",
                SessionState::Running,
                0,
                0,
                "刚刚",
            )
            .with_updated_at("1000"),
        ],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let did_stop = stop_webui_session_work(&mut workspace_snapshot, session_id, true, true, true)
        .expect("active stop should append stopped projection");

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    let messages = workspace_snapshot
        .session_messages
        .get(&session_id)
        .expect("stopped projection should be materialized");
    assert!(messages.iter().any(|message| {
        message.visible && message.title == "执行已停止" && message.body.contains("当前 AI 调用")
    }));
    assert!(messages.iter().any(|message| {
        message.visible && message.title == "执行已停止" && message.body.contains("运行中的智能体")
    }));
    let runtime_records = &workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should exist")
        .conversation_state
        .records;
    assert!(runtime_records.iter().any(|record| {
        !record.visible && record.record_kind == SessionMessageRecordKind::TurnTerminated
    }));
    let turn_stats = messages
        .iter()
        .find(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("stopped turn stats should be projected");
    assert!(turn_stats.visible);
    assert_eq!(turn_stats.title, "Turn Stats · stopped");
    assert!(turn_stats.body.contains("状态：已中止"));
    assert!(turn_stats.body.contains("Provider 调用：2"));
    let continuation = &workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("stopped runtime state should exist")
        .provider_call_ledger
        .continuation;
    assert!(continuation.eligible);
    assert_eq!(continuation.terminal_reason.as_deref(), Some("stopped"));
    assert_eq!(continuation.canonical_tail_kind.as_deref(), Some("error"));
    let stopped_message_count = messages.len();

    let did_stop_again =
        stop_webui_session_work(&mut workspace_snapshot, session_id, false, false, false)
            .expect("repeated stop should remain idempotent");

    assert!(!did_stop_again);
    let messages_after_second_stop = workspace_snapshot
        .session_messages
        .get(&session_id)
        .expect("stopped projection should still exist");
    assert_eq!(messages_after_second_stop.len(), stopped_message_count);
}

/// Verifies Continue is never materialized while a physical Provider call remains open.
/// 验证物理 Provider 调用仍未闭合时绝不会物化 Continue。
#[test]
fn webui_stop_session_work_rejects_open_provider_call() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    seed_terminal_provider_calls(&mut runtime_state, 1);
    let mut identity = runtime_state.provider_call_ledger.calls["stop-call-0"]
        .identity
        .clone();
    identity.logical_request_id = String::from("open-logical");
    identity.execution_id = String::from("open-execution");
    identity.attempt_index = 1;
    assert!(runtime_state.provider_call_ledger.observe_started(
        vulcan_workbench_model::SessionProviderCallStarted {
            provider_call_id: String::from("open-call"),
            identity,
            prepared_at_ms: 3,
            request_assembly_fingerprint: None,
            context_generation_id: None,
            context_start_record_id: None,
            model_route_fingerprint: None,
            toolset_fingerprint: None,
            system_prompt_fingerprint: None,
            request_text_measurement: None,
            budget_accounting: None,
        }
    ));
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "停止测试",
            "正在运行",
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
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let error = stop_webui_session_work(&mut workspace_snapshot, session_id, true, false, false)
        .expect_err("open Provider calls must block stopped Continue projection");

    assert_eq!(
        error,
        "ERR_CONVERSATION_CONTINUE_PROVIDER_CALL_STILL_ACTIVE"
    );
    assert!(
        workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should remain available")
            .provider_call_ledger
            .continuation
            .is_empty()
    );
}

/// Verifies a kernel-active turn still receives a stopped projection when its stream handle is gone.
/// 验证内核活动回合即使流句柄已丢失也仍会收到停止投影。
#[test]
fn webui_stop_session_work_uses_kernel_active_state_without_stream_handle() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    seed_terminal_provider_calls(&mut runtime_state, 1);
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "停止测试",
            "正在运行",
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
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let did_stop = stop_webui_session_work(&mut workspace_snapshot, session_id, true, false, false)
        .expect("kernel-active stop should append stopped projection");

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    let messages = workspace_snapshot
        .session_messages
        .get(&session_id)
        .expect("kernel-active stop should be projected");
    assert!(messages.iter().any(|message| {
        message.visible && message.title == "执行已停止" && message.body.contains("当前 AI 调用")
    }));
    assert!(messages.iter().all(|message| {
        !(message.visible
            && message.title == "执行已停止"
            && message.body.contains("运行中的智能体"))
    }));
}

/// Verifies persisted completed task mirrors repair stale spawn-agent display metadata.
/// 验证持久化的已完成任务镜像会修复陈旧的 spawn_agent 展示元数据。
#[test]
fn task_mirror_completed_status_repairs_stale_running_tool_card() {
    let task_id = SessionId::new_v4();
    let mut messages = vec![
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput),
        test_spawn_agent_tool_message(
            task_id,
            "running",
            SessionMessageRecordKind::ToolResultOutput,
        ),
    ];
    let mirrors = vec![SessionTaskMirrorSnapshot {
        task_session_id: task_id,
        status: String::from("completed"),
        line_count: 3,
        updated_at: String::from("1780000000000"),
    }];

    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        None,
        mirrors.as_slice(),
        |_| false,
    );

    for message in messages {
        let metadata = message.metadata.as_ref().expect("metadata");
        assert_eq!(
            task_session_execution_status_from_tool_metadata(metadata),
            Some("completed")
        );
        assert_eq!(
            metadata
                .get("tool_display_state")
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        assert_eq!(message.title, "子代理 image-worker");
    }
}

/// Verifies task mirror status repair keeps projected messages and runtime records synchronized.
/// 验证任务镜像状态修复会同步投影消息与运行时记录。
#[test]
fn task_mirror_completed_status_repairs_projected_card_and_runtime_record() {
    // Child task identity shared by the projected card and canonical runtime record.
    // 投影卡片与规范运行时记录共享的子任务身份。
    let task_id = SessionId::new_v4();
    // Projected parent task card shown in the WebUI transcript.
    // WebUI 转录中展示的父级任务卡片。
    let mut message =
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput);
    message.id = String::from("shared-task-card");
    // Runtime state holding the canonical parent conversation record for the same card.
    // 持有同一任务卡片规范父级会话记录的运行时状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: message.id.clone(),
            kind: message.kind,
            record_kind: message.record_kind,
            visible: message.visible,
            title: message.title.clone(),
            body: message.body.clone(),
            metadata: message.metadata.clone(),
            created_at: message.created_at.clone(),
        },
    )
    .expect("valid task-card record identity");
    let mut messages = vec![message];
    let mirrors = vec![SessionTaskMirrorSnapshot {
        task_session_id: task_id,
        status: String::from("completed"),
        line_count: 3,
        updated_at: String::from("1780000000000"),
    }];

    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        Some(&mut runtime_state),
        mirrors.as_slice(),
        |_| false,
    );

    let projected_metadata = messages[0].metadata.as_ref().expect("projected metadata");
    let runtime_metadata = runtime_state.conversation_state.records[0]
        .metadata
        .as_ref()
        .expect("runtime metadata");
    assert_eq!(
        task_session_execution_status_from_tool_metadata(projected_metadata),
        Some("completed")
    );
    assert_eq!(
        task_session_execution_status_from_tool_metadata(runtime_metadata),
        Some("completed")
    );
}

/// Verifies active task mirrors without live runtime are shown as stopped instead of completed.
/// 验证没有实时运行态的活动任务镜像会显示为已停止，而不是伪装成已完成。
#[test]
fn task_mirror_active_status_without_live_runtime_repairs_to_stopped() {
    let task_id = SessionId::new_v4();
    let mut messages = vec![test_spawn_agent_tool_message(
        task_id,
        "running",
        SessionMessageRecordKind::ToolCallOutput,
    )];
    let mirrors = vec![SessionTaskMirrorSnapshot {
        task_session_id: task_id,
        status: String::from("running"),
        line_count: 3,
        updated_at: String::from("1780000000000"),
    }];

    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        None,
        mirrors.as_slice(),
        |_| false,
    );

    let metadata = messages[0].metadata.as_ref().expect("metadata");
    assert_eq!(
        task_session_execution_status_from_tool_metadata(metadata),
        Some("stopped")
    );
    assert_eq!(
        metadata
            .get("tool_display_state")
            .and_then(serde_json::Value::as_str),
        Some("stopped")
    );
}

/// Verifies live runtime state overrides stale terminal task mirrors.
/// 验证实时运行态会覆盖陈旧的终态任务镜像。
#[test]
fn task_mirror_display_keeps_running_when_task_is_live() {
    let task_id = SessionId::new_v4();
    let mut messages = vec![test_spawn_agent_tool_message(
        task_id,
        "completed",
        SessionMessageRecordKind::ToolCallOutput,
    )];
    let mirrors = vec![SessionTaskMirrorSnapshot {
        task_session_id: task_id,
        status: String::from("completed"),
        line_count: 3,
        updated_at: String::from("1780000000000"),
    }];

    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        None,
        mirrors.as_slice(),
        |_| true,
    );

    let metadata = messages[0].metadata.as_ref().expect("metadata");
    assert_eq!(
        task_session_execution_status_from_tool_metadata(metadata),
        Some("running")
    );
    assert_eq!(
        metadata
            .get("tool_display_state")
            .and_then(serde_json::Value::as_str),
        Some("running")
    );
}

/// Verifies WebUI task-card lookup keeps newest projected messages before runtime records.
/// 验证 WebUI task 卡片查找保持投影消息最新优先且先于运行时记录。
#[test]
fn webui_task_session_lookup_keeps_newest_projected_message_priority() {
    // Parent session that owns the visible spawn-agent task card.
    // 拥有可见 spawn-agent task 卡片的父会话。
    let parent_session_id = SessionId::new_v4();
    // Child task identity shared by every matching parent card candidate.
    // 每个父级卡片候选项共享的子任务身份。
    let task_id = SessionId::new_v4();
    // Lookup payload emitted by delegate task-session metadata.
    // 由委派 task-session 元数据发出的查找载荷。
    let payload = json!({
        "task_id": "image-worker",
        (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
    });
    // Older projected card that must lose to the newer projected card.
    // 必须让位于较新投影卡片的较旧投影卡片。
    let mut older_message =
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput);
    older_message.id = String::from("older-message");
    // Newer projected card that should win the best-match lookup.
    // 应在最佳匹配查找中胜出的较新投影卡片。
    let mut newer_message =
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput);
    newer_message.id = String::from("newer-message");
    // Runtime-only card that the all-match lookup must still include.
    // 全量匹配查找仍然必须包含的仅运行时卡片。
    let mut runtime_message = test_spawn_agent_tool_message(
        task_id,
        "running",
        SessionMessageRecordKind::ToolResultOutput,
    );
    runtime_message.id = String::from("runtime-record");
    // Runtime state holding the canonical parent conversation record.
    // 持有父级规范会话记录的运行时状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: runtime_message.id.clone(),
            kind: runtime_message.kind,
            record_kind: runtime_message.record_kind,
            visible: runtime_message.visible,
            title: runtime_message.title.clone(),
            body: runtime_message.body.clone(),
            metadata: runtime_message.metadata.clone(),
            created_at: runtime_message.created_at.clone(),
        },
    )
    .expect("valid runtime task record identity");
    // Workspace snapshot with both projected messages and canonical runtime records.
    // 同时包含投影消息与规范运行时记录的工作区快照。
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(parent_session_id, vec![older_message, newer_message])]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(parent_session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert_eq!(
        find_webui_task_session_tool_message_id_for_payload(
            &workspace_snapshot,
            parent_session_id,
            &payload
        )
        .as_deref(),
        Some("newer-message")
    );
    assert_eq!(
        find_webui_task_session_tool_message_ids_for_payload(
            &workspace_snapshot,
            parent_session_id,
            &payload
        ),
        BTreeSet::from([
            String::from("newer-message"),
            String::from("older-message"),
            String::from("runtime-record"),
        ])
    );
}

/// Verifies ordinary tool rows do not trigger persisted task mirror loading.
/// 验证普通工具行不会触发持久化任务镜像加载。
#[test]
fn task_mirror_gate_ignores_non_task_tool_rows() {
    let mut message = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "调用 shell_command 工具",
        "ok",
    )
    .with_runtime_metadata("tool-shell", "1780000000000");
    message.record_kind = SessionMessageRecordKind::ToolCallOutput;
    message.metadata = Some(json!({
        "tool_name": "shell_command",
        "display_state": "completed"
    }));
    let messages = [message];

    assert!(!surface_contains_task_session_tool_card(
        messages.iter(),
        None
    ));
}

/// Verifies task-session tool rows pass the persisted mirror repair gate.
/// 验证 task-session 工具行会通过持久化镜像修复门禁。
#[test]
fn task_mirror_gate_accepts_task_session_tool_rows() {
    let messages = [test_spawn_agent_tool_message(
        SessionId::new_v4(),
        "running",
        SessionMessageRecordKind::ToolCallOutput,
    )];

    assert!(surface_contains_task_session_tool_card(
        messages.iter(),
        None
    ));
}
