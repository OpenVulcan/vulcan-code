use super::{
    VulcanManagedToolRuntimeBridge, acknowledge_runtime_event_projection_barrier_from_event,
    set_runtime_event_projection_barriers_enabled,
};
use crate::agent_runtime::events::RuntimeEventKind;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use async_trait::async_trait;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use uuid::Uuid;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::VulcanProviderAdapter;
use vulcan_core::tool::{AllowAllToolPolicy, VulcanToolExecutor};
use vulcan_core::types::{
    ControllerConfig, ConversationPayload, ConversationSnapshot, ConversationToolCall,
    CoreError as VulcanCoreError, ProviderRequestSnapshot, ProviderTurnResponse,
    ToolInvocationRequest,
};
use vulcan_tools_manager::{
    VULCAN_TOOL_ERROR_DETAIL_KIND, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolHostResult, VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
};

/// Returns one demo tools manager plus the resolved tool definitions used by the bridge tests.
/// 返回一份供桥接测试复用的演示工具管理器及其已解析工具定义集合。
fn build_demo_manager() -> (
    VulcanToolsManager,
    Vec<vulcan_tools_manager::ResolvedToolDefinition>,
) {
    let manager = VulcanToolsManager::new(vec![
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("demo_tool"),
                name: String::from("demo_tool"),
                description: String::from("Demo tool"),
                help_text: None,
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            invoke: Arc::new(|args, session_id| {
                Box::pin(async move {
                    Ok(VulcanToolInvocationResult::from_model_output(json!({
                        "args": args,
                        "session_id": session_id,
                    }))
                    .with_host_result(VulcanToolHostResult {
                        kind: String::from("change_set"),
                        payload: json!({ "path": "src/main.rs", "op": "read" }),
                    }))
                })
            }),
        },
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("failing_tool"),
                name: String::from("failing_tool"),
                description: String::from("Failing demo tool"),
                help_text: None,
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Hidden,
            invoke: Arc::new(|_args, _session_id| {
                Box::pin(async move { Err(String::from("LuaSkills 工具解析失败：演示错误")) })
            }),
        },
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("structured_failure_tool"),
                name: String::from("structured_failure_tool"),
                description: String::from("Structured failure demo tool"),
                help_text: None,
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({ "type": "object" }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            invoke: Arc::new(|_args, _session_id| {
                Box::pin(async move {
                    Ok(VulcanToolInvocationResult::from_tool_error_details(
                        "structured persistence failure",
                        json!({ "phase": "persist" }),
                    ))
                })
            }),
        },
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("invalid_structured_failure_tool"),
                name: String::from("invalid_structured_failure_tool"),
                description: String::from("Invalid structured failure demo tool"),
                help_text: None,
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({ "type": "object" }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            invoke: Arc::new(|_args, _session_id| {
                Box::pin(async move {
                    Ok(VulcanToolInvocationResult::from_model_output(json!({
                        "status": "failed"
                    }))
                    .with_host_result(VulcanToolHostResult {
                        kind: String::from(VULCAN_TOOL_ERROR_DETAIL_KIND),
                        payload: json!({ "status": "failed" }),
                    }))
                })
            }),
        },
    ]);
    let resolved = manager.resolve_for_execution_mode("Build");
    (manager, resolved.definitions)
}

/// Returns one demo tools manager that contains a tool panicking during asynchronous invocation.
/// 返回一份包含异步调用期间 panic 工具的演示工具管理器。
fn build_panic_demo_manager() -> (
    VulcanToolsManager,
    Vec<vulcan_tools_manager::ResolvedToolDefinition>,
) {
    let manager = VulcanToolsManager::new(vec![VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("panic_tool"),
            name: String::from("panic_tool"),
            description: String::from("Panic demo tool"),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        }),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        invoke: Arc::new(|_args, _session_id| {
            Box::pin(async move {
                panic!("模拟搜索节点 panic");
            })
        }),
    }]);
    let resolved = manager.resolve_for_execution_mode("Build");
    (manager, resolved.definitions)
}

/// Returns a tools manager whose tool body waits until the test explicitly lets it continue.
/// 返回一份工具主体会等待测试显式放行的工具管理器。
fn build_gated_demo_manager(
    entered_tool_body: Arc<AtomicBool>,
    release_tool_body: Arc<Notify>,
) -> (
    VulcanToolsManager,
    Vec<vulcan_tools_manager::ResolvedToolDefinition>,
) {
    let manager = VulcanToolsManager::new(vec![VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("gated_tool"),
            name: String::from("gated_tool"),
            description: String::from("Gated demo tool"),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        }),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        invoke: Arc::new(move |_args, _session_id| {
            let entered_tool_body = Arc::clone(&entered_tool_body);
            let release_tool_body = Arc::clone(&release_tool_body);
            Box::pin(async move {
                entered_tool_body.store(true, Ordering::SeqCst);
                release_tool_body.notified().await;
                Ok(VulcanToolInvocationResult::from_model_output(json!(
                    "gated done"
                )))
            })
        }),
    }]);
    let resolved = manager.resolve_for_execution_mode("Build");
    (manager, resolved.definitions)
}

/// Drains all live runtime events currently queued by a streaming tool executor.
/// 清空流式工具执行器当前已经排队的全部实时运行事件。
fn drain_live_events(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
) -> Vec<ProviderStreamDelta> {
    let mut events = Vec::new();
    while let Ok(item) = receiver.try_recv() {
        events.push(item.expect("live event should not fail"));
    }
    events
}

/// Resets runtime projection barriers when a test scope exits.
/// 测试作用域退出时重置运行时投影屏障。
struct RuntimeProjectionBarrierResetGuard;

impl Drop for RuntimeProjectionBarrierResetGuard {
    /// Disables projection barriers during test cleanup.
    /// 在测试清理阶段禁用投影屏障。
    fn drop(&mut self) {
        set_runtime_event_projection_barriers_enabled(false);
    }
}

/// Captures pre-planned provider turn responses and records the built request history for assertions.
/// 持有预先规划的 Provider 回合响应，并记录构建出的请求历史供断言使用。
#[derive(Debug, Clone)]
struct MockControllerProviderAdapter {
    /// Provider identifier exposed to the controller.
    /// 暴露给控制器的 Provider 标识。
    provider_id: String,
    /// Model identifier exposed to the controller.
    /// 暴露给控制器的模型标识。
    model_id: String,
    /// Planned turn responses returned sequentially.
    /// 按顺序返回的计划回合响应队列。
    planned_responses: Arc<Mutex<VecDeque<ProviderTurnResponse>>>,
    /// Built request snapshots captured during the loop.
    /// 在循环过程中捕获到的请求快照集合。
    built_requests: Arc<Mutex<Vec<ProviderRequestSnapshot>>>,
}

impl MockControllerProviderAdapter {
    /// Builds one mock controller provider adapter from a response queue.
    /// 根据响应队列构建一个模拟控制器 Provider 适配器。
    fn new(planned_responses: Vec<ProviderTurnResponse>) -> Self {
        Self {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            planned_responses: Arc::new(Mutex::new(VecDeque::from(planned_responses))),
            built_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns the built request snapshots captured by the adapter so far.
    /// 返回适配器截至当前捕获到的请求快照集合。
    fn built_requests(&self) -> Vec<ProviderRequestSnapshot> {
        self.built_requests
            .lock()
            .expect("lock built requests")
            .clone()
    }
}

#[async_trait]
impl VulcanProviderAdapter for MockControllerProviderAdapter {
    type Request = ProviderRequestSnapshot;
    type Response = ProviderTurnResponse;

    /// Returns the stable provider identifier exposed by the mock adapter.
    /// 返回模拟适配器暴露的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier exposed by the mock adapter.
    /// 返回模拟适配器暴露的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one simple request snapshot directly from the controller snapshot for later assertions.
    /// 直接根据控制器快照构建简单请求快照，供后续断言使用。
    async fn build_request(
        &self,
        snapshot: &ConversationSnapshot,
        config: &ControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let request = ProviderRequestSnapshot {
            provider_id: config.provider_id.clone(),
            model_id: config.model_id.clone(),
            system_prompt: config.system_prompt.clone(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                config.system_prompt.clone().clone(),
            ),
            prompt: String::from("controller-integration"),
            history: snapshot.messages.clone(),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: Vec::new(),
            additional_params: config.additional_params.clone(),
            request_headers: None,
            continuation: snapshot.continuation.clone(),
        };
        self.built_requests
            .lock()
            .expect("lock built requests")
            .push(request.clone());
        Ok(request)
    }

    /// Returns the next queued response or fails when the queue is empty.
    /// 返回下一个排队响应，若队列为空则返回失败。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        self.planned_responses
            .lock()
            .expect("lock planned responses")
            .pop_front()
            .ok_or_else(|| VulcanCoreError::InvalidState {
                message: String::from("模拟控制器 Provider 响应已耗尽。"),
            })
    }
}

/// Verifies the shared host-managed runtime bridge executes one resolved tool request and preserves host metadata.
/// 验证共享宿主管理运行时桥接层能够执行单条已解析工具请求，并保留宿主元数据。
#[test]
fn managed_tool_runtime_bridge_executes_core_tool_request() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("core tool request should execute");

    assert_eq!(result.tool_call_id, "call-1");
    assert_eq!(result.tool_name, "demo_tool");
    assert!(result.model_output.contains("\"src/main.rs\""));
    assert_eq!(
        result.host_result,
        Some(json!({
            "kind": "change_set",
            "payload": { "path": "src/main.rs", "op": "read" }
        }))
    );
}

/// Verifies host-managed tool invocation failures are downgraded into one error result returned to the controller instead of aborting the whole turn.
/// 验证宿主管理工具调用失败会被降级为回传给控制器的一条错误结果，而不是直接中断整轮执行。
#[test]
fn managed_tool_runtime_bridge_converts_tool_failures_into_error_results() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-error-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "README" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("failing tool should still return one controller-visible result");

    assert_eq!(result.tool_call_id, "call-error-1");
    assert_eq!(result.tool_name, "failing_tool");
    assert!(result.is_error);
    assert!(result.model_output.contains("Tool `failing_tool` failed"));
    assert!(
        result
            .model_output
            .contains("LuaSkills 工具解析失败：演示错误")
    );
    assert_eq!(
        result.host_result,
        Some(json!({
            "kind": "tool_error",
            "payload": {
                "tool_call_id": "call-error-1",
                "tool_name": "failing_tool",
                "status": "failed",
                "error": "LuaSkills 工具解析失败：演示错误",
                "arguments": { "query": "README" }
            }
        }))
    );
}

/// Verifies a tool-owned structured error keeps its host payload and Core error semantics.
/// 验证工具自有结构化错误会保留宿主载荷与 Core 错误语义。
#[test]
fn managed_tool_runtime_bridge_preserves_structured_tool_error_results() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-structured-error-1"),
        tool_name: String::from("structured_failure_tool"),
        arguments: json!({}),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("structured tool failure should remain controller-visible");

    assert!(result.is_error);
    assert!(result.model_output.contains("\"status\":\"failed\""));
    assert_eq!(
        result.host_result,
        Some(json!({
            "kind": "tool_error",
            "payload": {
                "tool_call_id": "call-structured-error-1",
                "tool_name": "structured_failure_tool",
                "status": "failed",
                "error": "structured persistence failure",
                "arguments": {},
                "details": {
                    "phase": "persist"
                }
            }
        }))
    );
}

/// Verifies malformed tool-owned error details become an explicit canonical protocol failure.
/// 验证格式错误的工具自有错误详情会转换为显式规范协议失败。
#[test]
fn managed_tool_runtime_bridge_rejects_structured_error_without_error_text() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-invalid-structured-error-1"),
        tool_name: String::from("invalid_structured_failure_tool"),
        arguments: json!({}),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("protocol failure should remain controller-visible");

    assert!(result.is_error);
    assert!(
        result
            .model_output
            .contains("without a non-empty `error` string")
    );
    let host_result = result
        .host_result
        .expect("protocol failure should use canonical host metadata");
    assert_eq!(host_result["kind"], "tool_error");
    assert_eq!(
        host_result["payload"]["tool_call_id"],
        "call-invalid-structured-error-1"
    );
    assert_eq!(
        host_result["payload"]["tool_name"],
        "invalid_structured_failure_tool"
    );
}

/// Verifies Plan mode blocks disallowed tools at the shared bridge instead of hiding them from the provider request.
/// 验证 Plan 模式会在共享桥接层拦截不允许的工具，而不是从 Provider 请求中隐藏它们。
#[test]
fn managed_tool_runtime_bridge_blocks_disallowed_plan_tool_invocation() {
    let (manager, definitions) = build_demo_manager();
    assert!(
        definitions
            .iter()
            .any(|definition| { definition.descriptor.name == "failing_tool" })
    );
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Plan");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-plan-blocked-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "README" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("plan-blocked tool should return model-visible guidance");

    assert_eq!(result.tool_call_id, "call-plan-blocked-1");
    assert_eq!(result.tool_name, "failing_tool");
    assert!(!result.is_error);
    assert!(result.model_output.contains("Plan Mode is active"));
    assert!(!result.model_output.contains("演示错误"));
    assert_eq!(result.host_result, None);
}

/// Verifies the host-owned `vulcan-core` loop can execute through the shared workbench tool bridge and preserve host metadata in canonical records.
/// 验证宿主自有的 `vulcan-core` 循环能够经由共享工作台工具桥接执行，并在规范记录中保留宿主元数据。
#[test]
fn managed_tool_runtime_bridge_integrates_with_core_controller_loop() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let provider = MockControllerProviderAdapter::new(vec![
        ProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::from("先调用工具获取文件状态。"),
            provider_message_id: Some(String::from("provider-1")),
            provider_message_state: None,
            tool_calls: vec![ConversationToolCall {
                id: String::from("tool-item-1"),
                tool_call_id: Some(String::from("tool-call-1")),
                tool_name: String::from("demo_tool"),
                arguments: json!({ "path": "src/main.rs" }),
            }],
            continuation: None,
            usage: None,
        },
        ProviderTurnResponse {
            assistant_text: String::from("工具执行完成，已经拿到宿主结果。"),
            assistant_reasoning: String::new(),
            provider_message_id: Some(String::from("provider-2")),
            provider_message_state: None,
            tool_calls: Vec::new(),
            continuation: None,
            usage: None,
        },
    ]);
    let mut controller = VulcanCoreController::new(
        ControllerConfig {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            system_prompt: String::from("system"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system").clone(),
            ),
            temperature: Some(0.2),
            max_turns: 3,
            additional_params: None,
            context_injection: None,
        },
        ConversationSnapshot::default(),
    )
    .expect("valid core conversation snapshot");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(controller.run_user_turn(
            &provider,
            &runtime_bridge,
            &AllowAllToolPolicy,
            "请检查主文件",
        ))
        .expect("core controller loop should succeed");

    assert_eq!(result.provider_turns_executed, 2);
    assert_eq!(result.tool_calls_processed, 1);
    assert_eq!(provider.built_requests().len(), 2);
    assert!(matches!(
        &controller.snapshot().messages[2].payload,
        ConversationPayload::ToolResult { text, .. } if text.contains("\"src/main.rs\"")
    ));
    assert!(controller.snapshot().records.iter().any(|record| {
        record.title == "调用 demo_tool 工具"
            && record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("kind"))
                .and_then(serde_json::Value::as_str)
                == Some("change_set")
    }));
}

/// Verifies the controller loop keeps running after one host-managed tool failure and can still receive a follow-up assistant response.
/// 验证宿主管理工具失败后控制器循环仍会继续运行，并且依然可以接收后续助手响应。
#[test]
fn managed_tool_runtime_bridge_keeps_controller_loop_alive_after_tool_failure() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let provider = MockControllerProviderAdapter::new(vec![
        ProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::from("先尝试调用会失败的工具。"),
            provider_message_id: Some(String::from("provider-1")),
            provider_message_state: None,
            tool_calls: vec![ConversationToolCall {
                id: String::from("tool-item-1"),
                tool_call_id: Some(String::from("tool-call-1")),
                tool_name: String::from("failing_tool"),
                arguments: json!({ "query": "README" }),
            }],
            continuation: None,
            usage: None,
        },
        ProviderTurnResponse {
            assistant_text: String::from("我已收到工具错误原因，接下来会调整策略继续处理。"),
            assistant_reasoning: String::new(),
            provider_message_id: Some(String::from("provider-2")),
            provider_message_state: None,
            tool_calls: Vec::new(),
            continuation: None,
            usage: None,
        },
    ]);
    let mut controller = VulcanCoreController::new(
        ControllerConfig {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            system_prompt: String::from("system"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system").clone(),
            ),
            temperature: Some(0.2),
            max_turns: 3,
            additional_params: None,
            context_injection: None,
        },
        ConversationSnapshot::default(),
    )
    .expect("valid core conversation snapshot");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(controller.run_user_turn(
            &provider,
            &runtime_bridge,
            &AllowAllToolPolicy,
            "请继续分析 README",
        ))
        .expect("controller loop should continue after one tool failure");

    assert_eq!(result.provider_turns_executed, 2);
    assert_eq!(result.tool_calls_processed, 1);
    assert!(matches!(
        &controller.snapshot().messages[2].payload,
        ConversationPayload::ToolResult { text, .. }
            if text.contains("Tool `failing_tool` failed")
                && text.contains("LuaSkills 工具解析失败：演示错误")
    ));
    assert!(matches!(
        controller
            .snapshot()
            .messages
            .last()
            .map(|message| &message.payload),
        Some(ConversationPayload::Assistant { text, .. })
            if text == "我已收到工具错误原因，接下来会调整策略继续处理。"
    ));
}

/// Verifies the streaming executor turns a panicking host tool into a model-visible tool error instead of crashing the controller task.
/// 验证流式执行器会将 panic 的宿主工具转换为模型可见的工具错误，而不是让控制器任务崩溃。
#[test]
fn streaming_tool_executor_converts_tool_panics_into_error_results() {
    let (manager, definitions) = build_panic_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("panic-call-1"),
        tool_name: String::from("panic_tool"),
        arguments: json!({ "query": "山姆零食" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(executor.execute_tool(&request))
        .expect("panicking tool should still return one controller-visible result");
    let events = drain_live_events(&mut receiver);

    assert!(result.is_error);
    assert!(result.model_output.contains("Tool `panic_tool` failed"));
    assert!(result.model_output.contains("模拟搜索节点 panic"));
    assert!(matches!(
        result.host_result,
        Some(ref value)
            if value["kind"] == "tool_error"
                && value["payload"]["tool_name"] == "panic_tool"
                && value["payload"]["error"]
                    .as_str()
                    .is_some_and(|message| message.contains("模拟搜索节点 panic"))
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolStarted
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolFinished
                && runtime_event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("failed")
    )));
}

/// Verifies a controller-visible tool error is projected with failed display state.
/// 验证控制器可见工具错误会投影为失败展示状态。
#[test]
fn streaming_tool_executor_marks_controller_error_results_failed() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("failing-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "演示错误" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(executor.execute_tool(&request))
        .expect("managed tool failure should still return a normal controller-visible result");
    let events = drain_live_events(&mut receiver);

    assert!(result.is_error);
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolFinished
                && runtime_event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("failed")
    )));
}

/// Verifies rejected tool requests return a friendly tool result without entering the tool body.
/// 验证被拒绝的工具请求会返回友好工具结果，且不会进入工具主体。
#[test]
fn streaming_tool_executor_rejects_tool_requests_before_host_execution() {
    let entered_tool_body = Arc::new(AtomicBool::new(false));
    let release_tool_body = Arc::new(Notify::new());
    let (manager, definitions) = build_gated_demo_manager(
        Arc::clone(&entered_tool_body),
        Arc::clone(&release_tool_body),
    );
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Compaction",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor = StreamingRuntimeToolExecutor::new(
        runtime_bridge,
        sender,
        Some(String::from(
            "Tool calls are not allowed during context compaction.",
        )),
        true,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        tool_call_id: String::from("rejected-call-1"),
        tool_name: String::from("gated_tool"),
        arguments: json!({ "query": "must not run" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(executor.execute_tool(&request))
        .expect("policy rejection should be returned as a normal tool result");
    let events = drain_live_events(&mut receiver);

    assert!(!result.is_error);
    assert!(
        result
            .model_output
            .contains("Tool request rejected by runtime policy")
    );
    assert!(
        result
            .model_output
            .contains("Tool calls are not allowed during context compaction")
    );
    assert_eq!(
        result
            .host_result
            .as_ref()
            .and_then(|value| value.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("tool_result")
    );
    assert_eq!(
        result
            .host_result
            .as_ref()
            .and_then(|value| value.get("payload"))
            .and_then(|payload| payload.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("rejected")
    );
    assert!(
        !entered_tool_body.load(Ordering::SeqCst),
        "rejected tool request must not enter the host tool body"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolStarted
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolFinished
                && runtime_event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("completed")
    )));
}

/// Verifies a rejection reason without an all-tools flag or scoped tool ids does not block normal tool calls.
/// 验证仅存在拒绝说明但没有全工具标记或限定工具标识时不会阻止普通工具调用。
#[test]
fn streaming_tool_executor_does_not_reject_when_scope_is_empty() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor = StreamingRuntimeToolExecutor::new(
        runtime_bridge,
        sender,
        Some(String::from("Scoped rejection is not active.")),
        false,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        tool_call_id: String::from("empty-scope-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "scope empty should still run" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(executor.execute_tool(&request))
        .expect("empty rejection scope should allow the host tool call to run");
    let events = drain_live_events(&mut receiver);

    assert!(result.is_error);
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolStarted
    )));
}

/// Verifies scoped policy rejection only blocks the listed tool identifiers.
/// 验证限定范围的策略拒绝只会阻止列出的工具标识。
#[test]
fn streaming_tool_executor_rejects_only_scoped_tool_ids() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor = StreamingRuntimeToolExecutor::new(
        runtime_bridge,
        sender,
        Some(String::from(
            "This tool is reserved for the parent session.",
        )),
        false,
        vec![String::from("failing_tool")],
    );
    let rejected_request = ToolInvocationRequest {
        tool_call_id: String::from("scoped-rejected-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "must be rejected before the failing body runs" }),
    };
    let allowed_request = ToolInvocationRequest {
        tool_call_id: String::from("scoped-allowed-call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let rejected_result = runtime
        .block_on(executor.execute_tool(&rejected_request))
        .expect("scoped policy rejection should return a normal tool result");
    let allowed_result = runtime
        .block_on(executor.execute_tool(&allowed_request))
        .expect("unlisted tool should execute normally");
    let events = drain_live_events(&mut receiver);

    assert!(!rejected_result.is_error);
    assert!(
        rejected_result
            .model_output
            .contains("Tool request rejected by runtime policy")
    );
    assert_eq!(
        rejected_result
            .host_result
            .as_ref()
            .and_then(|value| value.get("payload"))
            .and_then(|payload| payload.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("rejected")
    );
    assert!(!allowed_result.is_error);
    assert_eq!(
        allowed_result
            .host_result
            .as_ref()
            .and_then(|value| value.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("change_set")
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                ProviderStreamDelta::RuntimeEvent(runtime_event)
                    if runtime_event.kind == RuntimeEventKind::ToolFinished
            ))
            .count(),
        2
    );
}

/// Verifies the streaming executor yields the tool-start event before the actual tool body begins.
/// 验证流式执行器会在真实工具主体开始前先产出工具开始事件。
#[test]
fn streaming_tool_executor_yields_tool_started_before_tool_body_runs() {
    let entered_tool_body = Arc::new(AtomicBool::new(false));
    let release_tool_body = Arc::new(Notify::new());
    let (manager, definitions) = build_gated_demo_manager(
        Arc::clone(&entered_tool_body),
        Arc::clone(&release_tool_body),
    );
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("gated-call-1"),
        tool_name: String::from("gated_tool"),
        arguments: json!({ "query": "先显示工具调用" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        let task = tokio::spawn(async move { executor.execute_tool(&request).await });
        let first_event = receiver
            .recv()
            .await
            .expect("tool-start event should be sent")
            .expect("tool-start event should be ok");

        assert!(matches!(
            first_event,
            ProviderStreamDelta::RuntimeEvent(runtime_event)
                if runtime_event.kind == RuntimeEventKind::ToolStarted
                    && runtime_event
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("tool_call_id"))
                        .and_then(serde_json::Value::as_str)
                        == Some("gated-call-1")
        ));
        assert!(
            !entered_tool_body.load(Ordering::SeqCst),
            "tool body should not run before the UI-visible start event is observable"
        );

        release_tool_body.notify_one();
        let result = task
            .await
            .expect("tool task should join")
            .expect("tool call should complete");
        assert_eq!(result.model_output, "gated done");
    });
}

/// Verifies the streaming executor waits for the terminal tool event projection before returning to Core.
/// 验证流式执行器会等待工具终态事件完成投影后再返回给 Core。
#[test]
fn streaming_tool_executor_waits_for_tool_finished_projection_before_returning() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("barrier-call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        set_runtime_event_projection_barriers_enabled(false);
        set_runtime_event_projection_barriers_enabled(true);
        let _barrier_reset = RuntimeProjectionBarrierResetGuard;
        let task = tokio::spawn(async move { executor.execute_tool(&request).await });

        let started_event = receiver
            .recv()
            .await
            .expect("tool-start event should be sent")
            .expect("tool-start event should be ok");
        let ProviderStreamDelta::RuntimeEvent(started_event) = started_event else {
            panic!("first delta should be a tool-start runtime event");
        };
        assert_eq!(started_event.kind, RuntimeEventKind::ToolStarted);
        assert!(
            acknowledge_runtime_event_projection_barrier_from_event(&started_event),
            "tool-start barrier should acknowledge"
        );

        let finished_event = receiver
            .recv()
            .await
            .expect("tool-finished event should be sent")
            .expect("tool-finished event should be ok");
        let ProviderStreamDelta::RuntimeEvent(finished_event) = finished_event else {
            panic!("second delta should be a tool-finished runtime event");
        };
        assert_eq!(finished_event.kind, RuntimeEventKind::ToolFinished);
        assert!(
            !task.is_finished(),
            "execute_tool must wait until the terminal projection is acknowledged"
        );

        assert!(
            acknowledge_runtime_event_projection_barrier_from_event(&finished_event),
            "tool-finished barrier should acknowledge"
        );
        let result = task
            .await
            .expect("tool task should join")
            .expect("tool call should complete");
        assert!(result.model_output.contains("src/main.rs"));
    });
}
