//! Production prompt preparation under live semantic-index admission changes.
//! 实时语义索引准入变化下的生产提示词准备。

use super::vce_configuration_readiness_tests::configured_state;
use super::*;
use futures_util::FutureExt;
use std::time::Duration;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_session_core::v1::ids::ClientActionId;
use vulcan_workspace_store::vce_data_plane::{VceDataPlaneFactory, VceJobKind, VceJobState};

/// Prepare a real main-session request without dispatching a billable conversation model.
/// 准备真实主会话请求，不派发计费会话模型。
/// State, controller and root identify the isolated host; source selects ordinary or goal continuation input.
/// 状态、控制器和根目录标识隔离宿主；来源选择普通输入或目标续跑输入。
/// An optional action must already be admitted by the real V1 Continue authority.
/// 可选动作必须已经由真实 V1 Continue 权威准入。
/// Returns the production request after hook completion, V1 projection and exact-generation publication.
/// 返回完成 Hook、V1 投影及精确代际发布后的生产请求。
async fn prepare_request(
    state: &SharedServiceBridgeState,
    controller: &ControllerRuntimeHandle,
    root: &str,
    source: PendingSessionPromptSource,
    continuation_action_id: Option<ClientActionId>,
) -> ExecutionPreparationRequest {
    // Retain the exact bridge and session; no global model or project selection is substituted.
    // 保留精确桥接与会话，不替换全局模型或项目选择。
    let (bridge, session_id) = {
        // The short guard only captures fixture ownership.
        // 短期守卫只捕获夹具归属。
        let guard = state.lock().expect("fixture state");
        (
            guard.agent_runtime.clone().expect("registered bridge"),
            guard
                .workspace_snapshot
                .as_ref()
                .expect("workspace")
                .selected_session,
        )
    };
    // Use the shared begin reducer so hook identities and canonical prompt state are not fabricated.
    // 使用共享开始归约器，不伪造 Hook 身份及规范提示词状态。
    let (start, generation) = {
        // The workspace guard makes begin and kernel admission one fixture-owned transition.
        // 工作区守卫将开始与内核准入组成夹具持有的转换。
        let mut guard = state.lock().expect("fixture state");
        // Hook registration remains the real empty host registry, not a synthetic outcome.
        // Hook 注册保持真实空宿主注册表，而不是合成结果。
        let mut hooks = bridge
            .conversation_hook_runtime
            .lock()
            .expect("hook runtime");
        // Source semantics decide whether the ordinary user text is added to history.
        // 来源语义决定是否将普通用户文本加入历史。
        let start = begin_shared_prompt_turn(
            guard.workspace_snapshot.as_mut().expect("workspace"),
            &mut hooks,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: "VCE request admission",
                workspace_path: root,
                prompt_entry_id: Uuid::new_v4().to_string(),
                continuation_action_id,
                prompt: "Explain the cache lookup.",
                hidden_model_guidance: None,
                provider_visible_agent_messages: Vec::new(),
                attachments: Vec::new(),
                provider_attachments: None,
                existing_canonical_user_message_id: None,
                append_canonical_user_message: source.persists_canonical_user_message(),
                append_visible_user_prompt_record: source.appends_visible_user_prompt_record(),
                source,
                vmm_runtime_ready: false,
                vmm_precheck_ready: false,
                execution_mode: String::from("agent"),
                reasoning_effort: None,
            },
        )
        .expect("shared prompt begin");
        start_webui_session_kernel_user_turn(&mut guard, session_id, &start.preparation_turn_id)
            .expect("real kernel admission");
        // Only the actual kernel owns the generation consumed by production continuation.
        // 只有真实内核持有生产继续函数消费的代际。
        let generation = current_webui_prompt_preparation_generation(
            &guard,
            session_id,
            &start.preparation_turn_id,
        )
        .expect("current preparation generation");
        (start, generation)
    };
    // Complete the real empty profile dispatcher and the explicit disabled precheck path.
    // 完成真实空画像派发器及明确禁用的预检路径。
    let profile = start.profile_request.clone().run().await;
    // Disabled memory is bypassed without opening any memory controller.
    // 禁用记忆时直接绕过，不打开任何记忆控制器。
    let precheck = start.precheck_request.clone().skip();
    // This is the same complete host function used before ordinary and queued Provider execution.
    // 这是普通及排队 Provider 执行前使用的同一个完整宿主函数。
    let request =
        crate::webui_prompt_lifecycle::continue_webui_prompt_turn_after_preparation_from_state(
            &ServiceEventEmitter::new(Vec::new()),
            state,
            controller,
            &bridge,
            start,
            generation,
            profile,
            precheck,
            None,
        )
        .expect("production prompt preparation")
        .expect("current owner request");
    stop_webui_session_kernel_generation(&mut state.lock().expect("fixture state"), session_id);
    request
}

/// Create a durable retryable API failure and admit the exact Continue action through V1.
/// 创建可靠可重试 API 失败，并通过 V1 准入精确 Continue 动作。
/// The handle owns private session files. Returns the admitted action without a network request.
/// 句柄持有私有会话文件。返回已准入动作，不发送网络请求。
async fn admit_continue(
    handle: &Arc<crate::session_runtime_registry_v1::SessionHandle>,
) -> ClientActionId {
    use crate::session_runtime_registry_v1::{SessionFlushReason, SessionMutationAdmission};
    use vulcan_session_core::v1::context::LockedBaselineProjection;
    use vulcan_session_core::v1::document::{
        ApiCompletion, ClientInputSubmission, ContinuationRequest, PrepareAndStartApiInput,
    };
    use vulcan_session_core::v1::event::{
        ApiCallStatus, ApiEndReason, ApiErrorCategory, ApiPurposeKind, ApiRequestSnapshot,
        ApiTimings, ContinuationEligibility, ProviderProjectionProof, RedactedApiError,
    };
    use vulcan_session_core::v1::ids::Seq;
    use vulcan_session_core::v1::projection::provider_visible_fingerprint;
    use vulcan_session_core::v1::stats::UsageSnapshot;

    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("vce-request-root").expect("root action"),
                "Explain the cache lookup.",
                Vec::new(),
            ),
            2,
        )
        .expect("durable root input");
    // Reuse a real declared model identity; this fixture records a pre-wire failure without billing.
    // 复用真实声明模型身份；该夹具记录传输前失败，不产生计费。
    let request = ApiRequestSnapshot::new("alibaba", "qwen3.5-flash");
    // Projection proof is calculated from actual persisted events, not copied from another session.
    // 投影证明根据实际持久事件计算，不从另一会话复制。
    let input = handle
        .read(|document| {
            // The root input is the exact last provider-visible frontier before API preparation.
            // 根输入是 API 准备前精确的最后 Provider 可见前沿。
            let through_seq = document.events().last().expect("root event").seq();
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: request.clone(),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).expect("initial retained seq"),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .expect("actual history fingerprint"),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: 4,
            }
        })
        .expect("read private projection");
    // V1 starts the logical API through its real durable mutation boundary.
    // V1 通过真实可靠变更边界开始逻辑 API。
    let api_id = handle
        .prepare_and_start_api_durable(input)
        .expect("durable initial API")
        .value
        .api_call_id;
    Arc::clone(handle)
        .mutate_durable_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(
                    api_id,
                    ApiCompletion {
                        request,
                        usage: UsageSnapshot::default(),
                        status: ApiCallStatus::Failed,
                        end_reason: Some(ApiEndReason::NetworkError),
                        error: Some(RedactedApiError {
                            category: ApiErrorCategory::Network,
                            error_code: None,
                            message: String::from("VCE fixture pre-wire failure"),
                        }),
                        continuation_eligibility: ContinuationEligibility::Allowed,
                        timing: ApiTimings {
                            completed_at_ms: Some(5),
                            ..ApiTimings::default()
                        },
                        provider_source: None,
                        completed_at_ms: 5,
                    },
                )
            },
        )
        .await
        .expect("persist retryable API failure");
    // Admission must succeed before this action is passed into production request preparation.
    // 必须先准入成功，才能将动作传入生产请求准备。
    let action = ClientActionId::try_new("vce-request-continue").expect("continue identity");
    handle
        .continue_turn(ContinuationRequest::new(action.clone(), "", Vec::new()), 6)
        .expect("real V1 Continue admission");
    action
}

/// Verify actual request construction respects all prerequisites without creating an index.
/// 验证实际请求构建遵循全部前提，且不创建索引。
/// No input; a private controller and registered credential-rejecting AI isolate user storage and billing.
/// 无输入；私有控制器及拒绝凭据的已注册 AI 隔离用户存储与计费。
#[test]
#[ignore = "requires the bundled local VLDB controller executable"]
fn real_controller_prompt_preparation_obeys_vce_prerequisites() {
    // Bound asynchronous work independently of a running user service.
    // 独立于正在运行的用户服务约束异步工作。
    let executor = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("private request executor"),
    );
    // Reuse the production-shaped registered inference fixture without accessing credentials.
    // 复用生产形态已注册推理夹具，不访问凭据。
    let (state, runtime, valid) = configured_state();
    // Preserve the existing session identity when publishing its physical V1 files.
    // 发布物理 V1 文件时保留既有会话身份。
    let session_id = state
        .lock()
        .expect("state")
        .workspace_snapshot
        .as_ref()
        .expect("workspace")
        .selected_session;
    // The fixture owns exact temporary controller and session paths.
    // 夹具持有精确临时控制器及会话路径。
    let storage = TestControllerRuntimeRoot::new(session_id);
    executor.block_on(async {
        // Attach only private application storage.
        // 只附着私有应用存储。
        let (client, controller) = storage.attached_controller_runtime_handle().await;
        // Release the controller even if setup or a service assertion panics.
        // 即使准备或服务断言发生异常，也释放控制器。
        let outer = std::panic::AssertUnwindSafe(async {
            // A real V1 owner provides metadata, Goal/Plan projection and the physical locator.
            // 真实 V1 所有者提供元数据、目标／计划投影及物理位置。
            let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(
                &storage.v1_storage_locator(),
            )
            .expect("private V1 session");
            // The session registry retains the production persistence backend.
            // 会话注册表保留生产持久化后端。
            let handle = crate::session_runtime_registry_v1::SessionHandle::new(
                session_id,
                storage.v1_storage_locator(),
                loaded.document,
            );
            // Deliberately absent source makes accidental first-build work observable.
            // 特意不存在的源码目录使意外首次建库工作可观测。
            let source = storage.data_root().join("request-project-source");
            // The project identity matches actual V1 metadata rather than the fixture's old display summary.
            // 项目身份匹配真实 V1 元数据，而不是夹具的旧展示摘要。
            let mut project = vulcan_workbench_model::TaskProjectSummary::new(
                "v1-test-project",
                "VCE request project",
                source.display().to_string(),
                false,
                0,
                None,
                None,
                vec![session_id],
            );
            {
                // Publish exact owning project and V1 handle before beginning any request.
                // 开始任何请求前发布精确所属项目及 V1 句柄。
                let mut guard = state.lock().expect("state");
                guard.controller_runtime = Some(controller.clone());
                guard
                    .session_runtime_registry
                    .register(handle.clone())
                    .expect("register V1 owner");
                // Only this session's project identity is changed in the private workspace.
                // 只修改私有工作区中该会话的项目身份。
                let workspace = guard.workspace_snapshot.as_mut().expect("workspace");
                workspace.sessions[0].project_id = Some(project.id.clone());
                workspace.task_projects = vec![project.clone()];
            }
            // The factory shares host storage but creates no project binding before explicit admission.
            // 工厂共享宿主存储，但明确准入之前不创建项目绑定。
            let factory = VceDataPlaneFactory::bootstrap(&controller)
                .await
                .expect("private factory");
            // Capture the registered AI port instead of constructing another inference service.
            // 捕获已注册 AI 端口，不构建另一套推理服务。
            let ai = state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .internal_ai_port
                .clone();
            // The manager uses the actual live publication authority consumed by tool callbacks.
            // 管理器使用工具回调消费的实际实时发布权威。
            let manager = vce::VceRuntimeManager::new(
                ai,
                factory.clone(),
                ServiceEventEmitter::new(Vec::new()),
                vce::vce_publication_authority(&state),
            );
            state.lock().expect("state").vce_runtime = Some(manager.clone());
            // Drain owned runtime resources even when an assertion fails.
            // 即使断言失败，也排空持有的运行时资源。
            let outcome = std::panic::AssertUnwindSafe(async {
                for phase in 0..3 {
                    project.vce.enabled = phase != 1;
                    state
                        .lock()
                        .expect("state")
                        .workspace_snapshot
                        .as_mut()
                        .expect("workspace")
                        .task_projects = vec![project.clone()];
                    // Phase zero lacks models; phase one disables the project; phase two has no first checkpoint.
                    // 阶段零缺少模型；阶段一关闭项目；阶段二没有首次检查点。
                    let mut configuration = valid.clone();
                    if phase == 0 {
                        configuration
                            .inference_profiles
                            .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                            .expect("profile")
                            .embedding = None;
                    }
                    runtime
                        .replace_configuration(configuration)
                        .expect("private phase models");
                    for source_kind in [
                        PendingSessionPromptSource::UserFollowup,
                        PendingSessionPromptSource::GoalContinuation,
                    ] {
                        // Both entry sources must build ordinary requests while hiding only semantic retrieval.
                        // 两类入口都必须构建普通请求，同时隐藏语义检索。
                        let request = prepare_request(
                            &state,
                            &controller,
                            &source.display().to_string(),
                            source_kind,
                            None,
                        )
                        .await;
                        assert_eq!(request.project_id.as_deref(), Some(project.id.as_str()));
                        assert!(request.vce_retrieval_handler.is_none());
                        assert_eq!(
                            request
                                .tool_request_hidden_tool_ids
                                .iter()
                                .filter(|id| id.as_str()
                                    == vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID)
                                .count(),
                            1
                        );
                        assert!(!request.tool_request_rejection_all_tools);
                    }
                    assert!(!source.exists());
                    assert!(!storage.data_root().join("vce/projects").exists());
                    assert!(
                        manager
                            .get_job(&project.id, None)
                            .await
                            .expect("no job")
                            .is_none()
                    );
                }
                // Only explicit initialization below creates the first index.
                // 只有下面的明确初始化创建首个索引。
                std::fs::create_dir_all(&source).expect("human-selected empty source");
                // Physical contract comes from actual registered readiness.
                // 物理契约来自实际已注册准备度。
                let fingerprint = vce::vce_configuration_readiness(&state)
                    .embedding_physical_fingerprint
                    .expect("ready fingerprint");
                manager
                    .enable_project(
                        &project.id,
                        &source,
                        project.vce.clone(),
                        1024,
                        &fingerprint,
                        true,
                    )
                    .await
                    .expect("explicit project activation");
                // This receipt is the only admitted index job throughout request construction.
                // 该回执是整个请求构建过程唯一准入的索引作业。
                let initial = manager
                    .submit_job(
                        &project.id,
                        vce::VceJobAdmission {
                            retry_of_job_id: None,
                            kind: VceJobKind::Initialize,
                            idempotency_key: "request-admission-initialize".into(),
                            expected_settings_revision: project.vce.revision,
                            expected_checkpoint_version: None,
                            contract_fingerprint: fingerprint,
                        },
                        &executor,
                    )
                    .await
                    .expect("explicit first job");
                // The bounded wait observes the real durable job, not a fabricated ready flag.
                // 有界等待观察真实持久作业，而不是伪造就绪标志。
                let completed = tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        // Re-read the exact durable receipt until terminal.
                        // 重新读取精确持久回执，直到终态。
                        let job = manager
                            .get_job(&project.id, Some(&initial.job_id))
                            .await
                            .expect("progress")
                            .expect("receipt");
                        if job.state.is_terminal() {
                            break job;
                        }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .expect("initialization deadline");
                assert_eq!(completed.state, VceJobState::Succeeded);
                for source_kind in [
                    PendingSessionPromptSource::UserFollowup,
                    PendingSessionPromptSource::GoalContinuation,
                ] {
                    // Capture an admitted request so later revocation can test its retained callback.
                    // 捕获已准入请求，以便后续撤销测试保留的回调。
                    let request = prepare_request(
                        &state,
                        &controller,
                        &source.display().to_string(),
                        source_kind,
                        None,
                    )
                    .await;
                    assert!(
                        !request
                            .tool_request_hidden_tool_ids
                            .iter()
                            .any(|id| id == vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID)
                    );
                    // Request preparation must install the real retrieval handler only after all three prerequisites.
                    // 请求准备只有在三个前提全满足后才安装真实检索处理器。
                    let handler = request
                        .vce_retrieval_handler
                        .as_ref()
                        .expect("admitted callback");
                    assert!(
                        handler
                            .retrieve(project.id.clone(), "find cache".into())
                            .await
                            .is_ok()
                    );
                    state
                        .lock()
                        .expect("state")
                        .workspace_snapshot
                        .as_mut()
                        .expect("workspace")
                        .task_projects[0]
                        .vce
                        .enabled = false;
                    assert!(
                        handler
                            .retrieve(project.id.clone(), "find cache".into())
                            .await
                            .is_err()
                    );
                    assert!(
                        request.vce_retrieval_handler.is_some(),
                        "request snapshot remains intact"
                    );
                    state
                        .lock()
                        .expect("state")
                        .workspace_snapshot
                        .as_mut()
                        .expect("workspace")
                        .task_projects[0]
                        .vce
                        .enabled = true;
                }
                // Durable Continue restores V1 history and still uses the same live semantic admission.
                // 可靠 Continue 恢复 V1 历史，并仍使用同一个实时语义准入。
                let action = admit_continue(&handle).await;
                for enabled in [false, true] {
                    state
                        .lock()
                        .expect("state")
                        .workspace_snapshot
                        .as_mut()
                        .expect("workspace")
                        .task_projects[0]
                        .vce
                        .enabled = enabled;
                    // Run the actual continuation-action branch, not merely a goal-continuation source tag.
                    // 执行实际继续动作分支，而不只是目标续跑来源标记。
                    let request = prepare_request(
                        &state,
                        &controller,
                        &source.display().to_string(),
                        PendingSessionPromptSource::ExecutionRetry,
                        Some(action.clone()),
                    )
                    .await;
                    assert_eq!(request.continuation_action_id.as_ref(), Some(&action));
                    assert_eq!(request.vce_retrieval_handler.is_some(), enabled);
                    assert_eq!(
                        request
                            .tool_request_hidden_tool_ids
                            .iter()
                            .any(|id| id == vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID),
                        !enabled
                    );
                    assert!(!request.tool_request_rejection_all_tools);
                    assert!(
                        !request.conversation_state.messages.is_empty(),
                        "V1 history restored"
                    );
                }
                assert_eq!(
                    manager
                        .get_job(&project.id, None)
                        .await
                        .expect("latest job")
                        .expect("initial job retained")
                        .job_id,
                    initial.job_id
                );
                assert!(
                    state
                        .lock()
                        .expect("state")
                        .vmm_controller_registry
                        .snapshots()
                        .expect("memory registry")
                        .is_empty()
                );
                assert_eq!(
                    runtime
                        .configuration_snapshot()
                        .expect("configuration retained"),
                    valid
                );
            })
            .catch_unwind()
            .await;
            manager.shutdown().await.expect("drain VCE");
            factory.shutdown().await.expect("release VCE storage");
            if let Err(panic) = outcome {
                std::panic::resume_unwind(panic);
            }
        })
        .catch_unwind()
        .await;
        client.shutdown().await.expect("release private controller");
        if let Err(panic) = outer {
            std::panic::resume_unwind(panic);
        }
    });
}
