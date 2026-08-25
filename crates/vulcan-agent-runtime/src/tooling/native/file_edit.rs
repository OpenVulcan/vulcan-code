//! Native exact-text file editing definition, execution adapter, and safe request preview.
//! 原生精确文本文件编辑定义、执行适配器与安全请求预览。

use super::{
    NativeToolDefinition, NativeToolDescriptor, NativeToolError, NativeToolExecutionRequest,
    build_native_tool_definition_with_request_streaming,
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;
use vulcan_host_tools::file_runtime::{MAX_EDIT_REQUEST_BYTES, execute_file_edit};
use vulcan_host_tools::text_document::normalize_line_endings;
use vulcan_tools_manager::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, IncrementalJsonScalar,
    ToolRequestParserContext, ToolRequestParserFactory, ToolRequestParserSession,
    ToolRequestPreview, ToolRequestPreviewLine, ToolRequestProgressPolicy,
    ToolRequestSensitivityPolicy, ToolRequestStagingPolicy, ToolRequestStagingStatus,
    ToolRequestStreamingCapability, VulcanToolHostResult, VulcanToolInvocationResult,
};

/// Stable public identifier for exact text replacement.
/// 精确文本替换的稳定公开标识。
pub const FILE_EDIT_TOOL_ID: &str = "file_edit";

/// Maximum bytes retained in one streamed edit preview body.
/// 单次流式编辑预览正文允许保留的最大字节数。
const FILE_EDIT_PREVIEW_BYTES: usize = 32 * 1024;

/// Maximum semantic lines retained in one streamed edit preview.
/// 单次流式编辑预览允许保留的语义行数。
const FILE_EDIT_PREVIEW_LINES: usize = 2048;

/// Builds the native exact-text replacement tool.
/// 构建原生精确文本替换工具。
pub(crate) fn build_file_edit_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(FILE_EDIT_TOOL_ID),
        name: String::from("File Edit"),
        description: String::from(
            "Replace one exact text substring in one existing file. Provide path, old_string, and new_string. Line-ending spelling is normalized for matching, so CRLF, LFCR, CR, and LF are equivalent logical boundaries. By default old_string must occur exactly once; set replace_all=true only when every non-overlapping occurrence should change. The file's encoding, BOM, untouched line endings, and permissions are preserved, and no bytes are written when matching or encoding fails.",
        ),
        help_text: Some(String::from(
            "Read the current file first, then copy a distinctive exact old_string with enough surrounding text to make it unique. new_string is the complete replacement for that substring and may be empty. Do not send line-number ranges, arrays, anchors, operation IDs, dry_run, or expected_version. Use file for whole-file lifecycle actions and apply_patch for patch-shaped or multi-file edits.",
        )),
        ai_visible: true,
        available_in_plan: false,
    };
    build_native_tool_definition_with_request_streaming(
        descriptor,
        file_edit_args_schema(),
        Some(file_edit_request_streaming_capability()),
        |request| Box::pin(async move { execute_file_edit_tool(request).await }),
    )
}

/// Executes one exact-text edit and retains its canonical change set.
/// 执行一次精确文本编辑并保留其 canonical 变更集。
async fn execute_file_edit_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let workspace_root = request
        .workspace_root
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| {
            NativeToolError::new("Unable to resolve the current workspace directory.")
        })?;
    let response = match execute_file_edit(workspace_root, request.arguments).await {
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

/// Returns the portable minimal JSON Schema for exact text replacement.
/// 返回精确文本替换使用的可移植最小 JSON Schema。
fn file_edit_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "path": {
                "type": "string",
                "minLength": 1,
                "description": "Required. One existing text-file path. Relative paths resolve against the current workspace; absolute paths and parent-relative paths may target any local filesystem location. `${env:NAME}` placeholders are expanded once by the host."
            },
            "old_string": {
                "type": "string",
                "minLength": 1,
                "description": "Required. Exact decoded text to replace. Include enough unchanged surrounding text to make the match unique. CRLF, LFCR, CR, and LF spellings are treated as equivalent logical line endings."
            },
            "new_string": {
                "type": "string",
                "description": "Required. Complete replacement text. Use an empty string to delete old_string. Inserted logical line endings inherit the closest original style."
            },
            "replace_all": {
                "type": "boolean",
                "default": false,
                "description": "Replace every non-overlapping occurrence. Defaults to false, which rejects multiple matches instead of guessing."
            }
        },
        "required": ["path", "old_string", "new_string"]
    })
}

/// Builds the bounded side-effect-free streaming preview capability.
/// 构建有界且无副作用的流式预览能力。
fn file_edit_request_streaming_capability() -> ToolRequestStreamingCapability {
    ToolRequestStreamingCapability {
        parser_factory: Some(Arc::new(FileEditRequestParserFactory)),
        preview_kind: Some(String::from("file_edit")),
        staging_policy: ToolRequestStagingPolicy::Forbidden,
        sensitivity_policy: ToolRequestSensitivityPolicy {
            allowed_preview_fields: vec![
                String::from("path"),
                String::from("old_string"),
                String::from("new_string"),
                String::from("old_line_count"),
                String::from("new_line_count"),
                String::from("affected_resources"),
                String::from("body_preview"),
                String::from("structured_lines"),
            ],
            redacted_json_paths: Vec::new(),
            maximum_body_preview_bytes: FILE_EDIT_PREVIEW_BYTES,
            maximum_structured_preview_lines: FILE_EDIT_PREVIEW_LINES,
            maximum_structured_preview_bytes: FILE_EDIT_PREVIEW_BYTES,
        },
        progress_policy: ToolRequestProgressPolicy::default(),
    }
}

/// Creates one isolated exact-edit preview parser.
/// 创建一个隔离的精确编辑预览解析器。
struct FileEditRequestParserFactory;

impl ToolRequestParserFactory for FileEditRequestParserFactory {
    /// Creates a parser that never touches the filesystem.
    /// 创建一个永不访问文件系统的解析器。
    fn create(&self, _context: ToolRequestParserContext) -> Box<dyn ToolRequestParserSession> {
        Box::new(FileEditRequestParser::default())
    }
}

/// Bounded incremental state for one file_edit request.
/// 单次 file_edit 请求的有界增量状态。
#[derive(Debug, Default)]
struct FileEditRequestParser {
    /// Completed path field.
    /// 已完成的 path 字段。
    path: String,
    /// Completed old text.
    /// 已完成的旧文本。
    old_string: String,
    /// Completed new text.
    /// 已完成的新文本。
    new_string: String,
    /// Explicit replace-all choice.
    /// 显式全部替换选择。
    replace_all: bool,
    /// Active top-level JSON string field and decoded content.
    /// 当前顶层 JSON 字符串字段及已解码内容。
    active_string: Option<(String, String)>,
    /// Monotonic preview revision.
    /// 单调递增的预览修订号。
    revision: Cell<u64>,
    /// Whether display state changed after the last snapshot.
    /// 上次快照后展示状态是否变化。
    dirty: Cell<bool>,
    /// Whether retained display content was truncated.
    /// 保留的展示内容是否被截断。
    truncated: bool,
    /// Total decoded string bytes observed from the stream.
    /// 从流中观察到的已解码字符串总字节数。
    streamed_bytes: usize,
}

impl ToolRequestParserSession for FileEditRequestParser {
    /// Consumes safe incremental JSON events without affecting final argument assembly.
    /// 消费安全的增量 JSON 事件而不影响最终参数组装。
    fn push_json_event(&mut self, event: &IncrementalJsonEvent) -> Result<(), String> {
        match event {
            IncrementalJsonEvent::StringStarted { path } => {
                if let Some(field) = top_level_field(path) {
                    self.active_string = Some((field.to_string(), String::new()));
                }
            }
            IncrementalJsonEvent::StringContent { path, content } => {
                if top_level_field(path).is_some()
                    && let Some((_, value)) = self.active_string.as_mut()
                {
                    self.streamed_bytes = self
                        .streamed_bytes
                        .checked_add(content.len())
                        .ok_or_else(|| String::from("file_edit streamed byte count overflowed"))?;
                    if self.streamed_bytes > MAX_EDIT_REQUEST_BYTES {
                        return Err(format!(
                            "file_edit streamed content exceeds {MAX_EDIT_REQUEST_BYTES} bytes"
                        ));
                    }
                    value.push_str(content);
                    self.truncated |= value.len() > FILE_EDIT_PREVIEW_BYTES;
                    self.dirty.set(true);
                }
            }
            IncrementalJsonEvent::StringEnded { path } => {
                if let Some(field) = top_level_field(path)
                    && let Some((active_field, value)) = self.active_string.take()
                    && active_field == field
                {
                    self.set_string(field, value);
                    self.dirty.set(true);
                }
            }
            IncrementalJsonEvent::ScalarCompleted { path, value } => {
                if top_level_field(path) == Some("replace_all")
                    && let IncrementalJsonScalar::Boolean(replace_all) = value
                {
                    self.replace_all = *replace_all;
                    self.dirty.set(true);
                }
            }
            IncrementalJsonEvent::ObjectStarted { .. }
            | IncrementalJsonEvent::ObjectEnded { .. }
            | IncrementalJsonEvent::ArrayStarted { .. }
            | IncrementalJsonEvent::ArrayEnded { .. }
            | IncrementalJsonEvent::ObjectKey { .. } => {}
        }
        Ok(())
    }

    /// Returns a new preview after semantic content changes.
    /// 在语义内容变化后返回新预览。
    fn snapshot(&self) -> Option<ToolRequestPreview> {
        if !self.dirty.replace(false) {
            return None;
        }
        let revision = self.revision.get().saturating_add(1);
        self.revision.set(revision);
        Some(self.build_preview(revision))
    }

    /// Rebuilds preview state from authoritative final arguments without rejecting execution.
    /// 从权威最终参数重建预览状态且不拒绝执行。
    fn finish(&mut self, arguments: &Value) -> Result<Option<ToolRequestPreview>, String> {
        if let Some(object) = arguments.as_object() {
            self.path = object
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.old_string = object
                .get("old_string")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.new_string = object
                .get("new_string")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.replace_all = object
                .get("replace_all")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        }
        self.active_string = None;
        let revision = self.revision.get().saturating_add(1);
        self.revision.set(revision);
        self.dirty.set(false);
        Ok(Some(self.build_preview(revision)))
    }

    /// Clears all preview-only state after cancellation or invalid final JSON.
    /// 在取消或最终 JSON 无效后清除全部仅预览状态。
    fn abort(&mut self) {
        *self = Self::default();
    }
}

impl FileEditRequestParser {
    /// Stores one completed model-visible string field.
    /// 保存一个已完成的模型可见字符串字段。
    fn set_string(&mut self, field: &str, value: String) {
        match field {
            "path" => self.path = value,
            "old_string" => self.old_string = value,
            "new_string" => self.new_string = value,
            _ => {}
        }
    }

    /// Builds one bounded diff-style preview from decoded text fields.
    /// 从已解码文本字段构建一份有界 Diff 风格预览。
    fn build_preview(&self, revision: u64) -> ToolRequestPreview {
        let active = self.active_string.as_ref();
        let path = active
            .filter(|(field, _)| field == "path")
            .map(|(_, value)| value.as_str())
            .unwrap_or(&self.path);
        let old_string = active
            .filter(|(field, _)| field == "old_string")
            .map(|(_, value)| value.as_str())
            .unwrap_or(&self.old_string);
        let new_string = active
            .filter(|(field, _)| field == "new_string")
            .map(|(_, value)| value.as_str())
            .unwrap_or(&self.new_string);
        let old_lines = preview_lines(old_string);
        let new_lines = preview_lines(new_string);
        let mut body = String::new();
        let mut structured_lines = Vec::new();
        let mut structured_bytes = 0usize;
        let mut body_truncated = self.truncated;
        let mut structured_truncated = self.truncated;
        append_preview_lines(
            &mut body,
            &mut structured_lines,
            &mut structured_bytes,
            &mut body_truncated,
            &mut structured_truncated,
            &old_lines,
            "old",
            "- ",
        );
        append_preview_lines(
            &mut body,
            &mut structured_lines,
            &mut structured_bytes,
            &mut body_truncated,
            &mut structured_truncated,
            &new_lines,
            "new",
            "+ ",
        );
        let mut title_fields = BTreeMap::new();
        title_fields.insert(String::from("path"), json!(path));
        let mut progress_fields = BTreeMap::new();
        progress_fields.insert(String::from("old_line_count"), json!(old_lines.len()));
        progress_fields.insert(String::from("new_line_count"), json!(new_lines.len()));
        progress_fields.insert(String::from("replace_all"), json!(self.replace_all));
        ToolRequestPreview {
            kind: String::from("file_edit"),
            revision,
            title_fields,
            progress_fields,
            body_preview: (!body.is_empty()).then_some(body),
            structured_lines,
            affected_resources: if path.is_empty() {
                Vec::new()
            } else {
                vec![path.to_string()]
            },
            redaction_flags: [
                body_truncated.then_some(String::from("body_preview_truncated")),
                structured_truncated.then_some(String::from("structured_preview_truncated")),
            ]
            .into_iter()
            .flatten()
            .collect(),
            staging_status: ToolRequestStagingStatus::NotApplicable,
        }
    }
}

/// Returns the top-level field name for one incremental JSON path.
/// 返回一个增量 JSON 路径对应的顶层字段名。
fn top_level_field(path: &[IncrementalJsonPathSegment]) -> Option<&str> {
    match path {
        [IncrementalJsonPathSegment::Key(field)] => Some(field.as_str()),
        _ => None,
    }
}

/// Splits preview text using all supported logical line endings.
/// 使用全部受支持的逻辑换行拆分预览文本。
fn preview_lines(value: &str) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }
    let normalized = normalize_line_endings(value);
    let mut lines = normalized
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if normalized.ends_with('\n') {
        let _ = lines.pop();
    }
    lines
}

/// Appends bounded display and structured lines for one diff side.
/// 为 Diff 一侧追加有界展示行与结构化行。
fn append_preview_lines(
    body: &mut String,
    structured: &mut Vec<ToolRequestPreviewLine>,
    structured_bytes: &mut usize,
    body_truncated: &mut bool,
    structured_truncated: &mut bool,
    lines: &[String],
    side: &str,
    prefix: &str,
) {
    for (line_index, content) in lines.iter().enumerate() {
        if body.len() < FILE_EDIT_PREVIEW_BYTES {
            let remaining = FILE_EDIT_PREVIEW_BYTES.saturating_sub(body.len());
            let candidate = format!("{prefix}{content}\n");
            let bounded = truncate_to_boundary(&candidate, remaining);
            *body_truncated |= bounded.len() < candidate.len();
            body.push_str(&bounded);
        } else if !content.is_empty() || !prefix.is_empty() {
            *body_truncated = true;
        }
        if structured.len() >= FILE_EDIT_PREVIEW_LINES {
            *structured_truncated = true;
            break;
        }
        let remaining = FILE_EDIT_PREVIEW_BYTES.saturating_sub(*structured_bytes);
        if remaining == 0 && !content.is_empty() {
            *structured_truncated = true;
            break;
        }
        let bounded_content = truncate_to_boundary(content, remaining);
        *structured_truncated |= bounded_content.len() < content.len();
        *structured_bytes = structured_bytes.saturating_add(bounded_content.len());
        structured.push(ToolRequestPreviewLine {
            operation_index: 0,
            resource_index: 0,
            side: String::from(side),
            line_index,
            content: bounded_content,
        });
    }
}

/// Truncates UTF-8 text at a character boundary.
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
