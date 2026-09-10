//! Anthropic Messages wire profile.
//! Anthropic Messages Wire Profile。

use crate::{
    EncodedWireRequest, ProtocolConversionError, ProtocolProfile, ProtocolStreamDecoder,
    shared::append_bounded_tool_arguments,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiContinuationUpdate, AiErrorCategory, AiExecutionError, AiExecutionRequest, AiStreamEvent,
    ContentItem, DispatchOutcome, MessageRole, NativeToolKind, ParallelToolCallPolicy,
    PromptCachePolicy, PromptCacheTtlSelection, ReasoningOutputPolicy, RequestUsage,
    StructuredOutputPolicy, ToolCall, ToolCallProgress, ToolChoicePolicy, UsageValue,
    UsageValueSource, derive_usage_sum,
};

/// Stable Anthropic Messages profile identifier.
/// 稳定的 Anthropic Messages Profile 标识。
pub const ANTHROPIC_MESSAGES_PROFILE_ID: &str = "anthropic-messages.v1";

/// Stable Anthropic-compatible profile identifier with a registered video block extension.
/// 带已注册视频内容块扩展的稳定 Anthropic 兼容 Profile 标识。
pub const ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID: &str = "anthropic-messages-video.v1";
/// Stable Claude Code subscription Messages profile identifier.
/// 稳定的 Claude Code 订阅 Messages Profile 标识。
pub const CLAUDE_CODE_MESSAGES_PROFILE_ID: &str = "claude-code-messages.v1";

/// Exact lowercase-to-Claude-Code tool names reviewed against CLIProxyAPI.
/// 对照 CLIProxyAPI 审核后的精确小写到 Claude Code 工具名映射。
const CLAUDE_CODE_TOOL_NAME_PAIRS: [(&str, &str); 14] = [
    ("bash", "Bash"),
    ("read", "Read"),
    ("write", "Write"),
    ("edit", "Edit"),
    ("glob", "Glob"),
    ("grep", "Grep"),
    ("task", "Task"),
    ("webfetch", "WebFetch"),
    ("todowrite", "TodoWrite"),
    ("question", "Question"),
    ("skill", "Skill"),
    ("ls", "LS"),
    ("todoread", "TodoRead"),
    ("notebookedit", "NotebookEdit"),
];

/// Encodes public requests as Anthropic Messages requests.
/// 将公共请求编码为 Anthropic Messages 请求。
#[derive(Debug, Default)]
pub struct AnthropicMessagesProfile;

impl ProtocolProfile for AnthropicMessagesProfile {
    /// Returns the stable Anthropic Messages profile identifier.
    /// 返回稳定的 Anthropic Messages Profile 标识。
    fn id(&self) -> &'static str {
        ANTHROPIC_MESSAGES_PROFILE_ID
    }

    /// Returns the bounded Anthropic SSE transport declaration.
    /// 返回有界 Anthropic SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
    }

    /// Encodes one public request as Anthropic Messages JSON.
    /// 将一个公共请求编码为 Anthropic Messages JSON。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        require_anthropic_native_tools(request, self.id())?;
        encode_anthropic_request(request, false, self.id())
    }

    fn create_stream_decoder(
        &self,
        execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(AnthropicStreamDecoder::new(execution_id, BTreeMap::new()))
    }
}

/// Encodes Anthropic-compatible requests with the registered video block extension.
/// 使用已注册的视频内容块扩展编码 Anthropic 兼容请求。
#[derive(Debug, Default)]
pub struct AnthropicMessagesVideoProfile;

impl ProtocolProfile for AnthropicMessagesVideoProfile {
    /// Returns the stable video-extension profile identifier.
    /// 返回稳定的视频扩展 Profile 标识。
    fn id(&self) -> &'static str {
        ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
    }

    /// Returns the bounded Anthropic video SSE transport declaration.
    /// 返回有界 Anthropic 视频 SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
    }

    /// Encodes one request with the explicit video extension enabled.
    /// 在显式启用视频扩展的情况下编码一个请求。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::shared::require_no_native_tools(request, self.id())?;
        encode_anthropic_request(request, true, self.id())
    }

    /// Creates one isolated Anthropic-compatible stream decoder.
    /// 创建一个隔离的 Anthropic 兼容流解码器。
    fn create_stream_decoder(
        &self,
        execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(AnthropicStreamDecoder::new(execution_id, BTreeMap::new()))
    }
}

/// Encodes public requests through the isolated Claude Code Messages identity.
/// 通过隔离的 Claude Code Messages 身份编码公共请求。
#[derive(Debug, Default)]
pub struct ClaudeCodeMessagesProfile;

impl ProtocolProfile for ClaudeCodeMessagesProfile {
    /// Returns the stable Claude Code profile identifier.
    /// 返回稳定的 Claude Code Profile 标识。
    fn id(&self) -> &'static str {
        CLAUDE_CODE_MESSAGES_PROFILE_ID
    }

    /// Returns the bounded Claude Code SSE transport declaration.
    /// 返回有界 Claude Code SSE 传输声明。
    fn transport_profile(&self) -> crate::ProtocolTransportProfile {
        crate::ProtocolTransportProfile::bounded_sse()
    }

    /// Encodes one request while retaining Claude Code-specific route identity.
    /// 在保留 Claude Code 专用路由身份的情况下编码一个请求。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        crate::shared::require_no_native_tools(request, self.id())?;
        let (forward_names, _) = claude_code_tool_name_maps(request);
        let mut encoded = encode_anthropic_request(request, false, self.id())?;
        remap_claude_code_request_tool_names(&mut encoded.body, &forward_names)?;
        Ok(encoded)
    }

    /// Creates one isolated Claude Code stream decoder.
    /// 创建一个隔离的 Claude Code 流解码器。
    fn create_stream_decoder(
        &self,
        execution_id: String,
        request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        let (_, reverse_names) = claude_code_tool_name_maps(request);
        Box::new(AnthropicStreamDecoder::new(execution_id, reverse_names))
    }
}

/// Builds request-scoped forward and reverse Claude Code tool-name maps.
/// 构建请求作用域的 Claude Code 工具名前向与反向映射。
///
/// # Parameters
/// 参数
/// `request` supplies the exact model-visible tool set for this execution.
/// `request` 提供本次执行精确的模型可见工具集合。
///
/// # Returns
/// 返回值
/// Public-to-upstream and upstream-to-public maps containing only names actually rewritten.
/// 返回仅包含实际改写名称的公共到上游及上游到公共映射。
fn claude_code_tool_name_maps(
    request: &AiExecutionRequest,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let registered = request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut forward = BTreeMap::new();
    let mut reverse = BTreeMap::new();
    for (public_name, upstream_name) in CLAUDE_CODE_TOOL_NAME_PAIRS {
        if registered.contains(public_name) {
            forward.insert(public_name.to_string(), upstream_name.to_string());
            reverse
                .entry(upstream_name.to_string())
                .or_insert_with(|| public_name.to_string());
        }
    }
    (forward, reverse)
}

/// Rewrites exact request-scoped tool definitions and historical tool uses for Claude Code OAuth.
/// 为 Claude Code OAuth 改写精确请求作用域工具定义与历史工具调用。
///
/// # Parameters
/// 参数
/// `body` is the already encoded Anthropic request and `forward_names` contains only verified
/// tool names declared by this request.
/// `body` 是已编码 Anthropic 请求，`forward_names` 仅包含该请求声明的已验证工具名。
///
/// # Returns
/// 返回值
/// Success after deterministic in-place rewriting, or a structural protocol failure.
/// 确定性原地改写后返回成功，结构异常时返回协议错误。
fn remap_claude_code_request_tool_names(
    body: &mut Value,
    forward_names: &BTreeMap<String, String>,
) -> Result<(), ProtocolConversionError> {
    if forward_names.is_empty() {
        return Ok(());
    }
    if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            let name = tool.get("name").and_then(Value::as_str).ok_or_else(|| {
                ProtocolConversionError::new("Claude Code tool definition is missing `name`.")
            })?;
            if let Some(upstream_name) = forward_names.get(name) {
                tool["name"] = Value::String(upstream_name.clone());
            }
        }
    }
    if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
                continue;
            };
            for block in content {
                if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                    continue;
                }
                let name = block.get("name").and_then(Value::as_str).ok_or_else(|| {
                    ProtocolConversionError::new(
                        "Claude Code historical tool-use block is missing `name`.",
                    )
                })?;
                if let Some(upstream_name) = forward_names.get(name) {
                    block["name"] = Value::String(upstream_name.clone());
                }
            }
        }
    }
    Ok(())
}

/// Rejects native tools that have no exact Anthropic Messages projection.
/// 拒绝没有精确 Anthropic Messages 投影的原生工具。
///
/// # Parameters
/// 参数
/// `request` contains the closed native-tool selection and `profile_id` identifies the route.
/// `request` 包含封闭原生工具选择，`profile_id` 标识当前路由。
///
/// # Returns
/// 返回值
/// Success when every selected native tool is supported by this profile.
/// 当每个选中的原生工具均受此 Profile 支持时返回成功。
fn require_anthropic_native_tools(
    request: &AiExecutionRequest,
    profile_id: &str,
) -> Result<(), ProtocolConversionError> {
    if let Some(unsupported) = request
        .native_tools
        .iter()
        .find(|native_tool| !matches!(native_tool, NativeToolKind::WebSearch))
    {
        return Err(ProtocolConversionError::new(format!(
            "Protocol profile `{profile_id}` cannot represent native tool `{unsupported:?}`."
        )));
    }
    Ok(())
}

/// Appends exact Anthropic server-tool declarations after client function tools.
/// 在客户端函数工具之后追加精确的 Anthropic 服务端工具声明。
///
/// # Parameters
/// 参数
/// `request` owns the native-tool selection and `tools` is the mutable Anthropic tool array.
/// `request` 持有原生工具选择，`tools` 是可变 Anthropic 工具数组。
fn append_anthropic_native_tools(request: &AiExecutionRequest, tools: &mut Vec<Value>) {
    for native_tool in &request.native_tools {
        if matches!(native_tool, NativeToolKind::WebSearch) {
            tools.push(json!({
                "type": "web_search_20250305",
                "name": "web_search",
            }));
        }
    }
}

/// Encodes one Anthropic-compatible request with an explicit feature boundary.
/// 使用显式功能边界编码一个 Anthropic 兼容请求。
///
/// # Parameters
/// 参数
/// `request` is the public request and `allow_video` selects the registered
/// compatible-protocol extension.
/// `request` 是公共请求，`allow_video` 选择已注册的兼容协议扩展。
///
/// # Returns
/// 返回值
/// The exact wire request or a controlled unsupported-resource failure.
/// 返回精确 Wire 请求或受控的不支持资源错误。
fn encode_anthropic_request(
    request: &AiExecutionRequest,
    allow_video: bool,
    protocol_id: &str,
) -> Result<EncodedWireRequest, ProtocolConversionError> {
    crate::require_json_schema_tool_inputs(request, protocol_id)?;
    crate::require_no_route_bound_provider_context(request, protocol_id)?;
    let mut system_blocks = Vec::new();
    let mut message_values = Vec::new();
    let mut pending_tool_blocks = Vec::new();
    for message in &request.messages {
        let content = encode_content_items(&message.content, allow_video)?;
        if message.role == MessageRole::Tool {
            pending_tool_blocks.extend(content);
            continue;
        }
        if !pending_tool_blocks.is_empty() {
            message_values.push(json!({
                "role": "user",
                "content": std::mem::take(&mut pending_tool_blocks),
            }));
        }
        match message.role {
            MessageRole::System | MessageRole::Developer => system_blocks.extend(content),
            MessageRole::User | MessageRole::Assistant => {
                let role = match message.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System | MessageRole::Developer | MessageRole::Tool => {
                        unreachable!()
                    }
                };
                message_values.push(json!({ "role": role, "content": content }));
            }
            MessageRole::Tool => unreachable!(),
        }
    }
    if !pending_tool_blocks.is_empty() {
        message_values.push(json!({
            "role": "user",
            "content": pending_tool_blocks,
        }));
    }
    let mut body = Map::from_iter([
        (String::from("model"), json!(request.model_id)),
        (String::from("messages"), Value::Array(message_values)),
        (
            String::from("max_tokens"),
            json!(request.output_token_limit.ok_or_else(|| {
                ProtocolConversionError::new(
                    "Anthropic Messages requires an explicit resolved output token limit.",
                )
            })?),
        ),
        (String::from("stream"), Value::Bool(true)),
    ]);
    if !system_blocks.is_empty() {
        body.insert(String::from("system"), Value::Array(system_blocks));
    }
    if !request.tools.is_empty() || !request.native_tools.is_empty() {
        if let Some(tool) = request.tools.iter().find(|tool| tool.strict) {
            return Err(ProtocolConversionError::new(format!(
                "Anthropic Messages has no registered strict-schema projector for tool `{}`.",
                tool.name
            )));
        }
        let mut tools = request
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.input_schema,
                })
            })
            .collect::<Vec<_>>();
        append_anthropic_native_tools(request, &mut tools);
        body.insert(String::from("tools"), Value::Array(tools));
    }
    if let Some(effort) = request.reasoning_effort {
        body.insert(
            String::from("output_config"),
            json!({ "effort": effort.as_str() }),
        );
    }
    project_anthropic_request_controls(request, &mut body)?;
    encode_anthropic_cache_policy(&mut body, &request.prompt_cache)?;
    Ok(EncodedWireRequest {
        path: String::from("/v1/messages"),
        headers: std::collections::BTreeMap::from([(
            String::from("anthropic-version"),
            String::from("2023-06-01"),
        )]),
        body: Value::Object(body),
    })
}

/// Projects provider-neutral controls into the Anthropic Messages request shape.
/// 将供应商无关控制投影到 Anthropic Messages 请求结构。
///
/// # Parameters
/// 参数
/// `request` is the exact public request and `body` is the mutable Messages object.
/// `request` 是精确公共请求，`body` 是可变 Messages 对象。
///
/// # Returns
/// 返回值
/// A controlled conversion error when Anthropic has no exact representation.
/// 当 Anthropic 没有精确表示时返回受控转换错误。
fn project_anthropic_request_controls(
    request: &AiExecutionRequest,
    body: &mut Map<String, Value>,
) -> Result<(), ProtocolConversionError> {
    let mut tool_choice = match &request.tool_choice {
        ToolChoicePolicy::Auto => None,
        ToolChoicePolicy::Required => Some(json!({ "type": "any" })),
        ToolChoicePolicy::Disabled => Some(json!({ "type": "none" })),
        ToolChoicePolicy::Specific { tool_name } => {
            require_anthropic_registered_tool(request, tool_name)?;
            Some(json!({ "type": "tool", "name": tool_name }))
        }
    };
    match request.parallel_tool_calls {
        ParallelToolCallPolicy::ProviderDefault | ParallelToolCallPolicy::Allowed => {}
        ParallelToolCallPolicy::Disabled => {
            tool_choice.get_or_insert_with(|| json!({ "type": "auto" }))["disable_parallel_tool_use"] =
                Value::Bool(true);
        }
    }
    if let Some(tool_choice) = tool_choice {
        body.insert(String::from("tool_choice"), tool_choice);
    }
    if !request.stop_sequences.is_empty() {
        body.insert(
            String::from("stop_sequences"),
            json!(request.stop_sequences),
        );
    }
    match &request.structured_output {
        StructuredOutputPolicy::None => {}
        StructuredOutputPolicy::JsonObject => {
            // Anthropic structured JSON output is schema-driven; refusing schema-free JSON prevents a false text-only guarantee.
            // Anthropic 的结构化 JSON 输出由 Schema 驱动；拒绝无 Schema 请求可避免把纯文本回退误报为结构化保证。
            return Err(ProtocolConversionError::new(
                "Anthropic Messages requires an explicit JSON Schema for structured JSON output.",
            ));
        }
        StructuredOutputPolicy::JsonSchema { schema, .. } => {
            let output_config = body
                .entry(String::from("output_config"))
                .or_insert_with(|| Value::Object(Map::new()));
            output_config["format"] = json!({
                "type": "json_schema",
                "schema": schema,
            });
        }
    }
    match request.reasoning_output {
        ReasoningOutputPolicy::ProviderDefault | ReasoningOutputPolicy::Hidden => {
            // Anthropic exposes no separate "hide reasoning" response switch. Hidden remains a safe consumer-side policy because callers can ignore reasoning blocks without changing the upstream request.
            // Anthropic 没有独立的“隐藏思考”响应开关；调用方可以忽略思考块，因此 Hidden 可作为不改变上游请求的安全消费侧策略。
        }
        ReasoningOutputPolicy::SummaryAuto | ReasoningOutputPolicy::Detailed => {
            return Err(ProtocolConversionError::new(
                "Anthropic Messages cannot project the requested public reasoning-output visibility.",
            ));
        }
    }
    Ok(())
}

/// Verifies an Anthropic specific-tool selection references a registered request tool.
/// 验证 Anthropic 特定工具选择引用了已注册请求工具。
///
/// # Parameters
/// 参数
/// `request` owns the registered tools and `tool_name` is the exact selected name.
/// `request` 拥有已注册工具，`tool_name` 是精确所选名称。
///
/// # Returns
/// 返回值
/// Success only when the requested tool exists.
/// 仅当请求工具存在时成功。
fn require_anthropic_registered_tool(
    request: &AiExecutionRequest,
    tool_name: &str,
) -> Result<(), ProtocolConversionError> {
    if request.tools.iter().any(|tool| tool.name == tool_name) {
        return Ok(());
    }
    Err(ProtocolConversionError::new(format!(
        "Anthropic tool choice references unregistered tool `{tool_name}`."
    )))
}

/// Decodes Anthropic Messages SSE events without parsing incomplete tool JSON.
/// 解码 Anthropic Messages SSE 事件，并且不解析尚未完整的工具 JSON。
#[derive(Debug)]
struct AnthropicStreamDecoder {
    /// Stable execution identifier.
    /// 稳定执行标识。
    execution_id: String,
    /// Whether the response start event was emitted.
    /// 是否已经发送响应开始事件。
    started: bool,
    /// Whether a terminal event was emitted.
    /// 是否已经发送终态事件。
    terminal: bool,
    /// Stable upstream message identifier used by a later continuation request.
    /// 后续延续请求使用的稳定上游消息标识。
    message_id: Option<String>,
    /// Open content blocks keyed by Anthropic index.
    /// 按 Anthropic 索引保存的打开内容块。
    blocks: BTreeMap<u32, AnthropicBlock>,
    /// Request-scoped upstream-to-public tool names produced by the Claude Code profile.
    /// Claude Code Profile 产生的请求作用域上游到公共工具名映射。
    tool_names: BTreeMap<String, String>,
}

/// Tracks one Anthropic content block.
/// 跟踪一个 Anthropic 内容块。
#[derive(Debug)]
enum AnthropicBlock {
    /// Text output item.
    /// 文本输出项。
    Text {
        /// Stable public item identifier.
        /// 稳定公共项目标识。
        item_id: String,
    },
    /// Reasoning output item.
    /// 推理输出项。
    Reasoning {
        /// Stable public item identifier.
        /// 稳定公共项目标识。
        item_id: String,
        /// Optional streamed signature.
        /// 可选流式签名。
        signature: Option<String>,
    },
    /// Incomplete tool call.
    /// 尚未完整的工具调用。
    Tool {
        /// Stable public item identifier.
        /// 稳定公共项目标识。
        item_id: String,
        /// Upstream call identifier.
        /// 上游调用标识。
        call_id: String,
        /// Registered tool name.
        /// 已注册工具名称。
        tool_name: String,
        /// Raw accumulated JSON text.
        /// 累积的原始 JSON 文本。
        arguments: String,
    },
    /// Provider-managed server tool blocks whose execution is completed upstream.
    /// 已由供应商上游完成执行的服务端工具内容块。
    Ignored,
}

impl AnthropicStreamDecoder {
    /// Creates one decoder for a single execution.
    /// 为单次执行创建一个解码器。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the stable public execution identifier.
    /// `execution_id` 是稳定公共执行标识。
    ///
    /// # Returns
    /// 返回值
    /// A fresh decoder with no observed wire state.
    /// 返回尚未观察任何 Wire 状态的新解码器。
    fn new(execution_id: String, tool_names: BTreeMap<String, String>) -> Self {
        Self {
            execution_id,
            started: false,
            terminal: false,
            message_id: None,
            blocks: BTreeMap::new(),
            tool_names,
        }
    }

    /// Starts a response once and returns the corresponding event when needed.
    /// 仅启动一次响应，并在需要时返回对应事件。
    fn start_if_needed(&mut self) -> Vec<AiStreamEvent> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![AiStreamEvent::ResponseStarted {
            execution_id: self.execution_id.clone(),
        }]
    }

    /// Captures the immutable Anthropic message identifier from `message_start`.
    /// 从 `message_start` 捕获不可变 Anthropic 消息标识。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the decoded `message_start` payload.
    /// `value` 是已解码的 `message_start` 载荷。
    ///
    /// # Returns
    /// 返回值
    /// One metadata update for the first identifier, or an error if the identifier changes.
    /// 首次标识返回一条元数据更新；标识变化时返回错误。
    fn capture_message_id(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let Some(message_id) = value
            .pointer("/message/id")
            .and_then(Value::as_str)
            .filter(|message_id| !message_id.trim().is_empty())
        else {
            return Ok(Vec::new());
        };
        if let Some(existing) = self.message_id.as_deref() {
            if existing != message_id {
                return Err(ProtocolConversionError::new(
                    "Anthropic changed message identity during one stream.",
                ));
            }
            return Ok(Vec::new());
        }
        self.message_id = Some(message_id.to_string());
        Ok(vec![AiStreamEvent::ResponseMetadataUpdated {
            continuation: AiContinuationUpdate {
                provider_message_id: message_id.to_string(),
            },
        }])
    }

    /// Decodes an Anthropic content-block start event.
    /// 解码 Anthropic 内容块开始事件。
    fn decode_block_start(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index = required_u32(value, "index")?;
        let block = value
            .get("content_block")
            .and_then(Value::as_object)
            .ok_or_else(|| ProtocolConversionError::new("Anthropic content_block is missing."))?;
        let block_type = block.get("type").and_then(Value::as_str).ok_or_else(|| {
            ProtocolConversionError::new("Anthropic content block type is missing.")
        })?;
        let item_id = format!("anthropic:{}:{index}", self.execution_id);
        let event = match block_type {
            "text" => {
                self.blocks.insert(
                    index,
                    AnthropicBlock::Text {
                        item_id: item_id.clone(),
                    },
                );
                AiStreamEvent::ResponseItemStarted {
                    item_id,
                    item: ContentItem::Text {
                        text: block
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    },
                }
            }
            "thinking" => {
                self.blocks.insert(
                    index,
                    AnthropicBlock::Reasoning {
                        item_id: item_id.clone(),
                        signature: block
                            .get("signature")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    },
                );
                AiStreamEvent::ReasoningStarted { item_id }
            }
            "tool_use" => {
                let call_id = required_string_from_map(block, "id")?;
                let upstream_tool_name = required_string_from_map(block, "name")?;
                let tool_name = self
                    .tool_names
                    .get(upstream_tool_name.as_str())
                    .cloned()
                    .unwrap_or(upstream_tool_name);
                self.blocks.insert(
                    index,
                    AnthropicBlock::Tool {
                        item_id: item_id.clone(),
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                        arguments: String::new(),
                    },
                );
                AiStreamEvent::ToolCallStarted {
                    item_id,
                    call_id,
                    tool_name,
                    index,
                }
            }
            "server_tool_use" | "web_search_tool_result" => {
                self.blocks.insert(index, AnthropicBlock::Ignored);
                return Ok(Vec::new());
            }
            other => {
                return Err(ProtocolConversionError::new(format!(
                    "Anthropic content block type `{other}` is not supported."
                )));
            }
        };
        Ok(vec![event])
    }

    /// Decodes one Anthropic content-block delta.
    /// 解码一个 Anthropic 内容块增量。
    fn decode_block_delta(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index = required_u32(value, "index")?;
        let delta = value
            .get("delta")
            .and_then(Value::as_object)
            .ok_or_else(|| ProtocolConversionError::new("Anthropic content delta is missing."))?;
        let delta_type = delta
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| ProtocolConversionError::new("Anthropic delta type is missing."))?;
        let block = self.blocks.get_mut(&index).ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "Anthropic content delta references unopened index `{index}`."
            ))
        })?;
        match (block, delta_type) {
            (AnthropicBlock::Text { item_id }, "text_delta") => {
                let text = required_string_from_map(delta, "text")?;
                Ok(vec![AiStreamEvent::ResponseTextDelta {
                    item_id: item_id.clone(),
                    delta: text,
                }])
            }
            (AnthropicBlock::Reasoning { item_id, .. }, "thinking_delta") => {
                let thinking = required_string_from_map(delta, "thinking")?;
                Ok(vec![AiStreamEvent::ReasoningDelta {
                    item_id: item_id.clone(),
                    delta: thinking,
                }])
            }
            (AnthropicBlock::Reasoning { signature, .. }, "signature_delta") => {
                *signature = Some(required_string_from_map(delta, "signature")?);
                Ok(Vec::new())
            }
            (
                AnthropicBlock::Tool {
                    item_id,
                    call_id,
                    tool_name,
                    arguments,
                },
                "input_json_delta",
            ) => {
                let partial_json = required_string_from_map(delta, "partial_json")?;
                append_bounded_tool_arguments(arguments, partial_json.as_str(), "Anthropic")?;
                Ok(vec![AiStreamEvent::ToolCallArgumentsDelta {
                    item_id: item_id.clone(),
                    delta: partial_json,
                    progress: ToolCallProgress {
                        call_id: Some(call_id.clone()),
                        tool_name: Some(tool_name.clone()),
                        bytes_received: u64::try_from(arguments.len()).map_err(|_| {
                            ProtocolConversionError::new(
                                "Anthropic tool argument byte count overflowed.",
                            )
                        })?,
                        characters_received: u64::try_from(arguments.chars().count()).map_err(
                            |_| {
                                ProtocolConversionError::new(
                                    "Anthropic tool argument character count overflowed.",
                                )
                            },
                        )?,
                        complete: false,
                    },
                }])
            }
            (AnthropicBlock::Ignored, _) => Ok(Vec::new()),
            (_, other) => Err(ProtocolConversionError::new(format!(
                "Anthropic delta type `{other}` does not match content index `{index}`."
            ))),
        }
    }

    /// Completes one Anthropic content block.
    /// 完成一个 Anthropic 内容块。
    fn decode_block_stop(
        &mut self,
        value: &Value,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        let index = required_u32(value, "index")?;
        let block = self.blocks.remove(&index).ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "Anthropic content stop references unopened index `{index}`."
            ))
        })?;
        match block {
            AnthropicBlock::Text { item_id } => {
                Ok(vec![AiStreamEvent::ResponseItemCompleted { item_id }])
            }
            AnthropicBlock::Reasoning { item_id, signature } => {
                Ok(vec![AiStreamEvent::ReasoningCompleted {
                    item_id,
                    signature: signature.and_then(|signature| {
                        crate::wrap_provider_signature(
                            crate::SIGNATURE_PROVIDER_ANTHROPIC,
                            signature.as_str(),
                        )
                    }),
                }])
            }
            AnthropicBlock::Tool {
                item_id,
                call_id,
                tool_name,
                arguments,
            } => match serde_json::from_str::<Value>(arguments.as_str()) {
                Ok(arguments) => Ok(vec![AiStreamEvent::ToolCallCompleted {
                    item_id,
                    call: ToolCall {
                        id: call_id,
                        name: tool_name,
                        arguments,
                        signature: None,
                    },
                }]),
                Err(error) => Ok(vec![AiStreamEvent::ToolCallAborted {
                    item_id,
                    message: format!(
                        "Anthropic tool input ended before valid JSON was complete: {error}"
                    ),
                }]),
            },
            AnthropicBlock::Ignored => Ok(Vec::new()),
        }
    }
}

impl ProtocolStreamDecoder for AnthropicStreamDecoder {
    fn push_sse(
        &mut self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        // A gateway may append the transport-only OpenAI sentinel after Anthropic's semantic message_stop.
        // 网关可能在 Anthropic 语义 message_stop 之后追加仅用于传输收尾的 OpenAI 哨兵。
        if data.trim() == "[DONE]" {
            if self.terminal {
                return Ok(Vec::new());
            }
            return Err(ProtocolConversionError::new(
                "Anthropic stream received [DONE] before message_stop.",
            ));
        }
        if self.terminal {
            return Err(ProtocolConversionError::new(
                "Anthropic emitted data after a terminal event.",
            ));
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            ProtocolConversionError::new(format!("Anthropic SSE JSON is invalid: {error}"))
        })?;
        let wire_type = value
            .get("type")
            .and_then(Value::as_str)
            .or(event_name)
            .ok_or_else(|| ProtocolConversionError::new("Anthropic SSE event type is missing."))?;
        let mut events = self.start_if_needed();
        let mut decoded = match wire_type {
            "message_start" => {
                let mut message_events = self.capture_message_id(&value)?;
                message_events.extend(usage_from_anthropic(value.pointer("/message/usage")));
                message_events
            }
            "content_block_start" => self.decode_block_start(&value)?,
            "content_block_delta" => self.decode_block_delta(&value)?,
            "content_block_stop" => self.decode_block_stop(&value)?,
            "message_delta" => usage_from_anthropic(value.get("usage")),
            "message_stop" => {
                if !self.blocks.is_empty() {
                    return Err(ProtocolConversionError::new(
                        "Anthropic message stopped with incomplete content blocks.",
                    ));
                }
                self.terminal = true;
                vec![AiStreamEvent::ResponseCompleted]
            }
            "error" => {
                self.terminal = true;
                vec![AiStreamEvent::ResponseFailed {
                    error: anthropic_error(&value),
                }]
            }
            "ping" => Vec::new(),
            other => {
                return Err(ProtocolConversionError::new(format!(
                    "Anthropic SSE event type `{other}` is not registered."
                )));
            }
        };
        events.append(&mut decoded);
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.terminal {
            return Ok(Vec::new());
        }
        if !self.blocks.is_empty() {
            return Err(ProtocolConversionError::new(
                "Anthropic stream ended with incomplete content blocks.",
            ));
        }
        Err(ProtocolConversionError::new(
            "Anthropic stream ended before a terminal event.",
        ))
    }
}

/// Converts Anthropic usage fields while keeping missing fields unknown.
/// 转换 Anthropic 用量字段，并保持缺失字段为未知。
fn usage_from_anthropic(value: Option<&Value>) -> Vec<AiStreamEvent> {
    let Some(value) = value else {
        return Vec::new();
    };
    let input_tokens = reported_usage(value, "input_tokens");
    let cache_write_tokens = reported_usage(value, "cache_creation_input_tokens");
    let uncached_input_tokens = if matches!(cache_write_tokens, UsageValue::Unknown) {
        input_tokens
    } else {
        derive_usage_sum(input_tokens, cache_write_tokens)
    };
    let usage = RequestUsage {
        input_tokens,
        uncached_input_tokens,
        output_tokens: reported_usage(value, "output_tokens"),
        cache_read_tokens: reported_usage(value, "cache_read_input_tokens"),
        cache_write_tokens,
        cache_write_5m_tokens: reported_usage(value, "cache_creation_5m_input_tokens"),
        cache_write_1h_tokens: reported_usage(value, "cache_creation_1h_input_tokens"),
        ..RequestUsage::default()
    };
    vec![AiStreamEvent::UsageUpdated {
        usage: Box::new(usage),
    }]
}

/// Converts one reported unsigned usage field.
/// 转换一个已报告的无符号用量字段。
fn reported_usage(value: &Value, field: &str) -> UsageValue {
    value
        .get(field)
        .and_then(Value::as_u64)
        .map(|value| UsageValue::Known {
            value,
            source: UsageValueSource::Reported,
        })
        .unwrap_or(UsageValue::Unknown)
}

/// Normalizes an Anthropic stream error.
/// 归一化 Anthropic 流错误。
fn anthropic_error(value: &Value) -> AiExecutionError {
    let error_type = value
        .pointer("/error/type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let category = match error_type.as_deref() {
        Some("authentication_error") => AiErrorCategory::Authentication,
        Some("permission_error") => AiErrorCategory::PermissionDenied,
        Some("rate_limit_error") => AiErrorCategory::RateLimited,
        Some("overloaded_error") => AiErrorCategory::ProviderOverloaded,
        Some("invalid_request_error") => AiErrorCategory::InvalidRequest,
        _ => AiErrorCategory::Unclassified,
    };
    let upstream_message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(str::to_string);
    AiExecutionError {
        category,
        subcategory: error_type.clone(),
        stage: String::from("stream"),
        http_status: None,
        transport: None,
        websocket_close_code: None,
        upstream_code: error_type.clone(),
        upstream_type: error_type,
        upstream_parameter: value
            .pointer("/error/param")
            .and_then(Value::as_str)
            .map(str::to_string),
        upstream_message: upstream_message.clone(),
        request_id: value
            .get("request_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        provider_id: None,
        entry_id: None,
        model_id: None,
        message: upstream_message
            .as_deref()
            .unwrap_or("Anthropic stream failed.")
            .to_string(),
        dispatch_outcome: DispatchOutcome::Dispatched,
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: matches!(
            category,
            AiErrorCategory::RateLimited
                | AiErrorCategory::ProviderOverloaded
                | AiErrorCategory::ProviderUnavailable
                | AiErrorCategory::Timeout
        ),
        retry_after: None,
    }
}

/// Reads a required u32 field.
/// 读取一个必需的 u32 字段。
fn required_u32(value: &Value, field: &str) -> Result<u32, ProtocolConversionError> {
    let number = value.get(field).and_then(Value::as_u64).ok_or_else(|| {
        ProtocolConversionError::new(format!("Anthropic field `{field}` is missing."))
    })?;
    u32::try_from(number).map_err(|_| {
        ProtocolConversionError::new(format!("Anthropic field `{field}` exceeds u32."))
    })
}

/// Reads a required string from one JSON object map.
/// 从一个 JSON 对象映射读取必需字符串。
fn required_string_from_map(
    value: &Map<String, Value>,
    field: &str,
) -> Result<String, ProtocolConversionError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            ProtocolConversionError::new(format!("Anthropic field `{field}` is missing."))
        })
}

/// Encodes public content items into Anthropic blocks.
/// 将公共内容项编码为 Anthropic 内容块。
fn encode_content_items(
    items: &[ContentItem],
    allow_video: bool,
) -> Result<Vec<Value>, ProtocolConversionError> {
    items
        .iter()
        .map(|item| match item {
            ContentItem::Text { text } => Ok(json!({ "type": "text", "text": text })),
            ContentItem::Reasoning { text, signature } => {
                let Some(signature) = signature.as_deref().and_then(|signature| {
                    crate::compatible_provider_signature(
                        crate::SIGNATURE_PROVIDER_ANTHROPIC,
                        signature,
                    )
                }) else {
                    return Ok(Value::Null);
                };
                let mut block = Map::from_iter([
                    (
                        String::from("type"),
                        Value::String(String::from("thinking")),
                    ),
                    (String::from("thinking"), Value::String(text.clone())),
                ]);
                block.insert(
                    String::from("signature"),
                    Value::String(signature.to_string()),
                );
                Ok(Value::Object(block))
            }
            ContentItem::ToolResult { result } => Ok(json!({
                "type": "tool_result",
                "tool_use_id": result.call_id,
                "is_error": result.is_error,
                "content": result.content,
            })),
            ContentItem::ToolCall { call } => Ok(json!({
                "type": "tool_use",
                "id": call.id,
                "name": call.name,
                "input": call.arguments,
            })),
            ContentItem::Refusal { text } => Ok(json!({ "type": "text", "text": text })),
            ContentItem::StructuredJson { value } => Ok(json!({
                "type": "text",
                "text": serde_json::to_string(value).map_err(|error| {
                    ProtocolConversionError::new(format!(
                        "Structured JSON could not be encoded for Anthropic: {error}"
                    ))
                })?,
            })),
            ContentItem::Citation { .. } => Err(ProtocolConversionError::new(
                "Anthropic citation projection is not registered for request input.",
            )),
            ContentItem::Resource { resource }
                if resource.kind == vulcan_ai_contract::ResourceKind::Image =>
            {
                encode_anthropic_image(resource)
            }
            ContentItem::Resource { resource }
                if allow_video && resource.kind == vulcan_ai_contract::ResourceKind::Video =>
            {
                encode_anthropic_video(resource)
            }
            ContentItem::Resource { resource } => Err(ProtocolConversionError::new(format!(
                "Anthropic profile has no registered {:?} input projector.",
                resource.kind
            ))),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|blocks| {
            blocks
                .into_iter()
                .filter(|block| !block.is_null())
                .collect()
        })
}

/// Encodes one host-resolved image as an Anthropic image content block.
/// 将一个宿主已解析图片编码为 Anthropic 图片内容块。
///
/// # Parameters
/// 参数
/// `resource` contains either a remote URL or a Base64 data URL.
/// `resource` 包含远程 URL 或 Base64 Data URL。
///
/// # Returns
/// 返回值
/// Returns an Anthropic image block or a controlled conversion error.
/// 返回 Anthropic 图片块或受控转换错误。
pub(crate) fn encode_anthropic_image(
    resource: &vulcan_ai_contract::ResourceReference,
) -> Result<Value, ProtocolConversionError> {
    if resource.image_detail.is_some() {
        return Err(ProtocolConversionError::new(format!(
            "Anthropic Messages cannot preserve image detail for resource `{}`.",
            resource.id
        )));
    }
    let url = resource.url.as_deref().ok_or_else(|| {
        ProtocolConversionError::new(format!(
            "Resource `{}` was not resolved before Anthropic projection.",
            resource.id
        ))
    })?;
    if !url.starts_with("data:") {
        return Ok(json!({
            "type": "image",
            "source": {
                "type": "url",
                "url": url,
            },
        }));
    }
    let body = url
        .strip_prefix("data:")
        .ok_or_else(|| ProtocolConversionError::new("Anthropic image data URL is malformed."))?;
    let (metadata, data) = body.split_once(',').ok_or_else(|| {
        ProtocolConversionError::new("Anthropic image data URL is missing its payload separator.")
    })?;
    let media_type = metadata.strip_suffix(";base64").ok_or_else(|| {
        ProtocolConversionError::new("Anthropic image data URL must use Base64 encoding.")
    })?;
    if !matches!(
        media_type,
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    ) {
        return Err(ProtocolConversionError::new(format!(
            "Anthropic image MIME type `{media_type}` is not registered."
        )));
    }
    Ok(json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": media_type,
            "data": data,
        },
    }))
}

/// Encodes one host-validated video as an Anthropic-compatible video content block.
/// 将一个经宿主校验的视频编码为 Anthropic 兼容视频内容块。
///
/// # Parameters
/// 参数
/// `resource` contains either a provider-supported remote URL or a Base64 data URL.
/// `resource` 包含供应商支持的远程 URL 或 Base64 Data URL。
///
/// # Returns
/// 返回值
/// Returns a video block or a controlled MIME/data-URL conversion error.
/// 返回视频内容块，或受控的 MIME/Data URL 转换错误。
pub(crate) fn encode_anthropic_video(
    resource: &vulcan_ai_contract::ResourceReference,
) -> Result<Value, ProtocolConversionError> {
    let url = resource.url.as_deref().ok_or_else(|| {
        ProtocolConversionError::new(format!(
            "Resource `{}` was not resolved before Anthropic video projection.",
            resource.id
        ))
    })?;
    if !url.starts_with("data:") {
        return Ok(json!({
            "type": "video",
            "source": {
                "type": "url",
                "url": url,
            },
        }));
    }
    let body = url
        .strip_prefix("data:")
        .ok_or_else(|| ProtocolConversionError::new("Anthropic video data URL is malformed."))?;
    let (metadata, data) = body.split_once(',').ok_or_else(|| {
        ProtocolConversionError::new("Anthropic video data URL is missing its payload separator.")
    })?;
    let media_type = metadata.strip_suffix(";base64").ok_or_else(|| {
        ProtocolConversionError::new("Anthropic video data URL must use Base64 encoding.")
    })?;
    if !matches!(
        media_type,
        "video/mp4" | "video/x-msvideo" | "video/quicktime" | "video/mov" | "video/x-matroska"
    ) {
        return Err(ProtocolConversionError::new(format!(
            "Anthropic-compatible video MIME type `{media_type}` is not registered."
        )));
    }
    Ok(json!({
        "type": "video",
        "source": {
            "type": "base64",
            "media_type": media_type,
            "data": data,
        },
    }))
}

/// Projects the public prompt-cache policy into Anthropic request fields.
/// 将公共提示词缓存策略投影为 Anthropic 请求字段。
fn encode_anthropic_cache_policy(
    body: &mut Map<String, Value>,
    policy: &PromptCachePolicy,
) -> Result<(), ProtocolConversionError> {
    match policy {
        PromptCachePolicy::ProviderDefault => Ok(()),
        PromptCachePolicy::ManagedExplicit { ttl } => {
            let cache_control = match ttl {
                PromptCacheTtlSelection::ProfileDefault => json!({ "type": "ephemeral" }),
                PromptCacheTtlSelection::CapabilityOption { option_id } if option_id == "5m" => {
                    json!({ "type": "ephemeral", "ttl": "5m" })
                }
                PromptCacheTtlSelection::CapabilityOption { option_id } if option_id == "1h" => {
                    json!({ "type": "ephemeral", "ttl": "1h" })
                }
                PromptCacheTtlSelection::CapabilityOption { option_id } => {
                    return Err(ProtocolConversionError::new(format!(
                        "Anthropic prompt-cache TTL option `{option_id}` is not registered."
                    )));
                }
            };
            let mut breakpoints = 0_u32;
            if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut)
                && let Some(tool) = tools.last_mut().and_then(Value::as_object_mut)
            {
                tool.insert(String::from("cache_control"), cache_control.clone());
                breakpoints += 1;
            }
            if let Some(system) = body.get_mut("system").and_then(Value::as_array_mut)
                && let Some(block) = system.last_mut().and_then(Value::as_object_mut)
            {
                block.insert(String::from("cache_control"), cache_control.clone());
                breakpoints += 1;
            }
            if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut)
                && let Some(content) = messages
                    .last_mut()
                    .and_then(Value::as_object_mut)
                    .and_then(|message| message.get_mut("content"))
                    .and_then(Value::as_array_mut)
                && let Some(block) = content.last_mut().and_then(Value::as_object_mut)
            {
                block.insert(String::from("cache_control"), cache_control);
                breakpoints += 1;
            }
            if breakpoints == 0 {
                return Err(ProtocolConversionError::new(
                    "Anthropic managed caching requires at least one cacheable tool, system, or message block.",
                ));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::{AiOperation, PromptCacheTtlSelection};

    /// Builds one minimal public request.
    /// 构建一个最小公共请求。
    fn request(policy: PromptCachePolicy) -> AiExecutionRequest {
        AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("model"),
            cache_scope_id: None,
            continuation: None,
            messages: vec![vulcan_ai_contract::AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("hello"),
                }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: policy,
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: Default::default(),
            resources: Vec::new(),
            output_token_limit: Some(512),
        }
    }

    /// Verifies provider-default caching injects no cache field.
    /// 验证供应商默认缓存不会注入缓存字段。
    #[test]
    fn provider_default_omits_cache_control() {
        let encoded = AnthropicMessagesProfile
            .encode(&request(PromptCachePolicy::ProviderDefault))
            .expect("request should encode");
        assert!(encoded.body.get("cache_control").is_none());
        assert!(encoded.body.get("tool_choice").is_none());
    }

    /// Verifies schema-free JSON-object requests are rejected before dispatch.
    /// 验证无 Schema 的 JSON 对象请求会在分派前被拒绝。
    #[test]
    fn json_object_requires_explicit_schema() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.structured_output = StructuredOutputPolicy::JsonObject;
        let error = AnthropicMessagesProfile
            .encode(&public_request)
            .expect_err("schema-free JSON object request must be rejected");
        assert!(error.message.contains("explicit JSON Schema"));
    }

    /// Verifies explicit JSON Schema requests use Anthropic native structured output.
    /// 验证显式 JSON Schema 请求会使用 Anthropic 原生结构化输出。
    #[test]
    fn json_schema_uses_native_output_format() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.structured_output = StructuredOutputPolicy::JsonSchema {
            name: String::from("result"),
            description: Some(String::from("Structured result")),
            schema: json!({
                "type": "object",
                "properties": { "accepted": { "type": "boolean" } },
                "required": ["accepted"],
                "additionalProperties": false,
            }),
            strict: true,
        };
        let encoded = AnthropicMessagesProfile
            .encode(&public_request)
            .expect("explicit JSON Schema request should encode");
        assert_eq!(
            encoded.body["output_config"]["format"]["type"],
            "json_schema"
        );
        assert_eq!(
            encoded.body["output_config"]["format"]["schema"]["required"][0],
            "accepted"
        );
    }

    /// Verifies hidden reasoning is accepted as a consumer-side visibility policy.
    /// 验证隐藏思考可作为消费侧可见性策略被接受。
    #[test]
    fn hidden_reasoning_output_is_non_blocking() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.reasoning_output = ReasoningOutputPolicy::Hidden;
        AnthropicMessagesProfile
            .encode(&public_request)
            .expect("hidden reasoning output should not block Anthropic encoding");
    }

    /// Verifies Anthropic web search is encoded as a versioned server tool.
    /// 验证 Anthropic 网络搜索会编码为带版本的服务端工具。
    #[test]
    fn native_web_search_uses_versioned_server_tool() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.native_tools = vec![NativeToolKind::WebSearch];
        let encoded = AnthropicMessagesProfile
            .encode(&public_request)
            .expect("Anthropic web search should encode");
        assert_eq!(encoded.body["tools"][0]["type"], "web_search_20250305");
        assert_eq!(encoded.body["tools"][0]["name"], "web_search");
    }

    /// Verifies Anthropic rejects a native tool without an exact server-tool projection.
    /// 验证 Anthropic 会拒绝没有精确服务端工具投影的原生工具。
    #[test]
    fn unsupported_native_tool_is_rejected() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.native_tools = vec![NativeToolKind::CodeInterpreter];
        let error = AnthropicMessagesProfile
            .encode(&public_request)
            .expect_err("unsupported Anthropic native tool should fail");
        assert!(error.to_string().contains("CodeInterpreter"));
    }

    /// Verifies one-hour explicit caching uses the registered TTL.
    /// 验证一小时显式缓存使用已注册 TTL。
    #[test]
    fn explicit_one_hour_cache_is_encoded() {
        let encoded = AnthropicMessagesProfile
            .encode(&request(PromptCachePolicy::ManagedExplicit {
                ttl: PromptCacheTtlSelection::CapabilityOption {
                    option_id: String::from("1h"),
                },
            }))
            .expect("request should encode");
        assert_eq!(
            encoded.body["messages"][0]["content"][0]["cache_control"]["ttl"],
            "1h"
        );
    }

    /// Verifies adjacent tool results share one Anthropic user message.
    /// 验证相邻工具结果会共用一条 Anthropic 用户消息。
    #[test]
    fn adjacent_tool_results_are_grouped_into_one_user_message() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.messages.extend([
            vulcan_ai_contract::AiMessage {
                role: MessageRole::Assistant,
                content: vec![
                    ContentItem::ToolCall {
                        call: vulcan_ai_contract::ToolCall {
                            id: String::from("call-a"),
                            name: String::from("read"),
                            arguments: json!({"path": "a"}),
                            signature: None,
                        },
                    },
                    ContentItem::ToolCall {
                        call: vulcan_ai_contract::ToolCall {
                            id: String::from("call-b"),
                            name: String::from("read"),
                            arguments: json!({"path": "b"}),
                            signature: None,
                        },
                    },
                ],
                provider_context_items: Vec::new(),
            },
            vulcan_ai_contract::AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: vulcan_ai_contract::ToolResult {
                        call_id: String::from("call-a"),
                        content: json!("a"),
                        is_error: false,
                    },
                }],
                provider_context_items: Vec::new(),
            },
            vulcan_ai_contract::AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: vulcan_ai_contract::ToolResult {
                        call_id: String::from("call-b"),
                        content: json!("b"),
                        is_error: false,
                    },
                }],
                provider_context_items: Vec::new(),
            },
        ]);

        let encoded = AnthropicMessagesProfile
            .encode(&public_request)
            .expect("adjacent tool results should encode");
        let messages = encoded.body["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(
            messages[2]["content"]
                .as_array()
                .expect("tool blocks should be an array")
                .len(),
            2
        );
    }

    /// Verifies a host-resolved MiniMax-compatible video becomes one Anthropic video block.
    /// 验证宿主已解析的 MiniMax 兼容视频会成为一个 Anthropic 视频内容块。
    #[test]
    fn base64_video_resource_is_encoded() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.messages[0].content = vec![ContentItem::Resource {
            resource: vulcan_ai_contract::ResourceReference {
                id: String::from("video"),
                kind: vulcan_ai_contract::ResourceKind::Video,
                media_type: Some(String::from("video/mp4")),
                source_kind: vulcan_ai_contract::ResourceSourceKind::InlineData,
                file_name: None,
                image_detail: None,
                resource_path: None,
                local_path: None,
                url: Some(String::from("data:video/mp4;base64,AA==")),
            },
        }];
        let encoded = AnthropicMessagesVideoProfile
            .encode(&public_request)
            .expect("registered video should encode");
        assert_eq!(encoded.body["messages"][0]["content"][0]["type"], "video");
        assert_eq!(
            encoded.body["messages"][0]["content"][0]["source"]["media_type"],
            "video/mp4"
        );
    }

    /// Verifies a MiniMax uploaded-file URI becomes the documented Anthropic-compatible URL source.
    /// 验证 MiniMax 上传文件 URI 会成为文档化的 Anthropic 兼容 URL 来源。
    #[test]
    fn uploaded_video_resource_is_encoded_as_url_source() {
        // public_request carries the provider-native reference produced by the Files API.
        // public_request 携带 Files API 产生的供应商原生引用。
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.messages[0].content = vec![ContentItem::Resource {
            resource: vulcan_ai_contract::ResourceReference {
                id: String::from("video"),
                kind: vulcan_ai_contract::ResourceKind::Video,
                media_type: Some(String::from("video/mp4")),
                source_kind: vulcan_ai_contract::ResourceSourceKind::HostResolved,
                file_name: Some(String::from("clip.mp4")),
                image_detail: None,
                resource_path: None,
                local_path: None,
                url: Some(String::from("mm_file://file-minimax")),
            },
        }];

        // encoded is the exact Anthropic-compatible request body for MiniMax M3.
        // encoded 是 MiniMax M3 的精确 Anthropic 兼容请求正文。
        let encoded = AnthropicMessagesVideoProfile
            .encode(&public_request)
            .expect("uploaded video URI should encode");
        assert_eq!(
            encoded.body["messages"][0]["content"][0],
            json!({
                "type": "video",
                "source": {
                    "type": "url",
                    "url": "mm_file://file-minimax",
                },
            })
        );
    }

    /// Verifies the standard Anthropic profile rejects the compatible-only video extension.
    /// 验证标准 Anthropic Profile 会拒绝仅兼容协议支持的视频扩展。
    #[test]
    fn standard_profile_rejects_video_extension() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.messages[0].content = vec![ContentItem::Resource {
            resource: vulcan_ai_contract::ResourceReference {
                id: String::from("video"),
                kind: vulcan_ai_contract::ResourceKind::Video,
                media_type: Some(String::from("video/mp4")),
                source_kind: vulcan_ai_contract::ResourceSourceKind::InlineData,
                file_name: None,
                image_detail: None,
                resource_path: None,
                local_path: None,
                url: Some(String::from("data:video/mp4;base64,AA==")),
            },
        }];
        assert!(AnthropicMessagesProfile.encode(&public_request).is_err());
    }

    /// Verifies Claude Code rewrites only actually declared lowercase names and restores responses.
    /// 验证 Claude Code 仅改写实际声明的小写名称并恢复响应名称。
    #[test]
    fn claude_code_tool_names_are_request_scoped_and_reversible() {
        let mut public_request = request(PromptCachePolicy::ProviderDefault);
        public_request.tools = vec![
            vulcan_ai_contract::ToolDefinition {
                name: String::from("skill"),
                description: String::from("Loads one skill."),
                input_schema: json!({"type": "object"}),
                input_format: Default::default(),
                strict: false,
            },
            vulcan_ai_contract::ToolDefinition {
                name: String::from("Bash"),
                description: String::from("Already title-cased."),
                input_schema: json!({"type": "object"}),
                input_format: Default::default(),
                strict: false,
            },
        ];
        let encoded = ClaudeCodeMessagesProfile
            .encode(&public_request)
            .expect("Claude Code request should encode");
        assert_eq!(encoded.body["tools"][0]["name"], "Skill");
        assert_eq!(encoded.body["tools"][1]["name"], "Bash");

        let mut decoder = ClaudeCodeMessagesProfile
            .create_stream_decoder(String::from("execution"), &public_request);
        let started = decoder
            .push_sse(
                Some("content_block_start"),
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call","name":"Skill","input":{}}}"#,
            )
            .expect("Claude Code tool should start");
        assert!(started.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallStarted { tool_name, .. } if tool_name == "skill"
        )));
    }

    /// Verifies Anthropic publishes the immutable upstream message identifier.
    /// 验证 Anthropic 会发布不可变的上游消息标识。
    #[test]
    fn message_start_publishes_message_identifier() {
        let mut decoder = AnthropicStreamDecoder::new(String::from("execution"), BTreeMap::new());
        let events = decoder
            .push_sse(
                Some("message_start"),
                r#"{"type":"message_start","message":{"id":"msg_123","usage":{}}}"#,
            )
            .expect("message identity should decode");
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseMetadataUpdated { continuation }
                if continuation.provider_message_id == "msg_123"
        )));
    }

    /// Verifies fragmented Anthropic tool JSON stays incremental until block completion.
    /// 验证分片 Anthropic 工具 JSON 在内容块完成前始终保持增量状态。
    #[test]
    fn fragmented_tool_json_completes_only_at_block_stop() {
        let mut decoder = AnthropicStreamDecoder::new(String::from("execution"), BTreeMap::new());
        let started = decoder
            .push_sse(
                Some("content_block_start"),
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call","name":"edit","input":{}}}"#,
            )
            .expect("tool should start");
        assert!(started.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallStarted { tool_name, .. } if tool_name == "edit"
        )));

        for fragment in [r#"{"text":""#, r#"hello"}"#] {
            let event = decoder
                .push_sse(
                    Some("content_block_delta"),
                    json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": fragment,
                        },
                    })
                    .to_string()
                    .as_str(),
                )
                .expect("fragment should remain buffered");
            assert!(event.iter().any(|event| matches!(
                event,
                AiStreamEvent::ToolCallArgumentsDelta { progress, .. }
                    if !progress.complete
            )));
            assert!(
                !event
                    .iter()
                    .any(|event| matches!(event, AiStreamEvent::ToolCallCompleted { .. }))
            );
        }

        let completed = decoder
            .push_sse(
                Some("content_block_stop"),
                r#"{"type":"content_block_stop","index":0}"#,
            )
            .expect("complete JSON should finish");
        assert!(completed.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallCompleted { call, .. }
                if call.arguments == json!({"text": "hello"})
        )));
    }

    /// Verifies truncated Anthropic tool JSON becomes a controlled aborted event.
    /// 验证截断的 Anthropic 工具 JSON 转换为受控的工具中止事件。
    #[test]
    fn truncated_tool_json_is_aborted_at_block_stop() {
        let mut decoder = AnthropicStreamDecoder::new(String::from("execution"), BTreeMap::new());
        decoder
            .push_sse(
                Some("content_block_start"),
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call","name":"edit","input":{}}}"#,
            )
            .expect("tool should start");
        decoder
            .push_sse(
                Some("content_block_delta"),
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"text\":\""}}"#,
            )
            .expect("partial JSON should buffer");
        let stopped = decoder
            .push_sse(
                Some("content_block_stop"),
                r#"{"type":"content_block_stop","index":0}"#,
            )
            .expect("invalid JSON should normalize");
        assert!(stopped.iter().any(|event| matches!(
            event,
            AiStreamEvent::ToolCallAborted { message, .. }
                if message.contains("before valid JSON was complete")
        )));
    }

    /// Verifies missing cache usage stays unknown while reported zero remains known.
    /// 验证缺失的缓存用量保持未知，而明确上报的零保持已知。
    #[test]
    fn anthropic_usage_distinguishes_unknown_from_reported_zero() {
        let events = usage_from_anthropic(Some(&json!({
            "input_tokens": 7,
            "cache_read_input_tokens": 0
        })));
        let usage = events
            .iter()
            .find_map(|event| match event {
                AiStreamEvent::UsageUpdated { usage } => Some(usage),
                _ => None,
            })
            .expect("usage event should exist");
        assert_eq!(
            usage.cache_read_tokens,
            UsageValue::Known {
                value: 0,
                source: UsageValueSource::Reported,
            }
        );
        assert_eq!(usage.cache_write_tokens, UsageValue::Unknown);
    }

    /// Verifies a gateway transport sentinel after message_stop does not invalidate completed output.
    /// 验证 message_stop 后的网关传输哨兵不会使已完成输出失效。
    #[test]
    fn gateway_done_after_message_stop_is_ignored() {
        let mut decoder = AnthropicStreamDecoder::new(String::from("execution"), BTreeMap::new());
        let completed = decoder
            .push_sse(Some("message_stop"), r#"{"type":"message_stop"}"#)
            .expect("message_stop should complete the stream");
        assert!(
            completed
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
        );

        let sentinel = decoder
            .push_sse(Some("done"), "[DONE]")
            .expect("the transport sentinel should be ignored after message_stop");
        assert!(sentinel.is_empty());
        assert!(
            decoder
                .finish()
                .expect("the stream should remain valid")
                .is_empty()
        );

        let mut premature = AnthropicStreamDecoder::new(String::from("premature"), BTreeMap::new());
        assert!(premature.push_sse(Some("done"), "[DONE]").is_err());
    }

    /// Verifies DeepSeek server-side search blocks do not enter the host tool loop.
    /// 验证 DeepSeek 服务端搜索内容块不会进入宿主工具循环。
    #[test]
    fn server_side_search_blocks_are_consumed_without_host_tool_events() {
        let mut decoder = AnthropicStreamDecoder::new(String::from("execution"), BTreeMap::new());
        let started = decoder
            .push_sse(
                Some("content_block_start"),
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"search_1","name":"web_search","input":{}}}"#,
            )
            .expect("server search block should decode");
        assert!(
            started
                .iter()
                .all(|event| !matches!(event, AiStreamEvent::ToolCallStarted { .. }))
        );
        decoder
            .push_sse(
                Some("content_block_delta"),
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"query\":\"DeepSeek\"}"}}"#,
            )
            .expect("server search arguments should be ignored");
        decoder
            .push_sse(
                Some("content_block_stop"),
                r#"{"type":"content_block_stop","index":0}"#,
            )
            .expect("server search block should stop");
        decoder
            .push_sse(
                Some("content_block_start"),
                r#"{"type":"content_block_start","index":1,"content_block":{"type":"web_search_tool_result","tool_use_id":"search_1","content":[]}}"#,
            )
            .expect("search result block should decode");
        decoder
            .push_sse(
                Some("content_block_stop"),
                r#"{"type":"content_block_stop","index":1}"#,
            )
            .expect("search result block should stop");
    }
}
