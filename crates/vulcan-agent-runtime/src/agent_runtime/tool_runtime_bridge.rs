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
    ResolvedToolDefinition, VULCAN_TOOL_ERROR_DETAIL_KIND, VulcanToolInvocationResult,
    VulcanToolsManager,
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

/// One pending projection barrier created for a runtime event.
/// 为一条运行时事件创建的待确认投影屏障。
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

/// Waits until the host bridge confirms the runtime event has been projected.
/// 等待宿主桥接层确认运行时事件已经完成投影。
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
        && let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get()
        && let Ok(mut guard) = registry.lock()
    {
        guard.remove(&barrier_id);
    }
}

/// Removes a projection barrier for a runtime event that will never reach the host bridge.
/// 移除一条永远不会到达宿主桥接层的运行时事件投影屏障。
pub(crate) fn cancel_runtime_event_projection_barrier(
    barrier: Option<RuntimeEventProjectionBarrier>,
) {
    let Some(barrier) = barrier else {
        return;
    };
    let Some(registry) = RUNTIME_EVENT_PROJECTION_BARRIERS.get() else {
        return;
    };
    let Ok(mut guard) = registry.lock() else {
        eprintln!(
            "[tool-runtime] runtime-event projection barrier registry is locked; barrier {} could not be cancelled",
            barrier.id
        );
        return;
    };
    guard.remove(&barrier.id);
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

/// Builds the canonical controller-visible tool-error host result with trusted invocation identity.
/// 使用可信调用身份构建规范的控制器可见工具错误宿主结果。
///
/// # Parameters
/// 参数
/// - `request`: Core-owned invocation identity and arguments.
/// - `request`：Core 持有的调用身份与参数。
/// - `error`: Human-readable error text exposed at the canonical payload root.
/// - `error`：暴露在规范载荷根部的人类可读错误文本。
/// - `details`: Optional tool-owned structured failure details.
/// - `details`：可选的工具自有结构化失败详情。
///
/// # Returns
/// 返回值
/// Canonical `tool_error` host result consumed by runtime and WebUI projections.
/// 由运行时与 WebUI 投影消费的规范 `tool_error` 宿主结果。
fn build_canonical_tool_error_host_result(
    request: &CoreToolInvocationRequest,
    error: &str,
    details: Option<Value>,
) -> Value {
    let mut payload = json!({
        "tool_call_id": request.tool_call_id,
        "tool_name": request.tool_name,
        "status": "failed",
        "error": error,
        "arguments": request.arguments,
    });
    if let Some(details) = details {
        payload["details"] = details;
    }
    json!({
        "kind": "tool_error",
        "payload": payload,
    })
}

/// Canonicalizes one tool-owned structured error detail at the invocation-identity boundary.
/// 在调用身份边界规范化一条工具自有结构化错误详情。
///
/// # Parameters
/// 参数
/// - `request`: Core-owned invocation identity and arguments.
/// - `request`：Core 持有的调用身份与参数。
/// - `details`: Tool-owned envelope containing failed status, non-empty error, and domain details.
/// - `details`：包含失败状态、非空错误与领域详情的工具自有信封。
///
/// # Returns
/// 返回值
/// Canonical host result, or an explicit protocol violation.
/// 规范宿主结果，或显式协议违规错误。
fn canonicalize_tool_error_details(
    request: &CoreToolInvocationRequest,
    details: &Value,
) -> Result<Value, String> {
    if details.get("status").and_then(Value::as_str) != Some("failed") {
        return Err(format!(
            "Tool `{}` returned `{VULCAN_TOOL_ERROR_DETAIL_KIND}` without `status: failed`.",
            request.tool_name
        ));
    }
    let error = details
        .get("error")
        .and_then(Value::as_str)
        .filter(|error| !error.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Tool `{}` returned `{VULCAN_TOOL_ERROR_DETAIL_KIND}` without a non-empty `error` string.",
                request.tool_name
            )
        })?;
    let domain_details = details.get("details").cloned().ok_or_else(|| {
        format!(
            "Tool `{}` returned `{VULCAN_TOOL_ERROR_DETAIL_KIND}` without `details`.",
            request.tool_name
        )
    })?;
    Ok(build_canonical_tool_error_host_result(
        request,
        error,
        Some(domain_details),
    ))
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
        host_result: Some(build_canonical_tool_error_host_result(
            request, &message, None,
        )),
        is_error: true,
    }
}

/// Builds one controller-visible policy rejection result without failing the provider turn.
/// 构建一条控制器可见的策略拒绝结果，同时不使 Provider 回合失败。
pub(crate) fn build_tool_rejection_invocation_result(
    request: &CoreToolInvocationRequest,
    message: impl Into<String>,
) -> CoreToolInvocationResult {
    let message = message.into();
    let model_output = format!(
        "Tool request rejected by runtime policy.\nTool: `{}`\nReason: {}\nUse another available tool or report this limitation to the parent agent.",
        request.tool_name, message
    );
    CoreToolInvocationResult {
        tool_call_id: request.tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        model_output,
        host_result: Some(json!({
            "kind": "tool_result",
            "payload": {
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "status": "rejected",
                "rejected": true,
                "reason": message,
                "arguments": request.arguments,
            }
        })),
        is_error: false,
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

        let (host_result, is_error) = match result.host_result.as_ref() {
            Some(host_result) if host_result.kind == VULCAN_TOOL_ERROR_DETAIL_KIND => {
                let canonical = match canonicalize_tool_error_details(request, &host_result.payload)
                {
                    Ok(canonical) => canonical,
                    Err(message) => {
                        return Ok(build_tool_failure_invocation_result(request, message));
                    }
                };
                (Some(canonical), true)
            }
            Some(host_result) if host_result.kind == "tool_error" => {
                return Ok(build_tool_failure_invocation_result(
                    request,
                    format!(
                        "Tool `{}` returned reserved host-result kind `tool_error`; return `{VULCAN_TOOL_ERROR_DETAIL_KIND}` details instead.",
                        request.tool_name
                    ),
                ));
            }
            Some(host_result) => (
                Some(json!({
                    "kind": host_result.kind.clone(),
                    "payload": host_result.payload.clone(),
                })),
                false,
            ),
            None => (None, false),
        };
        Ok(CoreToolInvocationResult {
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            model_output: match result.model_output {
                Value::String(text) => text,
                other => serde_json::to_string(&other).unwrap_or_else(|_| other.to_string()),
            },
            host_result,
            is_error,
        })
    }
}

#[cfg(test)]
mod tests;
