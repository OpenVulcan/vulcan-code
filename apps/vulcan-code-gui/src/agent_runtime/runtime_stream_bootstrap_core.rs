//! Runtime stream bootstrap core-state construction and request-snapshot seeding helpers.
//! 面向运行时流 bootstrap 核心状态构建与请求快照播种的辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPreparationRequest};
use crate::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot;
use crate::agent_runtime::provider_request_controller_config::build_core_controller_config;
use crate::agent_runtime::provider_request_preview::build_provider_request_preview_markdown;
use crate::agent_runtime::provider_request_snapshot_builder::build_core_provider_request_snapshot;
use crate::agent_runtime::provider_runtime_context::{
    build_runtime_system_preamble, resolve_runtime_max_turns,
};
use crate::agent_runtime::provider_runtime_inputs::resolve_provider_additional_params;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage,
    ConversationSnapshot as CoreConversationSnapshot,
};
use vulcan_tools_manager::ResolvedToolDefinition;

/// Core provider-facing runtime bootstrap state derived from canonical conversation inputs.
/// 从规范会话输入推导得到的面向 Provider 的运行时 bootstrap 核心状态。
pub(crate) struct RuntimeBootstrapCore {
    /// Effective system preamble sent to the provider transport layer.
    /// 发送给 Provider 传输层的生效系统前导语。
    pub system_preamble: String,
    /// Provider-visible history reconstructed from canonical conversation messages.
    /// 从规范会话消息重建得到的 Provider 可见历史记录。
    pub request_history: Vec<CoreConversationMessage>,
    /// Canonical conversation snapshot handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的规范会话快照。
    pub core_snapshot: CoreConversationSnapshot,
    /// Host-owned controller that tracks streaming deltas and execution events.
    /// 负责跟踪流式增量与执行事件的宿主自有控制器。
    pub core_controller: VulcanCoreController,
}

/// Builds the provider-facing runtime bootstrap core from the canonical conversation state and execution preparation.
/// 根据规范会话状态与执行准备构建面向 Provider 的运行时 bootstrap 核心对象。
pub(crate) fn build_runtime_bootstrap_core(
    preparation: &ExecutionPreparation,
    request: &ExecutionPreparationRequest,
    active_agent: Option<&RuntimeAgentDefinition>,
) -> RuntimeBootstrapCore {
    let system_preamble = build_runtime_system_preamble(preparation);
    let core_snapshot = build_core_conversation_snapshot(&request.conversation_state);
    let request_history = core_snapshot.messages.clone();
    let core_controller = VulcanCoreController::new(
        build_core_controller_config(
            preparation,
            &system_preamble,
            resolve_runtime_max_turns(active_agent),
            resolve_provider_additional_params(&preparation.model_blueprint),
        ),
        core_snapshot.clone(),
    );

    RuntimeBootstrapCore {
        system_preamble,
        request_history,
        core_snapshot,
        core_controller,
    }
}

/// Seeds the final request preview and structured request snapshot onto the mutable execution preparation object.
/// 将最终请求预览与结构化请求快照播种到可变执行准备对象上。
pub(crate) async fn seed_runtime_request_snapshot(
    preparation: &mut ExecutionPreparation,
    request: &ExecutionPreparationRequest,
    system_preamble: &str,
    resolved_tool_definitions: &[ResolvedToolDefinition],
    runtime_max_turns: Option<usize>,
    warnings: &mut Vec<String>,
) {
    match build_core_provider_request_snapshot(
        preparation,
        &request.conversation_state,
        system_preamble,
        request.prompt.trim(),
        resolved_tool_definitions,
        runtime_max_turns,
    )
    .await
    {
        Ok(request_snapshot) => {
            preparation.provider_request_preview_markdown =
                build_provider_request_preview_markdown(preparation, &request_snapshot);
            preparation.provider_request_snapshot_json =
                serde_json::to_value(&request_snapshot).ok();
        }
        Err(error) => {
            preparation.provider_request_preview_markdown =
                format!("# 最后一次真实 API 请求快照\n\n请求快照构建失败：{}", error);
            preparation.provider_request_snapshot_json = None;
            warnings.push(format!("请求快照构建失败：{error}"));
        }
    }
}
