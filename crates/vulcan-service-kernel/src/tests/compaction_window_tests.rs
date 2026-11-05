//! Persisted compaction selection through cold navigation, submission hydration and idempotent reselection.
//! 持久压缩选择经过冷导航、提交水合及幂等重选的链路测试。

use super::*;
use crate::session_goal_lifecycle::session_v1_handle_from_state;
use vulcan_context::{ContextCompactionWindowNodeId, ContextWindowSelection};

/// The fourth fixed node as a session window selection.
/// 作为会话窗口选择的第四个固定节点。
const NODE4: ContextWindowSelection =
    ContextWindowSelection::Node(ContextCompactionWindowNodeId::CompressionWindow4);
use vulcan_provider_adapters::{PersistedAiConfiguration, PublicAiRuntime};

/// Builds an isolated configured runtime with a one-million-token model; returns no network activity.
/// 构建含百万 Token 模型的隔离已配置运行时，不产生网络活动。
fn configured_runtime() -> PublicAiRuntime {
    use vulcan_provider_adapters::{
        CredentialConnection, CredentialMode, UnavailableCredentialSecretResolver,
    };
    // Credentials are unavailable; these tests exercise configuration and never dispatch.
    // 凭证不可用；这些测试只验证配置，绝不分派。
    let runtime =
        PublicAiRuntime::first_batch(Arc::new(UnavailableCredentialSecretResolver)).unwrap();
    // This in-memory configuration is never written to the user's application root.
    // 此内存配置绝不写入用户应用根目录。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &runtime.providers().unwrap(),
            CredentialConnection {
                id: "compaction-account".into(),
                provider_id: "openrouter".into(),
                service_entry_id: "public-api".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: CredentialMode::Token,
                credential_id: "fixture-credential".into(),
                display_name: "Fixture".into(),
                secret_reference: "unavailable-fixture".into(),
                management_secret_reference: None,
                service_origin_override: None,
            },
        )
        .unwrap();
    configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
    configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
        provider_id: "openrouter".into(),
        model_id: "openai/gpt-5.6-sol".into(),
        connection_id: "compaction-account".into(),
    });
    // Publish exact protocol variants so selection uses the same route contract as production.
    // 发布精确协议变体，使选择使用与生产相同的路由契约。
    let mut catalog = vulcan_provider_adapters::parse_openrouter_model_catalog("compaction-account", 1,
        r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"Fixture","context_length":1000000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#).unwrap();
    for model in &mut catalog.models {
        model.protocol_variants.insert(
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID.into(),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities: model.capabilities.clone(),
                capability_evidence: Default::default(),
                discovery_signature: "fixture-signature".into(),
                catalog_generation: catalog.generation,
            },
        );
    }
    runtime.stage_source_catalog_for_testing(catalog).unwrap();
    runtime
        .replace_configuration(configuration.clone())
        .unwrap();
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .unwrap();
    runtime
}

/// Creates a retained service for `session_id` and `controller`; returns an empty process registry.
/// 为 `session_id` 与 `controller` 创建保留服务，返回空进程注册表。
fn cold_state(session_id: SessionId, controller: HostRuntimeHandle) -> SharedServiceBridgeState {
    // The workbench has an index row but no hydrated runtime or addon state after restart.
    // 重启后工作台存在索引行，但没有水合运行态或附属状态。
    let mut service = test_service_state_with_parent_session(session_id);
    service.controller_runtime = Some(controller);
    service
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .task_projects
        .push(TaskProjectSummary::new(
            "project-a",
            "Fixture",
            "D:/fixture",
            false,
            0,
            None,
            None,
            Vec::new(),
        ));
    service.workbench.conversations.push(ConversationSummary {
        id: session_id.to_string(),
        title: "Fixture".into(),
        status: SessionState::Completed.label().into(),
        run_phase: SessionRunPhase::Ready.to_string(),
        last_updated_label: "now".into(),
        last_updated_at: "1".into(),
        created_at: "1".into(),
        last_request_at: "1".into(),
        manual_position: None,
        unread: false,
        attention: None,
        project_id: Some("project-a".into()),
        dedicated: false,
        assistant_name: None,
    });
    // The production bridge uses a fixture-only model catalog for picker validation.
    // 生产桥使用仅供夹具的模型目录校验选择器。
    let mut agent = test_agent_runtime_bridge();
    agent.public_ai_runtime = Arc::new(configured_runtime());
    service.agent_runtime = Some(agent);
    Arc::new(Mutex::new(service))
}

/// Exercises save, restart, retained UI, stale-state submission and same-value reselection as one chain.
/// 将保存、重启、保留界面、陈旧状态提交及同值重选作为一条链路验证。
/// Takes no arguments; returns no value and writes only an isolated test session.
/// 无参数、无返回值，只写入隔离测试会话。
#[tokio::test]
async fn compaction_window_survives_cold_navigation_submission_and_same_value_reselection() {
    // This exact session uses the real indexed SQLite locator and segmented persistence owner.
    // 此精确会话使用真实 SQLite 索引定位器及分片持久化所有者。
    let session_id = SessionId::new_v4();
    // The fixture guard owns every file created by this test.
    // 夹具守卫拥有此测试创建的全部文件。
    let fixture = TestHostRuntimeRoot::new(session_id);
    // Attach once, then retain the same storage across simulated process restarts.
    // 仅附着一次，随后跨模拟进程重启保留同一存储。
    let controller = fixture.attached_host_runtime_handle().await;
    // The first process sets the real session control through the production service.
    // 第一个进程经生产服务设置真实会话控制项。
    let state = cold_state(session_id, controller.clone());
    // The emitter has no external subscriber or network sink.
    // 发射器没有外部订阅者或网络接收端。
    let app = ServiceEventEmitter::new(Vec::new());
    {
        // Bind the model through real user admission; cold sessions never borrow the global picker implicitly.
        // 经真实用户准入绑定模型；冷会话绝不隐式借用全局选择器。
        let handle = session_v1_handle_from_state(&state, session_id)
            .await
            .unwrap();
        handle
            .submit_root_input(
                vulcan_session_core::v1::ClientInputSubmission::new(
                    vulcan_session_core::v1::ClientActionId::try_new("compaction-window-root")
                        .unwrap(),
                    "fixture prompt",
                    Vec::new(),
                )
                .with_bound_request_config(
                    vulcan_session_core::v1::config::ActiveRequestConfig {
                        provider: Some("openrouter".into()),
                        connection_id: Some("compaction-account".into()),
                        model: Some("openai/gpt-5.6-sol".into()),
                        ..Default::default()
                    },
                ),
                2,
            )
            .unwrap();
        service_select_conversation(state.clone(), session_id.to_string())
            .await
            .unwrap();
    }
    service_set_session_context_compaction_window(
        app.clone(),
        state.clone(),
        SessionContextCompactionWindowUpdateRequest {
            conversation_id: session_id.to_string(),
            selection: Some(NODE4),
        },
    )
    .await
    .unwrap();
    drop(state);
    // Capture durable bytes to prove cold navigation and same-value writes do not rewrite metadata.
    // 捕获持久字节，证明冷导航及同值写入不会重写元数据。
    let metadata_path = fixture.v1_session_directory().join("metadata.json");
    // This restarted process begins with no resident session owner.
    // 此重启后的进程初始没有常驻会话所有者。
    let state = cold_state(session_id, controller.clone());
    // The original bytes include the explicit saved node.
    // 原始字节包含显式保存的节点。
    let saved = fs::read(&metadata_path).unwrap();
    // Selection must project both the top-level field and the retained frontend addon row.
    // 选择必须同时投影顶层字段及前端保留附属行。
    let selected = service_select_conversation(state.clone(), session_id.to_string())
        .await
        .unwrap();
    assert_eq!(selected.session_context_window_selection, Some(NODE4));
    assert_eq!(
        selected
            .session_addons_by_conversation
            .iter()
            .find(|row| row.conversation_id == session_id.to_string())
            .unwrap()
            .session_context_window_selection,
        Some(NODE4)
    );
    assert!(
        state
            .lock()
            .unwrap()
            .session_runtime_registry
            .get(session_id)
            .is_err()
    );
    assert_eq!(fs::read(&metadata_path).unwrap(), saved);

    // Simulate the stale metadata-only row observed in the failing submission, retaining a volatile review mode.
    // 模拟失败提交中观察到的陈旧纯元数据行，同时保留易失的审核模式。
    let mut workspace = state.lock().unwrap().workspace_snapshot.clone().unwrap();
    workspace
        .session_runtime_state
        .get_mut(&session_id)
        .unwrap()
        .context_window_selection = None;
    workspace
        .session_runtime_state
        .get_mut(&session_id)
        .unwrap()
        .active_work_mode = Some("review".into());
    // Production capture preserves precisely this stale row before submission hydration.
    // 生产捕获在提交水合前精确保留此陈旧行。
    let preserved =
        capture_preserved_workspace_run_state(Some(&workspace), &BTreeSet::new(), Some(session_id));
    hydrate_retained_submission_session_if_needed(
        &state,
        controller.clone(),
        &mut workspace,
        session_id,
        preserved.as_ref(),
    )
    .await
    .unwrap();
    assert_eq!(
        workspace.session_runtime_state[&session_id].context_window_selection,
        Some(NODE4)
    );
    assert_eq!(
        workspace.session_runtime_state[&session_id]
            .active_work_mode
            .as_deref(),
        Some("review")
    );
    // The same owner is used by normal dispatch and Continue preparation.
    // 普通分派与 Continue 准备使用同一所有者。
    let handle = state
        .lock()
        .unwrap()
        .session_runtime_registry
        .get(session_id)
        .unwrap();
    // Even a retained retry that bypasses hydration must reacquire the node at dispatch.
    // 即使保留态重试绕过水合，也必须在分派时重新取得节点。
    let runtime = state
        .lock()
        .unwrap()
        .agent_runtime
        .as_ref()
        .unwrap()
        .public_ai_runtime
        .clone();
    workspace
        .session_runtime_state
        .get_mut(&session_id)
        .unwrap()
        .context_window_selection = None;
    crate::session_request_config_v1::synchronize_v1_dispatch_model_selection(
        &handle,
        &runtime,
        workspace
            .session_runtime_state
            .get_mut(&session_id)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        context_compaction_window_limit_from_configuration(
            Some(&workspace.session_runtime_state[&session_id]),
            Some(&runtime.configuration_snapshot().unwrap())
        ),
        Some(373000)
    );
    state.lock().unwrap().workspace_snapshot = Some(workspace);

    for node in [Some(NODE4), None] {
        // First establish the desired durable choice; None explicitly restores the default policy.
        // 先建立目标持久选择；None 显式恢复默认策略。
        service_set_session_context_compaction_window(
            app.clone(),
            state.clone(),
            SessionContextCompactionWindowUpdateRequest {
                conversation_id: session_id.to_string(),
                selection: node,
            },
        )
        .await
        .unwrap();
        // A successful no-op must repair either stale direction without advancing the document revision.
        // 成功的空操作必须修复两个方向的陈旧状态，且不推进文档修订。
        let before = fs::read(&metadata_path).unwrap();
        // Revision is checked in addition to raw bytes so an unflushed duplicate mutation is detected too.
        // 除原始字节外也检查修订，以检测尚未刷盘的重复修改。
        let revision = handle.read(|document| document.memory_revision()).unwrap();
        {
            // Corrupt only the process mirror; durable authority remains valid and unchanged.
            // 仅破坏进程镜像；持久权威仍有效且不变。
            let mut guard = state.lock().unwrap();
            // This exact runtime entry owns the stale node and derived fingerprints.
            // 此精确运行态条目拥有陈旧节点及派生指纹。
            let mirror = guard
                .workspace_snapshot
                .as_mut()
                .unwrap()
                .session_runtime_state
                .get_mut(&session_id)
                .unwrap();
            mirror.context_window_selection = if node.is_some() { None } else { Some(NODE4) };
            mirror.live_context_budget_fingerprint = Some("stale-live".into());
            mirror.anchor_context_budget_fingerprint = Some("stale-anchor".into());
        }
        // Reselect through the same service RPC used by WebUI/TUI.
        // 通过 WebUI/TUI 使用的同一服务 RPC 重选。
        let response = service_set_session_context_compaction_window(
            app.clone(),
            state.clone(),
            SessionContextCompactionWindowUpdateRequest {
                conversation_id: session_id.to_string(),
                selection: node,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.session_context_window_selection, node);
        assert_eq!(fs::read(&metadata_path).unwrap(), before);
        assert_eq!(
            handle.read(|document| document.memory_revision()).unwrap(),
            revision
        );
        // The repaired mirror must invalidate fingerprints derived from the incorrect window.
        // 修复后的镜像必须使根据错误窗口派生的指纹失效。
        let guard = state.lock().unwrap();
        // Observe only the exact session, avoiding accidental global-control assertions.
        // 仅观察精确会话，避免误断言全局控制项。
        let mirror = &guard
            .workspace_snapshot
            .as_ref()
            .unwrap()
            .session_runtime_state[&session_id];
        assert_eq!(mirror.context_window_selection, node);
        assert!(mirror.live_context_budget_fingerprint.is_none());
        assert!(mirror.anchor_context_budget_fingerprint.is_none());
    }
    // Reopening an explicitly defaulted session must not revive its old explicit node.
    // 重新打开已显式恢复默认的会话时，绝不能复活旧显式节点。
    let reopened = cold_state(session_id, controller);
    assert!(
        service_select_conversation(reopened, session_id.to_string())
            .await
            .unwrap()
            .session_context_window_selection
            .is_none()
    );
}
