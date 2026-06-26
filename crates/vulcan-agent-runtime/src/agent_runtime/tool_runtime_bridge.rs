//! Shared host-managed tool runtime bridge for the host executor and `vulcan-core` controller.
//! 供宿主执行器与 `vulcan-core` 控制器共享的宿主管理工具运行时桥接层。

use crate::agent_runtime::events::RuntimeEvent;
use crate::model::SessionId;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;
use vulcan_core::tool::VulcanToolExecutor;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ToolInvocationRequest as CoreToolInvocationRequest,
    ToolInvocationResult as CoreToolInvocationResult,
};
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolInvocationResult, VulcanToolsManager,
};

/// Shared host-managed tool bridge that backs the host executor and `vulcan-core` host-side tool execution.
/// 同时支撑宿主执行器与 `vulcan-core` 宿主侧工具执行的共享宿主管理工具桥接层。
#[derive(Clone)]
pub struct VulcanManagedToolRuntimeBridge {
    /// Shared host-owned unified tools manager used for the final dispatch.
    /// 用于最终调度的共享宿主自有统一工具管理器。
    manager: Arc<VulcanToolsManager>,
    /// Stable resolved tool definitions indexed by canonical tool identifier.
    /// 以规范工具标识建立索引的稳定已解析工具定义集合。
    definitions_by_id: HashMap<String, ResolvedToolDefinition>,
    /// Workbench session identifier attached to every host-managed tool invocation.
    /// 附着到每一次宿主管理工具调用上的工作台会话标识。
    session_id: SessionId,
    /// Execution mode label used by invocation-time Plan-mode guards.
    /// 调用期计划模式守卫使用的执行模式标签。
    execution_mode: String,
}

/// Metadata key used to connect one runtime-event projection to its tool-execution barrier.
/// 用于把单条运行时事件投影与工具执行屏障关联起来的元数据键。
pub const RUNTIME_EVENT_PROJECTION_BARRIER_ID: &str = "webui_projection_barrier_id";

/// Enables projection barriers only when a service consumer can acknowledge them.
/// 仅在存在可确认投影的服务消费端时启用投影屏障。
static RUNTIME_EVENT_PROJECTION_BARRIERS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Stores pending projection barrier acknowledgements by stable barrier id.
/// 按稳定屏障标识保存等待中的投影确认发送端。
static RUNTIME_EVENT_PROJECTION_BARRIERS: OnceLock<Mutex<HashMap<String, oneshot::Sender<()>>>> =
    OnceLock::new();

/// Maximum time a tool may wait for the UI projection acknowledgement before continuing.
/// 工具等待界面投影确认后继续执行的最长时间。
const RUNTIME_EVENT_PROJECTION_BARRIER_TIMEOUT: Duration = Duration::from_secs(8);

/// One pending projection barrier created for a runtime start event.
/// 为一条运行时开始事件创建的待确认投影屏障。
pub(crate) struct RuntimeEventProjectionBarrier {
    /// Stable barrier id copied into runtime-event metadata.
    /// 复制到运行时事件元数据中的稳定屏障标识。
    id: String,
    /// Receiver resolved by the host bridge after it emits the projected workbench snapshot.
    /// 宿主桥接层完成投影快照发送后解析的接收端。
    receiver: oneshot::Receiver<()>,
}

impl VulcanManagedToolRuntimeBridge {
    /// Builds one shared runtime bridge from the current tools manager, resolved definitions, and workbench session.
    /// 根据当前工具管理器、已解析定义与工作台会话构建共享运行时桥接层。
    pub fn new(
        manager: Arc<VulcanToolsManager>,
        definitions: &[ResolvedToolDefinition],
        session_id: SessionId,
        execution_mode: impl Into<String>,
    ) -> Self {
        let definitions_by_id = definitions
            .iter()
            .cloned()
            .map(|definition| (definition.descriptor.id.clone(), definition))
            .collect::<HashMap<_, _>>();

        Self {
            manager,
            definitions_by_id,
            session_id,
            execution_mode: execution_mode.into(),
        }
    }

    /// Dispatches one resolved host-managed tool definition through the shared tools manager.
    /// 通过共享工具管理器调度一条已解析宿主管理工具定义。
    pub async fn invoke_definition(
        &self,
        definition: &ResolvedToolDefinition,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        self.manager
            .invoke_tool(
                definition,
                args,
                Some(self.session_id.to_string()),
                &self.execution_mode,
            )
            .await
    }
}

/// Enables or disables runtime-event projection barriers for host-owned streams.
/// 为宿主拥有的流启用或停用运行时事件投影屏障。
pub fn set_runtime_event_projection_barriers_enabled(enabled: bool) {
    RUNTIME_EVENT_PROJECTION_BARRIERS_ENABLED.store(enabled, Ordering::SeqCst);
    if !enabled
        && let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get()
        && let Ok(mut guard) = registry.lock()
    {
        guard.clear();
    }
}

/// Attaches a projection barrier id to one runtime event when service barriers are enabled.
/// 当服务屏障启用时，为单条运行时事件附加投影屏障标识。
pub(crate) fn attach_runtime_event_projection_barrier(
    mut event: RuntimeEvent,
) -> (RuntimeEvent, Option<RuntimeEventProjectionBarrier>) {
    if !RUNTIME_EVENT_PROJECTION_BARRIERS_ENABLED.load(Ordering::SeqCst) {
        return (event, None);
    }

    let barrier_id = Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel();
    let registry = RUNTIME_EVENT_PROJECTION_BARRIERS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = registry.lock() {
        guard.insert(barrier_id.clone(), sender);
    } else {
        return (event, None);
    }

    let mut metadata = match event.metadata.take() {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = Map::new();
            map.insert(String::from("payload"), other);
            map
        }
        None => Map::new(),
    };
    metadata.insert(
        String::from(RUNTIME_EVENT_PROJECTION_BARRIER_ID),
        Value::String(barrier_id.clone()),
    );
    event.metadata = Some(Value::Object(metadata));

    (
        event,
        Some(RuntimeEventProjectionBarrier {
            id: barrier_id,
            receiver,
        }),
    )
}

/// Waits until the host bridge confirms the start event has been projected.
/// 等待宿主桥接层确认开始事件已经完成投影。
pub(crate) async fn wait_runtime_event_projection_barrier(
    barrier: Option<RuntimeEventProjectionBarrier>,
) {
    let Some(barrier) = barrier else {
        return;
    };
    let barrier_id = barrier.id.clone();
    if tokio::time::timeout(RUNTIME_EVENT_PROJECTION_BARRIER_TIMEOUT, barrier.receiver)
        .await
        .is_err()
    {
        if let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get()
            && let Ok(mut guard) = registry.lock()
        {
            guard.remove(&barrier_id);
        }
    }
}

/// Acknowledges one projection barrier from runtime-event metadata after the service emits it.
/// 服务发出运行时事件投影后，根据事件元数据确认对应投影屏障。
pub fn acknowledge_runtime_event_projection_barrier_from_event(event: &RuntimeEvent) -> bool {
    let Some(barrier_id) = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get(RUNTIME_EVENT_PROJECTION_BARRIER_ID))
        .and_then(Value::as_str)
    else {
        return false;
    };
    let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get() else {
        return false;
    };
    let sender = registry
        .lock()
        .ok()
        .and_then(|mut guard| guard.remove(barrier_id));
    if let Some(sender) = sender {
        let _ = sender.send(());
        true
    } else {
        false
    }
}

/// Builds one controller-visible error result so host-managed tool failures can be returned to the model instead of aborting the whole turn.
/// 构建一条控制器可见的错误结果，使宿主管理工具失败能够回传给模型，而不是直接中断整轮执行。
pub(crate) fn build_tool_failure_invocation_result(
    request: &CoreToolInvocationRequest,
    message: impl Into<String>,
) -> CoreToolInvocationResult {
    let message = message.into();
    let model_output = format!(
        "Tool `{}` failed.\nReason: {}\nAdjust the arguments, narrow the scope, or switch to a more appropriate tool before continuing.",
        request.tool_name, message
    );
    CoreToolInvocationResult {
        tool_call_id: request.tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        model_output,
        host_result: Some(json!({
            "kind": "tool_error",
            "payload": {
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "status": "failed",
                "error": message,
                "arguments": request.arguments,
            }
        })),
        is_error: true,
    }
}

#[async_trait::async_trait]
impl VulcanToolExecutor for VulcanManagedToolRuntimeBridge {
    /// Executes one controller-owned tool request through the shared host-managed tool bridge.
    /// 通过共享宿主管理工具桥接层执行一次控制器自有工具请求。
    async fn execute_tool(
        &self,
        request: &CoreToolInvocationRequest,
    ) -> Result<CoreToolInvocationResult, VulcanCoreError> {
        let Some(definition) = self.definitions_by_id.get(&request.tool_name) else {
            return Ok(build_tool_failure_invocation_result(
                request,
                format!(
                    "Tool `{}` is not registered in the resolved definition set.",
                    request.tool_name
                ),
            ));
        };
        let result = match self
            .invoke_definition(definition, request.arguments.clone())
            .await
        {
            Ok(result) => result,
            Err(message) => return Ok(build_tool_failure_invocation_result(request, message)),
        };

        Ok(CoreToolInvocationResult {
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            model_output: match result.model_output {
                Value::String(text) => text,
                other => serde_json::to_string(&other).unwrap_or_else(|_| other.to_string()),
            },
            host_result: result.host_result.as_ref().map(|host_result| {
                json!({
                    "kind": host_result.kind.clone(),
                    "payload": host_result.payload.clone(),
                })
            }),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::VulcanManagedToolRuntimeBridge;
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
        VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
        VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
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
                    Box::pin(async move {
                        Err(String::from("LuaSkills 工具解析失败：演示错误"))
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
        let runtime_bridge = VulcanManagedToolRuntimeBridge::new(
            Arc::new(manager),
            &definitions,
            Uuid::nil(),
            "Build",
        );
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
        let runtime_bridge = VulcanManagedToolRuntimeBridge::new(
            Arc::new(manager),
            &definitions,
            Uuid::nil(),
            "Build",
        );
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
        let runtime_bridge = VulcanManagedToolRuntimeBridge::new(
            Arc::new(manager),
            &definitions,
            Uuid::nil(),
            "Plan",
        );
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
        let runtime_bridge = VulcanManagedToolRuntimeBridge::new(
            Arc::new(manager),
            &definitions,
            Uuid::nil(),
            "Build",
        );
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
        );
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
        let runtime_bridge = VulcanManagedToolRuntimeBridge::new(
            Arc::new(manager),
            &definitions,
            Uuid::nil(),
            "Build",
        );
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
        );
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
        let executor = StreamingRuntimeToolExecutor::new(runtime_bridge, sender, false);
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

    /// Verifies a model-visible tool error still displays as completed when the host invocation returned normally.
    /// 验证宿主调用正常返回时，即使工具结果对模型可见为错误，界面仍显示为已完成。
    #[test]
    fn streaming_tool_executor_marks_normal_error_results_completed() {
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
        let executor = StreamingRuntimeToolExecutor::new(runtime_bridge, sender, false);
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
                        == Some("completed")
        )));
    }

    /// Verifies rejected tool requests do not emit a start event or enter the tool body.
    /// 验证被拒绝的工具请求不会发出开始事件，也不会进入工具主体。
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
        let executor = StreamingRuntimeToolExecutor::new(runtime_bridge, sender, true);
        let request = ToolInvocationRequest {
            tool_call_id: String::from("rejected-call-1"),
            tool_name: String::from("gated_tool"),
            arguments: json!({ "query": "must not run" }),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let result = runtime.block_on(executor.execute_tool(&request));

        assert!(matches!(
            result,
            Err(VulcanCoreError::ToolFailure { ref tool_name, ref message })
                if tool_name == "gated_tool"
                    && message.contains("Tool calls are not allowed during context compaction")
        ));
        assert!(
            !entered_tool_body.load(Ordering::SeqCst),
            "rejected tool request must not enter the host tool body"
        );
        assert!(
            receiver.try_recv().is_err(),
            "rejected tool request must not emit a tool-start event"
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
        let executor = StreamingRuntimeToolExecutor::new(runtime_bridge, sender, false);
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
}
