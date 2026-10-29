//! Responses to Anthropic Messages conversion based on the CC Switch request rules.
//! 基于 CC Switch 请求规则实现 Responses 到 Anthropic Messages 的转换。

use crate::{
    AnthropicReasoningMode, LlmCoreError, anthropic_block_from_openai_reasoning_item,
    reasoning::canonical_reasoning_effort,
};
use serde_json::{Map, Value, json};
use vulcan_ai_contract::{ContentItem, MessageRole, ReplaySidecar, ReplaySidecarMessage};

/// Converts one canonical Responses request into an Anthropic Messages request.
/// 将一份规范 Responses 请求转换为 Anthropic Messages 请求。
pub fn responses_to_anthropic_messages(body: Value) -> Result<Value, LlmCoreError> {
    responses_to_anthropic_messages_for_route(body, AnthropicReasoningMode::NativeBudget)
}

/// Converts a Responses request using the selected Anthropic reasoning contract.
/// 使用选定的 Anthropic 推理契约转换 Responses 请求。
pub(crate) fn responses_to_anthropic_messages_for_route(
    body: Value,
    reasoning_mode: AnthropicReasoningMode,
) -> Result<Value, LlmCoreError> {
    responses_to_anthropic_messages_with_media(body, reasoning_mode, None, None)
}

/// Converts `body` with exact `reasoning_mode` and optional trusted internal `media` projection.
/// 使用精确 `reasoning_mode` 与可选可信内部 `media` 投影转换 `body`。
/// `sidecar` is present only for a route that declared an exact assistant replay contract.
/// 仅当路由声明了精确助手回传契约时才提供 `sidecar`。
/// Returns Anthropic JSON or an explicit mapping error; standard callers cannot inject private media.
/// 返回 Anthropic JSON 或明确映射错误；标准调用者不能注入私有媒体。
pub(crate) fn responses_to_anthropic_messages_with_media(
    body: Value,
    reasoning_mode: AnthropicReasoningMode,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
    sidecar: Option<&ReplaySidecar>,
) -> Result<Value, LlmCoreError> {
    let model = required_string(&body, "/model", "Responses model")?;
    let mut result = Map::new();
    result.insert(String::from("model"), Value::String(model.to_string()));

    let system = collect_system_text(&body)?;
    if !system.is_empty() {
        result.insert(String::from("system"), Value::String(system));
    }

    let mut messages = Vec::new();
    if let Some(input) = body.get("input") {
        match sidecar.filter(|_| input.is_array()) {
            // declared attribution owns the assistant blocks; without it the generic path stays.
            // 已声明归属持有助手内容块；没有它时保持通用路径。
            Some(sidecar) => {
                append_declared_anthropic_messages(input, sidecar, &mut messages, media)?
            }
            None => append_anthropic_input(input, &mut messages, media)?,
        }
    }
    normalize_anthropic_messages(&mut messages)?;
    result.insert(String::from("messages"), Value::Array(messages));

    let max_tokens = body
        .get("max_output_tokens")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            LlmCoreError::new(
                "Anthropic Messages conversion requires a positive `max_output_tokens`.",
            )
        })?;
    result.insert(String::from("max_tokens"), Value::Number(max_tokens.into()));
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(true);
    result.insert(String::from("stream"), Value::Bool(stream));

    if let Some(tools) = body.get("tools") {
        // Native Anthropic buffers tool input unless asked; compatible gateways may reject the field.
        // 原生 Anthropic 默认缓冲工具输入；兼容网关可能拒绝该字段。
        let eager_input_streaming =
            stream && reasoning_mode == AnthropicReasoningMode::NativeBudget;
        let mapped_tools = map_anthropic_tools(tools, eager_input_streaming)?;
        if !mapped_tools.is_empty() {
            result.insert(String::from("tools"), Value::Array(mapped_tools));
        }
    }
    if let Some(tool_choice) = body.get("tool_choice")
        && result
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty())
    {
        result.insert(
            String::from("tool_choice"),
            map_anthropic_tool_choice(tool_choice)?,
        );
    }
    project_anthropic_controls(&body, &mut result, max_tokens, reasoning_mode)?;
    Ok(Value::Object(result))
}

/// Reads one required non-empty string from a JSON object path.
/// 从 JSON 对象路径读取一个必需的非空字符串。
fn required_string<'a>(body: &'a Value, path: &str, label: &str) -> Result<&'a str, LlmCoreError> {
    body.pointer(path)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmCoreError::new(format!("{label} is missing or empty at `{path}`.")))
}

/// Collects top-level instructions and system/developer input into Anthropic system text.
/// 将顶层 instructions 以及 system/developer 输入汇总为 Anthropic system 文本。
fn collect_system_text(body: &Value) -> Result<String, LlmCoreError> {
    let mut parts = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        append_text_value(instructions, &mut parts, "instructions")?;
    }
    if let Some(Value::Array(items)) = body.get("input") {
        for item in items {
            let role = item.get("role").and_then(Value::as_str);
            if matches!(role, Some("system" | "developer")) {
                append_message_text(item, &mut parts)?;
            }
        }
    }
    Ok(parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// Appends text from a scalar or Responses content array.
/// 从标量或 Responses 内容数组追加文本。
fn append_text_value(
    value: &Value,
    parts: &mut Vec<String>,
    label: &str,
) -> Result<(), LlmCoreError> {
    match value {
        Value::String(text) => parts.push(text.clone()),
        Value::Array(values) => {
            for value in values {
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                } else if let Some(text) = value.as_str() {
                    parts.push(text.to_string());
                } else {
                    return Err(LlmCoreError::new(format!(
                        "Anthropic `{label}` contains a non-text part."
                    )));
                }
            }
        }
        Value::Null => {}
        _ => {
            return Err(LlmCoreError::new(format!(
                "Anthropic `{label}` must be a string or text array."
            )));
        }
    }
    Ok(())
}

/// Builds one Anthropic thinking block from the exact recorded same-route reasoning.
/// 从精确的已记录同路由推理构建一个 Anthropic thinking 块。
///
/// # Parameters
/// 参数
/// `text` is the complete recorded reasoning and `signature` is the provenance-tagged value the
/// Anthropic decoder stored beside it, when one exists.
/// `text` 是完整的已记录推理，`signature` 是 Anthropic 解码器与其一同存储的带来源标记值（若存在）。
///
/// # Returns
/// 返回值
/// The verified `thinking` block, or an explicit refusal to replay reasoning that carries no text.
/// 已验证的 `thinking` 块；或明确拒绝回放不含文本的推理。
///
/// # Remarks
/// 说明
/// Verified against the real endpoint on 2026-09-13: DeepSeek accepts a `thinking` block with the
/// original signature, without any signature, with empty text plus a signature, and even with a
/// fabricated signature, and it never validates the value. The frozen shape therefore follows this
/// engine's own integrity rules instead of endpoint tolerance: the text is required, and the
/// signature is emitted only when the original payload proves it belongs to this provider.
/// 已于 2026-09-13 对真实端点验证：DeepSeek 接受带原始签名、不带签名、空文本加签名，
/// 甚至带伪造签名的 `thinking` 块，且从不校验该值。因此冻结形态依据本引擎自身的完整性规则，
/// 而不是端点的容忍度：文本必需，且仅当原始载荷能证明归属本供应商时才输出签名。
fn declared_anthropic_thinking_block(
    text: &str,
    signature: Option<&str>,
) -> Result<Value, LlmCoreError> {
    if text.trim().is_empty() {
        return Err(LlmCoreError::new(
            "Declared Anthropic replay requires reasoning text that a thinking block can carry.",
        ));
    }
    let mut block = Map::from_iter([
        (
            String::from("type"),
            Value::String(String::from("thinking")),
        ),
        (String::from("thinking"), Value::String(text.to_string())),
    ]);
    // A missing or foreign signature omits the field entirely; it is never replaced by an empty
    // string, by a bridge envelope, or by visible text.
    // 签名缺失或来源不符时完全省略该字段；绝不用空字符串、桥接封装或可见文本替代。
    if let Some(payload) = signature.and_then(|signature| {
        crate::compatible_provider_signature(crate::SIGNATURE_PROVIDER_ANTHROPIC, signature)
    }) {
        block.insert(
            String::from("signature"),
            Value::String(payload.to_string()),
        );
    }
    Ok(Value::Object(block))
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
/// Ordered `thinking`, body and `tool_use` blocks per original assistant message, or a pre-dispatch
/// failure when a required thinking block cannot be proven sendable.
/// 每条原始助手消息对应有序的 `thinking`、正文与 `tool_use` 块；
/// 必需的 thinking 块无法证明可发送时返回派发前失败。
fn append_declared_anthropic_messages(
    input: &Value,
    sidecar: &ReplaySidecar,
    messages: &mut Vec<Value>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    let Some(items) = input.as_array() else {
        return append_anthropic_input(input, messages, media);
    };
    let mut cursor = 0usize;
    for entry in sidecar.messages() {
        if entry.input_range.start > cursor {
            // Unattributed leading items keep the generic projection instead of being dropped.
            // 未被归属的前导项保持通用投影，而不是被丢弃。
            for item in &items[cursor..entry.input_range.start.min(items.len())] {
                append_anthropic_item(item, messages, media)?;
            }
        }
        let Some(range) = items.get(entry.input_range.clone()) else {
            return Err(LlmCoreError::new(
                "Declared Anthropic replay attribution exceeds the encoded input length.",
            ));
        };
        if entry.role == MessageRole::Assistant {
            append_declared_anthropic_assistant(entry, range, messages)?;
        } else {
            for item in range {
                append_anthropic_item(item, messages, media)?;
            }
        }
        cursor = entry.input_range.end;
    }
    for item in items.get(cursor..).unwrap_or_default() {
        append_anthropic_item(item, messages, media)?;
    }
    Ok(())
}

/// Appends the ordered thinking, body and tool-call blocks of one original assistant message.
/// 追加一条原始助手消息的有序 thinking、正文与工具调用块。
///
/// # Parameters
/// 参数
/// `entry` owns the canonical content and `range` holds the exact internal input items that message
/// produced, including persisted reasoning that carries no proven provider signature.
/// `entry` 持有规范内容，`range` 持有该消息产生的精确内部输入项，
/// 其中包含没有已证实供应商签名的可持久化推理。
///
/// # Remarks
/// 说明
/// Exactly one authority owns the turn: canonical content when it carries reasoning, otherwise the
/// ordered persisted items, which is the shape an intact bare envelope replays. Mixing them could
/// duplicate or reorder thinking, so the choice is exclusive.
/// 一个回合只由一个权威持有：规范内容含推理时用规范内容，否则用有序可持久化项——
/// 后者正是完整裸封套回放出的形态。混合两者可能重复或错排 thinking，因此该选择是互斥的。
fn append_declared_anthropic_assistant(
    entry: &ReplaySidecarMessage,
    range: &[Value],
    messages: &mut Vec<Value>,
) -> Result<(), LlmCoreError> {
    let canonical_owns_reasoning = entry
        .content
        .iter()
        .any(|item| matches!(item, ContentItem::Reasoning { .. }));
    let recorded_reasoning = range
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("reasoning"))
        .count();
    if recorded_reasoning > 0 && !canonical_owns_reasoning {
        return append_recorded_anthropic_assistant(range, messages);
    }
    append_canonical_anthropic_assistant(&entry.content, messages)
}

/// Appends the ordered blocks of one assistant message from its canonical content.
/// 从规范内容追加一条助手消息的有序块。
fn append_canonical_anthropic_assistant(
    content: &[ContentItem],
    messages: &mut Vec<Value>,
) -> Result<(), LlmCoreError> {
    for item in content {
        match item {
            ContentItem::Reasoning { text, signature } => {
                let block = declared_anthropic_thinking_block(text, signature.as_deref())?;
                push_anthropic_block(messages, "assistant", block);
            }
            ContentItem::Text { text } | ContentItem::Refusal { text } => {
                push_anthropic_block(messages, "assistant", json!({"type":"text","text":text}));
            }
            ContentItem::StructuredJson { value } => {
                push_anthropic_block(
                    messages,
                    "assistant",
                    json!({"type":"text","text": canonical_json(value)}),
                );
            }
            ContentItem::ToolCall { call } => {
                push_anthropic_block(
                    messages,
                    "assistant",
                    json!({
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.name,
                        "input": call.arguments,
                    }),
                );
            }
            ContentItem::Resource { .. }
            | ContentItem::ToolResult { .. }
            | ContentItem::Citation { .. } => {
                return Err(LlmCoreError::new(
                    "Declared Anthropic replay cannot represent this assistant content item.",
                ));
            }
        }
    }
    Ok(())
}

/// Appends the ordered blocks of one assistant message from its persisted provider items.
/// 从可持久化供应商项追加一条助手消息的有序块。
///
/// # Remarks
/// 说明
/// A persisted item may carry the original signature in `encrypted_content`; `thinking_item_signature`
/// accepts it only when the provenance tag proves it belongs to this provider, so an untagged value
/// can never be replayed as a signature.
/// 可持久化项可能在 `encrypted_content` 中携带原始签名；`thinking_item_signature` 仅在来源标记
/// 证明其归属本供应商时才接受它，因此未标记的值永远不能被当作签名回放。
fn append_recorded_anthropic_assistant(
    range: &[Value],
    messages: &mut Vec<Value>,
) -> Result<(), LlmCoreError> {
    for item in range {
        match item.get("type").and_then(Value::as_str) {
            Some("reasoning") => {
                let text = recorded_reasoning_text(item);
                let block = declared_anthropic_thinking_block(
                    &text,
                    thinking_item_signature(item).as_deref(),
                )?;
                push_anthropic_block(messages, "assistant", block);
            }
            Some("message") => {
                let text = responses_message_text(item);
                if !text.is_empty() {
                    push_anthropic_block(
                        messages,
                        "assistant",
                        json!({"type":"text","text": text}),
                    );
                }
            }
            Some("function_call" | "custom_tool_call") => {
                let call_id = required_call_id(item, "replayed tool call")?;
                let name = required_string(item, "/name", "Replayed tool call name")?;
                let arguments = object_arguments(item.get("arguments"), name)?;
                push_anthropic_block(
                    messages,
                    "assistant",
                    json!({"type":"tool_use","id":call_id,"name":name,"input":arguments}),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

/// Reads the concatenated reasoning text of one persisted reasoning item.
/// 读取一个可持久化推理项的拼接推理文本。
fn recorded_reasoning_text(item: &Value) -> String {
    let plaintext = text_parts(item.get("content"));
    if !plaintext.trim().is_empty() {
        return plaintext;
    }
    text_parts(item.get("summary"))
}

/// Reads the concatenated `text` fields of one text-part array.
/// 读取一个文本项数组中拼接后的 `text` 字段。
fn text_parts(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

/// Reads the original provider signature of one persisted reasoning item, when proven.
/// 在可证明时读取一个可持久化推理项的原始供应商签名。
fn thinking_item_signature(item: &Value) -> Option<String> {
    let value = item.get("encrypted_content").and_then(Value::as_str)?;
    // The provenance tag is only *checked* here; the still-tagged value is handed on so exactly one
    // place performs the extraction. Unwrapping in both places would strip the tag twice and lose
    // the signature entirely.
    // 此处只**检查**来源标记，并把仍带标记的值继续传递，使解封只发生在一个地方。
    // 两处都解封会把标记去掉两次，从而彻底丢失签名。
    crate::compatible_provider_signature(crate::SIGNATURE_PROVIDER_ANTHROPIC, value)
        .map(|_| value.to_string())
}

/// Reads the concatenated visible text of one assistant `message` item.
/// 读取一个助手 `message` 项拼接后的可见正文。
fn responses_message_text(item: &Value) -> String {
    match item.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Appends one Responses input value to ordered Anthropic messages.
/// 将一个 Responses input 值追加到有序 Anthropic 消息中。
fn append_anthropic_input(
    input: &Value,
    messages: &mut Vec<Value>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    match input {
        Value::String(text) => push_anthropic_block(
            messages,
            "user",
            json!({
                "type": "text",
                "text": text,
            }),
        ),
        Value::Array(items) => {
            for item in items {
                append_anthropic_item(item, messages, media)?;
            }
        }
        Value::Object(_) => append_anthropic_item(input, messages, media)?,
        Value::Null => {}
        _ => {
            return Err(LlmCoreError::new(
                "Responses input must be a string, object, or array for Anthropic conversion.",
            ));
        }
    }
    Ok(())
}

/// Converts one Responses input item into Anthropic blocks without dropping unsupported content.
/// 将一个 Responses 输入项转换为 Anthropic 内容块，不静默丢弃不支持的内容。
fn append_anthropic_item(
    item: &Value,
    messages: &mut Vec<Value>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    let item_type = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    match item_type {
        "input_text" => {
            let text = required_string(item, "/text", "Responses input_text")?;
            push_anthropic_block(messages, "user", json!({"type":"text","text":text}));
        }
        "input_image" => {
            let image = anthropic_image_block(item)?;
            push_anthropic_block(messages, "user", image);
        }
        "function_call" => {
            let call_id = required_call_id(item, "function_call")?;
            let name = required_string(item, "/name", "Responses function_call name")?;
            let arguments = object_arguments(item.get("arguments"), name)?;
            push_anthropic_block(
                messages,
                "assistant",
                json!({"type":"tool_use","id":call_id,"name":name,"input":arguments}),
            );
        }
        "function_call_output" => {
            let call_id = required_call_id(item, "function_call_output")?;
            let content = anthropic_tool_result_content(item.get("output"))?;
            push_anthropic_block(
                messages,
                "user",
                json!({"type":"tool_result","tool_use_id":call_id,"content":content}),
            );
        }
        "reasoning" => {
            if let Some(block) = anthropic_block_from_openai_reasoning_item(item) {
                push_anthropic_block(messages, "assistant", block);
            }
        }
        "message" => {
            let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
            if matches!(role, "system" | "developer") {
                return Ok(());
            }
            let role = if role == "assistant" {
                "assistant"
            } else {
                "user"
            };
            append_message_blocks(item, role, messages, media)?;
        }
        "input_file" => {
            let document = anthropic_document_block(item)?;
            push_anthropic_block(messages, "user", document);
        }
        other => {
            return Err(LlmCoreError::new(format!(
                "Responses input item type `{other}` is not supported by Anthropic conversion."
            )));
        }
    }
    Ok(())
}

/// Converts one role-bearing Responses message into Anthropic content blocks.
/// 将一个带角色的 Responses 消息转换为 Anthropic 内容块。
fn append_message_blocks(
    item: &Value,
    role: &str,
    messages: &mut Vec<Value>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    let content = item.get("content").unwrap_or(&Value::Null);
    match content {
        Value::String(text) => {
            push_anthropic_block(messages, role, json!({"type":"text","text":text}));
        }
        Value::Array(parts) => {
            for part in parts {
                let kind = part
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("input_text");
                match kind {
                    "input_text" | "output_text" | "text" => {
                        let text = required_string(part, "/text", "Responses text part")?;
                        push_anthropic_block(messages, role, json!({"type":"text","text":text}));
                    }
                    "input_image" | "image_url" => {
                        push_anthropic_block(messages, role, anthropic_image_block(part)?);
                    }
                    "input_file" => {
                        push_anthropic_block(messages, role, anthropic_document_block(part)?);
                    }
                    crate::INTERNAL_RESOURCE_PART_TYPE => {
                        push_anthropic_block(
                            messages,
                            role,
                            crate::internal_media::project_internal_part(part, role, media)?,
                        );
                    }
                    "refusal" => {
                        let text = required_string(part, "/refusal", "Responses refusal")?;
                        push_anthropic_block(messages, role, json!({"type":"text","text":text}));
                    }
                    other => {
                        return Err(LlmCoreError::new(format!(
                            "Responses message content type `{other}` is not supported by Anthropic conversion."
                        )));
                    }
                }
            }
        }
        Value::Null => {}
        _ => {
            return Err(LlmCoreError::new(
                "Responses message content must be a string or array for Anthropic conversion.",
            ));
        }
    }
    Ok(())
}

/// Appends text from one system/developer message.
/// 从一个 system/developer 消息追加文本。
fn append_message_text(item: &Value, parts: &mut Vec<String>) -> Result<(), LlmCoreError> {
    if let Some(content) = item.get("content") {
        append_text_value(content, parts, "system message")?;
    }
    Ok(())
}

/// Creates one Anthropic image block from a URL or data URL.
/// 从 URL 或 data URL 创建一个 Anthropic 图片块。
fn anthropic_image_block(item: &Value) -> Result<Value, LlmCoreError> {
    if item.get("detail").is_some() {
        return Err(LlmCoreError::new(
            "Anthropic conversion cannot preserve Responses image detail.",
        ));
    }
    if item.get("file_id").is_some() {
        return Err(LlmCoreError::new(
            "Responses image file_id cannot be projected across providers to Anthropic.",
        ));
    }
    let url = item
        .get("image_url")
        .or_else(|| item.get("url"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmCoreError::new("Responses input_image is missing `image_url`."))?;
    if let Some((media_type, data)) = parse_data_url(url) {
        return Ok(json!({
            "type":"image",
            "source":{"type":"base64","media_type":media_type,"data":data}
        }));
    }
    Ok(json!({"type":"image","source":{"type":"url","url":url}}))
}

/// Creates one Anthropic document block from standard Responses file URL or data.
/// 根据标准 Responses 文件 URL 或数据创建一个 Anthropic document 块。
pub(crate) fn anthropic_document_block(item: &Value) -> Result<Value, LlmCoreError> {
    if item.get("file_id").is_some() {
        return Err(LlmCoreError::new(
            "Responses file_id cannot be projected across providers to Anthropic.",
        ));
    }
    let source = if let Some(url) = item
        .get("file_url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        json!({"type":"url","url":url})
    } else {
        let data_url = item
            .get("file_data")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| LlmCoreError::new("Responses input_file is missing file data."))?;
        let (media_type, data) = parse_data_url(data_url).ok_or_else(|| {
            LlmCoreError::new(
                "Anthropic file_data conversion requires a Base64 data URL with MIME type.",
            )
        })?;
        json!({"type":"base64","media_type":media_type,"data":data})
    };
    let mut block = json!({"type":"document","source":source});
    if let Some(title) = item
        .get("filename")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        block["title"] = Value::String(title.to_string());
    }
    Ok(block)
}

/// Parses a base64 data URL into its media type and encoded data.
/// 将 base64 data URL 解析为媒体类型与编码数据。
fn parse_data_url(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix("data:")?;
    let (metadata, data) = rest.split_once(',')?;
    let media_type = metadata.split(';').next()?.trim();
    metadata
        .split(';')
        .any(|part| part == "base64")
        .then_some((media_type, data))
}

/// Parses function arguments and requires the Anthropic tool-use object shape.
/// 解析函数参数并要求其符合 Anthropic tool-use 对象形状。
fn object_arguments(value: Option<&Value>, name: &str) -> Result<Value, LlmCoreError> {
    let empty_arguments = Value::Object(Map::new());
    let value = value.unwrap_or(&empty_arguments);
    let parsed = match value {
        Value::String(text) if text.trim().is_empty() => json!({}),
        Value::String(text) => serde_json::from_str::<Value>(text).map_err(|error| {
            LlmCoreError::new(format!(
                "Responses function_call `{name}` arguments are invalid JSON: {error}"
            ))
        })?,
        other => other.clone(),
    };
    if !parsed.is_object() {
        return Err(LlmCoreError::new(format!(
            "Responses function_call `{name}` arguments must be a JSON object."
        )));
    }
    Ok(parsed)
}

/// Requires a stable call identity from one Responses tool item.
/// 从一个 Responses 工具项读取稳定调用身份。
fn required_call_id<'a>(item: &'a Value, kind: &str) -> Result<&'a str, LlmCoreError> {
    item.get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmCoreError::new(format!("Responses {kind} is missing `call_id`.")))
}

/// Converts one tool output value into Anthropic tool-result content.
/// 将一个工具输出值转换为 Anthropic tool-result 内容。
fn anthropic_tool_result_content(value: Option<&Value>) -> Result<Value, LlmCoreError> {
    let Some(value) = value else {
        return Ok(Value::String(String::new()));
    };
    match value {
        Value::String(text) => Ok(Value::String(text.clone())),
        Value::Array(parts) => parts
            .iter()
            .map(|part| match part.get("type").and_then(Value::as_str) {
                Some("input_text" | "output_text" | "text") => {
                    let text = required_string(part, "/text", "Responses tool output text")?;
                    Ok(json!({"type":"text","text":text}))
                }
                Some("input_image" | "image_url") => anthropic_image_block(part),
                Some("input_file") => anthropic_document_block(part),
                Some(other) => Err(LlmCoreError::new(format!(
                    "Responses tool output content type `{other}` is not supported by Anthropic conversion."
                ))),
                None => Ok(json!({"type":"text","text":canonical_json(value)})),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        other => Ok(Value::String(canonical_json(other))),
    }
}

/// Maps Responses function tools into Anthropic tool definitions.
/// 将 Responses 函数工具转换为 Anthropic 工具定义。
///
/// `eager_input_streaming` streams function-tool input as it is generated; the stream decoder
/// still rejects input that does not end as complete JSON.
/// `eager_input_streaming` 让函数工具输入边生成边流式返回；流解码器仍拒绝未以完整 JSON 结束的输入。
fn map_anthropic_tools(
    tools: &Value,
    eager_input_streaming: bool,
) -> Result<Vec<Value>, LlmCoreError> {
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
            if matches!(kind, "web_search" | "web_search_preview") {
                return Ok(json!({
                    "type": "web_search_20250305",
                    "name": "web_search",
                }));
            }
            if kind != "function" {
                return Err(LlmCoreError::new(format!(
                    "Responses tool type `{kind}` is not supported by Anthropic conversion."
                )));
            }
            if tool.get("strict").and_then(Value::as_bool) == Some(true) {
                return Err(LlmCoreError::new(
                    "Anthropic conversion cannot preserve strict function-tool semantics.",
                ));
            }
            let name = tool
                .get("name")
                .or_else(|| tool.pointer("/function/name"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| LlmCoreError::new("Responses function tool is missing `name`."))?;
            let input_schema = tool
                .get("parameters")
                .or_else(|| tool.pointer("/function/parameters"))
                .cloned()
                .unwrap_or_else(|| json!({"type":"object","properties":{}}));
            let mut mapped = json!({"name":name,"input_schema":input_schema});
            if let Some(description) = tool.get("description") {
                mapped["description"] = description.clone();
            }
            if eager_input_streaming {
                mapped["eager_input_streaming"] = Value::Bool(true);
            }
            Ok(mapped)
        })
        .collect()
}

/// Maps OpenAI tool-choice values into Anthropic tool-choice values.
/// 将 OpenAI 工具选择值转换为 Anthropic 工具选择值。
fn map_anthropic_tool_choice(value: &Value) -> Result<Value, LlmCoreError> {
    match value {
        Value::String(choice) => match choice.as_str() {
            "auto" => Ok(json!({"type":"auto"})),
            "required" => Ok(json!({"type":"any"})),
            "none" => Ok(json!({"type":"none"})),
            other => Err(LlmCoreError::new(format!(
                "Anthropic does not support tool choice `{other}`."
            ))),
        },
        Value::Object(object) => {
            let name = object
                .get("name")
                .or_else(|| {
                    object
                        .get("function")
                        .and_then(|function| function.get("name"))
                })
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    LlmCoreError::new("Responses tool_choice is missing function name.")
                })?;
            Ok(json!({"type":"tool","name":name}))
        }
        _ => Err(LlmCoreError::new(
            "Responses tool_choice has an unsupported shape for Anthropic conversion.",
        )),
    }
}

/// Projects structured output, reasoning, and sampling controls into Anthropic fields.
/// 将结构化输出、推理与采样控制投影为 Anthropic 字段。
fn project_anthropic_controls(
    body: &Value,
    result: &mut Map<String, Value>,
    max_tokens: u64,
    reasoning_mode: AnthropicReasoningMode,
) -> Result<(), LlmCoreError> {
    if let Some(format) = body.pointer("/text/format") {
        match format.get("type").and_then(Value::as_str) {
            Some("json_schema") => {
                let schema = format.get("schema").ok_or_else(|| {
                    LlmCoreError::new("Responses JSON schema format is missing `schema`.")
                })?;
                result.insert(
                    String::from("output_config"),
                    json!({"format":{"type":"json_schema","schema":schema}}),
                );
            }
            Some("json_object") => {
                return Err(LlmCoreError::new(
                    "Anthropic conversion requires an explicit JSON Schema for structured output.",
                ));
            }
            Some(other) => {
                return Err(LlmCoreError::new(format!(
                    "Responses text format `{other}` is not supported by Anthropic conversion."
                )));
            }
            None => {
                return Err(LlmCoreError::new(
                    "Responses text format is missing `type`.",
                ));
            }
        }
    }
    if let Some(stop) = body.get("stop") {
        result.insert(String::from("stop_sequences"), stop.clone());
    }

    if reasoning_mode == AnthropicReasoningMode::NativeBudget
        && let Some(reasoning) = body.get("reasoning")
    {
        let effort = reasoning
            .get("effort")
            .and_then(Value::as_str)
            .unwrap_or("none");
        if matches!(effort, "none" | "off" | "disabled") {
            result.insert(String::from("thinking"), json!({"type":"disabled"}));
        } else {
            let mapped = canonical_reasoning_effort(effort).ok_or_else(|| {
                LlmCoreError::new(format!(
                    "Unsupported Anthropic reasoning effort `{effort}`."
                ))
            })?;
            let budget = anthropic_thinking_budget(mapped, max_tokens)?;
            result.insert(
                String::from("thinking"),
                json!({"type":"enabled","budget_tokens":budget}),
            );
        }
    }
    let thinking_disabled = result
        .get("thinking")
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        == Some("disabled");
    if thinking_disabled {
        if let Some(value) = body.get("temperature") {
            result.insert(String::from("temperature"), value.clone());
        }
        if let Some(value) = body.get("top_p") {
            result.insert(String::from("top_p"), value.clone());
        }
    }
    Ok(())
}

/// Derives a bounded Anthropic thinking budget while reserving visible output capacity.
/// 推导有界 Anthropic 思考预算，并为可见输出保留容量。
fn anthropic_thinking_budget(effort: &str, max_tokens: u64) -> Result<u64, LlmCoreError> {
    let requested = match effort {
        "minimal" => 1024,
        "low" => 2048,
        "medium" => 4096,
        "high" => 8192,
        "xhigh" | "max" => 16384,
        other => {
            return Err(LlmCoreError::new(format!(
                "Anthropic reasoning effort `{other}` has no registered budget mapping."
            )));
        }
    };
    let budget = requested.min(max_tokens / 2);
    if budget < 1024 {
        return Err(LlmCoreError::new(
            "Anthropic reasoning requires at least 1024 tokens after reserving output capacity.",
        ));
    }
    Ok(budget)
}

/// Merges adjacent blocks with the same role and guarantees Anthropic's user-first boundary.
/// 合并相邻同角色块，并保证 Anthropic 的 user-first 边界。
fn normalize_anthropic_messages(messages: &mut Vec<Value>) -> Result<(), LlmCoreError> {
    let mut normalized: Vec<Value> = Vec::new();
    for message in messages.drain(..) {
        let role = message.get("role").and_then(Value::as_str).ok_or_else(|| {
            LlmCoreError::new("Anthropic message is missing `role` after conversion.")
        })?;
        let content = message.get("content").cloned().unwrap_or_else(|| json!([]));
        if let Some(previous) = normalized.last_mut()
            && previous.get("role").and_then(Value::as_str) == Some(role)
        {
            let previous_content = previous
                .get_mut("content")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| LlmCoreError::new("Anthropic message content is not an array."))?;
            let current_content = content
                .as_array()
                .ok_or_else(|| LlmCoreError::new("Anthropic message content is not an array."))?;
            previous_content.extend(current_content.iter().cloned());
        } else {
            normalized.push(json!({"role":role,"content":content}));
        }
    }
    if normalized.is_empty() {
        normalized.push(json!({"role":"user","content":[{"type":"text","text":""}]}));
    } else if normalized[0].get("role").and_then(Value::as_str) != Some("user") {
        normalized.insert(
            0,
            json!({"role":"user","content":[{"type":"text","text":""}]}),
        );
    }
    *messages = normalized;
    Ok(())
}

/// Appends one block and merges it with the previous same-role message.
/// 追加一个内容块，并将其合并到前一个同角色消息中。
fn push_anthropic_block(messages: &mut Vec<Value>, role: &str, block: Value) {
    if let Some(previous) = messages.last_mut()
        && previous.get("role").and_then(Value::as_str) == Some(role)
        && let Some(content) = previous.get_mut("content").and_then(Value::as_array_mut)
    {
        content.push(block);
        return;
    }
    messages.push(json!({"role":role,"content":[block]}));
}

/// Serializes one JSON value without changing its semantic object shape.
/// 序列化一个 JSON 值且不改变其语义对象形状。
fn canonical_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| String::from("null"))
}
