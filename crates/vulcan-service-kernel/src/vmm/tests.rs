//! VMM tests.
//! VMM 测试。

use super::*;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER;
use vulcan_service_runtime::vmm_management::VmmWorkbenchAction;
use vulcan_vmm::{ProfileNodeEntry, ProfileNodeSourceKind, ProfileTarget};
use vulcan_workbench_model::{SessionId, SessionState, SessionSummary};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

/// Builds a default VMM action request for parser unit tests.
/// 为解析器单元测试构建默认 VMM 动作请求。
fn test_vmm_action_request(action: &str, payload: Value) -> VmmMemoryActionRequest {
    VmmMemoryActionRequest {
        action: action.to_string(),
        command_id: uuid::Uuid::new_v4().to_string(),
        scope: String::from("global"),
        project_id: None,
        conversation_id: None,
        payload,
    }
}

/// Verifies a static panel read never creates prompt-sync state when VMM is disabled.
/// 验证 VMM 关闭时静态侧栏读取绝不创建提示词同步状态。
#[tokio::test]
async fn static_profile_panel_read_is_read_only_when_disabled() {
    let session_id = SessionId::new_v4();
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(session_id);
    service.agent_runtime = Some(crate::tests::test_support::test_agent_runtime_bridge());
    let state = Arc::new(Mutex::new(service));
    let mut request = test_vmm_action_request("load_static_profile_for_panel", json!({}));
    request.conversation_id = Some(session_id.to_string());

    let result =
        super::memory_actions::service_load_static_profile_for_panel(state.clone(), request)
            .await
            .expect("disabled profile read should succeed");
    assert!(matches!(
        result.data,
        vulcan_service_api::VmmMemoryActionResponse::StaticProfile(None)
    ));
    assert!(
        state
            .lock()
            .expect("service state")
            .workspace_snapshot
            .as_ref()
            .expect("workspace")
            .session_runtime_state
            .is_empty()
    );
}

/// Verifies an indexed session can restore durable dynamic leases without prompt hydration.
/// 验证索引会话可在不水合提示词运行态的情况下恢复持久动态租约。
#[tokio::test]
async fn dynamic_memory_panel_read_restores_persisted_lease_after_restart() {
    let session_id = SessionId::new_v4();
    let root = crate::tests::test_support::TestHostRuntimeRoot::new(session_id);
    let host_runtime = root.attached_host_runtime_handle().await;
    let locator = vulcan_workspace_store::workspace_store::locate_session_v1_storage(
        &host_runtime,
        &session_id.to_string(),
    )
    .await
    .expect("indexed locator");
    let mut memory = vulcan_vmm::VmmSessionMemoryState::new(session_id.to_string(), "1")
        .expect("session memory");
    memory
        .apply_recall_batch(
            &session_id.to_string(),
            "user-anchor",
            &vulcan_vmm::VmmRecallBatch {
                ttl_turns: 2,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                items: vec![vulcan_vmm::ContextItem {
                    text: String::from("restored dynamic memory"),
                    memory_id: 901,
                    ..vulcan_vmm::ContextItem::default()
                }],
            },
            "1",
        )
        .expect("resident lease");
    vulcan_workspace_store::workspace_store::persist_session_vmm_state(&locator, &memory)
        .expect("persist VMM state before restart");

    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(session_id);
    service.controller_runtime = Some(host_runtime);
    let state = Arc::new(Mutex::new(service));
    let mut request = test_vmm_action_request("load_dynamic_memory_for_panel", json!({}));
    request.conversation_id = Some(session_id.to_string());
    let result =
        super::memory_actions::service_load_dynamic_memory_for_panel(state.clone(), request)
            .await
            .expect("persisted dynamic memory read");
    let vulcan_service_api::VmmMemoryActionResponse::DynamicMemoryCards(cards) = result.data else {
        panic!("dynamic memory cards response expected");
    };
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].tag, "VMM_MEMORY");
    assert_eq!(cards[0].items.len(), 1);
    assert!(
        state
            .lock()
            .expect("service state")
            .workspace_snapshot
            .as_ref()
            .expect("workspace")
            .session_runtime_state
            .is_empty()
    );
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

/// Verifies backend-projected navigation never presents external identities as native concepts.
/// 验证后端投影导航不会把外置身份概念呈现为原生概念。
#[test]
fn vmm_management_sections_use_backend_specific_identity_copy() {
    let native = vmm_management_sections(vulcan_vmm::VmmBackendMode::Native);
    let external = vmm_management_sections(vulcan_vmm::VmmBackendMode::ExternalGrpc);

    let native_identity = native
        .iter()
        .find(|section| section.id == "identity_projects")
        .expect("native identity section");
    let external_identity = external
        .iter()
        .find(|section| section.id == "identity_projects")
        .expect("external identity section");

    assert_eq!(native_identity.label, "记忆项目");
    assert!(!native_identity.description.contains("身份"));
    assert_eq!(external_identity.label, "用户与项目");
    assert!(external_identity.description.contains("外置 VMM 身份"));
    assert!(native.iter().any(|section| section.id == "inference"));
    assert!(native.iter().any(|section| section.id == "resource_rules"));
    assert!(native.iter().any(|section| section.id == "maintenance"));
    assert!(!external.iter().any(|section| section.id == "inference"));
    assert!(
        !external
            .iter()
            .any(|section| section.id == "resource_rules")
    );
    assert!(!external.iter().any(|section| section.id == "maintenance"));
}

/// Verifies setup probes can use one external connection without persisting a global draft.
/// 验证配置探测可以使用单次外置连接，而无需持久化全局草稿。
#[test]
fn external_setup_action_connection_builds_an_enabled_request_scope() {
    let request = test_vmm_action_request(
        "healthz",
        json!({
            "connection": {
                "grpcTarget": " 127.0.0.1:50051 ",
                "grpcApiKey": "secret"
            }
        }),
    );

    let layer = vmm_external_action_layer_override(&request)
        .expect("request-scoped connection should parse")
        .expect("connection override should exist");

    assert_eq!(
        layer.backend_mode,
        Some(vulcan_vmm::VmmBackendMode::ExternalGrpc)
    );
    assert_eq!(layer.enabled, Some(true));
    assert_eq!(layer.grpc_target.as_deref(), Some("127.0.0.1:50051"));
    assert_eq!(layer.grpc_api_key.as_deref(), Some("secret"));
}

/// Verifies setup connection overrides reject project scope and unknown fields.
/// 验证配置连接覆盖会拒绝项目作用域和未知字段。
#[test]
fn external_setup_action_connection_rejects_ambiguous_payloads() {
    let mut project_request = test_vmm_action_request(
        "healthz",
        json!({ "connection": { "grpcTarget": "127.0.0.1:50051" } }),
    );
    project_request.scope = String::from("project");
    assert!(
        vmm_external_action_layer_override(&project_request)
            .expect_err("project-scoped override must fail")
            .contains("只允许用于全局配置向导")
    );

    let unknown = test_vmm_action_request(
        "healthz",
        json!({
            "connection": {
                "grpcTarget": "127.0.0.1:50051",
                "fallbackTarget": "127.0.0.1:50052"
            }
        }),
    );
    assert!(
        vmm_external_action_layer_override(&unknown)
            .expect_err("unknown connection fields must fail")
            .contains("unknown field")
    );
}

/// Verifies transient setup credentials cannot authorize destructive external operations.
/// 验证临时配置凭据不能授权破坏性外置操作。
#[test]
fn transient_external_connection_allows_only_setup_actions() {
    for action in [
        "healthz",
        "load_users_projects",
        "resolve_user",
        "ensure_project",
    ] {
        validate_transient_external_action(action)
            .unwrap_or_else(|error| panic!("setup action `{action}` should be allowed: {error}"));
    }
    for action in ["delete_project", "migrate_project", "delete_memories"] {
        let error = validate_transient_external_action(action)
            .expect_err("destructive transient action should fail");
        assert!(error.contains(action));
    }
}

/// Verifies persisted external actions obey the global backend selection.
/// 验证持久化外置动作遵守全局后端选择。
#[test]
fn persisted_external_actions_require_external_backend() {
    ensure_external_action_backend(vulcan_vmm::VmmBackendMode::ExternalGrpc)
        .expect("external backend should allow external actions");
    let error = ensure_external_action_backend(vulcan_vmm::VmmBackendMode::Native)
        .expect_err("native backend should reject external actions");
    assert!(error.contains("全局选择外置 gRPC 后端"));
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

/// Verifies suppression retries preserve one command identity independently of VMM revisions.
/// 验证抑制重试独立于 VMM 修订号保留同一个命令身份。
#[test]
fn vmm_suppression_command_id_is_stable_and_strict() {
    // request_command_id represents the identity generated once by the initiating WebUI operation.
    // request_command_id 代表发起 WebUI 操作时仅生成一次的身份。
    let request_command_id = uuid::Uuid::new_v4().to_string();
    let first = vmm_suppression_command_id(&request_command_id).expect("accept valid command id");
    let retry = vmm_suppression_command_id(&request_command_id).expect("accept retry command id");

    assert_eq!(first, retry);
    assert_eq!(first, format!("suppress_memory:{request_command_id}"));
    assert!(vmm_suppression_command_id("not-a-uuid").is_err());
    assert!(vmm_suppression_command_id(&format!(" {request_command_id}")).is_err());
}

/// Verifies draft conversion parses transport and behavior fields without silent drift.
/// 验证草稿转换会稳定解析传输字段与行为字段，避免静默漂移。
#[test]
fn vmm_config_layer_from_draft_parses_transport_and_behavior_fields() {
    let draft = VmmConfigDraftSummary {
        has_override: true,
        backend_mode: String::from("external_grpc"),
        native_project_binding_mode: String::from("fixed"),
        external_project_binding_mode: String::from("per_project"),
        native_fixed_project_id: String::from("1"),
        external_project_prefix: String::from("vulcan/space"),
        enabled: true,
        grpc_target: String::from(" http://127.0.0.1:50051 "),
        grpc_api_key: String::from(" secret "),
        management_base_url: String::from("http://127.0.0.1:17635"),
        management_bearer_token: String::from("management-secret"),
        management_timeout_ms: String::from("30000"),
        user_id: String::from("7"),
        project_id: String::from("11"),
        receive_timeout_ms: String::from("12000"),
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
    assert_eq!(
        layer.management_base_url.as_deref(),
        Some("http://127.0.0.1:17635")
    );
    assert_eq!(
        layer.management_bearer_token.as_deref(),
        Some("management-secret")
    );
    assert_eq!(layer.management_timeout_ms, Some(30000));
    assert_eq!(layer.user_id, Some(7));
    assert_eq!(layer.project_id, Some(11));
    assert_eq!(layer.receive_timeout_ms, Some(12000));
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

/// Verifies project drafts cannot select a backend different from global execution authority.
/// 验证项目草稿不能选择与全局执行权威不同的后端。
#[test]
fn project_draft_uses_the_executable_global_backend() {
    // Both legacy directions must project the globally executed backend without changing persisted layers.
    // 两种历史冲突方向均必须投影全局执行后端，同时不修改持久层。
    for (global_mode, stale_project_mode) in [
        (
            vulcan_vmm::VmmBackendMode::Native,
            vulcan_vmm::VmmBackendMode::ExternalGrpc,
        ),
        (
            vulcan_vmm::VmmBackendMode::ExternalGrpc,
            vulcan_vmm::VmmBackendMode::Native,
        ),
    ] {
        // Global and project retain their original conflicting declarations for this regression.
        // 全局与项目保留本回归所需的原始冲突声明。
        let global = vulcan_vmm::VmmConfigLayer {
            backend_mode: Some(global_mode),
            ..Default::default()
        };
        let project = vulcan_vmm::VmmConfigLayer {
            backend_mode: Some(stale_project_mode),
            ..Default::default()
        };
        // The same resolver is used by execution and by the project-load service.
        // 执行和项目加载服务使用相同解析器。
        let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&global, Some(&project), None);
        assert_eq!(
            vmm_config_draft_summary(&project, &effective, true, false).backend_mode,
            global_mode.as_str()
        );
        assert_eq!(project.backend_mode, Some(stale_project_mode));
    }
}

/// Verifies persisted external transport secrets never cross the WebUI snapshot boundary.
/// 验证持久化外置传输秘密绝不跨过 WebUI 快照边界。
#[test]
fn vmm_config_draft_summary_redacts_external_transport_secrets() {
    let layer = vulcan_vmm::VmmConfigLayer {
        backend_mode: Some(vulcan_vmm::VmmBackendMode::ExternalGrpc),
        grpc_api_key: Some(String::from("grpc-secret")),
        management_bearer_token: Some(String::from("management-secret")),
        ..vulcan_vmm::VmmConfigLayer::default()
    };
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&layer, None, None);

    let draft = vmm_config_draft_summary(&layer, &effective, true, true);

    assert_eq!(draft.grpc_api_key, WEBUI_REDACTED_SECRET_PLACEHOLDER);
    assert_eq!(
        draft.management_bearer_token,
        WEBUI_REDACTED_SECRET_PLACEHOLDER
    );
    assert_ne!(draft.grpc_api_key, "grpc-secret");
    assert_ne!(draft.management_bearer_token, "management-secret");
}

/// Verifies external management transport requires a complete secure endpoint-token pair.
/// 验证外部管理传输必须提供完整且安全的端点与 Token 组合。
#[test]
fn vmm_config_layer_from_draft_validates_external_management_transport() {
    let missing_token = VmmConfigDraftSummary {
        backend_mode: String::from("external_grpc"),
        native_project_binding_mode: String::from("fixed"),
        external_project_binding_mode: String::from("fixed"),
        native_fixed_project_id: String::from("1"),
        external_project_prefix: String::from("vulcan/space"),
        management_base_url: String::from("https://memory.example.test"),
        management_timeout_ms: String::from("30000"),
        ..Default::default()
    };
    assert!(
        vmm_config_layer_from_draft(&missing_token, true)
            .expect_err("management token must be required")
            .contains("缺少管理 Token")
    );

    let insecure_remote = VmmConfigDraftSummary {
        backend_mode: String::from("external_grpc"),
        native_project_binding_mode: String::from("fixed"),
        external_project_binding_mode: String::from("fixed"),
        native_fixed_project_id: String::from("1"),
        external_project_prefix: String::from("vulcan/space"),
        management_base_url: String::from("http://memory.example.test"),
        management_bearer_token: String::from("secret"),
        management_timeout_ms: String::from("30000"),
        ..Default::default()
    };
    assert!(
        vmm_config_layer_from_draft(&insecure_remote, true)
            .expect_err("remote management must require HTTPS")
            .contains("必须使用 HTTPS")
    );

    let embedded_credentials = VmmConfigDraftSummary {
        backend_mode: String::from("external_grpc"),
        native_project_binding_mode: String::from("fixed"),
        external_project_binding_mode: String::from("fixed"),
        native_fixed_project_id: String::from("1"),
        external_project_prefix: String::from("vulcan/space"),
        management_base_url: String::from("https://user:password@memory.example.test"),
        management_bearer_token: String::from("secret"),
        management_timeout_ms: String::from("30000"),
        ..Default::default()
    };
    assert!(
        vmm_config_layer_from_draft(&embedded_credentials, true)
            .expect_err("management URL credentials must be rejected")
            .contains("不能包含账号或密码")
    );
}

/// Verifies identity drafts reject zero because VMM business scopes require positive identifiers.
/// 验证身份草稿拒绝零值，因为 VMM 业务作用域要求使用正整数标识。
#[test]
fn vmm_config_layer_from_draft_rejects_zero_identity_ids() {
    let zero_user = VmmConfigDraftSummary {
        user_id: String::from("0"),
        project_id: String::from("1"),
        ..VmmConfigDraftSummary::default()
    };
    let user_error =
        vmm_config_layer_from_draft(&zero_user, false).expect_err("zero user id must be rejected");
    assert!(user_error.contains("VMM 用户 ID 必须是正整数"));

    let zero_project = VmmConfigDraftSummary {
        user_id: String::from("1"),
        project_id: String::from("0"),
        ..VmmConfigDraftSummary::default()
    };
    let project_error = vmm_config_layer_from_draft(&zero_project, false)
        .expect_err("zero project id must be rejected");
    assert!(project_error.contains("VMM 项目 ID 必须是正整数"));
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
        precheck_mode: String::from("disabled"),
        memory_writeback_enabled: true,
        memory_writeback_mode: String::from("enabled"),
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
    assert_eq!(layer.enabled, Some(true));
    assert_eq!(layer.user_id, Some(7));
    assert_eq!(layer.project_id, Some(11));
    assert_eq!(layer.implicit_memory_turns, Some(9));
    assert_eq!(layer.profile_refresh_turns, Some(13));
    assert_eq!(layer.profile_node_limit, Some(17));
    assert_eq!(layer.precheck_enabled, Some(false));
    assert_eq!(layer.memory_writeback_enabled, Some(true));
    assert_eq!(layer.ai_tools_enabled, Some(true));
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
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let snapshot = vmm_memory_snapshot(
        &workspace,
        &ProviderConfigurationFile::default(),
        None,
        None,
    );

    assert_eq!(snapshot.selected_project_id.as_deref(), Some("project-a"));

    // Unique in-memory cache keys prevent either project scope from reading or modifying user files.
    // 唯一内存缓存键防止两个项目作用域读取或修改用户文件。
    let mut scoped_workspace = workspace;
    for (index, project) in scoped_workspace.task_projects.iter_mut().enumerate() {
        project.path = std::env::temp_dir()
            .join(format!("vulcan-memory-scope-{session_id}-{index}"))
            .display()
            .to_string();
        update_cached_project_vmm_layer(
            &project.path,
            vulcan_vmm::VmmConfigLayer {
                enabled: Some(index == 0),
                ..Default::default()
            },
        );
    }
    // The peer is disabled even when the globally selected project is enabled.
    // 即使全局选中的项目已启用，其他项目仍保持关闭。
    let mut configuration = ProviderConfigurationFile::default();
    configuration.vmm.enabled = Some(true);
    let scoped_snapshot = vmm_memory_snapshot(&scoped_workspace, &configuration, None, None);
    assert!(scoped_snapshot.project_scopes[0].memory_enabled);
    assert!(!scoped_snapshot.project_scopes[1].memory_enabled);
    // Global deactivation must close every project without erasing their explicit overrides.
    // 全局关闭必须关闭所有项目，但不能抹掉项目的显式覆盖。
    configuration.vmm.enabled = Some(false);
    let disabled_snapshot = vmm_memory_snapshot(&scoped_workspace, &configuration, None, None);
    assert!(
        disabled_snapshot
            .project_scopes
            .iter()
            .all(|scope| !scope.memory_enabled)
    );
    assert!(
        disabled_snapshot
            .project_scopes
            .iter()
            .all(|scope| scope.has_override)
    );
}

/// Verifies catalog refresh resolves exact global default ids to visible identity values.
/// 验证目录刷新会将准确全局默认标识解析为可见身份值。
#[test]
fn hydrate_vmm_default_identity_uses_exact_catalog_keys() {
    let mut snapshot = VmmMemorySnapshot::default();
    snapshot.global_draft.user_id = String::from("7");
    snapshot.global_draft.project_id = String::from("11");
    snapshot.management_catalog.users = vec![
        vulcan_service_api::VmmUserDisplaySummary {
            stable_key: String::from("70"),
            display_name: String::from("Wrong User"),
            ..Default::default()
        },
        vulcan_service_api::VmmUserDisplaySummary {
            stable_key: String::from("7"),
            display_name: String::from("Visible User"),
            ..Default::default()
        },
    ];
    snapshot.management_catalog.projects = vec![
        vulcan_service_api::VmmProjectDisplaySummary {
            stable_key: String::from("110"),
            display_name: String::from("Wrong Project"),
            display_path: String::from("wrong/space/project"),
            ..Default::default()
        },
        vulcan_service_api::VmmProjectDisplaySummary {
            stable_key: String::from("11"),
            display_name: String::from("Visible Project"),
            display_path: String::from("team/space/visible-project"),
            ..Default::default()
        },
    ];

    hydrate_vmm_default_identity(&mut snapshot);

    assert_eq!(
        snapshot.display_identity.user_name.as_deref(),
        Some("Visible User")
    );
    assert_eq!(
        snapshot.display_identity.project_path.as_deref(),
        Some("team/space/visible-project")
    );
    snapshot.global_draft.user_id.clear();
    snapshot.global_draft.project_id.clear();
    hydrate_vmm_default_identity(&mut snapshot);
    assert!(snapshot.display_identity.user_name.is_none());
    assert!(snapshot.display_identity.project_name.is_none());
    assert!(snapshot.display_identity.project_path.is_none());
}

/// Verifies native runtime readiness follows completed Host AI validation instead of handle presence.
/// 验证原生运行时就绪状态取决于已完成的宿主 AI 校验，而不是句柄是否存在。
#[test]
fn native_runtime_summary_requires_validated_host_ai() {
    // effective enables the native backend with its host-owned default identity.
    // effective 使用宿主持有的默认身份启用原生后端。
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
        &vulcan_vmm::VmmConfigLayer {
            backend_mode: Some(vulcan_vmm::VmmBackendMode::Native),
            enabled: Some(true),
            ..vulcan_vmm::VmmConfigLayer::default()
        },
        None,
        None,
    );
    // unchecked proves an allocated native runtime cannot impersonate validated AI readiness.
    // unchecked 证明已分配的原生运行时不能冒充已校验的 AI 就绪状态。
    let unchecked = crate::native_vmm::NativeVmmHostAiStatus::default();
    // blocked is the product projection before Host AI validation completes.
    // blocked 是宿主 AI 校验完成前的产品投影。
    let blocked = vmm_runtime_summary(None, &effective, Some(&unchecked));
    assert!(!blocked.process_ready);
    assert!(!blocked.scope_ready);
    assert_eq!(blocked.backend_state, "unchecked");
    assert!(blocked.backend_error.is_some());

    // validated independently exposes required readiness and optional capability availability.
    // validated 分别暴露必需能力就绪状态与可选能力可用性。
    let validated = crate::native_vmm::NativeVmmHostAiStatus {
        checked: true,
        ready: true,
        profile_instruction_ready: false,
        profile_instruction_error: Some(String::from("not configured")),
        rerank_ready: true,
        rerank_error: None,
        error: None,
    };
    // ready is the product projection after the same runtime completes validation.
    // ready 是同一运行时完成校验后的产品投影。
    let ready = vmm_runtime_summary(None, &effective, Some(&validated));
    assert!(ready.process_ready);
    assert!(ready.scope_ready);
    assert!(ready.precheck_ready);
    assert_eq!(
        ready.profile_instruction_error.as_deref(),
        Some("not configured")
    );
    assert!(ready.postaction_ready);
    assert!(!ready.profile_instruction_ready);
    assert!(ready.rerank_ready);
    assert_eq!(ready.backend_state, "ready");
    assert!(ready.backend_error.is_none());
}

/// Verifies project rows use the canonical structured path for actions and display.
/// 验证项目记录会把规范结构化路径用于动作与展示。
#[test]
fn vmm_project_display_summary_separates_transport_reference_from_human_path() {
    let project = vulcan_vmm::ProjectEntry {
        project_id: 9,
        team_id: 1,
        space_id: 2,
        team_name: String::from("Team A"),
        space_name: String::from("Workspace"),
        project_name: String::from("Product"),
        display_path: String::from("[9]Team A/Workspace/Product"),
    };

    let summary = vmm_project_display_summary(&project);

    assert_eq!(summary.display_name, "Product");
    assert_eq!(summary.display_path, "Team A/Workspace/Product");
    assert_eq!(summary.action_path, "Team A/Workspace/Product");
    assert!(!summary.protected);
}

/// Verifies the default VMM project remains protected through structured names.
/// 验证默认 VMM 项目会通过结构化名称继续受到保护。
#[test]
fn vmm_project_display_summary_protects_default_project_without_parsing_display_text() {
    let project = vulcan_vmm::ProjectEntry {
        project_id: 1,
        team_id: 1,
        space_id: 1,
        team_name: String::from("default"),
        space_name: String::from("default"),
        project_name: String::from("default"),
        display_path: String::from("[1]default/default/default"),
    };

    let summary = vmm_project_display_summary(&project);

    assert_eq!(summary.display_path, "default/default/default");
    assert_eq!(summary.action_path, "default/default/default");
    assert!(summary.protected);
}

/// Verifies runtime defaults cannot impersonate an explicitly configured VMM deployment.
/// 验证运行时默认值不会冒充用户明确配置的 VMM 部署。
#[test]
fn vmm_deployment_mode_requires_explicit_global_configuration() {
    let empty = vulcan_vmm::VmmConfigLayer::default();
    assert_eq!(vmm_deployment_mode(&empty), VmmDeploymentMode::Disabled);

    let external = vulcan_vmm::VmmConfigLayer {
        enabled: Some(true),
        grpc_target: Some(String::from("127.0.0.1:17625")),
        ..vulcan_vmm::VmmConfigLayer::default()
    };
    assert_eq!(vmm_deployment_mode(&external), VmmDeploymentMode::External);
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
