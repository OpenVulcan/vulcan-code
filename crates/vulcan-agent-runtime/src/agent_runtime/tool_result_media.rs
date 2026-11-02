//! Saves MCP tool-result images as session-owned files and tells the model where they are.
//! 将 MCP 工具结果图片保存为会话自有文件，并告知模型其位置。

use crate::attachment_asset_index::retain_generated_attachments_for_session;
use crate::attachment_assets::{attachment_descriptor_from_mime_type, stage_attachment_bytes};
use base64::Engine;
use serde_json::{Map, Value};
use std::path::Path;
use vulcan_core::types::ConversationAttachment;

/// MCP image payload limit shared with the MCP transport boundary.
/// 与 MCP 传输边界一致的单张 MCP 图片载荷上限。
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// Image MIME types admitted in MCP tool results.
/// MCP 工具结果准入的图片 MIME 类型。
const IMAGE_MIME_TYPES: &[&str] = &["image/png", "image/jpeg", "image/webp"];

/// One validated content block of an MCP media result.
/// MCP 媒体结果中的一份经过校验的内容块。
enum ToolResultMediaBlock {
    /// One textual result block.
    /// 一份文本结果块。
    Text(String),
    /// One decoded image with its exact registered MIME type.
    /// 一张带精确注册 MIME 类型的已解码图片。
    Image { bytes: Vec<u8>, mime_type: String },
}

/// Returns whether a structured tool output carries MCP image blocks.
/// 判断结构化工具输出是否携带 MCP 图片块。
pub(crate) fn has_image_blocks(model_output: &Value) -> bool {
    model_output
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|content| content.iter().any(is_image_block))
}

/// Saves every image of one MCP media result and replaces it with a text notice naming the saved file.
/// 保存一份 MCP 媒体结果中的每张图片，并以指明已保存文件的文本说明替换它。
///
/// Images never enter the model input; the model reads one with `read_media` when it needs to see it.
/// 图片从不进入模型输入；模型需要查看时通过 `read_media` 读取。
///
/// # Parameters
/// # 参数
/// `model_output` is the unmarked tool output, `workspace_root` owns the managed attachment store,
/// and `session_id` is the conversation that receives ownership of the saved images.
/// `model_output` 是未标记的工具输出，`workspace_root` 持有托管附件存储，`session_id` 是获得已保存图片
/// 所有权的会话。
///
/// # Returns
/// # 返回值
/// The output unchanged with no attachments when it has no image block; otherwise the readable text
/// output and the saved attachments in content order, or the first validation or staging error.
/// 无图片块时原样返回输出且无附件；否则返回可读文本输出及按内容顺序排列的已保存附件，或首个校验/暂存错误。
pub(crate) async fn save_tool_result_images(
    model_output: Value,
    workspace_root: Option<&Path>,
    session_id: &str,
) -> Result<(Value, Vec<ConversationAttachment>), String> {
    if !has_image_blocks(&model_output) {
        return Ok((model_output, Vec::new()));
    }
    let Value::Object(object) = model_output else {
        return Err(String::from("MCP media result must be an object."));
    };
    let (blocks, structured_content) = parse_mcp_media_object(object)?;
    let workspace_root = workspace_root.ok_or_else(|| {
        String::from("Saving tool result images requires an active workspace root.")
    })?;
    let workspace_text = vulcan_runtime_assets::native_host_path_text_from_path(workspace_root);
    let session_id = session_id.to_owned();
    tokio::task::spawn_blocking(move || {
        save_blocks(
            workspace_text.as_str(),
            session_id.as_str(),
            blocks,
            structured_content,
        )
    })
    .await
    .map_err(|error| format!("Tool result image saving worker failed: {error}"))?
}

/// Stages ordered image blocks, binds them to the session, and renders the model-facing text.
/// 暂存有序图片块、将其绑定到会话，并渲染面向模型的文本。
///
/// Returns the text output and saved attachments; any failure discards every staged image.
/// 返回文本输出及已保存附件；任何失败都会丢弃全部已暂存图片。
fn save_blocks(
    workspace_path: &str,
    session_id: &str,
    blocks: Vec<ToolResultMediaBlock>,
    structured_content: Option<Value>,
) -> Result<(Value, Vec<ConversationAttachment>), String> {
    let mut attachments = Vec::new();
    let mut parts = Vec::with_capacity(blocks.len() + 1);
    let saved: Result<(), String> = (|| {
        for block in blocks {
            match block {
                ToolResultMediaBlock::Text(text) => parts.push(text),
                ToolResultMediaBlock::Image { bytes, mime_type } => {
                    let descriptor = attachment_descriptor_from_mime_type(mime_type.as_str())
                        .ok_or_else(|| {
                            format!("MCP image MIME type `{mime_type}` is unsupported.")
                        })?;
                    let file_name = format!(
                        "tool-image-{}.{}",
                        attachments.len() + 1,
                        descriptor.extension
                    );
                    let attachment = stage_attachment_bytes(
                        workspace_path,
                        &descriptor,
                        file_name.as_str(),
                        None,
                        bytes.as_slice(),
                    )?;
                    parts.push(saved_image_notice(&attachment, bytes.as_slice())?);
                    attachments.push(attachment);
                }
            }
        }
        retain_generated_attachments_for_session(workspace_path, session_id, &attachments)
    })();
    if let Err(error) = saved {
        return Err(crate::provider_tool_bridge::rollback_staged_resources(
            workspace_path,
            attachments.as_slice(),
            error,
        ));
    }
    if let Some(structured_content) = structured_content {
        let source = serde_json::to_string_pretty(&structured_content).map_err(|error| {
            format!("Tool result structured content could not be encoded: {error}")
        })?;
        parts.push(format!(
            "## Structured result\n\n{}",
            vulcan_tools_manager::result_markdown::fenced_code(&source)
        ));
    }
    Ok((Value::String(parts.join("\n\n")), attachments))
}

/// Describes one saved image to the model without including its bytes.
/// 向模型描述一张已保存图片，不包含其字节。
///
/// Returns the notice naming the locator, file, MIME type, pixel size, and byte size.
/// 返回指明定位符、文件、MIME 类型、像素尺寸与字节大小的说明。
fn saved_image_notice(attachment: &ConversationAttachment, bytes: &[u8]) -> Result<String, String> {
    let asset_id = attachment.asset_id.as_deref().ok_or_else(|| {
        String::from("Saved tool result image is missing its managed asset identifier.")
    })?;
    // An unreadable header only omits the pixel size; the bytes were already validated as Base64.
    // 无法读取文件头只会省略像素尺寸；字节已通过 Base64 校验。
    let size = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
        .map(|(width, height)| format!(", {width}x{height} px"))
        .unwrap_or_default();
    Ok(format!(
        "[Image saved as `attachment://{asset_id}` ({}{size}, {} bytes; file `{}`). It is shown to the user but not in your input; call `read_media` with this locator to view it.]",
        attachment.mime_type,
        bytes.len(),
        attachment.local_path
    ))
}

/// Appends a model-facing notice while keeping image blocks for the saving step that follows.
/// 追加面向模型的提示，同时为随后的保存步骤保留图片块。
/// Accepts an upstream result and notice; returns readable text or the media object with the notice appended.
/// 接收上游结果和提示，返回可读正文或已追加提示的媒体对象。
pub(crate) fn append_tool_result_notice(value: Value, notice: &str) -> Result<Value, String> {
    match value {
        Value::String(text) => {
            if let Some(updated) = crate::native_media_input::append_notice(&text, notice)? {
                return Ok(Value::String(updated));
            }
            Ok(Value::String(format!("{text}\n\n{notice}")))
        }
        mut media if has_image_blocks(&media) => {
            media["content"]
                .as_array_mut()
                .expect("image blocks live in a content array")
                .push(serde_json::json!({"type": "text", "text": notice}));
            Ok(media)
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

/// Builds a bounded text preview for request budgeting without copying media bytes into it.
/// 构建用于请求预算的有界文本预览，避免把媒体字节复制进预算文本。
///
/// Returns the preview of a `read_media` receipt, or the original text for ordinary results.
/// 返回 `read_media` 回执的预览；普通结果返回原始文本。
pub(crate) fn tool_result_text_preview(text: &str) -> Result<String, String> {
    Ok(crate::native_media_input::text_preview(text)?.unwrap_or_else(|| text.to_string()))
}

/// Parses the MCP media object emitted by the rmcp support boundary, decoding every image.
/// 解析 rmcp 支持边界生成的 MCP 媒体对象，并解码每张图片。
fn parse_mcp_media_object(
    mut object: Map<String, Value>,
) -> Result<(Vec<ToolResultMediaBlock>, Option<Value>), String> {
    if object
        .keys()
        .any(|key| key != "content" && key != "structured_content")
    {
        return Err(String::from(
            "MCP media result contains an unsupported envelope field.",
        ));
    }
    let content = object
        .remove("content")
        .and_then(|content| match content {
            Value::Array(content) => Some(content),
            _ => None,
        })
        .ok_or_else(|| String::from("MCP media result is missing its content array."))?;
    let blocks = content
        .iter()
        .map(parse_media_block)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((blocks, object.remove("structured_content")))
}

/// Parses one exact text or image block and decodes every image byte.
/// 解析一份精确的文本或图片块，并解码每个图片字节。
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
            Ok(ToolResultMediaBlock::Image {
                bytes: decode_image_content(data, mime_type)?,
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

/// Decodes one image MIME/data pair with the same strict byte limit as the MCP boundary.
/// 使用与 MCP 边界一致的严格字节上限解码一组图片 MIME/数据。
fn decode_image_content(data: &str, mime_type: &str) -> Result<Vec<u8>, String> {
    if !IMAGE_MIME_TYPES.contains(&mime_type) {
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
    if base64::engine::general_purpose::STANDARD.encode(&decoded) != data {
        return Err(String::from("MCP image content has invalid Base64 data."));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Encodes one small real PNG so staging accepts it as an image.
    /// 编码一张小型真实 PNG，使暂存将其识别为图片。
    fn png_base64(width: u32, height: u32) -> String {
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(width, height))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    /// Proves images are saved for the session and the model gets only ordered text naming them.
    /// 证明图片为会话保存，模型只获得按顺序指明它们的文本。
    #[tokio::test]
    async fn images_are_saved_and_replaced_by_locator_notices() {
        let workspace = tempfile::tempdir().unwrap();
        let (output, attachments) = save_tool_result_images(
            json!({
                "content": [
                    {"type": "text", "text": "before"},
                    {"type": "image", "data": png_base64(3, 2), "mimeType": "image/png"},
                    {"type": "text", "text": "after"}
                ],
                "structured_content": {"status": "ok"}
            }),
            Some(workspace.path()),
            "session-1",
        )
        .await
        .unwrap();
        assert_eq!(attachments.len(), 1);
        let attachment = &attachments[0];
        assert!(Path::new(&attachment.local_path).is_file());
        let text = output.as_str().unwrap();
        let locator = format!("attachment://{}", attachment.asset_id.as_deref().unwrap());
        let (before, rest) = text.split_once(locator.as_str()).unwrap();
        assert!(before.starts_with("before"));
        assert!(rest.contains("3x2 px"));
        assert!(rest.contains("read_media"));
        assert!(rest.contains("after"));
        assert!(rest.contains("\"status\": \"ok\""));
    }

    /// Proves ordinary outputs pass through and malformed images fail before anything is saved.
    /// 证明普通输出原样通过，畸形图片在保存任何内容之前失败。
    #[tokio::test]
    async fn ordinary_outputs_pass_and_invalid_images_fail() {
        let workspace = tempfile::tempdir().unwrap();
        let plain = json!({"content": [{"type": "text", "text": "only text"}]});
        let (output, attachments) =
            save_tool_result_images(plain.clone(), Some(workspace.path()), "session-1")
                .await
                .unwrap();
        assert_eq!(output, plain);
        assert!(attachments.is_empty());

        for (image, expected) in [
            (
                json!({"type": "image", "data": "not-base64=", "mimeType": "image/png"}),
                "Base64",
            ),
            (
                json!({"type": "image", "data": png_base64(1, 1), "mimeType": "image/gif"}),
                "MIME",
            ),
            (
                json!({"type": "image", "data": png_base64(1, 1), "mimeType": "image/png", "x": 1}),
                "unsupported field",
            ),
        ] {
            let error = save_tool_result_images(
                json!({"content": [image]}),
                Some(workspace.path()),
                "session-1",
            )
            .await
            .unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
        let error = save_tool_result_images(
            json!({"content": [{"type": "image", "data": png_base64(1, 1), "mimeType": "image/png"}]}),
            None,
            "session-1",
        )
        .await
        .unwrap_err();
        assert!(error.contains("workspace root"), "{error}");
    }

    /// Proves post-processing notices keep image blocks for saving and append to text results.
    /// 证明后置提示为保存步骤保留图片块，并追加到文本结果之后。
    #[test]
    fn postprocessing_notice_keeps_images_and_literal_json_text() {
        let image = json!({"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"});
        let result =
            append_tool_result_notice(json!({"content": [image.clone()]}), "Completed.").unwrap();
        assert_eq!(
            result,
            json!({"content": [image, {"type": "text", "text": "Completed."}]})
        );
        let literal = "{\"content\":\"original-source\"}";
        let result =
            append_tool_result_notice(Value::String(literal.into()), "Completed.").unwrap();
        assert_eq!(result.as_str().unwrap(), format!("{literal}\n\nCompleted."));
    }
}
