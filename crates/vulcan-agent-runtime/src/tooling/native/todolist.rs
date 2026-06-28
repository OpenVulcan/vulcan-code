//! Todolist native tool implementation.
//! Todolist 原生工具实现。

use super::*;

pub(crate) fn build_todolist_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(TODOLIST_TOOL_ID),
        name: String::from(TODOLIST_TOOL_ID),
        description: String::from(
            "Updates the session todolist/checklist progress shown to the user.\nThis is a todolist progress tool, not Plan Mode and not a design-plan document tool.\nProvide an optional explanation and a list of todolist items, each with a step and status.\nAt most one step can be in_progress at a time.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: false,
    };

    build_native_tool_definition(
        descriptor,
        todolist_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_todolist_tool(request).await })
        },
    )
}

/// One parsed step status accepted by the `todolist` tool.
/// `todolist` 工具接受的单个已解析步骤状态。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TodolistStepStatusArg {
    /// The step has not started yet.
    /// 该步骤尚未开始。
    Pending,
    /// The step is currently being worked on.
    /// 该步骤正在进行中。
    InProgress,
    /// The step has been completed.
    /// 该步骤已经完成。
    Completed,
}

impl TodolistStepStatusArg {
    /// Returns the stable public status key.
    /// 返回稳定的公开状态键。
    ///
    /// # Returns
    /// 返回值
    /// The Codex-compatible snake_case status string.
    /// 与 Codex 兼容的 snake_case 状态字符串。
    fn as_key(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

/// One parsed todolist item accepted by the `todolist` tool.
/// `todolist` 工具接受的单个已解析条目。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TodolistItemArg {
    /// Task step text.
    /// 任务步骤文本。
    step: String,
    /// Step status.
    /// 步骤状态。
    status: TodolistStepStatusArg,
}

/// Parsed argument payload accepted by the `todolist` tool.
/// `todolist` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TodolistArgs {
    /// Optional explanation for this todolist/checklist update.
    /// 本次 todolist/检查清单更新的可选说明。
    explanation: Option<String>,
    /// The list of todolist/checklist steps.
    /// todolist/检查清单步骤列表。
    items: Vec<TodolistItemArg>,
}

/// Builds the Codex-compatible JSON Schema for `todolist`.
/// 构建与 Codex 兼容的 `todolist` JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for the todolist/checklist progress tool.
/// 面向模型的 todolist/检查清单进程工具参数结构。
fn todolist_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "explanation": {
                "type": "string",
                "description": "Optional explanation for this todolist/checklist progress update."
            },
            "items": {
                "type": "array",
                "description": "The ordered todolist/checklist steps to show as session progress.",
                "items": {
                    "type": "object",
                    "properties": {
                        "step": {
                            "type": "string",
                            "description": "Task step text.",
                            "minLength": 1
                        },
                        "status": {
                            "type": "string",
                            "description": "Step status.",
                            "enum": ["pending", "in_progress", "completed"]
                        }
                    },
                    "required": ["step", "status"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["items"],
        "additionalProperties": false
    })
}

/// Executes one `todolist` request and returns a host projection payload.
/// 执行单次 `todolist` 请求并返回宿主投影载荷。
///
/// # Parameters
/// 参数
/// `request` carries the raw model-supplied tool arguments.
/// `request` 携带模型提供的原始工具参数。
///
/// # Returns
/// 返回值
/// A Codex-compatible model response plus a Vulcan host result for session projection.
/// 与 Codex 兼容的模型响应以及用于会话投影的 Vulcan 宿主结果。
async fn execute_todolist_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: TodolistArgs = serde_json::from_value(request.arguments).map_err(|error| {
        NativeToolError::new(format!("failed to parse function arguments: {error}"))
    })?;
    let in_progress_count = args
        .items
        .iter()
        .filter(|item| matches!(item.status, TodolistStepStatusArg::InProgress))
        .count();
    if in_progress_count > 1 {
        return Err(NativeToolError::new(
            "At most one todolist item can be in_progress at a time.",
        ));
    }
    let mut items = Vec::with_capacity(args.items.len());
    for item in &args.items {
        // Trim here so the host projection never accepts a todolist item that the WebUI persistence layer would later discard.
        // 此处进行裁剪，避免宿主投影接受后又被 WebUI 持久化层丢弃。
        let step = item.step.trim();
        if step.is_empty() {
            return Err(NativeToolError::new(
                "Todolist item step cannot be empty after trimming.",
            ));
        }
        items.push(json!({
            "step": step,
            "status": item.status.as_key(),
        }));
    }
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(String::from(
            "Todolist updated",
        )))
        .with_host_result(VulcanToolHostResult {
            kind: String::from("todolist"),
            payload: json!({
                "explanation": args.explanation,
                "items": items,
            }),
        }),
    )
}
