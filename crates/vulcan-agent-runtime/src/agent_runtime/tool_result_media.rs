//! Strict MCP tool-result media normalization shared by execution and provider projection.
//! 供执行层与 Provider 投影层共享的严格 MCP 工具结果媒体规范化逻辑。

use base64::Engine;
use serde_json::{Map, Value, json};

/// Version marker for the durable tool-result media envelope.
/// 可持久化工具结果媒体 envelope 的版本标记。
pub(crate) const TOOL_RESULT_MEDIA_ENVELOPE_VERSION: u64 = 1;

/// Marker field that distinguishes a media envelope from an ordinary tool JSON object.
/// 区分媒体 envelope 与普通工具 JSON 对象的标记字段。
const TOOL_RESULT_MEDIA_MARKER: &str = "$vulcan_tool_result_media";

/// MCP image payload limit shared with the MCP transport boundary.
/// 与 MCP 传输边界一致的单张 MCP 图片载荷上限。
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// Holds one validated, ordered MCP tool-result media envelope.
/// 保存一份经过校验且保留顺序的 MCP 工具结果媒体 envelope。
#[derive(Debug, Clone, PartialEq)]
struct ToolResultMediaEnvelope {
    /// Ordered text and image blocks returned by the MCP server.
    /// MCP 服务端返回的有序文本与图片块。
    content: Vec<ToolResultMediaBlock>,
    /// Optional structured result returned beside the ordered content blocks.
    /// 伴随有序内容块返回的可选结构化结果。
    structured_content: Option<Value>,
}

/// Holds one validated content block from the MCP media envelope.
/// 保存 MCP 媒体 envelope 中的一份经过校验的内容块。
#[derive(Debug, Clone, PartialEq)]
enum ToolResultMediaBlock {
    /// One textual result block.
    /// 一份文本结果块。
    Text(String),
    /// One Base64-encoded image result block with an exact registered MIME type.
    /// 一份带有精确注册 MIME 类型的 Base64 图片结果块。
    Image { data: String, mime_type: String },
}

/// Converts one host-managed MCP media result into a durable, versioned model-output string.
/// 将一份宿主管理的 MCP 媒体结果转换为可持久化且带版本的模型输出字符串。
///
/// Ordinary tool values pass through unchanged. A value is treated as an MCP media envelope only
/// when its content array contains an image block; malformed media is rejected explicitly.
/// 普通工具值保持原样。只有内容数组包含图片块时才识别为 MCP 媒体 envelope；畸形媒体会明确拒绝。
///
/// # Parameters
/// # 参数
/// `value` is the model output returned by one host tool invocation.
/// `value` 是一次宿主工具调用返回的模型输出。
///
/// # Returns
/// # 返回值
/// The original value for ordinary results, or a marked JSON string for validated media results.
/// 普通结果返回原值，经过校验的媒体结果返回带标记的 JSON 字符串。
pub(crate) fn canonicalize_tool_model_output(value: Value) -> Result<Value, String> {
    let Value::Object(object) = &value else {
        return Ok(value);
    };
    let Some(content) = object.get("content").and_then(Value::as_array) else {
        return Ok(value);
    };
    if !content.iter().any(is_image_block) {
        return Ok(value);
    }

    let envelope = parse_unmarked_mcp_media_object(object)?;
    Ok(Value::String(serialize_media_envelope(&envelope)?))
}

/// Appends a model-facing notice while preserving ordered native media and original business data.
/// 追加面向模型的提示，同时保留有序原生媒体和原始业务数据。
/// Accepts an upstream result and notice; returns readable text or a validated media envelope.
/// 接收上游结果和提示，返回可读正文或已校验媒体封套。
pub(crate) fn append_tool_result_notice(value: Value, notice: &str) -> Result<Value, String> {
    let normalized = canonicalize_tool_model_output(value)?;
    match normalized {
        Value::String(text) => {
            if let Some(updated) = crate::native_media_input::append_notice(&text, notice)? {
                return Ok(Value::String(updated));
            }
            if let Some(mut envelope) = parse_marked_media_envelope(&text)? {
                envelope
                    .content
                    .push(ToolResultMediaBlock::Text(notice.to_owned()));
                return serialize_media_envelope(&envelope).map(Value::String);
            }
            Ok(Value::String(format!("{text}\n\n{notice}")))
        }
        business_data => {
            let source = serde_json::to_string_pretty(&business_data)
                .map_err(|error| format!("Could not encode the completed tool result: {error}"))?;
            Ok(Value::String(format!(
                "# Tool result\n\n{}\n\n{notice}",
                vulcan_tools_manager::result_markdown::fenced_code(&source)
            )))
        }
    }
}

/// Projects one durable tool-result string into provider-native tool-result content.
/// 将一份可持久化工具结果字符串投影为 Provider 原生工具结果内容。
///
/// # Parameters
/// # 参数
/// `text` is the exact Core-persisted model-output string and `protocol_profile_id` identifies
/// the frozen provider encoder that will receive it.
/// `text` 是 Core 精确持久化的模型输出字符串，`protocol_profile_id` 标识接收它的冻结 Provider 编码器。
///
/// # Returns
/// # 返回值
/// Ordinary text for ordinary results, native ordered content for supported image routes, or an
/// explicit unsupported/malformed-media error before a provider request is sent.
/// 普通结果返回文本，支持图片的路由返回原生有序内容，不支持或畸形媒体则在发送前返回明确错误。
pub(crate) fn project_tool_result_content(
    text: &str,
    protocol_profile_id: &str,
) -> Result<Value, String> {
    let Some(envelope) = parse_marked_media_envelope(text)? else {
        return Ok(Value::String(text.to_string()));
    };
    let project = media_projector(protocol_profile_id).ok_or_else(|| {
        format!(
            "Tool result image blocks are unsupported by protocol profile `{protocol_profile_id}`; use an OpenAI Responses or Anthropic Messages route."
        )
    })?;
    project(&envelope)
}

/// Native projector of tool-result media for one protocol profile.
/// 一个协议 Profile 的工具结果媒体原生投影器。
type MediaProjector = fn(&ToolResultMediaEnvelope) -> Result<Value, String>;

/// Returns the native media projector of one protocol profile, if it has one.
/// 返回一个协议 Profile 的原生媒体投影器（如有）。
fn media_projector(protocol_profile_id: &str) -> Option<MediaProjector> {
    match protocol_profile_id {
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID => {
            Some(project_openai_responses_content)
        }
        vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID
        | vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => Some(project_anthropic_content),
        _ => None,
    }
}

/// Returns whether one protocol profile receives tool-result image blocks natively.
/// 返回一个协议 Profile 是否原生接收工具结果图片块。
pub(crate) fn protocol_projects_tool_result_media(protocol_profile_id: &str) -> bool {
    media_projector(protocol_profile_id).is_some()
}

/// Returns whether one Base64 image payload passes the tool-result media validation.
/// 返回一份 Base64 图片载荷能否通过工具结果媒体校验。
pub(crate) fn tool_result_image_is_valid(data: &str, mime_type: &str) -> bool {
    validate_image_content(data, mime_type).is_ok()
}

/// Builds a bounded text preview for request budgeting without copying Base64 image bytes into it.
/// 构建用于请求预算的有界文本预览，避免把 Base64 图片字节复制进预算文本。
///
/// # Parameters
/// # 参数
/// `text` is the exact persisted tool-result string.
/// `text` 是精确持久化的工具结果字符串。
///
/// # Returns
/// # 返回值
/// A textual preview for media results, or the original text for ordinary results.
/// 媒体结果返回文本预览，普通结果返回原始文本。
pub(crate) fn tool_result_text_preview(text: &str) -> Result<String, String> {
    if let Some(preview) = crate::native_media_input::text_preview(text)? {
        return Ok(preview);
    }
    let Some(envelope) = parse_marked_media_envelope(text)? else {
        return Ok(text.to_string());
    };
    let mut parts = Vec::new();
    for block in &envelope.content {
        match block {
            ToolResultMediaBlock::Text(value) => parts.push(value.clone()),
            ToolResultMediaBlock::Image { mime_type, .. } => {
                parts.push(format!("[image content: {mime_type}]"));
            }
        }
    }
    if let Some(structured_content) = envelope.structured_content.as_ref() {
        parts.push(serde_json::to_string(structured_content).map_err(|error| {
            format!("Tool result structured content could not be encoded: {error}")
        })?);
    }
    Ok(parts.join("\n"))
}

/// Parses the exact marked envelope and rejects malformed versions instead of silently degrading.
/// 解析精确的带标记 envelope，发现版本或结构错误时明确拒绝而不是静默降级。
fn parse_marked_media_envelope(text: &str) -> Result<Option<ToolResultMediaEnvelope>, String> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Ok(None);
    };
    let Value::Object(object) = value else {
        return Ok(None);
    };
    if !object.contains_key(TOOL_RESULT_MEDIA_MARKER) {
        return Ok(None);
    }
    parse_marked_media_object(&object).map(Some)
}

/// Parses one exact marked media object with no compatibility aliases or fallback fields.
/// 解析一份精确的带标记媒体对象，不接受兼容别名或备用字段。
fn parse_marked_media_object(
    object: &Map<String, Value>,
) -> Result<ToolResultMediaEnvelope, String> {
    if object.keys().any(|key| {
        key != TOOL_RESULT_MEDIA_MARKER && key != "content" && key != "structured_content"
    }) {
        return Err(String::from(
            "Tool result media envelope contains an unsupported field.",
        ));
    }
    let version = object
        .get(TOOL_RESULT_MEDIA_MARKER)
        .and_then(Value::as_u64)
        .ok_or_else(|| String::from("Tool result media envelope has an invalid version."))?;
    if version != TOOL_RESULT_MEDIA_ENVELOPE_VERSION {
        return Err(format!(
            "Tool result media envelope version `{version}` is unsupported."
        ));
    }
    let content = object
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            String::from("Tool result media envelope is missing an ordered content array.")
        })?;
    let content = content
        .iter()
        .map(parse_media_block)
        .collect::<Result<Vec<_>, _>>()?;
    if content.is_empty() {
        return Err(String::from(
            "Tool result media envelope content cannot be empty.",
        ));
    }
    Ok(ToolResultMediaEnvelope {
        content,
        structured_content: object.get("structured_content").cloned(),
    })
}

/// Parses the pre-marker MCP object emitted by the rmcp support boundary.
/// 解析 rmcp 支持边界生成的带标记前 MCP 对象。
fn parse_unmarked_mcp_media_object(
    object: &Map<String, Value>,
) -> Result<ToolResultMediaEnvelope, String> {
    if object
        .keys()
        .any(|key| key != "content" && key != "structured_content")
    {
        return Err(String::from(
            "MCP media result contains an unsupported envelope field.",
        ));
    }
    let content = object
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("MCP media result is missing its content array."))?;
    let content = content
        .iter()
        .map(parse_media_block)
        .collect::<Result<Vec<_>, _>>()?;
    if content.is_empty() {
        return Err(String::from("MCP media result content cannot be empty."));
    }
    if !content
        .iter()
        .any(|block| matches!(block, ToolResultMediaBlock::Image { .. }))
    {
        return Err(String::from(
            "MCP media result does not contain an image block.",
        ));
    }
    Ok(ToolResultMediaEnvelope {
        content,
        structured_content: object.get("structured_content").cloned(),
    })
}

/// Parses one exact text or image block and validates every image byte before projection.
/// 解析一份精确的文本或图片块，并在投影前校验每个图片字节。
fn parse_media_block(value: &Value) -> Result<ToolResultMediaBlock, String> {
    let object = value
        .as_object()
        .ok_or_else(|| String::from("MCP media result content block must be an object."))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("MCP media result content block is missing its type."))?;
    match kind {
        "text" => {
            if object.keys().any(|key| key != "type" && key != "text") {
                return Err(String::from(
                    "MCP text result block contains an unsupported field.",
                ));
            }
            let text = object
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("MCP text result block is missing text."))?;
            Ok(ToolResultMediaBlock::Text(text.to_string()))
        }
        "image" => {
            if object
                .keys()
                .any(|key| key != "type" && key != "data" && key != "mimeType")
            {
                return Err(String::from(
                    "MCP image result block contains an unsupported field.",
                ));
            }
            let data = object
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("MCP image result block is missing Base64 data."))?;
            let mime_type = object
                .get("mimeType")
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("MCP image result block is missing its MIME type."))?;
            validate_image_content(data, mime_type)?;
            Ok(ToolResultMediaBlock::Image {
                data: data.to_string(),
                mime_type: mime_type.to_string(),
            })
        }
        other => Err(format!(
            "MCP media result content type `{other}` is unsupported."
        )),
    }
}

/// Returns whether one JSON block is explicitly an image block.
/// 判断一个 JSON 内容块是否明确声明为图片块。
fn is_image_block(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("image")
}

/// Serializes one validated envelope while retaining its ordered content and structured result.
/// 序列化一份经过校验的 envelope，同时保留内容顺序与结构化结果。
fn serialize_media_envelope(envelope: &ToolResultMediaEnvelope) -> Result<String, String> {
    let content = envelope
        .content
        .iter()
        .map(|block| match block {
            ToolResultMediaBlock::Text(text) => json!({"type": "text", "text": text}),
            ToolResultMediaBlock::Image { data, mime_type } => {
                json!({"type": "image", "data": data, "mimeType": mime_type})
            }
        })
        .collect::<Vec<_>>();
    let mut object = Map::new();
    object.insert(
        String::from(TOOL_RESULT_MEDIA_MARKER),
        Value::from(TOOL_RESULT_MEDIA_ENVELOPE_VERSION),
    );
    object.insert(String::from("content"), Value::Array(content));
    if let Some(structured_content) = envelope.structured_content.as_ref() {
        object.insert(
            String::from("structured_content"),
            structured_content.clone(),
        );
    }
    serde_json::to_string(&Value::Object(object))
        .map_err(|error| format!("Tool result media envelope could not be serialized: {error}"))
}

/// Converts validated content into the OpenAI Responses function-output shape.
/// 将经过校验的内容转换为 OpenAI Responses 函数输出结构。
fn project_openai_responses_content(envelope: &ToolResultMediaEnvelope) -> Result<Value, String> {
    let mut content = Vec::with_capacity(envelope.content.len() + 1);
    for block in &envelope.content {
        match block {
            ToolResultMediaBlock::Text(text) => {
                content.push(json!({"type": "input_text", "text": text}));
            }
            ToolResultMediaBlock::Image { data, mime_type } => {
                content.push(json!({
                    "type": "input_image",
                    "image_url": format!("data:{mime_type};base64,{data}"),
                }));
            }
        }
    }
    append_structured_content_text(
        &mut content,
        envelope.structured_content.as_ref(),
        "input_text",
    )?;
    Ok(Value::Array(content))
}

/// Converts validated content into the Anthropic Messages tool-result block shape.
/// 将经过校验的内容转换为 Anthropic Messages 工具结果块结构。
fn project_anthropic_content(envelope: &ToolResultMediaEnvelope) -> Result<Value, String> {
    let mut content = Vec::with_capacity(envelope.content.len() + 1);
    for block in &envelope.content {
        match block {
            ToolResultMediaBlock::Text(text) => {
                content.push(json!({"type": "text", "text": text}));
            }
            ToolResultMediaBlock::Image { data, mime_type } => {
                content.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": mime_type,
                        "data": data,
                    },
                }));
            }
        }
    }
    append_structured_content_text(&mut content, envelope.structured_content.as_ref(), "text")?;
    Ok(Value::Array(content))
}

/// Appends structured content as an ordered text block without altering its JSON value.
/// 将结构化内容作为有序文本块追加，同时不改变其 JSON 值。
fn append_structured_content_text(
    content: &mut Vec<Value>,
    structured_content: Option<&Value>,
    block_type: &str,
) -> Result<(), String> {
    if let Some(value) = structured_content {
        content.push(json!({
            "type": block_type,
            "text": serde_json::to_string(value).map_err(|error| {
                format!("Tool result structured content could not be encoded: {error}")
            })?,
        }));
    }
    Ok(())
}

/// Validates one image MIME/data pair with the same strict byte limit as the MCP boundary.
/// 使用与 MCP 边界一致的严格字节上限校验一组图片 MIME/数据。
fn validate_image_content(data: &str, mime_type: &str) -> Result<(), String> {
    if !matches!(mime_type, "image/png" | "image/jpeg" | "image/webp") {
        return Err(String::from(
            "MCP image content uses an unsupported MIME type.",
        ));
    }
    if data.is_empty() || !data.len().is_multiple_of(4) {
        return Err(String::from("MCP image content has invalid Base64 data."));
    }
    let max_encoded_len = MAX_IMAGE_BYTES.div_ceil(3) * 4;
    if data.len() > max_encoded_len {
        return Err(String::from("MCP image content exceeds the 20 MiB limit."));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| String::from("MCP image content has invalid Base64 data."))?;
    if decoded.is_empty() {
        return Err(String::from("MCP image content has empty Base64 data."));
    }
    if decoded.len() > MAX_IMAGE_BYTES {
        return Err(String::from("MCP image content exceeds the 20 MiB limit."));
    }
    if base64::engine::general_purpose::STANDARD.encode(decoded) != data {
        return Err(String::from("MCP image content has invalid Base64 data."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves post-processing notices preserve media order and a single copy of image bytes.
    /// 证明后置提示保留媒体顺序和唯一一份图片字节。
    #[test]
    fn postprocessing_notice_preserves_native_media_and_literal_json_text() {
        let original = mcp_media_result();
        let marked = canonicalize_tool_model_output(original.clone()).unwrap();
        for input in [original, marked] {
            let result =
                append_tool_result_notice(input, "Already completed; do not retry.").unwrap();
            let text = result.as_str().unwrap();
            assert_eq!(text.matches("iVBORw0KGgo=").count(), 1);
            let projected =
                project_tool_result_content(text, vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID)
                    .unwrap();
            let blocks = projected.as_array().unwrap();
            assert!(blocks.iter().any(|block| block["type"] == "input_image"));
            let texts: Vec<&str> = blocks
                .iter()
                .filter_map(|block| block["text"].as_str())
                .collect();
            assert_eq!(
                texts,
                vec![
                    "before",
                    "after",
                    "Already completed; do not retry.",
                    "{\"status\":\"ok\"}"
                ]
            );
        }
        let literal = "{\"content\":\"original-source\"}";
        let result =
            append_tool_result_notice(Value::String(literal.into()), "Completed.").unwrap();
        assert_eq!(result.as_str().unwrap(), format!("{literal}\n\nCompleted."));
    }

    /// Builds one small MCP result containing ordered text, image, and structured content.
    /// 构造一份包含有序文本、图片与结构化内容的小型 MCP 结果。
    fn mcp_media_result() -> Value {
        json!({
            "content": [
                {"type": "text", "text": "before"},
                {"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"},
                {"type": "text", "text": "after"}
            ],
            "structured_content": {"status": "ok"}
        })
    }

    /// Verifies the durable form is marked and the original block order survives projection.
    /// 验证可持久化形式带有标记，且原始块顺序在投影后保持不变。
    #[test]
    fn media_output_round_trips_to_openai_responses_content() {
        let output = canonicalize_tool_model_output(mcp_media_result())
            .expect("valid MCP media should canonicalize");
        let text = output.as_str().expect("media output should be a string");
        assert!(text.contains(TOOL_RESULT_MEDIA_MARKER));
        let projected =
            project_tool_result_content(text, vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID)
                .expect("OpenAI Responses media should project");
        assert_eq!(projected[0], json!({"type":"input_text","text":"before"}));
        assert_eq!(projected[1]["type"], "input_image");
        assert_eq!(
            projected[1]["image_url"],
            "data:image/png;base64,iVBORw0KGgo="
        );
        assert_eq!(projected[2], json!({"type":"input_text","text":"after"}));
        assert_eq!(projected[3]["text"], "{\"status\":\"ok\"}");
    }

    /// Verifies Anthropic receives its native base64 image source instead of a JSON envelope.
    /// 验证 Anthropic 收到原生 Base64 图片 source，而不是 JSON envelope。
    #[test]
    fn media_output_projects_to_anthropic_image_source() {
        let output = canonicalize_tool_model_output(mcp_media_result())
            .expect("valid MCP media should canonicalize");
        let projected = project_tool_result_content(
            output.as_str().expect("media output should be a string"),
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
        )
        .expect("Anthropic media should project");
        assert_eq!(projected[1]["type"], "image");
        assert_eq!(projected[1]["source"]["type"], "base64");
        assert_eq!(projected[1]["source"]["media_type"], "image/png");
        assert_eq!(projected[1]["source"]["data"], "iVBORw0KGgo=");
    }

    /// Verifies unsupported providers fail before image bytes can be serialized as ordinary text.
    /// 验证不支持的 Provider 会在图片字节被当作普通文本序列化前失败。
    #[test]
    fn media_output_rejects_unsupported_provider_profile() {
        let output = canonicalize_tool_model_output(mcp_media_result())
            .expect("valid MCP media should canonicalize");
        let error = project_tool_result_content(
            output.as_str().expect("media output should be a string"),
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
        )
        .expect_err("OpenAI Chat has no registered tool-result image projector");
        assert!(error.contains("unsupported by protocol profile"));
    }

    /// Verifies malformed image data and unregistered MIME types are rejected.
    /// 验证畸形图片数据与未注册 MIME 类型会被拒绝。
    #[test]
    fn media_output_rejects_invalid_image_payload() {
        let invalid_mime = json!({
            "content": [{"type":"image","data":"iVBORw0KGgo=","mimeType":"image/gif"}]
        });
        assert!(canonicalize_tool_model_output(invalid_mime).is_err());

        let invalid_base64 = json!({
            "content": [{"type":"image","data":"not-base64=","mimeType":"image/png"}]
        });
        assert!(canonicalize_tool_model_output(invalid_base64).is_err());
    }

    /// Verifies an envelope from an unknown contract version is refused instead of being reinterpreted.
    /// 验证来自未知契约版本的信封会被拒绝，而不是被重新解释。
    ///
    /// # Notes
    /// 说明
    /// The version is read before any content block is parsed, so a future version must fail on the
    /// version boundary itself and must never be treated as the current shape. A future version is used
    /// rather than a past one because this contract is unreleased and keeps no historical form.
    /// 版本在任何内容块被解析之前读取，因此未来版本必须在版本边界本身失败，绝不能被当成当前形态。
    /// 这里取未来版本而非过去版本，因为本契约尚未发布、不保留历史形态。
    #[test]
    fn media_output_rejects_unknown_envelope_version() {
        let canonical = canonicalize_tool_model_output(mcp_media_result())
            .expect("valid MCP media should canonicalize");
        let mut envelope: Value =
            serde_json::from_str(canonical.as_str().expect("media output should be a string"))
                .expect("the durable envelope must be JSON");
        // The declared version of the exact canonical bytes, before any mutation.
        // 精确规范字节在被修改之前所声明的版本。
        let declared = envelope[TOOL_RESULT_MEDIA_MARKER].clone();
        // Control: these exact bytes at their declared version do project, so a later refusal is
        // attributable to the version and not to the fixture being malformed.
        // 对照：这些精确字节在其声明版本下确实可以投影，因此后续拒绝可归因于版本而非夹具本身畸形。
        assert!(
            project_tool_result_content(
                envelope.to_string().as_str(),
                vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID,
            )
            .is_ok(),
            "the unmodified canonical envelope must project"
        );
        envelope[TOOL_RESULT_MEDIA_MARKER] = Value::from(TOOL_RESULT_MEDIA_ENVELOPE_VERSION + 1);
        let error = project_tool_result_content(
            envelope.to_string().as_str(),
            vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID,
        )
        .expect_err("an unknown envelope version must be refused");
        assert!(
            error.contains("unsupported"),
            "the refusal must name the version boundary: {error}"
        );
        assert_ne!(
            declared, envelope[TOOL_RESULT_MEDIA_MARKER],
            "the mutation must actually change the declared version"
        );
    }
}
