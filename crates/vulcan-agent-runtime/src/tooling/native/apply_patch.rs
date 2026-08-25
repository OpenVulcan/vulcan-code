//! Native Apply Patch tool definition, execution adapter, and streaming preview.
//! 原生 Apply Patch 工具定义、执行适配器与流式预览。

use super::{
    NativeToolDefinition, NativeToolDescriptor, NativeToolError, NativeToolExecutionRequest,
    build_native_tool_definition_with_request_streaming,
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;
pub use vulcan_ai_contract::{APPLY_PATCH_LANGUAGE_NAME, APPLY_PATCH_LARK_GRAMMAR};
use vulcan_host_tools::file_runtime::{MAX_EDIT_REQUEST_BYTES, execute_apply_patch};
use vulcan_host_tools::text_document::normalize_line_endings;
use vulcan_tools_manager::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, ToolRequestParserContext,
    ToolRequestParserFactory, ToolRequestParserSession, ToolRequestPreview, ToolRequestPreviewLine,
    ToolRequestProgressPolicy, ToolRequestSensitivityPolicy, ToolRequestStagingPolicy,
    ToolRequestStagingStatus, ToolRequestStreamingCapability, VulcanToolHostResult,
    VulcanToolInvocationResult,
};

/// Stable public identifier for the Apply Patch text tool.
/// Apply Patch 文本工具的稳定公开标识。
pub const APPLY_PATCH_TOOL_ID: &str = "apply_patch";

/// Maximum bytes retained in one streamed patch preview.
/// 单次流式 patch 预览允许保留的最大字节数。
const APPLY_PATCH_PREVIEW_BYTES: usize = 64 * 1024;

/// Maximum structured lines retained in one streamed patch preview.
/// 单次流式 patch 预览允许保留的结构化行数。
const APPLY_PATCH_PREVIEW_LINES: usize = 4096;

/// Builds the native Apply Patch tool for every model route.
/// 为每个模型路由构建原生 Apply Patch 工具。
pub(crate) fn build_apply_patch_tool() -> Arc<NativeToolDefinition> {
    let description = format!(
        "Apply one atomic multi-file change using the {APPLY_PATCH_LANGUAGE_NAME}. Use this tool for multiple edits, multiple files, file creation, deletion, or moves when you can produce the format exactly; use file_edit for a simple exact replacement or when unsure about this format. The input starts with `*** Begin Patch`, contains one or more `*** Add File: path`, `*** Delete File: path`, or `*** Update File: path` operations, and ends with `*** End Patch`. Add-file content lines start with `+`. Update hunks use an optional `@@ exact anchor line`, then prefix unchanged context with a space, deletions with `-`, and insertions with `+`. `*** Move to: destination` may follow an Update File header. Do not use Markdown fences or standard `---`/`+++` unified-diff headers. Minimal update example:\n*** Begin Patch\n*** Update File: src/example.rs\n@@ fn greet() {{\n-    println!(\"old\");\n+    println!(\"new\");\n*** End Patch\nAll files are preflighted before any write; a syntax, context, encoding, stale-file, or destination error writes nothing."
    );
    let help_text = format!(
        "Use one `patch` string containing the complete {APPLY_PATCH_LANGUAGE_NAME}. Example:\n\n*** Begin Patch\n*** Update File: src/example.rs\n@@ fn greet() {{\n-    println!(\"old\");\n+    println!(\"new\");\n*** End Patch\n\nThe exact grammar is:\n```lark\n{APPLY_PATCH_LARK_GRAMMAR}\n```\nRelative paths resolve against the current workspace. Absolute paths and parent-relative paths may target any local filesystem location. The tool preserves existing text encoding, BOM, permissions, and untouched line endings."
    );
    let descriptor = NativeToolDescriptor {
        id: String::from(APPLY_PATCH_TOOL_ID),
        name: String::from("Apply Patch"),
        description,
        help_text: Some(help_text),
        ai_visible: true,
        available_in_plan: false,
    };
    build_native_tool_definition_with_request_streaming(
        descriptor,
        apply_patch_args_schema(),
        Some(apply_patch_request_streaming_capability()),
        |request| Box::pin(async move { execute_apply_patch_tool(request).await }),
    )
}

/// Executes one Apply Patch request and retains its common change set.
/// 执行一次 Apply Patch 请求并保留其公共 change set。
async fn execute_apply_patch_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let Some(object) = request.arguments.as_object() else {
        return Ok(invalid_apply_patch_arguments(
            "apply_patch arguments must be an object containing patch.",
        ));
    };
    if object.len() != 1 || !object.contains_key("patch") {
        return Ok(VulcanToolInvocationResult::from_tool_error_details(
            "apply_patch accepts exactly one patch string.",
            json!({
                "code": "invalid_arguments",
                "message": "apply_patch accepts exactly one patch string.",
            }),
        ));
    }
    let Some(patch) = object.get("patch").and_then(Value::as_str) else {
        return Ok(invalid_apply_patch_arguments(
            "apply_patch patch must be a string.",
        ));
    };
    let patch = patch.to_string();
    let workspace_root = request
        .workspace_root
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| {
            NativeToolError::new("Unable to resolve the current workspace directory.")
        })?;
    let response = match execute_apply_patch(workspace_root, patch).await {
        Ok(response) => response,
        Err(error) => {
            return Ok(VulcanToolInvocationResult::from_tool_error_details(
                error.to_string(),
                error.as_value(),
            ));
        }
    };
    let Some(payload) = response.change_set else {
        return Ok(VulcanToolInvocationResult::from_model_output(
            response.model_output,
        ));
    };
    Ok(VulcanToolInvocationResult {
        model_output: response.model_output,
        host_result: Some(VulcanToolHostResult {
            kind: String::from("change_set"),
            payload,
        }),
    })
}

/// Builds one canonical non-retryable model-facing Apply Patch argument error.
/// 构建一个规范且不可重试的面向模型 Apply Patch 参数错误。
fn invalid_apply_patch_arguments(message: &str) -> VulcanToolInvocationResult {
    VulcanToolInvocationResult::from_tool_error_details(
        message,
        json!({
            "code": "invalid_arguments",
            "message": message,
            "retryable": false,
        }),
    )
}

/// Returns the portable function-tool schema used when custom text tools are unavailable.
/// 返回自定义文本工具不可用时使用的可移植函数工具 Schema。
fn apply_patch_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "patch": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_EDIT_REQUEST_BYTES,
                "description": format!("Required. A complete {APPLY_PATCH_LANGUAGE_NAME}. Start with `*** Begin Patch`, end with `*** End Patch`, use Add/Delete/Update File headers, prefix add lines with `+`, and prefix update context/delete/insert lines with space/`-`/`+`. An optional `@@ exact logical line` anchors the following hunk. The runtime applies all files atomically.")
            }
        },
        "required": ["patch"]
    })
}

/// Builds the bounded side-effect-free patch preview capability.
/// 构建有界且无副作用的 patch 预览能力。
fn apply_patch_request_streaming_capability() -> ToolRequestStreamingCapability {
    ToolRequestStreamingCapability {
        parser_factory: Some(Arc::new(ApplyPatchRequestParserFactory)),
        preview_kind: Some(String::from("apply_patch")),
        staging_policy: ToolRequestStagingPolicy::Forbidden,
        sensitivity_policy: ToolRequestSensitivityPolicy {
            allowed_preview_fields: vec![
                String::from("patch"),
                String::from("operation_count"),
                String::from("old_line_count"),
                String::from("new_line_count"),
                String::from("affected_resources"),
                String::from("body_preview"),
                String::from("structured_lines"),
            ],
            redacted_json_paths: Vec::new(),
            maximum_body_preview_bytes: APPLY_PATCH_PREVIEW_BYTES,
            maximum_structured_preview_lines: APPLY_PATCH_PREVIEW_LINES,
            maximum_structured_preview_bytes: APPLY_PATCH_PREVIEW_BYTES,
        },
        progress_policy: ToolRequestProgressPolicy::default(),
    }
}

/// Creates one isolated patch preview parser.
/// 创建一个隔离的 patch 预览解析器。
struct ApplyPatchRequestParserFactory;

impl ToolRequestParserFactory for ApplyPatchRequestParserFactory {
    /// Creates one parser that only observes decoded arguments.
    /// 创建一个只观察已解码参数的解析器。
    fn create(&self, _context: ToolRequestParserContext) -> Box<dyn ToolRequestParserSession> {
        Box::new(ApplyPatchRequestParser::default())
    }
}

/// Incremental state for one JSON-wrapped patch string.
/// 一份 JSON 包装 patch 字符串的增量状态。
#[derive(Debug, Default)]
struct ApplyPatchRequestParser {
    /// Decoded patch content observed so far.
    /// 目前观察到的已解码 patch 正文。
    patch: String,
    /// Whether the patch string is active.
    /// patch 字符串是否处于活动状态。
    active: bool,
    /// Monotonic preview revision.
    /// 单调递增的预览修订号。
    revision: Cell<u64>,
    /// Whether content changed after the previous snapshot.
    /// 上次快照后正文是否变化。
    dirty: Cell<bool>,
    /// Whether preview retention truncated content.
    /// 预览保留是否截断正文。
    truncated: bool,
}

impl ToolRequestParserSession for ApplyPatchRequestParser {
    /// Consumes decoded patch-string events without influencing execution assembly.
    /// 消费已解码 patch 字符串事件且不影响执行参数组装。
    fn push_json_event(&mut self, event: &IncrementalJsonEvent) -> Result<(), String> {
        match event {
            IncrementalJsonEvent::StringStarted { path } if is_patch_path(path) => {
                self.patch.clear();
                self.active = true;
            }
            IncrementalJsonEvent::StringContent { path, content }
                if self.active && is_patch_path(path) =>
            {
                if self.patch.len().saturating_add(content.len()) > MAX_EDIT_REQUEST_BYTES {
                    return Err(format!(
                        "apply_patch streamed content exceeds {MAX_EDIT_REQUEST_BYTES} bytes"
                    ));
                }
                self.patch.push_str(content);
                self.truncated |= self.patch.len() > APPLY_PATCH_PREVIEW_BYTES;
                self.dirty.set(true);
            }
            IncrementalJsonEvent::StringEnded { path } if is_patch_path(path) => {
                self.active = false;
                self.dirty.set(true);
            }
            _ => {}
        }
        Ok(())
    }

    /// Returns a preview after newly completed patch text arrives.
    /// 在新的完整 patch 文本到达后返回预览。
    fn snapshot(&self) -> Option<ToolRequestPreview> {
        if !self.dirty.replace(false) {
            return None;
        }
        let revision = self.revision.get().saturating_add(1);
        self.revision.set(revision);
        Some(build_patch_preview(&self.patch, revision, self.truncated))
    }

    /// Rebuilds the final preview from authoritative arguments without rejecting execution.
    /// 从权威参数重建最终预览且不拒绝执行。
    fn finish(&mut self, arguments: &Value) -> Result<Option<ToolRequestPreview>, String> {
        self.patch = arguments
            .get("patch")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.active = false;
        self.truncated = self.patch.len() > APPLY_PATCH_PREVIEW_BYTES;
        let revision = self.revision.get().saturating_add(1);
        self.revision.set(revision);
        self.dirty.set(false);
        Ok(Some(build_patch_preview(
            &self.patch,
            revision,
            self.truncated,
        )))
    }

    /// Clears all preview-only state after cancellation.
    /// 在取消后清除全部仅预览状态。
    fn abort(&mut self) {
        *self = Self::default();
    }
}

/// Returns whether an incremental JSON path targets the patch string.
/// 返回增量 JSON 路径是否指向 patch 字符串。
fn is_patch_path(path: &[IncrementalJsonPathSegment]) -> bool {
    matches!(path, [IncrementalJsonPathSegment::Key(field)] if field == "patch")
}

/// Builds one bounded multi-file patch preview from complete logical lines.
/// 从完整逻辑行构建一份有界多文件 patch 预览。
fn build_patch_preview(patch: &str, revision: u64, truncated: bool) -> ToolRequestPreview {
    let normalized = normalize_line_endings(patch);
    let mut lines = normalized.split('\n').collect::<Vec<_>>();
    if !normalized.ends_with('\n') {
        let _ = lines.pop();
    }
    let mut affected_resources = Vec::new();
    let mut structured_lines = Vec::new();
    let mut structured_bytes = 0usize;
    let mut structured_truncated = false;
    let mut operation_count = 0usize;
    let mut operation_index = 0usize;
    let mut resource_index = 0usize;
    let mut old_count = 0usize;
    let mut new_count = 0usize;
    let mut total_old_count = 0usize;
    let mut total_new_count = 0usize;
    for line in lines {
        if let Some(path) = patch_header_path(line) {
            operation_index = operation_count;
            operation_count = operation_count.saturating_add(1);
            resource_index = if let Some(existing_index) = affected_resources
                .iter()
                .position(|existing| existing == path)
            {
                existing_index
            } else {
                affected_resources.push(path.to_string());
                affected_resources.len() - 1
            };
            old_count = 0;
            new_count = 0;
            continue;
        }
        let Some((side, content)) = patch_preview_side(line) else {
            continue;
        };
        let line_index = match side {
            "old" => {
                let line_index = old_count;
                old_count = old_count.saturating_add(1);
                total_old_count = total_old_count.saturating_add(1);
                line_index
            }
            "new" => {
                let line_index = new_count;
                new_count = new_count.saturating_add(1);
                total_new_count = total_new_count.saturating_add(1);
                line_index
            }
            "context" => {
                let line_index = old_count;
                old_count = old_count.saturating_add(1);
                new_count = new_count.saturating_add(1);
                line_index
            }
            _ => continue,
        };
        if structured_lines.len() >= APPLY_PATCH_PREVIEW_LINES {
            structured_truncated = true;
            continue;
        }
        let remaining = APPLY_PATCH_PREVIEW_BYTES.saturating_sub(structured_bytes);
        if remaining == 0 && !content.is_empty() {
            structured_truncated = true;
            continue;
        }
        let bounded_content = truncate_to_boundary(content, remaining);
        structured_truncated |= bounded_content.len() < content.len();
        structured_bytes = structured_bytes.saturating_add(bounded_content.len());
        structured_lines.push(ToolRequestPreviewLine {
            operation_index,
            resource_index,
            side: String::from(side),
            line_index,
            content: bounded_content,
        });
    }
    let mut title_fields = BTreeMap::new();
    title_fields.insert(String::from("operation_count"), json!(operation_count));
    let mut progress_fields = BTreeMap::new();
    progress_fields.insert(String::from("old_line_count"), json!(total_old_count));
    progress_fields.insert(String::from("new_line_count"), json!(total_new_count));
    ToolRequestPreview {
        kind: String::from("apply_patch"),
        revision,
        title_fields,
        progress_fields,
        body_preview: (!patch.is_empty())
            .then(|| truncate_to_boundary(patch, APPLY_PATCH_PREVIEW_BYTES)),
        structured_lines,
        affected_resources,
        redaction_flags: [
            truncated.then_some(String::from("body_preview_truncated")),
            structured_truncated.then_some(String::from("structured_preview_truncated")),
        ]
        .into_iter()
        .flatten()
        .collect(),
        staging_status: ToolRequestStagingStatus::NotApplicable,
    }
}

/// Extracts one file path from an Apply Patch file header.
/// 从 Apply Patch 文件头提取一个文件路径。
fn patch_header_path(line: &str) -> Option<&str> {
    line.strip_prefix("*** Add File: ")
        .or_else(|| line.strip_prefix("*** Delete File: "))
        .or_else(|| line.strip_prefix("*** Update File: "))
}

/// Converts one patch line into a preview side and content.
/// 将一条 patch 行转换为预览侧与正文。
fn patch_preview_side(line: &str) -> Option<(&'static str, &str)> {
    let mut characters = line.chars();
    let prefix = characters.next()?;
    let content = &line[prefix.len_utf8()..];
    match prefix {
        '-' if !line.starts_with("*** ") => Some(("old", content)),
        '+' => Some(("new", content)),
        ' ' => Some(("context", content)),
        _ => None,
    }
}

/// Truncates UTF-8 text at one character boundary.
/// 在字符边界处截断 UTF-8 文本。
fn truncate_to_boundary(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_string();
    }
    let boundary = value
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| *end <= maximum_bytes)
        .last()
        .unwrap_or_default();
    value[..boundary].to_string()
}
