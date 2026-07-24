//! Session goal lifecycle summary parse helpers.
//! 会话目标生命周期 summary parse 辅助逻辑。

use super::*;

/// Field-level protocol error produced while parsing one goal host projection.
/// 解析单个目标宿主投影时产生的字段级协议错误。
#[derive(Debug, Eq, PartialEq)]
enum GoalSummaryParseError {
    /// A required field is absent.
    /// 必填字段缺失。
    MissingField {
        /// Stable host-projection field name.
        /// 稳定的宿主投影字段名。
        field: &'static str,
    },
    /// A present field violates its type or value contract.
    /// 已提供字段违反类型或取值契约。
    InvalidField {
        /// Stable host-projection field name.
        /// 稳定的宿主投影字段名。
        field: &'static str,
        /// Human-readable expected field contract.
        /// 可读的字段预期契约。
        expected: &'static str,
    },
}

impl std::fmt::Display for GoalSummaryParseError {
    /// Formats one field-level parse error for runtime event diagnostics.
    /// 为运行时事件诊断格式化一条字段级解析错误。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField { field } => {
                write!(formatter, "missing required field `{field}`")
            }
            Self::InvalidField { field, expected } => {
                write!(formatter, "invalid field `{field}`: expected {expected}")
            }
        }
    }
}

impl std::error::Error for GoalSummaryParseError {}

///
/// # Parameters
/// 参数
/// `event` is the runtime event emitted after a tool call finishes.
/// `event` 是工具调用结束后发出的运行时事件。
///
/// # Returns
/// 返回值
/// `Ok(Some(Some(summary)))` for an updated goal, `Ok(Some(None))` for a cleared goal,
/// `Ok(None)` when unrelated, or an error when a declared goal projection is malformed.
/// 相关更新返回 `Ok(Some(Some(summary)))`，清理返回 `Ok(Some(None))`，无关事件返回
/// `Ok(None)`，已声明的目标投影格式错误时返回错误。
pub(crate) fn goal_summary_from_tool_finished_event(
    event: &RuntimeEvent,
) -> Result<Option<Option<SessionGoalSummary>>, String> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return Ok(None);
    }
    let Some(host_result) = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
    else {
        return Ok(None);
    };
    if host_result.get("kind").and_then(Value::as_str) != Some("session_goal") {
        return Ok(None);
    }
    let payload = host_result
        .get("payload")
        .ok_or_else(|| String::from("session_goal host result is missing payload"))?;
    match payload.get("goal") {
        Some(Value::Null) => Ok(Some(None)),
        Some(goal) => goal_summary_from_value(goal)
            .map(|summary| Some(Some(summary)))
            .map_err(|error| format!("session_goal host result contains {error}")),
        None => Err(String::from(
            "session_goal host result payload is missing goal",
        )),
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
fn goal_summary_from_value(value: &Value) -> Result<SessionGoalSummary, GoalSummaryParseError> {
    let session_id =
        uuid::Uuid::parse_str(required_goal_string(value, "sessionId")?).map_err(|_| {
            GoalSummaryParseError::InvalidField {
                field: "sessionId",
                expected: "UUID string",
            }
        })?;
    let goal_id = required_goal_string(value, "goalId")?;
    uuid::Uuid::parse_str(goal_id).map_err(|_| GoalSummaryParseError::InvalidField {
        field: "goalId",
        expected: "UUID string",
    })?;
    let objective = normalize_thread_goal_objective(required_goal_string(value, "objective")?)
        .map_err(|error| match error {
            ThreadGoalObjectiveError::EmptyObjective => GoalSummaryParseError::InvalidField {
                field: "objective",
                expected: "non-empty string",
            },
            ThreadGoalObjectiveError::ObjectiveTooLong { .. } => {
                GoalSummaryParseError::InvalidField {
                    field: "objective",
                    expected: "string with at most 4000 characters",
                }
            }
        })?;
    let status = required_goal_string(value, "status")?;
    let status =
        ThreadGoalStatus::try_from(status).map_err(|_| GoalSummaryParseError::InvalidField {
            field: "status",
            expected: "active, paused, blocked, usage_limited, budget_limited, or complete",
        })?;
    let token_budget = optional_positive_goal_i64(value, "tokenBudget")?;
    let tokens_used = required_nonnegative_goal_i64(value, "tokensUsed")?;
    let usage_tokens = match value.get("usageTokens") {
        None | Some(Value::Null) => tokens_used,
        Some(_) => required_nonnegative_goal_i64(value, "usageTokens")?,
    };
    Ok(SessionGoalSummary {
        session_id: session_id.to_string(),
        goal_id: goal_id.to_string(),
        objective: objective.to_string(),
        status: session_goal_status_from_thread_goal_status(status),
        token_budget,
        tokens_used,
        usage_tokens,
        api_call_count: optional_goal_usize(value, "apiCallCount")?,
        tool_call_count: optional_goal_usize(value, "toolCallCount")?,
        input_tokens: optional_nonnegative_goal_i64(value, "inputTokens")?,
        cached_input_tokens: optional_nonnegative_goal_i64(value, "cachedInputTokens")?,
        output_tokens: optional_nonnegative_goal_i64(value, "outputTokens")?,
        reasoning_tokens: optional_nonnegative_goal_i64(value, "reasoningTokens")?,
        total_tokens: optional_nonnegative_goal_i64(value, "totalTokens")?,
        time_used_seconds: required_nonnegative_goal_i64(value, "timeUsedSeconds")?,
        created_at_ms: required_nonnegative_goal_i64(value, "createdAtMs")?,
        updated_at_ms: required_nonnegative_goal_i64(value, "updatedAtMs")?,
    })
}

/// Reads one required string field from a goal projection.
/// 从目标投影读取一个必填字符串字段。
fn required_goal_string<'a>(
    value: &'a Value,
    field: &'static str,
) -> Result<&'a str, GoalSummaryParseError> {
    let field_value = value
        .get(field)
        .ok_or(GoalSummaryParseError::MissingField { field })?;
    field_value
        .as_str()
        .ok_or(GoalSummaryParseError::InvalidField {
            field,
            expected: "string",
        })
}

/// Reads one required non-negative integer field from a goal projection.
/// 从目标投影读取一个必填非负整数字段。
fn required_nonnegative_goal_i64(
    value: &Value,
    field: &'static str,
) -> Result<i64, GoalSummaryParseError> {
    let field_value = value
        .get(field)
        .ok_or(GoalSummaryParseError::MissingField { field })?;
    field_value
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or(GoalSummaryParseError::InvalidField {
            field,
            expected: "non-negative integer",
        })
}

/// Reads one optional positive integer field from a goal projection.
/// 从目标投影读取一个可选正整数字段。
fn optional_positive_goal_i64(
    value: &Value,
    field: &'static str,
) -> Result<Option<i64>, GoalSummaryParseError> {
    let token_budget = match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(field_value) => {
            field_value
                .as_i64()
                .map(Some)
                .ok_or(GoalSummaryParseError::InvalidField {
                    field,
                    expected: "integer or null",
                })
        }
    }?;
    validate_thread_goal_token_budget(token_budget).map_err(|_| {
        GoalSummaryParseError::InvalidField {
            field,
            expected: "positive integer or null",
        }
    })?;
    Ok(token_budget)
}

/// Reads one optional non-negative integer field, defaulting only when absent or null.
/// 读取一个可选非负整数字段，仅在缺失或为 null 时使用默认值。
fn optional_nonnegative_goal_i64(
    value: &Value,
    field: &'static str,
) -> Result<i64, GoalSummaryParseError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(0),
        Some(field_value) => field_value.as_i64().filter(|value| *value >= 0).ok_or(
            GoalSummaryParseError::InvalidField {
                field,
                expected: "non-negative integer or null",
            },
        ),
    }
}

/// Reads one optional platform-sized non-negative count from a goal projection.
/// 从目标投影读取一个可选的平台宽度非负计数。
fn optional_goal_usize(value: &Value, field: &'static str) -> Result<usize, GoalSummaryParseError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(0),
        Some(field_value) => field_value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(GoalSummaryParseError::InvalidField {
                field,
                expected: "non-negative integer fitting usize or null",
            }),
    }
}
