//! VMM tests.
//! VMM 测试。

use super::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_service_runtime::vmm_management::VmmWorkbenchAction;
use vulcan_vmm::{ProfileNodeEntry, ProfileNodeSourceKind, ProfileTarget};
use vulcan_workbench_model::{SessionId, SessionState, SessionSummary};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

/// Builds a default VMM action request for parser unit tests.
/// 为解析器单元测试构建默认 VMM 动作请求。
fn test_vmm_action_request(action: &str, payload: Value) -> VmmMemoryActionRequest {
    VmmMemoryActionRequest {
        action: action.to_string(),
        scope: String::from("global"),
        project_id: None,
        conversation_id: None,
        payload,
    }
}

/// Verifies write-memory action parsing keeps the WebUI payload mapping stable.
/// 验证写入记忆动作解析保持 WebUI 载荷映射稳定。
#[test]
fn parse_vmm_memory_action_maps_write_memories_payload() {
    let request = test_vmm_action_request(
        "write_memories",
        json!({
            "sessionId": "session-a",
            "scopeLevel": 3,
            "abstractText": "short",
            "details": "long",
            "category": 4,
            "priority": 5,
            "memoryLevel": 6
        }),
    );

    let action = parse_vmm_memory_action(&request).expect("write action should parse");

    match action {
        VmmWorkbenchAction::WriteMemories { session_id, item } => {
            assert_eq!(session_id, "session-a");
            assert_eq!(item.scope_level, 3);
            assert_eq!(item.abstract_text, "short");
            assert_eq!(item.details, "long");
            assert_eq!(item.category, 4);
            assert_eq!(item.priority, 5);
            assert_eq!(item.memory_level, 6);
        }
        other => panic!("unexpected VMM workbench action: {other:?}"),
    }
}

/// Verifies VMM action parsing covers profile, search, delete and unknown-action branches.
/// 验证 VMM 动作解析覆盖画像、搜索、删除和未知动作分支。
#[test]
fn parse_vmm_memory_action_covers_common_management_branches() {
    let profile_request = test_vmm_action_request("load_profile_nodes", json!({}));
    let search_request =
        test_vmm_action_request("search_memory_events", json!({ "queries": "alpha,beta" }));
    let delete_request = test_vmm_action_request(
        "delete_memories",
        json!({ "memoryIds": [7, "11"], "reason": "stale" }),
    );

    match parse_vmm_memory_action(&profile_request).expect("profile action should parse") {
        VmmWorkbenchAction::LoadProfileNodes { target } => {
            assert_eq!(target, ProfileTarget::ProfileTargetUser);
        }
        other => panic!("unexpected profile action: {other:?}"),
    }
    match parse_vmm_memory_action(&search_request).expect("search action should parse") {
        VmmWorkbenchAction::SearchMemoryEvents { queries, top_k } => {
            assert_eq!(queries, vec![String::from("alpha"), String::from("beta")]);
            assert_eq!(top_k, 5);
        }
        other => panic!("unexpected search action: {other:?}"),
    }
    match parse_vmm_memory_action(&delete_request).expect("delete action should parse") {
        VmmWorkbenchAction::DeleteMemories { memory_ids, reason } => {
            assert_eq!(memory_ids, vec![7, 11]);
            assert_eq!(reason, "stale");
        }
        other => panic!("unexpected delete action: {other:?}"),
    }
    let error = parse_vmm_memory_action(&test_vmm_action_request("unknown", json!({})))
        .expect_err("unknown action should fail");
    assert!(error.contains("不支持的 VMM 记忆动作"));
}

/// Verifies session-scoped suppression resolves the explicit request route instead of current selection.
/// 验证会话级抑制解析明确请求路由，而不是使用当前选中状态。
#[test]
fn vmm_suppression_session_id_uses_explicit_conversation() {
    // selected_session_id represents mutable UI selection that must not own the request.
    // selected_session_id 代表不得拥有该请求的可变 UI 选中状态。
    let selected_session_id = SessionId::new_v4();
    // requested_session_id represents the route that initiated suppression before selection changed.
    // requested_session_id 代表选中状态变化前发起抑制动作的路由。
    let requested_session_id = SessionId::new_v4();
    // workspace retains both active sessions while the first one is currently selected.
    // workspace 在第一个会话当前被选中时保留两个活动会话。
    let workspace = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                selected_session_id,
                None,
                "当前选中会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
            SessionSummary::new(
                requested_session_id,
                None,
                "请求发起会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
        ],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: selected_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    // resolved_session_id proves the explicit request remains authoritative after route changes.
    // resolved_session_id 证明路由变化后明确请求仍保持权威。
    let resolved_session_id =
        vmm_suppression_session_id(&workspace, Some(&requested_session_id.to_string()))
            .expect("explicit active conversation should resolve");

    assert_eq!(resolved_session_id, requested_session_id);
    assert!(vmm_suppression_session_id(&workspace, None).is_err());
    assert!(
        vmm_suppression_session_id(&workspace, Some(&SessionId::new_v4().to_string())).is_err()
    );
}

/// Verifies draft conversion parses transport and behavior fields without silent drift.
/// 验证草稿转换会稳定解析传输字段与行为字段，避免静默漂移。
#[test]
fn vmm_config_layer_from_draft_parses_transport_and_behavior_fields() {
    let draft = VmmConfigDraftSummary {
        has_override: true,
        enabled: true,
        grpc_target: String::from(" http://127.0.0.1:50051 "),
        grpc_api_key: String::from(" secret "),
        user_id: String::from("7"),
        project_id: String::from("11"),
        receive_timeout_ms: String::from("1200"),
        handshake_timeout_ms: String::from("300"),
        max_receive_message_bytes: String::from("4096"),
        grpc_keepalive_time_ms: String::from("5000"),
        grpc_keepalive_timeout_ms: String::from("6000"),
        grpc_keepalive_permit_without_calls: true,
        implicit_memory_turns: String::from("9"),
        profile_refresh_turns: String::from("13"),
        profile_node_limit: String::from("17"),
        ai_tools_enabled: true,
        precheck_mode: String::from("disabled"),
        precheck_enabled: false,
        memory_writeback_enabled: true,
        memory_writeback_mode: String::from("enabled"),
        injection_tag: String::from("VMM_TAG"),
        injection_description: String::from("Visible memories"),
    };

    let layer =
        vmm_config_layer_from_draft(&draft, true).expect("draft should convert to VMM layer");

    assert_eq!(layer.enabled, Some(true));
    assert_eq!(layer.grpc_target.as_deref(), Some("http://127.0.0.1:50051"));
    assert_eq!(layer.grpc_api_key.as_deref(), Some("secret"));
    assert_eq!(layer.user_id, Some(7));
    assert_eq!(layer.project_id, Some(11));
    assert_eq!(layer.receive_timeout_ms, Some(1200));
    assert_eq!(layer.handshake_timeout_ms, Some(300));
    assert_eq!(layer.max_receive_message_bytes, Some(4096));
    assert_eq!(layer.grpc_keepalive_time_ms, Some(5000));
    assert_eq!(layer.grpc_keepalive_timeout_ms, Some(6000));
    assert_eq!(layer.grpc_keepalive_permit_without_calls, Some(true));
    assert_eq!(layer.implicit_memory_turns, Some(9));
    assert_eq!(layer.profile_refresh_turns, Some(13));
    assert_eq!(layer.profile_node_limit, Some(17));
    assert_eq!(layer.ai_tools_enabled, Some(true));
    assert_eq!(layer.precheck_enabled, Some(false));
    assert_eq!(layer.memory_writeback_enabled, Some(true));
    assert_eq!(layer.injection_tag.as_deref(), Some("VMM_TAG"));
    assert_eq!(
        layer.injection_description.as_deref(),
        Some("Visible memories")
    );
}

/// Verifies project-layer conversion strips transport-only fields.
/// 验证项目层转换会剥离仅属于传输层的字段。
#[test]
fn vmm_config_layer_from_draft_omits_transport_fields_for_project_layer() {
    let draft = VmmConfigDraftSummary {
        enabled: true,
        grpc_target: String::from("http://127.0.0.1:50051"),
        grpc_api_key: String::from("secret"),
        receive_timeout_ms: String::from("1200"),
        handshake_timeout_ms: String::from("300"),
        max_receive_message_bytes: String::from("4096"),
        grpc_keepalive_time_ms: String::from("5000"),
        grpc_keepalive_timeout_ms: String::from("6000"),
        grpc_keepalive_permit_without_calls: true,
        user_id: String::from("7"),
        project_id: String::from("11"),
        implicit_memory_turns: String::from("9"),
        profile_refresh_turns: String::from("13"),
        profile_node_limit: String::from("17"),
        precheck_enabled: true,
        ai_tools_enabled: true,
        injection_tag: String::from("PROJECT_TAG"),
        injection_description: String::from("Project memories"),
        ..Default::default()
    };

    let layer = vmm_config_layer_from_draft(&draft, false).expect("project draft should convert");

    assert_eq!(layer.grpc_target, None);
    assert_eq!(layer.grpc_api_key, None);
    assert_eq!(layer.receive_timeout_ms, None);
    assert_eq!(layer.handshake_timeout_ms, None);
    assert_eq!(layer.max_receive_message_bytes, None);
    assert_eq!(layer.grpc_keepalive_time_ms, None);
    assert_eq!(layer.grpc_keepalive_timeout_ms, None);
    assert_eq!(layer.grpc_keepalive_permit_without_calls, None);
    assert_eq!(layer.user_id, Some(7));
    assert_eq!(layer.project_id, Some(11));
    assert_eq!(layer.injection_tag.as_deref(), Some("PROJECT_TAG"));
}

/// Verifies selected-session VMM snapshots ignore blank project ids before scope fallback.
/// 验证选中会话的 VMM 快照会先忽略空白项目标识再执行作用域回退。
#[test]
fn vmm_memory_snapshot_ignores_blank_selected_session_project_id() {
    // session_id identifies the selected conversation in the VMM snapshot.
    // session_id 标识 VMM 快照中的选中会话。
    let session_id = SessionId::new_v4();
    // workspace contains a blank-id project after the valid fallback project.
    // workspace 在有效回退项目之后包含空白标识项目。
    let workspace = WorkspaceSnapshot {
        task_projects: vec![
            TaskProjectSummary::new(
                "project-a",
                "Project A",
                "D:/project-a",
                false,
                0,
                None,
                None,
                vec![session_id],
            ),
            TaskProjectSummary::new(
                "   ",
                "Blank Project",
                "D:/blank-project",
                false,
                0,
                None,
                None,
                Vec::new(),
            ),
        ],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("   ")),
            "VMM 快照测试会话",
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
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let snapshot = vmm_memory_snapshot(&workspace, &ProviderConfigurationFile::default(), None);

    assert_eq!(snapshot.selected_project_id.as_deref(), Some("project-a"));
}

/// Verifies static profile snapshots group active nodes by target and date.
/// 验证静态画像快照按作用域与日期分组 active 节点。
#[test]
fn static_vmm_profile_snapshot_groups_nodes_by_target_and_date() {
    let mut state = SessionRuntimeStateEntry::default();
    state.vmm_profile_sync.updated_at = Some(String::from("2026-06-27T00:00:00Z"));
    state.vmm_profile_sync.nodes_by_id.insert(
        42,
        ProfileNodeEntry {
            profile_node_id: 42,
            target: ProfileTarget::ProfileTargetProject,
            bind_id: 11,
            content: String::from("  project fact  "),
            priority: String::from("1"),
            level: String::from("L2"),
            refresh_weight: 5,
            profile_date: String::from("2026-06-27"),
            source_kind: ProfileNodeSourceKind::ProfileNodeSourceKindManualInstruction,
            source_id: 9,
            ..Default::default()
        },
    );

    let snapshot = static_vmm_profile_snapshot(&state).expect("profile snapshot should exist");

    assert_eq!(snapshot.updated_at.as_deref(), Some("2026-06-27T00:00:00Z"));
    assert_eq!(snapshot.sections.len(), 1);
    assert_eq!(snapshot.sections[0].target, "project");
    assert_eq!(snapshot.sections[0].dates.len(), 1);
    assert_eq!(snapshot.sections[0].dates[0].date, "2026-06-27");
    assert_eq!(snapshot.sections[0].dates[0].nodes[0].id, 42);
    assert_eq!(
        snapshot.sections[0].dates[0].nodes[0].content,
        "project fact"
    );
    assert_eq!(
        snapshot.sections[0].dates[0].nodes[0].source_kind,
        "manual_instruction"
    );
}
