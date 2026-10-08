//! Exercises live composer usage publication through the production Provider sink.
//! 通过生产 Provider sink 验证输入区用量的实时发布。

use super::*;
use vulcan_service_api::{
    ContextUsageSnapshot, ServiceEvent, ServiceEventEmitter, ServiceEventSink, WorkbenchDelta,
};

/// Records usage deltas and checks that they follow durable API statistics.
/// 记录用量增量，并检查它们晚于可靠 API 统计。
struct UsageEvents {
    /// Canonical session owner whose facts must already be committed when a delta arrives.
    /// 增量到达时其事实必须已提交的规范会话所有者。
    handle: Arc<SessionHandle>,
    /// Observed revisions and optional usage snapshots; other service events are ignored.
    /// 观察到的修订号和可选用量快照；忽略其他服务事件。
    updates: std::sync::Mutex<Vec<(u64, Option<ContextUsageSnapshot>)>>,
}

impl ServiceEventSink for UsageEvents {
    /// Captures the supplied service event without refreshing or selecting a conversation; returns nothing.
    /// 捕获传入的服务事件，不刷新或选择会话；无返回值。
    fn emit_service_event(&self, event: ServiceEvent) {
        if let ServiceEvent::WorkbenchDelta(envelope) = event
            && let WorkbenchDelta::ContextUsageChanged(delta) = envelope.delta
        {
            self.handle
                .read(|document| {
                    assert_eq!(document.memory_revision(), document.durable_revision());
                    assert!(
                        document
                            .events()
                            .iter()
                            .any(|row| matches!(row.event, EventBody::ApiStats(_)))
                    );
                })
                .expect("usage publication must follow durable API statistics");
            self.updates
                .lock()
                .unwrap()
                .push((envelope.generation, delta.context_usage));
        }
    }
}

/// Creates a selected session whose root admission activates the fixture's Provider route.
/// 创建一个选中会话，由根输入准入激活夹具的 Provider 路由。
/// Takes no arguments; returns the canonical owner and shared state without a real Provider connection.
/// 无参数；返回规范所有者和共享状态，不连接真实 Provider。
fn selected_usage_state() -> (Arc<SessionHandle>, SharedServiceBridgeState) {
    // Bind the request configuration through normal root admission so revisions remain authoritative.
    // 通过正常根输入准入绑定请求配置，保证修订号遵循权威状态机。
    let request = request_facts().request;
    // Give the isolated document a unique session identity.
    // 为隔离文档分配唯一会话身份。
    let session_id = SessionId::new_v4();
    // The mock backend preserves the same publication contract without writing a user session.
    // 模拟后端保留相同发布契约，不写入用户会话。
    let handle = SessionHandle::with_backend(
        session_id,
        SessionDocument::new(session_id.to_string(), "project-1", "title", 1),
        Arc::new(TestPersistenceBackend),
    );
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("context-usage-root").unwrap(),
                "root",
                Vec::new(),
            )
            .with_bound_request_config(
                vulcan_session_core::v1::config::ActiveRequestConfig {
                    provider: Some(request.provider.clone()),
                    connection_id: request.connection_id.clone(),
                    model: Some(request.model.clone()),
                    thinking_level: request.thinking_level.clone(),
                    temperature: request.temperature,
                },
            ),
            2,
        )
        .unwrap();
    // Seed the same runtime identity installed by execution preparation.
    // 播种执行准备阶段安装的同一运行时身份。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(handle.session_id());
    service
        .session_runtime_registry
        .register(Arc::clone(&handle))
        .unwrap();
    service.workbench.selected_conversation_id = Some(handle.session_id().to_string());
    service.workbench.context_usage = None;
    service
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .session_runtime_state
        .insert(
            handle.session_id(),
            crate::SessionRuntimeStateEntry {
                active_provider_id: Some(request.provider),
                active_model_id: Some(request.model),
                active_connection_id: request.connection_id,
                ..Default::default()
            },
        );
    (handle, Arc::new(std::sync::Mutex::new(service)))
}

/// Wraps `facts` in the physical terminal telemetry consumed by the production sink.
/// 将 `facts` 包装为生产 sink 消费的物理终态遥测。
/// Returns an event owned by `session_id` with matching normalized usage.
/// 返回属于 `session_id` 且带有匹配规范用量的事件。
fn usage_terminal(
    session_id: SessionId,
    facts: ProviderV1ApiTerminalFacts,
) -> vulcan_agent_runtime::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryEvent
{
    use vulcan_agent_runtime::agent_runtime::provider_request_telemetry::{
        ProviderRequestCompletedUsage, ProviderRequestTelemetryEvent,
        ProviderRequestTelemetryEventKind,
    };
    ProviderRequestTelemetryEvent {
        kind: ProviderRequestTelemetryEventKind::Completed,
        session_id,
        payload: json!({}),
        completed_usage: Some(ProviderRequestCompletedUsage {
            input_tokens: facts.usage.input_tokens.map(|value| value as usize),
            output_tokens: facts.usage.output_tokens.map(|value| value as usize),
            total_tokens: facts.usage.total_tokens.map(|value| value as usize),
            ..Default::default()
        }),
        v1_lifecycle: Some(vulcan_agent_runtime::agent_runtime::provider_v1_bridge::ProviderV1LifecycleFacts::ApiTerminal(facts)),
    }
}

/// First plain-text completion publishes usage immediately, without a selection refresh.
/// 首次纯文本完成立即发布用量，不依赖重新选择会话。
#[tokio::test]
async fn first_api_publishes_context_usage_without_navigation() {
    // A fresh selected session starts without an orb because no API has reported usage.
    // 新选中会话没有 API 上报用量，因此起初没有用量圈。
    let (handle, state) = selected_usage_state();
    // Capture the same wire events consumed by WebUI.
    // 捕获 WebUI 消费的同类线上事件。
    let capture = Arc::new(UsageEvents {
        handle: Arc::clone(&handle),
        updates: Default::default(),
    });
    // Deliver service publications synchronously to the observer.
    // 将服务发布同步传递给观察器。
    let app = ServiceEventEmitter::new(vec![capture.clone()]);
    // Exercise the production terminal boundary with an isolated owner.
    // 使用隔离所有者测试生产终态边界。
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request(&handle, &mut sink);
    sink.append_assistant_delta("answer", 6).unwrap();
    assert!(state.lock().unwrap().workbench.context_usage.is_none());
    sink.accept_telemetry_terminal(
        &usage_terminal(handle.session_id(), terminal_facts(0)),
        &app,
        &state,
    )
    .await
    .unwrap();
    // Assert the wire delta and retained snapshot, not only the projection helper's return value.
    // 同时断言线上增量和保留快照，而不只检查投影辅助函数的返回值。
    let updates = capture.updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    // Locate the latest publication independently of event positions in the session history.
    // 独立于会话历史事件位置，定位最近发布。
    let (revision, usage) = updates.last().unwrap();
    assert_eq!(usage.as_ref().unwrap().used_tokens, 15);
    assert_eq!(
        state.lock().unwrap().workbench.context_usage.as_ref(),
        usage.as_ref()
    );
    assert_eq!(state.lock().unwrap().workbench.revision, *revision);
}

/// Consecutive tool APIs update usage while the logical turn stays open and ignore background sessions.
/// 连续工具 API 在逻辑回合保持开放时更新用量，并忽略后台会话。
#[tokio::test]
async fn tool_apis_refresh_context_usage_before_turn_completion() {
    // One sink drives multiple API groups in the same open turn.
    // 同一 sink 驱动一个开放回合内的多个 API 组。
    let (handle, state) = selected_usage_state();
    // Only context-usage deltas enter this observer.
    // 此观察器只记录上下文用量增量。
    let capture = Arc::new(UsageEvents {
        handle: Arc::clone(&handle),
        updates: Default::default(),
    });
    // The service emitter requires no network listener.
    // 此服务发射器无需网络监听器。
    let app = ServiceEventEmitter::new(vec![capture.clone()]);
    // Reuse the sink exactly as a multi-step tool turn does.
    // 按多步工具回合的实际方式复用 sink。
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    for (input_tokens, selected, expected_updates) in [(10, true, 1), (30, true, 2), (50, false, 2)]
    {
        // Switch only for the background case; the first two updates must occur without navigation.
        // 仅后台场景切换选中对象；前两次更新必须无需导航发生。
        if !selected {
            state.lock().unwrap().workbench.selected_conversation_id =
                Some(SessionId::new_v4().to_string());
        }
        prepare_sink_request(&handle, &mut sink);
        // The physical terminal has usage, but ApiStats correctly waits for its tool to finish.
        // 物理终态已有用量，但 ApiStats 按契约等待所属工具完成。
        let mut facts = terminal_facts(1);
        facts.usage.input_tokens = Some(input_tokens);
        facts.usage.total_tokens = Some(input_tokens + 5);
        sink.accept_telemetry_terminal(&usage_terminal(handle.session_id(), facts), &app, &state)
            .await
            .unwrap();
        // This fixture declares a tool without executing external code.
        // 此夹具声明工具但不执行外部代码。
        let request = recovery_request("tool-1", "file_read", json!({"path": "test.txt"}));
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::BatchPrepared(vec![request.clone()]),
            9,
        )
        .await
        .unwrap();
        sink.apply_tool_boundary(
            &ToolExecutionRecoveryBoundary::Started {
                request: request.request.clone(),
                binding_fingerprint: request.binding_fingerprint,
            },
            10,
        )
        .await
        .unwrap();
        // The public boundary must emit usage as soon as this last tool commits.
        // 此最后工具提交后，公开边界必须立即发送用量。
        let event = RuntimeEvent::new(
            vulcan_agent_runtime::agent_runtime::events::RuntimeEventKind::ToolFinished,
            "tool",
            "done",
        )
        .with_tool_execution_boundary(ToolExecutionRecoveryBoundary::Finished(
            ToolInvocationResult {
                tool_call_id: request.request.tool_call_id,
                tool_name: request.request.tool_name,
                model_output: "content".into(),
                host_result: None,
                is_error: false,
            },
        ));
        sink.apply_tool_event(&app, &state, &event, 11)
            .await
            .unwrap();
        assert_eq!(capture.updates.lock().unwrap().len(), expected_updates);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .workbench
                .context_usage
                .as_ref()
                .unwrap()
                .used_tokens,
            if selected {
                input_tokens as usize + 5
            } else {
                35
            }
        );
        assert!(
            handle
                .read(|document| document.active_turn().is_some_and(|turn| !turn.closed))
                .unwrap()
        );
    }
    // Every later API publishes a newer revision even though no turn-level report has run.
    // 即使未运行回合级报告，后续 API 仍发布更新的修订号。
    let updates = capture.updates.lock().unwrap();
    assert!(updates.windows(2).all(|pair| pair[0].0 < pair[1].0));
}

/// Missing Provider usage must remain unknown rather than showing a fabricated zero-percent orb.
/// Provider 缺失用量时必须保留未知状态，不展示虚构的零占比用量圈。
#[tokio::test]
async fn first_api_without_usage_keeps_context_usage_hidden() {
    // Omit all terminal usage dimensions on an otherwise successful response.
    // 对其他方面成功的响应省略全部终态用量维度。
    let (handle, state) = selected_usage_state();
    // Capture any accidental publication of fabricated usage.
    // 捕获任何误发的虚构用量。
    let capture = Arc::new(UsageEvents {
        handle: Arc::clone(&handle),
        updates: Default::default(),
    });
    // Observe production events without a browser or remote endpoint.
    // 观察生产事件，无需浏览器或远端端点。
    let app = ServiceEventEmitter::new(vec![capture.clone()]);
    // Use the same terminal entry as a successful text response.
    // 使用成功文本响应的同一终态入口。
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request(&handle, &mut sink);
    // Unknown usage remains absent in the authoritative terminal record.
    // 未知用量在权威终态记录中保持缺省。
    let mut facts = terminal_facts(0);
    facts.usage = UsageSnapshot::default();
    // The successful terminal's replay source belongs to this completed assistant response.
    // 成功终态的重放来源属于此已完成助手响应。
    sink.append_assistant_delta("answer", 6).unwrap();
    sink.accept_telemetry_terminal(&usage_terminal(handle.session_id(), facts), &app, &state)
        .await
        .unwrap();
    assert!(state.lock().unwrap().workbench.context_usage.is_none());
    assert!(capture.updates.lock().unwrap().is_empty());
}
