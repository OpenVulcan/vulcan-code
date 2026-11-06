//! Responses to Chat Completions request conversion.
//! Responses 到 Chat Completions 请求转换。

use crate::{LlmCoreError, reasoning::apply_canonical_reasoning_option};
use serde_json::{Map, Value, json};
use vulcan_ai_contract::{ContentItem, MessageRole, ReplaySidecar, ReplaySidecarMessage};

/// Converts a canonical Responses request into an OpenAI-compatible Chat request.
/// 将规范 Responses 请求转换为 OpenAI 兼容 Chat 请求。
pub fn responses_to_chat_completions(body: Value) -> Result<Value, LlmCoreError> {
    responses_to_chat_completions_with_media(body, None, None)
}

/// Converts `body` using an explicitly authorized internal media projector, preserving other semantics.
/// 使用显式授权的内部媒体投影器转换 `body`，保留其他语义。
/// `media` is absent for standard public conversion and `sidecar` is present only for a route that
/// declared an exact assistant replay contract.
/// 标准公共转换不提供 `media`；仅当路由声明了精确助手回传契约时才提供 `sidecar`。
/// Returns exact Chat JSON or a projection error.
/// 返回精确 Chat JSON 或投影错误。
pub(crate) fn responses_to_chat_completions_with_media(
    body: Value,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
    sidecar: Option<&ReplaySidecar>,
) -> Result<Value, LlmCoreError> {
    let mut result = Map::new();
    copy_if_present(&body, &mut result, "model");
    copy_if_present(&body, &mut result, "stream");
    copy_if_present(&body, &mut result, "temperature");
    copy_if_present(&body, &mut result, "top_p");
    copy_if_present(&body, &mut result, "frequency_penalty");
    copy_if_present(&body, &mut result, "presence_penalty");
    copy_if_present(&body, &mut result, "stop");
    copy_if_present(&body, &mut result, "parallel_tool_calls");
    if let Some(response_format) = body.get("response_format") {
        result.insert("response_format".to_string(), response_format.clone());
    } else if let Some(text_format) = body.pointer("/text/format") {
        result.insert(
            "response_format".to_string(),
            map_responses_text_format(text_format)?,
        );
    }

    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let text = instruction_text(instructions);
        if !text.is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }
    if let Some(input) = body.get("input") {
        match sidecar.filter(|_| input.is_array()) {
            // declared attribution owns the assistant merge; without it the generic projection stays.
            // 已声明归属持有助手归并；没有它时保持通用投影。
            Some(sidecar) => append_declared_chat_messages(input, sidecar, &mut messages, media)?,
            None => append_input_as_chat_messages(input, &mut messages, media)?,
        }
    }
    result.insert(
        "messages".to_string(),
        Value::Array(collapse_system_messages(messages)?),
    );

    if let Some(limit) = body.get("max_output_tokens") {
        result.insert("max_tokens".to_string(), limit.clone());
    }
    if let Some(limit) = body.get("max_tokens") {
        result.insert("max_tokens".to_string(), limit.clone());
    }
    if let Some(limit) = body.get("max_completion_tokens") {
        result.insert("max_completion_tokens".to_string(), limit.clone());
    }
    let mut result = Value::Object(result);
    apply_canonical_reasoning_option(&mut result, &body);

    if let Some(tools) = body.get("tools") {
        let mapped = map_tools(tools)?;
        if !mapped.is_empty() {
            result["tools"] = Value::Array(mapped);
        }
    }
    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = map_tool_choice(tool_choice)?;
    }
    if result
        .get("tools")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        result
            .as_object_mut()
            .expect("object result")
            .remove("tool_choice");
        result
            .as_object_mut()
            .expect("object result")
            .remove("parallel_tool_calls");
    }
    if result.get("stream").and_then(Value::as_bool) == Some(true) {
        result["stream_options"] = json!({"include_usage": true});
    }
    Ok(result)
}

/// Copies one optional field without changing the canonical JSON value.
/// 复制一个可选字段且不改变规范 JSON 值。
fn copy_if_present(body: &Value, result: &mut Map<String, Value>, key: &str) {
    if let Some(value) = body.get(key) {
        result.insert(key.to_string(), value.clone());
    }
}

/// Converts the Responses `text.format` object into Chat `response_format`.
/// 将 Responses 的 `text.format` 对象转换为 Chat 的 `response_format`。
fn map_responses_text_format(value: &Value) -> Result<Value, LlmCoreError> {
    let format_type = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| LlmCoreError::new("Responses text.format is missing type."))?;
    match format_type {
        "json_object" => Ok(json!({"type": "json_object"})),
        "json_schema" => {
            let name = value.get("name").and_then(Value::as_str).ok_or_else(|| {
                LlmCoreError::new("Responses JSON schema format is missing name.")
            })?;
            let schema = value.get("schema").ok_or_else(|| {
                LlmCoreError::new("Responses JSON schema format is missing schema.")
            })?;
            let mut json_schema = json!({
                "name": name,
                "schema": schema,
                "strict": value.get("strict").cloned().unwrap_or(Value::Bool(false)),
            });
            if let Some(description) = value.get("description") {
                json_schema["description"] = description.clone();
            }
            Ok(json!({
                "type": "json_schema",
                "json_schema": json_schema,
            }))
        }
        unsupported => Err(LlmCoreError::new(format!(
            "Unsupported Responses text.format type `{unsupported}` for Chat conversion."
        ))),
    }
}

/// Converts Responses instructions to one Chat system string.
/// 将 Responses instructions 转换为一个 Chat system 字符串。
fn instruction_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Converts every canonical input item while preserving tool-call identity.
/// 转换所有规范输入项并保持工具调用身份。
fn append_input_as_chat_messages(
    input: &Value,
    messages: &mut Vec<Value>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    match input {
        Value::String(text) => messages.push(json!({"role":"user","content":text})),
        Value::Array(items) => {
            for item in items {
                append_input_item(item, messages, media)?;
            }
        }
        Value::Object(_) => append_input_item(input, messages, media)?,
        _ => {
            return Err(LlmCoreError::new(
                "Responses input must be a string, object, or array.",
            ));
        }
    }
    Ok(())
}

/// One ordered assistant segment recovered for a declared Chat replay contract.
/// 为已声明 Chat 回传契约恢复的一个有序助手片段。
enum DeclaredChatSegment {
    /// Visible reasoning text of this exact turn.
    /// 当前精确回合的可见推理文本。
    Reasoning(String),
    /// Assistant body text of this exact turn.
    /// 当前精确回合的助手正文。
    Text(String),
    /// One assistant tool call with its provider-visible identity.
    /// 一个携带供应商可见身份的助手工具调用。
    ToolCall {
        /// Provider-visible call identity.
        /// 供应商可见调用身份。
        call_id: String,
        /// Registered tool name.
        /// 已注册工具名。
        name: String,
        /// Serialized arguments.
        /// 序列化后的参数。
        arguments: String,
    },
}

/// Reads the visible reasoning text of one replayed reasoning item, preferring plaintext content.
/// 读取一个已回放推理项的可见推理文本，优先采用明文 content。
///
/// # Remarks
/// 说明
/// DeepSeek merges plaintext `content` and ignores `summary`; OpenAI-shaped items carry the text in
/// `summary`. Reading both keeps one converter correct for every declared route.
/// DeepSeek 归并明文 `content` 并忽略 `summary`；OpenAI 形状的项把文本放在 `summary`。
/// 同时读取两者使同一个转换器对所有已声明路由都正确。
fn reasoning_item_text(item: &Value) -> String {
    let content = instruction_text(item.get("content").unwrap_or(&Value::Null));
    if !content.is_empty() {
        return content;
    }
    instruction_text(item.get("summary").unwrap_or(&Value::Null))
}

/// Recovers the ordered assistant segments of one message from canonical content or raw items.
/// 从规范内容或原始项中恢复一条消息的有序助手片段。
///
/// # Parameters
/// 参数
/// `entry` owns the canonical content captured before encoding and `range` holds the exact internal
/// input items that message produced.
/// `entry` 持有编码前捕获的规范内容，`range` 持有该消息产生的精确内部输入项。
///
/// # Returns
/// 返回值
/// Ordered segments from exactly one authority, never a mixture that could duplicate content.
/// 只来自一个权威来源的有序片段，绝不是可能重复内容的混合结果。
fn declared_chat_segments(
    entry: &ReplaySidecarMessage,
    range: &[Value],
) -> Result<Vec<DeclaredChatSegment>, LlmCoreError> {
    if !entry.content.is_empty() {
        return entry
            .content
            .iter()
            .map(|item| match item {
                ContentItem::Reasoning { text, .. } => {
                    Ok(DeclaredChatSegment::Reasoning(text.clone()))
                }
                ContentItem::Text { text } | ContentItem::Refusal { text } => {
                    Ok(DeclaredChatSegment::Text(text.clone()))
                }
                ContentItem::StructuredJson { value } => Ok(DeclaredChatSegment::Text(
                    serde_json::to_string(value).map_err(|error| {
                        LlmCoreError::new(format!(
                            "Declared Chat replay structured content could not be encoded: {error}"
                        ))
                    })?,
                )),
                ContentItem::ToolCall { call } => Ok(DeclaredChatSegment::ToolCall {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: json_string(&call.arguments),
                }),
                ContentItem::Resource { .. }
                | ContentItem::ToolResult { .. }
                | ContentItem::Citation { .. } => Err(LlmCoreError::new(
                    "Declared Chat replay cannot represent this assistant content item.",
                )),
            })
            .collect();
    }
    // rawItems is the durable provider evidence the V1 rebuild replayed verbatim.
    // rawItems 是 V1 重建原样回放的可持久化供应商证据。
    let mut segments = Vec::new();
    for item in range {
        match item.get("type").and_then(Value::as_str) {
            Some("reasoning") => {
                let text = reasoning_item_text(item);
                if !text.is_empty() {
                    segments.push(DeclaredChatSegment::Reasoning(text));
                }
            }
            Some("message") => {
                let text = instruction_text(item.get("content").unwrap_or(&Value::Null));
                if !text.is_empty() {
                    segments.push(DeclaredChatSegment::Text(text));
                }
            }
            Some("function_call" | "custom_tool_call") => {
                let call_id = item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        LlmCoreError::new("Declared Chat replay requires a tool call identity.")
                    })?
                    .to_string();
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| LlmCoreError::new("Declared Chat replay requires a tool name."))?
                    .to_string();
                segments.push(DeclaredChatSegment::ToolCall {
                    call_id,
                    name,
                    arguments: json_string(item.get("arguments").unwrap_or(&Value::Null)),
                });
            }
            _ => {}
        }
    }
    Ok(segments)
}

/// Converts one request whose assistant attribution was captured before any encoder could drop it.
/// 转换一份在任何编码器可能丢弃之前已捕获助手归属的请求。
///
/// # Parameters
/// 参数
/// `input` is the exact ordered internal input array, `sidecar` attributes its items to original
/// messages, and `media` remains the only authorized private media projector.
/// `input` 是精确有序的内部输入数组，`sidecar` 把其中各项归属到原始消息，
/// `media` 仍是唯一被授权的私有媒体投影器。
///
/// # Returns
/// 返回值
/// Exactly one Chat message per original assistant message, or a pre-dispatch failure when the
/// original interleaving cannot be represented losslessly.
/// 每条原始助手消息恰好对应一条 Chat 消息；原始交错无法无损表示时返回派发前失败。
fn append_declared_chat_messages(
    input: &Value,
    sidecar: &ReplaySidecar,
    messages: &mut Vec<Value>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    let Some(items) = input.as_array() else {
        return append_input_as_chat_messages(input, messages, media);
    };
    let mut cursor = 0usize;
    for entry in sidecar.messages() {
        if entry.input_range.start > cursor {
            // Unattributed leading items keep the generic projection instead of being dropped.
            // 未被归属的前导项保持通用投影，而不是被丢弃。
            for item in &items[cursor..entry.input_range.start.min(items.len())] {
                append_input_item(item, messages, media)?;
            }
        }
        let Some(range) = items.get(entry.input_range.clone()) else {
            return Err(LlmCoreError::new(
                "Declared Chat replay attribution exceeds the encoded input length.",
            ));
        };
        if entry.role == MessageRole::Assistant {
            append_declared_chat_assistant(entry, range, messages)?;
        } else {
            for item in range {
                append_input_item(item, messages, media)?;
            }
        }
        cursor = entry.input_range.end;
    }
    for item in items.get(cursor..).unwrap_or_default() {
        append_input_item(item, messages, media)?;
    }
    Ok(())
}

/// Appends exactly one Chat assistant message for one original assistant message.
/// 为一条原始助手消息追加恰好一条 Chat 助手消息。
///
/// # Returns
/// 返回值
/// Success after reasoning, body and tool calls land on one message, or a controlled failure when
/// Chat cannot represent the original interleaving without reordering history.
/// 推理、正文与工具调用落在同一条消息上后返回成功；Chat 无法在不重排历史的前提下表示原始交错时返回受控失败。
fn append_declared_chat_assistant(
    entry: &ReplaySidecarMessage,
    range: &[Value],
    messages: &mut Vec<Value>,
) -> Result<(), LlmCoreError> {
    let mut reasoning = String::new();
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut tool_call_seen = false;
    for segment in declared_chat_segments(entry, range)? {
        match segment {
            DeclaredChatSegment::Reasoning(segment) => {
                if tool_call_seen && !segment.is_empty() {
                    return Err(LlmCoreError::new(
                        "Declared Chat replay cannot place reasoning after a tool call without reordering history.",
                    ));
                }
                reasoning.push_str(segment.as_str());
            }
            DeclaredChatSegment::Text(segment) => {
                // Chat carries one body field beside the tool-call array, so a body can only
                // precede the calls; a turn that placed text after a call cannot keep its order.
                // Chat 在工具调用数组之外只有一个正文字段，因此正文只能位于调用之前；
                // 把正文放在调用之后的回合无法保持原顺序。
                if tool_call_seen && !segment.is_empty() {
                    return Err(LlmCoreError::new(
                        "Declared Chat replay cannot place body text after a tool call without reordering history.",
                    ));
                }
                text.push_str(segment.as_str());
            }
            DeclaredChatSegment::ToolCall {
                call_id,
                name,
                arguments,
            } => {
                tool_call_seen = true;
                tool_calls.push(json!({
                    "id": call_id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments},
                }));
            }
        }
    }
    if reasoning.is_empty() && text.is_empty() && tool_calls.is_empty() {
        // A message with nothing to replay contributes no Chat message at all.
        // 没有任何可回放内容的消息完全不产生 Chat 消息。
        return Ok(());
    }
    let mut message = Map::new();
    message.insert(String::from("role"), json!("assistant"));
    // An empty string is the verified empty-body form; it never replaces missing reasoning.
    // 空字符串是已验证的空正文形态；它绝不替代缺失的推理。
    message.insert(String::from("content"), Value::String(text));
    if !reasoning.is_empty() {
        message.insert(String::from("reasoning_content"), Value::String(reasoning));
    }
    if !tool_calls.is_empty() {
        message.insert(String::from("tool_calls"), Value::Array(tool_calls));
    }
    messages.push(Value::Object(message));
    Ok(())
}

/// Converts one Responses input item to one or more Chat messages.
/// 将一个 Responses 输入项转换为一条或多条 Chat 消息。
fn append_input_item(
    item: &Value,
    messages: &mut Vec<Value>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    let kind = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    match kind {
        "message" => {
            let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
            // Normalize developer to the system role used by the compatible Chat contract.
            // 将 developer 规范化为兼容 Chat 契约使用的 system 角色。
            let role = if role == "developer" { "system" } else { role };
            let content =
                map_message_content(item.get("content").unwrap_or(&Value::Null), role, media)?;
            let mut message = json!({"role": role, "content": content});
            if let Some(reasoning) = item.get("reasoning_content") {
                message["reasoning_content"] = reasoning.clone();
            }
            messages.push(message);
        }
        "function_call" => {
            let call_id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .ok_or_else(|| LlmCoreError::new("Responses function_call is missing call_id."))?;
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| LlmCoreError::new("Responses function_call is missing name."))?;
            let arguments =
                json_string(item.get("arguments").unwrap_or(&Value::Object(Map::new())));
            append_chat_function_call(messages, call_id, name, arguments)?;
        }
        "function_call_output" => {
            let call_id = item.get("call_id").and_then(Value::as_str).ok_or_else(|| {
                LlmCoreError::new("Responses function_call_output is missing call_id.")
            })?;
            let output = item.get("output").unwrap_or(&Value::Null);
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output_to_chat_content(output)
            }));
        }
        "reasoning" => {
            let text = item
                .get("summary")
                .map(instruction_text)
                .unwrap_or_default();
            if !text.is_empty() {
                messages.push(json!({"role":"assistant","content":null,"reasoning_content":text}));
            }
        }
        "input_image" => {
            messages.push(json!({
                "role":"user",
                "content":[responses_image_to_chat_part(item)?]
            }));
        }
        "input_file" => {
            messages.push(json!({
                "role":"user",
                "content":[responses_file_to_chat_part(item)?]
            }));
        }
        other => {
            return Err(LlmCoreError::new(format!(
                "Responses input item type `{other}` is not supported by the Chat adapter."
            )));
        }
    }
    Ok(())
}

/// Appends one Responses function call to the current Chat assistant turn.
/// 将一个 Responses 函数调用追加到当前 Chat assistant 回合。
///
/// Adjacent Responses function-call items belong to one assistant turn. OpenAI Chat requires all
/// parallel calls to remain in that single assistant message before their tool-result messages.
/// 相邻的 Responses 函数调用条目属于同一个 assistant 回合。OpenAI Chat 要求所有并行调用在
/// 对应工具结果消息之前保留于同一条 assistant 消息中。
///
/// # Parameters
/// # 参数
/// - `messages`: Chat messages already projected from the ordered Responses input.
/// - `messages`：已经从有序 Responses 输入投影出的 Chat 消息。
/// - `call_id`: Stable tool-call identity returned by the model.
/// - `call_id`：模型返回的稳定工具调用标识。
/// - `name`: Registered function name.
/// - `name`：已注册的函数名称。
/// - `arguments`: JSON-encoded function arguments.
/// - `arguments`：JSON 编码后的函数参数。
///
/// # Returns
/// # 返回值
/// Returns success after appending to the adjacent assistant message, or an error when the existing
/// `tool_calls` field has an invalid shape.
/// 成功追加到相邻 assistant 消息后返回成功；既有 `tool_calls` 字段结构无效时返回错误。
fn append_chat_function_call(
    messages: &mut Vec<Value>,
    call_id: &str,
    name: &str,
    arguments: String,
) -> Result<(), LlmCoreError> {
    let tool_call = json!({
        "id": call_id,
        "type": "function",
        "function": {"name": name, "arguments": arguments}
    });
    if let Some(message) = messages.last_mut()
        && message.get("role").and_then(Value::as_str) == Some("assistant")
    {
        let message = message
            .as_object_mut()
            .ok_or_else(|| LlmCoreError::new("Chat assistant message must be an object."))?;
        let tool_calls = message
            .entry("tool_calls")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| LlmCoreError::new("Chat assistant tool_calls must be an array."))?;
        tool_calls.push(tool_call);
        return Ok(());
    }
    messages.push(json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [tool_call]
    }));
    Ok(())
}

/// Maps one Responses message content value to Chat content.
/// 将一个 Responses 消息内容值映射为 Chat 内容。
fn map_message_content(
    content: &Value,
    role: &str,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<Value, LlmCoreError> {
    match content {
        Value::String(_) => Ok(content.clone()),
        Value::Array(parts) => {
            let mapped = parts
                .iter()
                .map(|part| map_content_part(part, role, media))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(mapped))
        }
        Value::Null => Ok(Value::String(String::new())),
        _ => Err(LlmCoreError::new(
            "Responses message content must be a string or array.",
        )),
    }
}

/// Maps one content part, including the provider-neutral image shape.
/// 映射一个内容部分，包括供应商无关的图片形状。
fn map_content_part(
    part: &Value,
    role: &str,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<Value, LlmCoreError> {
    let kind = part
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("input_text");
    match kind {
        "input_text" | "output_text" | "text" => Ok(json!({
            "type":"text",
            "text":part.get("text").and_then(Value::as_str).unwrap_or_default()
        })),
        "input_image" | "image_url" => responses_image_to_chat_part(part),
        "input_file" => responses_file_to_chat_part(part),
        crate::INTERNAL_RESOURCE_PART_TYPE => {
            crate::internal_media::project_internal_part(part, role, media)
        }
        other => Err(LlmCoreError::new(format!(
            "Responses content part type `{other}` is not supported by the Chat adapter."
        ))),
    }
}

/// Converts one Responses image part into Chat image_url while preserving detail.
/// 将一项 Responses 图片分片转换为 Chat image_url，同时保留 detail。
fn responses_image_to_chat_part(part: &Value) -> Result<Value, LlmCoreError> {
    if part.get("file_id").is_some() {
        return Err(LlmCoreError::new(
            "Responses image file_id cannot be projected across providers to OpenAI Chat.",
        ));
    }
    let url = part
        .get("image_url")
        .or_else(|| part.get("url"))
        .and_then(Value::as_str)
        .ok_or_else(|| LlmCoreError::new("Responses image part is missing image_url."))?;
    let mut image_url = json!({"url": url});
    if let Some(detail) = part.get("detail") {
        image_url["detail"] = detail.clone();
    }
    Ok(json!({"type":"image_url","image_url":image_url}))
}

/// Converts one Responses file part into the registered Chat file content shape.
/// 将一项 Responses 文件分片转换为已注册的 Chat 文件内容形态。
fn responses_file_to_chat_part(part: &Value) -> Result<Value, LlmCoreError> {
    if let Some(file_id) = part
        .get("file_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(json!({"type":"file","file":{"file_id":file_id}}));
    }
    if part.get("file_url").is_some() {
        return Err(LlmCoreError::new(
            "OpenAI Chat file content has no registered file_url projector.",
        ));
    }
    let file_data = part
        .get("file_data")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmCoreError::new("Responses input_file is missing file data."))?;
    let filename = part
        .get("filename")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmCoreError::new("Responses file_data is missing filename."))?;
    Ok(json!({
        "type":"file",
        "file":{"filename":filename,"file_data":file_data}
    }))
}

/// Converts a tool output to a Chat-compatible string or content array.
/// 将工具输出转换为 Chat 兼容的字符串或内容数组。
fn output_to_chat_content(output: &Value) -> Value {
    match output {
        Value::String(_) | Value::Array(_) => output.clone(),
        Value::Null => Value::String(String::new()),
        _ => Value::String(json_string(output)),
    }
}

/// Converts Responses function tools to Chat function tools.
/// 将 Responses 函数工具转换为 Chat 函数工具。
fn map_tools(tools: &Value) -> Result<Vec<Value>, LlmCoreError> {
    let values = tools
        .as_array()
        .ok_or_else(|| LlmCoreError::new("Responses tools must be an array."))?;
    values
        .iter()
        .map(|tool| {
            let kind = tool
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("function");
            if kind != "function" {
                return Err(LlmCoreError::new(format!(
                    "Responses tool type `{kind}` is not supported by the Chat adapter."
                )));
            }
            let name = tool
                .get("name")
                .or_else(|| tool.pointer("/function/name"))
                .and_then(Value::as_str)
                .ok_or_else(|| LlmCoreError::new("Responses function tool is missing name."))?;
            let description = tool.get("description").cloned().unwrap_or(Value::Null);
            let parameters = tool
                .get("parameters")
                .or_else(|| tool.pointer("/function/parameters"))
                .cloned()
                .unwrap_or_else(|| json!({"type":"object","properties":{}}));
            let mut function = json!({"name":name,"parameters":parameters});
            if !description.is_null() {
                function["description"] = description;
            }
            if let Some(strict) = tool.get("strict") {
                function["strict"] = strict.clone();
            }
            Ok(json!({"type":"function","function":function}))
        })
        .collect()
}

/// Converts Responses tool choice to Chat tool choice.
/// 将 Responses 工具选择转换为 Chat 工具选择。
fn map_tool_choice(tool_choice: &Value) -> Result<Value, LlmCoreError> {
    match tool_choice {
        Value::String(value) if matches!(value.as_str(), "auto" | "none" | "required") => {
            Ok(tool_choice.clone())
        }
        Value::Object(object) => {
            let name = object
                .get("name")
                .or_else(|| {
                    object
                        .get("function")
                        .and_then(|function| function.get("name"))
                })
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    LlmCoreError::new("Responses tool_choice function is missing name.")
                })?;
            Ok(json!({"type":"function","function":{"name":name}}))
        }
        _ => Err(LlmCoreError::new(
            "Responses tool_choice has an unsupported shape.",
        )),
    }
}

/// Serializes object arguments exactly once for Chat function-call history.
/// 为 Chat 函数调用历史仅序列化一次对象参数。
fn json_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| String::from("{}")),
    }
}

/// Merges compatible Chat instructions at the head while preserving ordered text parts.
/// 在兼容 Chat 的开头合并指令，并保留有序文本块。
/// `messages` contains mapped Chat content; returns the merged sequence or an error for non-text instructions.
/// `messages` 包含已映射的 Chat 内容；返回合并序列，非文本指令则返回错误。
fn collapse_system_messages(messages: Vec<Value>) -> Result<Vec<Value>, LlmCoreError> {
    let mut system_text = Vec::new();
    let mut rest = Vec::new();
    for message in messages {
        if message.get("role").and_then(Value::as_str) == Some("system") {
            // Mapped Responses instructions are commonly text arrays, not scalar strings.
            // 映射后的 Responses 指令通常是文本数组，而不是单个字符串。
            let content = &message["content"];
            if let Some(parts) = content.as_array()
                && parts.iter().any(|part| part["type"] != "text")
            {
                return Err(LlmCoreError::new(
                    "Chat system messages require text content; non-text instruction parts cannot be discarded.",
                ));
            }
            // Preserve ordered text parts instead of silently dropping the complete instruction message.
            // 保留有序文本块，避免静默丢弃整条指令消息。
            let text = instruction_text(content);
            if !text.trim().is_empty() {
                system_text.push(text);
            }
        } else {
            rest.push(message);
        }
    }
    if system_text.is_empty() {
        Ok(rest)
    } else {
        let mut result = vec![json!({"role":"system","content":system_text.join("\n\n")})];
        result.extend(rest);
        Ok(result)
    }
}
