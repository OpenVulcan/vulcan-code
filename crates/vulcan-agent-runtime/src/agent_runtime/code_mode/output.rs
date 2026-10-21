//! Renders code-mode runtime observations as model-visible tool output.
//! 将代码模式运行时观察结果渲染为模型可见工具输出。
//!
//! The status header follows openai/codex `codex-rs/core/src/tools/code_mode/output.rs`.
//! 状态标题沿用 openai/codex `codex-rs/core/src/tools/code_mode/output.rs`。

use crate::agent_runtime::tool_result_media::tool_result_image_is_valid;
use serde_json::{Value, json};
use std::time::Duration;
use vulcan_code_mode::{
    DEFAULT_MAX_OUTPUT_TOKENS_PER_EXEC_CALL, FunctionCallOutputContentItem, RuntimeResponse,
};
use vulcan_tools_manager::VulcanToolInvocationResult;

/// Approximate bytes per token used to apply the token budget to text.
/// 将 Token 预算应用于文本时使用的近似每 Token 字节数。
const BYTES_PER_TOKEN: usize = 4;

/// One ordered piece of a cell observation.
/// 单元观察结果中的一个有序片段。
enum Piece {
    /// Text item or a notice that replaces media the route cannot carry.
    /// 文本项，或替代路由无法承载之媒体的说明。
    Text(String),
    /// Validated Base64 image.
    /// 已校验的 Base64 图片。
    Image { data: String, mime_type: String },
}

/// Renders one runtime observation with its status header and bounded output.
/// 渲染一次运行时观察结果，包括状态标题与有界输出。
///
/// # Parameters / 参数
/// - `response`: Observation returned by `exec` or `wait`.
/// - `max_output_tokens`: Caller budget; the code-mode default applies when absent.
/// - `wall_time`: Time this call spent waiting for the observation.
/// - `notifications`: `notify()` texts the cell sent since its last observation.
/// - `media`: Whether the route carries tool-result images natively.
/// - `response`：`exec` 或 `wait` 返回的观察结果。
/// - `max_output_tokens`：调用方预算；缺省时使用代码模式默认值。
/// - `wall_time`：本次调用等待观察结果所用的时间。
/// - `notifications`：单元自上次观察以来通过 `notify()` 发送的文本。
/// - `media`：路由是否原生承载工具结果图片。
///
/// # Returns / 返回值
/// Text output, or an ordered text-and-image result when the route carries images; audio and
/// images the route cannot carry are replaced by a short notice.
/// 文本输出；路由承载图片时返回有序的文本与图片结果；音频及路由无法承载的图片以简短说明替代。
pub(super) fn render_runtime_response(
    response: RuntimeResponse,
    max_output_tokens: Option<usize>,
    wall_time: Duration,
    notifications: Vec<String>,
    media: bool,
) -> VulcanToolInvocationResult {
    let (status, content_items, error_text) = match response {
        RuntimeResponse::Yielded {
            cell_id,
            content_items,
            ..
        } => (
            format!("Script running with cell ID {cell_id}"),
            content_items,
            None,
        ),
        RuntimeResponse::Terminated { content_items, .. } => {
            (String::from("Script terminated"), content_items, None)
        }
        RuntimeResponse::Result {
            content_items,
            error_text,
            ..
        } => (
            String::from(if error_text.is_none() {
                "Script completed"
            } else {
                "Script failed"
            }),
            content_items,
            error_text,
        ),
    };
    let mut pieces = content_items
        .into_iter()
        .map(|item| match item {
            FunctionCallOutputContentItem::InputText { text } => Piece::Text(text),
            FunctionCallOutputContentItem::InputImage { image_url, .. } => {
                image_piece(&image_url, media)
            }
            FunctionCallOutputContentItem::InputAudio { .. } => Piece::Text(String::from(
                "[Audio output omitted: this route does not carry audio tool results.]",
            )),
        })
        .collect::<Vec<_>>();
    if let Some(error_text) = error_text {
        pieces.push(Piece::Text(format!("Script error:\n{error_text}")));
    }
    let wall_time_seconds = (wall_time.as_secs_f32() * 10.0).round() / 10.0;
    let mut header = format!("{status}\nWall time {wall_time_seconds:.1} seconds\n");
    if !notifications.is_empty() {
        header.push_str("Notifications:\n");
        for notification in notifications {
            header.push_str(&notification);
            header.push('\n');
        }
    }
    header.push_str("Output:\n");
    let mut budget = max_output_tokens
        .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS_PER_EXEC_CALL)
        .saturating_mul(BYTES_PER_TOKEN);
    if !pieces
        .iter()
        .any(|piece| matches!(piece, Piece::Image { .. }))
    {
        let body = pieces
            .into_iter()
            .map(|piece| match piece {
                Piece::Text(text) => text,
                Piece::Image { .. } => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        return VulcanToolInvocationResult::from_model_output(Value::String(format!(
            "{header}{}",
            truncate_to_byte_budget(body, budget)
        )));
    }
    // The bridge turns this ordered content into the durable tool-result media envelope.
    // 桥接层把这份有序内容转换为可持久化的工具结果媒体封套。
    let mut content = vec![json!({"type": "text", "text": header})];
    for piece in pieces {
        match piece {
            Piece::Text(text) => {
                let text = truncate_to_byte_budget(text, budget);
                budget = budget.saturating_sub(text.len());
                content.push(json!({"type": "text", "text": text}));
            }
            Piece::Image { data, mime_type } => {
                content.push(json!({"type": "image", "data": data, "mimeType": mime_type}));
            }
        }
    }
    VulcanToolInvocationResult::from_model_output(json!({ "content": content }))
}

/// Keeps one image as native media when the route carries it, otherwise replaces it by a notice.
/// 路由承载时把图片保留为原生媒体，否则以说明替代。
fn image_piece(image_url: &str, media: bool) -> Piece {
    let omitted = |reason: &str| Piece::Text(format!("[Image output omitted: {reason}]"));
    if !media {
        return omitted("this route does not carry image tool results.");
    }
    let Some((mime_type, data)) = image_url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
    else {
        return omitted("only base64 `data:` image URLs are returned.");
    };
    if !tool_result_image_is_valid(data, mime_type) {
        return omitted("the image must be valid base64 PNG, JPEG or WebP within the size limit.");
    }
    Piece::Image {
        data: data.to_owned(),
        mime_type: mime_type.to_owned(),
    }
}

/// Keeps the head and tail of text that exceeds the byte budget.
/// 保留超出字节预算的文本的开头与结尾。
fn truncate_to_byte_budget(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let half = max_bytes / 2;
    let head_end = floor_char_boundary(&text, half);
    let tail_start = ceil_char_boundary(&text, text.len() - half);
    let omitted = tail_start - head_end;
    format!(
        "Warning: truncated output (original length {} bytes)\n{}…{omitted} bytes truncated…{}",
        text.len(),
        &text[..head_end],
        &text[tail_start..]
    )
}

/// Returns the largest char boundary at or below `index`.
/// 返回不大于 `index` 的最大字符边界。
fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Returns the smallest char boundary at or above `index`.
/// 返回不小于 `index` 的最小字符边界。
fn ceil_char_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}
