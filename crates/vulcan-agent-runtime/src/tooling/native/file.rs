//! Native read-only file tool definition and execution adapter.
//! 原生只读文件工具定义与执行适配器。

use super::{
    NativeToolDefinition, NativeToolDescriptor, NativeToolError, NativeToolExecutionRequest,
    build_native_tool_definition,
};
use serde_json::{Value, json};
use std::sync::Arc;
use vulcan_host_tools::file_runtime::{
    MAX_FILE_TARGETS, MAX_LIST_ENTRIES, MAX_READ_LINES, MAX_READ_SEGMENTS, MAX_SEARCH_QUERY_BYTES,
    MAX_SEARCH_RESULTS, execute_file,
};
use vulcan_tools_manager::VulcanToolInvocationResult;

/// Stable public identifier for the read-only file tool.
/// 只读文件工具的稳定公开标识。
pub const FILE_TOOL_ID: &str = "file";

/// Builds the native read-only file tool definition.
/// 构建原生只读文件工具定义。
pub(crate) fn build_file_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(FILE_TOOL_ID),
        name: String::from("File"),
        description: String::from(
            "Read UTF-8 files, list workspace files, search text, or inspect file metadata. This tool never writes files.",
        ),
        help_text: Some(String::from(
            "Use action=read for file contents, list for bounded directory discovery, search for literal or regular-expression text matches, and stat for metadata and an opaque version token. Provide one or more targets with paths. This tool is read-only.",
        )),
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition(descriptor, file_args_schema(), |request| {
        Box::pin(async move { execute_file_tool(request).await })
    })
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
    Ok(VulcanToolInvocationResult::from_model_output(
        response.model_output,
    ))
}

/// Returns the provider-facing JSON Schema for the read-only file tool.
/// 返回只读文件工具面向 Provider 的 JSON Schema。
fn file_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "action": {
                "type": "string",
                "enum": ["read", "list", "search", "stat"],
                "description": "Required. Choose exactly one action: read returns UTF-8 logical lines, list discovers bounded paths, search returns text matches, and stat returns metadata. The selected action determines which optional fields are meaningful; the runtime rejects fields that belong to another action."
            },
            "targets": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_FILE_TARGETS,
                "description": "Required. One or more file or directory targets. Each target must contain a path. The segments and numbered fields are meaningful only for read.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Workspace-relative or absolute file-system path. You may use the established `${env:NAME}` placeholder syntax; the host expands it once before resolving the path."
                        },
                        "segments": {
                            "type": "array",
                            "maxItems": MAX_READ_SEGMENTS,
                            "description": "Read only. Bounded one-based line ranges. Each item has start_line and line_count; omit this field to read the bounded file prefix.",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {
                                    "start_line": {"type": "integer", "minimum": 1},
                                    "line_count": {"type": "integer", "minimum": 1, "maximum": MAX_READ_LINES}
                                },
                                "required": ["start_line", "line_count"]
                            }
                        },
                        "numbered": {
                            "type": "boolean",
                            "description": "Read only. Include one-based line numbers. Defaults to true."
                        }
                    },
                    "required": ["path"]
                }
            },
            "query": {
                "type": "string",
                "maxLength": MAX_SEARCH_QUERY_BYTES,
                "description": "Search only. Literal text by default, or a regular-expression pattern when query_type=regex."
            },
            "query_type": {
                "type": "string",
                "enum": ["literal", "regex"],
                "description": "Search only. Use literal by default; use regex for a regular expression."
            },
            "pattern": {
                "type": "string",
                "maxLength": MAX_SEARCH_QUERY_BYTES,
                "description": "List only. Optional filename glob using * and ?."
            },
            "recursive": {
                "type": "boolean",
                "description": "List/search only. Recurse into directories. Defaults to true."
            },
            "include_ignored": {
                "type": "boolean",
                "description": "List/search only. Include conventional generated and dependency directories. Defaults to false."
            },
            "case_sensitive": {
                "type": "boolean",
                "description": "Search only. Use case-sensitive matching. Defaults to true."
            },
            "context_lines": {
                "type": "integer",
                "minimum": 0,
                "maximum": 20,
                "description": "Search only. Number of context lines before and after each match."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_LIST_ENTRIES,
                "description": format!("List/search only. Bound the number of returned entries or matches. List accepts up to {MAX_LIST_ENTRIES} entries; search accepts up to {MAX_SEARCH_RESULTS} matches, and values above the selected action limit are rejected.")
            }
        },
        "required": ["action", "targets"]
    })
}
