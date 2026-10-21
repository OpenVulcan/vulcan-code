//! Renders code-mode runtime observations as model-visible tool output.
//! 将代码模式运行时观察结果渲染为模型可见工具输出。
//!
//! The status header follows openai/codex `codex-rs/core/src/tools/code_mode/output.rs`.
//! 状态标题沿用 openai/codex `codex-rs/core/src/tools/code_mode/output.rs`。

use serde_json::Value;
use std::time::Duration;
use vulcan_code_mode::{
    DEFAULT_MAX_OUTPUT_TOKENS_PER_EXEC_CALL, FunctionCallOutputContentItem, RuntimeResponse,
};
use vulcan_tools_manager::VulcanToolInvocationResult;

/// Approximate bytes per token used to apply the token budget to text.
/// 将 Token 预算应用于文本时使用的近似每 Token 字节数。
const BYTES_PER_TOKEN: usize = 4;

/// Renders one runtime observation with its status header and bounded output.
/// 渲染一次运行时观察结果，包括状态标题与有界输出。
///
/// # Parameters / 参数
/// - `response`: Observation returned by `exec` or `wait`.
/// - `max_output_tokens`: Caller budget; the code-mode default applies when absent.
/// - `wall_time`: Time this call spent waiting for the observation.
/// - `response`：`exec` 或 `wait` 返回的观察结果。
/// - `max_output_tokens`：调用方预算；缺省时使用代码模式默认值。
/// - `wall_time`：本次调用等待观察结果所用的时间。
///
/// # Returns / 返回值
/// A text tool result; images and audio are replaced by a short notice.
/// 文本工具结果；图片与音频以简短说明替代。
pub(super) fn render_runtime_response(
    response: RuntimeResponse,
    max_output_tokens: Option<usize>,
    wall_time: Duration,
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
    let mut parts = content_items
        .into_iter()
        .map(|item| match item {
            FunctionCallOutputContentItem::InputText { text } => text,
            FunctionCallOutputContentItem::InputImage { .. } => {
                String::from("[Image output omitted: code mode returns text only.]")
            }
            FunctionCallOutputContentItem::InputAudio { .. } => {
                String::from("[Audio output omitted: code mode returns text only.]")
            }
        })
        .collect::<Vec<_>>();
    if let Some(error_text) = error_text {
        parts.push(format!("Script error:\n{error_text}"));
    }
    let body = truncate_to_token_budget(
        parts.join("\n"),
        max_output_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS_PER_EXEC_CALL),
    );
    let wall_time_seconds = (wall_time.as_secs_f32() * 10.0).round() / 10.0;
    VulcanToolInvocationResult::from_model_output(Value::String(format!(
        "{status}\nWall time {wall_time_seconds:.1} seconds\nOutput:\n{body}"
    )))
}

/// Keeps the head and tail of text that exceeds the approximate token budget.
/// 保留超出近似 Token 预算的文本的开头与结尾。
fn truncate_to_token_budget(text: String, max_tokens: usize) -> String {
    let max_bytes = max_tokens.saturating_mul(BYTES_PER_TOKEN);
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
