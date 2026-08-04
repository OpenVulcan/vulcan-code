//! Provider request preview Markdown renderer used by the execution runtime.
//! 供执行运行时使用的 Provider 请求预览 Markdown 渲染模块。

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
    // current_rewrite_fingerprint hashes the exact final system prompt shown in this request snapshot.
    // current_rewrite_fingerprint 对该请求快照中展示的精确最终系统提示词计算哈希。
    let current_rewrite_fingerprint =
        vulcan_core::types::provider_rewrite_fingerprint_for_system_prompt(
            request_snapshot.system_prompt.as_str(),
        );
    // local_boundary_matches locates the exact canonical message represented by the scoped provider cursor.
    // local_boundary_matches 定位限定作用域的 Provider 游标所表示的精确规范消息。
    let local_boundary_matches = request_snapshot
        .continuation
        .last_local_message_id
        .as_deref()
        .map(|boundary_id| {
            request_snapshot
                .history
                .iter()
                .enumerate()
                .filter_map(|(index, message)| (message.id == boundary_id).then_some(index))
                .collect::<Vec<_>>()
        });
    // continuation_boundary_is_proven mirrors dispatch: a cursor must have one unique historical local boundary.
    // continuation_boundary_is_proven 与实际分发一致：游标必须具备唯一历史本地边界。
    let continuation_boundary_is_proven = if request_snapshot
        .continuation
        .last_provider_message_id
        .is_none()
    {
        true
    } else {
        local_boundary_matches
            .as_deref()
            .is_some_and(|indices| indices.len() == 1)
    };
    // uncovered_history_start begins after the proven boundary, or at zero when no cursor exists.
    // uncovered_history_start 从已证明边界之后开始；没有游标时从零开始。
    let uncovered_history_start = local_boundary_matches
        .as_deref()
        .and_then(|indices| indices.first())
        .map_or(0, |index| index.saturating_add(1));
    // uncovered_message_ids contains historical suffix ids plus the successful exact anchor target for the current prompt.
    // uncovered_message_ids 包含历史后缀标识，以及当前提示消息上成功绑定的精确锚点目标。
    let mut uncovered_message_ids = request_snapshot.history[uncovered_history_start..]
        .iter()
        .map(|message| message.id.as_str())
        .collect::<Vec<_>>();
    if let Some(injection) = preparation.context_injection.as_ref() {
        for target_message_id in injection
            .anchored_prefixes
            .iter()
            .map(|anchor| anchor.target_message_id.as_str())
            .filter(|target_message_id| {
                request_snapshot
                    .history
                    .iter()
                    .all(|message| message.id != *target_message_id)
            })
        {
            if !uncovered_message_ids.contains(&target_message_id) {
                uncovered_message_ids.push(target_message_id);
            }
        }
    }
    let context_injection_diagnostics = render_provider_context_injection_diagnostics(
        preparation.context_injection.as_ref(),
        &request_snapshot.continuation,
        current_rewrite_fingerprint.as_str(),
        &uncovered_message_ids,
        continuation_boundary_is_proven,
        preparation.model_blueprint.protocol_profile_key()
            != vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
            || vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().openai_append_continuation_enabled,
    );
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
            "## Context Injection Diagnostics\n",
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
        preparation.model_blueprint.protocol_profile_label(),
        preparation.model_blueprint.protocol_profile_key(),
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
        context_injection_diagnostics,
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
            tool_call_id,
            is_error,
            text,
        } => format!(
            "### {}. tool_result · `{}`{} · error=`{}`\n{}",
            index + 1,
            message.id,
            tool_call_id
                .as_deref()
                .map(|call_id| format!(" · call_id=`{call_id}`"))
                .unwrap_or_default(),
            is_error,
            render_text_code_fence(text)
        ),
        CoreConversationPayload::AgentMessage { .. } => format!(
            "### {}. agent_message · `{}`\n{}",
            index + 1,
            message.id,
            render_text_code_fence(
                &message
                    .payload
                    .agent_message_model_text()
                    .unwrap_or_default()
            )
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
        && continuation.last_local_message_id.is_none()
        && continuation.context_cache_epoch_id.is_none()
        && continuation.context_anchor_revision.is_none()
        && continuation.context_rewrite_fingerprint.is_none()
        && continuation.updated_at.is_none()
    {
        return String::from("（本次请求没有 continuation 游标）");
    }

    let payload = json!({
        "provider_id": continuation.provider_id,
        "model_id": continuation.model_id,
        "last_provider_message_id": continuation.last_provider_message_id,
        "last_local_message_id": continuation.last_local_message_id,
        "context_cache_epoch_id": continuation.context_cache_epoch_id,
        "context_anchor_revision": continuation.context_anchor_revision,
        "context_rewrite_fingerprint": continuation.context_rewrite_fingerprint,
        "updated_at": continuation.updated_at,
    });
    render_json_code_fence(&payload)
}

/// Renders cache-epoch, anchor revision, mutation class, and continuation safety diagnostics.
/// 渲染缓存纪元、锚点修订、变更分类与 continuation 安全诊断。
///
/// # Parameters
/// 参数
/// `injection` is the exact provider context overlay applied to the request.
/// `injection` 是应用到请求的精确 Provider 上下文覆盖层。
/// `continuation` is the scoped provider cursor considered for reuse.
/// `continuation` 是参与复用判断的 Provider 作用域游标。
/// `current_rewrite_fingerprint` is the fingerprint of the exact current Provider system input.
/// `current_rewrite_fingerprint` 是当前精确 Provider 系统输入的指纹。
/// `uncovered_message_ids` are exact local messages appended after the saved cursor boundary.
/// `uncovered_message_ids` 是保存在游标边界之后追加的精确本地消息集合。
/// `continuation_boundary_is_proven` reports whether the scoped cursor has one unique local boundary.
/// `continuation_boundary_is_proven` 表示限定作用域的游标是否具备唯一的本地边界。
/// `append_continuation_feature_enabled` is the effective provider-specific rollout decision.
/// `append_continuation_feature_enabled` 是生效的 Provider 专属追加续传灰度决策。
///
/// # Returns
/// 返回值
/// A compact Markdown list with a deterministic continuation decision reason.
/// 返回包含确定性 continuation 决策原因的紧凑 Markdown 列表。
fn render_provider_context_injection_diagnostics(
    injection: Option<&vulcan_core::types::ProviderContextInjection>,
    continuation: &CoreProviderContinuation,
    current_rewrite_fingerprint: &str,
    uncovered_message_ids: &[&str],
    continuation_boundary_is_proven: bool,
    append_continuation_feature_enabled: bool,
) -> String {
    // The mutation label is stable user-facing diagnostic text, independent of debug formatting.
    // 变更标签是稳定的用户诊断文本，不依赖调试格式。
    let mutation_label = injection.map_or("none", |injection| match injection.mutation_kind {
        vulcan_core::types::ProviderContextMutationKind::None => "none",
        vulcan_core::types::ProviderContextMutationKind::AppendOnly => "append_only",
        vulcan_core::types::ProviderContextMutationKind::HistoricalRewrite => "historical_rewrite",
        vulcan_core::types::ProviderContextMutationKind::EpochReset => "epoch_reset",
    });
    // rewrite_fingerprint_matches proves the cursor covered the exact current system input.
    // rewrite_fingerprint_matches 证明游标覆盖了精确的当前系统输入。
    let rewrite_fingerprint_matches =
        continuation.context_rewrite_fingerprint.as_deref() == Some(current_rewrite_fingerprint);
    // overlay_reuse_is_proven applies the same transient and anchor-coverage predicate as dispatch.
    // overlay_reuse_is_proven 应用与实际分发相同的临时内容和锚点覆盖判定。
    let overlay_reuse_is_proven = match injection {
        Some(injection) => {
            injection.continuation_reuse_is_proven(continuation, uncovered_message_ids)
        }
        None => {
            continuation.context_cache_epoch_id.is_none()
                && continuation.context_anchor_revision.is_none()
        }
    };
    // append_vmm_change identifies the only continuation behavior controlled by the rollout switch.
    // append_vmm_change 标识唯一受灰度开关控制的 continuation 行为。
    let append_vmm_change = injection.is_some_and(|injection| {
        injection.mutation_kind == vulcan_core::types::ProviderContextMutationKind::AppendOnly
            && !injection.anchored_prefixes.is_empty()
    });
    // The decision reason mirrors the provider continuation safety predicate exactly.
    // 决策原因与 Provider continuation 安全判定谓词严格一致。
    let continuation_decision = if continuation.last_provider_message_id.is_none() {
        "不复用：当前没有 Provider continuation 游标"
    } else if !continuation_boundary_is_proven {
        "不复用：Provider continuation 缺少唯一的本地规范消息边界"
    } else if append_vmm_change && !append_continuation_feature_enabled {
        "不复用：OpenAI Responses 追加式 VMM continuation 灰度开关已关闭"
    } else if !rewrite_fingerprint_matches {
        "不复用：系统或持久上下文指纹与游标覆盖内容不一致"
    } else if overlay_reuse_is_proven {
        "允许复用：无历史改写，且锚点覆盖关系可证明安全"
    } else if injection.is_some_and(|injection| injection.requires_historical_rewrite()) {
        "不复用：上下文覆盖层会改写既有 Provider 输入"
    } else {
        "不复用：缓存纪元或锚点修订覆盖关系无法证明安全"
    };
    format!(
        concat!(
            "- 变更分类：`{}`\n",
            "- 缓存纪元：`{}`\n",
            "- 锚点修订：`{}`\n",
            "- 锚点数量：`{}`\n",
            "- 覆盖层指纹：`{}`\n",
            "- 当前改写指纹：`{}`\n",
            "- 活跃 VMM 正文：`{} bytes / ~{} tokens`\n",
            "- 可回收 VMM 锚点：`{} bytes / ~{} tokens`\n",
            "- Continuation：{}"
        ),
        mutation_label,
        injection
            .and_then(|injection| injection.cache_epoch_id.as_deref())
            .unwrap_or("none"),
        injection
            .and_then(|injection| injection.anchor_revision)
            .map(|revision| revision.to_string())
            .unwrap_or_else(|| String::from("none")),
        injection.map_or(0, |injection| injection.anchored_prefixes.len()),
        injection.map_or("none", |injection| injection.fingerprint.as_str()),
        current_rewrite_fingerprint,
        injection.map_or(0, |injection| injection.active_memory_body_bytes),
        injection.map_or(0, |injection| injection.active_memory_estimated_tokens),
        injection.map_or(0, |injection| injection.reclaimable_anchor_bytes),
        injection.map_or(0, |injection| {
            injection.reclaimable_anchor_estimated_tokens
        }),
        continuation_decision,
    )
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
    let _ = preparation;
    String::from("（AI 专用能力由当前公共契约模型快照动态注册）")
}

#[cfg(test)]
mod tests {
    use super::build_provider_request_preview_markdown;
    use crate::agent_runtime::execution_models::ExecutionPreparation;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use uuid::Uuid;
    use vulcan_context::{ContextFullRequestReason, ContextRequestMode};
    use vulcan_core::types::{
        ProviderContinuation as CoreProviderContinuation,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    };

    /// Verifies the provider request preview explicitly renders the public protocol profile.
    /// 验证 Provider 请求预览会明确渲染公共协议画像。
    #[test]
    fn provider_request_preview_renders_protocol_profile_summary_line() {
        let preparation = ExecutionPreparation {
            session_id: Uuid::nil(),
            session_title: String::from("接口族预览测试"),
            agent_id: String::from("main"),
            agent_name: String::from("Main Agent"),
            agent_kind: String::from("analysis"),
            agent_prompt_snapshot: None,
            subagent_prompt_context: None,
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("anthropic"),
                service_entry_id: String::from("public-api"),
                connection_id: String::from("test-connection"),
                provider_name: String::from("Anthropic"),
                runtime_label: String::from("Anthropic"),
                model_id: String::from("claude-sonnet-4-6"),
                model_name: String::from("Claude Sonnet 4.6"),
                protocol_profile_id: String::from(
                    vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
                ),
                secondary_route: Default::default(),
                public_capabilities: vulcan_ai_contract::ModelCapabilities::default(),
                base_url: Some(String::from("https://api.anthropic.com")),
                supports_remote_model_refresh: false,
                credential_source: String::from("环境变量 `ANTHROPIC_API_KEY`"),
                reasoning: Default::default(),
            },
            prompt_preview: String::from("检查请求预览"),
            workspace_path: String::from(r"D:\openvulcan\vulcan-code"),
            task_session_store_root: None,
            conversation_attachments: Vec::new(),
            context_history_count: 0,
            execution_mode: String::from("默认模式"),
            temperature: Some(0.2),
            prompt_configuration: crate::provider_config::PersistedPromptConfiguration::default(),
            output_style_snapshot: None,
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            implicit_luaskill_prompt_entries: Vec::new(),
            prompt_skill_developer_segment: None,
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            provider_wire_request_json: None,
            context_budget: None,
            context_compaction_policy: None,
            context_budget_override: None,
            context_assembly: None,
            context_baseline: None,
            context_request_mode: Some(ContextRequestMode::Full {
                reason: ContextFullRequestReason::ProviderContinuationMissing,
            }),
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
            boundary_events: Vec::new(),
            runtime_system_updates: Vec::new(),
            session_goal_context: None,
            session_plan_context: None,
            embedded_turn_usage_segments: Vec::new(),
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

        assert!(markdown.contains("- 运行时：`Anthropic` · kind=`public-ai-contract`"));
        assert!(
            markdown.contains("- 接口族：`Anthropic Messages` · family=`anthropic-messages.v1`")
        );
        assert!(markdown.contains("## Context Layers"));
        assert!(markdown.contains("## Context Baseline"));
        assert!(markdown.contains("## Context Request Mode"));
        assert!(markdown.contains("- 模式：`全量上下文`"));
        assert!(markdown.contains("系统"));
        assert!(markdown.contains("## AI Specialized Tools"));
        assert!(markdown.contains("AI 专用能力由当前公共契约模型快照动态注册"));
    }
}
