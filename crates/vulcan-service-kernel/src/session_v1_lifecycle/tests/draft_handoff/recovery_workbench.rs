//! Covers recovery publication after a newer project removal has committed.
//! 覆盖较新的项目移除提交后的恢复发布。

use super::*;
use vulcan_service_api::{ServiceEvent, ServiceEventEmitter, WorkbenchDelta};
use vulcan_workspace_store::workspace_store::load_workspace_index_snapshot;

/// Captures the old index, completes the real project removal, and then resumes recovery publication.
/// 捕获旧索引，完成真实的项目移除，再继续恢复发布。
#[tokio::test]
async fn reasoning_recovery_stale_index_cannot_replace_completed_project_removal() {
    // Two project rows let the removal target differ from the owner of the recovered draft.
    // 两条项目记录让移除目标和恢复草稿的所属项目不同。
    let fixture = LifecycleStorageFixture::start().await;
    prepare_recovery_project(&fixture).await;
    // Give the removable project its own identity and directory.
    // 为待移除项目设置独立的标识及目录。
    let removed_id = "removed-during-recovery";
    // Use a valid absolute project path so the real index admits both projects.
    // 使用有效的绝对项目路径，使真实索引接纳两个项目。
    let removed_path = fixture._temporary.path().join("removed-project");
    std::fs::create_dir(&removed_path).unwrap();
    execute_script_async(
        &fixture.controller.local,
        LogicalDatabase::AppState,
        "INSERT INTO projects (id, name, root_path, created_at, updated_at)
         VALUES (?1, 'Removed project', ?2, '1', '1');",
        vec![
            SqliteValue::Text(removed_id.into()),
            SqliteValue::Text(removed_path.to_string_lossy().into_owned()),
        ],
    )
    .await
    .unwrap();
    // Attach the isolated controller to the same workbench used by both commands.
    // 将隔离控制器挂到两个命令共用的工作台。
    let state = create_service_bridge_state(test_service_shell_host());
    // Recovery writes and workbench projections must use the same registry.
    // 恢复写入和工作台投影必须使用同一个注册表。
    let registry = {
        // Install the fixture controller before taking the initial workbench snapshot.
        // 在读取初始工作台快照前安装夹具控制器。
        let mut guard = state.lock().unwrap();
        guard.controller_runtime = Some(fixture.controller.clone());
        guard.session_runtime_registry.clone()
    };
    crate::workbench_refresh::refresh_workbench_snapshot(state.clone())
        .await
        .unwrap();

    // Write the recovered draft through its actual durable lifecycle and project admission.
    // 通过实际持久化生命周期和项目准入写入恢复草稿。
    let recovered_id = create_or_get_recovery_draft_session_v1(
        &state,
        &fixture.controller,
        &registry,
        SessionId::new_v4(),
        std::future::ready(Ok(RecoveryDraftSessionV1 {
            project_id: RECOVERY_PROJECT_ID.into(),
            title: "Recovered conversation".into(),
            text: RECOVERY_TRANSCRIPT.into(),
            selection: vulcan_provider_adapters::ActiveAiModelSelection {
                provider_id: "deepseek".into(),
                model_id: "deepseek-flash".into(),
                connection_id: "deepseek-test".into(),
            },
            now_ms: RECOVERY_CREATED_AT_MS,
        })),
    )
    .await
    .unwrap();
    // Pause exactly after the production index-read phase, with the removable project still visible.
    // 在生产链路的索引读取阶段后暂停，此时待移除项目仍然可见。
    let stale = load_workspace_index_snapshot(fixture.controller.clone())
        .await
        .unwrap();
    assert!(stale.task_projects.iter().any(|row| row.id == removed_id));
    assert!(stale.sessions.iter().any(|row| row.id == recovered_id));

    // Observe the same event channel for the completed removal and the resumed recovery.
    // 在同一个事件通道观测已完成的移除和继续执行的恢复。
    let (events, mut updates) = tokio::sync::broadcast::channel(8);
    // Use the production bridge sink, so the test also checks the public event form.
    // 使用生产桥接的事件出口，同时检查公开事件形式。
    let app = ServiceEventEmitter::new(vec![vulcan_service_host::webui_service_event_bus_sink(
        events,
    )]);
    // Complete the real removal, including its retained refresh and delta.
    // 完成真实移除，包括保留状态刷新与增量。
    let removed = crate::project_conversation_services::service_remove_project(
        app.clone(),
        state.clone(),
        removed_id.into(),
    )
    .await
    .unwrap();
    assert!(removed.projects.iter().all(|row| row.id != removed_id));
    // Verify the removal event owns the earlier commit before resuming recovery.
    // 继续恢复前验证移除事件对应较早的提交。
    let ServiceEvent::WorkbenchDelta(removal) = updates.try_recv().unwrap() else {
        panic!("expected a project removal delta");
    };
    assert!(matches!(removal.delta, WorkbenchDelta::ProjectRemoved(_)));

    crate::workbench_refresh::publish_indexed_recovered_session(
        &app,
        &state,
        &fixture.controller,
        &registry,
        recovered_id,
        stale,
    )
    .await
    .unwrap();
    // The later generation must contain only the recovered row and keep the committed removal.
    // 较新的版本只能包含恢复会话行，并保留已提交的移除结果。
    let ServiceEvent::WorkbenchDelta(recovery) = updates.try_recv().unwrap() else {
        panic!("expected a session-scoped recovery delta");
    };
    assert!(recovery.generation > removal.generation);
    // The delta identifies the recovered conversation without changing selection.
    // 增量标识恢复会话且不改变选择。
    let WorkbenchDelta::ConversationSummaryChanged(change) = recovery.delta else {
        panic!("expected a recovered conversation summary");
    };
    assert_eq!(change.conversation.id, recovered_id.to_string());
    {
        // Validate both retained projections that a later request or reconnect can read.
        // 验证后续请求或重连可读取的两份保留投影。
        let retained = state.lock().unwrap();
        // The internal workspace uses the same project membership as the visible workbench.
        // 内部工作区与可见工作台使用相同的项目成员关系。
        let workspace = retained.workspace_snapshot.as_ref().unwrap();
        assert!(
            workspace
                .task_projects
                .iter()
                .all(|row| row.id != removed_id)
        );
        assert!(
            retained
                .workbench
                .projects
                .iter()
                .all(|row| row.id != removed_id)
        );
        assert!(
            retained
                .workbench
                .conversations
                .contains(&change.conversation)
        );
    }
    fixture.shutdown().await;
}
