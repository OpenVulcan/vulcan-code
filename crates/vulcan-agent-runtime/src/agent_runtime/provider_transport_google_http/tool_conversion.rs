//! Tool-call and function-declaration conversion for Google Gemini.
//! Google Gemini 的工具调用与函数声明转换逻辑。

use crate::agent_runtime::provider_protocol_gemini::{
    GeminiFunctionCall, GeminiFunctionDeclaration,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use vulcan_core::types::ConversationToolCall as CoreConversationToolCall;
use vulcan_tools_manager::{VulcanToolSchemaProjectionProfile, project_tool_parameters_schema};

/// Converts one canonical core tool call into one Gemini-native function-call payload while preserving the provider-visible call identifier when available.
/// 将一条规范 core 工具调用转换为一条 Gemini 原生函数调用载荷，并在可用时保留 Provider 可见调用标识。
pub(super) fn core_tool_call_to_gemini_function_call(
    tool_call: CoreConversationToolCall,
) -> GeminiFunctionCall {
    GeminiFunctionCall {
        id: tool_call
            .tool_call_id
            .clone()
            .filter(|value| !value.trim().is_empty()),
        name: tool_call.tool_name,
        args: tool_call.arguments,
    }
}

/// Converts one host-owned provider tool definition into one Gemini-native function declaration.
/// 将一条宿主自有 Provider 工具定义转换为一条 Gemini 原生函数声明。
pub(super) fn provider_tool_definition_to_gemini_function_declaration(
    definition: &ProviderToolDefinition,
) -> GeminiFunctionDeclaration {
    let parameters = project_tool_parameters_schema(
        &definition.parameters,
        VulcanToolSchemaProjectionProfile::GeminiSlim,
    );

    GeminiFunctionDeclaration {
        name: definition.name.clone(),
        description: definition.description.clone(),
        parameters: Some(parameters),
    }
}

pub(super) fn gemini_function_call_key(function_call: &GeminiFunctionCall) -> String {
    function_call
        .id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "{}:{}",
                function_call.name,
                serde_json::to_string(&function_call.args)
                    .unwrap_or_else(|_| function_call.args.to_string())
            )
        })
}

/// Converts one Gemini-native function call into one canonical core tool call.
/// 将一条 Gemini 原生函数调用转换为一条规范 core 工具调用。
pub(super) fn gemini_function_call_to_core_tool_call(
    function_call: &GeminiFunctionCall,
) -> CoreConversationToolCall {
    let local_id = function_call
        .id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| gemini_function_call_key(function_call));

    CoreConversationToolCall {
        id: local_id,
        tool_call_id: function_call
            .id
            .clone()
            .filter(|value| !value.trim().is_empty()),
        tool_name: function_call.name.clone(),
        arguments: function_call.args.clone(),
    }
}
