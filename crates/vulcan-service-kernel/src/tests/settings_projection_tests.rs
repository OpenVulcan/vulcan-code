use super::*;
use vulcan_service_api::{
    LuaSkillsManagedRuntimeProgressChangedDelta, LuaSkillsManagedRuntimeProgressSummary,
    ProjectSettingsChangedDelta, ProjectSettingsDomain, ServiceEventSink,
};

/// Captures broadcast service events for focused projection assertions.
/// 为聚焦投影断言捕获广播服务事件。
struct CapturingSettingsEventSink {
    /// Ordered events received by this connected client.
    /// 当前已连接客户端收到的有序事件。
    events: Arc<Mutex<Vec<ServiceEvent>>>,
}

impl ServiceEventSink for CapturingSettingsEventSink {
    /// Records one emitted service event.
    /// 记录一条已发送的服务事件。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the exact cloned event delivered to this sink.
    /// `event` 是发送到当前接收端的准确克隆事件。
    ///
    /// # Returns
    /// 返回值
    /// This callback returns no value after appending the event.
    /// 此回调追加事件后不返回值。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events
            .lock()
            .expect("captured settings events should remain lockable")
            .push(event);
    }
}

/// Builds one event emitter and its independently inspectable client queues.
/// 构建一个事件发送器及其可独立检查的客户端队列。
///
/// # Parameters
/// 参数
/// `client_count` is the number of connected sinks registered for the test.
/// `client_count` 是测试中注册的已连接接收端数量。
///
/// # Returns
/// 返回值
/// The shared emitter and one ordered event queue per connected sink.
/// 返回共享发送器以及每个已连接接收端的一条有序事件队列。
fn capturing_settings_emitter(
    client_count: usize,
) -> (ServiceEventEmitter, Vec<Arc<Mutex<Vec<ServiceEvent>>>>) {
    // queues retains every sink's events independently for exact-once assertions.
    // queues 独立保留每个接收端的事件，用于恰好一次断言。
    let queues = (0..client_count)
        .map(|_| Arc::new(Mutex::new(Vec::new())))
        .collect::<Vec<_>>();
    // sinks adapts the queues to the service event interface.
    // sinks 将队列适配为服务事件接口。
    let sinks = queues
        .iter()
        .map(|events| {
            Arc::new(CapturingSettingsEventSink {
                events: Arc::clone(events),
            }) as Arc<dyn ServiceEventSink>
        })
        .collect();
    (ServiceEventEmitter::new(sinks), queues)
}

/// Verifies one project mutation invalidates every connected client exactly once.
/// 验证单次项目变更会让每个已连接客户端恰好失效一次。
#[test]
fn project_settings_invalidation_broadcasts_once_per_client() {
    // parent_session_id seeds the smallest valid shared service state.
    // parent_session_id 用于构建最小有效共享服务状态。
    let parent_session_id = SessionId::new_v4();
    // state owns the monotonic workbench revision used by the invalidation.
    // state 持有失效通知使用的单调工作台版本。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    // emitter and queues model two independently connected WebUI clients.
    // emitter 与 queues 模拟两个独立连接的 WebUI 客户端。
    let (emitter, queues) = capturing_settings_emitter(2);

    commit_and_emit_project_settings_changed_delta(
        &emitter,
        &state,
        Some(String::from("project-a")),
        ProjectSettingsDomain::AgentSkills,
    )
    .expect("project settings invalidation should commit");

    for events in queues {
        // captured locks one client's events for exact payload verification.
        // captured 锁定单个客户端的事件以验证准确载荷。
        let captured = events
            .lock()
            .expect("captured settings events should remain lockable");
        assert_eq!(captured.len(), 1);
        let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
            panic!("project settings invalidation must use a focused delta");
        };
        assert_eq!(envelope.generation, 1);
        assert_eq!(
            envelope.delta,
            WorkbenchDelta::ProjectSettingsChanged(ProjectSettingsChangedDelta {
                project_id: Some(String::from("project-a")),
                domain: ProjectSettingsDomain::AgentSkills,
            })
        );
    }
    // retained verifies a later full snapshot can recover the exact missed invalidation.
    // retained 验证后续完整快照可以恢复准确漏失的失效通知。
    let retained = state
        .lock()
        .expect("project settings state should remain lockable");
    assert_eq!(
        retained
            .workbench
            .project_settings_revisions
            .by_project
            .get("project-a")
            .expect("project revision should be retained")
            .agent_skills,
        1
    );
    assert_eq!(retained.workbench.project_settings_revisions.global.vmm, 0);
}

/// Verifies a global settings mutation carries an all-project invalidation target.
/// 验证全局设置变更携带面向所有项目的失效目标。
#[test]
fn global_project_settings_invalidation_uses_all_project_target() {
    // state owns the revision allocated for the global invalidation.
    // state 持有为全局失效通知分配的版本。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        SessionId::new_v4(),
    )));
    // emitter and queue capture one connected client's focused invalidation.
    // emitter 与 queue 捕获单个已连接客户端的聚焦失效通知。
    let (emitter, mut queues) = capturing_settings_emitter(1);
    let queue = queues.pop().expect("one client queue should exist");

    commit_and_emit_project_settings_changed_delta(
        &emitter,
        &state,
        None,
        ProjectSettingsDomain::Vmm,
    )
    .expect("global project settings invalidation should commit");

    // captured contains the exact null project target serialized for all projects.
    // captured 包含为所有项目序列化的准确空项目目标。
    let captured = queue
        .lock()
        .expect("captured settings events should remain lockable");
    assert_eq!(captured.len(), 1);
    let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
        panic!("global settings invalidation must use a focused delta");
    };
    assert_eq!(
        envelope.delta,
        WorkbenchDelta::ProjectSettingsChanged(ProjectSettingsChangedDelta {
            project_id: None,
            domain: ProjectSettingsDomain::Vmm,
        })
    );
    drop(captured);
    // retained verifies the all-project revision is part of authoritative snapshot state.
    // retained 验证面向所有项目的版本属于权威快照状态。
    let retained = state
        .lock()
        .expect("global settings state should remain lockable");
    assert_eq!(retained.workbench.project_settings_revisions.global.vmm, 1);
}

/// Verifies a full workbench rebuild preserves retained project-settings revisions.
/// 验证完整工作台重建会保留已留存的项目设置版本。
#[test]
fn full_workbench_rebuild_preserves_project_settings_revisions() {
    // retained models the authoritative state after one focused settings mutation.
    // retained 模拟一次聚焦设置变更后的权威状态。
    let mut retained = fallback_workbench_snapshot(&[]);
    retained.project_settings_revisions.global.vmm = 7;
    retained
        .project_settings_revisions
        .by_project
        .entry(String::from("project-a"))
        .or_default()
        .agent_skills = 9;
    // rebuilt models a generic full projection that cannot derive these revisions from workspace files.
    // rebuilt 模拟无法从工作区文件派生这些版本的通用完整投影。
    let mut rebuilt = fallback_workbench_snapshot(&[]);

    preserve_project_settings_revisions(&mut rebuilt, &retained);

    assert_eq!(
        rebuilt.project_settings_revisions,
        retained.project_settings_revisions
    );
}

/// Verifies a focused settings refresh immediately projects the Provider request-trace switch.
/// 验证聚焦设置刷新会立即投影 Provider 请求追踪开关。
#[test]
fn focused_settings_refresh_projects_provider_request_trace_switch() {
    // service_state models the retained workbench that the save RPC returns without a restart.
    // service_state 模拟保存 RPC 无需重启便会返回的保留工作台。
    let mut service_state = test_service_state_with_parent_session(SessionId::new_v4());
    // configuration carries the newly persisted switch value into the focused projection.
    // configuration 将刚持久化的开关值送入聚焦投影。
    let mut configuration = ProviderConfigurationFile {
        provider_request_trace_enabled: true,
        ..ProviderConfigurationFile::default()
    };

    refresh_settings_projection_locked(&mut service_state, &configuration);

    assert!(service_state.workbench.provider_request_trace_enabled);

    configuration.provider_request_trace_enabled = false;
    refresh_settings_projection_locked(&mut service_state, &configuration);

    assert!(!service_state.workbench.provider_request_trace_enabled);
}

/// Verifies identical progress is deduplicated and terminal clearing is always broadcast once.
/// 验证相同进度会被去重，且终态清理始终只广播一次。
#[test]
fn managed_runtime_progress_deduplicates_and_clears_terminal_state() {
    // state owns the retained LuaSkills progress projection.
    // state 持有保留的 LuaSkills 进度投影。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        SessionId::new_v4(),
    )));
    // emitter and queue capture the exact focused progress stream.
    // emitter 与 queue 捕获准确的聚焦进度流。
    let (emitter, mut queues) = capturing_settings_emitter(1);
    let queue = queues.pop().expect("one client queue should exist");
    // progress is the native value emitted by a running managed-runtime installation.
    // progress 是受管运行时安装过程中发送的原生进度值。
    let progress = LuaSkillsManagedRuntimeProgressSummary {
        phase: String::from("download"),
        overall_percent: Some(25),
        ..LuaSkillsManagedRuntimeProgressSummary::default()
    };

    assert!(
        commit_and_emit_luaskills_managed_runtime_progress(
            &emitter,
            &state,
            Some(progress.clone()),
        )
        .expect("initial progress should commit")
    );
    assert!(
        !commit_and_emit_luaskills_managed_runtime_progress(&emitter, &state, Some(progress),)
            .expect("identical progress should be accepted without a new revision")
    );
    assert!(
        commit_and_emit_luaskills_managed_runtime_progress(&emitter, &state, None)
            .expect("terminal progress clear should commit")
    );

    // captured contains only the changed progress and its terminal clear.
    // captured 仅包含变化后的进度及其终态清理。
    let captured = queue
        .lock()
        .expect("captured settings events should remain lockable");
    assert_eq!(captured.len(), 2);
    let ServiceEvent::WorkbenchDelta(terminal_envelope) = &captured[1] else {
        panic!("terminal progress clear must use a focused delta");
    };
    assert_eq!(terminal_envelope.generation, 2);
    assert_eq!(
        terminal_envelope.delta,
        WorkbenchDelta::LuaSkillsManagedRuntimeProgressChanged(
            LuaSkillsManagedRuntimeProgressChangedDelta { progress: None }
        )
    );
    drop(captured);
    // guard confirms the retained snapshot cannot remain stuck after the terminal clear.
    // guard 确认终态清理后保留快照不会继续卡在进行中状态。
    let guard = state
        .lock()
        .expect("test service state should remain lockable");
    assert_eq!(guard.workbench.revision, 2);
    assert!(
        guard
            .workbench
            .lua_skills
            .managed_runtime_progress
            .is_none()
    );
}

/// Verifies an exact background-session projection is broadcast without replacing the selected surface.
/// 验证准确的后台会话投影会被广播，且不会替换当前选中界面。
#[test]
fn context_injection_exact_session_emitter_supports_background_sessions() {
    // selected_session_id remains the workbench-selected conversation throughout the mutation.
    // selected_session_id 在整个变更过程中保持为工作台选中的会话。
    let selected_session_id = SessionId::new_v4();
    // background_session_id is the independently changed target.
    // background_session_id 是独立发生变化的目标。
    let background_session_id = SessionId::new_v4();
    // service_state starts with one selected regular session.
    // service_state 以一个选中的常规会话为初始状态。
    let mut service_state = test_service_state_with_parent_session(selected_session_id);
    service_state.workbench.selected_conversation_id = Some(selected_session_id.to_string());
    // The background emitter reads its exact V1 owner even when the visible surface stays selected elsewhere.
    // 后台发送器即使不占据当前可见界面，也必须读取自身精确的 V1 owner。
    let background_storage = TestControllerRuntimeRoot::new(background_session_id);
    let background_handle = crate::session_runtime_registry_v1::SessionHandle::new(
        background_session_id,
        background_storage.v1_storage_locator(),
        vulcan_session_core::v1::document::SessionDocument::new(
            background_session_id.to_string(),
            "project-a",
            "Background projection test",
            1,
        ),
    );
    service_state
        .session_runtime_registry
        .register(background_handle)
        .expect("background V1 session should register");
    // state shares the retained projections with the exact-session emitter.
    // state 与准确会话发送器共享保留投影。
    let state = Arc::new(Mutex::new(service_state));
    // emitter and queue capture the background projection broadcast.
    // emitter 与 queue 捕获后台投影广播。
    let (emitter, mut queues) = capturing_settings_emitter(1);
    let queue = queues.pop().expect("one client queue should exist");

    emit_context_injection_changed_delta_for_session(&emitter, &state, background_session_id)
        .expect("background context injection projection should emit");

    // captured verifies the event targets the requested background identity.
    // captured 验证事件指向请求中的后台身份。
    let captured = queue
        .lock()
        .expect("captured settings events should remain lockable");
    assert_eq!(captured.len(), 1);
    let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
        panic!("context injection projection must use a focused delta");
    };
    let WorkbenchDelta::ContextInjectionChanged(delta) = &envelope.delta else {
        panic!("exact-session emitter must publish a context injection delta");
    };
    assert_eq!(delta.conversation_id, background_session_id.to_string());
    assert!(delta.context_injection.is_none());
    drop(captured);
    // guard verifies a background event did not overwrite the selected surface projection.
    // guard 验证后台事件没有覆盖当前选中界面投影。
    let guard = state
        .lock()
        .expect("test service state should remain lockable");
    assert_eq!(
        guard.workbench.selected_conversation_id.as_deref(),
        Some(selected_session_id.to_string().as_str())
    );
    assert!(guard.workbench.context_injection.is_none());
}
