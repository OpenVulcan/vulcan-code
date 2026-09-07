//! Real native binding actions and resident WebUI projection acceptance.
//! 真实原生绑定动作及驻留 WebUI 投影验收。

use super::*;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_service_api::{ServiceEvent, ServiceEventSink, VmmMemoryActionRequest, WorkbenchDelta};
use vulcan_vmm::{
    GetProfileNodesRequest, ProfileTarget, VmmClient, VmmMemoryReadSource, VmmMemorySource,
};
use vulcan_workbench_model::{SessionState, SessionSummary, VmmProfileSyncState};

/// Captures the actual service events emitted to connected clients.
/// 捕获实际发送给连接客户端的服务事件。
#[derive(Default)]
struct BindingEvents {
    /// Ordered events from the production emitter, protected for its synchronous callback.
    /// 生产发送器的有序事件，受同步回调互斥保护。
    events: Mutex<Vec<ServiceEvent>>,
}

impl ServiceEventSink for BindingEvents {
    /// Records one real service event; takes ownership and returns after capture.
    /// 记录一个真实服务事件；接收其所有权，捕获后返回。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events.lock().expect("event capture").push(event);
    }
}

/// Dispatches one exact native management operation through the product service boundary.
/// 通过产品服务边界分派一个准确原生管理操作。
/// Takes isolated state, emitter, action and payload; returns only after the real action succeeds.
/// 接收隔离状态、发送器、动作及参数；真实动作成功后才返回。
async fn action(
    state: &crate::state::SharedServiceBridgeState,
    app: &ServiceEventEmitter,
    name: &str,
    payload: serde_json::Value,
) {
    crate::service_run_vmm_memory_action(
        app.clone(),
        state.clone(),
        VmmMemoryActionRequest {
            action: name.to_owned(),
            command_id: SessionId::new_v4().to_string(),
            scope: "global".to_owned(),
            project_id: None,
            conversation_id: None,
            payload,
        },
    )
    .await
    .expect("real binding action");
}

/// Validates all five binding actions without deleting memory, hydrating cold chats or disturbing peer projects.
/// 验证全部五个绑定动作，不删除记忆、不水合冷聊天、不扰动其他项目。
/// Takes the existing real nonempty fixture; compares durable caches, actual events and complete stored content.
/// 接收既有真实非空夹具；比较持久缓存、实际事件及完整存储内容。
pub(super) async fn verify(
    state: &crate::state::SharedServiceBridgeState,
    client: &crate::native_vmm::client::NativeVmmClient,
    data_plane: &VmmDataPlaneHandle,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    session_id: SessionId,
    project_id: u64,
    source: &VmmMemoryReadSource,
) {
    // Stored profiles come from preceding normal extraction, not fabricated UI nodes.
    // 存储画像来自前序正常提取，而非虚构界面节点。
    let mut nodes = BTreeMap::new();
    for target in [
        ProfileTarget::ProfileTargetProject,
        ProfileTarget::ProfileTargetUser,
    ] {
        let result = client
            .get_profile_nodes(GetProfileNodesRequest {
                expected_read_source: Some(source.clone()),
                host_project_id: "project-a".to_owned(),
                target,
                user_id: 1,
                project_id,
                limit: 256,
            })
            .await
            .expect("real profiles");
        assert!(!result.nodes.is_empty());
        nodes.extend(
            result
                .nodes
                .into_iter()
                .map(|node| (node.profile_node_id, node)),
        );
    }
    // Cache metadata is a process-local mirror; content preservation is checked against actual storage below.
    // 缓存元数据为进程内镜像；正文保留通过下方实际存储比较检查。
    let profile = VmmProfileSyncState {
        read_source: Some(source.clone()),
        nodes_by_id: nodes,
        base_profile_content: Some("cached profile baseline".to_owned()),
        ..Default::default()
    };
    let stored_before = data_plane
        .load_core_snapshot_json()
        .await
        .expect("stored baseline");
    let controller = crate::vmm_session_controller::open_registered_session_vmm_controller(
        state, session_id, "10",
    )
    .expect("resident owner");
    let cache_before = controller.snapshot().expect("nonempty resident");
    assert!(!cache_before.active_leases.is_empty());
    // These IDs model retained archived and peer sessions, plus a cold metadata-only session.
    // 这些 ID 分别表示保留的归档会话、其他项目会话及仅有元数据的冷会话。
    let archived_id = SessionId::new_v4();
    let peer_id = SessionId::new_v4();
    let cold_id = SessionId::new_v4();
    let retained = SessionRuntimeStateEntry {
        vmm_profile_sync: profile.clone(),
        ..Default::default()
    };
    {
        let mut guard = state.lock().expect("fixture state");
        let workspace = guard.workspace_snapshot.as_mut().expect("workspace");
        workspace.archived_sessions.push(SessionSummary::new(
            archived_id,
            Some("project-a".to_owned()),
            "archived",
            "",
            SessionState::Completed,
            0,
            0,
            "1",
        ));
        workspace.sessions.push(SessionSummary::new(
            peer_id,
            Some("peer-project".to_owned()),
            "peer",
            "",
            SessionState::Completed,
            0,
            0,
            "1",
        ));
        workspace.sessions.push(SessionSummary::new(
            cold_id,
            Some("project-a".to_owned()),
            "cold",
            "",
            SessionState::Completed,
            0,
            0,
            "1",
        ));
        workspace
            .archived_session_runtime_state
            .insert(archived_id, retained.clone());
        workspace
            .session_runtime_state
            .insert(peer_id, retained.clone());
        workspace
            .session_runtime_state
            .get_mut(&session_id)
            .expect("active row")
            .vmm_profile_sync = profile.clone();
        // A retained UI card may outlive its runtime row; the action must explicitly clear it without reopening storage.
        // 保留界面卡片可能长于运行态行存活；动作须明确清除卡片，而非重开存储。
        guard.workbench.context_injection_by_conversation.push(
            crate::workbench_session_panels::context_injection_changed_delta_from_runtime_state(
                cold_id,
                Some(&retained),
                None,
            ),
        );
        guard.workbench.selected_conversation_id = Some(session_id.to_string());
    }
    // The production emitter is observed directly, not replaced with a response stub.
    // 直接观察生产发送器，不替换为响应桩。
    let capture = Arc::new(BindingEvents::default());
    let app = ServiceEventEmitter::new(vec![capture.clone()]);
    action(
        state,
        &app,
        "native_project_create_and_bind",
        serde_json::json!({"vulcanProjectId":"project-a", "projectName":"binding-destination"}),
    )
    .await;
    let bound = data_plane
        .resolve_project_access("project-a")
        .await
        .expect("current binding");
    assert_ne!(bound.owner.id, project_id);
    let new_id = bound.owner.id;
    let new_source = VmmMemoryReadSource::new(
        VmmMemorySource::native(new_id).expect("new owner"),
        vec![new_id],
    )
    .expect("new source");
    let after = controller.snapshot().expect("rebound cache");
    assert_eq!(after.read_source.as_ref(), Some(&new_source));
    assert!(after.active_leases.is_empty());
    assert!(after.has_committed_controller_command("before-compaction"));
    {
        let guard = state.lock().expect("projected state");
        let workspace = guard.workspace_snapshot.as_ref().expect("workspace");
        for row in [
            workspace
                .session_runtime_state
                .get(&session_id)
                .expect("active"),
            workspace
                .archived_session_runtime_state
                .get(&archived_id)
                .expect("archived"),
        ] {
            assert_eq!(row.vmm_profile_sync.read_source.as_ref(), Some(&new_source));
            assert!(row.vmm_profile_sync.nodes_by_id.is_empty());
            assert!(row.vmm_profile_sync.base_profile_content.is_none());
        }
        assert_eq!(
            workspace.session_runtime_state[&peer_id].vmm_profile_sync,
            profile
        );
        assert!(!workspace.session_runtime_state.contains_key(&cold_id));
        assert!(
            !guard
                .vmm_controller_registry
                .contains(cold_id)
                .expect("cold not opened")
        );
        assert!(
            !guard
                .vmm_controller_registry
                .contains(archived_id)
                .expect("archived not reopened")
        );
        assert!(
            !guard
                .workbench
                .context_injection_by_conversation
                .iter()
                .any(|entry| entry.conversation_id == cold_id.to_string())
        );
    }
    let emitted_ids = capture
        .events
        .lock()
        .expect("events")
        .iter()
        .filter_map(|event| match event {
            ServiceEvent::WorkbenchDelta(envelope) => match &envelope.delta {
                WorkbenchDelta::ContextInjectionChanged(delta) => {
                    Some(delta.conversation_id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        emitted_ids,
        BTreeSet::from([
            session_id.to_string(),
            archived_id.to_string(),
            cold_id.to_string()
        ])
    );
    // Saving the same owner must preserve the complete durable cache, including revision and receipts.
    // 保存相同归属必须保留完整持久缓存，包括版本及回执。
    action(
        state,
        &app,
        "native_project_bind_existing",
        serde_json::json!({"vulcanProjectId":"project-a", "vmmProjectId":new_id}),
    )
    .await;
    assert_eq!(controller.snapshot().expect("same source"), after);
    action(
        state,
        &app,
        "native_project_replace_query_links",
        serde_json::json!({"vulcanProjectId":"project-a", "linkedProjectIds":[project_id]}),
    )
    .await;
    let linked = VmmMemoryReadSource::new(
        VmmMemorySource::native(new_id).expect("owner"),
        vec![new_id, project_id],
    )
    .expect("links");
    assert_eq!(
        controller
            .snapshot()
            .expect("linked source")
            .read_source
            .as_ref(),
        Some(&linked)
    );
    action(state, &app, "native_project_set_access", serde_json::json!({"vulcanProjectId":"project-a", "ownerProjectId":project_id, "linkedProjectIds":[new_id]})).await;
    let replaced = VmmMemoryReadSource::new(
        VmmMemorySource::native(project_id).expect("owner"),
        vec![project_id, new_id],
    )
    .expect("replaced");
    assert_eq!(
        controller
            .snapshot()
            .expect("replaced source")
            .read_source
            .as_ref(),
        Some(&replaced)
    );
    action(
        state,
        &app,
        "native_project_unbind",
        serde_json::json!({"vulcanProjectId":"project-a"}),
    )
    .await;
    assert!(
        controller
            .snapshot()
            .expect("unbound source")
            .read_source
            .is_none()
    );
    assert_eq!(
        data_plane
            .load_core_snapshot_json()
            .await
            .expect("content preserved"),
        stored_before
    );
    // Restore the fixture's binding only; accepted memory and all profiles remain untouched throughout.
    // 仅恢复夹具绑定；整个过程中已接纳记忆与全部画像保持不变。
    maintenance
        .bind_existing_project("project-a", project_id)
        .await
        .expect("restore fixture binding");
    // Exercise the actual global-save callback with real Core data present; cached projections are not stored memory.
    // 在存在真实 Core 数据时执行实际全局保存回调；缓存投影不等同于存储记忆。
    let mut configuration = state
        .lock()
        .expect("configuration owner")
        .agent_runtime
        .as_ref()
        .expect("agent")
        .provider_configuration
        .clone();
    for (enabled, backend) in [
        (false, vulcan_vmm::VmmBackendMode::Native),
        (true, vulcan_vmm::VmmBackendMode::ExternalGrpc),
        (true, vulcan_vmm::VmmBackendMode::Native),
    ] {
        configuration.vmm.enabled = Some(enabled);
        configuration.vmm.backend_mode = Some(backend);
        crate::vmm_hooks::refresh_guard_vmm_hook_snapshot(
            &mut state.lock().expect("mode publication"),
            &configuration,
        )
        .expect("global mode callback");
        assert_eq!(
            data_plane
                .load_core_snapshot_json()
                .await
                .expect("backend data retained"),
            stored_before
        );
        assert!(
            controller
                .snapshot()
                .expect("no old cache resurrection")
                .active_leases
                .is_empty()
        );
    }
}
