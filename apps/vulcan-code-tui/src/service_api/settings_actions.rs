// Mirror of `service/settings-actions-service.ts`.
// `service/settings-actions-service.ts` 的镜像。

use serde_json::{Value, json};
use vulcan_service_api::SystemPromptScope;

use super::{ServiceCall, Slot};

const PUBLIC_AI: &[Slot] = &[Slot::PublicAiSettings];
const WORKBENCH: &[Slot] = &[Slot::Workbench];

/// Builds the request for `service_output_style_catalog`.
/// 构建 `service_output_style_catalog` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_output_style_catalog() -> ServiceCall {
    ServiceCall::load(
        "load_output_style_catalog",
        "service_output_style_catalog",
        Value::Null,
        Slot::OutputStyleCatalog,
    )
}

/// Builds the request for `service_save_prompt_output_style`.
/// 构建 `service_save_prompt_output_style` 的请求。
/// `style_id` is the catalog-owned output-style ID.
/// `style_id` 为目录持有的输出风格标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_prompt_output_style(style_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "save_prompt_output_style",
        "service_save_prompt_output_style",
        json!({ "request": { "styleId": style_id } }),
        &[Slot::OutputStyleCatalog],
    )
}

/// Builds the request for `service_system_prompt_settings`.
/// 构建 `service_system_prompt_settings` 的请求。
/// `project_id` is the exact project whose layer is included, or `None` for the user layer only.
/// `project_id` 为需包含其层级的准确项目；`None` 表示仅用户层级。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_system_prompt_settings(project_id: Option<&str>) -> ServiceCall {
    ServiceCall::load(
        "load_system_prompt_settings",
        "service_system_prompt_settings",
        json!({ "projectId": project_id }),
        Slot::SystemPrompt,
    )
}

/// Builds the request for `service_save_system_prompt`.
/// 构建 `service_save_system_prompt` 的请求。
/// `scope` is the target override layer.
/// `scope` 为目标覆盖层级。
/// `project_id` is the exact project identity; the service requires it for the project scope.
/// `project_id` 为准确项目标识；项目作用域时服务要求必填。
/// `text` is the complete prompt body; the service rejects blank text.
/// `text` 为完整提示词正文；服务拒绝空白正文。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_system_prompt(
    scope: SystemPromptScope,
    project_id: Option<&str>,
    text: &str,
) -> ServiceCall {
    ServiceCall::mutate(
        "save_system_prompt",
        "service_save_system_prompt",
        json!({ "request": { "scope": scope, "projectId": project_id, "text": text } }),
        &[Slot::SystemPrompt],
    )
}

/// Builds the request for `service_reset_system_prompt`.
/// 构建 `service_reset_system_prompt` 的请求。
/// `scope` is the target override layer.
/// `scope` 为目标覆盖层级。
/// `project_id` is the exact project identity; the service requires it for the project scope.
/// `project_id` 为准确项目标识；项目作用域时服务要求必填。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn reset_system_prompt(scope: SystemPromptScope, project_id: Option<&str>) -> ServiceCall {
    ServiceCall::mutate(
        "reset_system_prompt",
        "service_reset_system_prompt",
        json!({ "request": { "scope": scope, "projectId": project_id } }),
        &[Slot::SystemPrompt],
    )
}

/// Builds the request for `service_public_ai_settings_snapshot`.
/// 构建 `service_public_ai_settings_snapshot` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_public_ai_settings() -> ServiceCall {
    ServiceCall::load(
        "load_public_ai_settings",
        "service_public_ai_settings_snapshot",
        Value::Null,
        Slot::PublicAiSettings,
    )
}

/// `request` is a `PublicAiConnectionAddRequest`; the secret travels once and is never kept.
/// `request` 为 `PublicAiConnectionAddRequest`；密钥只传输一次，从不保留。
/// Builds the request for `service_add_public_ai_connection`.
/// 构建 `service_add_public_ai_connection` 的请求。
/// `request` is the complete request document declared by the matching WebUI API.
/// `request` 为对应 WebUI API 声明的完整请求文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn add_public_ai_connection(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "add_public_ai_connection",
        "service_add_public_ai_connection",
        json!({ "request": request }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_connection_enabled`.
/// 构建 `service_save_public_ai_connection_enabled` 的请求。
/// `connection_id` is the exact public AI connection ID.
/// `connection_id` 为准确公开 AI 连接标识。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_connection_enabled(connection_id: &str, enabled: bool) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_connection_enabled",
        "service_save_public_ai_connection_enabled",
        json!({ "request": { "connectionId": connection_id, "enabled": enabled } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_remove_public_ai_connection`.
/// 构建 `service_remove_public_ai_connection` 的请求。
/// `connection_id` is the exact public AI connection ID.
/// `connection_id` 为准确公开 AI 连接标识。
/// `replacement_default_connection_id` is the same-provider replacement default, when removal requires one.
/// `replacement_default_connection_id` 为移除时需要的同提供方替代默认连接。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn remove_public_ai_connection(
    connection_id: &str,
    replacement_default_connection_id: Option<&str>,
) -> ServiceCall {
    // Exact protocol request.
    // 协议请求文档。
    let mut request = json!({ "connectionId": connection_id });
    if let Some(replacement) = replacement_default_connection_id {
        request["replacementDefaultConnectionId"] = json!(replacement);
    }
    ServiceCall::mutate(
        "remove_public_ai_connection",
        "service_remove_public_ai_connection",
        json!({ "request": request }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_sync_public_ai_connection`.
/// 构建 `service_sync_public_ai_connection` 的请求。
/// `connection_id` is the exact public AI connection ID.
/// `connection_id` 为准确公开 AI 连接标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn sync_public_ai_connection(connection_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "sync_public_ai_connection",
        "service_sync_public_ai_connection",
        json!({ "connectionId": connection_id }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_default_connection`.
/// 构建 `service_save_public_ai_default_connection` 的请求。
/// `provider_id` is the provider ID from the public AI catalog.
/// `provider_id` 为公开 AI 目录中的提供方标识。
/// `connection_id` is the exact public AI connection ID.
/// `connection_id` 为准确公开 AI 连接标识。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_default_connection(provider_id: &str, connection_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_default_connection",
        "service_save_public_ai_default_connection",
        json!({ "request": { "providerId": provider_id, "connectionId": connection_id } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_usage_refresh_settings`.
/// 构建 `service_save_public_ai_usage_refresh_settings` 的请求。
/// `interval_seconds` is the refresh interval in seconds.
/// `interval_seconds` 为以秒为单位的刷新间隔。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_usage_refresh_settings(interval_seconds: i64) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_usage_refresh_settings",
        "service_save_public_ai_usage_refresh_settings",
        json!({ "request": { "intervalSeconds": interval_seconds } }),
        PUBLIC_AI,
    )
}

/// `nodes` is the whole `PublicAiContextCompactionWindowConfiguration`.
/// `nodes` 为完整的 `PublicAiContextCompactionWindowConfiguration`。
/// Builds the request for `service_save_public_ai_context_compaction_window_nodes`.
/// 构建 `service_save_public_ai_context_compaction_window_nodes` 的请求。
/// `nodes` is the complete context-window configuration, including all five nodes.
/// `nodes` 为包含全部五档节点的完整上下文窗口配置。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_context_compaction_window_nodes(nodes: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_context_compaction_window_nodes",
        "service_save_public_ai_context_compaction_window_nodes",
        json!({ "request": { "nodes": nodes } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_new_session_context_window_default`.
/// 构建 `service_save_public_ai_new_session_context_window_default` 的请求。
/// `node_id` is the exact context-window node ID; None selects service inheritance.
/// `node_id` 为准确上下文窗口节点标识；None 选择服务继承。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_new_session_context_window_default(node_id: Option<&str>) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_new_session_context_window_default",
        "service_save_public_ai_new_session_context_window_default",
        json!({ "request": { "nodeId": node_id } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_model_enabled`.
/// 构建 `service_save_public_ai_model_enabled` 的请求。
/// `provider_id` is the provider ID from the public AI catalog.
/// `provider_id` 为公开 AI 目录中的提供方标识。
/// `model_id` is the provider-owned model ID.
/// `model_id` 为提供方持有的模型标识。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_model_enabled(
    provider_id: &str,
    model_id: &str,
    enabled: bool,
) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_model_enabled",
        "service_save_public_ai_model_enabled",
        json!({ "request": { "providerId": provider_id, "modelId": model_id, "enabled": enabled } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_internal_model_enabled`, the model-level
/// switch shared by every non-conversation internal operation.
/// 构建 `service_save_public_ai_internal_model_enabled` 的请求，即所有非会话内部操作共用的模型级开关。
/// `provider_id` and `model_id` name the exact provider model; `enabled` is the requested state.
/// `provider_id` 与 `model_id` 指明准确的提供方模型；`enabled` 为请求的状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_internal_model_enabled(
    provider_id: &str,
    model_id: &str,
    enabled: bool,
) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_internal_model_enabled",
        "service_save_public_ai_internal_model_enabled",
        json!({ "request": { "providerId": provider_id, "modelId": model_id, "enabled": enabled } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_multimodal_understanding_candidate`, the shared
/// provider-model admission switch independent from conversation use.
/// 构建 `service_save_public_ai_multimodal_understanding_candidate` 的请求，即独立于会话使用的共享
/// 提供方模型准入开关。
/// `provider_id` and `model_id` name the exact provider model; `enabled` is the requested state.
/// `provider_id` 与 `model_id` 指明准确的提供方模型；`enabled` 为请求的状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_multimodal_understanding_candidate(
    provider_id: &str,
    model_id: &str,
    enabled: bool,
) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_multimodal_understanding_candidate",
        "service_save_public_ai_multimodal_understanding_candidate",
        json!({ "request": { "providerId": provider_id, "modelId": model_id, "enabled": enabled } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_provider_extension_target_set`, which replaces
/// the complete native extension-model set of one capability.
/// 构建 `service_save_public_ai_provider_extension_target_set` 的请求，替换一项能力的完整原生扩展模型集合。
/// `capability` is the extension capability; `selections` is the full `{connection_id, target_id}` array.
/// `capability` 为扩展能力；`selections` 为完整的 `{connection_id, target_id}` 数组。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_provider_extension_target_set(
    capability: &str,
    selections: Vec<Value>,
) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_provider_extension_target_set",
        "service_save_public_ai_provider_extension_target_set",
        json!({ "request": { "capability": capability, "selections": selections } }),
        PUBLIC_AI,
    )
}

/// `route` is a `PublicAiInferenceRoute` (`null` clears the route).
/// `route` 为 `PublicAiInferenceRoute`（`null` 表示清除路由）。
/// Builds the request for `service_save_public_ai_inference_route`.
/// 构建 `service_save_public_ai_inference_route` 的请求。
/// `consumer_profile_id` is the host background consumer profile ID.
/// `consumer_profile_id` 为宿主后台消费者配置标识。
/// `purpose_id` is the declared LLM purpose, absent for other operation families.
/// `purpose_id` 为已声明的 LLM 用途；其他操作类别中为空。
/// `operation` is the exact declared inference operation.
/// `operation` 为准确声明的推理操作。
/// `route` is the catalog route object; Null explicitly removes the route.
/// `route` 为目录路由对象；Null 明确移除路由。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_inference_route(
    consumer_profile_id: &str,
    purpose_id: Option<&str>,
    operation: &str,
    route: Value,
) -> ServiceCall {
    // Exact protocol request.
    // 协议请求文档。
    let mut request = json!({
        "consumerProfileId": consumer_profile_id,
        "operation": operation,
        "route": route,
    });
    if let Some(purpose) = purpose_id {
        request["purposeId"] = json!(purpose);
    }
    ServiceCall::mutate(
        "save_public_ai_inference_route",
        "service_save_public_ai_inference_route",
        json!({ "request": request }),
        PUBLIC_AI,
    )
}

/// Runs a real request against the saved route; the result arrives as a notice.
/// 对已保存路由发起真实请求；结果以提示形式返回。
/// Builds the request for `service_test_public_ai_inference_route`.
/// 构建 `service_test_public_ai_inference_route` 的请求。
/// `consumer_profile_id` is the host background consumer profile ID.
/// `consumer_profile_id` 为宿主后台消费者配置标识。
/// `operation` is the exact declared inference operation.
/// `operation` 为准确声明的推理操作。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn test_public_ai_inference_route(consumer_profile_id: &str, operation: &str) -> ServiceCall {
    ServiceCall::mutate(
        "test_public_ai_inference_route",
        "service_test_public_ai_inference_route",
        json!({ "request": { "consumerProfileId": consumer_profile_id, "operation": operation } }),
        &[],
    )
}

/// `selection` is `{connection_id, target_id}` or `null`.
/// `selection` 为 `{connection_id, target_id}` 或 `null`。
/// Builds the request for `service_save_public_ai_provider_tool_selection`.
/// 构建 `service_save_public_ai_provider_tool_selection` 的请求。
/// `capability` is the catalog capability name.
/// `capability` 为目录中的能力名称。
/// `selection` is the capability target selected from the published catalog.
/// `selection` 为从已发布目录选中的能力目标。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_provider_tool_selection(capability: &str, selection: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_provider_tool_selection",
        "service_save_public_ai_provider_tool_selection",
        json!({ "request": { "capability": capability, "selection": selection } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_save_public_ai_provider_tool_capability_enabled`.
/// 构建 `service_save_public_ai_provider_tool_capability_enabled` 的请求。
/// `capability` is the catalog capability name.
/// `capability` 为目录中的能力名称。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_public_ai_provider_tool_capability_enabled(
    capability: &str,
    enabled: bool,
) -> ServiceCall {
    ServiceCall::mutate(
        "save_public_ai_provider_tool_capability_enabled",
        "service_save_public_ai_provider_tool_capability_enabled",
        json!({ "request": { "capability": capability, "enabled": enabled } }),
        PUBLIC_AI,
    )
}

/// Builds the request for `service_select_workbench_thinking_display_mode`.
/// 构建 `service_select_workbench_thinking_display_mode` 的请求。
/// `mode` is the declared display or maintenance mode for this operation.
/// `mode` 为本操作声明的显示或维护模式。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn select_workbench_thinking_display_mode(mode: &str) -> ServiceCall {
    ServiceCall::mutate(
        "select_workbench_thinking_display_mode",
        "service_select_workbench_thinking_display_mode",
        json!({ "mode": mode }),
        WORKBENCH,
    )
}

/// Builds the request for `service_save_tool_collapse_default`.
/// 构建 `service_save_tool_collapse_default` 的请求。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_tool_collapse_default(enabled: bool) -> ServiceCall {
    ServiceCall::mutate(
        "save_tool_collapse_default",
        "service_save_tool_collapse_default",
        json!({ "enabled": enabled }),
        WORKBENCH,
    )
}

/// Builds the request for `service_select_webui_locale`.
/// 构建 `service_select_webui_locale` 的请求。
/// `locale` is the supported locale tag.
/// `locale` 为支持的语言标签。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn select_webui_locale(locale: &str) -> ServiceCall {
    ServiceCall::mutate(
        "select_webui_locale",
        "service_select_webui_locale",
        json!({ "locale": locale }),
        WORKBENCH,
    )
}

/// `request` is the whole git commit settings DTO (`newBranchPrefix`, `summaryModel`,
/// `summaryExtraPrompt`).
/// `request` 为完整的 Git 提交设置 DTO。
/// Builds the request for `service_save_git_commit_settings`.
/// 构建 `service_save_git_commit_settings` 的请求。
/// `request` is the complete request document declared by the matching WebUI API.
/// `request` 为对应 WebUI API 声明的完整请求文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_git_commit_settings(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_git_commit_settings",
        "service_save_git_commit_settings",
        json!({ "request": request }),
        WORKBENCH,
    )
}

/// `request` is the whole Provider retry DTO, including `rateLimit`.
/// `request` 为完整的 Provider 重试 DTO，包括 `rateLimit`。
/// Builds the request for `service_save_provider_retry_settings`.
/// 构建 `service_save_provider_retry_settings` 的请求。
/// `request` is the complete request document declared by the matching WebUI API.
/// `request` 为对应 WebUI API 声明的完整请求文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_provider_retry_settings(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_provider_retry_settings",
        "service_save_provider_retry_settings",
        json!({ "request": request }),
        WORKBENCH,
    )
}

/// Builds the request for `service_save_provider_request_trace_settings`.
/// 构建 `service_save_provider_request_trace_settings` 的请求。
/// `enabled` is the explicit requested enabled state.
/// `enabled` 为明确请求的启用状态。
/// `errors_enabled` changes independent session-error recording when present.
/// 传入 `errors_enabled` 时更改独立会话错误记录。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_provider_request_trace_settings(
    enabled: bool,
    errors_enabled: Option<bool>,
) -> ServiceCall {
    ServiceCall::mutate(
        "save_provider_request_trace_settings",
        "service_save_provider_request_trace_settings",
        json!({ "request": { "enabled": enabled, "errorsEnabled": errors_enabled } }),
        WORKBENCH,
    )
}

/// Builds the request for `service_open_selected_provider_api_trace_directory`.
/// 构建 `service_open_selected_provider_api_trace_directory` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn open_selected_provider_api_trace_directory() -> ServiceCall {
    ServiceCall::mutate(
        "open_selected_provider_api_trace_directory",
        "service_open_selected_provider_api_trace_directory",
        Value::Null,
        &[],
    )
}

/// Builds the request for `service_clear_selected_provider_api_trace_directory`.
/// 构建 `service_clear_selected_provider_api_trace_directory` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn clear_selected_provider_api_trace_directory() -> ServiceCall {
    ServiceCall::mutate(
        "clear_selected_provider_api_trace_directory",
        "service_clear_selected_provider_api_trace_directory",
        Value::Null,
        WORKBENCH,
    )
}

/// `request` is the full MCP server configuration as the WebUI editor saves it.
/// `request` 为 WebUI 编辑器保存的完整 MCP 服务器配置。
/// Builds the request for `service_save_mcp_server_configuration`.
/// 构建 `service_save_mcp_server_configuration` 的请求。
/// `request` is the complete request document declared by the matching WebUI API.
/// `request` 为对应 WebUI API 声明的完整请求文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_mcp_server_configuration(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_mcp_server_configuration",
        "service_save_mcp_server_configuration",
        json!({ "request": request }),
        &[Slot::Workbench, Slot::McpRuntime],
    )
}

/// Builds the request for `service_remove_mcp_server_configuration`.
/// 构建 `service_remove_mcp_server_configuration` 的请求。
/// `server_id` is the configured MCP server identity.
/// `server_id` 为已配置的 MCP 服务器身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn remove_mcp_server_configuration(server_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "remove_mcp_server_configuration",
        "service_remove_mcp_server_configuration",
        json!({ "serverId": server_id }),
        &[Slot::Workbench, Slot::McpRuntime],
    )
}

/// Builds the request for `service_list_provider_tasks`.
/// 构建 `service_list_provider_tasks` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn list_provider_tasks() -> ServiceCall {
    ServiceCall::load(
        "list_provider_tasks",
        "service_list_provider_tasks",
        json!({}),
        Slot::ProviderTasks,
    )
}

/// Builds the request for `service_cancel_provider_task`.
/// 构建 `service_cancel_provider_task` 的请求。
/// `execution_id` is the stable task identity shown in the list.
/// `execution_id` 为列表中显示的稳定任务标识。
/// `owner_session_id` repeats the listed owner so a stale view cannot cancel a task that changed hands.
/// `owner_session_id` 重复列表中的所有者，使过期视图无法取消已易主的任务。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn cancel_provider_task(execution_id: &str, owner_session_id: Option<&str>) -> ServiceCall {
    ServiceCall::mutate(
        "cancel_provider_task",
        "service_cancel_provider_task",
        json!({ "executionId": execution_id, "ownerSessionId": owner_session_id }),
        &[Slot::ProviderTasks],
    )
}
