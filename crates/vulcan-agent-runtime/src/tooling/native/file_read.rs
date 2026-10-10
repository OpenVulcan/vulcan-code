//! Dedicated text reading tool backed by the shared file runtime.
//! 由共享文件运行时支撑的独立文本读取工具。

use super::{NativeToolDefinition, NativeToolDescriptor, build_native_tool_definition};
use serde_json::{Value, json};
use std::sync::Arc;
use vulcan_host_tools::file_runtime::{
    DEFAULT_READ_LINES, DEFAULT_READ_RESPONSE_BYTES, MAX_READ_LINES,
};

/// Stable public identity for text reads.
/// 文本读取的稳定公开标识。
pub const FILE_READ_TOOL_ID: &str = "file_read";

/// Builds the read-only definition; takes no arguments and returns shared tool metadata and execution.
/// 构建只读定义；无参数，返回共享工具元数据及执行入口。
pub(crate) fn build_file_read_tool() -> Arc<NativeToolDefinition> {
    // Read identity is available before any streamed arguments and is allowed in Plan mode.
    // 读取身份在任何流式参数到达前即可确定，并允许在计划模式使用。
    let descriptor = NativeToolDescriptor {
        id: String::from(FILE_READ_TOOL_ID),
        name: String::from("File Read"),
        description: String::from(
            "Read text from one file, optionally limited to a logical line range. Returns Markdown with real source line prefixes such as L12:. This tool never writes files. Use file for directory listing or metadata, file_edit for localized changes, and file_write for complete text replacement.",
        ),
        help_text: Some(String::from(
            "Provide path without action. Truncated reads include the next start_line. Optional encoding selects source decoding and validates byte sequences and BOM compatibility; it is a declared codec, not independent proof of the original encoding. Unverified automatic decoding must be confirmed before editing.",
        )),
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition(descriptor, file_read_args_schema(), |request| {
        Box::pin(super::file::execute_file_tool(request, Some("read")))
    })
}

/// Returns the action-free provider schema; limits come from the shared runtime constants.
/// 返回不含动作字段的供应商结构；限制来自共享运行时常量。
fn file_read_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "path": {
                "type": "string", "minLength": 1,
                "description": "Required. Relative paths resolve against the workspace. Absolute and parent-relative paths may target any local filesystem location. `${env:NAME}` placeholders are expanded once by the host."
            },
            "start_line": {
                "type": "integer", "minimum": 1,
                "description": "First one-based logical line to return. Defaults to 1."
            },
            "end_line": {
                "type": "integer", "minimum": 1,
                "description": format!("Inclusive one-based final logical line. Omit for a page of at most {DEFAULT_READ_LINES} lines and {DEFAULT_READ_RESPONSE_BYTES} serialized bytes. Explicit ranges allow up to {MAX_READ_LINES} lines, subject to the hard response budget. Follow next_start_line when truncated.")
            },
            "encoding": {
                "type": "string", "minLength": 1,
                "description": "Optional source decoding, such as utf-8, utf-16le, gbk, big5, or shift_jis. Validates BOM compatibility and byte sequences. Omit for automatic detection."
            }
        },
        "required": ["path"]
    })
}
