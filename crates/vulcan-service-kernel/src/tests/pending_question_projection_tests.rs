//! Pending confirmation projection tests.
//! 待确认状态投影测试。

use super::*;
use vulcan_agent_runtime::agent_runtime::question_runtime::{QuestionPrompt, QuestionReply};
use vulcan_service_api::ServiceEventSink;
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, ToolCallId};

/// Captures service events delivered to one simulated client endpoint.
/// 捕获投递到单个模拟客户端端点的服务事件。
struct CapturingServiceEventSink {
    /// Shared ordered event buffer owned by this simulated client.
    /// 由该模拟客户端持有的共享有序事件缓冲区。
    events: Arc<Mutex<Vec<ServiceEvent>>>,
}

impl ServiceEventSink for CapturingServiceEventSink {
    /// Stores one cloned service event in this client's independent buffer.
    /// 将单条克隆后的服务事件保存到该客户端的独立缓冲区。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the service event delivered by the shared emitter.
    /// `event` 是共享事件发送器投递的服务事件。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events
            .lock()
            .expect("capturing service-event buffer should lock")
            .push(event);
    }
}

/// Returns generations for pending-question deltas captured by one simulated client.
/// 返回单个模拟客户端捕获的待回答问题增量 generation。
///
/// # Parameters
/// 参数
/// `events` is the shared event buffer to inspect.
/// `events` 是要检查的共享事件缓冲区。
///
/// # Returns
/// 返回值
/// Ordered generations belonging only to pending-question deltas.
/// 仅属于待回答问题增量的有序 generation 集合。
fn pending_question_delta_generations(events: &Arc<Mutex<Vec<ServiceEvent>>>) -> Vec<u64> {
    events
        .lock()
        .expect("capturing service-event buffer should lock")
        .iter()
        .filter_map(|event| match event {
            ServiceEvent::WorkbenchDelta(envelope)
                if matches!(&envelope.delta, WorkbenchDelta::PendingQuestionChanged(_)) =>
            {
                Some(envelope.generation)
            }
            _ => None,
        })
        .collect()
}

/// Returns generations for session-addon deltas captured by one simulated client.
/// 返回单个模拟客户端捕获的会话附属状态增量 generation。
///
/// # Parameters
/// 参数
/// `events` is the shared event buffer to inspect.
/// `events` 是要检查的共享事件缓冲区。
///
/// # Returns
/// 返回值
/// Ordered generations belonging only to session-addon state deltas.
/// 仅属于会话附属状态增量的有序 generation 集合。
fn session_addon_delta_generations(events: &Arc<Mutex<Vec<ServiceEvent>>>) -> Vec<u64> {
    events
        .lock()
        .expect("capturing service-event buffer should lock")
        .iter()
        .filter_map(|event| match event {
            ServiceEvent::WorkbenchDelta(envelope)
                if matches!(&envelope.delta, WorkbenchDelta::SessionAddonsChanged(_)) =>
            {
                Some(envelope.generation)
            }
            _ => None,
        })
        .collect()
}

/// Starts one pending Ask request with explicit V1 local ownership identities.
/// 使用明确的 V1 本地所有权身份启动一条待回答 Ask 请求。
///
/// `session_id`, `api_call_id`, and `tool_call_id` form the authoritative local identity;
/// `provider_tool_call_id` is optional correlation data, and `question_text` is display text.
/// `session_id`、`api_call_id` 与 `tool_call_id` 组成权威本地身份；
/// `provider_tool_call_id` 仅为可选关联数据，`question_text` 是展示文本。
/// Returns the suspended Ask task and the registered request identifier.
/// 返回挂起的 Ask 任务与已注册的请求标识。
async fn start_pending_question(
    question_runtime: Arc<QuestionRuntime>,
    session_id: SessionId,
    api_call_id: ApiCallId,
    tool_call_id: ToolCallId,
    provider_tool_call_id: &str,
    question_text: &str,
) -> (
    tokio::task::JoinHandle<Result<QuestionReply, String>>,
    String,
) {
    // promptText owns the question text across the spawned asynchronous task boundary.
    // promptText 跨越异步任务边界持有问题文本。
    let prompt_text = question_text.to_string();
    // providerToolCallId owns the exact correlation identity across the spawned task boundary.
    // providerToolCallId 跨越异步任务边界持有精确的关联身份。
    let provider_tool_call_id = provider_tool_call_id.to_string();
    // controllerToolCallId owns the exact runtime result-envelope identity.
    // controllerToolCallId 持有精确的运行时结果信封身份。
    let controller_tool_call_id = format!("controller-{provider_tool_call_id}");
    // requestStreamId owns the exact Provider stream item bound to this local tool fact.
    // requestStreamId 持有与本地工具事实绑定的精确 Provider 流项目。
    let request_stream_id =
        RequestStreamId::try_new(format!("pending-question-stream-{provider_tool_call_id}"))
            .expect("pending question request-stream identity");
    // askRuntime shares the exact registry installed into the service state.
    // askRuntime 共享安装到服务状态中的同一注册表。
    let ask_runtime = Arc::clone(&question_runtime);
    // askTask owns the suspended ask future until reply, rejection, or cancellation.
    // askTask 持有挂起的 ask future，直到回答、拒绝或取消。
    let ask_task = tokio::spawn(async move {
        ask_runtime
            .ask(
                session_id,
                request_stream_id,
                api_call_id,
                tool_call_id,
                controller_tool_call_id,
                Some(provider_tool_call_id),
                String::from("ask"),
                serde_json::json!({"questions": []}),
                vec![QuestionPrompt {
                    question: prompt_text,
                    header: String::from("Confirm"),
                    options: Vec::new(),
                    multiple: Some(false),
                }],
            )
            .await
    });
    // requestId is filled only after the spawned ask future commits registration.
    // requestId 仅在已启动 ask future 提交注册后填充。
    let mut request_id = None;
    for _ in 0..100 {
        // pendingRequest observes the oldest registered request without guessing its identifier.
        // pendingRequest 在不猜测标识的前提下读取最早已注册请求。
        let pending_request = question_runtime
            .list_pending()
            .expect("pending question registry should remain readable")
            .into_iter()
            .next();
        if let Some(pending_request) = pending_request {
            request_id = Some(pending_request.id);
            break;
        }
        tokio::task::yield_now().await;
    }
    (
        ask_task,
        request_id.expect("ask request should register within bounded scheduler yields"),
    )
}

/// Verifies reply and rejection each publish one authoritative Ask removal to every client sink.
/// 验证回答与拒绝各自仅向每个客户端 sink 发布一次权威 Ask 移除事件。
#[tokio::test]
async fn ask_completions_publish_once_to_every_client_sink() {
    // sessionId identifies the conversation that owns both confirmation requests.
    // sessionId 标识拥有两条确认请求的会话。
    let session_id = SessionId::new_v4();
    // serviceState supplies the retained workbench and runtime bridge used by projection callbacks.
    // serviceState 提供投影回调使用的保留工作台与运行时桥。
    let mut service_state = test_service_state_with_parent_session(session_id);
    // agentRuntime owns the shared question runtime used by the service handlers.
    // agentRuntime 持有服务处理器使用的共享问题运行时。
    let agent_runtime = test_agent_runtime_bridge();
    // questionRuntime is retained for creating and inspecting pending requests.
    // questionRuntime 用于创建并检查待回答请求。
    let question_runtime = Arc::clone(&agent_runtime.question_runtime);
    service_state.agent_runtime = Some(agent_runtime.clone());
    // state shares the authoritative service projection state with the callback and handlers.
    // state 在回调与处理器之间共享权威服务投影状态。
    let state = Arc::new(Mutex::new(service_state));
    // firstClientEvents captures the first independently registered client sink.
    // firstClientEvents 捕获第一个独立注册的客户端 sink。
    let first_client_events = Arc::new(Mutex::new(Vec::new()));
    // secondClientEvents captures the second independently registered client sink.
    // secondClientEvents 捕获第二个独立注册的客户端 sink。
    let second_client_events = Arc::new(Mutex::new(Vec::new()));
    // app multiplexes each authoritative service event to both simulated clients.
    // app 将每条权威服务事件复用到两个模拟客户端。
    let app = ServiceEventEmitter::new(vec![
        Arc::new(CapturingServiceEventSink {
            events: Arc::clone(&first_client_events),
        }),
        Arc::new(CapturingServiceEventSink {
            events: Arc::clone(&second_client_events),
        }),
    ]);
    install_webui_question_runtime_event_bridge(&app, &state, &agent_runtime)
        .expect("question projection bridge should install");
    // This focused projection test supplies deterministic successful durability after verifying
    // the production installer wires both callbacks. V1 mutation semantics are covered by the
    // SessionHandle and Ask durability tests with real session documents.
    // 该聚焦投影测试在验证生产安装器同时接入两类回调后，提供确定性成功的可靠性提交；
    // V1 修改语义由使用真实会话文档的 SessionHandle 与 Ask 可靠性测试覆盖。
    question_runtime
        .set_durability_callback(Some(Arc::new(|_| Ok(()))))
        .expect("focused projection durability callback should install");

    // replyTask owns the first pending ask future that will receive structured answers.
    // replyTask 持有第一条将接收结构化答案的待回答 ask future。
    let (reply_task, reply_request_id) = start_pending_question(
        Arc::clone(&question_runtime),
        session_id,
        ApiCallId::try_new(1).expect("reply API id should be valid"),
        ToolCallId::try_new(1).expect("reply tool id should be valid"),
        "provider-tool-call-reply",
        "Approve execution?",
    )
    .await;
    assert_eq!(
        pending_question_delta_generations(&first_client_events).len(),
        1
    );
    assert_eq!(
        pending_question_delta_generations(&second_client_events).len(),
        1
    );
    first_client_events
        .lock()
        .expect("first client buffer should lock")
        .clear();
    second_client_events
        .lock()
        .expect("second client buffer should lock")
        .clear();
    // replyGenerationBefore records the single expected projection increment boundary.
    // replyGenerationBefore 记录预期单次投影递增的边界起点。
    let reply_generation_before = state
        .lock()
        .expect("service state should lock")
        .workbench_generation;

    assert!(
        service_reply_pending_question(
            Arc::clone(&state),
            reply_request_id,
            vec![vec![String::from("Approve")]],
        )
        .expect("reply should commit")
    );
    // replyGenerationAfter proves the state transition generated exactly one projection revision.
    // replyGenerationAfter 证明状态转换仅生成一个投影版本。
    let reply_generation_after = state
        .lock()
        .expect("service state should lock")
        .workbench_generation;
    assert_eq!(reply_generation_after, reply_generation_before + 1);
    assert_eq!(
        pending_question_delta_generations(&first_client_events),
        vec![reply_generation_after]
    );
    assert_eq!(
        pending_question_delta_generations(&second_client_events),
        vec![reply_generation_after]
    );
    assert_eq!(
        reply_task
            .await
            .expect("reply ask task should join")
            .expect("reply ask task should complete")
            .answers,
        vec![vec![String::from("Approve")]]
    );

    // rejectTask owns the second pending ask future that will be explicitly rejected.
    // rejectTask 持有第二条将被明确拒绝的待回答 ask future。
    let (reject_task, reject_request_id) = start_pending_question(
        Arc::clone(&question_runtime),
        session_id,
        ApiCallId::try_new(1).expect("reject API id should be valid"),
        ToolCallId::try_new(2).expect("reject tool id should be valid"),
        "provider-tool-call-reject",
        "Abandon execution?",
    )
    .await;
    first_client_events
        .lock()
        .expect("first client buffer should lock")
        .clear();
    second_client_events
        .lock()
        .expect("second client buffer should lock")
        .clear();
    // rejectGenerationBefore records the single expected rejection projection boundary.
    // rejectGenerationBefore 记录预期单次拒绝投影的边界起点。
    let reject_generation_before = state
        .lock()
        .expect("service state should lock")
        .workbench_generation;

    assert!(
        service_reject_pending_question(Arc::clone(&state), reject_request_id)
            .expect("rejection should commit")
    );
    // rejectGenerationAfter proves rejection also generated exactly one projection revision.
    // rejectGenerationAfter 证明拒绝同样仅生成一个投影版本。
    let reject_generation_after = state
        .lock()
        .expect("service state should lock")
        .workbench_generation;
    assert_eq!(reject_generation_after, reject_generation_before + 1);
    assert_eq!(
        pending_question_delta_generations(&first_client_events),
        vec![reject_generation_after]
    );
    assert_eq!(
        pending_question_delta_generations(&second_client_events),
        vec![reject_generation_after]
    );
    assert!(
        reject_task
            .await
            .expect("rejected ask task should join")
            .expect_err("rejected ask task should return cancellation")
            .contains("abandoned")
    );
}

/// Verifies the committed Plan-confirmation projection clears stale artifacts once for every client sink.
/// 验证已提交的 Plan 确认投影会为每个客户端 sink 各清理一次陈旧计划产物。
#[test]
fn plan_confirmation_projection_publishes_once_to_every_client_sink() {
    // sessionId identifies the session whose Plan artifact has just been confirmed.
    // sessionId 标识刚完成计划确认的会话。
    let session_id = SessionId::new_v4();
    // serviceState starts with a stale Plan confirmation surface retained by the UI projection.
    // serviceState 从界面投影保留的陈旧 Plan 确认表面开始。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.workbench.selected_conversation_id = Some(session_id.to_string());
    service_state.workbench.session_plan_mode = Some(SessionPlanModeSummary {
        execution_mode: String::from("plan"),
        prompt_injected: true,
        artifact: Some(SessionPlanArtifactSummary {
            name: String::from("Execution plan"),
            path: String::from("D:/workspace/docs/plan.md"),
            content: Some(String::from("# Plan")),
            updated_at: String::from("1"),
        }),
    });
    // The default V1 document is the authoritative post-confirmation Build state.
    // 默认 V1 文档是确认后的权威 Build 状态。
    register_test_session_v1_document(
        &mut service_state,
        session_id,
        vulcan_session_core::v1::document::SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Plan projection test",
            1,
        ),
    );
    // state shares the authoritative Plan state with the projection function.
    // state 与投影函数共享权威 Plan 状态。
    let state = Arc::new(Mutex::new(service_state));
    // firstClientEvents captures the first independently registered client sink.
    // firstClientEvents 捕获第一个独立注册的客户端 sink。
    let first_client_events = Arc::new(Mutex::new(Vec::new()));
    // secondClientEvents captures the second independently registered client sink.
    // secondClientEvents 捕获第二个独立注册的客户端 sink。
    let second_client_events = Arc::new(Mutex::new(Vec::new()));
    // app multiplexes the Plan state delta to both simulated clients.
    // app 将 Plan 状态增量复用到两个模拟客户端。
    let app = ServiceEventEmitter::new(vec![
        Arc::new(CapturingServiceEventSink {
            events: Arc::clone(&first_client_events),
        }),
        Arc::new(CapturingServiceEventSink {
            events: Arc::clone(&second_client_events),
        }),
    ]);
    // generationBefore records the single expected Plan projection revision boundary.
    // generationBefore 记录预期单次 Plan 投影版本的边界起点。
    let generation_before = state
        .lock()
        .expect("service state should lock")
        .workbench_generation;

    // response is the same committed addon state returned to the client that performed confirmation.
    // response 是返回给执行确认客户端的同一份已提交附属状态。
    let response = emit_session_addons_changed_delta(&app, &state, session_id)
        .expect("Plan confirmation projection should succeed");
    // generationAfter proves the Plan state transition generated exactly one delta revision.
    // generationAfter 证明 Plan 状态转换仅生成一个增量版本。
    let generation_after = state
        .lock()
        .expect("service state should lock")
        .workbench_generation;

    assert_eq!(generation_after, generation_before + 1);
    assert_eq!(response.revision, generation_after);
    assert!(response.session_plan_mode.is_none());
    assert_eq!(
        session_addon_delta_generations(&first_client_events),
        vec![generation_after]
    );
    assert_eq!(
        session_addon_delta_generations(&second_client_events),
        vec![generation_after]
    );
    assert!(
        state
            .lock()
            .expect("service state should lock")
            .workbench
            .session_plan_mode
            .is_none()
    );
}

/// Verifies the production Ask durability bridge never waits on the service state lock.
/// 验证生产 Ask 可靠性桥接永不等待服务状态锁。
///
/// Reply and rejection run the durability callback while holding the pending-question lock, and
/// workbench projections read pending questions while holding the service state lock, so a
/// callback that takes the state lock can deadlock against a concurrent projection.
/// 回答与拒绝在持有待回答问题锁时运行可靠性回调，而工作台投影在持有服务状态锁时读取待回答
/// 问题；回调若获取状态锁，就可能与并发投影互相死锁。
#[test]
fn ask_durability_bridge_does_not_wait_on_service_state_lock() {
    let mut service_state = test_service_state_with_parent_session(SessionId::new_v4());
    let agent_runtime = test_agent_runtime_bridge();
    let question_runtime = Arc::clone(&agent_runtime.question_runtime);
    service_state.agent_runtime = Some(agent_runtime.clone());
    let state = Arc::new(Mutex::new(service_state));
    install_webui_question_runtime_event_bridge(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent_runtime,
    )
    .expect("question projection bridge should install");

    // stateGuard stands in for a projection that holds the service state lock.
    // stateGuard 模拟持有服务状态锁的投影。
    let state_guard = state.lock().expect("service state should lock");
    let (sender, receiver) = std::sync::mpsc::channel();
    // askThread registers an Ask for a session the registry has never hydrated, so the durable
    // WaitingUser commit fails as soon as it consults the registry.
    // askThread 为注册表从未 hydration 的会话注册 Ask，使可靠 WaitingUser 提交一查询注册表即失败。
    let ask_thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("ask runtime should build");
        let result = runtime.block_on(question_runtime.ask(
            SessionId::new_v4(),
            RequestStreamId::try_new("lock-order-stream").expect("request-stream identity"),
            ApiCallId::try_new(1).expect("API id should be valid"),
            ToolCallId::try_new(1).expect("tool id should be valid"),
            String::from("controller-lock-order"),
            None,
            String::from("ask"),
            serde_json::json!({"questions": []}),
            vec![QuestionPrompt {
                question: String::from("Proceed?"),
                header: String::from("Confirm"),
                options: Vec::new(),
                multiple: Some(false),
            }],
        ));
        let _ = sender.send(result);
    });

    let result = receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("Ask durability must settle without the service state lock");
    drop(state_guard);
    ask_thread.join().expect("ask thread should join");
    assert!(
        result.is_err(),
        "an unhydrated session must reject the durable WaitingUser commit"
    );
}

/// Verifies committed answers and rejections reach plugins subscribed to the question hook points.
/// 验证已提交的回答与拒绝会送达订阅问题 hook 点位的插件。
#[tokio::test]
async fn committed_ask_outcomes_dispatch_question_hooks() {
    use vulcan_agent_runtime::agent_runtime::conversation_hooks::{
        HOOK_CONVERSATION_QUESTION_ANSWERED, HOOK_CONVERSATION_QUESTION_REJECTED,
    };
    use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    let agent_runtime = test_agent_runtime_bridge();
    let question_runtime = Arc::clone(&agent_runtime.question_runtime);
    service_state.agent_runtime = Some(agent_runtime.clone());
    let state = Arc::new(Mutex::new(service_state));
    install_webui_question_runtime_event_bridge(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent_runtime,
    )
    .expect("question projection bridge should install");
    // Durable V1 Ask mutations are covered with real documents elsewhere; this test isolates hooks.
    // V1 Ask 可靠修改由其他使用真实文档的测试覆盖；本测试只隔离 hook 行为。
    question_runtime
        .set_durability_callback(Some(Arc::new(|_| Ok(()))))
        .expect("focused durability callback should install");

    // observed receives (hook point, request id) for every question outcome hook.
    // observed 接收每个问题结局 hook 的 (hook 点位, 请求标识)。
    let (sender, observed) = std::sync::mpsc::channel::<(String, String)>();
    let sender = Arc::new(Mutex::new(sender));
    {
        let mut hooks = agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("hook runtime should lock");
        for point in [
            HOOK_CONVERSATION_QUESTION_ANSWERED,
            HOOK_CONVERSATION_QUESTION_REJECTED,
        ] {
            let sender = Arc::clone(&sender);
            hooks
                .dispatcher_mut()
                .register(
                    HookRegistration::new(format!("listener-{point}"), point, HookMode::Effect),
                    Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                        let request_id = invocation
                            .payload
                            .value
                            .get("request_id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let _ = sender
                            .lock()
                            .expect("observer sender should lock")
                            .send((point.to_string(), request_id));
                        async { Ok(HookResult::default()) }
                    })),
                )
                .expect("question hook listener should register");
        }
    }

    let (reply_task, reply_request_id) = start_pending_question(
        Arc::clone(&question_runtime),
        session_id,
        ApiCallId::try_new(1).expect("reply API id should be valid"),
        ToolCallId::try_new(1).expect("reply tool id should be valid"),
        "provider-tool-call-hook-reply",
        "Approve execution?",
    )
    .await;
    service_reply_pending_question(
        Arc::clone(&state),
        reply_request_id.clone(),
        vec![vec![String::from("Approve")]],
    )
    .expect("reply should commit");
    reply_task
        .await
        .expect("reply ask task should join")
        .expect("reply ask task should complete");
    assert_eq!(
        observed
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("answered hook should run"),
        (
            HOOK_CONVERSATION_QUESTION_ANSWERED.to_string(),
            reply_request_id
        )
    );

    let (reject_task, reject_request_id) = start_pending_question(
        Arc::clone(&question_runtime),
        session_id,
        ApiCallId::try_new(1).expect("reject API id should be valid"),
        ToolCallId::try_new(2).expect("reject tool id should be valid"),
        "provider-tool-call-hook-reject",
        "Abandon execution?",
    )
    .await;
    service_reject_pending_question(Arc::clone(&state), reject_request_id.clone())
        .expect("rejection should commit");
    assert!(
        reject_task
            .await
            .expect("reject ask task should join")
            .is_err()
    );
    assert_eq!(
        observed
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("rejected hook should run"),
        (
            HOOK_CONVERSATION_QUESTION_REJECTED.to_string(),
            reject_request_id
        )
    );
}
