//! Offline VCE readiness against the real registered inference service and bundled model declarations.
//! 使用真实已注册推理服务及内置模型声明的离线 VCE 准备度验证。

/// Exercise real route persistence and publication only below a private task-local data root.
/// 仅在私有任务局部数据根内验证真实路由持久化及发布。
/// No input; verifies stale review rejection, explicit dimensions, peer preservation and no first index.
/// 无输入；验证过期审查拒绝、明确维度、其他消费者保留及不创建首次索引。
#[test]
#[ignore = "requires the bundled local VLDB controller executable"]
fn real_controller_vce_route_save_is_atomic_and_preserves_peer_profiles() {
    use super::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT;
    use super::test_support::TestControllerRuntimeRoot;
    use crate::public_ai_settings::{
        service_preflight_vce_embedding_route, service_save_public_ai_inference_route,
    };
    use futures_util::FutureExt;
    use serde_json::json;
    use vulcan_workbench_model::TaskProjectSummary;
    use vulcan_workspace_store::vce_data_plane::VceDataPlaneFactory;

    // The executor and controller are private; configured credentials cannot dispatch network calls.
    // 执行器及控制器私有；配置凭据无法派发网络调用。
    let executor = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("save test executor");
    let storage = TestControllerRuntimeRoot::new(uuid::Uuid::new_v4());
    executor.block_on(async {
        let (client, controller) = storage.attached_controller_runtime_handle().await;
        let (state, runtime, mut configuration) = configured_state();
        // Seed both actual consumer namespaces so whole-document equality detects collateral edits.
        // 播种两个真实消费者命名空间，使整份文档相等检查能发现附带修改。
        let profile = configuration.inference_profiles[vce::VCE_CONSUMER_PROFILE_ID].clone();
        configuration.inference_profiles.insert(
            crate::configured_inference::VMM_CONSUMER_PROFILE_ID.to_owned(), profile.clone());
        configuration.inference_profiles.insert("knowledge_base:vce-save-fixture".into(), profile);
        runtime.replace_configuration(configuration.clone()).expect("install isolated peers");
        let ai = Arc::clone(&state.lock().expect("state").agent_runtime.as_ref()
            .expect("registered AI").internal_ai_port);
        let factory = VceDataPlaneFactory::bootstrap(&controller).await.expect("private factory");
        let manager = vce::VceRuntimeManager::new(ai, factory.clone(),
            crate::ServiceEventEmitter::new(Vec::new()), vce::vce_publication_authority(&state));
        // An absent source path makes accidental first-index preparation observable.
        // 不存在的源码路径使意外首次索引准备可被观测。
        let source = storage.data_root().join("uninitialized-save-project");
        {
            let mut guard = state.lock().expect("state");
            guard.controller_runtime = Some(controller);
            guard.vce_runtime = Some(manager.clone());
            guard.workspace_snapshot.as_mut().expect("workspace").task_projects = vec![
                TaskProjectSummary::new("save-project", "Save project",
                    source.display().to_string(), false, 0, None, None, Vec::new())
                    .with_vce_settings(vulcan_workbench_model::ProjectVceSettings {
                        enabled: true, ..Default::default()
                    })
            ];
        }
        // Every forward save and compensation uses the same real serializer in this exact private root.
        // 每次正向保存与补偿都在此精确私有根内使用同一真实序列化器。
        let config_root = storage.data_root().join("configuration-save");
        let outcome = std::panic::AssertUnwindSafe(PUBLIC_AI_CONFIGURATION_ROOT.scope(
            config_root.clone(), async {
                let config_path = config_root.join("ai-connections.toml");
                vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                    &configuration, &config_root).expect("persist private before-image");
                let before_bytes = std::fs::read(&config_path).expect("before bytes");
                let candidate = configuration.inference_profiles[vce::VCE_CONSUMER_PROFILE_ID]
                    .embedding.clone().expect("candidate embedding");
                let preflight = service_preflight_vce_embedding_route(state.clone(),
                    serde_json::from_value(json!({"route": candidate, "dimension": 2048}))
                        .expect("preflight request")).await.expect("read-only preflight");
                assert!(preflight.projects.is_empty(), "no initialized projects");
                assert_eq!(std::fs::read(&config_path).expect("after preflight"), before_bytes);
                // Moving the retained project after review must invalidate even an empty origin set.
                // 审查后移动保留项目，即使来源集合为空也必须使确认失效。
                state.lock().expect("state").workspace_snapshot.as_mut().expect("workspace")
                    .task_projects[0].path = storage.data_root().join("moved-save-project")
                    .display().to_string();
                let stale_request = serde_json::from_value(json!({
                    "consumerProfileId": "vce", "operation": AiOperation::EmbeddingCreate,
                    "route": candidate, "embeddingDimensions": 2048,
                    "vceEmbeddingConfirmation": {"preflightRevision": preflight.revision, "decisions": []}
                })).expect("stale save request");
                let before_revision = state.lock().expect("state").workbench.revision;
                let error = service_save_public_ai_inference_route(
                    crate::ServiceEventEmitter::new(Vec::new()), state.clone(), stale_request)
                    .await.err().expect("stale review must fail");
                assert!(error.contains("changed after preflight"), "{error}");
                assert_eq!(std::fs::read(&config_path).expect("rejected bytes"), before_bytes);
                assert_eq!(runtime.configuration_snapshot().expect("rejected runtime"), configuration);
                assert_eq!(state.lock().expect("state").workbench.revision, before_revision);
                // A newly reviewed request commits the explicit dimension with its exact route.
                // 重新审查的请求将明确维度与精确路由一起提交。
                let refreshed = service_preflight_vce_embedding_route(state.clone(),
                    serde_json::from_value(json!({"route": candidate, "dimension": 2048}))
                        .expect("refreshed request")).await.expect("refreshed preflight");
                let save_request = serde_json::from_value(json!({
                    "consumerProfileId": "vce", "operation": AiOperation::EmbeddingCreate,
                    "route": candidate, "embeddingDimensions": 2048,
                    "vceEmbeddingConfirmation": {"preflightRevision": refreshed.revision, "decisions": []}
                })).expect("confirmed save request");
                service_save_public_ai_inference_route(crate::ServiceEventEmitter::new(Vec::new()),
                    state.clone(), save_request).await.expect("real save and workbench publication");
                let mut expected = configuration.clone();
                expected.inference_profiles.get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE profile").embedding_dimensions = Some(2048);
                assert_eq!(runtime.configuration_snapshot().expect("committed runtime"), expected);
                let durable: vulcan_provider_config::config::PublicAiConnectionsFile = toml::from_str(
                    &std::fs::read_to_string(&config_path).expect("committed TOML"))
                    .expect("actual persisted configuration");
                assert_eq!(durable.schema_version, 1);
                assert_eq!(durable.configuration, expected, "all non-VCE configuration must remain identical");
                assert!(state.lock().expect("state").workbench.revision > before_revision);
                assert!(vce::vce_configuration_readiness(&state).status.ready);
                assert!(!vce::vce_project_tool_enabled(&state, "save-project"));
                assert!(manager.project_status("save-project").await.is_none());
                assert!(manager.get_job("save-project", None).await.expect("no job").is_none());
                assert!(!source.exists());
                assert!(!storage.data_root().join("vce/projects").exists());
            }
        )).catch_unwind().await;
        manager.shutdown().await.expect("drain private manager");
        factory.shutdown().await.expect("release private factory");
        client.shutdown().await.expect("release private controller");
        if let Err(panic) = outcome { std::panic::resume_unwind(panic); }
    });
}

use super::test_support::{test_agent_runtime_bridge, test_service_state_with_parent_session};
use crate::{SharedServiceBridgeState, vce};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use vulcan_ai_contract::{AiOperation, InferenceUsageScope};
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, InferenceProfile, InferenceRouteBinding,
    InferenceRouteConfiguration, PersistedAiConfiguration, PublicAiRuntime,
};

/// Use a fixture-only account whose secret resolver rejects access; no request is ever dispatched.
/// 使用秘密解析器拒绝访问的夹具专用账号；绝不派发请求。
const CONNECTION_ID: &str = "vce-readiness-account";
/// Exact bundled service declares both embedding and rerank bindings plus background usage.
/// 精确内置服务声明向量、重排绑定及后台用途。
const SERVICE_ID: &str = "model-studio-cn";

/// Verify both runtime-owned scheduling registrations against the real host registry without inference.
/// 在真实宿主注册表中验证两份运行时调度注册，不执行推理。
/// No inputs; checks shared configuration, revocation, teardown and independent runtime ownership.
/// 无输入；检查共享配置、撤销、拆卸及独立运行时所有权。
#[test]
fn vce_query_registration_shares_routes_and_cleans_both_runtime_handles() {
    use vulcan_context_engine::VceAiPort;
    // Bundled model declarations are real, but the isolated credential store rejects network access.
    // 内置模型声明真实，但隔离凭据存储拒绝网络访问。
    let (state, runtime, configuration) = configured_state();
    // Retain only the registered AI port, releasing the service state before activation.
    // 只保留已注册 AI 端口，在激活前释放服务状态。
    let port = state
        .lock()
        .expect("state")
        .agent_runtime
        .as_ref()
        .expect("bridge")
        .internal_ai_port
        .clone();
    // Stable project identity coexists with distinct runtime capability ownership.
    // 稳定项目身份与不同运行时能力所有权共存。
    let mut config = vce::VceHostAiRegistrationConfig {
        runtime_instance_id: uuid::Uuid::new_v4(),
        project_id: "query-registration-project".into(),
        consumer_profile_id: "vce".into(),
        vector_space_id: "query-registration-space".into(),
        dimension: Some(1024),
        preprocessing_version: "fixture-preprocess".into(),
        chunker_version: "fixture-chunker".into(),
    };
    // An old runtime may still serve readers while its successor is being prepared.
    // 后继运行时准备期间，旧运行时仍可服务读取方。
    let old = vce::VceHostAiAdapter::activate(port.clone(), config.clone()).expect("old runtime");
    config.runtime_instance_id = uuid::Uuid::new_v4();
    // Activation must not replace either capability belonging to the older reader.
    // 激活不得替换属于旧读取方的任何能力。
    let next = vce::VceHostAiAdapter::activate(port.clone(), config).expect("next runtime");
    for adapter in [&old, &next] {
        // Query identity is an explicit part of the host registration contract, not a guessed lookup.
        // 查询身份是宿主注册契约的明确组成部分，不是猜测查找。
        let index = port
            .capability_handle(adapter.registration_id())
            .expect("index grant");
        // These two grants must expose the exact same configured operations and model routes.
        // 两份权限必须暴露精确相同的已配置操作和模型路由。
        let mut query = port
            .capability_handle(&format!("{}:query", adapter.registration_id()))
            .expect("query grant");
        assert_eq!(
            query.caller.scheduling_class,
            vulcan_ai_contract::InferenceSchedulingClass::InternalQuery
        );
        assert_eq!(
            index.caller.scheduling_class,
            vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope
        );
        assert_eq!(query.snapshot, index.snapshot);
        assert_eq!(query.owner_id, index.owner_id);
        query.caller.scheduling_class = vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope;
        assert_eq!(
            query.caller, index.caller,
            "identical route authorization and shared budgets"
        );
        assert!(adapter.capability_snapshot().registered);
    }
    port.unregister_capability(&format!("{}:query", old.registration_id()))
        .expect("revoke old query");
    assert!(!old.capability_snapshot().registered);
    assert!(next.capability_snapshot().registered);
    old.deactivate().expect("clean both old registrations");
    assert!(port.capability_handle(old.registration_id()).is_err());
    assert!(
        port.capability_handle(&format!("{}:query", old.registration_id()))
            .is_err()
    );
    assert!(next.capability_snapshot().registered);
    next.deactivate().expect("clean successor");
    assert!(port.capability_handle(next.registration_id()).is_err());
    assert!(
        port.capability_handle(&format!("{}:query", next.registration_id()))
            .is_err()
    );
    assert_eq!(
        runtime.configuration_snapshot().expect("unchanged models"),
        configuration
    );
}

/// Create a live registered model configuration without filesystem indexes, secrets or network calls.
/// 创建存活已注册模型配置，不使用文件系统索引、秘密或网络调用。
/// Returns the isolated state, its registered public runtime, and the exact admitted configuration.
/// 返回隔离状态、其已注册公共运行时及精确准入配置。
pub(crate) fn configured_state() -> (
    SharedServiceBridgeState,
    Arc<PublicAiRuntime>,
    PersistedAiConfiguration,
) {
    // Bridge creates the production configured-inference service over an isolated credential-rejecting runtime.
    // bridge 在隔离的拒绝凭据运行时之上创建生产已配置推理服务。
    let bridge = test_agent_runtime_bridge();
    let runtime = Arc::clone(&bridge.public_ai_runtime);
    let mut configuration = PersistedAiConfiguration::default();
    let registry = runtime.providers().expect("bundled providers");
    configuration
        .connection_graph
        .add_connection(
            &registry,
            CredentialConnection {
                id: CONNECTION_ID.to_owned(),
                provider_id: "alibaba".to_owned(),
                service_entry_id: SERVICE_ID.to_owned(),
                credential_profile_id: "api-key".to_owned(),
                credential_mode: CredentialMode::Token,
                credential_id: "vce-readiness-credential".to_owned(),
                display_name: "VCE offline readiness fixture".to_owned(),
                secret_reference: "vce-readiness-no-secret".to_owned(),
                management_secret_reference: None,
                service_origin_override: Some(
                    "https://vce-readiness.cn-beijing.maas.aliyuncs.com".to_owned(),
                ),
                prompt_cache_by_protocol: BTreeMap::new(),
            },
        )
        .expect("fixture account admission");
    for model_id in ["qwen3.5-flash", "text-embedding-v4", "qwen3-rerank"] {
        configuration.set_model_enabled("alibaba", model_id, true);
        configuration.set_internal_model_enabled("alibaba", model_id, true);
    }
    runtime
        .replace_configuration(configuration.clone())
        .expect("activate bundled account models");
    // Model-owned protocol selection is resolved from the actual published catalog, never guessed.
    // 模型自有协议选择从实际已发布目录解析，绝不猜测。
    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            CONNECTION_ID,
            "alibaba",
            "qwen3.5-flash",
        )
        .expect("resolve declared conversation model");
    let mut profile = InferenceProfile::default();
    profile.llm_routes.insert(
        vce::VCE_QUERY_REWRITE_PURPOSE_ID.to_owned(),
        route(
            "qwen3.5-flash",
            AiOperation::ConversationRespond,
            InferenceRouteBinding::Protocol {
                protocol_profile_id: selection.protocol_binding.protocol_profile_id,
            },
        ),
    );
    profile.embedding = Some(route(
        "text-embedding-v4",
        AiOperation::EmbeddingCreate,
        InferenceRouteBinding::Action {
            action_binding_id: "alibaba.model_studio.embeddings.v1".to_owned(),
        },
    ));
    profile.embedding_dimensions = Some(1024);
    profile.rerank = Some(route(
        "qwen3-rerank",
        AiOperation::RerankDocuments,
        InferenceRouteBinding::Action {
            action_binding_id: "alibaba.model_studio.rerank.v1".to_owned(),
        },
    ));
    configuration
        .inference_profiles
        .insert(vce::VCE_CONSUMER_PROFILE_ID.to_owned(), profile);
    runtime
        .replace_configuration(configuration.clone())
        .expect("activate VCE routes");
    // State owns the exact bridge consumed by production readiness, with no index manager installed.
    // state 持有生产准备度实际消费的精确桥接，不安装索引管理器。
    let mut state = test_service_state_with_parent_session(uuid::Uuid::new_v4());
    state.agent_runtime = Some(bridge);
    (Arc::new(Mutex::new(state)), runtime, configuration)
}

/// Bind one fixture model and operation to the declared service and background scope.
/// 将一个夹具模型及操作绑定到声明服务和后台范围。
/// model_id/operation/binding select exact authority; returns a complete route without alternative providers.
/// model_id/operation/binding 选择精确权威；返回没有备选供应商的完整路由。
fn route(
    model_id: &str,
    operation: AiOperation,
    binding: InferenceRouteBinding,
) -> InferenceRouteConfiguration {
    InferenceRouteConfiguration {
        provider_id: "alibaba".to_owned(),
        service_entry_id: SERVICE_ID.to_owned(),
        connection_id: CONNECTION_ID.to_owned(),
        model_id: model_id.to_owned(),
        binding,
        operation,
        usage_scope: InferenceUsageScope::InternalBackground,
    }
}

/// Actual published route and account revocations must close readiness without creating indexes or changing peers.
/// 实际已发布路由及账号撤销必须关闭准备度，不创建索引，也不修改其他消费者。
/// Takes no input; restores and revalidates the exact valid configuration between isolated mutation cases.
/// 无输入；在隔离修改场景之间恢复并重新验证精确有效配置。
#[test]
fn vce_registered_model_revocations_close_readiness_without_side_effects() {
    // The same registered runtime is used before, during and after each revocation.
    // 每次撤销前、中、后使用同一个已注册运行时。
    let (state, runtime, valid) = configured_state();
    let ready = vce::vce_configuration_readiness(&state);
    assert!(
        ready.status.ready,
        "initial readiness: {:?}",
        ready.status.issues
    );
    assert_eq!(ready.embedding_dimension, Some(1024));
    assert!(ready.embedding_physical_fingerprint.is_some());
    assert!(
        !vce::vce_project_tool_enabled(&state, "project-a"),
        "models alone must not register project retrieval"
    );
    for case in 0..7 {
        // Each candidate changes exactly one declared prerequisite, leaving all other configuration intact.
        // 每个候选仅改变一个声明前提，保持其他配置完整。
        let mut revoked = valid.clone();
        match case {
            0 => {
                revoked
                    .inference_profiles
                    .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE profile")
                    .embedding = None;
            }
            1 => {
                revoked
                    .inference_profiles
                    .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE profile")
                    .llm_routes
                    .remove(vce::VCE_QUERY_REWRITE_PURPOSE_ID);
            }
            2 => {
                revoked
                    .inference_profiles
                    .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE profile")
                    .rerank = None;
            }
            3 => {
                revoked
                    .inference_profiles
                    .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE profile")
                    .embedding_dimensions = None;
            }
            4 => {
                revoked
                    .inference_profiles
                    .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE profile")
                    .embedding_dimensions = Some(999);
            }
            5 => {
                revoked
                    .set_connection_pending_validation(CONNECTION_ID, true)
                    .expect("mark fixture account pending validation");
            }
            _ => revoked.set_internal_model_enabled("alibaba", "text-embedding-v4", false),
        }
        runtime
            .replace_configuration(revoked.clone())
            .expect("publish exact revocation");
        let unavailable = vce::vce_configuration_readiness(&state);
        assert!(!unavailable.status.ready, "case {case} remained ready");
        assert!(
            !unavailable.status.issues.is_empty(),
            "case {case} must explain its failed prerequisite"
        );
        assert!(!vce::vce_project_tool_enabled(&state, "project-a"));
        assert_eq!(
            runtime.configuration_snapshot().expect("read after status"),
            revoked,
            "readiness cannot rewrite configuration"
        );
        // No manager, watcher or project index is created just because models or their status changed.
        // 不会仅因模型或其状态变化就创建管理器、监听器或项目索引。
        let guard = state.lock().expect("fixture state");
        assert!(guard.vce_runtime.is_none());
        assert!(guard.vce_watcher.is_none());
        drop(guard);
        runtime
            .replace_configuration(valid.clone())
            .expect("restore exact routes");
        assert!(
            vce::vce_configuration_readiness(&state).status.ready,
            "case {case} cannot recover"
        );
    }
}

/// Verify live model and project revocation against the real tool callback after a committed empty index.
/// 在提交空索引后，通过真实工具回调验证实时模型和项目撤销。
/// Takes no input; uses isolated controller storage and rejects all credential access, so no billed calls occur.
/// 无输入；使用隔离控制器存储并拒绝所有凭据访问，因此没有计费调用。
#[test]
#[ignore = "requires the bundled local VLDB controller executable"]
fn real_controller_registered_configuration_revokes_existing_index_access() {
    use super::test_support::TestControllerRuntimeRoot;
    use futures_util::FutureExt;
    use std::time::Duration;
    use vulcan_workbench_model::{ProjectVceSettings, TaskProjectSummary};
    use vulcan_workspace_store::vce_data_plane::{VceDataPlaneFactory, VceJobKind, VceJobState};

    // Executor and storage root belong only to this test, not to a running user service.
    // 执行器与存储根仅属于此测试，不属于正在运行的用户服务。
    let executor = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("isolated configuration-revocation runtime"),
    );
    let storage = TestControllerRuntimeRoot::new(uuid::Uuid::new_v4());
    executor.block_on(async {
        let (client, controller) = storage.attached_controller_runtime_handle().await;
        // Retire the managed controller even if a production gate or assertion fails.
        // 即使生产门禁或断言失败，也退出托管控制器。
        let outcome = std::panic::AssertUnwindSafe(async {
            let source = storage.data_root().join("empty-vce-project");
            std::fs::create_dir_all(&source).expect("isolated empty source root");
            let (state, runtime, valid) = configured_state();
            let settings = ProjectVceSettings { enabled: true, ..ProjectVceSettings::default() };
            let project = TaskProjectSummary::new("project-a", "VCE revocation fixture",
                source.display().to_string(), false, 0, None, None, Vec::new())
                .with_vce_settings(settings.clone());
            let ai = {
                let mut guard = state.lock().expect("fixture state");
                guard.workspace_snapshot.as_mut().expect("workspace").task_projects.push(project.clone());
                Arc::clone(&guard.agent_runtime.as_ref().expect("registered bridge").internal_ai_port)
            };
            let fingerprint = vce::vce_configuration_readiness(&state)
                .embedding_physical_fingerprint.expect("ready registered embedding identity");
            let factory = VceDataPlaneFactory::bootstrap(&controller).await.expect("private VCE factory");
            let manager = vce::VceRuntimeManager::new(ai, factory,
                crate::ServiceEventEmitter::new(Vec::new()), vce::vce_publication_authority(&state));
            state.lock().expect("fixture state").vce_runtime = Some(manager.clone());
            manager.enable_project("project-a", &source, settings.clone(), 1024, &fingerprint, true)
                .await.expect("prepare enabled project without indexing");
            // Retained callbacks must recheck current authority rather than their creation-time state.
            // 保留的回调必须复查当前权威，而不是使用创建时状态。
            let handler = vce::vce_retrieval_handler(state.clone(), manager.clone());
            assert!(!vce::vce_project_tool_enabled(&state, "project-a"));
            assert!(handler.retrieve("project-a".into(), "find cache code".into()).await.is_err());
            let initial = manager.submit_job("project-a", vce::VceJobAdmission {
                retry_of_job_id: None, kind: VceJobKind::Initialize, idempotency_key: "revocation-initial".into(),
                expected_settings_revision: settings.revision, expected_checkpoint_version: None,
                contract_fingerprint: fingerprint.clone(),
            }, &executor).await.expect("explicit initialization");
            let completed = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let receipt = manager.get_job("project-a", Some(&initial.job_id)).await
                        .expect("read initialization").expect("durable initial job");
                    if receipt.state.is_terminal() { break receipt; }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }).await.expect("empty initialization deadline");
            assert_eq!(completed.state, VceJobState::Succeeded, "initial diagnostic: {completed:?}");
            assert!(vce::vce_project_tool_enabled(&state, "project-a"));
            let baseline: vulcan_service_api::VceRetrievalResponse = serde_json::from_value(
                handler.retrieve("project-a".into(), "find cache code".into()).await
                    .expect("retrieve initialized empty scope through real callback"))
                    .expect("typed retrieval response");
            assert!(baseline.hits.is_empty());
            for case in 0..5 {
                // Only the registered runtime is replaced; the callback, manager and published index remain unchanged.
                // 仅替换已注册运行时配置；回调、管理器及已发布索引保持不变。
                let mut revoked = valid.clone();
                let profile = revoked.inference_profiles.get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE profile");
                match case {
                    0 => { profile.llm_routes.remove(vce::VCE_QUERY_REWRITE_PURPOSE_ID); }
                    1 => profile.embedding = None,
                    2 => profile.rerank = None,
                    3 => profile.embedding_dimensions = Some(2048),
                    _ => revoked.set_connection_pending_validation(CONNECTION_ID, true)
                        .expect("revoke configured account"),
                }
                runtime.replace_configuration(revoked).expect("publish runtime revocation");
                if case == 3 {
                    assert!(vce::vce_configuration_readiness(&state).status.ready,
                        "supported new dimension is globally valid but not compatible with this index");
                }
                assert!(!vce::vce_project_tool_enabled(&state, "project-a"), "case {case} tool gate");
                assert!(handler.retrieve("project-a".into(), "find cache code".into()).await.is_err(),
                    "case {case} retained callback must reject");
                // Stored physical maintenance remains readable while model authority is revoked.
                // 模型权威撤销期间，已存物理维护仍必须可读取。
                let compact_plan = vce::service_plan_vce_maintenance(
                    state.clone(),
                    vulcan_service_api::VceMaintenancePlanRequest {
                        project_id: String::from("project-a"),
                        mode: String::from("compact"),
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("case {case} stored compaction plan: {error}"));
                assert_eq!(compact_plan.mode, "compact");
                assert_eq!(compact_plan.checkpoint_version, Some(baseline.checkpoint_version));
                runtime.replace_configuration(valid.clone()).expect("restore exact model routes");
                assert!(vce::vce_project_tool_enabled(&state, "project-a"), "case {case} recovery");
                let recovered: vulcan_service_api::VceRetrievalResponse = serde_json::from_value(
                    handler.retrieve("project-a".into(), "find cache code".into()).await
                        .expect("existing index recovers without rebuilding"))
                        .expect("typed recovered response");
                assert_eq!(recovered.checkpoint_version, baseline.checkpoint_version);
            }
            for case in 0..3 {
                // These are exact current workspace facts, not delayed manager reconciliation.
                // 这些是当前工作区的精确事实，而不是延迟执行的管理器协调。
                {
                    let mut guard = state.lock().expect("fixture state");
                    let projects = &mut guard.workspace_snapshot.as_mut().expect("workspace").task_projects;
                    match case {
                        0 => projects[0].vce.enabled = false,
                        1 => projects[0].path = storage.data_root().display().to_string(),
                        _ => projects.clear(),
                    }
                }
                assert!(!vce::vce_project_tool_enabled(&state, "project-a"), "project case {case}");
                assert!(handler.retrieve("project-a".into(), "find cache code".into()).await.is_err());
                state.lock().expect("fixture state").workspace_snapshot.as_mut()
                    .expect("workspace").task_projects = vec![project.clone()];
                assert!(vce::vce_project_tool_enabled(&state, "project-a"));
            }
            assert_eq!(manager.get_job("project-a", None).await.expect("latest job")
                .expect("initial remains latest").job_id, initial.job_id,
                "status and model/project changes must not create another index job");
            assert!(state.lock().expect("fixture state").vce_watcher.is_none());
            manager.shutdown().await.expect("drain isolated VCE runtime");
        }).catch_unwind().await;
        client.shutdown().await.expect("release isolated controller");
        if let Err(panic) = outcome { std::panic::resume_unwind(panic); }
    });
}

/// Verify a hundred enabled cold projects remain metadata-only until one explicit initialization.
/// 验证一百个启用冷态项目在其中一个被明确初始化前，始终只读取元数据。
/// No input; uses a real isolated controller/watcher and credential-rejecting registered AI, never user data.
/// 无输入；使用真实隔离控制器、监听器和拒绝凭据的注册模型，不使用用户数据。
#[test]
#[ignore = "requires the bundled local VLDB controller executable"]
fn real_controller_hundred_cold_projects_only_initialize_explicit_target() {
    use super::test_support::TestControllerRuntimeRoot;
    use futures_util::FutureExt;
    use std::time::Duration;
    use vulcan_service_api::{VceProjectLifecycleState, VceProjectRequest};
    use vulcan_workbench_model::{ProjectVceSettings, ProjectVceUpdateMode, TaskProjectSummary};
    use vulcan_workspace_store::vce_data_plane::{VceDataPlaneFactory, VceJobKind, VceJobState};

    // Only this bounded executor owns the fixture jobs and watcher subscription.
    // 仅此有界执行器持有夹具作业与监听订阅。
    let executor = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("cold-project executor"),
    );
    // This generic session/controller fixture is independent of all installed application roots.
    // 此通用会话及控制器夹具独立于全部已安装应用数据根。
    let storage = TestControllerRuntimeRoot::new(uuid::Uuid::new_v4());
    executor.block_on(async {
        let (client, controller) = storage.attached_controller_runtime_handle().await;
        let (state, runtime, configuration) = configured_state();
        let ai = Arc::clone(
            &state
                .lock()
                .expect("fixture state")
                .agent_runtime
                .as_ref()
                .expect("registered AI bridge")
                .internal_ai_port,
        );
        let factory = VceDataPlaneFactory::bootstrap(&controller)
            .await
            .expect("cold VCE factory");
        let manager = vce::VceRuntimeManager::new(
            ai,
            factory.clone(),
            crate::ServiceEventEmitter::new(Vec::new()),
            vce::vce_publication_authority(&state),
        );
        let center =
            crate::workspace_watcher::WorkspaceWatcherCenter::new().expect("physical watcher");
        let watcher = vce::VceWatcherService::new(
            &center,
            manager.clone(),
            crate::ServiceEventEmitter::new(Vec::new()),
            executor.clone(),
        );
        // Automatic policy alone is intentionally insufficient to create any initial index or source watch.
        // 自动策略本身故意不足以创建任何初始索引或源码监听。
        let settings = ProjectVceSettings {
            enabled: true,
            update_mode: ProjectVceUpdateMode::Automatic,
            ..ProjectVceSettings::default()
        };
        let projects = (0..100)
            .map(|index| {
                TaskProjectSummary::new(
                    format!("cold-project-{index}"),
                    format!("Cold project {index}"),
                    storage
                        .data_root()
                        .join(format!("source-{index}"))
                        .display()
                        .to_string(),
                    false,
                    index,
                    None,
                    None,
                    Vec::new(),
                )
                .with_vce_settings(settings.clone())
            })
            .collect::<Vec<_>>();
        {
            let mut guard = state.lock().expect("fixture state");
            guard.controller_runtime = Some(controller.clone());
            guard.vce_runtime = Some(manager.clone());
            guard.vce_watcher = Some(watcher.clone());
            guard
                .workspace_snapshot
                .as_mut()
                .expect("workspace")
                .task_projects = projects.clone();
        }
        // Retain cleanup ownership outside the assertion future so failures cannot leave worker subscriptions alive.
        // 在断言协程外保留清理所有权，避免失败遗留工作订阅。
        let outcome = std::panic::AssertUnwindSafe(async {
            assert!(
                vce::service_get_vce_configuration_status(state.clone())
                    .expect("global status")
                    .ready
            );
            assert!(!storage.data_root().join("vce/projects").exists());
            for _round in 0..2 {
                for project in &projects {
                    let request = VceProjectRequest {
                        project_id: project.id.clone(),
                    };
                    let activity =
                        vce::service_get_vce_project_activity(state.clone(), request.clone())
                            .await
                            .expect("cold activity is a metadata-only read");
                    let overview = vce::service_get_vce_project_overview(state.clone(), request)
                        .await
                        .expect("cold overview is a metadata-only read");
                    assert_eq!(
                        activity.status.lifecycle,
                        VceProjectLifecycleState::Uninitialized
                    );
                    assert_eq!(
                        overview.status.lifecycle,
                        VceProjectLifecycleState::Uninitialized
                    );
                    assert!(!activity.query_available);
                    assert!(!activity.source_freshness.watching);
                    assert!(activity.last_successful_update_unix_ms.is_none());
                    assert!(manager.project_status(&project.id).await.is_none());
                    assert!(
                        manager
                            .get_job(&project.id, None)
                            .await
                            .expect("no cold job")
                            .is_none()
                    );
                    assert!(!vce::vce_project_tool_enabled(&state, &project.id));
                    assert!(
                        !std::path::Path::new(&project.path).exists(),
                        "status cannot prepare or scan absent sources"
                    );
                }
            }
            assert!(
                !storage.data_root().join("vce/projects").exists(),
                "all cold reads must avoid project databases"
            );
            // Simulate the one human-selected source becoming available; only this empty project is initialized.
            // 模拟用户唯一选择的源码目录变为可用；仅初始化这个空项目。
            let selected = &projects[0];
            let source = std::path::Path::new(&selected.path);
            std::fs::create_dir_all(source).expect("create human-selected empty source");
            let fingerprint = vce::vce_configuration_readiness(&state)
                .embedding_physical_fingerprint
                .expect("explicit registered vector identity");
            manager
                .enable_project(
                    &selected.id,
                    source,
                    settings.clone(),
                    1024,
                    &fingerprint,
                    true,
                )
                .await
                .expect("prepare explicitly selected project");
            let admitted = manager
                .submit_job(
                    &selected.id,
                    vce::VceJobAdmission {
                        retry_of_job_id: None,
                        kind: VceJobKind::Initialize,
                        idempotency_key: "human-selected-only".into(),
                        expected_settings_revision: settings.revision,
                        expected_checkpoint_version: None,
                        contract_fingerprint: fingerprint,
                    },
                    &executor,
                )
                .await
                .expect("human initialization");
            let terminal = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let receipt = manager
                        .get_job(&selected.id, Some(&admitted.job_id))
                        .await
                        .expect("read selected job")
                        .expect("selected receipt");
                    if receipt.state.is_terminal() {
                        break receipt;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("empty selected project completion");
            assert_eq!(terminal.state, VceJobState::Succeeded, "{terminal:?}");
            assert!(vce::vce_project_tool_enabled(&state, &selected.id));
            for project in projects.iter().skip(1) {
                let activity = vce::service_get_vce_project_activity(
                    state.clone(),
                    VceProjectRequest {
                        project_id: project.id.clone(),
                    },
                )
                .await
                .expect("unselected project still cold");
                assert_eq!(
                    activity.status.lifecycle,
                    VceProjectLifecycleState::Uninitialized
                );
                assert!(!activity.source_freshness.watching);
                assert!(manager.project_status(&project.id).await.is_none());
                assert!(
                    factory
                        .load_active_project_config(&project.id)
                        .await
                        .expect("unselected catalog")
                        .is_none()
                );
            }
            assert_eq!(
                std::fs::read_dir(storage.data_root().join("vce/projects"))
                    .expect("selected physical project directory")
                    .collect::<Result<Vec<_>, _>>()
                    .expect("read physical directory entries")
                    .len(),
                1
            );
            assert_eq!(
                runtime
                    .configuration_snapshot()
                    .expect("retained runtime configuration"),
                configuration
            );
        })
        .catch_unwind()
        .await;
        watcher.shutdown().await;
        manager
            .shutdown()
            .await
            .expect("drain selected index runtime");
        factory
            .shutdown()
            .await
            .expect("release private data plane");
        client.shutdown().await.expect("release private controller");
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    });
}
