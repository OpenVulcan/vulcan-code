//! Goal native tool implementation.
//! 目标原生工具实现。

use super::*;

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

/// Model-facing goal view aligned with Codex protocol fields.
/// 与 Codex 协议字段对齐的模型可见目标视图。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoalModelView {
    /// Session identifier that owns this goal.
    /// 拥有该目标的会话标识。
    thread_id: SessionId,
    /// Human-authored goal objective.
    /// 用户编写的目标内容。
    objective: String,
    /// Current lifecycle status.
    /// 当前生命周期状态。
    status: ThreadGoalStatus,
    /// Optional explicit token budget.
    /// 可选的显式 token 预算。
    #[serde(skip_serializing_if = "Option::is_none")]
    token_budget: Option<i64>,
    /// Accounted token usage for this goal.
    /// 当前目标已记账的 token 用量。
    tokens_used: i64,
    /// Accounted wall-clock time in seconds.
    /// 当前目标已记账的墙钟时间秒数。
    time_used_seconds: i64,
    /// Creation timestamp in Unix epoch seconds.
    /// 创建时间的 Unix 纪元秒值。
    created_at: i64,
    /// Last update timestamp in Unix epoch seconds.
    /// 最近更新时间的 Unix 纪元秒值。
    updated_at: i64,
}

/// Model-facing tool response shared by all Goal native tools.
/// 所有 Goal 原生工具共享的模型可见工具响应。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoalToolResponse {
    /// Current goal, if one exists.
    /// 当前目标，如果存在。
    goal: Option<GoalModelView>,
    /// Remaining explicit token budget, if a budget exists.
    /// 剩余显式 token 预算，如果存在预算。
    remaining_tokens: Option<i64>,
    /// Final usage-report instruction returned for completed goals with budget or elapsed time.
    /// 在带预算或耗时的目标完成时返回的最终用量汇报指令。
    completion_budget_report: Option<String>,
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
                "maxLength": MAX_THREAD_GOAL_OBJECTIVE_CHARS
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
    let store = goal_store_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let goal = store
        .get_thread_goal(session_id)
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
    args.objective = normalize_thread_goal_objective(&args.objective)
        .map_err(|error| NativeToolError::new(error.to_string()))?
        .to_string();
    validate_thread_goal_token_budget(args.token_budget)
        .map_err(|error| NativeToolError::new(error.to_string()))?;

    let store = goal_store_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let goal = store
        .insert_thread_goal(
            session_id,
            args.objective.as_str(),
            ThreadGoalStatus::Active,
            args.token_budget,
        )
        .await
        .map_err(|error| NativeToolError::new(format!("failed to create goal: {error}")))?
        .ok_or_else(|| {
            NativeToolError::new(
                "cannot create a new goal because this thread has an unfinished goal; complete the existing goal first",
            )
        })?;
    Ok(goal_tool_result_with_host(Some(goal), false))
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
    let store = goal_store_from_request(&request)?;
    let session_id = goal_session_id_from_request(&request)?;
    let target_status = match args.status {
        UpdateGoalStatusArg::Complete => ThreadGoalStatus::Complete,
        UpdateGoalStatusArg::Blocked => ThreadGoalStatus::Blocked,
    };
    let existing = store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to read goal: {error}")))?
        .ok_or_else(|| {
            NativeToolError::new("cannot update goal because this thread has no goal")
        })?;
    if !goal_status_allows_terminal_update(existing.status, target_status) {
        return Err(NativeToolError::new(format!(
            "cannot change goal status from {} to {}",
            existing.status.as_str(),
            target_status.as_str()
        )));
    }
    let expected_goal_id = existing.goal_id;
    let expected_status = existing.status;
    let goal = store
        .update_thread_goal(
            session_id,
            GoalUpdate {
                objective: None,
                status: Some(target_status),
                token_budget: None,
                expected_goal_id: Some(expected_goal_id.clone()),
                expected_status: Some(expected_status),
            },
        )
        .await
        .map_err(|error| NativeToolError::new(format!("failed to update goal: {error}")))?
        .ok_or_else(|| {
            NativeToolError::new("cannot update goal because it changed concurrently")
        })?;
    if goal.goal_id != expected_goal_id || goal.status != target_status {
        return Err(NativeToolError::new(
            "goal update returned a row that does not match the requested transition",
        ));
    }
    Ok(goal_tool_result_with_host(
        Some(goal),
        args.status == UpdateGoalStatusArg::Complete,
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
pub(crate) const fn goal_status_allows_terminal_update(
    current: ThreadGoalStatus,
    target: ThreadGoalStatus,
) -> bool {
    match target {
        ThreadGoalStatus::Complete => matches!(
            current,
            ThreadGoalStatus::Active | ThreadGoalStatus::Complete
        ),
        ThreadGoalStatus::Blocked => matches!(
            current,
            ThreadGoalStatus::Active
                | ThreadGoalStatus::Paused
                | ThreadGoalStatus::Blocked
                | ThreadGoalStatus::UsageLimited
                | ThreadGoalStatus::BudgetLimited
        ),
        ThreadGoalStatus::Active
        | ThreadGoalStatus::Paused
        | ThreadGoalStatus::UsageLimited
        | ThreadGoalStatus::BudgetLimited => false,
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
fn goal_store_from_request(
    request: &NativeToolExecutionRequest,
) -> Result<VulcanGoalStore, NativeToolError> {
    request
        .controller_runtime
        .clone()
        .map(VulcanGoalStore::new)
        .ok_or_else(|| NativeToolError::new("goal tools require a controller runtime context"))
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
fn goal_model_view(goal: &ThreadGoal) -> GoalModelView {
    GoalModelView {
        thread_id: goal.thread_id,
        objective: goal.objective.clone(),
        status: goal.status,
        token_budget: goal.token_budget,
        tokens_used: goal.tokens_used,
        time_used_seconds: goal.time_used_seconds,
        created_at: goal.created_at_ms / 1_000,
        updated_at: goal.updated_at_ms / 1_000,
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
    goal: Option<ThreadGoal>,
    include_completion_report: bool,
) -> VulcanToolInvocationResult {
    let response = goal_response_value(goal.as_ref(), include_completion_report);
    VulcanToolInvocationResult::from_model_output(response)
}

/// Renders the shared goal tool result with a host projection.
/// 渲染携带宿主投影的共享目标工具结果。
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
/// A model invocation result plus a host `session_goal` projection.
/// 模型调用结果以及宿主 `session_goal` 投影。
fn goal_tool_result_with_host(
    goal: Option<ThreadGoal>,
    include_completion_report: bool,
) -> VulcanToolInvocationResult {
    let response = goal_response_value(goal.as_ref(), include_completion_report);
    let host_payload = json!({
        "goal": goal.as_ref().map(goal_host_projection),
    });
    VulcanToolInvocationResult::from_model_output(response).with_host_result(VulcanToolHostResult {
        kind: String::from("session_goal"),
        payload: host_payload,
    })
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
    goal: Option<&ThreadGoal>,
    include_completion_report: bool,
) -> Value {
    let remaining_tokens = goal.and_then(|goal| {
        goal.token_budget
            .map(|budget| (budget - goal.tokens_used).max(0))
    });
    let completion_budget_report = if include_completion_report {
        goal.and_then(goal_completion_budget_report)
    } else {
        None
    };
    json!(GoalToolResponse {
        goal: goal.map(goal_model_view),
        remaining_tokens,
        completion_budget_report,
    })
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
fn goal_completion_budget_report(goal: &ThreadGoal) -> Option<String> {
    if goal.token_budget.is_none() && goal.time_used_seconds <= 0 {
        None
    } else {
        Some(
            "Goal achieved. Report final usage from this tool result's structured goal fields. If `goal.tokenBudget` is present, include token usage from `goal.tokensUsed` and `goal.tokenBudget`. If `goal.timeUsedSeconds` is greater than 0, summarize elapsed time in a concise, human-friendly form appropriate to the response language."
                .to_string(),
        )
    }
}

/// Builds the host-facing goal projection used by workbench snapshots.
/// 构建工作台快照使用的宿主可见目标投影。
///
/// # Parameters
/// 参数
/// `goal` is the persisted goal record.
/// `goal` 是持久化目标记录。
///
/// # Returns
/// 返回值
/// A JSON projection including internal goal id and millisecond timestamps.
/// 包含内部目标标识与毫秒时间戳的 JSON 投影。
fn goal_host_projection(goal: &ThreadGoal) -> Value {
    json!({
        "sessionId": goal.thread_id,
        "goalId": goal.goal_id,
        "objective": goal.objective,
        "status": goal.status,
        "tokenBudget": goal.token_budget,
        "tokensUsed": goal.tokens_used,
        "timeUsedSeconds": goal.time_used_seconds,
        "createdAtMs": goal.created_at_ms,
        "updatedAtMs": goal.updated_at_ms,
    })
}
