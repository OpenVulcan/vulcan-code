//! Actual maintenance dispatch, HTTP summary and durable child checkpoint acceptance.
//! 实际维护分发、HTTP 摘要和可靠子检查点验收。

use super::*;
use vulcan_agent_runtime::agent_runtime::RuntimeFoundation;
use vulcan_agent_runtime::agent_runtime::prompt_compaction::{
    ChildTaskCompactionPhase, ChildTaskCompactionRequest, resolve_prompt_compaction_model_target,
};
use vulcan_agent_runtime::agent_runtime::task_runtime::{
    TaskExecutionRequest, TestSubagentRegistryEntrySeed,
};
use vulcan_session_core::v1::context::{
    LockedBaselineProjection, LockedBaselineSegmentInput, LockedPromptTarget, LockedSourceKind,
};
use vulcan_session_core::v1::document::{ApiCompletion, PrepareAndStartApiInput, SessionDocument};
use vulcan_session_core::v1::event::{
    ApiRequestSnapshot, AssistantSemanticStatus, ProviderProjectionProof, TurnClosureReason,
};
use vulcan_session_core::v1::projection::provider_visible_fingerprint;
use vulcan_session_core::v1::{
    ApiCallStatus, ApiEndReason, ApiTimings, ContinuationEligibility, TurnOutcome,
};
use vulcan_workspace_store::workspace_store::{
    CreateEmbeddedTaskSessionV1Request, create_embedded_task_session_v1,
    load_session_v1_for_runtime,
};

mod https;
mod main_only;
mod standalone;

/// Completes `document` through real API terminals for the declared `provider` and `model`; returns nothing.
/// 经声明的`provider`和`model`的真实 API 终态完成`document`；无返回值。
fn complete_business(document: &mut SessionDocument, provider: &str, model: &str) {
    // The original domain requests must establish the same immutable host guidance required of every real V1 request.
    // 原领域请求必须建立每个真实 V1 请求都要求的同一不可变宿主指引。
    let baseline = LockedBaselineProjection::project(vec![LockedBaselineSegmentInput {
        kind: LockedSourceKind::BaselineHooks,
        node_id: "host-message-guidance".into(),
        target: LockedPromptTarget::System,
        source_label: Some("host_message_guidance".into()),
        source_id: Some("v1:host_message_guidance".into()),
        content: vulcan_ai_contract::HOST_MESSAGE_GUIDANCE.into(),
    }])
    .unwrap();
    // Exceed the real maximum retained segment count so the production planner has an actual prefix to summarize.
    // 超过真实最大保留执行段数，让生产规划器确实拥有可摘要前缀。
    let rounds = vulcan_context::CONTEXT_COMPACTION_MAX_TAIL_SEGMENTS + 1;
    for round in 0..rounds {
        // Each original physical request has ordered timestamps and independent completion evidence.
        // 每个原物理请求拥有有序时间戳和独立完成凭据。
        let started_at = 5 + i64::try_from(round).unwrap() * 4;
        // Match the route captured at original child creation.
        // 匹配原子创建时捕获的路线。
        let mut request = ApiRequestSnapshot::new(provider, model);
        request.connection_id = Some("compaction-test-account".into());
        // Creation captures the profile before any request configuration has been activated.
        // 创建先捕获档案，此时尚未激活任何请求配置。
        let profile = document.metadata.child_execution_profile.as_ref().unwrap();
        request.thinking_level = profile
            .effective_config
            .provider_model
            .reasoning_effort
            .clone();
        request.temperature = profile.temperature.map(f64::from);
        // Preserve the actual semantic frontier in the public preparation proof.
        // 在公开准备凭据中保留实际语义前沿。
        let through_seq = document.events().last().unwrap().seq;
        // These real domain operations produce the original completion evidence.
        // 这些实际领域操作产生原完成凭据。
        let api = document
            .prepare_and_start_api(PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: request.clone(),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: baseline.clone(),
                continuation_action_id: None,
                now_ms: started_at,
            })
            .unwrap()
            .into_value()
            .api_call_id;
        document
            .append_assistant_text(
                api,
                "Original completed review: preserve ownership and chronological messages.",
                AssistantSemanticStatus::Completed,
                started_at + 1,
            )
            .unwrap();
        document
            .complete_api_group(
                api,
                ApiCompletion {
                    request,
                    usage: Default::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: started_at + 2,
                },
            )
            .unwrap();
    }
    // Close only the original business after every physical request has completed.
    // 所有物理请求完成后才关闭原业务。
    let completed_at = 5 + i64::try_from(rounds).unwrap() * 4;
    document
        .close_turn(TurnOutcome::Completed, 5, completed_at)
        .unwrap();
    document
        .set_collaboration_run_control(vulcan_session_core::v1::collaboration::SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("summary-business-idle").unwrap(),
            state: CollaborationRunControl::NaturallyIdle,
            changed_at_ms: completed_at + 1,
        })
        .unwrap();
}

/// Runs the production maintenance owner and stream against private HTTPS, then reloads the actual files.
/// 针对私有 HTTPS 运行生产维护所有者和流，再重载实际文件。
/// No fake stream events are injected; asserts summary, terminal, original business and next auxiliary admission.
/// 不注入假流事件；断言摘要、终态、原业务及下一辅助准入。
#[test]
fn local_child_maintenance_commits_real_http_summary_and_reloads() {
    // Use the production stream executor's stack contract, without changing Cargo or harness concurrency.
    // 使用生产流执行器的线程栈契约，不改变 Cargo 或测试框架并发。
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    // Poll the real stream on its owning service worker, not the smaller harness thread stack.
    // 在所属服务工作线程轮询真实流，而非较小的测试框架线程栈。
    let task = executor.spawn(Box::pin(run_child_maintenance(false)));
    executor.block_on(task).unwrap();
}

/// Executes the complete fixture on the service runtime; returns only after all assertions and cleanup.
/// 在服务运行时执行完整夹具；仅在全部断言和清理后返回。
async fn run_child_maintenance(standalone: bool) {
    // The fixture uses an actual bundled service route.
    // 夹具使用实际内置服务路线。
    let (provider, entry, model, credential_profile, credential_mode, reasoning) = (
        "alibaba",
        "model-studio-cn",
        "qwen3.5-flash",
        "api-key",
        vulcan_provider_adapters::CredentialMode::Token,
        "off",
    );
    // The returned text passes the same tagged summary parser used by real models.
    // 返回正文通过真实模型使用的同一标签摘要解析器。
    let server = https::Server::start("<summary>Review complete. Preserve child ownership and chronological injected messages.</summary>").await;
    // Public runtime and isolated secrets exercise the actual provider pipeline.
    // 公共运行时和隔离秘密验证实际供应商链路。
    let public_ai = Arc::new(
        vulcan_provider_adapters::PublicAiRuntime::first_batch_with_http_client_for_testing(
            Arc::new(https::Secrets),
            server.http.clone(),
        )
        .unwrap(),
    );
    // This single configured account uses existing bundled declarations.
    // 此单个配置账号使用已有内置声明。
    let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &public_ai.providers().unwrap(),
            vulcan_provider_adapters::CredentialConnection {
                id: "compaction-test-account".into(),
                provider_id: provider.into(),
                service_entry_id: entry.into(),
                credential_profile_id: credential_profile.into(),
                credential_mode,
                credential_id: "compaction-test-credential".into(),
                display_name: "Summary fixture".into(),
                secret_reference: "compaction-test-secret".into(),
                management_secret_reference: None,
                service_origin_override: Some(format!("https://{}", https::HOST)),
            },
        )
        .unwrap();
    configuration.set_model_enabled(provider, model, true);
    configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
        provider_id: provider.into(),
        model_id: model.into(),
        connection_id: "compaction-test-account".into(),
    });
    public_ai
        .replace_configuration(configuration.clone())
        .unwrap();
    // Existing bridge construction supplies isolated task, question and command runtimes.
    // 已有桥接构造提供隔离任务、提问和命令运行时。
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge.public_ai_runtime = public_ai.clone();
    // The same runtimes must own service exclusion and physical preparation.
    // 同一组运行时必须持有服务互斥和物理准备。
    let mut foundation = RuntimeFoundation::build_from_optional_mcp_access_with_runtimes(
        &bridge.provider_configuration,
        &bridge.agent_profile_catalog,
        None,
        None,
        bridge.task_runtime.clone(),
        bridge.question_runtime.clone(),
        bridge.command_session_runtime.clone(),
    );
    foundation.install_public_ai_runtime_for_testing(public_ai.clone());
    // Capture the real agent identity before creating the original immutable child profile.
    // 创建原始不可变子档案之前捕获真实代理身份。
    let agent = foundation.active_agent_definition().unwrap().id.clone();
    // Real parent reservation and child creation preserve ownership from the first durable event.
    // 真实父预留和子创建从第一条可靠事件开始保留归属。
    let (_storage, parent, mut document, _reservation, profile) = crate::agent_collaboration::startup_recovery::native_execution::tests::reserved_creation_source_with_preparation(|preparation| {
        preparation.effective_config.provider_model = ProviderModelSnapshot {
            provider_id: Some(provider.into()), model_id: Some(model.into()),
            reasoning_effort: Some(reasoning.into()), execution_mode: Some("build".into()),
        };
        preparation.resolved_config.effective_config = preparation.effective_config.clone();
        preparation.execution_agent_id = Some(agent);
        preparation.connection_id = "compaction-test-account".into();
        // This bundled model declares no explicit sampling parameter, so its actual captured request has no temperature.
        // 此内置模型没有声明显式采样参数，因此实际捕获请求不含温度。
        preparation.temperature = None;
    });
    complete_business(&mut document, provider, model);
    assert_eq!(
        document
            .metadata
            .config
            .active_request
            .thinking_level
            .as_deref(),
        Some(reasoning),
    );
    // Publish with the production embedded first-file writer, then use the normal storage backend.
    // 使用生产嵌入首次文件写入器发布，再使用普通存储后端。
    let candidate = document.prepare_flush_candidates().unwrap().unwrap();
    // The parent's exact locator determines physical ownership; there is no guessed child path.
    // 父精确定位器决定物理归属；不存在猜测子路径。
    let created = create_embedded_task_session_v1(
        &parent.storage_locator().unwrap(),
        &CreateEmbeddedTaskSessionV1Request {
            metadata: &candidate.metadata,
            runtime_lock: &candidate.runtime_lock,
            compactions: &candidate.compactions,
            events: &candidate.tail_events,
        },
    )
    .unwrap();
    document
        .apply_published_storage_metadata(&created.metadata)
        .unwrap();
    document.advance_durable(candidate.revision).unwrap();
    // The same UUID selects durable state, runtime mirror and service worker.
    // 同一个 UUID 选择可靠状态、运行时镜像和服务工作任务。
    let child = Uuid::parse_str(profile.task_id.as_str()).unwrap();
    // Preserve the locator for the real cold reload after completion.
    // 保留定位器，用于完成后的真实冷重载。
    let locator = created.locator;
    // All physical writes go through the canonical file-backed handle.
    // 全部物理写入通过规范文件后端句柄执行。
    let handle = SessionHandle::new(child, locator.clone(), document);
    // Seed only the runtime mirror; successful business proof comes from actual persisted Core history.
    // 仅播种运行时镜像；成功业务凭据来自实际持久化 Core 历史。
    let runtime = bridge.task_runtime.clone();
    runtime
        .seed_test_subagent_registry_entry(TestSubagentRegistryEntrySeed {
            root_session_id: parent.session_id(),
            parent_session_id: parent.session_id(),
            task_id: child,
            public_task_id: profile.runtime_identity.public_task_id.clone(),
            display_title: profile.runtime_identity.display_title.clone(),
            description: None,
            subagent: None,
            status: vulcan_task_session::TaskSessionStatus::Completed,
            result_summary: Some("Original completed review".into()),
        })
        .unwrap();
    runtime
        .bind_test_subagent_execution(child, &profile.business_execution_id)
        .unwrap();
    runtime
        .expect_durable_task_return(child, &profile.business_execution_id)
        .unwrap();
    runtime
        .publish_durable_task_return(
            &profile.business_execution_id,
            vulcan_session_boundary::SubagentMailboxEvent {
                parent_thread_id: parent.session_id().into(),
                parent_turn_id: None,
                child_thread_id: child.into(),
                kind: vulcan_session_boundary::SubagentMailboxEventKind::Completed,
                timestamp_ms: 9,
                payload: json!({"execution_id": profile.business_execution_id}),
            },
        )
        .unwrap();
    // Root UI selection intentionally differs from the child being compacted.
    // 根界面选择刻意区别于被压缩子任务。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(parent.session_id());
    service
        .session_runtime_registry
        .register(handle.clone())
        .unwrap();
    // Maintenance registration also owns the real root directory against archival.
    // 维护登记还针对归档持有实际根目录。
    service
        .session_runtime_registry
        .register(parent.clone())
        .unwrap();
    service.agent_runtime = Some(bridge.clone());
    // The state owns both the canonical handle and registered maintenance task.
    // 状态同时持有规范句柄和已登记维护任务。
    let state = Arc::new(Mutex::new(service));
    // Rebuild exact child history using the same canonical conversion as ordinary preparation.
    // 使用普通准备相同的规范转换重建精确子历史。
    let history = crate::agent_stream::provider_projection_controller_state(
        &handle.capture_controller_projection_snapshot().unwrap(),
    )
    .unwrap();
    // This existing helper restores only the child's captured settings; compaction chooses its own prompt purpose.
    // 此已有辅助方法仅恢复子捕获设置；压缩自行选择提示用途。
    let mut preparation = build_child_question_request(
        child,
        "Review".into(),
        &locator,
        &profile,
        profile.segment_generation,
        history,
        Default::default(),
    )
    .unwrap();
    preparation.prompt_purpose = ExecutionPromptPurpose::AgentTask;
    // Resolve an actual frozen session route rather than manufacturing provider facts.
    // 解析实际冻结会话路线，而非制造供应商事实。
    let target = resolve_prompt_compaction_model_target(
        &public_ai,
        &child.to_string(),
        None,
        Some(provider),
        Some("compaction-test-account"),
        Some(model),
        None,
        None,
    )
    .unwrap()
    .unwrap();
    if standalone {
        standalone::verify(
            preparation,
            target,
            bridge.provider_configuration,
            Arc::new(foundation),
            &handle,
            &server,
        )
        .await;
        server.shutdown().await;
        return;
    }
    // Invocation identity comes entirely from the original captured profile.
    // 调用身份完全来自原捕获档案。
    let invocation = TaskExecutionRequest {
        current_session_id: parent.session_id(),
        root_session_id: parent.session_id(),
        task_id: child,
        execution_id: profile.business_execution_id.clone(),
        public_task_id: profile.runtime_identity.public_task_id.clone(),
        prompt: profile.invocation_config.prompt.clone(),
        description: None,
        subagent: None,
        invocation_config: profile.invocation_config.clone(),
        display_title: profile.runtime_identity.display_title.clone(),
        parent_task_id: None,
        depth: usize::try_from(profile.runtime_identity.depth).unwrap(),
        snapshot: Default::default(),
        controller_config: None,
        is_resumed: false,
    };
    // Drive the actual registration, admission, provider stream, summary commit and independent settlement.
    // 驱动实际登记、准入、供应商流、摘要提交及独立结算。
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(50),
        crate::agent_collaboration::child_compaction::compact(
            &crate::ServiceEventEmitter::new(Vec::new()),
            &state,
            ChildTaskCompactionRequest {
                phase: ChildTaskCompactionPhase::CompletedBusinessMaintenance,
                invocation,
                preparation,
                provider_configuration: bridge.provider_configuration,
                model_target: target,
                foundation: Arc::new(foundation),
                luaskills_runtime: None,
            },
        ),
    )
    .await
    .expect("bounded maintenance")
    .expect("real child maintenance")
    .expect("committed summary");
    assert!(!result.messages.is_empty());
    // The wire request must contain the original task and assistant body, not an empty synthetic source.
    // 传输请求必须包含原任务和助手正文，而非空合成来源。
    let requests = server.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["model"], model);
    assert!(requests[0].get("temperature").is_none());
    assert!(
        requests[0]["messages"]
            .to_string()
            .contains("Original completed review")
    );
    // Reloading validates the actual journal, maintenance record and successful node association.
    // 重载校验实际日志、维护记录及成功节点关联。
    let reloaded = load_session_v1_for_runtime(&locator).unwrap().document;
    assert_eq!(
        reloaded.metadata.child_execution_profile.as_ref(),
        Some(&profile)
    );
    assert!(reloaded.active_api().is_none());
    assert!(reloaded.active_turn().is_none_or(|turn| turn.closed));
    // The maintenance terminal is independent from the unchanged original business profile.
    // 维护终态独立于保持不变的原业务档案。
    let maintenance = reloaded.metadata.context_maintenance.as_ref().unwrap();
    assert_eq!(
        maintenance.business_execution_id.as_ref().unwrap().as_str(),
        profile.business_execution_id
    );
    assert_eq!(
        maintenance.terminal.as_ref().unwrap().reason,
        TurnClosureReason::Completed
    );
    assert!(
        maintenance
            .terminal
            .as_ref()
            .unwrap()
            .compaction_id
            .is_some()
    );
    // Check the exact persisted summary node, not merely a successful auxiliary terminal.
    // 检查精确持久化摘要节点，而不仅是成功辅助终态。
    let node_id = maintenance
        .terminal
        .as_ref()
        .unwrap()
        .compaction_id
        .unwrap();
    // The terminal's stable node identity is the authority after cold reload.
    // 冷重载后，终态的稳定节点身份是权威。
    let node = reloaded
        .compactions
        .iter()
        .find(|node| node.compaction_id == node_id)
        .unwrap();
    assert_eq!(
        node.content,
        vulcan_session_core::v1::context::CompactionContent::Summary {
            text: "Review complete. Preserve child ownership and chronological injected messages."
                .to_string()
        }
    );
    assert_eq!(reloaded.runtime_lock.current_compaction_id, Some(node_id));
    assert!(state.lock().unwrap().child_maintenance_tasks.is_empty());
    assert!(state.lock().unwrap().child_maintenance_owners.is_empty());
    assert!(!runtime.is_task_running(child).unwrap());
    assert!(
        runtime
            .try_reserve_child_question_execution(child)
            .unwrap()
            .is_some()
    );
    server.shutdown().await;
}
