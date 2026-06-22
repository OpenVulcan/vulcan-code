//! Structured provider request snapshot builders reused by runtime bootstrap and regression tests.
//! 供运行时 bootstrap 与回归测试复用的结构化 Provider 请求快照构建模块。

use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot;
use crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history_for_model_with_system_updates;
use crate::agent_runtime::provider_request_bridge_tools::resolved_tool_definition_to_core_tool_schema;
use crate::agent_runtime::provider_request_controller_config::build_core_controller_config_with_layers;
use crate::agent_runtime::provider_request_snapshot_preview::CoreRequestPreviewProviderAdapter;
use crate::agent_runtime::provider_request_snapshot_support::serialize_provider_request_headers;
use crate::agent_runtime::provider_system_update_lowering::ProviderSystemUpdateLoweringPolicy;
use crate::conversation_protocol::SessionConversationState;
use vulcan_core::types::{
    ProviderPromptLayers as CoreProviderPromptLayers,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};
use vulcan_tools_manager::ResolvedToolDefinition;

/// Builds one `vulcan-core` request snapshot from the current execution preparation and canonical session state.
/// 根据当前执行准备信息与规范会话状态构建一份 `vulcan-core` 请求快照。
pub(crate) async fn build_core_provider_request_snapshot(
    preparation: &ExecutionPreparation,
    conversation_state: &SessionConversationState,
    system_preamble: &str,
    prompt_layers: CoreProviderPromptLayers,
    prompt: &str,
    resolved_tools: &[ResolvedToolDefinition],
    max_turns_override: Option<usize>,
) -> Result<CoreProviderRequestSnapshot, String> {
    let request_headers =
        crate::agent_runtime::provider_runtime_inputs::build_provider_request_headers(
            &preparation.model_blueprint,
        )?;
    let serialized_headers = serialize_provider_request_headers(&request_headers);
    let additional_params =
        crate::agent_runtime::provider_runtime_inputs::resolve_provider_additional_params(
            &preparation.model_blueprint,
        );
    let tool_schemas = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_core_tool_schema)
        .collect::<Vec<_>>();
    let mut core_snapshot = build_core_conversation_snapshot(conversation_state);
    core_snapshot.messages =
        core_messages_to_provider_visible_history_for_model_with_system_updates(
            &core_snapshot.messages,
            Some(preparation.model_blueprint.api_family()),
            Some(&preparation.model_blueprint.capabilities),
            &preparation.runtime_system_updates,
            ProviderSystemUpdateLoweringPolicy::default(),
        )?;
    let mut controller = vulcan_core::controller::VulcanCoreController::new(
        build_core_controller_config_with_layers(
            preparation,
            system_preamble,
            prompt_layers,
            max_turns_override,
            additional_params.clone(),
        ),
        core_snapshot,
    );
    let adapter = CoreRequestPreviewProviderAdapter::new(
        preparation.model_blueprint.provider_id.clone(),
        preparation.model_blueprint.model_id.clone(),
        prompt.to_string(),
        serialized_headers,
        tool_schemas,
    );

    controller
        .build_request(&adapter)
        .await
        .map_err(|error| error.to_string())
}
