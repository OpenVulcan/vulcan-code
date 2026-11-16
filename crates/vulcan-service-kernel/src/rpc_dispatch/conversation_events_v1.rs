//! V1 conversation-event RPC routing.
//! V1 会话事件 RPC 路由。

use super::*;
use crate::conversation_history_cleanup::{
    ConversationHistoryRequest, service_conversation_history,
};
use vulcan_service_api::{
    ConversationEditGenerationV1Request, ConversationEventHydrationV1Request,
    ConversationEventPageV1Request, ConversationEventReconnectV1Request,
    ConversationForkGenerationV1Request,
};

/// Exact public RPC wire names owned by the V1 event-window router.
/// V1 事件窗口路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_session_event_hydration_v1",
    "service_session_event_page_v1",
    "service_session_event_reconnect_v1",
    "service_edit_conversation_generation_v1",
    "service_fork_conversation_generation_v1",
    "service_conversation_history_v1",
    "service_latest_request_context_v1",
    "service_simulate_request_context_v1",
];

/// Dispatches one V1 conversation-event RPC with strict whole-request parsing.
/// 使用严格整请求解析分发一个 V1 会话事件 RPC。
///
/// # Parameters
/// 参数
/// `state` owns the service runtime, `method` is the exact wire name, and
/// `params` is the complete normalized request object.
/// `state` 持有服务运行时，`method` 是精确线协议名称，`params` 是完整归一化请求对象。
///
/// # Returns
/// 返回值
/// Returns `Handled` for the three owned methods and `Unknown` for every other
/// domain.
/// 对三个自有方法返回 `Handled`，对其他领域返回 `Unknown`。
pub(super) async fn dispatch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    let result = match method {
        "service_session_event_hydration_v1" => rpc_json_result(
            service_session_event_hydration_v1(
                state,
                parse_request::<ConversationEventHydrationV1Request>(params, method)?,
            )
            .await,
        ),
        "service_session_event_page_v1" => rpc_json_result(
            service_session_event_page_v1(
                state,
                parse_request::<ConversationEventPageV1Request>(params, method)?,
            )
            .await,
        ),
        "service_session_event_reconnect_v1" => rpc_json_result(
            service_session_event_reconnect_v1(
                state,
                parse_request::<ConversationEventReconnectV1Request>(params, method)?,
            )
            .await,
        ),
        "service_edit_conversation_generation_v1" => rpc_json_result(
            service_edit_conversation_generation_v1(
                app.clone(),
                state,
                parse_request::<ConversationEditGenerationV1Request>(params, method)?,
            )
            .await,
        ),
        "service_fork_conversation_generation_v1" => rpc_json_result(
            service_fork_conversation_generation_v1(
                app,
                state,
                parse_request::<ConversationForkGenerationV1Request>(params, method)?,
            )
            .await,
        ),
        "service_conversation_history_v1" => rpc_json_result(
            service_conversation_history(
                state,
                parse_request::<ConversationHistoryRequest>(params, method)?,
            )
            .await,
        ),
        "service_latest_request_context_v1" => rpc_json_result(
            latest_request_context(
                state,
                parse_request::<LatestRequestContextInput>(params, method)?,
            )
            .await,
        ),
        "service_simulate_request_context_v1" => {
            // The simulation admits no new input and assembles against one published storage locator.
            // 模拟不准入新输入，只针对一个已发布的存储定位器组装。
            let request = parse_request::<SimulateRequestContextInput>(params, method)?;
            let result = async {
                let (session_id, locator, _) =
                    crate::conversation_events_v1::resolve_exact_locator_and_registry(
                        &state,
                        &request.session_id,
                    )
                    .await?;
                // Read the exact project and agent configuration used for conversation prompt sources.
                // 读取会话提示词来源对应的精确项目和代理配置。
                let (workspace_path, runtime) = {
                    // Capture local sources before leaving the service mutex for composition.
                    // 离开服务互斥锁前获取本地来源，随后执行组装。
                    let guard = state
                        .lock()
                        .map_err(|error| format!("failed to lock service state: {error}"))?;
                    let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
                        String::from("Workspace state is not ready to simulate request context.")
                    })?;
                    let project = crate::runtime_workspace_helpers::workspace_task_project_for_project_id_including_archived(
                        workspace,
                        locator.project_id(),
                    )
                    .ok_or_else(|| format!("Workspace project `{}` was not found.", locator.project_id()))?;
                    let runtime = guard.agent_runtime.as_ref().ok_or_else(|| {
                        String::from("Agent runtime is not ready to simulate request context.")
                    })?;
                    (project.path.clone(), runtime.clone())
                };
                // Read the same knowledge authorization used by turn preparation.
                // 读取回合准备所用的同一份知识库授权。
                let knowledge_access =
                    crate::agent_stream::resolve_turn_knowledge_access(&state, session_id).await;
                tokio::task::spawn_blocking(move || {
                    // Pin prompt and tool definitions to the same agent configuration.
                    // 将提示词与工具定义固定在同一份代理配置上。
                    let agent_config = vulcan_agent_runtime::agent_runtime::config::AgentRuntimeConfig::from_sources(
                        &runtime.provider_configuration,
                        &runtime.agent_profile_catalog,
                    );
                    crate::session_runtime_registry_v1::simulate_request_context(
                        &locator,
                        &workspace_path,
                        &agent_config,
                        |loaded| crate::session_runtime_registry_v1::preview_session_tool_definitions(
                            loaded, session_id, &workspace_path, &app, &state, &runtime, knowledge_access?,
                        ),
                    )
                })
                .await
                .map_err(|error| error.to_string())?
            }
            .await;
            rpc_json_result(result)
        }
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}

/// Identifies the existing conversation whose context will be assembled locally.
/// 标识将在本机组装上下文的已有会话。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SimulateRequestContextInput {
    /// Exact existing session ID.
    /// 精确的已有会话 ID。
    session_id: String,
}

/// Reads only the latest physical request for one exact session.
/// 只读取精确会话的最近一次物理请求。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LatestRequestContextInput {
    /// Indexed session owning the live snapshot.
    /// 拥有实时快照的索引会话。
    session_id: String,
    /// Optional execution ID already displayed by the caller.
    /// 调用者已显示的可选执行标识。
    known_execution_id: Option<String>,
    /// Whether the caller already has the finalized assistant text for that execution.
    /// 调用方是否已经取得该执行已完成的 assistant 正文。
    #[serde(default)]
    known_assistant_response: bool,
}

/// Returns the current context or a lightweight unchanged/empty result.
/// 返回当前上下文，或轻量的未更新／空结果。
async fn latest_request_context(
    state: SharedServiceBridgeState,
    input: LatestRequestContextInput,
) -> Result<Value, String> {
    use crate::session_runtime_registry_v1::SessionRuntimeError;

    // Resolve the same indexed owner as the V1 conversation endpoints, without loading its history.
    // 解析与 V1 会话入口相同的索引所有者，无需加载会话历史。
    let (session_id, _, registry) =
        crate::conversation_events_v1::resolve_exact_locator_and_registry(
            &state,
            &input.session_id,
        )
        .await?;
    let handle = match registry.get(session_id) {
        Ok(handle) => handle,
        Err(SessionRuntimeError::SessionNotRegistered(_)) => {
            return Ok(serde_json::json!({ "status": "empty" }));
        }
        Err(error) => return Err(error.to_string()),
    };
    let Some(snapshot) = handle
        .provider_api_trace_store()
        .and_then(|owner| owner.latest_request_context())
    else {
        return Ok(serde_json::json!({ "status": "empty" }));
    };
    // A matching execution may gain its assistant response after its request has been displayed.
    // 请求显示后，相同执行仍可能新增其 assistant 回复。
    let known_execution =
        input.known_execution_id.as_deref() == Some(snapshot.dispatch.execution_id.as_str());
    if known_execution && input.known_assistant_response {
        return Ok(serde_json::json!({ "status": "unchanged" }));
    }
    // API IDs bind each reply to the exact request even when an assistant emits several steps per turn.
    // API 标识把每条回复绑定到精确请求，即使助手在一轮内输出多个步骤。
    let api_call_id = snapshot
        .dispatch
        .api_call_id
        .parse::<u64>()
        .map_err(|error| format!("The dispatched context has an invalid API call ID: {error}"))?;
    let assistant_response = handle
        .read(|document| {
            crate::session_runtime_registry_v1::latest_context_assistant_response(
                document.events(),
                api_call_id,
            )
        })
        .map_err(|error| error.to_string())?;
    if known_execution && assistant_response.is_none() {
        return Ok(serde_json::json!({ "status": "unchanged" }));
    }
    Ok(serde_json::json!({
        "status": "available",
        "snapshot": snapshot.as_ref(),
        "assistantResponse": assistant_response,
    }))
}

/// Deserializes one complete strict V1 request object.
/// 反序列化一个完整严格 V1 请求对象。
fn parse_request<T: serde::de::DeserializeOwned>(
    params: &Value,
    method: &str,
) -> Result<T, WebuiServiceRpcError> {
    serde_json::from_value(params.clone()).map_err(|error| {
        WebuiServiceRpcError::InvalidParams(format!("invalid params for {method}: {error}"))
    })
}
