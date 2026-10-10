//! Dedicated complete-file writing tool with side-effect-free request progress.
//! 带无副作用请求进度的独立整文件写入工具。

use super::{
    NativeToolDefinition, NativeToolDescriptor, build_native_tool_definition_with_request_streaming,
};
use serde_json::{Value, json};
use std::sync::Arc;

mod request_preview;

/// Stable public identity for complete-file writes.
/// 整文件写入的稳定公开标识。
pub const FILE_WRITE_TOOL_ID: &str = "file_write";

/// Builds the write definition; takes no arguments and returns metadata, execution and preview capability.
/// 构建写入定义；无参数，返回元数据、执行入口及预览能力。
pub(crate) fn build_file_write_tool() -> Arc<NativeToolDefinition> {
    // Make full replacement explicit at tool selection, before content or path is generated.
    // 在工具选择时就明确完整替换语义，早于正文或路径生成。
    let descriptor = NativeToolDescriptor {
        id: String::from(FILE_WRITE_TOOL_ID),
        name: String::from("File Write"),
        description: String::from(
            "Create one text file or overwrite an existing file with the complete supplied content. Existing content is replaced in full, not appended or locally edited. For localized changes to an existing file, prefer file_edit or apply_patch. Preserves existing encoding, BOM, preferred line endings and permissions unless explicitly overridden.",
        ),
        help_text: Some(String::from(
            "Provide path and the complete content without action. Prefer emitting path before content so progress can show the filename immediately. Empty content creates or truncates to an empty file. Read an existing file with file_read before replacing it. source_encoding confirms existing source decoding; encoding independently selects the output encoding. Uncertain source detection blocks writes until source_encoding is provided. Omit encoding to preserve the confirmed source encoding. For new files, use encoding only; the default is UTF-8 with LF. Use file for move or delete.",
        )),
        ai_visible: true,
        available_in_plan: false,
    };
    build_native_tool_definition_with_request_streaming(
        descriptor,
        file_write_args_schema(),
        Some(request_preview::capability()),
        |request| Box::pin(super::file::execute_file_tool(request, Some("write"))),
    )
}

/// Returns the complete-write provider schema; path and content are required, with no action selector.
/// 返回整文件写入的供应商结构；路径与正文必填，不提供动作选择器。
fn file_write_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "path": {
                "type": "string", "minLength": 1,
                "description": "Required. Emit before content when possible. Relative paths resolve against the workspace. Absolute and parent-relative paths may target any local filesystem location. `${env:NAME}` placeholders are expanded once by the host."
            },
            "content": {
                "type": "string",
                "description": "Required complete replacement text, including every part that should remain. Empty text truncates the file. For a partial change, use file_edit instead."
            },
            "encoding": {
                "type": "string", "minLength": 1,
                "description": "Optional target encoding, such as utf-8, utf-16le, gbk, big5, or shift_jis. Omit to preserve the confirmed existing source encoding or use UTF-8 for a new file. Does not confirm source decoding."
            },
            "source_encoding": {
                "type": "string", "minLength": 1,
                "description": "Existing files only. Confirm source decoding when automatic detection is uncertain or the file uses BOM-less Unicode. encoding independently selects the output encoding."
            },
            "bom": {
                "type": "string", "enum": ["preserve", "present", "absent"], "default": "preserve",
                "description": "Preserve an existing BOM by default, require an encoding-compatible BOM, or remove it."
            },
            "line_ending": {
                "type": "string", "enum": ["preserve", "lf", "crlf", "cr", "lfcr"], "default": "preserve",
                "description": "Preserve the existing file's preferred line ending by default; new files default to LF. An explicit value normalizes the complete content."
            }
        },
        "required": ["path", "content"]
    })
}
