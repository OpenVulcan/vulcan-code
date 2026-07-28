//! Provider-neutral host tool event formatting.
//! 供应商无关的宿主工具事件格式化。

use serde_json::{Map, Value};

/// Attaches the exact tool name to runtime metadata.
/// 将精确工具名称附加到运行时元数据。
///
/// # Parameters
/// 参数
/// `metadata` is existing host metadata and `tool_name` is the invoked tool identity.
/// `metadata` 是既有宿主元数据，`tool_name` 是被调用工具身份。
///
/// # Returns
/// 返回值
/// Returns one normalized metadata object.
/// 返回一个规范化元数据对象。
pub fn attach_tool_name_to_runtime_metadata(
    metadata: Option<Value>,
    tool_name: &str,
) -> Option<Value> {
    let mut object = match metadata {
        Some(Value::Object(object)) => object,
        Some(other) => {
            let mut object = Map::new();
            object.insert(String::from("host_metadata"), other);
            object
        }
        None => Map::new(),
    };
    object.insert(
        String::from("tool_name"),
        Value::String(tool_name.to_string()),
    );
    Some(Value::Object(object))
}

/// Formats one complete host tool result for transcript display.
/// 为转录显示格式化一个完整宿主工具结果。
///
/// # Parameters
/// 参数
/// `model_output` is the exact model-visible tool output.
/// `model_output` 是精确的模型可见工具输出。
///
/// # Returns
/// 返回值
/// Returns normalized Markdown detail text.
/// 返回规范化 Markdown 详情文本。
pub(crate) fn format_host_tool_result_runtime_detail(model_output: &str) -> String {
    let normalized_output = if model_output.trim().is_empty() {
        String::from("(empty result)")
    } else {
        model_output.trim().to_string()
    };
    format!("## Tool Result\n\n{normalized_output}")
}

/// Formats one tool call for transcript display.
/// 为转录显示格式化一个工具调用。
///
/// # Parameters
/// 参数
/// `tool_name` is the exact tool identity and `arguments` is its completed argument object.
/// `tool_name` 是精确工具身份，`arguments` 是其完整参数对象。
///
/// # Returns
/// 返回值
/// Returns normalized Markdown detail text.
/// 返回规范化 Markdown 详情文本。
pub(crate) fn format_tool_call_runtime_detail(tool_name: &str, arguments: &Value) -> String {
    let pretty_arguments =
        serde_json::to_string_pretty(arguments).unwrap_or_else(|_| arguments.to_string());
    format!("## Tool Call\n\n`{tool_name}`\n\n## Arguments\n\n```json\n{pretty_arguments}\n```")
}

/// Builds normalized completion metadata for one tool result.
/// 为一个工具结果构建规范化完成元数据。
///
/// # Parameters
/// 参数
/// `tool_call_id` owns the result, `host_metadata` carries structured host data,
/// and `display_state` is the stable transcript state.
/// `tool_call_id` 归属该结果，`host_metadata` 承载结构化宿主数据，
/// `display_state` 是稳定转录状态。
///
/// # Returns
/// 返回值
/// Returns one metadata object keyed by the owning tool call.
/// 返回一个按所属工具调用建立的元数据对象。
pub fn build_tool_result_runtime_metadata(
    tool_call_id: &str,
    host_metadata: Option<&Value>,
    display_state: &str,
) -> Value {
    let mut metadata = Map::new();
    metadata.insert(
        String::from("tool_call_id"),
        Value::String(tool_call_id.to_string()),
    );
    metadata.insert(
        String::from("display_state"),
        Value::String(display_state.to_string()),
    );
    if let Some(Value::Object(object)) = host_metadata {
        metadata.insert(String::from("host_result"), Value::Object(object.clone()));
        metadata.extend(object.clone());
    } else if let Some(other) = host_metadata {
        metadata.insert(String::from("host_metadata"), other.clone());
    }
    Value::Object(metadata)
}
