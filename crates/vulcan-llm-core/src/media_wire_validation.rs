//! Media-position validation after target projection, without inspecting opaque tool arguments.
//! 目标投影后的媒体位置校验，不检查不透明工具参数。

use crate::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    GEMINI_GENERATE_CONTENT_PROFILE_ID, GEMINI_INTERACTIONS_PROFILE_ID,
    GOOGLE_CODE_ASSIST_PROFILE_ID, INTERNAL_RESOURCE_PART_TYPE, LlmCoreError,
    OPENAI_CHAT_PROFILE_ID, OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};
use serde_json::Value;

/// Validates media positions in `body` for the exact `profile_id` after provider projections.
/// 在供应商投影之后，为精确 `profile_id` 校验 `body` 中的媒体位置。
/// Returns success only when no internal media envelope or forbidden family extension remains.
/// 仅在没有内部媒体封装或禁止的协议族扩展残留时返回成功。
/// This is a media guard, not a replacement for model limits, full request schemas or upload policy.
/// 此函数是媒体护栏，不能替代模型限制、完整请求结构或上传策略。
pub fn validate_target_media_projection(
    profile_id: &str,
    body: &Value,
) -> Result<(), LlmCoreError> {
    // These paths are owned by the registered target encoders, not discovered by recursive guessing.
    // 这些路径由已注册目标编码器持有，不通过递归猜测发现。
    let (items_path, content_key) = match profile_id {
        OPENAI_RESPONSES_PROFILE_ID
        | OPENAI_CODEX_RESPONSES_PROFILE_ID
        | GEMINI_INTERACTIONS_PROFILE_ID => ("/input", "content"),
        OPENAI_CHAT_PROFILE_ID
        | ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => ("/messages", "content"),
        GEMINI_GENERATE_CONTENT_PROFILE_ID => ("/contents", "parts"),
        GOOGLE_CODE_ASSIST_PROFILE_ID => ("/request/contents", "parts"),
        _ => {
            return Err(LlmCoreError::new(format!(
                "No media wire validation is registered for protocol `{profile_id}`."
            )));
        }
    };
    // Standard Responses also admits a direct text input; every generated structured target uses an array.
    // 标准 Responses 也允许直接文本输入；每个生成的结构化目标均使用数组。
    let input = body.pointer(items_path).ok_or_else(|| {
        LlmCoreError::new(format!(
            "Target `{profile_id}` is missing its required media-input container `{items_path}`."
        ))
    })?;
    if matches!(
        profile_id,
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID
    ) && input.is_string()
    {
        return Ok(());
    }
    // Wrong root shapes must fail, not silently bypass the final content guard.
    // 错误根结构必须失败，不能静默绕过最终内容护栏。
    let items = input.as_array().ok_or_else(|| {
        LlmCoreError::new(format!(
            "Target `{profile_id}` requires an array at `{items_path}`."
        ))
    })?;
    for item in items {
        validate_media_part_type(profile_id, item)?;
        if let Some(parts) = item.get(content_key).and_then(Value::as_array) {
            for part in parts {
                validate_media_part_type(profile_id, part)?;
            }
        }
        if matches!(
            profile_id,
            OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID
        ) && matches!(
            item.get("type").and_then(Value::as_str),
            Some("function_call_output" | "custom_tool_call_output")
        ) && let Some(parts) = item.get("output").and_then(Value::as_array)
        {
            // Responses tool-output arrays are actual protocol content, unlike opaque function arguments.
            // Responses 工具输出数组是实际协议内容，不同于不透明函数参数。
            for part in parts {
                validate_media_part_type(profile_id, part)?;
            }
        }
    }
    Ok(())
}

/// Checks a real content `part` against `profile_id`; opaque tool payloads are never passed here.
/// 根据 `profile_id` 检查实际内容 `part`；不透明工具载荷不会传入此处。
/// Returns an explicit error for internal or unsupported audio/video content tags.
/// 对内部或不支持的音视频内容标签返回明确错误。
fn validate_media_part_type(profile_id: &str, part: &Value) -> Result<(), LlmCoreError> {
    // A missing type is legal for Gemini parts and Responses role-bearing message objects.
    // Gemini Part 及带角色的 Responses 消息对象允许缺失 type。
    let kind = part.get("type").and_then(Value::as_str);
    if kind == Some(INTERNAL_RESOURCE_PART_TYPE) {
        return Err(LlmCoreError::new(
            "An internal media envelope reached the target wire boundary.",
        ));
    }
    if matches!(
        profile_id,
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID
    ) && matches!(
        kind,
        Some("input_audio" | "input_video" | "video_url" | "audio" | "video")
    ) {
        return Err(LlmCoreError::new(
            "Standard Responses has no registered audio/video input shape.",
        ));
    }
    if matches!(profile_id, ANTHROPIC_MESSAGES_PROFILE_ID)
        && matches!(
            kind,
            Some("video" | "audio" | "input_audio" | "input_video")
        )
    {
        return Err(LlmCoreError::new(
            "The selected standard Anthropic profile cannot consume audio/video extensions.",
        ));
    }
    if profile_id == ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        && matches!(kind, Some("audio" | "input_audio"))
    {
        return Err(LlmCoreError::new(
            "The Anthropic video profile has no registered audio input shape.",
        ));
    }
    Ok(())
}
