//! Provider request preview Markdown renderer used by the execution runtime.
//! 供执行运行时使用的 Provider 请求预览 Markdown 渲染模块。

use crate::agent_runtime::ai_tool_router::{AiToolRouteKind, resolve_ai_tool_plan};
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_runtime_dispatch::DEFAULT_RUNTIME_MAX_TURNS_LIMIT;
use serde_json::{Value, json};
use vulcan_context::{ContextAssembly, build_provider_request_context_assembly};
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
    let ai_tool_plan_markdown = render_ai_tool_plan_markdown(preparation);
    let context_budget_label = preparation
        .context_budget
        .as_ref()
        .map(|budget| {
            format!(
                "{}\n- 预算指纹：`{}`",
                budget.summary_note(),
                budget.fingerprint
            )
        })
        .unwrap_or_else(|| String::from("尚未生成上下文预算快照"));
    let context_assembly = preparation
        .context_assembly
        .as_ref()
        .cloned()
        .unwrap_or_else(|| build_provider_request_context_assembly(request_snapshot));
    let context_baseline_markdown = preparation
        .context_baseline
        .as_ref()
        .map(|baseline| {
            let comparison = baseline.compare(&context_assembly);
            format!(
                "- 状态：{}\n- 层数：`{}`\n- 指纹：`{}`",
                comparison.summary_note(),
                baseline.layer_count,
                baseline.fingerprint
            )
        })
        .unwrap_or_else(|| String::from("（尚未生成上下文基线）"));
    let context_request_mode_markdown = preparation
        .context_request_mode
        .as_ref()
        .map(|mode| {
            format!(
                "- 模式：`{}`\n- 说明：{}",
                mode.label(),
                mode.summary_note()
            )
        })
        .unwrap_or_else(|| String::from("（尚未生成上下文发送模式）"));
    let context_layers_markdown = render_context_layers_markdown(&context_assembly);
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
            "## Context Layers\n",
            "{}\n\n",
            "## Context Baseline\n",
            "{}\n\n",
            "## Context Request Mode\n",
            "{}\n\n",
            "## System Preamble\n",
            "{}\n\n",
            "## Continuation\n",
            "{}\n\n",
            "## Outgoing History\n",
            "{}\n\n",
            "## Effective Outgoing Prompt\n",
            "{}\n\n",
            "## AI Specialized Tools\n",
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
        context_layers_markdown,
        context_baseline_markdown,
        context_request_mode_markdown,
        render_text_code_fence(&request_snapshot.system_prompt),
        continuation_markdown,
        outgoing_history_markdown,
        render_text_code_fence(&request_snapshot.prompt),
        ai_tool_plan_markdown,
        tool_rows,
        request_headers_markdown,
        extra_body_markdown,
    )
}

/// Renders one compact layered-context summary for the request preview.
/// 为请求预览渲染一份紧凑的分层上下文摘要。
fn render_context_layers_markdown(assembly: &ContextAssembly) -> String {
    if assembly.layers.is_empty() {
        return String::from("（本次请求没有独立上下文层）");
    }

    assembly
        .layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            let preview = layer.content.trim();
            let preview = if preview.is_empty() {
                String::from("（空）")
            } else {
                truncate_preview(preview, 120)
            };
            format!(
                "- {}. {} · `{}` · {}",
                index + 1,
                layer.kind.label(),
                layer.label,
                preview
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Truncates one text preview to a display-safe length without breaking the request preview layout.
/// 将一段文本预览截断到安全显示长度，避免破坏请求预览布局。
fn truncate_preview(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let truncated = trimmed.chars().take(max_chars).collect::<String>();
    format!("{truncated}…")
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
                    let replay_status =
                        crate::attachment_asset_index::resolve_attachment_replay_status(attachment);
                    let replay_status_text = match replay_status {
                        crate::attachment_asset_index::AttachmentReplayStatus::Available => "可回放",
                        crate::attachment_asset_index::AttachmentReplayStatus::FallbackSourceOnly => {
                            "回退来源"
                        }
                        crate::attachment_asset_index::AttachmentReplayStatus::Purged => "已清理",
                        crate::attachment_asset_index::AttachmentReplayStatus::Missing => "已缺失",
                    };
                    format!(
                        "- `{}` · `{}` · `{}` · `{}`",
                        attachment.kind.display_label(),
                        attachment.file_name,
                        attachment.mime_type,
                        replay_status_text
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

/// Renders the AI-specialized tool routing plan for the active provider/model pair.
/// 渲染当前供应商/模型组合的 AI 专用工具路由计划。
fn render_ai_tool_plan_markdown(preparation: &ExecutionPreparation) -> String {
    let plan = resolve_ai_tool_plan(&preparation.model_blueprint);
    if plan.is_empty() {
        return String::from("（当前模型未解析到 AI 专用工具能力）");
    }

    plan.decisions
        .iter()
        .map(|decision| {
            let route_label = match decision.route {
                AiToolRouteKind::ProviderNative => "Provider 原生",
                AiToolRouteKind::HostFallback => "宿主 fallback",
                AiToolRouteKind::Disabled => "禁用",
            };
            let native_label = decision
                .native_tool_type
                .as_deref()
                .map(|value| format!("`{value}`"))
                .unwrap_or_else(|| String::from("（无）"));
            let native_patch_label = if decision.native_request_patch.is_some() {
                "已声明"
            } else {
                "未声明"
            };
            let fallback_label = decision
                .fallback_tool_id
                .as_deref()
                .map(|value| format!("`{value}`"))
                .unwrap_or_else(|| String::from("（无）"));
            let approval_label = if decision.requires_approval {
                "需要确认"
            } else {
                "无需额外确认"
            };
            let citation_label = if decision.citation_support {
                "支持引用"
            } else {
                "未声明引用"
            };

            format!(
                concat!(
                    "### `{}` · {}\n",
                    "- 路由：{}\n",
                    "- 原生类型：{}\n",
                    "- 原生请求补丁：{}\n",
                    "- fallback：{}\n",
                    "- 权限：{}\n",
                    "- 引用：{}\n",
                    "- 来源：{}\n",
                    "- 原因：{}"
                ),
                decision.kind.key(),
                decision.kind.label(),
                route_label,
                native_label,
                native_patch_label,
                fallback_label,
                approval_label,
                citation_label,
                decision.source_label,
                decision.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::build_provider_request_preview_markdown;
    use crate::agent_runtime::execution_models::ExecutionPreparation;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::provider_capabilities::{ResolvedAiToolCapability, ResolvedModelCapabilities};
    use crate::provider_config::{AiToolKind, AiToolPolicy, ProviderRuntimeKind};
    use serde_json::Value;
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use vulcan_context::{ContextFullRequestReason, ContextRequestMode};
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
            agent_system_prompt_override: None,
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("anthropic"),
                provider_name: String::from("Anthropic"),
                runtime_kind: ProviderRuntimeKind::Anthropic,
                runtime_label: String::from("Anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                model_name: String::from("Claude Sonnet 4.6"),
                openrouter_endpoint_route: None,
                base_url: Some(String::from("https://api.anthropic.com")),
                supports_remote_model_refresh: false,
                credential_source: String::from("环境变量 `ANTHROPIC_API_KEY`"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: Value::Object(Default::default()),
                reasoning: Default::default(),
                capabilities: ResolvedModelCapabilities {
                    ai_tools: vec![ResolvedAiToolCapability {
                        kind: AiToolKind::WebSearch,
                        policy: AiToolPolicy::Auto,
                        native_provider_tool: true,
                        native_tool_type: Some(String::from("web_search_20250305")),
                        native_request_patch: None,
                        fallback_tool_id: None,
                        requires_approval: false,
                        result_modalities: vec![String::from("text")],
                        citation_support: true,
                        source_label: String::from("接口类型默认"),
                    }],
                    ..Default::default()
                },
                interleaved_reasoning: None,
            },
            prompt_preview: String::from("检查请求预览"),
            workspace_path: String::from(r"D:\openvulcan\vulcan-code"),
            task_session_store_root: None,
            conversation_attachments: Vec::new(),
            context_history_count: 0,
            execution_mode: String::from("默认模式"),
            temperature: Some(0.2),
            prompt_configuration: crate::provider_config::PersistedPromptConfiguration::default(),
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            implicit_luaskill_prompt_entries: Vec::new(),
            prompt_skill_developer_segment: None,
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            provider_wire_request_json: None,
            context_budget: None,
            context_assembly: None,
            context_baseline: None,
            context_request_mode: Some(ContextRequestMode::Full {
                reason: ContextFullRequestReason::ProviderContinuationMissing,
            }),
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
        };
        let snapshot = CoreProviderRequestSnapshot {
            provider_id: String::from("anthropic"),
            model_id: String::from("claude-sonnet-4-6"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt").clone(),
            ),
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
        assert!(markdown.contains("## Context Layers"));
        assert!(markdown.contains("## Context Baseline"));
        assert!(markdown.contains("## Context Request Mode"));
        assert!(markdown.contains("- 模式：`全量上下文`"));
        assert!(markdown.contains("系统"));
        assert!(markdown.contains("## AI Specialized Tools"));
        assert!(markdown.contains("### `web_search` · 网页搜索"));
        assert!(markdown.contains("- 路由：Provider 原生"));
    }
}
