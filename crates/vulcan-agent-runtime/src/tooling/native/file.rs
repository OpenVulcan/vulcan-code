//! Native single-path file maintenance tool definition and execution adapter.
//! 原生单路径文件维护工具定义与执行适配器。

use super::{
    NativeToolDefinition, NativeToolDescriptor, NativeToolError, NativeToolExecutionRequest,
    build_native_tool_definition_with_request_streaming,
};
use serde_json::{Value, json};
use std::sync::Arc;
use vulcan_host_tools::file_runtime::{
    DEFAULT_READ_LINES, DEFAULT_READ_RESPONSE_BYTES, MAX_LIST_ENTRIES, MAX_READ_LINES, execute_file,
};
use vulcan_tools_manager::VulcanToolInvocationResult;

mod request_preview;

/// Stable public identifier for the native file maintenance tool.
/// 原生文件维护工具的稳定公开标识。
pub const FILE_TOOL_ID: &str = "file";

/// Builds the native single-path file maintenance tool definition.
/// 构建原生单路径文件维护工具定义。
pub(crate) fn build_file_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(FILE_TOOL_ID),
        name: String::from("File"),
        description: String::from(
            "Read files and perform whole-file lifecycle operations on one filesystem path. Use file_edit for a localized exact replacement and apply_patch for multiple hunks or multiple files. This tool does not search file contents and does not recursively delete directories. It preserves an existing text file's encoding and BOM unless you explicitly override them.",
        ),
        help_text: Some(String::from(
            "Provide exactly one path. read returns one Markdown block with real source line prefixes such as L12:; truncated reads include the next start_line. list performs bounded directory discovery; stat returns metadata; write replaces a complete file; delete removes one regular file; move renames one path to a destination that must not exist. Action-specific fields are rejected when used with another action.",
        )),
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition_with_request_streaming(
        descriptor,
        file_args_schema(),
        Some(request_preview::capability()),
        |request| Box::pin(async move { execute_file_tool(request).await }),
    )
}

/// Executes one native file request and converts the shared host response.
/// 执行一次原生文件请求并转换共享宿主响应。
async fn execute_file_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let workspace_root = request
        .workspace_root
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| {
            NativeToolError::new("Unable to resolve the current workspace directory.")
        })?;
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

/// Returns the provider-facing JSON Schema for single-path file maintenance.
/// 返回单路径文件维护面向 Provider 的 JSON Schema。
fn file_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "action": {
                "type": "string",
                "enum": ["read", "list", "stat", "write", "delete", "move"],
                "description": "Required. read returns decoded code content, list discovers bounded directory entries, stat returns metadata, write replaces a complete file, delete removes one regular file, and move renames one path."
            },
            "path": {
                "type": "string",
                "minLength": 1,
                "description": "Required. Relative paths resolve against the current workspace. Absolute paths and parent-relative paths may target any local filesystem location. `${env:NAME}` placeholders are expanded once by the host."
            },
            "start_line": {
                "type": "integer",
                "minimum": 1,
                "description": "read only. First one-based logical line to return. Defaults to 1."
            },
            "end_line": {
                "type": "integer",
                "minimum": 1,
                "description": format!("read only. Inclusive one-based final logical line. Omit for a page of at most {DEFAULT_READ_LINES} lines and {DEFAULT_READ_RESPONSE_BYTES} serialized bytes. Explicit ranges allow up to {MAX_READ_LINES} lines, subject to the hard response budget. Follow next_start_line when truncated.")
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
                "default": 200,
                "description": "list only. Maximum returned entries."
            },
            "content": {
                "type": "string",
                "description": "Required for write. Complete replacement text. Use file_edit for exact substring replacement."
            },
            "destination": {
                "type": "string",
                "minLength": 1,
                "description": "Required for move. Destination path, which must not already exist."
            },
            "encoding": {
                "type": "string",
                "minLength": 1,
                "description": "read/write only. Explicit decoding (read) or target encoding (write), such as utf-8, utf-16le, gbk, big5, or shift_jis. Explicit reads validate BOM compatibility and byte sequences; the reported encoding is the chosen codec, not independent proof of the original encoding. Omit to detect reads and preserve existing files."
            },
            "source_encoding": {
                "type": "string",
                "minLength": 1,
                "description": "write only, existing files only. Confirm the original file's encoding when automatic detection is uncertain or the file uses BOM-less Unicode. This controls source decoding; encoding separately selects the output encoding. Omit encoding to preserve the confirmed source encoding."
            },
            "bom": {
                "type": "string",
                "enum": ["preserve", "present", "absent"],
                "default": "preserve",
                "description": "write only. Preserve an existing BOM by default, require an encoding-compatible BOM, or remove it."
            },
            "line_ending": {
                "type": "string",
                "enum": ["preserve", "lf", "crlf", "cr", "lfcr"],
                "default": "preserve",
                "description": "write only. Preserve the existing file's preferred line ending by default; new files default to LF. Select an explicit value to normalize the complete content."
            }
        },
        "required": ["action", "path"]
    })
}
