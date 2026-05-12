//! Shared host-controlled provider transport helpers reused by the provider-specific adapters.
//! 供各 Provider 专用适配器复用的共享宿主控制 Provider 传输辅助逻辑。

use crate::agent_runtime::provider_request_bridge::{
    build_openai_compatible_turn_request_snapshot, core_message_to_rig_message,
    core_messages_to_rig_history, core_snapshot_to_session_conversation_state,
    resolved_tool_definition_to_rig_tool_definition,
};
use crate::agent_runtime::provider_stream::{ProviderDeltaStream, ProviderStreamDelta};
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::model::SessionId;
use async_stream::stream;
use reqwest::header::HeaderMap;
use rig::completion::ToolDefinition as RigToolDefinition;
use rig::message::Message as RigMessage;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::VulcanStreamingProviderAdapter;
use vulcan_core::tool::AllowAllToolPolicy;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Shared live-delta sender used to forward request snapshots, streaming chunks, and runtime events from the host-owned controller task.
/// 用于从宿主自有控制器任务转发请求快照、流式分片与运行时事件的共享实时增量发送器。
pub(crate) type ProviderLiveDeltaSender = UnboundedSender<Result<ProviderStreamDelta, String>>;

/// Prepared host-side tool runtime context reused by provider transport wrappers so bridge creation and rig-tool mirroring stay consistent.
/// 供 Provider 传输包装器复用的宿主侧工具运行时上下文，使桥接层创建与 rig 工具镜像保持一致。
pub(crate) struct PreparedHostToolRuntimeContext {
    /// Shared host-managed bridge that owns the real tool dispatch path for the current execution turn.
    /// 持有当前执行回合真实工具调度路径的共享宿主管理桥接层。
    pub(crate) runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    /// Mirrored low-level rig tool definitions exposed to the transport request builder.
    /// 暴露给传输请求构建器的镜像低层 rig 工具定义集合。
    pub(crate) rig_tool_definitions: Vec<RigToolDefinition>,
}

/// Serializable OpenAI-compatible single-turn request owned by `vulcan-core` while preserving transport-only rig messages outside persistence.
/// 由 `vulcan-core` 持有的可序列化 OpenAI 兼容单回合请求，同时在持久化之外保留仅供传输使用的 rig 消息。
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct OpenAiCompatibleCoreTurnRequest {
    /// Structured request snapshot persisted for inspection and debugging.
    /// 被持久化用于检查与调试的结构化请求快照。
    #[serde(flatten)]
    pub(crate) snapshot: CoreProviderRequestSnapshot,
    /// Provider-visible history converted into rig messages for the real transport layer.
    /// 为真实传输层转换出的 Provider 可见 rig 历史消息。
    #[serde(skip_serializing)]
    pub(crate) current_history: Vec<RigMessage>,
    /// Last outgoing prompt message converted into the rig shape expected by the transport layer.
    /// 被转换为传输层所需 rig 形状的最后一条出站提示消息。
    #[serde(skip_serializing)]
    pub(crate) current_prompt: RigMessage,
}

/// Builds one exact host-controlled turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
/// 根据规范控制器快照构建一份精确宿主控制回合请求，并立即转发结构化快照。
pub(crate) fn build_host_controlled_turn_request(
    provider_id: &str,
    model_id: &str,
    request_label: &str,
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    resolved_tools: &[ResolvedToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
    additional_params: Option<Value>,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    let Some((current_prompt_message, current_history_messages)) = snapshot.messages.split_last()
    else {
        return Err(VulcanCoreError::InvalidState {
            message: format!(
                "{request_label} 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。"
            ),
        });
    };
    let current_history = core_messages_to_rig_history(current_history_messages);
    let current_prompt = core_message_to_rig_message(current_prompt_message);
    let request_snapshot = build_openai_compatible_turn_request_snapshot(
        provider_id,
        model_id,
        config.system_prompt.as_str(),
        &current_history,
        &current_prompt,
        config.temperature,
        config.max_turns,
        resolved_tools,
        request_headers,
        additional_params,
    );
    let _ = live_sender.send(Ok(ProviderStreamDelta::RequestSnapshot(
        request_snapshot.clone(),
    )));

    Ok(OpenAiCompatibleCoreTurnRequest {
        snapshot: request_snapshot,
        current_history,
        current_prompt,
    })
}

/// Prepares one shared host-managed tool bridge plus the mirrored rig tool definitions for the current execution turn.
/// 为当前执行回合准备共享宿主管理工具桥接层及其镜像 rig 工具定义集合。
pub(crate) fn prepare_host_tool_runtime_context(
    tools_manager: VulcanToolsManager,
    resolved_tools: &[ResolvedToolDefinition],
    session_id: SessionId,
) -> PreparedHostToolRuntimeContext {
    PreparedHostToolRuntimeContext {
        runtime_bridge: Arc::new(VulcanManagedToolRuntimeBridge::new(
            Arc::new(tools_manager),
            resolved_tools,
            session_id,
        )),
        rig_tool_definitions: resolved_tools
            .iter()
            .map(resolved_tool_definition_to_rig_tool_definition)
            .collect(),
    }
}

/// Executes one host-controlled provider stream through `vulcan-core`, forwards live deltas, and returns the final canonical state to the GUI layer.
/// 通过 `vulcan-core` 执行一次宿主控制的 Provider 流，转发实时增量，并把最终规范状态返回给图形界面层。
pub(crate) fn execute_host_controlled_stream<A, F>(
    controller_config: CoreControllerConfig,
    core_snapshot: CoreConversationSnapshot,
    prompt: String,
    provider_factory: F,
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    provider_label: &'static str,
) -> ProviderDeltaStream
where
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse> + Send + Sync + 'static,
    F: FnOnce(ProviderLiveDeltaSender) -> A + Send + 'static,
{
    Box::pin(stream! {
        let (live_sender, mut live_receiver) =
            unbounded_channel::<Result<ProviderStreamDelta, String>>();
        let task_sender = live_sender.clone();
        let mut controller = VulcanCoreController::new(controller_config, core_snapshot);
        let provider_adapter = provider_factory(live_sender.clone());
        let tool_executor =
            StreamingRuntimeToolExecutor::new(runtime_bridge, live_sender.clone());
        let controller_prompt = prompt.clone();
        let controller_task = tokio::spawn(async move {
            let tool_policy = AllowAllToolPolicy;
            match controller
                .run_user_turn_streaming(
                    &provider_adapter,
                    &tool_executor,
                    &tool_policy,
                    controller_prompt,
                )
                .await
            {
                Ok(_) => {
                    let _ = task_sender.send(Ok(ProviderStreamDelta::FinalConversationState(
                        core_snapshot_to_session_conversation_state(controller.snapshot()),
                    )));
                }
                Err(error) => {
                    let _ = task_sender.send(Err(format!(
                        "{}流式执行失败：{}。",
                        provider_label,
                        error
                    )));
                }
            }
        });
        drop(live_sender);

        while let Some(item) = live_receiver.recv().await {
            yield item;
        }

        if let Err(error) = controller_task.await {
            yield Err(format!(
                "{}控制器后台任务失败：{}。",
                provider_label,
                error
            ));
        }
    })
}
