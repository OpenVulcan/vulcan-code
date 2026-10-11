//! Real saved embedding decisions against two independently committed project indexes.
//! 针对两个独立已提交项目索引的真实向量决定保存验收。

use super::public_ai_persistence_scope::PUBLIC_AI_CONFIGURATION_ROOT;
use super::test_support::TestHostRuntimeRoot;
use super::vce_configuration_readiness_tests::configured_state;
use crate::public_ai_settings::{
    PublicAiInferenceRouteSaveRequest, service_preflight_vce_embedding_route,
    service_save_public_ai_inference_route,
};
use crate::{ServiceEventEmitter, SharedServiceBridgeState, vce};
use futures_util::FutureExt;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use vulcan_ai_contract::AiOperation;
use vulcan_provider_adapters::{
    ContextEngineEmbeddingDecision, InferenceRouteConfiguration, PersistedAiConfiguration,
};
use vulcan_service_api::VceProjectRequest;
use vulcan_workbench_model::{ProjectVceSettings, TaskProjectSummary};
use vulcan_workspace_store::vce_data_plane::{
    VceDataPlaneFactory, VceJobKind, VceJobState, VceStoredJob,
};

/// Build the actual save RPC input from one reviewed candidate and exact per-project decisions.
/// 根据一个已审查候选及精确逐项目决定构建实际保存 RPC 输入。
/// Route/dimension/revision identify the draft; decisions identify human choices. Returns typed input.
/// 路由、维度、修订标识草稿；决定标识人为选择。返回强类型输入。
fn confirmed_embedding_request(
    route: &InferenceRouteConfiguration,
    dimension: u32,
    revision: &str,
    decisions: &[(&str, ContextEngineEmbeddingDecision)],
) -> PublicAiInferenceRouteSaveRequest {
    serde_json::from_value(json!({
        "consumerProfileId": vce::VCE_CONSUMER_PROFILE_ID,
        "operation": AiOperation::EmbeddingCreate,
        "route": route,
        "embeddingDimensions": dimension,
        "vceEmbeddingConfirmation": {
            "preflightRevision": revision,
            "decisions": decisions.iter().map(|(project_id, decision)| json!({
                "projectId": project_id, "decision": decision
            })).collect::<Vec<_>>()
        }
    }))
    .expect("typed embedding save request")
}

/// Wait for one already-admitted private job without starting or retrying another operation.
/// 等待一个已准入的私有作业，不启动或重试其他操作。
/// Manager/project/job identify an exact durable receipt. Returns its successful terminal receipt.
/// 管理器、项目、作业标识精确持久回执。返回成功终态回执。
async fn successful_job(
    manager: &vce::VceRuntimeManager,
    project_id: &str,
    job_id: &str,
) -> VceStoredJob {
    // Bounded polling checks the same stable job identity throughout the test.
    // 有界轮询在整个测试中检查同一个稳定作业身份。
    let receipt = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            // Only the exact accepted receipt can satisfy completion.
            // 只有精确已受理回执能够满足完成条件。
            let receipt = manager
                .get_job(project_id, Some(job_id))
                .await
                .expect("read admitted job")
                .expect("durable admitted receipt");
            if receipt.state.is_terminal() {
                break receipt;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("private empty-index job deadline");
    assert_eq!(receipt.state, VceJobState::Succeeded, "{receipt:?}");
    receipt
}

/// Check the actual persisted envelope and all consumers against the exact runtime image.
/// 对照精确运行时镜像检查实际持久化外层文档及全部消费者。
/// Path owns the private TOML; expected is the complete accepted configuration. Returns no value.
/// 路径持有私有 TOML；期望值是完整已接受配置。无返回值。
fn assert_durable_configuration(path: &std::path::Path, expected: &PersistedAiConfiguration) {
    // The file envelope is distinct from its inner provider configuration.
    // 文件外层文档与内部供应商配置不同。
    let document: vulcan_provider_config::config::PublicAiConnectionsFile =
        toml::from_str(&std::fs::read_to_string(path).expect("private TOML bytes"))
            .expect("real configuration envelope");
    assert_eq!(document.schema_version, 1);
    assert_eq!(&document.configuration, expected);
}

/// Reject one invalid save without changing any durable/runtime/presentation before-image.
/// 拒绝一次无效保存，不改变任何持久化、运行时或展示前镜像。
/// State/path own isolated resources; request is the attempted save; reason identifies its exact failure.
/// 状态、路径持有隔离资源；请求为待尝试保存；原因标识精确失败。无返回值。
async fn assert_save_rejected(
    state: &SharedServiceBridgeState,
    path: &std::path::Path,
    request: PublicAiInferenceRouteSaveRequest,
    reason: &str,
) {
    // Freeze all three externally observable boundaries before the real service call.
    // 在真实服务调用前冻结三个外部可见边界。
    let before_bytes = std::fs::read(path).expect("private before-image");
    let (runtime, before_revision) = {
        let guard = state.lock().expect("state");
        (
            Arc::clone(&guard.agent_runtime.as_ref().expect("AI").public_ai_runtime),
            guard.workbench.revision,
        )
    };
    let before_configuration = runtime
        .configuration_snapshot()
        .expect("runtime before-image");
    let error = service_save_public_ai_inference_route(
        ServiceEventEmitter::new(Vec::new()),
        state.clone(),
        request,
    )
    .await
    .expect_err("invalid save must fail");
    assert!(error.contains(reason), "expected {reason:?}: {error}");
    assert_eq!(
        std::fs::read(path).expect("private after-image"),
        before_bytes
    );
    assert_eq!(
        runtime
            .configuration_snapshot()
            .expect("runtime after-image"),
        before_configuration
    );
    assert_eq!(
        state.lock().expect("state").workbench.revision,
        before_revision
    );
}

/// Verify indexed-project decisions through real storage, preflight, save and project activation.
/// 通过真实存储、预检、保存及项目激活验证已有索引的逐项目决定。
/// No input; uses two empty sources and credential-rejecting AI, with no user files or paid calls.
/// 无输入；使用两个空源码目录及拒绝凭据的 AI，不访问用户文件或计费调用。
#[test]
fn real_host_storage_embedding_decisions_are_project_scoped_and_revision_checked() {
    // Private bounded executor owns all index jobs and storage lifecycle.
    // 私有有界执行器持有所有索引作业及存储生命周期。
    let executor = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("decision executor"),
    );
    let storage = TestHostRuntimeRoot::new(uuid::Uuid::new_v4());
    executor.block_on(async {
        let controller = storage.attached_host_runtime_handle().await;
        let (state, runtime, mut configuration) = configured_state();
        // The second account serves the identical named model but is a distinct execution identity.
        // 第二个账号提供名称完全相同的模型，但属于不同执行身份。
        let original_route = configuration.inference_profiles[vce::VCE_CONSUMER_PROFILE_ID]
            .embedding
            .clone()
            .expect("original embedding");
        let mut second_account = configuration
            .connection_graph
            .connections
            .get(&original_route.connection_id)
            .expect("fixture account")
            .clone();
        second_account.id = "vce-confirmation-second-account".into();
        configuration
            .connection_graph
            .add_connection(
                &runtime.providers().expect("actual declarations"),
                second_account,
            )
            .expect("second account admission");
        // Preserve peer profiles byte-for-byte through every VCE save below.
        // 在以下每次 VCE 保存中逐字节保留其他消费者配置。
        let peer_profile = configuration.inference_profiles[vce::VCE_CONSUMER_PROFILE_ID].clone();
        configuration
            .inference_profiles
            .insert("vmm".into(), peer_profile.clone());
        configuration
            .inference_profiles
            .insert("knowledge_base:vce-decision-fixture".into(), peer_profile);
        runtime
            .replace_configuration(configuration.clone())
            .expect("publish two accounts");
        let mut candidate_route = original_route.clone();
        candidate_route.connection_id = "vce-confirmation-second-account".into();
        let original_fingerprint = vce::vce_configuration_readiness(&state)
            .embedding_physical_fingerprint
            .expect("original identity");
        let settings = ProjectVceSettings {
            enabled: true,
            ..Default::default()
        };
        // Two physical roots make the project identity and namespace assertions independent.
        // 两个物理根使项目身份及命名空间断言相互独立。
        let projects = ["confirmed-project", "rebuild-project"].map(|id| {
            let source = storage.data_root().join(id);
            std::fs::create_dir_all(&source).expect("private empty source");
            TaskProjectSummary::new(
                id,
                id,
                source.display().to_string(),
                false,
                0,
                None,
                None,
                Vec::new(),
            )
            .with_vce_settings(settings.clone())
        });
        let ai = Arc::clone(
            &state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("registered AI")
                .internal_ai_port,
        );
        let factory = VceDataPlaneFactory::bootstrap(&controller)
            .await
            .expect("private factory");
        let manager = vce::VceRuntimeManager::new(
            ai,
            factory.clone(),
            ServiceEventEmitter::new(Vec::new()),
            vce::vce_publication_authority(&state),
        );
        {
            let mut guard = state.lock().expect("state");
            guard.controller_runtime = Some(controller);
            guard.vce_runtime = Some(manager.clone());
            guard
                .workspace_snapshot
                .as_mut()
                .expect("workspace")
                .task_projects = projects.to_vec();
        }
        // The task-local scope redirects actual persistence, not the save implementation.
        // 任务局部作用域重定向真实持久化路径，而不是替换保存实现。
        let config_root = storage.data_root().join("decision-configuration");
        let outcome = std::panic::AssertUnwindSafe(PUBLIC_AI_CONFIGURATION_ROOT.scope(
            config_root.clone(),
            async {
                let config_path = config_root.join("ai-connections.toml");
                vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                    &configuration,
                    &config_root,
                )
                .expect("private durable baseline");
                for project in &projects {
                    manager
                        .enable_project(
                            &project.id,
                            std::path::Path::new(&project.path),
                            settings.clone(),
                            1024,
                            &original_fingerprint,
                            true,
                        )
                        .await
                        .expect("explicit project preparation");
                    let accepted = manager
                        .submit_job(
                            &project.id,
                            vce::VceJobAdmission {
                                retry_of_job_id: None,
                                kind: VceJobKind::Initialize,
                                idempotency_key: format!("initialize-{}", project.id),
                                expected_settings_revision: settings.revision,
                                expected_checkpoint_version: None,
                                contract_fingerprint: original_fingerprint.clone(),
                            },
                            &executor,
                        )
                        .await
                        .expect("human initialization");
                    successful_job(&manager, &project.id, &accepted.job_id).await;
                    assert!(vce::vce_project_tool_enabled(&state, &project.id));
                }
                // Same model names are insufficient: both committed projects require their own decision.
                // 相同模型名称不足以证明兼容：两个已提交项目均要求自己的决定。
                let reviewed = service_preflight_vce_embedding_route(
                    state.clone(),
                    serde_json::from_value(json!({"route": candidate_route, "dimension": 1024}))
                        .expect("candidate request"),
                )
                .await
                .expect("indexed preflight");
                assert_eq!(reviewed.projects.len(), 2);
                assert_ne!(
                    reviewed
                        .candidate
                        .as_ref()
                        .expect("candidate")
                        .physical_fingerprint,
                    original_fingerprint
                );
                for project in &reviewed.projects {
                    assert!(!project.compatible);
                    assert_eq!(project.impact.confirmation_required.len(), 1);
                    assert!(project.impact.hard_rebuild_reason.is_none());
                }
                assert_save_rejected(
                    &state,
                    &config_path,
                    confirmed_embedding_request(
                        &candidate_route,
                        1024,
                        &reviewed.revision,
                        &[(
                            "confirmed-project",
                            ContextEngineEmbeddingDecision::Compatible,
                        )],
                    ),
                    "requires an explicit compatibility or rebuild decision",
                )
                .await;
                // A supported but different dimension still cannot be approved as compatible.
                // 一个受支持但不同的维度仍不能被批准为兼容。
                let different_dimension = service_preflight_vce_embedding_route(
                    state.clone(),
                    serde_json::from_value(json!({"route": candidate_route, "dimension": 2048}))
                        .expect("dimension request"),
                )
                .await
                .expect("dimension impact");
                assert!(
                    different_dimension.projects.iter().all(|project| project
                        .impact
                        .hard_rebuild_reason
                        .as_deref()
                        == Some("Embedding dimensions changed from 1024 to 2048"))
                );
                assert_save_rejected(
                    &state,
                    &config_path,
                    confirmed_embedding_request(
                        &candidate_route,
                        2048,
                        &different_dimension.revision,
                        &[
                            (
                                "confirmed-project",
                                ContextEngineEmbeddingDecision::Compatible,
                            ),
                            (
                                "rebuild-project",
                                ContextEngineEmbeddingDecision::Compatible,
                            ),
                        ],
                    ),
                    "hard incompatibility cannot be overridden",
                )
                .await;
                // A compatible explicit rebuild advances the checkpoint while preserving its existing chain.
                // 兼容的明确重建推进检查点，同时保留既有检查点链。
                let before_rebuild = manager
                    .embedding_provenance("confirmed-project")
                    .await
                    .expect("before provenance")
                    .expect("committed provenance");
                let maintenance = manager
                    .stored_maintenance_plan(
                        "confirmed-project",
                        vulcan_context_engine::VceMaintenanceMode::Rebuild,
                    )
                    .await
                    .expect("rebuild preview");
                let rebuilt = manager
                    .submit_project_rebuild(
                        vce::VceRebuildProjectInput {
                            project_id: "confirmed-project".into(),
                            project_root: std::path::PathBuf::from(&projects[0].path),
                            settings: settings.clone(),
                            dimension: 1024,
                            fingerprint: original_fingerprint.clone(),
                        },
                        vce::VceJobAdmission {
                            retry_of_job_id: None,
                            kind: VceJobKind::Rebuild,
                            idempotency_key: "human-rebuild-after-review".into(),
                            expected_settings_revision: settings.revision,
                            expected_checkpoint_version: before_rebuild.checkpoint_version,
                            contract_fingerprint: original_fingerprint.clone(),
                        },
                        false,
                        maintenance
                            .actions
                            .into_iter()
                            .map(|action| action.action_id)
                            .collect(),
                        &executor,
                    )
                    .await
                    .expect("explicit rebuild admission");
                successful_job(&manager, "confirmed-project", &rebuilt.job_id).await;
                let after_rebuild = manager
                    .embedding_provenance("confirmed-project")
                    .await
                    .expect("after provenance")
                    .expect("replacement provenance");
                assert_ne!(
                    before_rebuild.checkpoint_version,
                    after_rebuild.checkpoint_version
                );
                let decisions = [
                    (
                        "confirmed-project",
                        ContextEngineEmbeddingDecision::Compatible,
                    ),
                    ("rebuild-project", ContextEngineEmbeddingDecision::Rebuild),
                ];
                assert_save_rejected(
                    &state,
                    &config_path,
                    confirmed_embedding_request(
                        &candidate_route,
                        1024,
                        &reviewed.revision,
                        &decisions,
                    ),
                    "changed after preflight",
                )
                .await;
                // Fresh confirmation grants only the first project and records an explicit refusal for the second.
                // 新确认仅批准第一个项目，并为第二个项目记录明确拒绝。
                let fresh = service_preflight_vce_embedding_route(
                    state.clone(),
                    serde_json::from_value(json!({"route": candidate_route, "dimension": 1024}))
                        .expect("fresh request"),
                )
                .await
                .expect("fresh provenance review");
                service_save_public_ai_inference_route(
                    ServiceEventEmitter::new(Vec::new()),
                    state.clone(),
                    confirmed_embedding_request(
                        &candidate_route,
                        1024,
                        &fresh.revision,
                        &decisions,
                    ),
                )
                .await
                .expect("save both project decisions atomically");
                let saved = runtime.configuration_snapshot().expect("saved policy");
                assert_eq!(saved.context_engine_embedding_compatibility.len(), 2);
                for (project_id, decision) in decisions {
                    let record = saved
                        .context_engine_embedding_compatibility
                        .iter()
                        .find(|record| record.scope.project_id == project_id)
                        .expect("exact project record");
                    assert_eq!(record.decision, decision);
                    assert_eq!(record.source_fingerprint, original_fingerprint);
                    assert_eq!(record.target_route, candidate_route);
                }
                assert_eq!(
                    saved.inference_profiles["vmm"],
                    configuration.inference_profiles["vmm"]
                );
                assert_eq!(
                    saved.inference_profiles["knowledge_base:vce-decision-fixture"],
                    configuration.inference_profiles["knowledge_base:vce-decision-fixture"]
                );
                assert_durable_configuration(&config_path, &saved);
                vce::reconcile_vce_projects(&state)
                    .await
                    .expect("consume saved configuration");
                // Real project reads restore compatible storage without indexing it again.
                // 真实项目读取恢复兼容存储，不再次建立索引。
                let first = vce::service_get_vce_project_activity(
                    state.clone(),
                    VceProjectRequest {
                        project_id: "confirmed-project".into(),
                    },
                )
                .await
                .expect("compatible project activity");
                let second = vce::service_get_vce_project_activity(
                    state.clone(),
                    VceProjectRequest {
                        project_id: "rebuild-project".into(),
                    },
                )
                .await
                .expect("rebuild-required project activity");
                assert!(first.query_available);
                assert!(!second.query_available);
                assert_eq!(
                    second.status.lifecycle,
                    vulcan_service_api::VceProjectLifecycleState::RebuildRequired
                );
                assert!(second.status.last_error.is_some());
                assert!(vce::vce_project_tool_enabled(&state, "confirmed-project"));
                assert!(!vce::vce_project_tool_enabled(&state, "rebuild-project"));
                let handler = vce::vce_retrieval_handler(state.clone(), manager.clone());
                // The conversation callback renders the compatible result as text.
                // 会话回调将兼容结果渲染为文本。
                assert!(
                    handler
                        .retrieve("confirmed-project".into(), "find source".into())
                        .await
                        .expect("real compatible query")
                        .is_string()
                );
                assert!(
                    handler
                        .retrieve("rebuild-project".into(), "find source".into())
                        .await
                        .is_err()
                );
                assert_eq!(
                    manager
                        .embedding_provenance("confirmed-project")
                        .await
                        .expect("retained provenance")
                        .expect("retained origin"),
                    after_rebuild
                );
                assert_eq!(
                    manager
                        .get_job("confirmed-project", None)
                        .await
                        .expect("latest job")
                        .expect("explicit rebuild remains latest")
                        .job_id,
                    rebuilt.job_id
                );
                // A real filesystem write failure must not activate the unsaved rerank route.
                // 真实文件系统写入失败不得激活未保存的重排路由。
                let mut rerank_route = saved.inference_profiles[vce::VCE_CONSUMER_PROFILE_ID]
                    .rerank
                    .clone()
                    .expect("saved rerank route");
                rerank_route.connection_id = candidate_route.connection_id.clone();
                let rerank_request: PublicAiInferenceRouteSaveRequest =
                    serde_json::from_value(json!({
                        "consumerProfileId": vce::VCE_CONSUMER_PROFILE_ID,
                        "operation": AiOperation::RerankDocuments, "route": rerank_route,
                    }))
                    .expect("rerank save request");
                let blocked_root = config_root.join("regular-file-not-a-directory");
                std::fs::write(&blocked_root, b"private persistence failure fixture")
                    .expect("private filesystem obstruction");
                PUBLIC_AI_CONFIGURATION_ROOT
                    .scope(blocked_root, async {
                        assert_save_rejected(
                            &state,
                            &config_path,
                            rerank_request.clone(),
                            "Failed to save the internal inference route",
                        )
                        .await;
                    })
                    .await;
                // Successful peer-operation saves must retain every embedding approval and committed index.
                // 其他操作成功保存时必须保留每项向量批准及已提交索引。
                service_save_public_ai_inference_route(
                    ServiceEventEmitter::new(Vec::new()),
                    state.clone(),
                    rerank_request,
                )
                .await
                .expect("real rerank route save");
                let mut llm_route = saved.inference_profiles[vce::VCE_CONSUMER_PROFILE_ID]
                    .llm_routes[vce::VCE_QUERY_REWRITE_PURPOSE_ID]
                    .clone();
                llm_route.connection_id = candidate_route.connection_id.clone();
                service_save_public_ai_inference_route(
                    ServiceEventEmitter::new(Vec::new()),
                    state.clone(),
                    serde_json::from_value(json!({
                        "consumerProfileId": vce::VCE_CONSUMER_PROFILE_ID,
                        "operation": AiOperation::ConversationRespond,
                        "purposeId": vce::VCE_QUERY_REWRITE_PURPOSE_ID, "route": llm_route,
                    }))
                    .expect("LLM save request"),
                )
                .await
                .expect("real LLM route save");
                let mut expected = saved.clone();
                let expected_profile = expected
                    .inference_profiles
                    .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE expected profile");
                expected_profile.rerank = Some(rerank_route);
                expected_profile
                    .llm_routes
                    .insert(vce::VCE_QUERY_REWRITE_PURPOSE_ID.into(), llm_route);
                assert_eq!(
                    runtime
                        .configuration_snapshot()
                        .expect("after auxiliary routes"),
                    expected
                );
                assert_durable_configuration(&config_path, &expected);
                vce::reconcile_vce_projects(&state)
                    .await
                    .expect("reconcile auxiliary routes");
                assert!(vce::vce_project_tool_enabled(&state, "confirmed-project"));
                assert!(!vce::vce_project_tool_enabled(&state, "rebuild-project"));
                assert_eq!(
                    manager
                        .embedding_provenance("confirmed-project")
                        .await
                        .expect("unchanged index")
                        .expect("committed origin"),
                    after_rebuild
                );
                assert_eq!(
                    manager
                        .get_job("confirmed-project", None)
                        .await
                        .expect("last explicit job")
                        .expect("retained receipt")
                        .job_id,
                    rebuilt.job_id
                );
                // A final human acknowledgement of different dimensions closes both projects without auto-rebuilding.
                // 最后明确确认不同维度后，两项目均关闭查询，但不会自动重建。
                let dimension_review = service_preflight_vce_embedding_route(
                    state.clone(),
                    serde_json::from_value(json!({"route": candidate_route, "dimension": 2048}))
                        .expect("final dimension request"),
                )
                .await
                .expect("final dimension preflight");
                service_save_public_ai_inference_route(
                    ServiceEventEmitter::new(Vec::new()),
                    state.clone(),
                    confirmed_embedding_request(
                        &candidate_route,
                        2048,
                        &dimension_review.revision,
                        &[
                            ("confirmed-project", ContextEngineEmbeddingDecision::Rebuild),
                            ("rebuild-project", ContextEngineEmbeddingDecision::Rebuild),
                        ],
                    ),
                )
                .await
                .expect("save explicit hard-rebuild decisions");
                expected
                    .inference_profiles
                    .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("expected VCE dimensions")
                    .embedding_dimensions = Some(2048);
                assert_eq!(
                    runtime
                        .configuration_snapshot()
                        .expect("hard-rebuild configuration"),
                    expected
                );
                assert_durable_configuration(&config_path, &expected);
                assert!(vce::vce_configuration_readiness(&state).status.ready);
                assert!(!vce::vce_project_tool_enabled(&state, "confirmed-project"));
                assert!(!vce::vce_project_tool_enabled(&state, "rebuild-project"));
                assert_eq!(
                    manager
                        .embedding_provenance("confirmed-project")
                        .await
                        .expect("original retained after dimension change")
                        .expect("old index"),
                    after_rebuild
                );
                assert_eq!(
                    manager
                        .get_job("confirmed-project", None)
                        .await
                        .expect("no automatic rebuild")
                        .expect("retained final receipt")
                        .job_id,
                    rebuilt.job_id
                );
            },
        ))
        .catch_unwind()
        .await;
        manager.shutdown().await.expect("drain private manager");
        factory.shutdown().await.expect("release private factory");
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    });
}
