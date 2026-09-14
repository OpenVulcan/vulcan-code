//! Shared ordered request envelope with a distinct, non-serializable internal media representation.
//! 带有独立且不可序列化内部媒体表示的共享有序请求封装。

use super::*;
use vulcan_ai_contract::ResourceReference;

/// Private input-part tag emitted only from a typed internal resource.
/// 仅从强类型内部资源生成的私有输入项标签。
pub const INTERNAL_RESOURCE_PART_TYPE: &str = "vulcan_resource";

/// Resource encoder selected explicitly by a standard or internal entry point.
/// 由标准或内部入口显式选择的资源编码器。
pub(super) type ResponsesResourceEncoder<'a> =
    &'a mut dyn FnMut(&ResourceReference) -> Result<Value, ProtocolConversionError>;

/// Internal request envelope that cannot be passed directly to an HTTP or WebSocket transport.
/// 无法直接传给 HTTP 或 WebSocket 传输的内部请求封装。
/// It deliberately implements neither Serialize nor Deserialize; public APIs retain their own schema.
/// 有意不实现 Serialize 或 Deserialize；公开 API 保持自身结构契约。
///
/// A transport cannot accept the internal owner before target projection.
/// 目标投影之前，传输不能接受内部所有者。
/// ```compile_fail,E0308
/// use vulcan_llm_core::{EncodedWireRequest, InternalResponsesRequest};
/// // Only wire requests may reach this transport boundary.
/// // 只有线路请求可到达此传输边界。
/// fn send(_request: EncodedWireRequest) {}
/// // An internal request must first pass the selected target projector.
/// // 内部请求必须先经过选中目标投影器。
/// fn invalid(request: InternalResponsesRequest) { send(request); }
/// ```
///
/// The internal resource owner cannot be serialized as a public request.
/// 内部资源所有者不能序列化为公共请求。
/// ```compile_fail,E0277
/// use vulcan_llm_core::InternalResponsesRequest;
/// // Serializing the internal envelope would bypass target media validation.
/// // 序列化内部封装会绕过目标媒体校验。
/// fn invalid(request: InternalResponsesRequest) { let _ = serde_json::to_value(request); }
/// ```
#[derive(Debug, Clone)]
pub struct InternalResponsesRequest {
    /// Ordered semantic body containing private resource indices, never a ready-to-send wire body.
    /// 包含私有资源索引的有序语义正文，不是可直接发送的线路正文。
    body: Value,
    /// Request-local typed resources; binary references never round-trip through internal JSON.
    /// 请求局部强类型资源；二进制引用不会在内部 JSON 中来回序列化。
    resources: Vec<ResourceReference>,
    /// Existing baseline transport identity, retained until the exact target projection is chosen.
    /// 既有基线传输身份，保留到选定精确目标投影为止。
    headers: std::collections::BTreeMap<String, String>,
    /// Assistant messages captured before the generic encoder could drop any of their content.
    /// 在通用编码器可能丢弃其内容之前捕获的助手消息。
    replay_sidecar: vulcan_ai_contract::ReplaySidecar,
}

impl InternalResponsesRequest {
    /// Exposes the internal body for existing trusted replay and token-limit projection.
    /// 公开内部正文，供既有可信重放与 Token 限制投影使用。
    /// Returns a mutable reference, not a serialized request or a transport permit.
    /// 返回可变引用，不返回序列化请求或传输许可。
    pub fn body_mut(&mut self) -> &mut Value {
        &mut self.body
    }

    /// Returns the assistant replay sidecar this internal request captured.
    /// 返回本内部请求捕获的助手重放侧车。
    ///
    /// # Returns
    /// 返回值
    /// The non-serializable sidecar; only the exact target converter of this request may read it.
    /// 不可序列化侧车；只有本次请求的精确目标转换器可以读取它。
    pub fn replay_sidecar(&self) -> &vulcan_ai_contract::ReplaySidecar {
        &self.replay_sidecar
    }

    /// Consumes the owner and returns its semantic body, baseline identity, typed resources and
    /// assistant replay sidecar.
    /// 消费所有者，返回语义正文、基线身份、强类型资源表与助手重放侧车。
    pub(crate) fn into_parts(
        self,
    ) -> (
        Value,
        std::collections::BTreeMap<String, String>,
        Vec<ResourceReference>,
        vulcan_ai_contract::ReplaySidecar,
    ) {
        (self.body, self.headers, self.resources, self.replay_sidecar)
    }
}

/// Encodes typed `request` without constraining media to the standard Responses input union.
/// 编码强类型 `request`，不将媒体限制为标准 Responses 输入联合类型。
/// Returns a non-wire internal owner, or an existing role, tool, cache or continuation error.
/// 返回非线路内部所有者，或既有角色、工具、缓存及延续错误。
pub fn encode_internal_responses_request(
    request: &AiExecutionRequest,
) -> Result<InternalResponsesRequest, ProtocolConversionError> {
    // The resource table belongs only to this immutable request, not to shared history or upload caches.
    // 资源表仅属于此不可变请求，不属于共享历史或上传缓存。
    let mut resources = Vec::new();
    // Index assignment follows canonical message order and retains the exact typed resource once.
    // 索引赋值遵循规范消息顺序，精确强类型资源仅保存一份。
    let mut resource_encoder = |resource: &ResourceReference| {
        // The index is derived from this owner's actual table length, never from external IDs.
        // 索引由此所有者实际表长度产生，绝不从外部 ID 推断。
        let resource_index = resources.len();
        resources.push(resource.clone());
        Ok(json!({ "type": INTERNAL_RESOURCE_PART_TYPE, "resource_index": resource_index }))
    };
    // Standard callers still select their strict resource encoder at this shared envelope boundary.
    // 标准调用者仍在此共享封装边界选择自身严格资源编码器。
    let (mut encoded, input_ranges) = encode_request_envelope(request, &mut resource_encoder)?;
    if let Some(limit) = request.output_token_limit {
        // Internal semantics retain the limit even when a particular wire target omits it.
        // 即使某个线路目标省略该限制，内部语义仍然保留它。
        encoded.body["max_output_tokens"] = json!(limit);
    }
    // The sidecar is captured here, before any target converter can drop a message's content.
    // 侧车在此处捕获，先于任何目标转换器可能丢弃消息内容之前。
    let replay_sidecar = request_replay_sidecar(request, &input_ranges);
    Ok(InternalResponsesRequest {
        body: encoded.body,
        headers: encoded.headers,
        resources,
        replay_sidecar,
    })
}

/// Captures one sidecar entry per message this envelope actually encoded.
/// 为本次封装实际编码的每条消息捕获一条侧车条目。
///
/// # Parameters
/// 参数
/// `request` owns the canonical messages and `input_ranges` attributes each encoded message to the
/// internal input items it produced.
/// `request` 持有规范消息，`input_ranges` 把每条已编码消息归属到它所产生的内部输入项。
///
/// # Returns
/// 返回值
/// Every encoded message in original order, including one that produced no input item.
/// 按原始顺序排列的每条已编码消息，包含没有产生任何输入项的消息。
fn request_replay_sidecar(
    request: &AiExecutionRequest,
    input_ranges: &[ResponsesMessageInputRange],
) -> vulcan_ai_contract::ReplaySidecar {
    let messages = input_ranges
        .iter()
        .filter_map(|range| {
            let message = request.messages.get(range.message_index)?;
            Some(vulcan_ai_contract::ReplaySidecarMessage {
                message_index: range.message_index,
                role: message.role,
                content: message.content.clone(),
                input_range: range.input_range.clone(),
                tool_call_ids: message_tool_call_ids(message),
            })
        })
        .collect();
    vulcan_ai_contract::ReplaySidecar::new(messages)
}

/// Returns the provider-visible tool call identities declared by one exact message.
/// 返回一条精确消息声明的供应商可见工具调用身份。
fn message_tool_call_ids(message: &vulcan_ai_contract::AiMessage) -> Vec<String> {
    let mut call_ids = Vec::new();
    for item in &message.provider_context_items {
        // Only real tool call items own an identity; outputs and hosted items never add one here.
        // 只有真正的工具调用项持有身份；输出项与托管项在此绝不新增身份。
        if matches!(
            item.get("type").and_then(Value::as_str),
            Some("function_call" | "custom_tool_call")
        ) && let Some(call_id) = item.get("call_id").and_then(Value::as_str)
        {
            call_ids.push(call_id.to_string());
        }
    }
    for item in &message.content {
        if let vulcan_ai_contract::ContentItem::ToolCall { call } = item {
            call_ids.push(call.id.clone());
        }
    }
    call_ids
}

/// Builds the shared envelope for `request` using the explicitly chosen `resource_encoder`.
/// 使用显式选中的 `resource_encoder` 为 `request` 构建共享封装。
/// Returns the existing envelope fields together with the input range each encoded message owned.
/// 返回既有封装字段，以及每条已编码消息所拥有的输入区间。
/// Only the strict standard caller may send the envelope without adaptation.
/// 仅严格标准调用者可以不经适配发送该封装。
pub(super) fn encode_request_envelope(
    request: &AiExecutionRequest,
    resource_encoder: ResponsesResourceEncoder<'_>,
) -> Result<(EncodedWireRequest, Vec<ResponsesMessageInputRange>), ProtocolConversionError> {
    require_default_cache_policy(&request.prompt_cache, OPENAI_CODEX_RESPONSES_PROFILE_ID)?;
    // The cursor stays bound to the existing Codex continuation proof.
    // 游标继续绑定既有 Codex 延续证明。
    let continuation =
        validated_responses_continuation(request, OPENAI_CODEX_RESPONSES_PROFILE_ID)?;
    // Only resource encoding varies; role and item ordering use the shared implementation.
    // 仅资源编码发生变化；角色与项目顺序使用共用实现。
    let (input, input_ranges) =
        encode_responses_input_with_resources(request, false, continuation, resource_encoder)?;
    // System instructions retain the canonical ordered text projection.
    // 系统指令保持规范有序文本投影。
    let instructions = encode_codex_instructions(request)?;
    // Tool definitions keep the existing standard function and native-tool contract.
    // 工具定义保持既有标准函数与原生工具契约。
    let tools = encode_codex_responses_tools(request);
    // The neutral envelope preserves established reasoning, cache and continuation semantics.
    // 中立封装保持既有思考、缓存和延续语义。
    let mut body = json!({
        "model": request.model_id,
        "instructions": instructions,
        "input": input,
        "tool_choice": "auto",
        "parallel_tool_calls": !matches!(
            request.parallel_tool_calls,
            ParallelToolCallPolicy::Disabled
        ),
        "stream": true,
        "store": false,
        "include": ["reasoning.encrypted_content"],
    });
    if body["instructions"].as_str().is_some_and(str::is_empty) {
        body.as_object_mut()
            .expect("Codex Responses request body is an object")
            .remove("instructions");
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    if let Some(continuation) = continuation {
        body["previous_response_id"] = Value::String(continuation.provider_message_id.clone());
    }
    if let Some(effort) = request.reasoning_effort {
        body["reasoning"] = json!({
            "effort": effort.as_str(),
        });
        project_responses_reasoning_output(request.reasoning_output, &mut body["reasoning"])?;
    }
    project_responses_request_controls(request, &mut body)?;
    // Codex identity is retained as baseline metadata and replaced for other target transports.
    // Codex 身份作为基线元数据保留，其他目标传输会替换它。
    let mut headers = std::collections::BTreeMap::from([
        (
            String::from("originator"),
            String::from(CODEX_RESPONSES_ORIGINATOR),
        ),
        (
            String::from("user-agent"),
            String::from(CODEX_RESPONSES_USER_AGENT),
        ),
    ]);
    if let Some(cache_scope_id) = request.cache_scope_id.as_deref() {
        body["prompt_cache_key"] = Value::String(cache_scope_id.to_string());
        body["client_metadata"] = json!({
            "session_id": cache_scope_id,
            "thread_id": cache_scope_id,
        });
        headers.insert(
            String::from("x-client-request-id"),
            cache_scope_id.to_string(),
        );
        headers.insert(String::from("session-id"), cache_scope_id.to_string());
        // Root Vulcan task sessions map to both official Codex session and thread identities.
        // Vulcan 根任务会话同时映射为官方 Codex 的会话身份与线程身份。
        headers.insert(String::from("thread-id"), cache_scope_id.to_string());
    }
    Ok((
        EncodedWireRequest {
            path: String::from("/responses"),
            headers,
            body,
        },
        input_ranges,
    ))
}
