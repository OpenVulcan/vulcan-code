//! Declare-intent native tool: explains the next tool call to the permission reviewer.
//! 声明意图原生工具：向权限复核器说明下一次工具调用。
//!
//! The permission gate keeps the intent when it admits this call and hands it to the next tool
//! call only; this tool merely validates it and confirms.
//! 权限闸门在准入本调用时保存意图，并只交给下一次工具调用；本工具仅校验并确认。

use super::*;

/// Builds the tool that declares the intent of the next tool call.
/// 构建声明下一次工具调用意图的工具。
pub(crate) fn build_declare_intent_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(DECLARE_INTENT_TOOL_ID),
        name: String::from(DECLARE_INTENT_TOOL_ID),
        description: String::from(
            "Explain why your next tool call is needed, for the permission reviewer that decides calls needing approval. You normally do not need this tool. Use it right before a call the reviewer denied or is likely to question, such as a file write outside the workspace, when the reason is not obvious from the call itself. The intent accompanies only the next tool call; command tools can take a `purpose` argument instead.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition(
        descriptor,
        declare_intent_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_declare_intent_tool(request) })
        },
    )
}

/// Parsed argument payload accepted by the `declare_intent` tool.
/// `declare_intent` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclareIntentArgs {
    /// Why the next tool call is needed.
    /// 下一次工具调用的必要理由。
    intent: String,
}

/// Builds the model-facing JSON Schema for `declare_intent`.
/// 构建 `declare_intent` 面向模型的 JSON Schema。
fn declare_intent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "intent": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_PERMISSION_REASON_CHARS,
                "description": "What the next tool call is for and why it is needed, written for the permission reviewer."
            }
        },
        "required": ["intent"],
        "additionalProperties": false
    })
}

/// Returns the declared intent when it is non-blank and within the reviewer's limit.
/// 意图非空白且未超过复核器上限时返回该意图。
pub(crate) fn declared_intent(arguments: &Value) -> Option<String> {
    let intent = arguments.get("intent")?.as_str()?.trim();
    (!intent.is_empty() && intent.chars().count() <= MAX_PERMISSION_REASON_CHARS)
        .then(|| intent.to_string())
}

/// Validates one `declare_intent` request; the gate already kept the intent.
/// 校验一次 `declare_intent` 请求；闸门已保存该意图。
fn execute_declare_intent_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: DeclareIntentArgs = serde_json::from_value(request.arguments).map_err(|error| {
        NativeToolError::new(format!("failed to parse function arguments: {error}"))
    })?;
    if declared_intent(&json!({ "intent": args.intent })).is_none() {
        return Err(NativeToolError::new(format!(
            "The intent must be non-blank and at most {MAX_PERMISSION_REASON_CHARS} characters."
        )));
    }
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(String::from(
            "Intent recorded. It accompanies your next tool call to the permission reviewer, and only that call.",
        )),
    ))
}
