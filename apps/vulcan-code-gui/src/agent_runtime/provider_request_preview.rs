//! Provider request preview Markdown renderer used by the execution runtime.
//! 供执行运行时使用的 Provider 请求预览 Markdown 渲染模块。

use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_runtime_dispatch::DEFAULT_RUNTIME_MAX_TURNS_LIMIT;
use serde_json::{Value, json};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ProviderContinuation as CoreProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema,
};

/// Builds the full Markdown preview of the effective provider request captured right before dispatch.
/// 构建在真正分发前捕获的生效 Provider 请求完整 Markdown 预览。
pub(crate) fn build_provider_request_preview_markdown(
    preparation: &ExecutionPreparation,
    request_snapshot: &CoreProviderRequestSnapshot,
) -> String {
    let temperature_label = request_snapshot
        .temperature
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| String::from("使用模型默认值"));
    let max_turns_label = if request_snapshot.max_turns == DEFAULT_RUNTIME_MAX_TURNS_LIMIT {
        String::from("不限制")
    } else {
        format!("`{}`", request_snapshot.max_turns)
    };
    let extra_body_markdown = request_snapshot
        .additional_params
        .as_ref()
        .map(render_json_code_fence)
        .unwrap_or_else(|| String::from("（无附加 Body 参数）"));
    let request_headers_markdown = request_snapshot
        .request_headers
        .as_ref()
        .map(render_json_code_fence)
        .unwrap_or_else(|| String::from("（无附加请求头）"));
    let continuation_markdown =
        render_provider_continuation_markdown(&request_snapshot.continuation);
    let context_budget_label = preparation
        .context_budget
        .as_ref()
        .map(|budget| budget.summary_note())
        .unwrap_or_else(|| String::from("尚未生成上下文预算快照"));
    let tool_rows = if request_snapshot.tool_schemas.is_empty() {
        String::from("（本次请求未直接暴露工具 schema）")
    } else {
        request_snapshot
            .tool_schemas
            .iter()
            .map(render_core_tool_request_section)
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let outgoing_history_markdown =
        render_core_conversation_history_markdown(&request_snapshot.history);

    format!(
        concat!(
            "# 最后一次真实 API 请求快照\n\n",
            "## 路由摘要\n",
            "- 会话：`#{}` · {}\n",
            "- 代理：`{}` · {} · id=`{}`\n",
            "- Provider：`{}` · `{}`\n",
            "- 模型：`{}` · `{}`\n",
            "- 运行时：`{}` · kind=`{}`\n",
            "- 接口族：`{}` · family=`{}`\n",
            "- 接口：`{}`\n",
            "- 执行模式：`{}`\n",
            "- 温度：{}\n",
            "- 最大轮次：{}\n",
            "- 历史上下文：`{}` 条\n",
            "- 工具数量：`{}` 个\n\n",
            "## Context Budget\n",
            "{}\n\n",
            "## System Preamble\n",
            "{}\n\n",
            "## Continuation\n",
            "{}\n\n",
            "## Outgoing History\n",
            "{}\n\n",
            "## Effective Outgoing Prompt\n",
            "{}\n\n",
            "## Tool Schemas\n",
            "{}\n\n",
            "## Request Headers\n",
            "{}\n\n",
            "## Additional Body Parameters\n",
            "{}\n"
        ),
        preparation.session_id,
        preparation.session_title,
        preparation.agent_name,
        preparation.agent_kind,
        preparation.agent_id,
        preparation.model_blueprint.provider_name,
        preparation.model_blueprint.provider_id,
        preparation.model_blueprint.model_name,
        preparation.model_blueprint.model_id,
        preparation.model_blueprint.runtime_label,
        preparation.model_blueprint.runtime_kind_key(),
        preparation.model_blueprint.api_family_label(),
        preparation.model_blueprint.api_family_key(),
        preparation.model_blueprint.endpoint_summary(),
        preparation.execution_mode,
        temperature_label,
        max_turns_label,
        request_snapshot.history.len(),
        request_snapshot.tool_schemas.len(),
        context_budget_label,
        render_text_code_fence(&request_snapshot.system_prompt),
        continuation_markdown,
        outgoing_history_markdown,
        render_text_code_fence(&request_snapshot.prompt),
        tool_rows,
        request_headers_markdown,
        extra_body_markdown,
    )
}

/// Renders the `vulcan-core` canonical history into a structured Markdown snapshot that is easier to inspect than one flattened prompt string.
/// 将 `vulcan-core` 规范历史渲染为结构化 Markdown 快照，使其比单条扁平 Prompt 字符串更易检查。
fn render_core_conversation_history_markdown(history: &[CoreConversationMessage]) -> String {
    if history.is_empty() {
        return String::from("（本次请求没有历史消息）");
    }

    history
        .iter()
        .enumerate()
        .map(|(index, message)| render_core_conversation_message_preview(index, message))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Renders one `vulcan-core` canonical conversation message into the provider-preview Markdown.
/// 将单条 `vulcan-core` 规范会话消息渲染到 Provider 预览 Markdown 中。
fn render_core_conversation_message_preview(
    index: usize,
    message: &CoreConversationMessage,
) -> String {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let mut sections = vec![format!("### {}. user · `{}`", index + 1, message.id)];
            if !text.trim().is_empty() {
                sections.push(render_text_code_fence(text));
            }
            if !attachments.is_empty() {
                sections.push(String::from("#### Attachments"));
                sections.extend(attachments.iter().map(|attachment| {
                    format!(
                        "- `{}` · `{}` · `{}`",
                        attachment.kind.display_label(),
                        attachment.file_name,
                        attachment.mime_type
                    )
                }));
            }
            sections.join("\n\n")
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            tool_calls,
            ..
        } => {
            let mut sections = vec![format!(
                "### {}. assistant · `{}`{}",
                index + 1,
                message.id,
                provider_message_id
                    .as_deref()
                    .map(|id| format!(" · provider_id=`{id}`"))
                    .unwrap_or_default()
            )];
            if !text.trim().is_empty() {
                sections.push(String::from("#### Text"));
                sections.push(render_text_code_fence(text));
            }
            if !reasoning.trim().is_empty() {
                sections.push(String::from("#### Reasoning"));
                sections.push(render_text_code_fence(reasoning));
            }
            if !tool_calls.is_empty() {
                sections.push(String::from("#### Tool Calls"));
                sections.extend(tool_calls.iter().map(|tool_call| {
                    let arguments = serde_json::to_string_pretty(&tool_call.arguments)
                        .unwrap_or_else(|_| tool_call.arguments.to_string());
                    format!(
                        "- `{}` · id=`{}`{}\n```json\n{}\n```",
                        tool_call.tool_name,
                        tool_call.id,
                        tool_call
                            .tool_call_id
                            .as_deref()
                            .map(|call_id| format!(" · call_id=`{call_id}`"))
                            .unwrap_or_default(),
                        arguments
                    )
                }));
            }
            sections.join("\n\n")
        }
        CoreConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => format!(
            "### {}. tool_result · `{}`{}\n{}",
            index + 1,
            tool_result_id,
            tool_call_id
                .as_deref()
                .map(|call_id| format!(" · call_id=`{call_id}`"))
                .unwrap_or_default(),
            render_text_code_fence(text)
        ),
    }
}

/// Renders one `vulcan-core` tool schema into a Markdown section used by the request-preview tab.
/// 将单个 `vulcan-core` 工具 schema 渲染为请求预览页签使用的 Markdown 分节。
fn render_core_tool_request_section(definition: &CoreProviderRequestToolSchema) -> String {
    format!(
        concat!(
            "### `{}`\n",
            "- 显示名：{}\n",
            "- 来源：{}\n",
            "- 描述：{}\n",
            "{}"
        ),
        definition.id,
        definition.name,
        definition.source_label,
        definition.description,
        render_json_code_fence(&definition.parameters_schema),
    )
}

/// Renders one provider continuation snapshot into readable Markdown for request inspection.
/// 将一份 Provider continuation 快照渲染为便于请求检查的可读 Markdown。
fn render_provider_continuation_markdown(continuation: &CoreProviderContinuation) -> String {
    if continuation.provider_id.is_none()
        && continuation.model_id.is_none()
        && continuation.last_provider_message_id.is_none()
        && continuation.updated_at.is_none()
    {
        return String::from("（本次请求没有 continuation 游标）");
    }

    let payload = json!({
        "provider_id": continuation.provider_id,
        "model_id": continuation.model_id,
        "last_provider_message_id": continuation.last_provider_message_id,
        "updated_at": continuation.updated_at,
    });
    render_json_code_fence(&payload)
}

/// Wraps arbitrary text inside a safe fenced code block for request previews.
/// 将任意文本包装进请求预览使用的安全围栏代码块中。
fn render_text_code_fence(content: &str) -> String {
    format!("````text\n{}\n````", content)
}

/// Wraps one JSON value inside a pretty-printed fenced code block for request previews.
/// 将单个 JSON 值包装进请求预览使用的格式化围栏代码块中。
fn render_json_code_fence(value: &Value) -> String {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    format!("````json\n{}\n````", pretty)
}

#[cfg(test)]
mod tests {
    use super::build_provider_request_preview_markdown;
    use crate::agent_runtime::execution_models::ExecutionPreparation;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::provider_config::ProviderRuntimeKind;
    use serde_json::Value;
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use vulcan_core::types::{
        ProviderContinuation as CoreProviderContinuation,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    };

    /// Verifies the provider request preview explicitly renders the canonical API family alongside the runtime kind.
    /// 验证 Provider 请求预览会把规范接口族与运行时类型一起明确渲染出来。
    #[test]
    fn provider_request_preview_renders_api_family_summary_line() {
        let preparation = ExecutionPreparation {
            session_id: Uuid::nil(),
            session_title: String::from("接口族预览测试"),
            agent_id: String::from("main"),
            agent_name: String::from("Main Agent"),
            agent_kind: String::from("analysis"),
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("anthropic"),
                provider_name: String::from("Anthropic"),
                runtime_kind: ProviderRuntimeKind::Anthropic,
                runtime_label: String::from("Anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                model_name: String::from("Claude Sonnet 4.6"),
                base_url: Some(String::from("https://api.anthropic.com")),
                supports_remote_model_refresh: false,
                credential_source: String::from("环境变量 `ANTHROPIC_API_KEY`"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: Value::Object(Default::default()),
                reasoning: Default::default(),
                capabilities: Default::default(),
                interleaved_reasoning: None,
            },
            prompt_preview: String::from("检查请求预览"),
            workspace_path: String::from("D:/openvulcan/vulcan-code"),
            context_history_count: 0,
            execution_mode: String::from("默认模式"),
            temperature: Some(0.2),
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            context_budget: None,
        };
        let snapshot = CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt: String::from("preview prompt"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: CoreProviderContinuation::default(),
        };

        let markdown = build_provider_request_preview_markdown(&preparation, &snapshot);

        assert!(markdown.contains("- 运行时：`Anthropic` · kind=`anthropic`"));
        assert!(markdown.contains("- 接口族：`Anthropic Messages` · family=`anthropic-messages`"));
    }
}
