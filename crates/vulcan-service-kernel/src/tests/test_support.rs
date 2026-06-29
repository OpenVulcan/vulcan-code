//! Shared service-kernel test support.
//! service-kernel 共享测试支撑。

use super::*;

/// Test shell host that rejects interactive shell operations.
/// 拒绝交互式 shell 操作的测试宿主壳。
struct TestServiceShellHost;

impl ServiceShellHost for TestServiceShellHost {
    /// Rejects host file-manager use in unit tests.
    /// 在单元测试中拒绝使用宿主文件管理器。
    fn open_path(&self, _path: &str) -> Result<(), String> {
        Err(String::from(
            "test shell host does not support opening paths",
        ))
    }

    /// Rejects host browser use in unit tests.
    /// 在单元测试中拒绝使用宿主浏览器。
    fn open_url(&self, _url: &str) -> Result<(), String> {
        Err(String::from(
            "test shell host does not support opening URLs",
        ))
    }
}

/// Builds a shared test shell host.
/// 构建共享测试宿主壳。
pub(super) fn test_service_shell_host() -> Arc<dyn ServiceShellHost> {
    Arc::new(TestServiceShellHost)
}

/// Builds a minimal service state containing one regular parent session.
/// 构建只包含一个常规父会话的最小服务状态。
pub(super) fn test_service_state_with_parent_session(
    parent_session_id: SessionId,
) -> ServiceBridgeState {
    ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: Some(WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                parent_session_id,
                Some(String::from("project-a")),
                "父会话",
                "",
                SessionState::Completed,
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
            session_run_phases: BTreeMap::from([(parent_session_id, SessionRunPhase::Ready)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: parent_session_id,
            hydrated_session_ids: BTreeSet::new(),
        }),
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        session_subagent_tasks: BTreeMap::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    }
}

/// Builds one delegate-finished runtime event carrying task-session metadata.
/// 构建一条携带 task-session 元数据的委派完成运行时事件。
pub(super) fn test_delegate_finished_event(
    task_session_id: SessionId,
    public_task_id: &str,
    result: &str,
) -> RuntimeEvent {
    RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "子代理完成",
        "子代理任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": public_task_id,
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_status": "completed",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": result,
        }
    })))
}

/// Builds one parent spawn-agent tool output message with task-session metadata.
/// 构建一条带 task-session 元数据的父级 spawn_agent 工具输出消息。
pub(super) fn test_spawn_agent_tool_message(
    task_id: SessionId,
    execution_status: &str,
    record_kind: SessionMessageRecordKind,
) -> SessionMessageEntry {
    let mut message = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "子代理 image-worker",
        "spawn_agent returned",
    )
    .with_runtime_metadata(format!("tool-{task_id}"), String::from("1780000000000"));
    message.record_kind = record_kind;
    message.metadata = Some(json!({
        "tool_name": "spawn_agent",
        "display_state": execution_status,
        "tool_display_state": execution_status,
        "host_result": {
            "kind": SUBAGENT_TASK_KIND,
            "payload": {
                "task_id": "image-worker",
                (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
                "execution_status": execution_status
            }
        }
    }));
    message
}
