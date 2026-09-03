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
        command_id: uuid::Uuid::new_v4().to_string(),
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
        enabled: true,
        grpc_target: String::from(" http://127.0.0.1:50051 "),
        grpc_api_key: String::from(" secret "),
        management_base_url: String::from("http://127.0.0.1:17635"),
        management_bearer_token: String::from("management-secret"),
        management_timeout_ms: String::from("30000"),
        managed_install_enabled: false,
        managed_binary_path: String::new(),
        auto_start_policy: String::from("on_start"),
        managed_runtime_settings: Default::default(),
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

/// Verifies external management transport requires a complete secure endpoint-token pair.
/// 验证外部管理传输必须提供完整且安全的端点与 Token 组合。
#[test]
fn vmm_config_layer_from_draft_validates_external_management_transport() {
    let missing_token = VmmConfigDraftSummary {
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

/// Verifies managed behavior conversion preserves explicit values and rejects unsafe timing relationships.
/// 验证托管行为转换会保留明确值，并拒绝不安全的超时关系。
#[test]
fn vmm_config_layer_from_draft_validates_managed_behavior_boundaries() {
    let mut draft = VmmConfigDraftSummary {
        receive_timeout_ms: String::from("12000"),
        ..Default::default()
    };
    draft.managed_runtime_settings.precheck_top_k = String::from("9");
    draft.managed_runtime_settings.precheck_intent_timeout = String::from("7s");
    draft.managed_runtime_settings.retention_session_idle_after = String::from("360h");
    draft.managed_runtime_settings.postaction_analysis_timeout = String::from("2m");
    draft
        .managed_runtime_settings
        .postaction_failure_pass_threshold = String::from("3");
    draft
        .managed_runtime_settings
        .inference_max_connections_per_host = String::from("16");

    let layer = vmm_config_layer_from_draft(&draft, true).expect("managed behavior should convert");
    let settings = layer
        .install
        .expect("global transport layer should retain managed installation state")
        .runtime_settings;
    assert_eq!(settings.precheck_top_k, 9);
    assert_eq!(settings.precheck_intent_timeout, "7s");
    assert_eq!(settings.retention_session_idle_after, "360h");
    assert_eq!(settings.postaction_analysis_timeout, "2m");
    assert_eq!(settings.postaction_failure_pass_threshold, 3);
    assert_eq!(settings.inference_max_connections_per_host, 16);

    draft.receive_timeout_ms = String::from("7000");
    let timeout_error = vmm_config_layer_from_draft(&draft, true)
        .expect_err("request timeout must exceed PreCheck intent timeout");
    assert!(timeout_error.contains("必须严格大于 PreCheck 意图超时"));

    draft.receive_timeout_ms = String::from("12000");
    draft.managed_runtime_settings.precheck_intent_timeout = String::from("not-a-duration");
    let duration_error = vmm_config_layer_from_draft(&draft, true)
        .expect_err("invalid Go duration must be rejected");
    assert!(duration_error.contains("必须使用 Go 时长格式"));

    draft.managed_runtime_settings.precheck_intent_timeout = String::from("7s");
    draft.managed_runtime_settings.retention_session_idle_after = String::from("359h");
    let retention_error = vmm_config_layer_from_draft(&draft, true)
        .expect_err("retention idle duration below the safety floor must be rejected");
    assert!(retention_error.contains("必须大于或等于 360h"));

    draft.managed_runtime_settings.retention_session_idle_after = String::from("360h");
    draft.managed_runtime_settings.postaction_analysis_timeout = String::from("119s");
    let postaction_timeout_error = vmm_config_layer_from_draft(&draft, true)
        .expect_err("PostAction background timeout below two minutes must be rejected");
    assert!(postaction_timeout_error.contains("必须大于或等于 2m"));

    draft.managed_runtime_settings.postaction_analysis_timeout = String::from("2m");
    draft
        .managed_runtime_settings
        .postaction_failure_pass_threshold = String::from("0");
    let failure_threshold_error = vmm_config_layer_from_draft(&draft, true)
        .expect_err("zero failure Pass threshold must be rejected");
    assert!(failure_threshold_error.contains("必须位于 1 到 4294967295 之间"));

    draft
        .managed_runtime_settings
        .postaction_failure_pass_threshold = String::from("3");
    draft
        .managed_runtime_settings
        .inference_max_connections_per_host = String::from("65");
    let connection_limit_error = vmm_config_layer_from_draft(&draft, true)
        .expect_err("inference connection ceiling above 64 must be rejected");
    assert!(connection_limit_error.contains("必须位于 1 到 64 之间"));
}

/// Verifies project-layer conversion strips transport-only fields.
/// 验证项目层转换会剥离仅属于传输层的字段。
#[test]
fn vmm_config_layer_from_draft_omits_transport_fields_for_project_layer() {
    let draft = VmmConfigDraftSummary {
        enabled: true,
        grpc_target: String::from("http://127.0.0.1:50051"),
        grpc_api_key: String::from("secret"),
        managed_install_enabled: false,
        managed_binary_path: String::new(),
        auto_start_policy: String::from("on_start"),
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

/// Verifies managed lifecycle transitions rebuild all product states without losing the management catalog.
/// 验证托管生命周期转换会重建全部产品状态且不会丢失管理目录。
#[test]
fn managed_runtime_projection_rebuilds_product_state_and_preserves_catalog() {
    // session_id identifies the exact retained conversation used by the canonical VMM snapshot builder.
    // session_id 标识规范 VMM 快照构建器使用的准确保留会话。
    let session_id = SessionId::new_v4();
    // workspace supplies one valid selected project and conversation without relying on process-global fixtures.
    // workspace 提供一个有效的当前项目与会话，不依赖进程级全局夹具。
    let workspace = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            "project-a",
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "Managed VMM projection",
            "",
            SessionState::Completed,
            0,
            0,
            "now",
        )],
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    // provider_configuration expresses one complete managed deployment with every user-controlled memory feature enabled.
    // provider_configuration 表示一份完整托管部署，并启用所有由用户控制的记忆能力。
    let provider_configuration = ProviderConfigurationFile {
        vmm: vulcan_vmm::VmmConfigLayer {
            enabled: Some(true),
            grpc_target: Some(String::from("127.0.0.1:17901")),
            user_id: Some(7),
            project_id: Some(11),
            precheck_enabled: Some(true),
            memory_writeback_enabled: Some(true),
            ai_tools_enabled: Some(true),
            install: Some(vulcan_vmm::VmmInstallReservation {
                managed_install_enabled: true,
                ..vulcan_vmm::VmmInstallReservation::default()
            }),
            ..vulcan_vmm::VmmConfigLayer::default()
        },
        ..ProviderConfigurationFile::default()
    };
    // retained_snapshot carries the explicit catalog that lifecycle refreshes must preserve.
    // retained_snapshot 携带生命周期刷新必须保留的显式管理目录。
    let mut retained_snapshot = VmmMemorySnapshot::default();
    retained_snapshot.management_catalog.users = vec![vulcan_service_api::VmmUserDisplaySummary {
        stable_key: String::from("7"),
        display_name: String::from("Visible User"),
        ..Default::default()
    }];
    retained_snapshot.management_catalog.projects =
        vec![vulcan_service_api::VmmProjectDisplaySummary {
            stable_key: String::from("11"),
            display_name: String::from("Visible Project"),
            display_path: String::from("default/default/visible-project"),
            ..Default::default()
        }];
    // managed_runtime represents a healthy child with every independently preflighted capability available.
    // managed_runtime 表示一个健康子进程，且每项独立预检能力均可用。
    let mut managed_runtime = crate::managed_memory_mesh_runtime::ManagedMemoryMeshSnapshot {
        data_space_state: vulcan_service_api::VmmDataSpaceState::Ready,
        phase: String::from("ready"),
        grpc_target: Some(String::from("127.0.0.1:17901")),
        controller_endpoint: String::from("http://127.0.0.1:19801"),
        controller_space_id: String::from("vmm-memory"),
        data_root: std::env::temp_dir().join("vulcan-vmm-product-state-data"),
        log_path: std::env::temp_dir().join("vulcan-vmm-product-state.log"),
        last_error: None,
        scope_ready: true,
        scope_error: None,
        capabilities: crate::managed_memory_mesh_runtime::ManagedVmmCapabilityPreflight {
            embedding_dimension: 1024,
            embedding_fingerprint: None,
            precheck_ready: true,
            precheck_error: None,
            postaction_ready: true,
            postaction_error: None,
            profile_instruction_ready: true,
            profile_instruction_error: None,
            rerank_ready: true,
            rerank_error: None,
        },
    };

    let ready = refresh_vmm_memory_runtime_projection(
        &retained_snapshot,
        &workspace,
        &provider_configuration,
        &managed_runtime,
    );

    assert!(ready.runtime.process_ready);
    assert_eq!(
        ready.experience_state.deployment,
        VmmDeploymentMode::Managed
    );
    assert_eq!(ready.experience_state.setup, VmmSetupState::Ready);
    assert_eq!(ready.experience_state.lifecycle, VmmLifecycleState::Running);
    assert_eq!(ready.experience_state.health, VmmHealthState::Healthy);
    assert!(
        ready
            .capabilities
            .iter()
            .all(|capability| capability.available)
    );
    assert_eq!(
        ready.management_catalog,
        retained_snapshot.management_catalog
    );
    assert_eq!(
        ready.display_identity.user_name.as_deref(),
        Some("Visible User")
    );
    assert_eq!(
        ready.display_identity.project_path.as_deref(),
        Some("default/default/visible-project")
    );

    // A live child without an initialized scope is not an operational VMM service.
    // 子进程存活但作用域未初始化时，不构成可运行的 VMM 服务。
    managed_runtime.scope_ready = false;
    let scope_incomplete = refresh_vmm_memory_runtime_projection(
        &ready,
        &workspace,
        &provider_configuration,
        &managed_runtime,
    );
    assert!(scope_incomplete.runtime.process_ready);
    assert!(!scope_incomplete.runtime.scope_ready);
    assert!(!scope_incomplete.runtime.precheck_ready);
    assert!(!scope_incomplete.runtime.postaction_ready);
    assert!(!scope_incomplete.runtime.profile_instruction_ready);
    assert!(!scope_incomplete.runtime.rerank_ready);
    assert!(
        scope_incomplete
            .capabilities
            .iter()
            .all(|capability| !capability.available)
    );

    managed_runtime.phase = String::from("stopped");
    managed_runtime.grpc_target = None;
    let stopped = refresh_vmm_memory_runtime_projection(
        &ready,
        &workspace,
        &provider_configuration,
        &managed_runtime,
    );

    assert!(!stopped.runtime.process_ready);
    assert_eq!(
        stopped.experience_state.lifecycle,
        VmmLifecycleState::Stopped
    );
    assert_eq!(stopped.experience_state.health, VmmHealthState::Unknown);
    assert!(
        stopped
            .capabilities
            .iter()
            .all(|capability| !capability.available)
    );
    assert_eq!(
        stopped.management_catalog,
        retained_snapshot.management_catalog
    );
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
        grpc_target: Some(String::from("127.0.0.1:17625")),
        ..vulcan_vmm::VmmConfigLayer::default()
    };
    assert_eq!(vmm_deployment_mode(&external), VmmDeploymentMode::External);

    let managed = vulcan_vmm::VmmConfigLayer {
        install: Some(vulcan_vmm::VmmInstallReservation {
            managed_install_enabled: true,
            ..vulcan_vmm::VmmInstallReservation::default()
        }),
        ..vulcan_vmm::VmmConfigLayer::default()
    };
    assert_eq!(vmm_deployment_mode(&managed), VmmDeploymentMode::Managed);
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
