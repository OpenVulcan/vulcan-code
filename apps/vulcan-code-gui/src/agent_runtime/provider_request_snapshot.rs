//! Provider request snapshot and preview builders used by the execution runtime.
//! 供执行运行时使用的 Provider 请求快照与预览构建模块。

use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_request_bridge::{
    build_core_conversation_snapshot, resolved_tool_definition_to_core_tool_schema,
    serialize_provider_request_headers,
};
pub(crate) use crate::agent_runtime::provider_request_preview::build_provider_request_preview_markdown;
use crate::agent_runtime::provider_request_snapshot_adapter::PreviewSnapshotProviderAdapter;
use crate::agent_runtime::provider_runtime_dispatch::DEFAULT_RUNTIME_MAX_TURNS_LIMIT;
use crate::conversation_protocol::SessionConversationState;
use serde_json::Value;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};
use vulcan_tools_manager::ResolvedToolDefinition;

/// Builds one `vulcan-core` request snapshot from the current execution preparation and canonical session state.
/// 根据当前执行准备信息与规范会话状态构建一份 `vulcan-core` 请求快照。
pub(crate) async fn build_core_provider_request_snapshot(
    preparation: &ExecutionPreparation,
    conversation_state: &SessionConversationState,
    system_preamble: &str,
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
    let mut controller = vulcan_core::controller::VulcanCoreController::new(
        build_core_controller_config(
            preparation,
            system_preamble,
            max_turns_override,
            additional_params.clone(),
        ),
        build_core_conversation_snapshot(conversation_state),
    );
    let adapter = PreviewSnapshotProviderAdapter::new(
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

/// Builds one `vulcan-core` controller configuration from the current execution preparation.
/// 根据当前执行准备信息构建一份 `vulcan-core` 控制器配置。
pub(crate) fn build_core_controller_config(
    preparation: &ExecutionPreparation,
    system_preamble: &str,
    max_turns_override: Option<usize>,
    additional_params: Option<Value>,
) -> CoreControllerConfig {
    CoreControllerConfig {
        provider_id: preparation.model_blueprint.provider_id.clone(),
        model_id: preparation.model_blueprint.model_id.clone(),
        system_prompt: system_preamble.to_string(),
        temperature: preparation.temperature.map(f64::from),
        max_turns: max_turns_override.unwrap_or(DEFAULT_RUNTIME_MAX_TURNS_LIMIT),
        additional_params,
    }
}
