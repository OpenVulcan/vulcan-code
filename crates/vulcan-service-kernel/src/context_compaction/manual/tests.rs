//! Real session-store regressions for manual ownership, error visibility and independent retry budgets.
//! 手动归属、错误可见性和独立重试额度的真实会话存储回归。

use super::*;
use crate::session_runtime_registry_v1::SessionHandle;
use crate::tests::test_support::{TestHostRuntimeRoot, test_service_state_with_parent_session};
use std::sync::Mutex;
use vulcan_session_core::{
    ActiveTurnKind,
    v1::{ClientActionId, ClientInputSubmission, event::EventBody},
};

mod borrowed;

/// Runs both manual owners through physical dispatch and summary commit after the old account is removed.
/// 旧账号删除后，验证两种手动所有者均通过物理分派及摘要提交。
#[test]
fn manual_compaction_dispatches_pending_route_after_old_connection_removal() {
    // Use the production worker stack for the real summary transport.
    // 使用生产工作线程栈执行真实摘要传输。
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    executor
        .block_on(executor.spawn(Box::pin(async {
            verify_manual_compaction_replacement(false).await;
            verify_manual_compaction_replacement(true).await;
        })))
        .unwrap();
}

/// Verifies a standalone or `borrowed` summary using isolated storage and loopback TLS.
/// 使用隔离存储和回环 TLS 验证独立或 `borrowed` 摘要。
/// Returns after checking the physical model, committed summary and retained business configuration.
/// 检查物理模型、已提交摘要及保留的业务配置后返回。
async fn verify_manual_compaction_replacement(borrowed: bool) {
    use crate::agent_collaboration::compaction_test_https as https;
    use vulcan_session_core::v1::event::ApiPurposeKind;

    // Use isolated durable state and the bundled static catalog.
    // 使用隔离持久状态与内置静态目录。
    let (root, state, handle, fixture_task_id) = fixture().await;
    // The bundled OpenCode host is pinned to this private TLS listener.
    // 内置 OpenCode 主机固定至此私有 TLS 监听器。
    let server = https::Server::start(
        "<summary>Retain the prior review and all outstanding tasks.</summary>",
    )
    .await;
    // Install a fixture credential and the pinned HTTP client on the real provider runtime.
    // 将夹具凭据及固定 HTTP 客户端安装到真实供应商运行时。
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge.public_ai_runtime = Arc::new(
        vulcan_provider_adapters::PublicAiRuntime::first_batch_with_http_client_for_testing(
            Arc::new(https::Secrets),
            server.http.clone(),
        )
        .unwrap(),
    );
    // The replacement is the only configured connection in this fixture.
    // 替换账号是本夹具唯一配置的连接。
    let mut configuration = bridge.public_ai_runtime.configuration_snapshot().unwrap();
    configuration
        .connection_graph
        .add_connection(
            &bridge.public_ai_runtime.providers().unwrap(),
            vulcan_provider_adapters::CredentialConnection {
                id: "replacement-account".into(),
                provider_id: "opencode-go".into(),
                service_entry_id: "go-api".into(),
                credential_profile_id: "api-key".into(),
                credential_mode: vulcan_provider_adapters::CredentialMode::Token,
                credential_id: "replacement-credential".into(),
                display_name: "Replacement account".into(),
                secret_reference: "compaction-test-secret".into(),
                management_secret_reference: None,
                service_origin_override: None,
            },
        )
        .unwrap();
    configuration.set_model_enabled("opencode-go", "deepseek-v4.1-flash", true);
    bridge
        .public_ai_runtime
        .replace_configuration(configuration)
        .unwrap();
    // Persist the former model through ordinary root admission as it was saved before removal.
    // 经普通根输入准入持久化删除前保存的旧模型。
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("manual-removed-account").unwrap(),
                "Previous request on the removed account",
                Vec::new(),
            )
            .with_bound_request_config(
                vulcan_session_core::v1::config::ActiveRequestConfig {
                    provider: Some("anthropic".into()),
                    connection_id: Some("removed-account".into()),
                    model: Some("claude-opus-5-5".into()),
                    thinking_level: Some("high".into()),
                    temperature: None,
                },
            ),
            4,
        )
        .unwrap();
    seed_manual_replacement_history(&handle, borrowed);
    crate::session_request_config_v1::set_pending_model_request_v1(
        handle.as_ref(),
        "opencode-go".into(),
        "replacement-account".into(),
        "deepseek-v4.1-flash".into(),
        Some(ReasoningEffortLevel::Low),
        crate::session_request_config_v1::SessionRequestFallback::default(),
        crate::current_v1_unix_millis().unwrap(),
    )
    .unwrap();
    // Only the preinstalled test owner is removed before starting the real worker.
    // 启动真实工作任务前仅移除夹具预装的所有者。
    let fixture_task = crate::remove_active_stream_task_if_id(
        &mut state.lock().unwrap(),
        handle.session_id(),
        fixture_task_id,
    )
    .unwrap();
    fixture_task.abort_and_wait().await.unwrap();
    state.lock().unwrap().agent_runtime = Some(bridge.clone());
    // The production planner captures the pending route and the history in one revision.
    // 生产规划器在同一修订捕获待生效路由及历史。
    let (planned, target) =
        plan_manual_webui_context_compaction(&state, &bridge, handle.session_id()).unwrap();
    // Use the worker's ordinary status row and provider pipeline.
    // 使用工作任务的常规状态行及供应商链路。
    let status = append_webui_context_compaction_status_message_to_workspace(
        state.lock().unwrap().workspace_snapshot.as_mut().unwrap(),
        handle.session_id(),
        "",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
    )
    .unwrap();
    // The registered worker owns transport, durable API events and compaction commit.
    // 已登记工作任务拥有传输、可靠 API 事件及压缩提交。
    let app = ServiceEventEmitter::new(Vec::new());
    // Reuse the storage and statistics owner already attached to the fixture.
    // 复用夹具已连接的存储及统计所有者。
    let controller = state.lock().unwrap().controller_runtime.clone().unwrap();
    spawn_webui_context_compaction_task(
        app,
        state.clone(),
        controller,
        bridge.clone(),
        handle.session_id(),
        SessionRunPhase::Ready,
        status,
        String::new(),
        MANUAL_CONTEXT_COMPACTION_TRIGGER.into(),
        planned.plan,
        planned.full_context_state,
        planned.media_projection,
        planned.retained_from_seq,
        planned.source,
        target,
    )
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while state
            .lock()
            .unwrap()
            .active_stream_tasks
            .contains_key(&handle.session_id())
        {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("manual compaction must settle");
    {
        // The only physical request must reach the exact selected model.
        // 唯一物理请求必须到达精确选择的模型。
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            1,
            "manual compaction must reach the replacement transport"
        );
        assert_eq!(requests[0]["model"], "deepseek-v4.1-flash");
    }
    assert!(
        bridge
            .public_ai_runtime
            .session_selection_lease(&handle.session_id().to_string())
            .unwrap()
            .is_none()
    );
    // A cold reload verifies the summary and request route reached durable storage.
    // 冷加载验证摘要及请求路由已可靠落盘。
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_for_runtime(
        &root.v1_storage_locator(),
    )
    .unwrap();
    {
        // Both business and pending request metadata remain owned by ordinary admission.
        // 业务及待生效请求元数据仍归普通准入持有。
        let document = &loaded.document;
        assert!(document.runtime_lock.current_compaction_id.is_some());
        assert_eq!(
            document.active_turn().is_some_and(|turn| !turn.closed),
            borrowed
        );
        assert!(document.events().iter().any(|event| matches!(
            &event.event, EventBody::ApiStarted(api)
                if api.purpose == ApiPurposeKind::Compaction
                    && api.request.connection_id.as_deref() == Some("replacement-account")
                    && api.request.model == "deepseek-v4.1-flash"
        )));
        assert_eq!(
            document
                .metadata
                .config
                .active_request
                .connection_id
                .as_deref(),
            Some("removed-account"),
        );
        assert_eq!(
            document
                .metadata
                .config
                .pending_request
                .as_ref()
                .unwrap()
                .connection_id
                .as_deref(),
            Some("replacement-account"),
        );
    }
    server.shutdown().await;
}

/// Seeds replayable history on the removed route using `handle`; leaves the business open when `borrowed`.
/// 使用 `handle` 在已删除路由上播种可重放历史，`borrowed` 时保留开放业务。
/// Returns after publishing enough real API segments for the production planner to compact.
/// 发布足够的真实 API 执行段供生产规划器压缩后返回。
fn seed_manual_replacement_history(handle: &SessionHandle, borrowed: bool) {
    use vulcan_session_core::v1::{
        context::{
            LockedBaselineProjection, LockedBaselineSegmentInput, LockedPromptTarget,
            LockedSourceKind,
        },
        document::{ApiCompletion, PrepareAndStartApiInput},
        event::{
            ApiCallStatus, ApiEndReason, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
            AssistantSemanticStatus, ContinuationEligibility, ProviderProjectionProof, TurnOutcome,
        },
        projection::provider_visible_fingerprint,
    };

    // Real historical requests establish the locked host baseline.
    // 真实历史请求建立锁定的宿主基线。
    let baseline = LockedBaselineProjection::project(vec![LockedBaselineSegmentInput {
        kind: LockedSourceKind::BaselineHooks,
        node_id: "host-message-guidance".into(),
        target: LockedPromptTarget::System,
        source_label: Some("host_message_guidance".into()),
        source_id: Some("v1:host_message_guidance".into()),
        content: vulcan_ai_contract::HOST_MESSAGE_GUIDANCE.into(),
    }])
    .unwrap();
    // The old request facts remain the business authority after compaction.
    // 压缩后原请求事实仍为业务权威。
    let request = ApiRequestSnapshot {
        connection_id: Some("removed-account".into()),
        thinking_level: Some("high".into()),
        ..ApiRequestSnapshot::new("anthropic", "claude-opus-5-5")
    };
    // Exceed the real retained segment ceiling without hardcoding that policy.
    // 超过真实保留执行段上限，不硬编码该策略。
    let rounds = vulcan_context::CONTEXT_COMPACTION_MAX_TAIL_SEGMENTS + 1;
    handle.mutate_durable(
        SessionMutationAdmission::ExistingTurn,
        SessionFlushReason::MetadataChanged,
        |document| document.apply_atomic_transition(|document| {
            for round in 0..rounds {
                // Each historical request has its own chronological projection frontier.
                // 每个历史请求具有独立的时序投影前沿。
                let now = 6 + i64::try_from(round).unwrap() * 4;
                let through_seq = document.events().last().unwrap().seq;
                let api = document.prepare_and_start_api(PrepareAndStartApiInput {
                    purpose: ApiPurposeKind::MainGeneration,
                    request: request.clone(),
                    request_budget_evidence:
                        crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                    projection: ProviderProjectionProof {
                        segment_generation: document.segment_generation(),
                        document_revision: document.memory_revision(),
                        retained_from_seq: document.events().first().unwrap().seq,
                        through_seq,
                        fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)?,
                    },
                    locked_baseline: baseline.clone(),
                    continuation_action_id: None,
                    now_ms: now,
                })?.into_value().api_call_id;
                document.append_assistant_text(api, "Original review evidence and work items.",
                    AssistantSemanticStatus::Completed, now + 1)?;
                document.complete_api_group(api, ApiCompletion {
                    request: request.clone(),
                    usage: Default::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: now + 2,
                })?;
            }
            if !borrowed {
                document.close_turn(TurnOutcome::Completed, 5, 6 + i64::try_from(rounds).unwrap() * 4)?;
            }
            Ok(())
        }),
    ).unwrap();
}

/// Verifies the real insert RPC defers one exact queue identity during independent maintenance.
/// 验证真实插队 RPC 在独立维护期间延后一个精确排队身份。
/// Returns nothing; duplicate clicks and summary failure leave the durable queue untouched.
/// 不返回值；重复点击及摘要失败均保留可靠队列。
#[tokio::test]
async fn manual_compaction_insertion_rpc_defers_and_deduplicates_until_success() {
    // fixture wires the exact service, storage, and task owner used by the public RPC.
    // fixture 连接公开 RPC 使用的精确服务、存储及任务所有者。
    let (_root, state, handle, task_id) = fixture().await;
    // session_id, source, and mode bind the independent maintenance turn.
    // session_id、source 与 mode 绑定独立维护回合。
    let session_id = handle.session_id();
    let source = handle
        .read(|document| document.capture_manual_context_maintenance_source())
        .unwrap()
        .unwrap();
    let mode = begin(&state, session_id, task_id, source).await.unwrap();
    state.lock().unwrap().workbench.selected_conversation_id = Some(session_id.to_string());
    // action is the client identity that remains queued until the summary commits.
    // action 是在摘要提交前保持排队的客户端身份。
    let action = ClientActionId::try_new("manual-deferred-insertion").unwrap();
    handle
        .enqueue_input(
            ClientInputSubmission::new(action.clone(), "After the summary", Vec::new()),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap();
    // app has no external subscriber and captures only this fixture's events.
    // app 没有外部订阅方，仅捕获此夹具的事件。
    let app = ServiceEventEmitter::new(Vec::new());
    for _ in 0..2 {
        assert!(
            crate::service_insert_pending_prompt(
                app.clone(),
                state.clone(),
                action.as_str().to_owned()
            )
            .await
            .unwrap()
        );
    }
    assert_eq!(
        state.lock().unwrap().active_stream_tasks[&session_id]
            .deferred_compaction_insertions
            .len(),
        1
    );
    assert!(
        handle
            .read(|document| document
                .metadata
                .input_state
                .pending_in_turn_inputs
                .is_empty())
            .unwrap()
    );
    assert!(matches!(
        mode,
        ContextCompactionCommitMode::FinishStandalone { .. }
    ));
    crate::context_compaction::fail_webui_context_compaction_execution(
        &app,
        &state,
        session_id,
        SessionRunPhase::Idle,
        "manual-deferred-status",
        "",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        mode,
        "Synthetic summary rejection",
    )
    .unwrap();
    assert!(
        !state
            .lock()
            .unwrap()
            .active_stream_tasks
            .contains_key(&session_id)
    );
    assert!(
        handle
            .read(|document| document
                .metadata
                .input_state
                .queued_inputs
                .iter()
                .any(|entry| entry.client_action_id == action))
            .unwrap()
    );
}

/// Registers an idle failed business session and a separate pending manual stream in isolated storage.
/// 在隔离存储中登记空闲失败业务会话及独立待执行手动流。
/// Returns the storage guard, service state, canonical handle and exact stream owner.
/// 返回存储守卫、服务状态、规范句柄和精确流所有者。
async fn fixture() -> (
    TestHostRuntimeRoot,
    SharedServiceBridgeState,
    Arc<SessionHandle>,
    Uuid,
) {
    // The fixture owns every file and never opens the user's Release session.
    // 夹具拥有全部文件，绝不打开用户 Release 会话。
    let session_id = Uuid::new_v4();
    // Keep the isolated storage alive until all handles and assertions are finished.
    // 保持隔离存储存活，直到全部句柄和断言结束。
    let root = TestHostRuntimeRoot::new(session_id);
    // Attach only the fixture's controller, never the running Release controller.
    // 仅附加夹具控制器，绝不附加运行中的 Release 控制器。
    let controller = root.attached_host_runtime_handle().await;
    // The parent-session fixture supplies production registry and workspace wiring.
    // 父会话夹具提供生产注册表和工作区连接。
    let mut service = test_service_state_with_parent_session(session_id);
    service.controller_runtime = Some(controller);
    // Share the same service authority used by admission and terminal publication.
    // 共享准入和终态发布使用的同一服务权威。
    let state = Arc::new(Mutex::new(service));
    // Load through the real registered session-store path.
    // 通过真实已登记会话存储路径加载。
    let handle = crate::session_goal_lifecycle::session_v1_handle_from_state(&state, session_id)
        .await
        .unwrap();
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("manual-root-input").unwrap(),
                "Retained user task",
                Vec::new(),
            ),
            2,
        )
        .unwrap();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.fail_execution_boundary_and_close_turn(
                    "fixture_business_failed",
                    Some("Fixture failure"),
                    3,
                )
            },
        )
        .unwrap();
    // Kernel ownership excludes new execution while Core owns the separate auxiliary turn.
    // 在 Core 拥有独立辅助回合时，内核归属排除新的执行。
    {
        // Initialize process-local execution ownership under the service lock.
        // 在服务锁内初始化进程内执行归属。
        let mut guard = state.lock().unwrap();
        guard
            .workspace_snapshot
            .as_mut()
            .unwrap()
            .session_runtime_state
            .insert(session_id, SessionRuntimeStateEntry::default());
        crate::start_webui_session_kernel_turn(
            &mut guard,
            session_id,
            "manual-fixture",
            ActiveTurnKind::ContextCompaction,
        )
        .unwrap();
    }
    // Each attempt receives a distinct physical stream identity.
    // 每次尝试获得不同物理流身份。
    let task_id = Uuid::new_v4();
    crate::spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        task_id,
        None,
        crate::ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        std::future::pending::<()>(),
    )
    .unwrap();
    (root, state, handle, task_id)
}

/// A physically idle open business turn remains available to explicit manual compaction.
/// 物理空闲但仍开放的业务回合应允许显式手动压缩。
#[tokio::test]
async fn manual_compaction_admits_open_business_without_closing_it() {
    let (_root, state, handle, task_id) = fixture().await;
    let session_id = handle.session_id();
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("open-manual-business").unwrap(),
                "Retained unfinished work",
                Vec::new(),
            ),
            4,
        )
        .unwrap();
    let before = handle
        .read_full_durable(|document| {
            serde_json::to_value((&document.metadata, document.events())).unwrap()
        })
        .unwrap();
    let (_, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .unwrap();
    begin(&state, session_id, task_id, source)
        .await
        .expect("manual compaction must not reject only an open business journal");
    assert_eq!(
        handle
            .read_full_durable(|document| {
                serde_json::to_value((&document.metadata, document.events())).unwrap()
            })
            .unwrap(),
        before
    );
    let worker = state
        .lock()
        .unwrap()
        .active_stream_tasks
        .remove(&session_id)
        .unwrap();
    worker.abort_and_wait().await.unwrap();
}

/// Pre-dispatch failure is a durable UI event, closes the exact Core owner and releases the kernel slot.
/// 分发前失败成为可靠界面事件，闭合精确 Core 所有者并释放内核槽位。
/// Returns nothing; verifies error correlation and reloadable disk facts without any Provider network call.
/// 不返回值；验证错误关联及可重载磁盘事实，不调用任何供应商网络。
#[tokio::test]
async fn manual_compaction_failure_publishes_visible_event_and_closes_owner() {
    // Establish actual storage and stream registration through production helpers.
    // 通过生产辅助函数建立真实存储和流登记。
    let (root, state, handle, task_id) = fixture().await;
    // The canonical handle supplies this attempt's session identity.
    // 规范句柄提供本次尝试的会话身份。
    let session_id = handle.session_id();
    // Capture the source through the same atomic planning path used by the slash command.
    // 通过与斜杠命令相同的原子规划路径捕获来源。
    let (_, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .unwrap();
    begin(&state, session_id, task_id, source).await.unwrap();
    // Error correlation must point at maintenance, never the preceding business turn.
    // 错误关联必须指向维护，绝不指向先前业务回合。
    let maintenance_turn = handle
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    super::super::completion::fail_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        session_id,
        SessionRunPhase::Ready,
        "manual-section",
        "",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishStandalone { task_id },
        "Fixture provider preparation failed",
    )
    .unwrap();
    handle.read(|document| {
        assert!(document.active_turn().is_none_or(|turn| turn.closed));
        assert!(document.active_api().is_none());
        assert!(document.events().iter().any(|event| event.turn_id == Some(maintenance_turn)
            && event.visibility == vulcan_session_core::v1::event::EventVisibility::UiOnly
            && matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "context_compaction_failed")));
        assert!(!document.events().iter().any(|event| matches!(event.event, EventBody::ApiStarted(_))));
    }).unwrap();
    assert!(
        !state
            .lock()
            .unwrap()
            .active_stream_tasks
            .contains_key(&session_id)
    );
    assert!(!crate::webui_session_kernel_has_active_turn(
        &state.lock().unwrap(),
        session_id
    ));
    // The same UI fact is stored as an event, not merely in the old workbench message vector.
    // 同一界面事实作为事件存储，不只是写入旧工作台消息数组。
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_for_runtime(
        &root.v1_storage_locator(),
    )
    .unwrap();
    assert!(loaded.document.events().iter().any(|event| matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "context_compaction_failed")));
    // Once this owner is terminal, even a repeated callback cannot close a later real business turn.
    // 本所有者终结后，即使重复回调也不能闭合后来真实开启的业务回合。
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("later-business").unwrap(),
                "New business",
                Vec::new(),
            ),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap();
    // Record the exact post-submission revision to detect any stale terminal mutation.
    // 记录提交后的精确修订，以检测任何过期终态修改。
    let later_revision = handle.read(|document| document.durable_revision()).unwrap();
    fail(&state, session_id, task_id, "Duplicate fixture failure").unwrap();
    handle
        .read(|document| {
            assert_eq!(document.durable_revision(), later_revision);
            assert!(document.active_turn().is_some_and(|turn| !turn.closed));
        })
        .unwrap();
}

/// A real manual start admits a compaction API after the prior business was closed.
/// 先前业务闭合后，真实手动起始能够准入压缩 API。
/// Returns nothing; proves the regression no longer reaches Core with NoActiveTurn and keeps history unchanged.
/// 不返回值；证明回归不再以 NoActiveTurn 到达 Core，且保持历史不变。
#[tokio::test]
async fn manual_compaction_can_prepare_a_physical_api_from_idle_history() {
    use vulcan_session_core::v1::{
        context::LockedBaselineProjection,
        document::PrepareAndStartApiInput,
        event::{ApiPurposeKind, ApiRequestSnapshot, ProviderProjectionProof},
        projection::provider_visible_fingerprint,
    };
    // Each test has its own published document and kernel generation.
    // 每个测试拥有独立已发布文档与内核代次。
    let (_root, state, handle, task_id) = fixture().await;
    // Session identity remains bound to the registered handle.
    // 会话身份保持绑定已登记句柄。
    let session_id = handle.session_id();
    // Freeze history before the slash-command reset persists its composer-only changes.
    // 在斜杠命令重置持久化仅输入框变化之前冻结历史。
    let (_, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .unwrap();
    handle
        .set_draft(
            "/compact".to_owned(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap();
    handle
        .clear_draft(crate::current_v1_unix_millis().unwrap())
        .unwrap();
    begin(&state, session_id, task_id, source).await.unwrap();
    // Build proof from the same real event frontier used by normal physical admission.
    // 从正常物理准入使用的同一真实事件前沿构造证明。
    let proof = handle
        .read(|document| {
            // The current frontier includes the auxiliary owner but no synthetic user input.
            // 当前前沿包含辅助所有者，但不包含合成用户输入。
            let through_seq = document.events().last().unwrap().seq;
            ProviderProjectionProof {
                segment_generation: document.segment_generation(),
                document_revision: document.memory_revision(),
                retained_from_seq: document.events().first().unwrap().seq,
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .unwrap(),
            }
        })
        .unwrap();
    // Prepare a physical compaction API through the same durable registry admission as production.
    // 通过与生产相同的可靠注册表准入准备物理压缩 API。
    let started = handle
        .prepare_and_start_api_durable(PrepareAndStartApiInput {
            purpose: ApiPurposeKind::Compaction,
            request: ApiRequestSnapshot::new("provider", "model"),
            request_budget_evidence:
                crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
            projection: proof,
            locked_baseline: LockedBaselineProjection::empty(),
            continuation_action_id: None,
            now_ms: crate::current_v1_unix_millis().unwrap(),
        })
        .unwrap();
    assert!(started.changed);
    handle
        .read(|document| {
            assert_eq!(
                document.active_api().unwrap().purpose,
                ApiPurposeKind::Compaction
            );
            assert_eq!(
                document
                    .events()
                    .iter()
                    .filter(|event| matches!(event.event, EventBody::UserInput(_)))
                    .count(),
                1
            );
        })
        .unwrap();
    super::super::completion::fail_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        session_id,
        SessionRunPhase::Ready,
        "manual-section",
        "",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishStandalone { task_id },
        "Fixture ends after verified dispatch admission",
    )
    .unwrap();
}

/// A rejected summary must use the same durable failure boundary as a Provider error.
/// 被拒绝的摘要必须与供应商错误使用同一可靠失败边界。
/// Returns nothing; verifies a fallback never commits a summary or leaves an auxiliary owner open.
/// 不返回值；验证回退绝不提交摘要或遗留开放辅助所有者。
#[tokio::test]
async fn manual_compaction_rejected_summary_closes_owner_and_preserves_history() {
    use crate::agent_stream::provider_v1_sink::ProviderV1FinishedDurability;
    use vulcan_agent_runtime::agent_runtime::prompt_compaction::{
        PromptCompactionExecution, PromptCompactionExecutionProfile,
    };
    use vulcan_context_compaction::{CompactionResult, CompactionTailSelectionReport};

    // The production fixture owns the actual idle history and active stream slot.
    // 生产夹具拥有真实空闲历史和活动流槽位。
    let (_root, state, handle, task_id) = fixture().await;
    // Session identity is derived from its canonical handle.
    // 会话身份从其规范句柄派生。
    let session_id = handle.session_id();
    // The atomic snapshot preserves the source used by auxiliary admission.
    // 原子快照保留辅助准入使用的来源。
    let (_, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .unwrap();
    begin(&state, session_id, task_id, source).await.unwrap();
    // A rejected candidate is structurally valid so rejection tests the real terminal path.
    // 被拒绝候选具有有效结构，使拒绝测试覆盖真实终态路径。
    let policy = ContextCompactionPolicy::default();
    // No source is replaced because fallback outcomes cannot publish compaction nodes.
    // 回退结果不能发布压缩节点，因此没有任何来源被替换。
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: "rejected-fixture".to_owned(),
        created_at: "1".to_owned(),
        policy,
        tail_selection: CompactionTailSelectionReport {
            context_limit: policy.tail_context_limit,
            policy_source: policy.tail_policy_source,
            nominal_token_budget: policy.preserve_recent_tokens,
            segment_limit: policy.tail_turns,
            retained_segment_count: 0,
            retained_message_ids: Vec::new(),
            retained_token_estimate: 0,
            previewed_tool_output_count: 0,
            omitted_tool_output_token_estimate: 0,
            newest_segment_exceeds_nominal_budget: false,
        },
    };
    // Call the same completion boundary used by the standalone stream driver.
    // 调用独立流驱动使用的同一完成边界。
    let outcome = finish_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &crate::tests::test_support::test_agent_runtime_bridge(),
        session_id,
        SessionRunPhase::Ready,
        "rejected-section",
        "",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishStandalone { task_id },
        WebuiContextCompactionExecution {
            execution: PromptCompactionExecution {
                compaction_result: CompactionResult::from_model_summary(
                    &plan,
                    "Unusable fixture summary",
                ),
                fallback_used: true,
                provider_request_count: 1,
                last_successful_provider_usage: None,
                provider_id: None,
                model_id: None,
                execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
                input_prune_report: None,
            },
            retained_from_seq: Seq::try_new(1).unwrap(),
            durability: ProviderV1FinishedDurability::default(),
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        outcome,
        ContextCompactionFinishOutcome::Rejected { .. }
    ));
    handle.read(|document| {
        assert!(document.runtime_lock.current_compaction_id.is_none());
        assert!(document.active_turn().is_none_or(|turn| turn.closed));
        assert!(document.metadata.context_maintenance.as_ref().unwrap().terminal.is_some());
        assert!(document.events().iter().any(|event| matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "context_compaction_failed")));
        assert_eq!(document.events().iter().filter(|event| matches!(event.event, EventBody::UserInput(_))).count(), 1);
    }).unwrap();
    assert!(
        !state
            .lock()
            .unwrap()
            .active_stream_tasks
            .contains_key(&session_id)
    );
    assert!(!crate::webui_session_kernel_has_active_turn(
        &state.lock().unwrap(),
        session_id
    ));
}

/// Admission rejection is visible without closing a business turn that won the revision race.
/// 准入拒绝保持可见，且不会闭合赢得修订竞争的业务回合。
/// Returns nothing; verifies stale preparation cannot dispatch, relabel history or block the kernel slot.
/// 不返回值；验证过期准备不能分发、重新归属历史或阻塞内核槽位。
#[tokio::test]
async fn manual_compaction_stale_admission_keeps_new_business_and_reports_failure() {
    // Begin from the same isolated state as a real user-triggered compaction request.
    // 从与真实用户触发压缩请求相同的隔离状态开始。
    let (_root, state, handle, task_id) = fixture().await;
    // This handle owns the only authoritative session identity.
    // 此句柄拥有唯一权威会话身份。
    let session_id = handle.session_id();
    // Freeze the old source before another accepted input changes the canonical history.
    // 在另一个已接纳输入改变规范历史前冻结旧来源。
    let (_, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .unwrap();
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("racing-business").unwrap(),
                "Racing business",
                Vec::new(),
            ),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap();
    // Capture the genuine turn so failed maintenance cannot silently replace it.
    // 捕获真实回合，使失败维护不能静默替换它。
    let business_turn = handle
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    // Exercise the production rejection rather than manufacturing a failed status row.
    // 执行生产拒绝路径，不制造失败状态行。
    let error = begin(&state, session_id, task_id, source)
        .await
        .unwrap_err();
    fail_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        session_id,
        SessionRunPhase::Ready,
        "stale-section",
        "",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishStandalone { task_id },
        &error,
    )
    .unwrap();
    handle.read(|document| {
        assert!(document.active_turn().is_some_and(|turn| turn.turn_id == business_turn && !turn.closed));
        assert!(document.metadata.context_maintenance.is_none());
        assert!(document.events().iter().any(|event| event.turn_id.is_none() && matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "context_compaction_failed")));
        assert!(!document.events().iter().any(|event| matches!(event.event, EventBody::ApiStarted(_))));
    }).unwrap();
    assert!(
        !state
            .lock()
            .unwrap()
            .active_stream_tasks
            .contains_key(&session_id)
    );
}

/// Cold draft hydration recovers interrupted manual maintenance before exposing the canonical session handle.
/// 冷草稿加载在暴露规范会话句柄前恢复已中断手动维护。
/// Returns nothing; checks the real service read/write path, durable recovery, visible failure and new input admission.
/// 不返回值；检查真实服务读写路径、可靠恢复、可见失败与新输入准入。
#[tokio::test]
async fn manual_compaction_restart_restores_draft_service_and_new_input() {
    // Build current-format storage from its first session creation, with no legacy migration dependency.
    // 从首次会话创建起构造当前格式存储，不依赖旧版迁移。
    let (root, state, handle, task_id) = fixture().await;
    // The same explicit session identity must survive the simulated process replacement.
    // 同一明确会话身份必须在模拟进程替换后保留。
    let session_id = handle.session_id();
    // The manual command's planned source is captured through the production registry.
    // 手动命令规划来源通过生产注册表捕获。
    let (_, source) = handle
        .capture_context_compaction_planning_snapshot_with_source()
        .unwrap();
    begin(&state, session_id, task_id, source).await.unwrap();
    handle
        .set_draft(
            "Retained unsent text".to_owned(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap();
    // Stop only the fixture worker, without manufacturing a durable completion before restart.
    // 仅停止夹具工作器，不在重启前制造可靠完成事实。
    let task = state
        .lock()
        .unwrap()
        .active_stream_tasks
        .remove(&session_id)
        .unwrap();
    task.abort_and_wait().await.unwrap();
    // Reuse the fixture's existing database binding; attaching it again would create the same indexed session twice.
    // 复用夹具现有数据库绑定；再次附加会重复创建同一索引会话。
    let controller = state.lock().unwrap().controller_runtime.clone().unwrap();
    drop(handle);
    drop(state);
    // A fresh service owns an empty registry and must hydrate entirely from the published files.
    // 新服务拥有空注册表，必须完全从已发布文件加载。
    let mut restarted = test_service_state_with_parent_session(session_id);
    restarted.controller_runtime = Some(controller);
    // This is the actual browser-facing draft service, not a direct Core-only load.
    // 这是实际面向浏览器的草稿服务，而非仅直接加载 Core。
    let restarted = Arc::new(Mutex::new(restarted));
    // Reading must recover the maintenance owner before returning the original draft.
    // 读取必须在返回原草稿前恢复维护所有者。
    let draft = crate::conversation_drafts_v1::service_session_draft_v1(
        Arc::clone(&restarted),
        vulcan_service_api::SessionDraftReadV1Request {
            session_id: session_id.to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(draft.text, "Retained unsent text");
    // Disk verification proves recovery was published before the UI was allowed to send again.
    // 磁盘校验证明恢复在界面重新允许发送前已经发布。
    let recovered = vulcan_workspace_store::workspace_store::load_session_v1_for_runtime(
        &root.v1_storage_locator(),
    )
    .unwrap();
    assert!(recovered.document.active_turn().is_none());
    assert!(recovered.document.active_api().is_none());
    assert!(
        recovered
            .document
            .metadata
            .context_maintenance
            .as_ref()
            .unwrap()
            .terminal
            .is_some()
    );
    assert_eq!(recovered.document.events().iter().filter(|event| matches!(&event.event,
        EventBody::DisplayNote(note) if note.reason_code == "context_maintenance_process_interrupted"
    )).count(), 1);
    // Saving after authoritative hydration must use the normal draft endpoint successfully.
    // 权威加载后保存必须成功使用正常草稿端点。
    let saved = crate::conversation_drafts_v1::service_set_session_draft_v1(
        Arc::clone(&restarted),
        vulcan_service_api::SessionDraftWriteV1Request {
            session_id: session_id.to_string(),
            text: "New user input".to_owned(),
            attachments: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert_eq!(saved.text, "New user input");
    // The recovered registered handle admits a genuine root turn without a Provider call.
    // 已恢复的登记句柄无需调用供应商即可接纳真实根回合。
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&restarted, session_id)
            .await
            .unwrap();
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("after-cold-maintenance-recovery").unwrap(),
                saved.text,
                Vec::new(),
            ),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap();
    handle
        .read(|document| {
            assert_eq!(
                document.active_turn().unwrap().origin,
                vulcan_session_core::v1::document::TurnOrigin::UserRoot
            );
            assert!(document.active_api().is_none());
        })
        .unwrap();
}

/// Three failures stop only the original stream; a new user retry receives a fresh automatic compaction budget.
/// 三次失败只停止原流；用户的新重试获得新的自动压缩额度。
/// Returns nothing; verifies new-owner isolation and failure accounting across the real workspace mutation path.
/// 不返回值；通过真实工作区修改路径验证新所有者隔离和失败计数。
#[test]
fn auto_compaction_retry_after_three_failures_uses_new_stream_budget() {
    // Both attempts target the same session but have distinct execution ownership.
    // 两次尝试针对同一会话，但拥有不同执行归属。
    let session_id = Uuid::new_v4();
    // This identity accumulates the original execution's failures.
    // 此身份累计原执行的失败。
    let first_task = Uuid::new_v4();
    // Explicit retry has independent ownership even within the same session.
    // 即使在同一会话中，显式重试也拥有独立归属。
    let retry_task = Uuid::new_v4();
    // Retained runtime state is the production authority for failure accounting.
    // 保留运行态是失败计数的生产权威。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    for _ in 0..MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES {
        // Each failed attempt mutates the real workspace under its service lock.
        // 每次失败尝试在服务锁内修改真实工作区。
        let mut guard = state.lock().unwrap();
        super::super::completion::apply_webui_context_compaction_failure_to_workspace(
            guard.workspace_snapshot.as_mut().unwrap(),
            session_id,
            SessionRunPhase::Ready,
            "auto-section",
            "",
            AUTO_CONTEXT_COMPACTION_TRIGGER,
            first_task,
            "Fixture invalid summary",
        )
        .unwrap();
    }
    assert_eq!(
        webui_auto_context_compaction_failure_count(&state, session_id, first_task),
        MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES
    );
    assert_eq!(
        webui_auto_context_compaction_failure_count(&state, session_id, retry_task),
        0
    );
    {
        // A new failure replaces the counter's owner instead of inheriting the old limit.
        // 新失败替换计数所有者，而非继承旧限制。
        let mut guard = state.lock().unwrap();
        super::super::completion::apply_webui_context_compaction_failure_to_workspace(
            guard.workspace_snapshot.as_mut().unwrap(),
            session_id,
            SessionRunPhase::Ready,
            "retry-section",
            "",
            AUTO_CONTEXT_COMPACTION_TRIGGER,
            retry_task,
            "Fixture invalid summary",
        )
        .unwrap();
    }
    assert_eq!(
        webui_auto_context_compaction_failure_count(&state, session_id, retry_task),
        1
    );
    assert_eq!(
        webui_auto_context_compaction_failure_count(&state, session_id, first_task),
        0
    );
}
