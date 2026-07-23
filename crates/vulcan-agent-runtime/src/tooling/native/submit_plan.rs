//! Submit-plan native tool implementation.
//! 提交计划原生工具实现。

use super::*;
use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use vulcan_tools_manager::{
    IncrementalJsonEvent, IncrementalJsonPathSegment, IncrementalJsonScalar,
    ToolRequestParserContext, ToolRequestParserFactory, ToolRequestParserSession,
    ToolRequestPreview, ToolRequestProgressPolicy, ToolRequestSensitivityPolicy,
    ToolRequestStagingPolicy, ToolRequestStagingStatus, ToolRequestStreamingCapability,
};

/// Hidden marker proving a Markdown plan was created by the submit_plan tool.
/// 证明 Markdown 计划由 submit_plan 工具创建的隐藏标记。
pub(crate) const SUBMIT_PLAN_MARKER: &str = "<!-- generated-by: vulcan-submit-plan -->";

/// Parsed argument payload accepted by the `submit_plan` tool.
/// `submit_plan` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitPlanArgs {
    /// Human-readable plan name used for the generated filename.
    /// 用于生成文件名的人类可读计划名称。
    name: String,
    /// Complete Markdown plan content to write.
    /// 需要写入的完整 Markdown 计划内容。
    content: String,
    /// Optional target directory under the workspace root.
    /// 工作区根目录下的可选目标目录。
    directory: Option<String>,
    /// Whether to replace a previous tool-created plan file.
    /// 是否替换此前由工具创建的计划文件。
    replace: Option<bool>,
    /// Exact previous plan path required when replacing.
    /// 替换时必需的此前计划完整路径。
    path: Option<String>,
}

/// Builds the Plan-mode plan artifact submission tool.
/// 构建计划模式计划产物提交工具。
pub(crate) fn build_submit_plan_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(SUBMIT_PLAN_TOOL_ID),
        name: String::from(SUBMIT_PLAN_TOOL_ID),
        description: String::from(
            "Submit a complete Plan-mode Markdown plan artifact. Use this only in Plan mode after the implementation plan is ready. For revisions, set replace=true and pass the exact path returned by the previous submit_plan call.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition_with_request_streaming(
        descriptor,
        submit_plan_args_schema(),
        Some(submit_plan_request_streaming_capability()),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_submit_plan_tool(request).await })
        },
    )
}

/// Maximum decoded Markdown bytes retained in the user-visible streaming preview.
/// 用户可见流式预览保留的最大已解码 Markdown 字节数。
const SUBMIT_PLAN_BODY_PREVIEW_BYTES: usize = 4096;

/// Minimum decoded content growth required before publishing another parser preview.
/// 再次发布解析器预览前所需的最小已解码正文增长量。
const SUBMIT_PLAN_PREVIEW_GROWTH_BYTES: usize = 1024;

/// Host-owned factory for isolated submit-plan request parser sessions.
/// 用于隔离 submit-plan 请求解析器会话的宿主自有工厂。
#[derive(Debug, Default)]
struct SubmitPlanRequestParserFactory;

impl ToolRequestParserFactory for SubmitPlanRequestParserFactory {
    /// Creates one independent parser for an exact submit-plan request.
    /// 为一条精确 submit-plan 请求创建独立解析器。
    ///
    /// # Parameters
    /// 参数
    /// `_context` carries stable correlation already owned by the coordinator.
    /// `_context` 携带已由协调器持有的稳定关联信息。
    ///
    /// # Returns
    /// 返回值
    /// A new bounded, side-effect-free parser session.
    /// 返回新的有界无副作用解析器会话。
    fn create(&self, _context: ToolRequestParserContext) -> Box<dyn ToolRequestParserSession> {
        Box::<SubmitPlanRequestParser>::default()
    }
}

/// Bounded semantic state extracted from one incomplete submit-plan JSON request.
/// 从一条不完整 submit-plan JSON 请求提取的有界语义状态。
#[derive(Debug, Default)]
struct SubmitPlanRequestParser {
    /// Bounded plan-name preview plus exact consistency evidence.
    /// 有界计划名称预览及精确一致性证据。
    name: SubmitPlanStreamedString,
    /// Bounded target-directory preview plus exact consistency evidence.
    /// 有界目标目录预览及精确一致性证据。
    directory: SubmitPlanStreamedString,
    /// Bounded replacement-path preview plus exact consistency evidence.
    /// 有界替换路径预览及精确一致性证据。
    path: SubmitPlanStreamedString,
    /// Optional replacement flag observed from a complete scalar.
    /// 从完整标量观测到的可选替换标志。
    replace: Option<bool>,
    /// Exact decoded content byte count without retaining the full body.
    /// 不保留完整正文时的精确已解码正文字节数。
    content_bytes: usize,
    /// Exact decoded newline count used for progress display.
    /// 用于进度显示的精确已解码换行符数量。
    content_newlines: usize,
    /// Whether the decoded content is currently non-empty.
    /// 已解码正文当前是否非空。
    content_nonempty: bool,
    /// Incremental consistency hash over decoded content bytes.
    /// 已解码正文字节的增量一致性哈希。
    content_hasher: DefaultHasher,
    /// Bounded prefix explicitly approved for user-visible preview.
    /// 明确获准用于用户可见预览的有界前缀。
    body_preview: String,
    /// Latest published parser revision.
    /// 最近发布的解析器修订号。
    revision: Cell<u64>,
    /// Content byte count at the latest published preview.
    /// 最近发布预览时的正文字节数。
    last_snapshot_content_bytes: Cell<usize>,
    /// Whether declared title or scalar fields changed since the latest preview.
    /// 声明式标题或标量字段自最近预览后是否发生变化。
    metadata_dirty: Cell<bool>,
}

/// Bounded streamed string state that retains exact length and hash evidence for final validation.
/// 为最终校验保留精确长度与哈希证据的有界流式字符串状态。
#[derive(Debug, Default)]
struct SubmitPlanStreamedString {
    /// Bounded prefix approved for preview projection.
    /// 获准用于预览投影的有界前缀。
    preview: String,
    /// Exact UTF-8 bytes observed across all decoded fragments.
    /// 在全部已解码碎片中观测到的精确 UTF-8 字节数。
    bytes: usize,
    /// Incremental hash over the complete decoded value.
    /// 完整已解码值的增量哈希。
    hasher: DefaultHasher,
}

impl SubmitPlanStreamedString {
    /// Clears all preview and consistency state before a new string value.
    /// 在新的字符串值开始前清空全部预览与一致性状态。
    fn reset(&mut self) {
        self.preview.clear();
        self.bytes = 0;
        self.hasher = DefaultHasher::new();
    }

    /// Appends one decoded fragment while retaining only the declared preview prefix.
    /// 追加一段已解码碎片，同时仅保留声明的预览前缀。
    ///
    /// # Parameters
    /// 参数
    /// `fragment` is the exact next decoded string content.
    /// `fragment` 是下一段精确已解码字符串内容。
    /// `preview_limit` is the maximum retained UTF-8 preview bytes.
    /// `preview_limit` 是保留 UTF-8 预览的最大字节数。
    fn append(&mut self, fragment: &str, preview_limit: usize) {
        self.bytes = self.bytes.saturating_add(fragment.len());
        self.hasher.write(fragment.as_bytes());
        append_bounded_utf8(&mut self.preview, fragment, preview_limit);
    }

    /// Returns whether authoritative final text matches the complete streamed value exactly.
    /// 返回权威最终文本是否与完整流式值精确匹配。
    ///
    /// # Parameters
    /// 参数
    /// `final_value` is the authoritative complete field value.
    /// `final_value` 是权威完整字段值。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when byte length and complete incremental hash both match.
    /// 仅当字节长度与完整增量哈希均匹配时返回 `true`。
    fn matches(&self, final_value: &str) -> bool {
        let mut final_hasher = DefaultHasher::new();
        final_hasher.write(final_value.as_bytes());
        self.bytes == final_value.len() && self.hasher.finish() == final_hasher.finish()
    }

    /// Returns whether no decoded bytes have been observed.
    /// 返回是否尚未观测到任何已解码字节。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the complete streamed value is empty.
    /// 完整流式值为空时返回 `true`。
    fn is_empty(&self) -> bool {
        self.bytes == 0
    }
}

impl ToolRequestParserSession for SubmitPlanRequestParser {
    /// Consumes one decoded semantic JSON event without writing any filesystem resource.
    /// 消费一条已解码语义 JSON 事件，且不写入任何文件系统资源。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the next canonical event from the strict incremental lexer.
    /// `event` 是严格增量词法器发出的下一条规范事件。
    ///
    /// # Returns
    /// 返回值
    /// Success because unsupported fields are ignored rather than guessed.
    /// 返回成功；不支持的字段会被忽略而不是猜测。
    fn push_json_event(&mut self, event: &IncrementalJsonEvent) -> Result<(), String> {
        match event {
            IncrementalJsonEvent::StringStarted { path } => {
                self.reset_string_field(path);
            }
            IncrementalJsonEvent::StringContent { path, content } => {
                self.append_string_field(path, content);
            }
            IncrementalJsonEvent::StringEnded { path } => {
                if submit_plan_root_field(path).is_some() {
                    self.metadata_dirty.set(true);
                }
            }
            IncrementalJsonEvent::ScalarCompleted { path, value } => {
                if submit_plan_root_field(path) == Some("replace")
                    && let IncrementalJsonScalar::Boolean(replace) = value
                {
                    self.replace = Some(*replace);
                    self.metadata_dirty.set(true);
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

    /// Returns a newer sanitized preview only after declared metadata or bounded content progress changes.
    /// 仅在声明式元数据或有界正文进度变化后返回较新的净化预览。
    ///
    /// # Returns
    /// 返回值
    /// A bounded preview snapshot or `None` when the visible cadence threshold was not reached.
    /// 返回有界预览快照；未达到可见节奏阈值时返回 `None`。
    fn snapshot(&self) -> Option<ToolRequestPreview> {
        let content_growth = self
            .content_bytes
            .saturating_sub(self.last_snapshot_content_bytes.get());
        if !self.metadata_dirty.get() && content_growth < SUBMIT_PLAN_PREVIEW_GROWTH_BYTES {
            return None;
        }
        let revision = self.revision.get().saturating_add(1);
        self.revision.set(revision);
        self.last_snapshot_content_bytes.set(self.content_bytes);
        self.metadata_dirty.set(false);
        Some(self.build_preview(revision))
    }

    /// Validates bounded streamed state against the authoritative complete submit-plan arguments.
    /// 根据权威完整 submit-plan 参数校验有界流式状态。
    ///
    /// # Parameters
    /// 参数
    /// `arguments` is the complete JSON object accepted by the existing execution path.
    /// `arguments` 是现有执行链接受的完整 JSON 对象。
    ///
    /// # Returns
    /// 返回值
    /// The final validated preview or an English consistency diagnostic.
    /// 返回最终已校验预览或英文一致性诊断。
    fn finish(&mut self, arguments: &Value) -> Result<Option<ToolRequestPreview>, String> {
        let object = arguments
            .as_object()
            .ok_or_else(|| String::from("submit_plan arguments must be an object"))?;
        let name = required_submit_plan_string(object, "name")?;
        let content = required_submit_plan_string(object, "content")?;
        let directory = optional_submit_plan_string(object, "directory")?;
        let path = optional_submit_plan_string(object, "path")?;
        let replace = optional_submit_plan_boolean(object, "replace")?;
        let mut final_hasher = DefaultHasher::new();
        final_hasher.write(content.as_bytes());
        if !self.name.matches(name)
            || !self.directory.matches(directory.unwrap_or_default())
            || !self.path.matches(path.unwrap_or_default())
            || self.replace != replace
            || self.content_bytes != content.len()
            || self.content_hasher.finish() != final_hasher.finish()
        {
            return Err(String::from(
                "submit_plan streamed preview did not match final arguments",
            ));
        }
        let revision = self.revision.get().saturating_add(1);
        self.revision.set(revision);
        self.metadata_dirty.set(false);
        self.last_snapshot_content_bytes.set(self.content_bytes);
        Ok(Some(self.build_preview(revision)))
    }

    /// Releases parser-owned in-memory preview state after cancellation or retry.
    /// 在取消或重试后释放解析器持有的内存预览状态。
    fn abort(&mut self) {
        self.body_preview.clear();
        self.content_bytes = 0;
        self.content_newlines = 0;
        self.content_nonempty = false;
        self.content_hasher = DefaultHasher::new();
        self.metadata_dirty.set(false);
    }
}

impl SubmitPlanRequestParser {
    /// Resets one declared root string field when its next JSON value begins.
    /// 在声明式根字符串字段的下一条 JSON 值开始时重置该字段。
    ///
    /// # Parameters
    /// 参数
    /// `path` is the canonical path of the string value.
    /// `path` 是字符串值的规范路径。
    ///
    /// # Returns
    /// 返回值
    /// This function mutates bounded parser state and returns no value.
    /// 此函数修改有界解析器状态，不返回值。
    fn reset_string_field(&mut self, path: &[IncrementalJsonPathSegment]) {
        match submit_plan_root_field(path) {
            Some("name") => self.name.reset(),
            Some("directory") => self.directory.reset(),
            Some("path") => self.path.reset(),
            Some("content") => {
                self.content_bytes = 0;
                self.content_newlines = 0;
                self.content_nonempty = false;
                self.content_hasher = DefaultHasher::new();
                self.body_preview.clear();
                self.last_snapshot_content_bytes.set(0);
            }
            Some("replace") | None | Some(_) => {}
        }
    }

    /// Appends decoded content to one declared root field using bounded retention for user-visible text.
    /// 将已解码内容追加到声明式根字段，并对用户可见文本采用有界保留。
    ///
    /// # Parameters
    /// 参数
    /// `path` is the canonical path of the string fragment.
    /// `path` 是字符串碎片的规范路径。
    /// `content` is the decoded non-empty fragment emitted by the lexer.
    /// `content` 是词法器发出的已解码非空碎片。
    ///
    /// # Returns
    /// 返回值
    /// This function updates parser state and returns no value.
    /// 此函数更新解析器状态，不返回值。
    fn append_string_field(&mut self, path: &[IncrementalJsonPathSegment], content: &str) {
        match submit_plan_root_field(path) {
            Some("name") => self.name.append(content, 256),
            Some("directory") => self.directory.append(content, 1024),
            Some("path") => self.path.append(content, 2048),
            Some("content") => {
                self.content_bytes = self.content_bytes.saturating_add(content.len());
                self.content_newlines = self
                    .content_newlines
                    .saturating_add(content.bytes().filter(|byte| *byte == b'\n').count());
                self.content_nonempty = true;
                self.content_hasher.write(content.as_bytes());
                append_bounded_utf8(
                    &mut self.body_preview,
                    content,
                    SUBMIT_PLAN_BODY_PREVIEW_BYTES,
                );
            }
            Some("replace") | None | Some(_) => {}
        }
    }

    /// Builds one sanitized preview containing only fields declared by the submit-plan capability.
    /// 构建仅包含 submit-plan 能力所声明字段的净化预览。
    ///
    /// # Parameters
    /// 参数
    /// `revision` is the next monotonic parser snapshot revision.
    /// `revision` 是下一条单调解析器快照修订号。
    ///
    /// # Returns
    /// 返回值
    /// A bounded preview with no final-file side effects or temporary staging claims.
    /// 返回不产生最终文件副作用且不声明临时暂存的有界预览。
    fn build_preview(&self, revision: u64) -> ToolRequestPreview {
        let mut title_fields = BTreeMap::new();
        if !self.name.is_empty() {
            title_fields.insert(
                String::from("name"),
                Value::String(self.name.preview.clone()),
            );
        }
        if !self.directory.is_empty() {
            title_fields.insert(
                String::from("directory"),
                Value::String(self.directory.preview.clone()),
            );
        }
        if !self.path.is_empty() {
            title_fields.insert(
                String::from("path"),
                Value::String(self.path.preview.clone()),
            );
        }
        if let Some(replace) = self.replace {
            title_fields.insert(String::from("replace"), Value::Bool(replace));
        }
        let mut progress_fields = BTreeMap::new();
        progress_fields.insert(
            String::from("content_bytes"),
            Value::from(self.content_bytes as u64),
        );
        progress_fields.insert(
            String::from("content_lines"),
            Value::from(if self.content_nonempty {
                self.content_newlines.saturating_add(1) as u64
            } else {
                0
            }),
        );
        let affected_resources = if !self.path.is_empty() {
            vec![self.path.preview.clone()]
        } else if !self.directory.is_empty() {
            vec![self.directory.preview.clone()]
        } else {
            Vec::new()
        };
        ToolRequestPreview {
            kind: String::from("submit_plan"),
            revision,
            title_fields,
            progress_fields,
            body_preview: (!self.body_preview.is_empty()).then(|| self.body_preview.clone()),
            affected_resources,
            redaction_flags: Vec::new(),
            staging_status: ToolRequestStagingStatus::NotApplicable,
        }
    }
}

/// Builds the execution-only parser and preview capability for submit-plan requests.
/// 构建 submit-plan 请求使用的仅执行期解析器与预览能力。
///
/// # Returns
/// 返回值
/// A frozen host-owned capability that never stages or writes incomplete content.
/// 返回绝不暂存或写入不完整内容的冻结宿主自有能力。
fn submit_plan_request_streaming_capability() -> ToolRequestStreamingCapability {
    ToolRequestStreamingCapability {
        parser_factory: Some(Arc::new(SubmitPlanRequestParserFactory)),
        preview_kind: Some(String::from("submit_plan")),
        staging_policy: ToolRequestStagingPolicy::Forbidden,
        sensitivity_policy: ToolRequestSensitivityPolicy {
            allowed_preview_fields: vec![
                String::from("name"),
                String::from("directory"),
                String::from("path"),
                String::from("replace"),
                String::from("content_bytes"),
                String::from("content_lines"),
                String::from("body_preview"),
                String::from("affected_resources"),
            ],
            redacted_json_paths: Vec::new(),
            maximum_body_preview_bytes: SUBMIT_PLAN_BODY_PREVIEW_BYTES,
        },
        progress_policy: ToolRequestProgressPolicy::default(),
    }
}

/// Returns one exact root field name from a canonical JSON path.
/// 从规范 JSON 路径返回一个精确根字段名称。
///
/// # Parameters
/// 参数
/// `path` is the canonical path emitted by the incremental lexer.
/// `path` 是增量词法器发出的规范路径。
///
/// # Returns
/// 返回值
/// The exact root key only when the path contains one key segment.
/// 仅当路径包含一个键片段时返回精确根键。
fn submit_plan_root_field(path: &[IncrementalJsonPathSegment]) -> Option<&str> {
    match path {
        [IncrementalJsonPathSegment::Key(field)] => Some(field.as_str()),
        _ => None,
    }
}

/// Appends a UTF-8 fragment without splitting a scalar and without exceeding the declared byte limit.
/// 在不拆分字符且不超过声明字节限制的前提下追加 UTF-8 碎片。
///
/// # Parameters
/// 参数
/// `target` is the bounded retained string.
/// `target` 是有界保留字符串。
/// `fragment` is the decoded text to append.
/// `fragment` 是需要追加的已解码文本。
/// `maximum_bytes` is the inclusive retained UTF-8 byte ceiling.
/// `maximum_bytes` 是保留 UTF-8 字节数的上限。
///
/// # Returns
/// 返回值
/// This function updates `target` and returns no value.
/// 此函数更新 `target`，不返回值。
fn append_bounded_utf8(target: &mut String, fragment: &str, maximum_bytes: usize) {
    let remaining = maximum_bytes.saturating_sub(target.len());
    if remaining == 0 {
        return;
    }
    let end = fragment
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| *end <= remaining)
        .last()
        .unwrap_or_default();
    target.push_str(&fragment[..end]);
}

/// Reads one required submit-plan string from the authoritative final object.
/// 从权威最终对象读取一个必需 submit-plan 字符串。
fn required_submit_plan_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("submit_plan field `{field}` must be a string"))
}

/// Reads one optional submit-plan string from the authoritative final object.
/// 从权威最终对象读取一个可选 submit-plan 字符串。
fn optional_submit_plan_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(format!("submit_plan field `{field}` must be a string")),
    }
}

/// Reads one optional submit-plan boolean from the authoritative final object.
/// 从权威最终对象读取一个可选 submit-plan 布尔值。
fn optional_submit_plan_boolean(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<bool>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("submit_plan field `{field}` must be a boolean")),
    }
}

/// Builds the JSON Schema for `submit_plan`.
/// 构建 `submit_plan` 的 JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for plan artifact submission.
/// 面向模型的计划产物提交参数结构。
fn submit_plan_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Short human-readable plan name used for the generated filename.",
                "minLength": 1
            },
            "content": {
                "type": "string",
                "description": "Complete self-contained Markdown plan content.",
                "minLength": 1
            },
            "directory": {
                "type": "string",
                "description": "Optional target directory under the current workspace root. Defaults to docs/plan."
            },
            "replace": {
                "type": "boolean",
                "description": "Set true only when replacing an existing tool-created plan artifact."
            },
            "path": {
                "type": "string",
                "description": "Exact previous plan file path returned by submit_plan. Required when replace=true."
            }
        },
        "required": ["name", "content"],
        "additionalProperties": false
    })
}

/// Executes one `submit_plan` request and writes the Markdown artifact.
/// 执行单次 `submit_plan` 请求并写入 Markdown 产物。
///
/// # Parameters
/// 参数
/// `request` carries raw model arguments and the workspace root.
/// `request` 携带模型原始参数与工作区根目录。
///
/// # Returns
/// 返回值
/// A model response and host projection pointing at the generated plan artifact.
/// 返回模型响应以及指向已生成计划产物的宿主投影。
async fn execute_submit_plan_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: SubmitPlanArgs =
        serde_json::from_value(request.arguments.clone()).map_err(|error| {
            NativeToolError::new(format!("failed to parse function arguments: {error}"))
        })?;
    let name = args.name.trim();
    let content = args.content.trim();
    if name.is_empty() {
        return Err(NativeToolError::new("Plan name cannot be empty."));
    }
    if content.is_empty() {
        return Err(NativeToolError::new("Plan content cannot be empty."));
    }
    let workspace_root = request
        .workspace_root
        .as_deref()
        .ok_or_else(|| NativeToolError::new("submit_plan requires a workspace root."))?;
    let workspace_root = canonicalize_existing_path(workspace_root).await?;
    let replace = args.replace.unwrap_or(false);
    let target_path = if replace {
        let path = args
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| NativeToolError::new("replace=true requires the original plan path."))?;
        let path = PathBuf::from(path);
        ensure_workspace_child_path(&workspace_root, &path).await?;
        let existing_bytes = tokio_fs::read(&path).await.map_err(|error| {
            NativeToolError::new(format!("failed to read original plan: {error}"))
        })?;
        let existing =
            vulcan_runtime_assets::text_decode::decode_complete_user_text(&existing_bytes)
                .map_err(|error| {
                    NativeToolError::new(format!(
                        "failed to decode original plan as UTF-8 or GB18030: {error}"
                    ))
                })?;
        if !existing.content.contains(SUBMIT_PLAN_MARKER) {
            return Err(NativeToolError::new(
                "Refusing to replace a file that was not created by submit_plan.",
            ));
        }
        path
    } else {
        if args
            .path
            .as_deref()
            .is_some_and(|path| !path.trim().is_empty())
        {
            return Err(NativeToolError::new(
                "Pass path only with replace=true; new plans choose a fresh filename automatically.",
            ));
        }
        let directory = args
            .directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("docs/plan");
        let directory_path = resolve_new_plan_directory(&workspace_root, directory).await?;
        tokio_fs::create_dir_all(&directory_path)
            .await
            .map_err(|error| {
                NativeToolError::new(format!("failed to create plan directory: {error}"))
            })?;
        unique_plan_path(&directory_path, name).await?
    };
    let plan_text = format!("{SUBMIT_PLAN_MARKER}\n\n{content}\n");
    tokio_fs::write(&target_path, plan_text)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to write plan file: {error}")))?;
    let path_text = display_submit_plan_path(&target_path);
    let updated_at = unix_millis_now().to_string();
    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "path": path_text,
        "name": name,
        "message": "Plan artifact saved."
    }))
    .with_host_result(VulcanToolHostResult {
        kind: String::from("plan_artifact"),
        payload: json!({
            "name": name,
            "path": path_text,
            "content": content,
            "updatedAt": updated_at,
        }),
    }))
}

#[cfg(test)]
#[allow(
    clippy::items_after_test_module,
    reason = "request-streaming parser tests stay adjacent to the parser implementation while existing filesystem helpers remain below"
)]
mod request_streaming_tests {
    use super::*;

    /// Builds one canonical root string path for focused parser tests.
    /// 为聚焦的解析器测试构建一条规范根字符串路径。
    ///
    /// # Parameters
    /// 参数
    /// `field` is the exact submit-plan field name.
    /// `field` 是精确 submit-plan 字段名称。
    ///
    /// # Returns
    /// 返回值
    /// A one-segment canonical JSON path.
    /// 返回单片段规范 JSON 路径。
    fn field_path(field: &str) -> Vec<IncrementalJsonPathSegment> {
        vec![IncrementalJsonPathSegment::Key(field.to_string())]
    }

    /// Pushes one complete decoded string through the same semantic boundaries emitted by the lexer.
    /// 通过词法器发出的相同语义边界推送一个完整已解码字符串。
    ///
    /// # Parameters
    /// 参数
    /// `parser` is the isolated submit-plan parser under test.
    /// `parser` 是待测隔离 submit-plan 解析器。
    /// `field` is the exact root string field.
    /// `field` 是精确根字符串字段。
    /// `fragments` contains ordered decoded content fragments.
    /// `fragments` 包含有序已解码内容碎片。
    ///
    /// # Returns
    /// 返回值
    /// This helper asserts successful parser consumption and returns no value.
    /// 此辅助函数断言解析器消费成功，不返回值。
    fn push_string(parser: &mut SubmitPlanRequestParser, field: &str, fragments: &[&str]) {
        let path = field_path(field);
        parser
            .push_json_event(&IncrementalJsonEvent::StringStarted { path: path.clone() })
            .expect("string start should parse");
        for fragment in fragments {
            parser
                .push_json_event(&IncrementalJsonEvent::StringContent {
                    path: path.clone(),
                    content: (*fragment).to_string(),
                })
                .expect("string content should parse");
        }
        parser
            .push_json_event(&IncrementalJsonEvent::StringEnded { path })
            .expect("string end should parse");
    }

    /// Verifies submit-plan publishes bounded decoded progress and validates split Unicode content exactly.
    /// 验证 submit-plan 发布有界已解码进度，并精确校验拆分的 Unicode 正文。
    #[test]
    fn submit_plan_parser_previews_and_validates_split_content() {
        let mut parser = SubmitPlanRequestParser::default();
        push_string(&mut parser, "name", &["流式计划"]);
        push_string(&mut parser, "content", &["# 计划\n", "内容😀"]);

        let preview = parser.snapshot().expect("preview should be available");
        assert_eq!(preview.kind, "submit_plan");
        assert_eq!(preview.title_fields["name"], json!("流式计划"));
        assert_eq!(preview.progress_fields["content_bytes"], json!(19));
        assert_eq!(preview.progress_fields["content_lines"], json!(2));
        assert_eq!(preview.body_preview.as_deref(), Some("# 计划\n内容😀"));

        let final_preview = parser
            .finish(&json!({
                "name": "流式计划",
                "content": "# 计划\n内容😀"
            }))
            .expect("split content should match final arguments")
            .expect("final preview should exist");
        assert!(final_preview.revision > preview.revision);
        assert_eq!(
            final_preview.staging_status,
            ToolRequestStagingStatus::NotApplicable
        );
    }

    /// Verifies final content divergence cannot be hidden by matching byte counts alone.
    /// 验证最终正文差异不能仅凭相同字节数被掩盖。
    #[test]
    fn submit_plan_parser_rejects_equal_length_content_mismatch() {
        let mut parser = SubmitPlanRequestParser::default();
        push_string(&mut parser, "name", &["plan"]);
        push_string(&mut parser, "content", &["abcd"]);

        let error = parser
            .finish(&json!({"name": "plan", "content": "wxyz"}))
            .expect_err("different equal-length content must fail consistency validation");
        assert!(error.contains("did not match"));
    }

    /// Verifies oversized metadata fields retain bounded previews while exact final validation still succeeds.
    /// 验证超长元数据字段只保留有界预览，同时仍能通过精确最终校验。
    #[test]
    fn submit_plan_parser_bounds_metadata_without_losing_consistency() {
        // longName exceeds the declared preview ceiling but remains exact through length and hash evidence.
        // longName 超过声明的预览上限，但仍通过长度与哈希证据保持精确。
        let long_name = "名".repeat(2_000);
        let mut parser = SubmitPlanRequestParser::default();
        push_string(&mut parser, "name", &[&long_name]);
        push_string(&mut parser, "content", &["body"]);
        let preview = parser.snapshot().expect("bounded preview should exist");
        let preview_name = preview.title_fields["name"]
            .as_str()
            .expect("name preview should be text");
        assert!(preview_name.len() <= 256);
        assert!(parser.name.preview.len() <= 256);
        parser
            .finish(&json!({ "name": long_name, "content": "body" }))
            .expect("length and hash evidence should validate complete metadata");
    }

    /// Verifies the real native submit-plan definition freezes its host-owned parser capability.
    /// 验证真实 Native submit-plan 定义冻结其宿主自有解析器能力。
    #[test]
    fn submit_plan_definition_registers_request_streaming_parser() {
        let definition = build_submit_plan_tool();
        let capability = definition
            .request_streaming
            .as_ref()
            .expect("submit_plan should register request streaming");
        assert!(capability.parser_factory.is_some());
        assert_eq!(capability.preview_kind.as_deref(), Some("submit_plan"));
        assert_eq!(
            capability.staging_policy,
            ToolRequestStagingPolicy::Forbidden
        );
    }
}

/// Formats a submitted Plan path for model-visible and UI-visible tool feedback.
/// 将已提交计划路径格式化为模型可见与界面可见的工具反馈文本。
///
/// # Parameters
/// 参数
/// `path` is the actual filesystem path used for writing the artifact.
/// `path` 是实际用于写入计划产物的文件系统路径。
///
/// # Returns
/// 返回值
/// A display path with Windows extended-path prefixes removed.
/// 返回移除 Windows 扩展路径前缀后的展示路径。
pub(crate) fn display_submit_plan_path(path: &Path) -> String {
    normalized_workspace_path_text_from_path(path)
}

/// Returns the current Unix epoch milliseconds.
/// 返回当前 Unix 纪元毫秒数。
///
/// # Returns
/// 返回值
/// The current timestamp in milliseconds, or zero if the system clock is before epoch.
/// 当前毫秒时间戳；如果系统时间早于纪元则返回零。
fn unix_millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

/// Canonicalizes an existing path and converts failures into native-tool errors.
/// 规范化已存在路径，并将失败转换为原生工具错误。
///
/// # Parameters
/// 参数
/// `path` is the filesystem path to canonicalize.
/// `path` 是需要规范化的文件系统路径。
///
/// # Returns
/// 返回值
/// The canonical absolute path.
/// 规范化后的绝对路径。
async fn canonicalize_existing_path(path: &Path) -> Result<PathBuf, NativeToolError> {
    tokio_fs::canonicalize(path)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to resolve path: {error}")))
}

/// Ensures a target path remains inside the workspace root.
/// 确保目标路径仍位于工作区根目录内。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化后的工作区根目录。
/// `path` is the target path to validate.
/// `path` 是需要校验的目标路径。
///
/// # Returns
/// 返回值
/// `Ok(())` when the path is accepted.
/// 路径通过校验时返回 `Ok(())`。
async fn ensure_workspace_child_path(
    workspace_root: &Path,
    path: &Path,
) -> Result<(), NativeToolError> {
    let candidate = if path.exists() {
        canonicalize_existing_path(path).await?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| NativeToolError::new("target path has no parent directory."))?;
        let canonical_parent = canonicalize_existing_path(parent).await?;
        let file_name = path
            .file_name()
            .ok_or_else(|| NativeToolError::new("target path has no filename."))?;
        canonical_parent.join(file_name)
    };
    if !candidate.starts_with(workspace_root) {
        return Err(NativeToolError::new(
            "Plan path must stay inside the current workspace.",
        ));
    }
    Ok(())
}

/// Resolves a new plan directory while keeping it inside the workspace root.
/// 解析新的计划目录，并确保其位于工作区根目录内。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化后的工作区根目录。
/// `directory` is the model-supplied relative or absolute directory.
/// `directory` 是模型提供的相对或绝对目录。
///
/// # Returns
/// 返回值
/// A workspace-contained directory path that may not exist yet.
/// 一个位于工作区内且可能尚不存在的目录路径。
async fn resolve_new_plan_directory(
    workspace_root: &Path,
    directory: &str,
) -> Result<PathBuf, NativeToolError> {
    let requested = PathBuf::from(directory);
    if requested
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(NativeToolError::new(
            "Plan directory cannot contain parent-directory components.",
        ));
    }
    if requested.is_absolute() {
        let parent = requested
            .parent()
            .ok_or_else(|| NativeToolError::new("plan directory has no parent."))?;
        let canonical_parent = canonicalize_existing_path(parent).await?;
        let file_name = requested
            .file_name()
            .ok_or_else(|| NativeToolError::new("plan directory has no final component."))?;
        let resolved = canonical_parent.join(file_name);
        if !resolved.starts_with(workspace_root) {
            return Err(NativeToolError::new(
                "Plan directory must stay inside the current workspace.",
            ));
        }
        return Ok(resolved);
    }
    Ok(workspace_root.join(requested))
}

/// Builds a unique Markdown path for a new plan artifact.
/// 为新的计划产物构建唯一 Markdown 路径。
///
/// # Parameters
/// 参数
/// `directory` is the already-created target directory.
/// `directory` 是已经创建的目标目录。
/// `name` is the human-readable plan name.
/// `name` 是人类可读计划名称。
///
/// # Returns
/// 返回值
/// A non-existing Markdown path.
/// 一个尚不存在的 Markdown 路径。
async fn unique_plan_path(directory: &Path, name: &str) -> Result<PathBuf, NativeToolError> {
    let base = slugify_plan_name(name);
    for suffix in 0..1000 {
        let filename = if suffix == 0 {
            format!("{base}.md")
        } else {
            format!("{base}_{suffix}.md")
        };
        let path = directory.join(filename);
        if tokio_fs::metadata(&path).await.is_err() {
            return Ok(path);
        }
    }
    Err(NativeToolError::new(
        "failed to allocate a unique plan filename after 1000 attempts.",
    ))
}

/// Converts one human-readable plan name into a conservative filename stem.
/// 将单个人类可读计划名称转换为保守文件名主体。
///
/// # Parameters
/// 参数
/// `name` is the model-supplied plan name.
/// `name` 是模型提供的计划名称。
///
/// # Returns
/// 返回值
/// A lowercase ASCII filename stem.
/// 小写 ASCII 文件名主体。
fn slugify_plan_name(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        String::from("plan")
    } else {
        slug.chars().take(80).collect()
    }
}
