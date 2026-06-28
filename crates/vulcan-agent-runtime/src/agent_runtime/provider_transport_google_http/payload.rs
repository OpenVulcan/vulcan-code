//! Request payload construction for Google Gemini.
//! Google Gemini 请求载荷构建逻辑。

use super::messages::core_message_to_gemini_content;
use super::tool_conversion::provider_tool_definition_to_gemini_function_declaration;
use super::util::merge_json_values;
use crate::agent_runtime::provider_prompt_layers::provider_fallback_system_prompt;
use crate::agent_runtime::provider_protocol_gemini::{
    GeminiFunctionCallingConfig, GeminiTool, GeminiToolConfig,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use serde_json::{Value, json};
use vulcan_core::types::CoreError as VulcanCoreError;

/// Builds one Google Gemini native request payload from the canonical host-owned turn request and mirrored tool definitions.
/// 根据规范宿主单回合请求与镜像工具定义构建一份 Google Gemini 原生请求载荷。
pub(super) fn build_google_gemini_request_payload(
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Result<Value, VulcanCoreError> {
    let OpenAiCompatibleCoreTurnRequest {
        snapshot,
        current_history,
        current_prompt,
        ..
    } = request;

    let system_prompt = provider_fallback_system_prompt(&snapshot);
    let mut payload = match snapshot.additional_params {
        Some(Value::Object(object)) => Value::Object(object),
        Some(other) => json!({ "hostAdditionalParams": other }),
        None => json!({}),
    };

    let mut all_messages = current_history;
    all_messages.push(current_prompt);

    let mut contents = Vec::new();
    for index in 0..all_messages.len() {
        let content = core_message_to_gemini_content(
            &all_messages[index],
            &all_messages[..index],
            snapshot.provider_id.as_str(),
            snapshot.model_id.as_str(),
        )?;
        if !content.parts.is_empty() {
            contents.push(serde_json::to_value(content).map_err(|error| {
                VulcanCoreError::InvalidState {
                    message: format!("Google Gemini 历史消息序列化失败：{error}"),
                }
            })?);
        }
    }

    payload = merge_json_values(payload, json!({ "contents": contents }));

    if !system_prompt.trim().is_empty() {
        payload = merge_json_values(
            payload,
            json!({
                "systemInstruction": {
                    "parts": [
                        {
                            "text": system_prompt
                        }
                    ]
                }
            }),
        );
    }

    if let Some(temperature) = snapshot.temperature {
        payload = merge_json_values(
            payload,
            json!({
                "generationConfig": {
                    "temperature": temperature
                }
            }),
        );
    }

    remove_gemini_builtin_search_tools(&mut payload);
    append_gemini_function_tools(&mut payload, provider_tool_definitions)?;

    Ok(payload)
}

/// Removes Google Gemini built-in search tools from the request payload so coding function tools stay usable.
/// 从请求载荷中移除 Google Gemini 内置搜索工具，确保编码函数工具保持可用。
fn remove_gemini_builtin_search_tools(payload: &mut Value) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    let Some(tools) = object.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };

    tools.retain(|tool| !gemini_tool_is_builtin_search(tool));
    if tools.is_empty() {
        object.remove("tools");
    }
}

/// Returns whether one Gemini tool entry is a provider-native search tool that conflicts with function calling.
/// 返回单条 Gemini 工具是否是会与函数调用冲突的 Provider 原生搜索工具。
fn gemini_tool_is_builtin_search(tool: &Value) -> bool {
    tool.as_object()
        .map(|object| object.contains_key("google_search") || object.contains_key("googleSearch"))
        .unwrap_or(false)
}

/// Appends Gemini function declarations after provider-native search tools have been removed from the payload.
/// 在移除 Provider 原生搜索工具后追加 Gemini 函数声明。
fn append_gemini_function_tools(
    payload: &mut Value,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> Result<(), VulcanCoreError> {
    if provider_tool_definitions.is_empty() {
        return Ok(());
    }

    let Value::Object(object) = payload else {
        return Err(VulcanCoreError::InvalidState {
            message: String::from("Google Gemini 请求载荷不是对象，无法追加工具声明。"),
        });
    };
    let function_tool = serde_json::to_value(GeminiTool {
        function_declarations: provider_tool_definitions
            .iter()
            .map(provider_tool_definition_to_gemini_function_declaration)
            .collect::<Vec<_>>(),
    })
    .map_err(|error| VulcanCoreError::InvalidState {
        message: format!("Google Gemini 工具声明序列化失败：{error}"),
    })?;

    let tools = object
        .entry(String::from("tools"))
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(tools) = tools.as_array_mut() else {
        return Err(VulcanCoreError::InvalidState {
            message: String::from("Google Gemini 请求载荷中的 tools 字段不是数组。"),
        });
    };
    tools.push(function_tool);

    object.entry(String::from("toolConfig")).or_insert_with(|| {
        json!(GeminiToolConfig {
            function_calling_config: GeminiFunctionCallingConfig {
                mode: String::from("AUTO"),
            }
        })
    });

    Ok(())
}
