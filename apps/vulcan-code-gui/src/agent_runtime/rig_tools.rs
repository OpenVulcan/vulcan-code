//! Rig-facing dynamic tool adapters backed by the standalone Vulcan tools manager crate.
//! 基于独立 VulcanToolsManager crate 的面向 rig 的动态工具适配器。

use crate::model::SessionId;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use rig::wasm_compat::WasmBoxedFuture;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};
use vulcan_core::tool::VulcanToolExecutor;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ToolInvocationRequest as CoreToolInvocationRequest,
    ToolInvocationResult as CoreToolInvocationResult,
};
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolInvocationResult, VulcanToolsManager,
};

/// Lightweight tool-call error wrapper used when one host-managed tool fails inside rig.
/// 当单个宿主管理工具在 rig 内部执行失败时使用的轻量工具错误包装器。
#[derive(Debug)]
struct VulcanRigToolError {
    /// Human-readable error message forwarded to the rig tool runtime.
    /// 转发给 rig 工具运行时的人类可读错误消息。
    message: String,
}

impl fmt::Display for VulcanRigToolError {
    /// Render the rig-facing host-managed tool error as one human-readable message.
    /// 将面向 rig 的宿主管理工具错误渲染为一条人类可读消息。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for VulcanRigToolError {}

/// Recorded host-side tool outcome captured during one rig-backed execution turn.
/// 在单次 rig 驱动执行回合中捕获的宿主侧工具结果记录。
#[derive(Debug, Clone)]
pub struct RecordedRigToolOutcome {
    /// Optional host-only structured metadata preserved outside the model-visible text.
    /// 保留在模型可见文本之外的可选宿主专用结构化元数据。
    pub metadata: Option<Value>,
}

/// Shared FIFO capture used to bridge host-only tool metadata back into GUI runtime events.
/// 用于把宿主专用工具元数据桥接回 GUI 运行时事件的共享先进先出捕获器。
#[derive(Debug, Clone, Default)]
pub struct RigToolOutcomeCapture {
    /// Shared queue that preserves tool execution order within one run.
    /// 在单次运行内保留工具执行顺序的共享队列。
    queue: Arc<Mutex<VecDeque<RecordedRigToolOutcome>>>,
}

impl RigToolOutcomeCapture {
    /// Records one completed tool outcome so the executor can attach metadata to the matching runtime event later.
    /// 记录一条已完成工具结果，便于执行器稍后把元数据附着到对应运行时事件上。
    pub fn record(&self, definition: &ResolvedToolDefinition, result: &VulcanToolInvocationResult) {
        let metadata = result.host_result.as_ref().map(|host_result| {
            json!({
                "tool_name": definition.descriptor.id.clone(),
                "tool_source": format!("{:?}", definition.descriptor.source).to_ascii_lowercase(),
                "host_result": {
                    "kind": host_result.kind.clone(),
                    "payload": host_result.payload.clone(),
                },
            })
        });
        let outcome = RecordedRigToolOutcome { metadata };
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        queue.push_back(outcome);
    }

    /// Records one empty placeholder so tool-result ordering remains stable even when host execution fails before metadata exists.
    /// 记录一条空占位结果，确保宿主执行在元数据生成前失败时工具返回顺序仍然稳定。
    pub fn record_placeholder(&self) {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        queue.push_back(RecordedRigToolOutcome { metadata: None });
    }

    /// Takes the next recorded outcome in execution order for one incoming tool-result runtime event.
    /// 按执行顺序取出下一条已记录结果，供单条工具返回运行时事件使用。
    pub fn take_next(&self) -> Option<RecordedRigToolOutcome> {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        queue.pop_front()
    }
}

/// Shared host-managed tool bridge that backs both rig adapters and future `vulcan-core` host-side tool execution.
/// 同时支撑 rig 适配器与未来 `vulcan-core` 宿主侧工具执行的共享宿主管理工具桥接层。
#[derive(Clone)]
pub struct VulcanManagedToolRuntimeBridge {
    /// Shared host-owned unified tools manager used for the final dispatch.
    /// 用于最终调度的共享宿主自有统一工具管理器。
    manager: Arc<VulcanToolsManager>,
    /// Stable resolved tool definitions indexed by canonical tool identifier.
    /// 以规范工具标识建立索引的稳定已解析工具定义集合。
    definitions_by_id: HashMap<String, ResolvedToolDefinition>,
    /// GUI session identifier attached to every host-managed tool invocation.
    /// 附着到每一次宿主管理工具调用上的图形界面会话标识。
    session_id: SessionId,
}

impl VulcanManagedToolRuntimeBridge {
    /// Builds one shared runtime bridge from the current tools manager, resolved definitions, and GUI session.
    /// 根据当前工具管理器、已解析定义与图形界面会话构建共享运行时桥接层。
    pub fn new(
        manager: Arc<VulcanToolsManager>,
        definitions: &[ResolvedToolDefinition],
        session_id: SessionId,
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
            .invoke_tool(definition, args, Some(self.session_id.to_string()))
            .await
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
            return Err(VulcanCoreError::InvalidState {
                message: format!(
                    "工具 `{}` 当前未在已解析定义集合中注册。",
                    request.tool_name
                ),
            });
        };
        let result = self
            .invoke_definition(definition, request.arguments.clone())
            .await
            .map_err(|message| VulcanCoreError::ToolFailure {
                tool_name: request.tool_name.clone(),
                message,
            })?;

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

/// One dynamic rig tool backed by one resolved host-managed tool definition.
/// 由单个已解析宿主管理工具定义驱动的动态 rig 工具。
pub struct VulcanManagedRigTool {
    /// Shared host-managed runtime bridge used for the final dispatch.
    /// 用于最终调度的共享宿主管理运行时桥接层。
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    /// Stable resolved tool definition mirrored into the rig tool definition.
    /// 被镜像到 rig 工具定义中的稳定已解析工具定义。
    definition: ResolvedToolDefinition,
    /// Shared host-only outcome capture used to reattach metadata after rig emits the tool result.
    /// 在 rig 发出工具结果后重新附着元数据时使用的共享宿主专用结果捕获器。
    outcome_capture: RigToolOutcomeCapture,
}

impl VulcanManagedRigTool {
    /// Builds one rig-facing dynamic tool adapter from the host-managed tool definition.
    /// 根据宿主管理工具定义构建单个面向 rig 的动态工具适配器。
    pub fn new(
        runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
        definition: ResolvedToolDefinition,
        outcome_capture: RigToolOutcomeCapture,
    ) -> Self {
        Self {
            runtime_bridge,
            definition,
            outcome_capture,
        }
    }
}

impl ToolDyn for VulcanManagedRigTool {
    /// Return the stable host-managed tool identifier as the rig tool identifier.
    /// 返回稳定宿主管理工具标识，作为 rig 工具标识。
    fn name(&self) -> String {
        self.definition.descriptor.id.clone()
    }

    /// Build one rig tool definition from the host-managed resolved tool definition.
    /// 根据宿主管理的已解析工具定义构建单个 rig 工具定义。
    fn definition<'a>(&'a self, _prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: self.definition.descriptor.id.clone(),
                description: self.definition.descriptor.description.clone(),
                parameters: self.definition.parameters_schema.clone(),
            }
        })
    }

    /// Dispatch the tool call through the shared host-owned tools manager and serialize the structured result for rig.
    /// 通过共享宿主自有工具管理器调度工具调用，并为 rig 序列化结构化结果。
    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let args_json: Value = match serde_json::from_str(&args) {
                Ok(args_json) => args_json,
                Err(error) => {
                    self.outcome_capture.record_placeholder();
                    return Err(ToolError::JsonError(error));
                }
            };
            let result = self
                .runtime_bridge
                .invoke_definition(&self.definition, args_json)
                .await
                .map_err(|error| {
                    self.outcome_capture.record_placeholder();
                    ToolError::ToolCallError(Box::new(VulcanRigToolError { message: error }))
                })?;
            self.outcome_capture.record(&self.definition, &result);
            match result.model_output {
                Value::String(text) => Ok(text),
                other => serde_json::to_string(&other).map_err(ToolError::JsonError),
            }
        })
    }
}

/// Build the rig-facing unified dynamic tool set for one GUI session.
/// 为单个图形界面会话构建面向 rig 的统一动态工具集合。
pub fn build_vulcan_rig_tools(
    manager: VulcanToolsManager,
    session_id: SessionId,
    definitions: &[ResolvedToolDefinition],
    outcome_capture: RigToolOutcomeCapture,
) -> Vec<Box<dyn ToolDyn>> {
    let manager = Arc::new(manager);
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        manager.clone(),
        definitions,
        session_id,
    ));

    definitions
        .iter()
        .cloned()
        .map(|definition| {
            Box::new(VulcanManagedRigTool::new(
                runtime_bridge.clone(),
                definition,
                outcome_capture.clone(),
            )) as Box<dyn ToolDyn>
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::VulcanManagedToolRuntimeBridge;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
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
        let manager = VulcanToolsManager::new(vec![VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("demo_tool"),
                name: String::from("demo_tool"),
                description: String::from("Demo tool"),
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
        }]);
        let resolved = manager.resolve_for_execution_mode("Build");
        (manager, resolved.definitions)
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
            VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil());
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

    /// Verifies the host-owned `vulcan-core` loop can execute through the shared GUI tool bridge and preserve host metadata in canonical records.
    /// 验证宿主自有的 `vulcan-core` 循环能够经由共享 GUI 工具桥接执行，并在规范记录中保留宿主元数据。
    #[test]
    fn managed_tool_runtime_bridge_integrates_with_core_controller_loop() {
        let (manager, definitions) = build_demo_manager();
        let runtime_bridge =
            VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil());
        let provider = MockControllerProviderAdapter::new(vec![
            ProviderTurnResponse {
                assistant_text: String::new(),
                assistant_reasoning: String::from("先调用工具获取文件状态。"),
                provider_message_id: Some(String::from("provider-1")),
                tool_calls: vec![ConversationToolCall {
                    id: String::from("tool-item-1"),
                    tool_call_id: Some(String::from("tool-call-1")),
                    tool_name: String::from("demo_tool"),
                    arguments: json!({ "path": "src/main.rs" }),
                }],
                continuation: None,
            },
            ProviderTurnResponse {
                assistant_text: String::from("工具执行完成，已经拿到宿主结果。"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("provider-2")),
                tool_calls: Vec::new(),
                continuation: None,
            },
        ]);
        let mut controller = VulcanCoreController::new(
            ControllerConfig {
                provider_id: String::from("mock-openai"),
                model_id: String::from("mock-model"),
                system_prompt: String::from("system"),
                temperature: Some(0.2),
                max_turns: 3,
                additional_params: None,
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
}
