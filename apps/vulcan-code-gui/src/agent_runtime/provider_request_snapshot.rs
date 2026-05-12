//! Provider request snapshot and preview builders used by the execution runtime.
//! 供执行运行时使用的 Provider 请求快照与预览构建模块。

use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_request_bridge::{
    build_core_conversation_snapshot, core_messages_to_provider_visible_history,
    resolved_tool_definition_to_core_tool_schema, serialize_provider_request_headers,
};
use crate::agent_runtime::provider_runtime_dispatch::DEFAULT_RUNTIME_MAX_TURNS_LIMIT;
use crate::conversation_protocol::SessionConversationState;
use async_trait::async_trait;
use serde_json::{Value, json};
use vulcan_core::provider::VulcanProviderAdapter;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationMessage as CoreConversationMessage,
    ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot, CoreError as VulcanCoreError,
    ProviderContinuation as CoreProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema,
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
    let adapter = PreviewSnapshotProviderAdapter {
        provider_id: preparation.model_blueprint.provider_id.clone(),
        model_id: preparation.model_blueprint.model_id.clone(),
        prompt: prompt.to_string(),
        request_headers: serialized_headers,
        tool_schemas,
    };

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

/// Snapshot-only provider adapter used to let `vulcan-core` own request construction before real transport execution is migrated.
/// 在真实传输执行迁移完成前，仅供快照使用的 Provider 适配器，用于让 `vulcan-core` 持有请求构造权。
#[derive(Debug, Clone)]
struct PreviewSnapshotProviderAdapter {
    /// Stable provider identifier used by the snapshot adapter.
    /// 快照适配器使用的稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier used by the snapshot adapter.
    /// 快照适配器使用的稳定模型标识。
    model_id: String,
    /// Latest outgoing prompt body attached to the snapshot request.
    /// 附着在快照请求中的最新出站提示词正文。
    prompt: String,
    /// Serialized request headers captured from the active provider blueprint.
    /// 从当前 Provider 蓝图捕获的序列化请求头。
    request_headers: Option<Value>,
    /// Tool schemas exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的工具 schema 集合。
    tool_schemas: Vec<CoreProviderRequestToolSchema>,
}

#[async_trait]
impl VulcanProviderAdapter for PreviewSnapshotProviderAdapter {
    type Request = CoreProviderRequestSnapshot;
    type Response = ();

    /// Returns the stable provider identifier used by the snapshot adapter.
    /// 返回快照适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the snapshot adapter.
    /// 返回快照适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one structured request snapshot owned by `vulcan-core`.
    /// 构建一份由 `vulcan-core` 持有的结构化请求快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        Ok(CoreProviderRequestSnapshot {
            provider_id: config.provider_id.clone(),
            model_id: config.model_id.clone(),
            system_prompt: config.system_prompt.clone(),
            prompt: self.prompt.clone(),
            history: core_messages_to_provider_visible_history(&snapshot.messages),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: self.tool_schemas.clone(),
            additional_params: config.additional_params.clone(),
            request_headers: self.request_headers.clone(),
            continuation: snapshot.continuation.clone(),
        })
    }

    /// Snapshot adapters do not perform real network execution and therefore always return a no-op response.
    /// 快照适配器不会执行真实网络调用，因此始终返回一次空操作响应。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Ok(())
    }
}

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
            "- 接口：`{}`\n",
            "- 执行模式：`{}`\n",
            "- 温度：{}\n",
            "- 最大轮次：{}\n",
            "- 历史上下文：`{}` 条\n",
            "- 工具数量：`{}` 个\n\n",
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
        preparation.model_blueprint.endpoint_summary(),
        preparation.execution_mode,
        temperature_label,
        max_turns_label,
        request_snapshot.history.len(),
        request_snapshot.tool_schemas.len(),
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
        CoreConversationPayload::UserText { text } => format!(
            "### {}. user · `{}`\n{}",
            index + 1,
            message.id,
            render_text_code_fence(text)
        ),
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            tool_calls,
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
