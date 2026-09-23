//! Actual child question, MainOnly wakeup, model tool reply and retained-parent acceptance.
//! 真实子问题、MainOnly 唤醒、模型工具回复及父状态保留验收。

use super::*;
use crate::agent_collaboration::{
    AgentCollaborationCommand, participants, recovery_invocation,
    session_v1_native_agent_collaboration_runtime,
};
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskV1ExecutionFacts;
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_PARENT_TOOL_ID, AskParentArgs, WAIT_QUESTION_TOOL_ID, WaitQuestionArgs,
};
use vulcan_session_core::v1::collaboration::{
    CollaborationQuestionExecutionState, CollaborationQuestionState, ReserveChildTaskInput,
    SetRunControlInput,
};
use vulcan_session_core::v1::config::ActiveRequestConfig;
use vulcan_session_core::v1::document::ClientInputSubmission;
use vulcan_session_core::v1::event::{EventBody, ToolPayload, UserInputSource};
use vulcan_session_core::v1::ids::{ClientActionId, RequestStreamId};
use vulcan_tools_manager::VulcanToolInvocationContext;

/// Restores the test default after the real service projection barriers have drained.
/// 真实服务投影屏障排空后恢复测试默认值。
struct BarrierPolicyGuard;

impl Drop for BarrierPolicyGuard {
    /// Disables barriers even when the acceptance assertion unwinds.
    /// 即使验收断言退栈，也关闭屏障。
    fn drop(&mut self) {
        let _ = vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::set_runtime_event_projection_barriers_enabled(false);
    }
}

/// Runs the full automatic QA dispatcher using the production service worker stack.
/// 使用生产服务工作线程栈运行完整自动答疑分发器。
#[test]
fn main_only_child_question_wakes_real_model_replies_and_preserves_paused_goal() {
    // Match service bootstrap: model tools need exact durable identities returned by the stream consumer.
    // 对齐服务引导：模型工具需要事件流消费者返回精确持久化身份。
    vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::set_runtime_event_projection_barriers_enabled(true).unwrap();
    let _barrier_guard = BarrierPolicyGuard;
    // Own the private executor through all asynchronous dispatch and cleanup.
    // 在全部异步分发及清理期间持有私有执行器。
    let executor = Arc::new(
        crate::service_bridge_io::webui_service_runtime_builder()
            .worker_threads(2)
            .build()
            .unwrap(),
    );
    let task = executor.spawn(Box::pin(run(executor.clone())));
    executor.block_on(task).unwrap();
}

/// Executes the real parent/child flow on `executor`; returns after durable reply and QA settlement.
/// 在 `executor` 执行真实父子流程；可靠回复及答疑结算后返回。
async fn run(executor: Arc<tokio::runtime::Runtime>) {
    // Reuse indexed storage and the exact canonical registry, never a synthetic workspace-only session.
    // 复用索引存储及精确规范注册表，不使用仅工作区合成会话。
    let (storage, mut invocation, parent) =
        crate::agent_collaboration::test_support::fixture(false).await;
    let server = https::Server::start_question_answerer().await;
    let public_ai = Arc::new(
        vulcan_provider_adapters::PublicAiRuntime::first_batch_with_http_client_for_testing(
            Arc::new(https::Secrets {
                native: false,
                refreshed: Mutex::new(None),
            }),
            server.http.clone(),
        )
        .unwrap(),
    );
    let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &public_ai.providers().unwrap(),
            vulcan_provider_adapters::CredentialConnection {
                id: "compaction-test-account".into(),
                provider_id: "alibaba".into(),
                service_entry_id: "model-studio-cn".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                credential_id: "compaction-test-credential".into(),
                display_name: "Question fixture".into(),
                secret_reference: "compaction-test-secret".into(),
                management_secret_reference: None,
                service_origin_override: Some(format!("https://{}", https::HOST)),
                prompt_cache_by_protocol: Default::default(),
            },
        )
        .unwrap();
    configuration.set_model_enabled("alibaba", "qwen3.5-flash", true);
    configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
        provider_id: "alibaba".into(),
        model_id: "qwen3.5-flash".into(),
        connection_id: "compaction-test-account".into(),
    });
    public_ai.replace_configuration(configuration).unwrap();
    // Register actual statistics and project storage required by full service preparation.
    // 登记完整服务准备所需的真实统计及项目存储。
    let mut controller = invocation.controller_runtime.clone();
    controller.system_statistics_sqlite_path = storage.data_root().join("question-usage.sqlite3");
    controller.system_statistics_sqlite_binding_id =
        format!("question-usage-{}", parent.session_id());
    vulcan_workspace_store::data_plane::prepare_application_schema_for_test(&controller)
        .await
        .unwrap();
    invocation.controller_runtime = controller.clone();
    let project = vulcan_workbench_model::TaskProjectSummary::new(
        parent.storage_locator().unwrap().project_id(),
        "Question fixture",
        storage.data_root().display().to_string(),
        false,
        0,
        None,
        None,
        vec![invocation.source_session_id, parent.session_id()],
    );
    vulcan_workspace_store::workspace_store::persist_new_project(controller.clone(), &project)
        .await
        .unwrap();
    parent.initialize_root_usage(true, 1);
    parent
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("retained-parent-business").unwrap(),
                "Retain this parent task while the owned child works.",
                vec![],
            )
            .with_bound_request_config(ActiveRequestConfig {
                provider: Some("alibaba".into()),
                model: Some("qwen3.5-flash".into()),
                connection_id: Some("compaction-test-account".into()),
                thinking_level: None,
                temperature: None,
            }),
            2,
        )
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.create_goal("Keep the original goal paused", None, "Goal is active", 3)
            },
        )
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| document.pause_goal("User paused this goal", 4),
        )
        .unwrap();
    // The production child creator establishes durable ownership before any question is sent.
    // 生产子任务创建器在发送问题前建立可靠归属。
    let child_id = Uuid::new_v4();
    let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &invocation.registry,
        TaskV1ExecutionFacts {
            root_session_id: parent.session_id(),
            parent_session_id: parent.session_id(),
            task_id: child_id,
            execution_id: "main-only-http-child".into(),
            prompt: "Ask the parent and continue owned work.".into(),
            display_title: "Question child".into(),
            is_resumed: false,
            admitted_at_ms: 5,
        },
    )
    .unwrap();
    let child_invocation = recovery_invocation(
        invocation.state.clone(),
        controller.clone(),
        invocation.registry.clone(),
        child_id,
        child.clone(),
    )
    .unwrap();
    let identity = participants::participant_for_handle(&child_invocation, child_id, &child, None)
        .await
        .unwrap();
    let parent_turn = parent
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_task(ReserveChildTaskInput {
                    source_tool_name: "spawn_agent".into(),
                    operation_id: CollaborationOperationId::try_new("reserve-http-child").unwrap(),
                    execution_id:
                        vulcan_session_core::v1::collaboration::CollaborationExecutionId::try_new(
                            "main-only-http-child",
                        )
                        .unwrap(),
                    task_id: identity.task_id.clone().unwrap(),
                    parent_turn_id: parent_turn,
                    child: identity,
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    parent
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("main-only-http-stop").unwrap(),
            state: CollaborationRunControl::MainOnly,
            changed_at_ms: 6,
        })
        .unwrap();
    // Preserve real paused goal metadata, original human inputs and queued work across QA.
    // 跨答疑保留真实暂停目标元数据、原人类输入及排队工作。
    let before = parent
        .read(|document| {
            (
                document.metadata.mode.clone(),
                document
                    .events()
                    .iter()
                    .filter(|event| matches!(&event.event, EventBody::UserInput(input) if input.input_source != UserInputSource::AgentMessage))
                    .count(),
                document.metadata.input_state.queued_inputs.len(),
            )
        })
        .unwrap();
    let workspace =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(controller.clone())
            .await
            .unwrap();
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge.public_ai_runtime = public_ai.clone();
    bridge.configured_inference_service =
        Arc::new(crate::configured_inference::ConfiguredInferenceService::new(public_ai));
    bridge.internal_ai_port = Arc::new(crate::internal_ai::ConfiguredInferenceHostAiPort::new(
        bridge.configured_inference_service.clone(),
    ));
    {
        let mut guard = invocation.state.lock().unwrap();
        guard.controller_runtime = Some(controller);
        guard.workspace_snapshot = Some(workspace);
        guard.service_runtime = Some(executor);
        guard.agent_runtime = Some(bridge);
    }
    // AskParent schedules the actual wakeup; the fixture never calls the wakeup or reply reducers directly.
    // AskParent 调度真实唤醒；夹具不直接调用唤醒或回复归约器。
    let port = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        crate::ServiceEventEmitter::new(vec![]),
    );
    let asked = port
        .invoke(
            tool_context(&child, ASK_PARENT_TOOL_ID, 7),
            AgentCollaborationCommand::AskParent(AskParentArgs {
                question: "May I continue the child work?".into(),
            }),
        )
        .await
        .unwrap();
    let question_id = asked["question_id"].as_str().unwrap().to_owned();
    let observed = port
        .invoke(
            tool_context(&child, WAIT_QUESTION_TOOL_ID, 8),
            AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
                question_id: question_id.clone(),
                after_revision: None,
                timeout_ms: 10_000,
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        observed["question_state"],
        "answered",
        "dispatch diagnostics: {:?}",
        invocation.state.lock().unwrap().startup.recent_logs
    );
    assert_eq!(
        observed["answer"]["body"],
        "Continue the owned child work; the parent goal remains paused."
    );
    // Waiting for QA completion is bounded and checks the actual durable execution state.
    // 等待答疑完成有明确上限，并检查实际可靠执行状态。
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if parent
                .read(|document| {
                    document
                        .collaboration_state()
                        .question_executions
                        .values()
                        .any(|execution| {
                            execution.question_id.as_str() == question_id
                                && execution.state == CollaborationQuestionExecutionState::Completed
                        })
                })
                .unwrap()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the auxiliary execution must settle after its model final");
    let reloaded = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        reloaded.collaboration_run_control(),
        CollaborationRunControl::MainOnly
    );
    assert_eq!(
        (
            reloaded.metadata.mode.clone(),
            reloaded
                .events()
                .iter()
                .filter(|event| matches!(&event.event, EventBody::UserInput(input) if input.input_source != UserInputSource::AgentMessage))
                .count(),
            reloaded.metadata.input_state.queued_inputs.len()
        ),
        before
    );
    assert_eq!(reloaded.active_turn().unwrap().turn_id, parent_turn);
    assert!(!reloaded.active_turn().unwrap().closed);
    assert!(reloaded.active_api().is_none());
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    assert!(
        child
            .read(|document| document
                .collaboration_state()
                .questions
                .values()
                .any(|question| {
                    question.question_id.as_str() == question_id
                        && question.state == CollaborationQuestionState::Answered
                }))
            .unwrap()
    );
    server.shutdown().await;
}

/// Prepares an authenticated `tool` call on `child` at `stamp`, preserving its active API when present.
/// 在 `stamp` 为 `child` 准备已认证 `tool` 调用，存在活动 API 时保留它。
/// Returns the exact domain ToolCall identity consumed by the native collaboration port.
/// 返回原生协作端口消费的精确领域工具调用身份。
fn tool_context(child: &Arc<SessionHandle>, tool: &str, stamp: i64) -> VulcanToolInvocationContext {
    let api_call_id = match child
        .read(|document| document.active_api().map(|api| api.api_call_id))
        .unwrap()
    {
        Some(api) => api,
        None => {
            let input = child.read(|document| {
                let through_seq = document.events().last().unwrap().seq;
                PrepareAndStartApiInput {
                    purpose: ApiPurposeKind::MainGeneration,
                    request: ApiRequestSnapshot::new("alibaba", "qwen3.5-flash"),
                    request_budget_evidence: crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                    projection: ProviderProjectionProof {
                        segment_generation: document.segment_generation(), document_revision: document.memory_revision(),
                        retained_from_seq: Seq::try_new(1).unwrap(), through_seq,
                        fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq).unwrap(),
                    },
                    locked_baseline: LockedBaselineProjection::empty(), continuation_action_id: None, now_ms: stamp,
                }
            }).unwrap();
            child
                .prepare_and_start_api_durable(input)
                .unwrap()
                .value
                .api_call_id
        }
    };
    let stream = RequestStreamId::try_new(format!("main-only-{tool}")).unwrap();
    let receipt = child
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document
                    .prepare_tool_batch(
                        api_call_id,
                        vec![vulcan_session_core::v1::document::ToolPreparation {
                            request_stream_id: stream.clone(),
                            tool_name: tool.into(),
                            input: ToolPayload::empty(),
                            provider_tool_call_id: Some(tool.into()),
                        }],
                        stamp,
                    )
                    .map(|result| result.into_value())
            },
        )
        .unwrap()
        .value;
    let call = receipt.tools.into_iter().next().unwrap();
    VulcanToolInvocationContext {
        session_id: child.session_id().to_string(),
        request_stream_id: stream,
        api_call_id,
        tool_call_id: call.tool_call_id,
        tool_seq: call.tool_seq,
        controller_tool_call_id: tool.into(),
        provider_tool_call_id: Some(tool.into()),
    }
}
