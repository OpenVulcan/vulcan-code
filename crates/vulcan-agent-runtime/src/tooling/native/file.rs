//! Native single-path file maintenance tool definition and execution adapter.
//! 原生单路径文件维护工具定义与执行适配器。

use super::{
    NativeToolDefinition, NativeToolDescriptor, NativeToolError, NativeToolExecutionRequest,
    build_native_tool_definition,
};
use serde_json::{Value, json};
use std::sync::Arc;
use vulcan_host_tools::file_runtime::{
    DEFAULT_LIST_ENTRIES, FileToolError, MAX_LIST_ENTRIES, execute_file,
};
use vulcan_tools_manager::VulcanToolInvocationResult;

/// Stable public identifier for the native file maintenance tool.
/// 原生文件维护工具的稳定公开标识。
pub const FILE_TOOL_ID: &str = "file";

/// Builds the native single-path file maintenance tool definition.
/// 构建原生单路径文件维护工具定义。
pub(crate) fn build_file_tool() -> Arc<NativeToolDefinition> {
    // Publish lifecycle actions separately from content reads and writes.
    // 将生命周期动作与正文读写分开公布。
    let descriptor = NativeToolDescriptor {
        id: String::from(FILE_TOOL_ID),
        name: String::from("File"),
        description: String::from(
            "List directory entries, inspect metadata, move one path, or delete one regular file. Use file_read to read text, file_write to create or replace complete text, file_edit for localized exact replacement, and apply_patch for patch-shaped edits. This tool does not search file contents and does not recursively delete directories.",
        ),
        help_text: Some(String::from(
            "Provide exactly one path. list performs bounded directory discovery; stat returns metadata; delete removes one regular file; move renames one path to a destination that must not exist. Action-specific fields are rejected when used with another action. read and write are not accepted actions; invoke file_read or file_write instead.",
        )),
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition(descriptor, file_args_schema(), |request| {
        Box::pin(async move { execute_file_tool(request, None).await })
    })
}

/// Adapts a complete request to the shared host runtime and returns its projected result.
/// 将完整请求适配到共享宿主运行时并返回投影结果。
/// `action` is a fixed read/write operation, or None for the restricted lifecycle tool.
/// `action` 是固定读写动作，或以 None 表示受限生命周期工具。
pub(super) async fn execute_file_tool(
    mut request: NativeToolExecutionRequest,
    action: Option<&str>,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    // Reject legacy shapes and caller-controlled actions before entering the compatibility runtime.
    // 在进入兼容运行时之前拒绝旧结构及调用方控制的动作。
    let Some(arguments) = request.arguments.as_object_mut() else {
        return Ok(invalid_file_arguments(
            "File tool arguments must be an object.",
        ));
    };
    if arguments.contains_key("targets") {
        return Ok(invalid_file_arguments(
            "Provide one path; targets is not supported.",
        ));
    }
    if let Some(action) = action {
        if arguments.contains_key("action") {
            return Ok(invalid_file_arguments(
                "This tool does not accept action. Use its declared parameters.",
            ));
        }
        arguments.insert(String::from("action"), json!(action));
    } else if !matches!(
        arguments.get("action").and_then(Value::as_str),
        Some("list" | "stat" | "move" | "delete")
    ) {
        return Ok(invalid_file_arguments(
            "file accepts list, stat, move, or delete. Use file_read for reading and file_write for complete writes.",
        ));
    }
    // Workspace resolution remains identical for every file content and lifecycle entry point.
    // 所有文件正文及生命周期入口保持相同的工作区解析方式。
    let workspace_root = request
        .workspace_root
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| {
            NativeToolError::new("Unable to resolve the current workspace directory.")
        })?;
    // The shared runtime owns encoding, field validation, locks, commit and change-set checks.
    // 共享运行时负责编码、字段校验、锁、提交及变更集检查。
    let response = match execute_file(workspace_root, request.arguments).await {
        Ok(response) => response,
        Err(error) => {
            return Ok(VulcanToolInvocationResult::from_tool_error_details(
                error.to_string(),
                error.as_value(),
            ));
        }
    };
    Ok(super::file_result::project(response))
}

/// Converts an invalid public request message into the standard structured file error result.
/// 将无效公开请求的说明转换为标准结构化文件错误结果。
fn invalid_file_arguments(message: &str) -> VulcanToolInvocationResult {
    // Reuse the host error contract rather than introducing an adapter-specific envelope.
    // 复用宿主错误契约，避免引入适配器专属信封。
    let error = FileToolError::new("invalid_arguments", message);
    VulcanToolInvocationResult::from_tool_error_details(error.to_string(), error.as_value())
}

/// Returns the provider-facing JSON Schema for single-path file maintenance.
/// 返回单路径文件维护面向 Provider 的 JSON Schema。
fn file_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "action": {
                "type": "string",
                "enum": ["list", "stat", "delete", "move"],
                "description": "Required. list discovers bounded directory entries, stat returns metadata, delete removes one regular file, and move renames one path."
            },
            "path": {
                "type": "string",
                "minLength": 1,
                "description": "Required. Relative paths resolve against the current workspace. Absolute paths and parent-relative paths may target any local filesystem location. `${env:NAME}` placeholders are expanded once by the host."
            },
            "recursive": {
                "type": "boolean",
                "default": false,
                "description": "list only. Recurse into child directories. Defaults to false."
            },
            "glob": {
                "type": "string",
                "maxLength": 4096,
                "description": "list only. Optional filename glob using * and ?."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_LIST_ENTRIES,
                "default": DEFAULT_LIST_ENTRIES,
                "description": "list only. Maximum returned entries."
            },
            "destination": {
                "type": "string",
                "minLength": 1,
                "description": "Required for move. Destination path, which must not already exist."
            }
        },
        "required": ["action", "path"]
    })
}
