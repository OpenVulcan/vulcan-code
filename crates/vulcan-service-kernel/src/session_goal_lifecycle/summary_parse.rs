//! Session goal lifecycle summary parse helpers.
//! 会话目标生命周期 summary parse 辅助逻辑。

use super::*;

///
/// # Parameters
/// 参数
/// `event` is the runtime event emitted after a tool call finishes.
/// `event` 是工具调用结束后发出的运行时事件。
///
/// # Returns
/// 返回值
/// `Some(Some(summary))` for an updated goal, `Some(None)` for a cleared goal, or `None` when unrelated.
/// 相关更新返回 `Some(Some(summary))`，清理返回 `Some(None)`，无关事件返回 `None`。
pub(crate) fn goal_summary_from_tool_finished_event(
    event: &RuntimeEvent,
) -> Option<Option<SessionGoalSummary>> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return None;
    }
    let payload = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| {
            (host_result.get("kind").and_then(Value::as_str) == Some("session_goal"))
                .then(|| host_result.get("payload"))
                .flatten()
        })?;
    match payload.get("goal") {
        Some(Value::Null) => Some(None),
        Some(goal) => goal_summary_from_value(goal).map(Some),
        None => None,
    }
}

/// Parses one host-result JSON value into a session goal summary.
/// 将单个宿主结果 JSON 值解析为会话目标摘要。
///
/// # Parameters
/// 参数
/// `value` is the raw goal projection emitted by the native tool.
/// `value` 是原生工具发出的原始目标投影。
///
/// # Returns
/// 返回值
/// The parsed goal summary when required fields are valid.
/// 必需字段有效时返回已解析的目标摘要。
pub(crate) fn goal_summary_from_value(value: &Value) -> Option<SessionGoalSummary> {
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())?;
    let goal_id = value
        .get("goalId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())?;
    let objective = value
        .get("objective")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())?;
    let token_budget = match value.get("tokenBudget") {
        Some(Value::Null) | None => None,
        Some(value) => Some(value.as_i64()?),
    };
    Some(SessionGoalSummary {
        session_id: session_id.to_string(),
        goal_id,
        objective,
        status,
        token_budget,
        tokens_used: value.get("tokensUsed").and_then(Value::as_i64)?,
        usage_tokens: value
            .get("usageTokens")
            .and_then(Value::as_i64)
            .or_else(|| value.get("tokensUsed").and_then(Value::as_i64))?,
        api_call_count: value
            .get("apiCallCount")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or_default(),
        tool_call_count: value
            .get("toolCallCount")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or_default(),
        input_tokens: value
            .get("inputTokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        cached_input_tokens: value
            .get("cachedInputTokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        output_tokens: value
            .get("outputTokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        total_tokens: value
            .get("totalTokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        time_used_seconds: value.get("timeUsedSeconds").and_then(Value::as_i64)?,
        created_at_ms: value.get("createdAtMs").and_then(Value::as_i64)?,
        updated_at_ms: value.get("updatedAtMs").and_then(Value::as_i64)?,
    })
}
