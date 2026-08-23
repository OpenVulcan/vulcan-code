//! Native structured file-edit tool definition, execution adapter, and request preview parser.
//! 原生结构化文件编辑工具定义、执行适配器与请求预览解析器。

use super::{
    NativeToolDefinition, NativeToolDescriptor, NativeToolError, NativeToolExecutionRequest,
    build_native_tool_definition_with_request_streaming,
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;
use vulcan_host_tools::file_runtime::{
    MAX_EDIT_ANCHORS, MAX_EDIT_LINES_PER_ARRAY, MAX_EDIT_OPERATIONS, MAX_EDIT_REQUEST_BYTES,
    MAX_EDIT_TOTAL_LINES, execute_file_edit,
};
use vulcan_tools_manager::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, IncrementalJsonScalar,
    ToolRequestParserContext, ToolRequestParserFactory, ToolRequestParserSession,
    ToolRequestPreview, ToolRequestPreviewLine, ToolRequestProgressPolicy,
    ToolRequestSensitivityPolicy, ToolRequestStagingPolicy, ToolRequestStagingStatus,
    ToolRequestStreamingCapability, VulcanToolHostResult, VulcanToolInvocationResult,
};

/// Stable public identifier for the only native file-writing tool.
/// 唯一原生文件写入工具的稳定公开标识。
pub const FILE_EDIT_TOOL_ID: &str = "file_edit";

/// Maximum bytes retained in one streamed edit preview body.
/// 单次流式编辑预览正文允许保留的最大字节数。
const FILE_EDIT_PREVIEW_BYTES: usize = 32 * 1024;

/// Maximum structured logical lines retained in one streamed edit preview.
/// 单次流式编辑预览允许保留的结构化逻辑行数上限。
const FILE_EDIT_PREVIEW_LINES: usize = 2048;

/// Maximum complete string bytes retained by the side-effect-free stream parser.
/// 无副作用流式解析器允许保留的单个完整字符串最大字节数。
const FILE_EDIT_STREAM_STRING_BYTES: usize = 262_144;

/// Builds the native structured file-edit tool definition.
/// 构建原生结构化文件编辑工具定义。
pub(crate) fn build_file_edit_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(FILE_EDIT_TOOL_ID),
        name: String::from("File Edit"),
        description: String::from(
            "Create, replace, delete, or move UTF-8 text files using explicit operations, complete old-line arrays, and optional ordered literal anchors.",
        ),
        help_text: Some(String::from(
            "Use one operations array. Each old_lines or new_lines item is exactly one logical line without a newline character. A replace operation matches the complete old_lines block; add anchors only when the block is repeated. Anchors are literal lines that must occur before the target in the declared order. The runtime rejects ambiguous or overlapping matches before writing. Use file first for current contents or expected_version. The tool returns a canonical change_set and may show a streamed preview while arguments are arriving.",
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

/// Executes one native file-edit request and preserves the canonical host change-set payload.
/// 执行一次原生文件编辑请求并保留 canonical 宿主变更集载荷。
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

/// Returns the provider-facing JSON Schema for structured file editing.
/// 返回结构化文件编辑面向 Provider 的 JSON Schema。
fn file_edit_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "dry_run": {
                "type": "boolean",
                "description": "Validate, locate, and calculate the complete change_set without writing files. Defaults to false."
            },
            "operations": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_EDIT_OPERATIONS,
                "description": format!("Ordered file operations. Every item requires id, operation, and path. For create provide new_lines; for replace provide old_lines and new_lines; for delete_file provide expected_version; for move_file provide destination and expected_version. Fields that do not belong to the selected operation are rejected at runtime. Multiple replace operations may target one existing file when their original ranges do not overlap. The request body is limited to {MAX_EDIT_REQUEST_BYTES} bytes and all logical-line arrays together are limited to {MAX_EDIT_TOTAL_LINES} items."),
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": 128,
                            "description": "Unique identifier within this call. It is returned in errors, previews, and operation_results."
                        },
                        "operation": {
                            "type": "string",
                            "enum": ["create", "replace", "delete_file", "move_file"],
                            "description": "Required operation selector: create a new file, replace an existing logical-line block, delete one file, or move one file. The runtime applies the selected operation's required and forbidden field rules."
                        },
                        "path": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Workspace-relative or absolute source/target path. You may use the established `${env:NAME}` placeholder syntax; the host expands it once before resolving the path."
                        },
                        "destination": {
                            "type": "string",
                            "minLength": 1,
                            "description": "move_file only. Required for move_file and forbidden for create, replace, and delete_file. The destination must not already exist. It accepts the same relative, absolute, and `${env:NAME}` path forms as path."
                        },
                        "anchors": {
                            "type": "array",
                            "description": "replace only. Optional ordered literal positioning chain. Each item is one complete logical line without CR or LF. Search anchors in the listed order; every later anchor and the final old_lines block must occur after the previous anchor. Anchors are positioning-only: they are not alternatives, regexes, line numbers, AST selectors, or replacement text. Omit them when old_lines is already unique.",
                            "items": {"type": "string", "maxLength": 262144},
                            "maxItems": MAX_EDIT_ANCHORS
                        },
                        "old_lines": {
                            "type": "array",
                            "description": "replace only and required there. The complete consecutive old block matched against the original file snapshot; every array item is exactly one logical line and may be an empty string for a blank line. This is not a keyword list or a line-number range.",
                            "items": {"type": "string", "maxLength": 262144},
                            "maxItems": MAX_EDIT_LINES_PER_ARRAY
                        },
                        "new_lines": {
                            "type": "array",
                            "description": "create or replace only and required there. The complete replacement block; [] creates an empty file for create or deletes the old block during replace. Preserve unchanged lines explicitly when they belong in the replacement.",
                            "items": {"type": "string", "maxLength": 262144},
                            "maxItems": MAX_EDIT_LINES_PER_ARRAY
                        },
                        "match_mode": {
                            "type": "string",
                            "enum": ["exact", "ignore_trailing_whitespace"],
                            "description": "replace only. Matching is exact by default. The optional relaxed mode ignores only trailing whitespace and never ignores indentation, case, or punctuation."
                        },
                        "expected_version": {
                            "type": "string",
                            "description": "Optional for replace and required for delete_file or move_file; copy the opaque version returned by file read/stat. Never construct or edit it."
                        },
                        "final_newline": {
                            "type": "string",
                            "enum": ["preserve", "present", "absent"],
                            "description": "create or replace only. Preserve the existing final-newline state by default, or explicitly require a final newline to be present or absent."
                        }
                    },
                    "required": ["id", "operation", "path"]
                }
            }
        },
        "required": ["operations"]
    })
}

/// Builds the bounded request-streaming capability for file_edit.
/// 构建 file_edit 使用的有界请求流能力。
fn file_edit_request_streaming_capability() -> ToolRequestStreamingCapability {
    ToolRequestStreamingCapability {
        parser_factory: Some(Arc::new(FileEditRequestParserFactory)),
        preview_kind: Some(String::from("file_edit")),
        staging_policy: ToolRequestStagingPolicy::Forbidden,
        sensitivity_policy: ToolRequestSensitivityPolicy {
            allowed_preview_fields: vec![
                String::from("operations"),
                String::from("operation_count"),
                String::from("path"),
                String::from("old_lines"),
                String::from("new_lines"),
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

/// Creates an isolated parser session for one streamed file-edit call.
/// 为一次流式文件编辑调用创建隔离解析器会话。
struct FileEditRequestParserFactory;

impl ToolRequestParserFactory for FileEditRequestParserFactory {
    /// Creates one parser without reading or writing any target file.
    /// 创建一个不读取或写入任何目标文件的解析器。
    fn create(&self, _context: ToolRequestParserContext) -> Box<dyn ToolRequestParserSession> {
        Box::new(FileEditRequestParser::default())
    }
}

/// Incrementally collected operation fields used only for safe request previews.
/// 仅用于安全请求预览的增量操作字段集合。
#[derive(Debug, Default, Clone)]
struct StreamedOperation {
    /// Operation identifier.
    /// 操作标识。
    id: String,
    /// Source or target path.
    /// 源路径或目标路径。
    path: String,
    /// Move destination path when the operation is a move.
    /// move_file 鎿嶄綔鐨勭洰鏍囪矾寰勩€?
    destination: String,
    /// Operation kind.
    /// 操作类型。
    operation: String,
    /// Completed old logical lines.
    /// 已完成的旧逻辑行。
    old_lines: Vec<String>,
    /// Completed new logical lines.
    /// 已完成的新逻辑行。
    new_lines: Vec<String>,
    /// Completed ordered literal anchors.
    /// 已完成的有序字面量锚点。
    anchors: Vec<String>,
    /// Effective line matching mode, defaulting to exact when omitted.
    /// 生效行匹配模式，省略时默认为 exact。
    match_mode: String,
    /// Opaque expected version used for optimistic concurrency checks.
    /// 用于乐观并发检查的不透明期望版本。
    expected_version: Option<String>,
    /// Effective final-newline preference, where preserve is represented as none.
    /// 生效的末尾换行偏好，其中 preserve 用 none 表示。
    final_newline: Option<bool>,
}

/// Incremental file-edit parser that never performs filesystem side effects.
/// 永不执行文件系统副作用的增量文件编辑解析器。
#[derive(Debug, Default)]
struct FileEditRequestParser {
    /// Operations keyed by their array index.
    /// 按数组索引保存的操作。
    operations: BTreeMap<usize, StreamedOperation>,
    /// Currently streamed JSON string path and bounded content.
    /// 当前流式 JSON 字符串路径及有界内容。
    active_string: Option<(Vec<IncrementalJsonPathSegment>, String)>,
    /// Monotonic preview revision.
    /// 单调递增的预览修订号。
    revision: Cell<u64>,
    /// Whether a semantic field changed since the previous snapshot.
    /// 自上次快照后是否发生语义字段变化。
    dirty: Cell<bool>,
    /// Whether preview content exceeded its bounded body budget.
    /// 预览内容是否超过有界正文预算。
    truncated: bool,
    /// Total decoded string bytes retained from the streamed request.
    /// 娴佸紡璇锋眰宸叉敹闆嗙殑瑙ｇ爜瀛楃涓诧紝鐢ㄤ簬闃叉瑙ｆ瀽鍣ㄥ鎺ㄥ姩鍐呭瓨鏃犻檺澧炲姞銆?
    streamed_string_bytes: usize,
    /// Total completed logical array items retained from the streamed request.
    /// 娴佸紡璇锋眰宸叉敹闆嗙殑閫昏緫琛屾暟缁勫厓绱犳€绘暟銆?
    streamed_array_lines: usize,
    /// Top-level dry-run value observed from the request stream.
    /// 从请求流观察到的顶层 dry_run 值。
    dry_run: Option<bool>,
}

impl ToolRequestParserSession for FileEditRequestParser {
    /// Consumes one incremental JSON event and records only safe display fields.
    /// 消费一条增量 JSON 事件，只记录安全展示字段。
    fn push_json_event(&mut self, event: &IncrementalJsonEvent) -> Result<(), String> {
        match event {
            IncrementalJsonEvent::StringStarted { path } => {
                self.active_string = Some((path.clone(), String::new()));
            }
            IncrementalJsonEvent::StringContent { path, content } => {
                let active_path_matches = self
                    .active_string
                    .as_ref()
                    .is_some_and(|(active_path, _)| active_path == path);
                if active_path_matches {
                    self.streamed_string_bytes = self
                        .streamed_string_bytes
                        .checked_add(content.len())
                        .ok_or_else(|| {
                            String::from("file_edit streamed string byte count overflowed")
                        })?;
                    if self.streamed_string_bytes > MAX_EDIT_REQUEST_BYTES {
                        return Err(format!(
                            "file_edit streamed string content exceeds {MAX_EDIT_REQUEST_BYTES} bytes"
                        ));
                    }
                    let Some((_, value)) = self.active_string.as_mut() else {
                        return Err(String::from("file_edit streamed string state disappeared"));
                    };
                    append_streamed_string(value, content, &mut self.truncated)?;
                }
            }
            IncrementalJsonEvent::StringEnded { path } => {
                if let Some((active_path, value)) = self.active_string.take()
                    && active_path == *path
                    && let Some((operation_index, field, line_index)) = operation_string_path(path)
                {
                    if operation_index >= MAX_EDIT_OPERATIONS {
                        return Err(format!(
                            "file_edit streamed operation index exceeds {MAX_EDIT_OPERATIONS}"
                        ));
                    }
                    if let Some(index) = line_index {
                        let maximum = if field == "anchors" {
                            MAX_EDIT_ANCHORS
                        } else {
                            MAX_EDIT_LINES_PER_ARRAY
                        };
                        self.record_array_line(index, maximum)?;
                    }
                    let operation = self.operations.entry(operation_index).or_insert_with(|| {
                        StreamedOperation {
                            match_mode: String::from("exact"),
                            ..StreamedOperation::default()
                        }
                    });
                    match (field, line_index) {
                        ("id", None) => operation.id = value,
                        ("path", None) => operation.path = value,
                        ("destination", None) => operation.destination = value,
                        ("operation", None) => operation.operation = value,
                        ("match_mode", None) => operation.match_mode = value,
                        ("expected_version", None) => operation.expected_version = Some(value),
                        ("final_newline", None) => {
                            operation.final_newline = parse_final_newline_preference(&value)?
                        }
                        ("old_lines", Some(index)) => {
                            set_bounded_line(&mut operation.old_lines, index, value)
                        }
                        ("new_lines", Some(index)) => {
                            set_bounded_line(&mut operation.new_lines, index, value)
                        }
                        ("anchors", Some(index)) => {
                            set_bounded_line(&mut operation.anchors, index, value)
                        }
                        _ => {}
                    }
                    self.dirty.set(true);
                }
            }
            IncrementalJsonEvent::ScalarCompleted { path, value } => {
                if path.as_slice() == [IncrementalJsonPathSegment::Key(String::from("dry_run"))]
                    && let IncrementalJsonScalar::Boolean(value) = value
                {
                    self.dry_run = Some(*value);
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

    /// Returns a new bounded preview only after a meaningful completed field changed.
    /// 仅在有意义的完整字段变化后返回新的有界预览。
    fn snapshot(&self) -> Option<ToolRequestPreview> {
        if !self.dirty.get() {
            return None;
        }
        let revision = self.revision.get().saturating_add(1);
        self.revision.set(revision);
        self.dirty.set(false);
        Some(self.build_preview(revision))
    }

    /// Validates that the completed parser saw the same operation shape as final arguments.
    /// 校验已完成解析器看到的操作形状与最终参数一致。
    fn finish(&mut self, arguments: &Value) -> Result<Option<ToolRequestPreview>, String> {
        let final_dry_run = arguments
            .get("dry_run")
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| String::from("file_edit final dry_run must be boolean"))
            })
            .transpose()?
            .unwrap_or(false);
        if self.dry_run.unwrap_or(false) != final_dry_run {
            return Err(String::from(
                "file_edit streamed preview changed top-level field dry_run",
            ));
        }
        let operations = arguments
            .get("operations")
            .and_then(Value::as_array)
            .ok_or_else(|| String::from("file_edit arguments must contain operations"))?;
        if operations.len() != self.operations.len() {
            return Err(String::from(
                "file_edit streamed preview did not match final operation count",
            ));
        }
        for (index, value) in operations.iter().enumerate() {
            let object = value
                .as_object()
                .ok_or_else(|| format!("file_edit operation {index} is not an object"))?;
            let streamed = self
                .operations
                .get(&index)
                .ok_or_else(|| format!("file_edit streamed preview missed operation {index}"))?;
            for (field, streamed_value) in [
                ("id", &streamed.id),
                ("operation", &streamed.operation),
                ("path", &streamed.path),
            ] {
                let final_value = object.get(field).and_then(Value::as_str).ok_or_else(|| {
                    format!("file_edit final operation {index} is missing string field {field}")
                })?;
                if final_value != streamed_value {
                    return Err(format!(
                        "file_edit streamed preview changed operation {index} field {field}"
                    ));
                }
            }
            match object.get("destination") {
                Some(value) => {
                    let final_value = value.as_str().ok_or_else(|| {
                        format!("file_edit final operation {index} has a non-string destination")
                    })?;
                    if final_value != streamed.destination {
                        return Err(format!(
                            "file_edit streamed preview changed operation {index} field destination"
                        ));
                    }
                }
                None if !streamed.destination.is_empty() => {
                    return Err(format!(
                        "file_edit streamed preview lost operation {index} field destination"
                    ));
                }
                None => {}
            }
            for (field, streamed_value) in [
                ("old_lines", &streamed.old_lines),
                ("new_lines", &streamed.new_lines),
                ("anchors", &streamed.anchors),
            ] {
                let final_lines = object
                    .get(field)
                    .map(|value| {
                        value
                            .as_array()
                            .ok_or_else(|| format!("file_edit final operation {index} has a non-array {field} field"))
                            .and_then(|items| {
                                items
                                    .iter()
                                    .map(|value| {
                                        value
                                            .as_str()
                                            .map(str::to_string)
                                            .ok_or_else(|| format!("file_edit final operation {index} has a non-string {field} item"))
                                    })
                                    .collect::<Result<Vec<_>, _>>()
                            })
                    })
                    .transpose()?
                    .unwrap_or_default();
                /*
                 * The public schema treats omitted optional arrays as empty arrays.
                 * 公开 Schema 将省略的可选数组视为空数组。
                 */
                if &final_lines != streamed_value {
                    return Err(format!(
                        "file_edit streamed preview changed operation {index} field {field}"
                    ));
                }
            }
            let final_match_mode = object
                .get("match_mode")
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        format!("file_edit final operation {index} has a non-string match_mode")
                    })
                })
                .transpose()?
                .unwrap_or_else(|| String::from("exact"));
            if final_match_mode != streamed.match_mode {
                return Err(format!(
                    "file_edit streamed preview changed operation {index} field match_mode"
                ));
            }
            let final_expected_version = object
                .get("expected_version")
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        format!(
                            "file_edit final operation {index} has a non-string expected_version"
                        )
                    })
                })
                .transpose()?;
            if final_expected_version != streamed.expected_version {
                return Err(format!(
                    "file_edit streamed preview changed operation {index} field expected_version"
                ));
            }
            let final_newline = object
                .get("final_newline")
                .map(|value| {
                    value
                        .as_str()
                        .map(parse_final_newline_preference)
                        .ok_or_else(|| {
                            format!(
                                "file_edit final operation {index} has a non-string final_newline"
                            )
                        })?
                })
                .transpose()?
                .flatten();
            if final_newline != streamed.final_newline {
                return Err(format!(
                    "file_edit streamed preview changed operation {index} field final_newline"
                ));
            }
        }
        let revision = self.revision.get().saturating_add(1);
        self.revision.set(revision);
        self.dirty.set(false);
        Ok(Some(self.build_preview(revision)))
    }

    /// Clears all parser state after cancellation or invalid final arguments.
    /// 在取消或最终参数无效后清理全部解析器状态。
    fn abort(&mut self) {
        self.operations.clear();
        self.active_string = None;
        self.dirty.set(false);
        self.truncated = false;
        self.streamed_string_bytes = 0;
        self.streamed_array_lines = 0;
        self.dry_run = None;
    }
}

/// Converts the public final-newline string into the runtime's effective boolean preference.
/// 将公开的末尾换行字符串转换为运行时实际使用的布尔偏好。
fn parse_final_newline_preference(value: &str) -> Result<Option<bool>, String> {
    match value {
        "preserve" => Ok(None),
        "present" => Ok(Some(true)),
        "absent" => Ok(Some(false)),
        _ => Err(format!(
            "file_edit final_newline must be preserve, present, or absent, got {value}"
        )),
    }
}

impl FileEditRequestParser {
    /// Records one completed array item against both index and request-wide limits.
    /// 按数组索引和请求总量限制记录一个已完成的数组元素。
    fn record_array_line(&mut self, index: usize, maximum: usize) -> Result<(), String> {
        if index >= maximum {
            return Err(format!(
                "file_edit streamed array index exceeds {maximum} items"
            ));
        }
        self.streamed_array_lines = self
            .streamed_array_lines
            .checked_add(1)
            .ok_or_else(|| String::from("file_edit streamed array-line count overflowed"))?;
        if self.streamed_array_lines > MAX_EDIT_TOTAL_LINES {
            return Err(format!(
                "file_edit streamed array content exceeds {MAX_EDIT_TOTAL_LINES} logical lines"
            ));
        }
        Ok(())
    }

    /// Builds one bounded request preview from completed logical lines.
    /// 根据已完成的逻辑行构建一份有界请求预览。
    fn build_preview(&self, revision: u64) -> ToolRequestPreview {
        let mut affected_resources = Vec::new();
        let mut body = String::new();
        let mut body_truncated = false;
        let mut structured_lines = Vec::new();
        let mut structured_bytes = 0usize;
        let mut structured_truncated = false;
        let mut old_count = 0usize;
        let mut new_count = 0usize;
        for (operation_index, operation) in &self.operations {
            if !operation.path.is_empty() && !affected_resources.contains(&operation.path) {
                affected_resources.push(operation.path.clone());
            }
            let resource_index = affected_resources
                .iter()
                .position(|path| path == &operation.path)
                .unwrap_or_default();
            if !operation.path.is_empty() {
                append_preview_body(
                    &mut body,
                    &format!("{} ({})", operation.path, operation.operation),
                    false,
                    &mut body_truncated,
                );
            }
            for (line_index, line) in operation.old_lines.iter().enumerate() {
                old_count = old_count.saturating_add(1);
                append_preview_body(&mut body, line, true, &mut body_truncated);
                if !operation.path.is_empty() {
                    append_structured_preview_line(
                        &mut structured_lines,
                        &mut structured_bytes,
                        &mut structured_truncated,
                        *operation_index,
                        resource_index,
                        "old",
                        line_index,
                        line,
                    );
                }
            }
            for (line_index, line) in operation.new_lines.iter().enumerate() {
                new_count = new_count.saturating_add(1);
                append_preview_body(&mut body, line, false, &mut body_truncated);
                if !operation.path.is_empty() {
                    append_structured_preview_line(
                        &mut structured_lines,
                        &mut structured_bytes,
                        &mut structured_truncated,
                        *operation_index,
                        resource_index,
                        "new",
                        line_index,
                        line,
                    );
                }
            }
        }
        let mut title_fields = BTreeMap::new();
        title_fields.insert(
            String::from("operation_count"),
            json!(self.operations.len()),
        );
        let mut progress_fields = BTreeMap::new();
        progress_fields.insert(String::from("old_line_count"), json!(old_count));
        progress_fields.insert(String::from("new_line_count"), json!(new_count));
        ToolRequestPreview {
            kind: String::from("file_edit"),
            revision,
            title_fields,
            progress_fields,
            body_preview: (!body.is_empty()).then_some(body),
            structured_lines,
            affected_resources,
            redaction_flags: {
                let mut flags = Vec::new();
                if self.truncated || body_truncated {
                    flags.push(String::from("body_preview_truncated"));
                }
                if structured_truncated {
                    flags.push(String::from("structured_preview_truncated"));
                }
                flags
            },
            staging_status: ToolRequestStagingStatus::NotApplicable,
        }
    }
}

/// Appends one bounded semantic old/new line without encoding it into display text.
/// 追加一条有界的 old/new 语义行，避免把结构化数据编码到展示文本中。
fn append_structured_preview_line(
    target: &mut Vec<ToolRequestPreviewLine>,
    retained_bytes: &mut usize,
    truncated: &mut bool,
    operation_index: usize,
    resource_index: usize,
    side: &str,
    line_index: usize,
    content: &str,
) {
    if target.len() >= FILE_EDIT_PREVIEW_LINES || *retained_bytes >= FILE_EDIT_PREVIEW_BYTES {
        *truncated = true;
        return;
    }
    let remaining = FILE_EDIT_PREVIEW_BYTES.saturating_sub(*retained_bytes);
    let mut bounded_content = content.to_string();
    truncate_utf8_to_bytes(&mut bounded_content, remaining);
    if bounded_content.len() < content.len() {
        *truncated = true;
    }
    *retained_bytes = retained_bytes.saturating_add(bounded_content.len());
    target.push(ToolRequestPreviewLine {
        operation_index,
        resource_index,
        side: String::from(side),
        line_index,
        content: bounded_content,
    });
}

/// Truncates one UTF-8 string at a character boundary for structured preview retention.
/// 按字符边界截断 UTF-8 字符串，供结构化预览有界保留使用。
fn truncate_utf8_to_bytes(value: &mut String, maximum_bytes: usize) {
    if value.len() <= maximum_bytes {
        return;
    }
    let boundary = value
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| *end <= maximum_bytes)
        .last()
        .unwrap_or_default();
    value.truncate(boundary);
}

/// Appends one decoded string fragment while enforcing the preview byte ceiling.
/// 追加一段已解码字符串，同时执行预览字节上限。
fn append_streamed_string(
    target: &mut String,
    fragment: &str,
    truncated: &mut bool,
) -> Result<(), String> {
    if target.len().saturating_add(fragment.len()) > FILE_EDIT_STREAM_STRING_BYTES {
        return Err(format!(
            "file_edit streamed string exceeds {FILE_EDIT_STREAM_STRING_BYTES} bytes"
        ));
    }
    target.push_str(fragment);
    if target.len() > FILE_EDIT_PREVIEW_BYTES {
        *truncated = true;
    }
    Ok(())
}

/// Appends one bounded display line with a diff-style sign.
/// 使用 Diff 风格符号追加一条有界展示行。
fn append_preview_body(target: &mut String, value: &str, deleted: bool, truncated: &mut bool) {
    if target.len() >= FILE_EDIT_PREVIEW_BYTES {
        *truncated = true;
        return;
    }
    let prefix = if deleted { "- " } else { "+ " };
    let remaining = FILE_EDIT_PREVIEW_BYTES.saturating_sub(target.len());
    let candidate = format!("{prefix}{value}\n");
    let mut end = candidate.len().min(remaining);
    while end > 0 && !candidate.is_char_boundary(end) {
        end -= 1;
    }
    target.push_str(&candidate[..end]);
    if end < candidate.len() {
        *truncated = true;
    }
}

/// Writes one completed streamed line at its array index without allocating unbounded gaps.
/// 将一条完成的流式逻辑行写入数组索引，并避免分配无界空洞。
fn set_bounded_line(lines: &mut Vec<String>, index: usize, value: String) {
    if index >= MAX_EDIT_LINES_PER_ARRAY {
        return;
    }
    while lines.len() <= index {
        lines.push(String::new());
    }
    lines[index] = value;
}

/// Extracts one operation field and optional line-array index from an incremental path.
/// 从增量路径提取操作字段及可选的逻辑行数组索引。
fn operation_string_path(
    path: &[IncrementalJsonPathSegment],
) -> Option<(usize, &str, Option<usize>)> {
    if path.len() < 3 {
        return None;
    }
    let [
        IncrementalJsonPathSegment::Key(root),
        IncrementalJsonPathSegment::Index(operation_index),
        IncrementalJsonPathSegment::Key(field),
        tail @ ..,
    ] = path
    else {
        return None;
    };
    if root != "operations" {
        return None;
    }
    let line_index = match tail {
        [] => None,
        [IncrementalJsonPathSegment::Index(index)] => Some(*index),
        _ => return None,
    };
    Some((*operation_index, field.as_str(), line_index))
}
