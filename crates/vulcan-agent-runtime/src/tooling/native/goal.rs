//! Goal native tool implementation.
//! 目标原生工具实现。

use super::*;
use crate::agent_runtime::session_goal_prompt::render_active_session_goal_context;

/// Parsed argument payload accepted by the `create_goal` tool.
/// `create_goal` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateGoalArgs {
    /// Required concrete objective for the new goal.
    /// 新目标必需的具体目标内容。
    objective: String,
    /// Optional positive token budget explicitly requested by the user.
    /// 用户明确要求时提供的可选正数 token 预算。
    token_budget: Option<i64>,
}

/// Parsed status accepted by the `update_goal` tool.
/// `update_goal` 工具接受的已解析状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum UpdateGoalStatusArg {
    /// Mark the current goal as complete.
    /// 将当前目标标记为完成。
    Complete,
    /// Mark the current goal as genuinely blocked.
    /// 将当前目标标记为真正受阻。
    Blocked,
}

/// Parsed argument payload accepted by the `update_goal` tool.
/// `update_goal` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateGoalArgs {
    /// Terminal status requested by the model.
    /// 模型请求设置的终态状态。
    status: UpdateGoalStatusArg,
}

/// Minimal accounting view used by the conversation receipt.
/// 会话回执使用的最小记账视图。
struct GoalModelView {
    /// Human-authored goal objective.
    /// 用户编写的目标内容。
    objective: String,
    /// Current lifecycle status.
    /// 当前生命周期状态。
    status: NativeGoalStatus,
    /// Optional explicit token budget.
    /// 可选的显式 token 预算。
    token_budget: Option<i64>,
    /// Accounted token usage for this goal.
    /// 当前目标已记账的 token 用量。
    tokens_used: Option<i64>,
    /// Accounted wall-clock time in seconds.
    /// 当前目标已记账的墙钟时间秒数。
    time_used_seconds: i64,
}

/// Builds the AI-visible goal read tool.
/// 构建 AI 可见的目标读取工具。
pub(crate) fn build_get_goal_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(GET_GOAL_TOOL_ID),
        name: String::from(GET_GOAL_TOOL_ID),
        description: String::from(
            "Get the current goal for this thread, including status, budgets, token and elapsed-time usage, and remaining token budget.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_get_goal_tool(request).await })
        },
    )
}

/// Builds the AI-visible goal creation tool.
/// 构建 AI 可见的目标创建工具。
pub(crate) fn build_create_goal_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(CREATE_GOAL_TOOL_ID),
        name: String::from(CREATE_GOAL_TOOL_ID),
        description: String::from(
            "Create a goal only when explicitly requested by the user or system/developer instructions; do not infer goals from ordinary tasks.\nSet token_budget only when an explicit token budget is requested. Fails if an unfinished goal exists; use update_goal only for status.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        create_goal_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_create_goal_tool(request).await })
        },
    )
}

/// Builds the AI-visible goal status update tool.
/// 构建 AI 可见的目标状态更新工具。
pub(crate) fn build_update_goal_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(UPDATE_GOAL_TOOL_ID),
        name: String::from(UPDATE_GOAL_TOOL_ID),
        description: String::from(
            "Update the existing goal.\nUse this tool only to mark the goal achieved or genuinely blocked.\nSet status to `complete` only when the objective has actually been achieved and no required work remains.\nSet status to `blocked` only when the same blocking condition has repeated for at least three consecutive goal turns, counting the original/user-triggered turn and any automatic continuations, and the agent cannot make meaningful progress without user input or an external-state change.\nIf the user resumes a goal that was previously marked `blocked`, treat the resumed run as a fresh blocked audit. If the same blocking condition then repeats for at least three consecutive resumed goal turns, set status to `blocked` again.\nOnce the blocked threshold is satisfied, do not keep reporting that you are still blocked while leaving the goal active; set status to `blocked`.\nDo not use `blocked` merely because the work is hard, slow, uncertain, incomplete, or would benefit from clarification.\nDo not mark a goal complete merely because its budget is nearly exhausted or because you are stopping work.\nYou cannot use this tool to pause, resume, budget-limit, or usage-limit a goal; those status changes are controlled by the user or system.\nWhen marking a budgeted goal achieved with status `complete`, report the final token usage from the tool result to the user.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        update_goal_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_update_goal_tool(request).await })
        },
    )
}

/// Builds the Codex-compatible JSON Schema for `create_goal`.
/// 构建与 Codex 兼容的 `create_goal` JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for the goal creation tool.
/// 面向模型的目标创建工具参数结构。
fn create_goal_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "objective": {
                "type": "string",
                "description": "Required. The concrete objective to start pursuing. This starts a new active goal when no goal exists or replaces the current goal when it is complete.",
                "minLength": 1,
                "maxLength": vulcan_session_core::v1::config::MAX_GOAL_OBJECTIVE_CHARS
            },
            "token_budget": {
                "type": "integer",
                "description": "Positive token budget for the new goal. Omit unless explicitly requested.",
                "minimum": 1
            }
        },
        "required": ["objective"],
        "additionalProperties": false
    })
}

/// Builds the Codex-compatible JSON Schema for `update_goal`.
/// 构建与 Codex 兼容的 `update_goal` JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for the goal status update tool.
/// 面向模型的目标状态更新工具参数结构。
fn update_goal_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "status": {
                "type": "string",
                "description": "Required. Set to `complete` only when the objective is achieved and no required work remains. Set to `blocked` only after the same blocking condition has recurred for at least three consecutive goal turns and the agent is at an impasse. After a previously blocked goal is resumed, the resumed run starts a fresh blocked audit.",
                "enum": ["complete", "blocked"]
            }
        },
        "required": ["status"],
        "additionalProperties": false
    })
}

/// Executes one `get_goal` request.
/// 执行单次 `get_goal` 请求。
///
/// # Parameters
/// 参数
/// `request` carries the current session and controller runtime context.
/// `request` 携带当前会话与控制器运行时上下文。
///
/// # Returns
/// 返回值
/// The current goal response formatted for the model.
/// 面向模型格式化的当前目标响应。
async fn execute_get_goal_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    ensure_empty_object_arguments(&request.arguments)?;
    let runtime = goal_runtime_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let goal = runtime
        .get_goal(session_id)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to read goal: {error}")))?;
    Ok(goal_tool_result(goal, false))
}

/// Executes one `create_goal` request.
/// 执行单次 `create_goal` 请求。
///
/// # Parameters
/// 参数
/// `request` carries model arguments plus current session persistence context.
/// `request` 携带模型参数与当前会话持久化上下文。
///
/// # Returns
/// 返回值
/// The newly created goal and a host projection payload.
/// 新创建的目标以及宿主投影载荷。
async fn execute_create_goal_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let mut args: CreateGoalArgs =
        serde_json::from_value(request.arguments.clone()).map_err(|error| {
            NativeToolError::new(format!("failed to parse function arguments: {error}"))
        })?;
    args.objective = normalize_native_goal_objective(&args.objective)?;
    let token_budget = native_goal_token_budget(args.token_budget)?;

    let runtime = goal_runtime_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let goal = runtime
        .create_goal(session_id, args.objective, token_budget)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to create goal: {error}")))?;
    Ok(goal_tool_result_with_context(Some(goal), false, true))
}

/// Executes one `update_goal` request.
/// 执行单次 `update_goal` 请求。
///
/// # Parameters
/// 参数
/// `request` carries the target terminal status for the current goal.
/// `request` 携带当前目标的目标终态状态。
///
/// # Returns
/// 返回值
/// The updated goal and a host projection payload.
/// 更新后的目标以及宿主投影载荷。
async fn execute_update_goal_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: UpdateGoalArgs =
        serde_json::from_value(request.arguments.clone()).map_err(|error| {
            NativeToolError::new(format!("failed to parse function arguments: {error}"))
        })?;
    let runtime = goal_runtime_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let target_status = match args.status {
        UpdateGoalStatusArg::Complete => NativeGoalTerminalStatus::Complete,
        UpdateGoalStatusArg::Blocked => NativeGoalTerminalStatus::Blocked,
    };
    let goal = runtime
        .update_goal_status(session_id, target_status)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to update goal: {error}")))?;
    Ok(goal_tool_result_with_context(
        Some(goal),
        args.status == UpdateGoalStatusArg::Complete,
        false,
    ))
}

/// Returns whether a model-owned terminal update is valid from the persisted status.
/// 返回模型拥有的终态更新是否可从当前持久化状态执行。
///
/// # Parameters
/// 参数
/// `current` is the status read before the atomic update.
/// `current` 是原子更新前读取的状态。
/// `target` is the terminal status requested by the model.
/// `target` 是模型请求的终态状态。
///
/// # Returns
/// 返回值
/// `true` when the transition preserves the Goal lifecycle contract.
/// 当转换符合 Goal 生命周期契约时返回 `true`。
#[cfg(test)]
pub(crate) const fn goal_status_allows_terminal_update(
    current: NativeGoalStatus,
    target: NativeGoalTerminalStatus,
) -> bool {
    match target {
        NativeGoalTerminalStatus::Complete => matches!(
            current,
            NativeGoalStatus::Active | NativeGoalStatus::Complete
        ),
        NativeGoalTerminalStatus::Blocked => matches!(
            current,
            NativeGoalStatus::Active
                | NativeGoalStatus::Paused
                | NativeGoalStatus::Blocked
                | NativeGoalStatus::UsageLimited
                | NativeGoalStatus::BudgetLimited
        ),
    }
}

/// Resolves the goal store from one native-tool request.
/// 从单条原生工具请求解析目标存储。
///
/// # Parameters
/// 参数
/// `request` carries the optional controller runtime handle.
/// `request` 携带可选控制器运行时句柄。
///
/// # Returns
/// 返回值
/// A controller-backed goal store or a model-visible error.
/// 控制器承载目标存储或一个模型可见错误。
fn goal_runtime_from_request(
    request: &NativeToolExecutionRequest,
) -> Result<Arc<dyn NativeGoalRuntime>, NativeToolError> {
    request
        .goal_runtime
        .clone()
        .ok_or_else(|| NativeToolError::new("goal tools require a V1 goal runtime context"))
}

/// Resolves the owning session id from one native-tool request.
/// 从单条原生工具请求解析所属会话标识。
///
/// # Parameters
/// 参数
/// `request` carries the optional current session id.
/// `request` 携带可选当前会话标识。
///
/// # Returns
/// 返回值
/// The current session id or a model-visible error.
/// 当前会话标识或一个模型可见错误。
fn goal_session_id_from_request(
    request: &NativeToolExecutionRequest,
) -> Result<SessionId, NativeToolError> {
    request
        .session_id
        .ok_or_else(|| NativeToolError::new("goal tools require a current session id"))
}

/// Validates and normalizes one model-authored Goal objective before the host call.
/// 在宿主调用前校验并规范化一条模型编写的 Goal 目标。
///
/// # Parameters
/// 参数
/// `objective` is the raw model-authored objective.
/// `objective` 是模型编写的原始目标。
///
/// # Returns
/// 返回值
/// Returns the trimmed objective or an explicit tool error.
/// 返回去除首尾空白后的目标，或显式工具错误。
fn normalize_native_goal_objective(objective: &str) -> Result<String, NativeToolError> {
    let objective = objective.trim();
    if objective.is_empty() {
        return Err(NativeToolError::new("goal objective must not be empty"));
    }
    if objective.chars().count() > vulcan_session_core::v1::config::MAX_GOAL_OBJECTIVE_CHARS {
        return Err(NativeToolError::new(format!(
            "goal objective must not exceed {} characters",
            vulcan_session_core::v1::config::MAX_GOAL_OBJECTIVE_CHARS
        )));
    }
    Ok(objective.to_string())
}

/// Converts the optional signed tool argument into the V1 positive budget domain.
/// 将可选有符号工具参数转换为 V1 正数预算领域。
///
/// # Parameters
/// 参数
/// `token_budget` is the optional model-authored budget.
/// `token_budget` 是模型编写的可选预算。
///
/// # Returns
/// 返回值
/// Returns the exact unsigned V1 budget or an explicit tool error.
/// 返回精确的无符号 V1 预算，或显式工具错误。
fn native_goal_token_budget(token_budget: Option<i64>) -> Result<Option<u64>, NativeToolError> {
    token_budget
        .map(|value| {
            u64::try_from(value)
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| NativeToolError::new("goal token_budget must be positive"))
        })
        .transpose()
}

/// Validates that an argument payload is an empty object when provided.
/// 校验参数载荷在提供时必须为空对象。
///
/// # Parameters
/// 参数
/// `arguments` is the raw JSON argument value.
/// `arguments` 是原始 JSON 参数值。
///
/// # Returns
/// 返回值
/// Success when the arguments are absent-like or empty.
/// 参数类似缺省或为空时返回成功。
fn ensure_empty_object_arguments(arguments: &Value) -> Result<(), NativeToolError> {
    match arguments {
        Value::Object(map) if map.is_empty() => Ok(()),
        Value::Null => Ok(()),
        _ => Err(NativeToolError::new(
            "get_goal does not accept any arguments.",
        )),
    }
}

/// Converts a persisted goal into the Codex-compatible model view.
/// 将持久化目标转换为 Codex 兼容的模型视图。
///
/// # Parameters
/// 参数
/// `goal` is the persisted goal record.
/// `goal` 是持久化目标记录。
///
/// # Returns
/// 返回值
/// A model-facing goal view with second-level timestamps.
/// 带秒级时间戳的模型可见目标视图。
fn goal_model_view(goal: &NativeGoalSnapshot) -> GoalModelView {
    GoalModelView {
        objective: goal.objective.clone(),
        status: goal.status,
        token_budget: goal.token_budget,
        tokens_used: goal.usage_known.then_some(goal.tokens_used),
        time_used_seconds: goal.time_used_seconds,
    }
}

/// Renders the shared goal tool result without a host projection.
/// 渲染不携带宿主投影的共享目标工具结果。
///
/// # Parameters
/// 参数
/// `goal` is the optional persisted goal.
/// `goal` 是可选的持久化目标。
/// `include_completion_report` controls final budget report visibility.
/// `include_completion_report` 控制最终预算报告是否可见。
///
/// # Returns
/// 返回值
/// A model-only invocation result.
/// 仅包含模型输出的调用结果。
fn goal_tool_result(
    goal: Option<NativeGoalSnapshot>,
    include_completion_report: bool,
) -> VulcanToolInvocationResult {
    let response = goal_response_value(goal.as_ref(), include_completion_report);
    VulcanToolInvocationResult::from_model_output(response)
}

/// Renders the shared goal result with optional immediate creation guidance.
/// 渲染可包含即时创建指引的共享目标工具结果。
///
/// # Parameters
/// 参数
/// `goal` is the optional persisted goal.
/// `goal` 是可选的持久化目标。
/// `include_completion_report` controls final budget report visibility.
/// `include_completion_report` 控制最终预算报告是否可见。
/// `include_active_context` controls immediate append-only guidance after goal creation.
/// `include_active_context` 控制目标创建后是否立即返回追加式活动目标引导。
///
/// # Returns
/// 返回值
/// One Markdown result shared by model and conversation display.
/// 模型与会话展示共用一份 Markdown 结果。
fn goal_tool_result_with_context(
    goal: Option<NativeGoalSnapshot>,
    include_completion_report: bool,
    include_active_context: bool,
) -> VulcanToolInvocationResult {
    let response = if include_active_context {
        goal.as_ref()
            .map(goal_created_response_value)
            .unwrap_or_else(|| goal_response_value(None, include_completion_report))
    } else {
        goal_response_value(goal.as_ref(), include_completion_report)
    };
    // Goal controls read the session domain snapshot, not this obsolete duplicated host view.
    // 目标控件读取会话领域快照，而非此处已废弃的重复宿主视图。
    VulcanToolInvocationResult::from_model_output(response)
}

/// Builds the model-facing creation response with immediate append-only goal guidance.
/// 构建包含即时追加式目标引导的模型可见创建响应。
///
/// # Parameters
/// 参数
/// `goal` is the newly persisted active goal.
/// `goal` 是刚持久化的活动目标。
///
/// # Returns
/// 返回值
/// The ordinary goal response plus guidance consumed directly after the tool call.
/// 返回普通目标响应以及紧随工具调用后消费的目标引导。
pub(crate) fn goal_created_response_value(goal: &NativeGoalSnapshot) -> Value {
    // The immutable guidance already owns the objective; avoid echoing it in the receipt.
    // 不可变指引已经包含目标正文，回执不再重复。
    let mut body = goal_response_markdown(Some(goal), false, false);
    body.push_str("\n");
    body.push_str(&render_active_session_goal_context(
        &goal.goal_id,
        &goal.objective,
        goal.token_budget,
    ));
    Value::String(body)
}

/// Builds the model-facing goal response value.
/// 构建模型可见的目标响应值。
///
/// # Parameters
/// 参数
/// `goal` is the optional persisted goal.
/// `goal` 是可选的持久化目标。
/// `include_completion_report` controls final budget report visibility.
/// `include_completion_report` 控制最终预算报告是否可见。
///
/// # Returns
/// 返回值
/// A serializable JSON value for the tool result.
/// 工具结果使用的可序列化 JSON 值。
pub(crate) fn goal_response_value(
    goal: Option<&NativeGoalSnapshot>,
    include_completion_report: bool,
) -> Value {
    Value::String(goal_response_markdown(
        goal,
        include_completion_report,
        true,
    ))
}

/// Renders the goal fields needed to act, including explicit unknown token accounting.
/// 渲染行动所需目标字段，并明确未知的 Token 记账状态。
/// Takes the optional snapshot and report/objective flags; returns one shared Markdown body.
/// 接收可选快照及报告、正文开关，返回一份共享 Markdown 正文。
fn goal_response_markdown(
    goal: Option<&NativeGoalSnapshot>,
    report: bool,
    include_objective: bool,
) -> String {
    // A missing goal is not a completed goal.
    // 未设置目标不代表目标已完成。
    let Some(goal) = goal else {
        return String::from("# Goal\n\nNo goal is set.\n");
    };
    // Reuse canonical usage projection so unknown accounting never becomes zero.
    // 复用规范用量投影，未知记账状态不得变成零。
    let view = goal_model_view(goal);
    // Navigation and timestamps belong to the domain snapshot, not the tool reading result.
    // 导航与时间戳归领域快照所有，不进入工具阅读结果。
    let mut output = format!("# Goal\n\nStatus: {}\n", view.status.as_str());
    if include_objective {
        output.push_str(&format!("\n## Objective\n\n{}\n", view.objective));
    }
    match view.tokens_used {
        Some(used) => output.push_str(&format!("\nTokens used: {used}\n")),
        None => output.push_str("\nTokens used: unknown\n"),
    }
    if let Some(budget) = view.token_budget {
        output.push_str(&format!("\nToken budget: {budget}\n"));
        if let Some(used) = view.tokens_used {
            output.push_str(&format!(
                "\nRemaining tokens: {}\n",
                budget.saturating_sub(used).max(0)
            ));
        }
    }
    output.push_str(&format!("\nElapsed seconds: {}\n", view.time_used_seconds));
    if report && let Some(instruction) = goal_completion_budget_report(goal) {
        output.push_str(&format!("\n{instruction}\n"));
    }
    output
}

/// Builds the final completion usage-report instruction used by Codex goal tools.
/// 构建 Codex 目标工具使用的最终完成用量汇报指令。
///
/// # Parameters
/// 参数
/// `goal` is the completed persisted goal.
/// `goal` 是已完成的持久化目标。
///
/// # Returns
/// 返回值
/// A usage-report instruction when an explicit budget or elapsed time exists.
/// 存在显式预算或耗时时返回用量汇报指令。
fn goal_completion_budget_report(goal: &NativeGoalSnapshot) -> Option<String> {
    if goal.token_budget.is_none() && goal.time_used_seconds <= 0 {
        None
    } else {
        Some(
            "Goal achieved. Report final usage from this receipt. Include the token budget when present; never turn unknown token usage into zero. Summarize nonzero elapsed time concisely in the response language."
                .to_string(),
        )
    }
}

#[cfg(test)]
mod goal_projection_tests {
    use super::*;

    /// Verifies get_goal declares an explicit closed empty-object parameter schema.
    /// 验证 get_goal 声明显式封闭的空对象参数 Schema。
    #[test]
    fn get_goal_uses_explicit_empty_object_schema() {
        let tool = build_get_goal_tool();

        assert_eq!(
            tool.args_schema,
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            })
        );
    }
}
