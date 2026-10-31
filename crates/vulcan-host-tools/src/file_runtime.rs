//! Shared Rust file-reading and file-editing runtime used by native AI tools.
//! 供原生 AI 工具使用的共享 Rust 文件读写运行时。

use crate::text_document::{
    DocumentLine, LineEnding, MAX_ERROR_CANDIDATE_RANGES, TextDocument, TextReplacementRange,
    parse_document_lines, rewrite_replacement_line_endings,
};
use crate::text_encoding::{
    DecodedText, TextBom, TextEncoding, TextEncodingSource, decode_text, encode_text,
    parse_encoding_label,
};
use regex::Regex;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::fs::{self, File, Metadata};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::time::UNIX_EPOCH;
use tokio::task;
use vulcan_change_set::persistent_change_set_from_host_result;

mod text_transfer;
pub use text_transfer::{
    LocalTextBatch, LocalTextSource, LocalTextSourceFailure, collect_local_text_sources,
};

/// Maximum number of read/list/search targets accepted in one request.
/// 单次请求允许处理的读取、列表或搜索目标最大数量。
pub const MAX_FILE_TARGETS: usize = 10;

/// Maximum number of edit operations accepted in one request.
/// 单次编辑请求允许接受的操作最大数量。
pub const MAX_EDIT_OPERATIONS: usize = 50;

/// Maximum number of line segments accepted by one read target.
/// 单个 read 目标允许请求的行段最大数量。
pub const MAX_READ_SEGMENTS: usize = 64;

/// Maximum logical lines returned by one read target.
/// 单个 read 目标最多返回的逻辑行数。
pub const MAX_READ_LINES: usize = 10_000;

/// Default page length when the caller omits end_line; explicit ranges retain the hard limit.
/// 调用方省略 end_line 时的默认页长；显式范围保留硬上限。
pub const DEFAULT_READ_LINES: usize = 400;

/// Default serialized response budget, including Markdown and content-free metadata.
/// 默认序列化响应预算，包含 Markdown 和无正文元数据。
pub const DEFAULT_READ_RESPONSE_BYTES: usize = 64 * 1024;

/// Maximum serialized JSON bytes accepted by one file_edit request.
/// 单次 file_edit 请求允许的序列化 JSON 最大字节数。
pub const MAX_EDIT_REQUEST_BYTES: usize = 8 * 1024 * 1024;

/// Maximum logical lines accepted in one old_lines, new_lines, or anchors array.
/// 单个 old_lines、new_lines 或 anchors 数组允许的逻辑行最大数量。
pub const MAX_EDIT_LINES_PER_ARRAY: usize = 16_384;

/// Maximum logical lines across all arrays in one file_edit request.
/// 单次 file_edit 请求所有数组合计允许的逻辑行最大数量。
pub const MAX_EDIT_TOTAL_LINES: usize = 65_536;

/// Maximum ordered anchors accepted by one replacement operation.
/// 单个 replace 操作允许接受的有序锚点最大数量。
pub const MAX_EDIT_ANCHORS: usize = 8;

/// Maximum UTF-8 file size processed by the first native text runtime.
/// 第一版原生文本运行时允许处理的 UTF-8 文件最大字节数。
const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

/// Maximum logical line size accepted by the structured edit schema.
/// 结构化编辑 Schema 允许的单条逻辑行最大字节数。
const MAX_LINE_BYTES: usize = 256 * 1024;

/// Maximum search results returned by one target.
/// 单个目标允许返回的搜索结果最大数量。
pub const MAX_SEARCH_RESULTS: usize = 1000;

/// Maximum regular files scanned by one directory search.
/// 单次目录搜索允许扫描的普通文件最大数量。
pub const MAX_SEARCH_FILES: usize = 50_000;

/// Maximum list entries returned by one list target.
/// 单个 list 目标最多返回的条目数量。
pub const MAX_LIST_ENTRIES: usize = 1_000;

/// Maximum UTF-8 bytes accepted in one search query or pattern.
/// 单个搜索查询或文件名模式允许的最大 UTF-8 字节数。
pub const MAX_SEARCH_QUERY_BYTES: usize = 4_096;

/// Maximum serialized line payload returned by one read target.
/// 单个 read 目标返回的行正文序列化字节最大大小。
const MAX_READ_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Maximum serialized entry payload returned by one list target.
/// 单个 list 目标返回的文件条目序列化字节最大大小。
const MAX_LIST_RESPONSE_BYTES: usize = 1024 * 1024;

/// Maximum serialized match payload returned by one search target.
/// 单个 search 目标返回的搜索命中序列化字节最大大小。
const MAX_SEARCH_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Maximum inline lifecycle content retained in a change-set row.
/// 变更集文件行允许内联保留的生命周期正文最大字节数。
const MAX_INLINE_LIFECYCLE_BYTES: usize = 64 * 1024;

/// Maximum operation contexts retained in one structured edit error.
/// 单条结构化编辑错误最多保留的操作上下文数量。
const MAX_ERROR_OPERATION_CONTEXTS: usize = MAX_EDIT_OPERATIONS;

/// Process-wide weak registry for deterministic per-path mutation locks.
/// 用于确定性逐路径变更锁的进程级弱引用注册表。
static FILE_RESOURCE_LOCKS: OnceLock<Mutex<HashMap<String, Weak<Mutex<()>>>>> = OnceLock::new();

/// One structured error returned by the shared file runtime.
/// 共享文件运行时返回的一条结构化错误。
#[derive(Debug, Clone)]
pub struct FileToolError {
    /// Stable machine-readable error code.
    /// 稳定的机器可读错误码。
    pub code: String,
    /// English recovery-oriented error message.
    /// 面向恢复操作的英文错误消息。
    pub message: String,
    /// Optional bounded structured diagnostic details.
    /// 可选的有界结构化诊断详情。
    pub details: Value,
}

impl FileToolError {
    /// Creates one file-runtime error with a stable code and message.
    /// 使用稳定错误码和消息创建一条文件运行时错误。
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Value::Null,
        }
    }

    /// Attaches bounded structured details to this error.
    /// 为当前错误附加有界结构化详情。
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    /// Converts the error into a compact model-visible JSON object.
    /// 将错误转换为紧凑的模型可见 JSON 对象。
    pub fn as_value(&self) -> Value {
        let mut object = Map::new();
        object.insert(String::from("code"), Value::String(self.code.clone()));
        object.insert(String::from("message"), Value::String(self.message.clone()));
        let detail_object = self.details.as_object();
        let requires_read = detail_object
            .and_then(|details| {
                details
                    .get("requires_read")
                    .or_else(|| details.get("requires_reread"))
            })
            .and_then(Value::as_bool)
            .unwrap_or_else(|| edit_error_requires_reread(self.code.as_str()));
        let retryable = detail_object
            .and_then(|details| details.get("retryable"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let hint = detail_object
            .and_then(|details| details.get("hint").or_else(|| details.get("recovery_hint")))
            .and_then(Value::as_str)
            .unwrap_or_else(|| edit_error_recovery_hint(self.code.as_str()));
        object.insert(String::from("requires_read"), Value::Bool(requires_read));
        object.insert(String::from("retryable"), Value::Bool(retryable));
        object.insert(String::from("hint"), Value::String(hint.to_string()));
        if let Some(path) = detail_object
            .and_then(|details| details.get("path"))
            .filter(|path| !path.is_null())
        {
            object.insert(String::from("path"), path.clone());
        }
        // Emit each diagnostic once; common fields above own the public names, legacy aliases stay internal.
        // 每个诊断只输出一次；上方公共字段拥有公开名称，旧别名留在内部。
        if let Some(details) = detail_object {
            for (key, value) in details {
                if !value.is_null() && !matches!(key.as_str(), "requires_reread" | "recovery_hint")
                {
                    object.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
        }
        if self.code == "execution_failed" {
            // A failed blocking task may have exited during commit; never claim that execution did not start.
            // 阻塞任务失败可能发生于提交期间；绝不能声称执行尚未开始。
            object.insert(String::from("commit_scope"), json!("unknown"));
            object.insert(String::from("rollback_state"), json!("unknown"));
        }
        Value::Object(object)
    }
}

impl Display for FileToolError {
    /// Renders the stable code and human-readable message for native-tool failure paths.
    /// 为原生工具失败路径渲染稳定错误码和人类可读消息。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for FileToolError {}

/// Result returned by the shared runtime before the agent layer adds host-result metadata.
/// 共享运行时返回的结果，之后由 Agent 层补充宿主结果元数据。
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileToolResponse {
    /// Compact model-visible result body.
    /// 紧凑的模型可见结果正文。
    pub model_output: Value,
    /// Read-only presentation coordinates into model_output; source text is never duplicated here.
    /// 指向 model_output 的只读展示坐标；此处绝不重复保存源文件正文。
    pub read_metadata: Option<Value>,
    /// Optional canonical change-set payload for write operations.
    /// 写入操作可选的 canonical change-set 载荷。
    pub change_set: Option<Value>,
}

/// Executes one read-only file action on a blocking worker.
/// 在受控阻塞任务中执行一次只读文件动作。
pub async fn execute_file(
    workspace_root: PathBuf,
    arguments: Value,
) -> Result<FileToolResponse, FileToolError> {
    task::spawn_blocking(move || execute_file_sync(&workspace_root, &arguments))
        .await
        .map_err(|error| FileToolError::new("execution_failed", error.to_string()))?
}

/// Executes one structured multi-file edit on a blocking worker.
/// 在受控阻塞任务中执行一次结构化多文件编辑。
pub async fn execute_file_edit(
    workspace_root: PathBuf,
    arguments: Value,
) -> Result<FileToolResponse, FileToolError> {
    task::spawn_blocking(move || execute_file_edit_sync(&workspace_root, &arguments))
        .await
        .map_err(|error| FileToolError::new("execution_failed", error.to_string()))?
}

/// Executes one Apply Patch text request on a blocking worker.
/// 在受控阻塞任务中执行一次 Apply Patch 文本请求。
pub async fn execute_apply_patch(
    workspace_root: PathBuf,
    patch: String,
) -> Result<FileToolResponse, FileToolError> {
    task::spawn_blocking(move || execute_apply_patch_sync(&workspace_root, &patch))
        .await
        .map_err(|error| FileToolError::new("execution_failed", error.to_string()))?
}

/// Executes the read-only action synchronously after arguments have crossed the async boundary.
/// 参数跨过异步边界后，同步执行只读动作。
fn execute_file_sync(
    workspace_root: &Path,
    arguments: &Value,
) -> Result<FileToolResponse, FileToolError> {
    if arguments
        .as_object()
        .is_some_and(|object| object.contains_key("targets"))
    {
        return execute_legacy_file_sync(workspace_root, arguments);
    }
    execute_native_file_sync(workspace_root, arguments)
}

/// Executes the hidden historical multi-target contract for persisted calls.
/// 为持久化调用执行隐藏的历史多目标契约。
fn execute_legacy_file_sync(
    workspace_root: &Path,
    arguments: &Value,
) -> Result<FileToolResponse, FileToolError> {
    let object = arguments.as_object().ok_or_else(|| {
        FileToolError::new("invalid_arguments", "The file arguments must be an object.")
    })?;
    let action = required_string(object, "action")?;
    let targets = object
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            FileToolError::new(
                "invalid_arguments",
                "The file request requires a targets array.",
            )
        })?;
    if targets.is_empty() || targets.len() > MAX_FILE_TARGETS {
        return Err(FileToolError::new(
            "invalid_arguments",
            format!("The targets array must contain between 1 and {MAX_FILE_TARGETS} items."),
        ));
    }
    if !matches!(action.as_str(), "read" | "list" | "search" | "stat") {
        return Err(FileToolError::new(
            "invalid_arguments",
            "The action must be one of read, list, search, or stat.",
        ));
    }
    validate_file_request_fields(object, &action)?;
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        let result = execute_file_target(workspace_root, object, target, &action);
        match result {
            Ok(value) => results.push(value),
            Err(error) => results.push(json!({
                "ok": false,
                "error": error.as_value(),
            })),
        }
    }
    Ok(FileToolResponse {
        model_output: json!({
            "ok": results.iter().all(|value| value.get("ok").and_then(Value::as_bool).unwrap_or(false)),
            "action": action,
            "results": results,
        }),
        read_metadata: None,
        change_set: None,
    })
}

/// Executes one public single-path file action with strict action-specific fields.
/// 使用严格的动作专属字段执行一个公开单路径文件动作。
fn execute_native_file_sync(
    workspace_root: &Path,
    arguments: &Value,
) -> Result<FileToolResponse, FileToolError> {
    let object = arguments.as_object().ok_or_else(|| {
        FileToolError::new("invalid_arguments", "The file arguments must be an object.")
    })?;
    let action = required_string(object, "action")?;
    validate_native_file_fields(object, &action)?;
    let raw_path = required_string(object, "path")?;
    let path = resolve_path(workspace_root, &raw_path)?;
    let mut mutation_paths = Vec::new();
    if matches!(action.as_str(), "write" | "delete" | "move") {
        mutation_paths.push(path.clone());
        if action == "move" {
            mutation_paths.push(resolve_path(
                workspace_root,
                &required_string(object, "destination")?,
            )?);
        }
    }
    let resource_locks = acquire_resource_locks(&mutation_paths)?;
    let _resource_guards = lock_resource_set(&resource_locks)?;
    match action.as_str() {
        "read" => native_read_file(&path, &raw_path, object),
        "list" => native_list_files(&path, &raw_path, object),
        "stat" => {
            let mut model_output = stat_target(&path, &display_path(&path))?;
            if let Some(result) = model_output.as_object_mut() {
                result.insert(String::from("schema"), json!("vulcan.file_access.v2"));
                result.insert(String::from("action"), json!("stat"));
                result.insert(String::from("request_path"), json!(raw_path));
                result.insert(String::from("resolved_path"), json!(display_path(&path)));
                result.insert(String::from("resource_id"), json!("resource-1"));
                result.insert(String::from("language"), json!(language_for_path(&path)));
            }
            Ok(FileToolResponse {
                model_output,
                read_metadata: None,
                change_set: None,
            })
        }
        "write" => native_write_file(&path, &raw_path, object),
        "delete" => native_delete_file(&path, &raw_path),
        "move" => native_move_file(workspace_root, &path, &raw_path, object),
        _ => Err(FileToolError::new(
            "invalid_arguments",
            "The action must be one of read, list, stat, write, delete, or move.",
        )),
    }
}

/// Rejects every public file field that does not belong to the selected action.
/// 拒绝每个不属于所选动作的公开 file 字段。
fn validate_native_file_fields(
    object: &Map<String, Value>,
    action: &str,
) -> Result<(), FileToolError> {
    let allowed = match action {
        "read" => ["action", "path", "start_line", "end_line", "encoding"].as_slice(),
        "list" => ["action", "path", "recursive", "glob", "limit"].as_slice(),
        "stat" | "delete" => ["action", "path"].as_slice(),
        "write" => [
            "action",
            "path",
            "content",
            "encoding",
            "source_encoding",
            "bom",
            "line_ending",
        ]
        .as_slice(),
        "move" => ["action", "path", "destination"].as_slice(),
        _ => {
            return Err(FileToolError::new(
                "invalid_arguments",
                "The action must be one of read, list, stat, write, delete, or move.",
            ));
        }
    };
    validate_allowed_fields(object, allowed, "file request")
}

/// Reads one decoded range without embedding textual line-number prefixes.
/// 读取一段已解码范围且不嵌入文本行号前缀。
fn native_read_file(
    path: &Path,
    request_path: &str,
    object: &Map<String, Value>,
) -> Result<FileToolResponse, FileToolError> {
    let encoding = optional_string(object, "encoding")?;
    let snapshot = read_snapshot_with_encoding(path, encoding.as_deref())?;
    let start_line = object
        .get("start_line")
        .map(|_| required_positive_usize(object, "start_line"))
        .transpose()?
        .unwrap_or(1);
    let requested_end = object
        .get("end_line")
        .map(|_| required_positive_usize(object, "end_line"))
        .transpose()?;
    if requested_end.is_some_and(|end_line| end_line < start_line) {
        return Err(FileToolError::new(
            "invalid_arguments",
            "end_line must be greater than or equal to start_line.",
        ));
    }
    let maximum_end = start_line.saturating_add(MAX_READ_LINES.saturating_sub(1));
    if requested_end.is_some_and(|end_line| end_line > maximum_end) {
        return Err(FileToolError::new(
            "invalid_arguments",
            format!("A read call may return at most {MAX_READ_LINES} logical lines."),
        ));
    }
    let start_index = start_line
        .saturating_sub(1)
        .min(snapshot.document.lines.len());
    // Explicit ranges opt into the existing hard budget; unbounded requests use a small continuable page.
    // 显式范围选择现有硬预算；无结束行的请求使用可续读的小页。
    let response_budget = if requested_end.is_some() {
        MAX_READ_RESPONSE_BYTES
    } else {
        DEFAULT_READ_RESPONSE_BYTES
    };
    let desired_end_index = requested_end
        .unwrap_or_else(|| start_line.saturating_add(DEFAULT_READ_LINES - 1))
        .min(snapshot.document.lines.len());
    let full_output = native_read_model_output(
        path,
        request_path,
        &snapshot,
        NativeReadRange {
            requested_start_line: start_line,
            requested_end_line: requested_end,
            start_index,
            end_index: desired_end_index,
            size_truncated: false,
        },
    );
    let response = if serialized_value_bytes(&full_output) <= response_budget {
        full_output
    } else {
        let metadata_only = native_read_model_output(
            path,
            request_path,
            &snapshot,
            NativeReadRange {
                requested_start_line: start_line,
                requested_end_line: requested_end,
                start_index,
                end_index: start_index,
                size_truncated: true,
            },
        );
        if serialized_value_bytes(&metadata_only) > response_budget {
            return Err(FileToolError::new(
                "output_limit_exceeded",
                "The file read metadata exceeds the native response size limit.",
            ));
        }

        // Serialized payload size grows monotonically as complete lines are appended, so a binary
        // search finds the largest safe contiguous range without repeatedly rebuilding megabytes.
        // 完整行持续追加时序列化载荷大小单调增长，因此使用二分搜索即可找到最大安全连续范围，
        // 避免反复重建数 MB 数据。
        let mut fitting_end = start_index;
        let mut overflowing_end = desired_end_index;
        while fitting_end.saturating_add(1) < overflowing_end {
            let candidate_end = fitting_end + (overflowing_end - fitting_end) / 2;
            let candidate = native_read_model_output(
                path,
                request_path,
                &snapshot,
                NativeReadRange {
                    requested_start_line: start_line,
                    requested_end_line: requested_end,
                    start_index,
                    end_index: candidate_end,
                    size_truncated: true,
                },
            );
            if serialized_value_bytes(&candidate) <= response_budget {
                fitting_end = candidate_end;
            } else {
                overflowing_end = candidate_end;
            }
        }
        if fitting_end == start_index && desired_end_index > start_index {
            return Err(FileToolError::new(
                "output_limit_exceeded",
                format!(
                    "Logical line {start_line} cannot fit inside the {response_budget}-byte read budget. Specify end_line explicitly to request the hard budget; if it still fails, use a tool supporting partial-line reads."
                ),
            ));
        }
        native_read_model_output(
            path,
            request_path,
            &snapshot,
            NativeReadRange {
                requested_start_line: start_line,
                requested_end_line: requested_end,
                start_index,
                end_index: fitting_end,
                size_truncated: true,
            },
        )
    };
    Ok(response)
}

/// Groups one requested and returned native read range with its size-limit state.
/// 将原生读取的请求范围、返回范围与大小限制状态组合在一起。
#[derive(Debug, Clone, Copy)]
struct NativeReadRange {
    /// Exact requested one-based start; retained even when it exceeds EOF.
    /// 精确请求的一基起始行；超出文件末尾时仍保留。
    requested_start_line: usize,
    /// Optional one-based inclusive final line requested by the model.
    /// 模型请求的可选一基闭区间结束行。
    requested_end_line: Option<usize>,
    /// Zero-based inclusive first selected line index.
    /// 零基闭区间起始选中行索引。
    start_index: usize,
    /// Zero-based exclusive final selected line index.
    /// 零基开区间结束选中行索引。
    end_index: usize,
    /// Whether response-size limiting shortened the returned range.
    /// 响应大小限制是否缩短了返回范围。
    size_truncated: bool,
}

/// Renders the bounded snapshot range once as Markdown and returns content-free host coordinates.
/// 将快照中的有界范围渲染为一份 Markdown，并返回不含正文的宿主坐标。
/// path selects language and canonical identity; request_path labels the model-visible header.
/// path 确定语言与规范身份；request_path 提供模型可见标题。
fn native_read_model_output(
    path: &Path,
    request_path: &str,
    snapshot: &TextSnapshot,
    range: NativeReadRange,
) -> FileToolResponse {
    // Select complete logical lines; normalize only their presentation separators to LF.
    // 选择完整逻辑行，仅将展示分隔符归一化为 LF。
    let selected = &snapshot.document.lines[range.start_index..range.end_index];
    // Keep the source range independent from the Markdown header length.
    // 保持源文件范围独立于 Markdown 标题长度。
    let returned_start_line = range.requested_start_line;
    let returned_end_line = range.end_index;
    // Report continuation only when the requested range has unread source lines.
    // 仅当请求范围仍有未读源文件行时报告续读位置。
    let truncated = range.size_truncated
        || (returned_end_line < snapshot.document.lines.len()
            && range
                .requested_end_line
                .is_none_or(|end| returned_end_line < end));
    // A fence longer than every source backtick run prevents source text from closing it.
    // 围栏长度超过正文所有连续反引号，避免源文件内容提前关闭代码块。
    let fence_length = selected
        .iter()
        .flat_map(|line| line.content.split(|character| character != '`'))
        .map(str::len)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
        .max(3);
    let fence = "`".repeat(fence_length);
    // Escape header line breaks so paths cannot invalidate the numeric line mapping.
    // 转义标题中的换行，防止路径破坏数值行映射。
    let header_path = request_path.replace('\r', "\\r").replace('\n', "\\n");
    let language = language_for_path(path);
    // Empty reads must not display an inverted or fabricated source range.
    // 空读取不得展示逆序或虚构的源文件范围。
    let showing = if selected.is_empty() {
        String::from("empty")
    } else {
        format!("{returned_start_line}-{returned_end_line}")
    };
    // The Markdown string is the only owner of source text in the returned payload.
    // Markdown 字符串是返回载荷中源文件正文的唯一持有者。
    let mut markdown = format!(
        "# File: {header_path}\n\nLines: {} | Showing: {showing} | Encoding: {} ({})\n\n{fence}{language}\n",
        snapshot.document.lines.len(),
        snapshot.document.encoding.label(),
        snapshot.document.encoding_source.as_str(),
    );
    // Record the first body line before appending content instead of hard-coding a header offset.
    // 追加正文前记录首行位置，避免硬编码标题偏移。
    let output_start_line = markdown.lines().count() + 1;
    // Number every logical line so models can cite exact source positions without counting.
    // 为每个逻辑行标号，使模型无需计数即可引用精确源位置。
    for (index, line) in selected.iter().enumerate() {
        markdown.push_str(&format!(
            "L{}: {}\n",
            returned_start_line + index,
            line.content
        ));
    }
    markdown.push_str(&fence);
    markdown.push('\n');
    if !snapshot.document.encoding_certain {
        markdown.push_str("\n[encoding_unverified] Automatic detection has limited evidence; this decoding may be incorrect. Confirm the source encoding with file_read using encoding before editing; pass that encoding to file_edit.\n");
    }
    // Empty files and out-of-range requests are distinct successful reads, never missing paths.
    // 空文件与越界请求是两种不同的成功读取，不能混同路径不存在。
    if snapshot.document.lines.is_empty() {
        markdown.push_str("\n[empty_file] The file exists but contains no text. There are no source lines to read.\n");
    } else if selected.is_empty() {
        markdown.push_str(&format!("\n[range_beyond_eof] Requested start line {} exceeds the file's {} lines. No content returned.\n", range.requested_start_line, snapshot.document.lines.len()));
    }
    if truncated {
        markdown.push_str(&format!(
            "\nTruncated. Continue with start_line={}.\n",
            returned_end_line.saturating_add(1),
        ));
    }
    // Host metadata contains only coordinates and file-format facts, never source content.
    // 宿主元数据仅含定位信息和文件格式事实，绝不保存源文件正文。
    let metadata = json!({
        "schema": "vulcan.file_read.v3",
        "action": "read",
        "path": display_path(path),
        "resource_id": "resource-1",
        "language": language,
        "encoding": snapshot.document.encoding.label(),
        "encoding_source": snapshot.document.encoding_source.as_str(),
        "encoding_certain": snapshot.document.encoding_certain,
        "bom": snapshot.document.bom.is_present(),
        "line_ending": snapshot.document.line_ending_label(),
        "mixed_line_endings": snapshot.document.mixed_line_endings,
        "final_newline": snapshot.document.final_newline(),
        "total_line_count": snapshot.document.lines.len(),
        "read_status": if snapshot.document.lines.is_empty() { "empty_file" } else if range.start_index == snapshot.document.lines.len() { "range_beyond_eof" } else { "content" },
        "requested_range": { "start_line": range.requested_start_line, "end_line": range.requested_end_line },
        "range": {
            "source_start_line": returned_start_line,
            "line_count": selected.len(),
        },
        "render": { "output_start_line": output_start_line },
        "truncated": truncated,
        "next_start_line": truncated.then_some(returned_end_line.saturating_add(1)),
    });
    FileToolResponse {
        model_output: Value::String(markdown),
        read_metadata: Some(metadata),
        change_set: None,
    }
}

/// Lists one directory through the existing bounded traversal implementation.
/// 通过现有有界遍历实现列出一个目录。
fn native_list_files(
    path: &Path,
    request_path: &str,
    object: &Map<String, Value>,
) -> Result<FileToolResponse, FileToolError> {
    let recursive = optional_bool(object, "recursive", false)?;
    let limit = optional_limit(object, "limit", 200, 1, MAX_LIST_ENTRIES)?;
    let mut request = Map::new();
    request.insert(String::from("recursive"), Value::Bool(recursive));
    request.insert(String::from("include_ignored"), Value::Bool(false));
    request.insert(String::from("limit"), json!(limit));
    if let Some(glob) = optional_string(object, "glob")? {
        request.insert(String::from("pattern"), Value::String(glob));
    }
    let mut model_output = list_target(path, &display_path(path), &request)?;
    if let Some(result) = model_output.as_object_mut() {
        result.insert(String::from("schema"), json!("vulcan.file_access.v2"));
        result.insert(String::from("action"), json!("list"));
        result.insert(String::from("request_path"), json!(request_path));
        result.insert(String::from("resolved_path"), json!(display_path(path)));
        result.insert(String::from("resource_id"), json!("resource-1"));
    }
    Ok(FileToolResponse {
        model_output,
        read_metadata: None,
        change_set: None,
    })
}

/// Replaces or creates one complete text file through the shared transaction engine.
/// 通过共享事务引擎替换或创建一份完整文本文件。
fn native_write_file(
    path: &Path,
    request_path: &str,
    object: &Map<String, Value>,
) -> Result<FileToolResponse, FileToolError> {
    let content = required_text(object, "content")?;
    let requested_encoding = optional_string(object, "encoding")?;
    // Source and destination codecs have separate authority; an output override cannot reinterpret old bytes.
    // 源编码与目标编码各有独立权威；输出覆盖不能重新解释原始字节。
    let source_encoding = optional_string(object, "source_encoding")?;
    let bom_policy = optional_string(object, "bom")?.unwrap_or_else(|| String::from("preserve"));
    let line_ending_policy =
        optional_string(object, "line_ending")?.unwrap_or_else(|| String::from("preserve"));
    if !matches!(bom_policy.as_str(), "preserve" | "present" | "absent") {
        return Err(FileToolError::new(
            "invalid_arguments",
            "bom must be preserve, present, or absent.",
        ));
    }
    if line_ending_policy != "preserve" && LineEnding::parse(&line_ending_policy).is_none() {
        return Err(FileToolError::new(
            "invalid_arguments",
            "line_ending must be preserve, lf, crlf, cr, or lfcr.",
        ));
    }
    validate_parent_directory(path)?;
    let existing = if path_exists_including_symlink(path) {
        let snapshot = read_edit_snapshot(path, source_encoding.as_deref())?;
        Some(snapshot)
    } else {
        if source_encoding.is_some() {
            return Err(FileToolError::new(
                "invalid_arguments",
                "source_encoding requires an existing source file. Use encoding to select a new file's output encoding.",
            ));
        }
        None
    };
    let encoding = match (requested_encoding.as_deref(), existing.as_ref()) {
        (Some(label), _) => parse_encoding_label(label).map_err(codec_error)?,
        (None, Some(snapshot)) => snapshot.document.encoding,
        (None, None) => TextEncoding::Utf8,
    };
    let existing_bom = existing
        .as_ref()
        .map(|snapshot| snapshot.document.bom)
        .unwrap_or(TextBom::None);
    let bom = match bom_policy.as_str() {
        "preserve" if existing_bom.is_present() => default_bom_for_encoding(encoding)?,
        "preserve" => TextBom::None,
        "present" => default_bom_for_encoding(encoding)?,
        "absent" => TextBom::None,
        _ => TextBom::None,
    };
    let normalized_content = match LineEnding::parse(&line_ending_policy) {
        Some(ending) => crate::text_document::rewrite_line_endings(&content, ending),
        None if existing.is_some() => crate::text_document::rewrite_line_endings(
            &content,
            existing
                .as_ref()
                .map(|snapshot| snapshot.document.preferred_line_ending)
                .unwrap_or(LineEnding::Lf),
        ),
        None => crate::text_document::rewrite_line_endings(&content, LineEnding::Lf),
    };
    let after_bytes = encode_text(&normalized_content, encoding, bom).map_err(codec_error)?;
    if existing
        .as_ref()
        .is_some_and(|snapshot| snapshot.bytes == after_bytes)
    {
        return Err(FileToolError::new(
            "no_change",
            "The write content is byte-identical to the existing file.",
        ).with_details(json!({"hint": "Provide content that differs from the current file, or skip the write."})));
    }
    let change_kind = if existing.is_some() {
        "modify"
    } else {
        "create"
    };
    let before_lines = existing
        .as_ref()
        .map(|snapshot| snapshot.lines.clone())
        .unwrap_or_default();
    let after_document = TextDocument::from_decoded(DecodedText {
        text: normalized_content,
        encoding,
        bom,
        source: TextEncodingSource::Explicit,
        certain: true,
    });
    let after_lines = after_document.logical_lines();
    // file_payload selects the only lifecycle representation allowed by the canonical V1 parser.
    // file_payload 选择规范 V1 解析器允许的唯一生命周期表示。
    let file_payload = if let Some(snapshot) = &existing {
        whole_file_modify_payload(path, &snapshot.document.lines, &after_document.lines)
    } else {
        whole_file_create_payload(path, &after_document.text, &after_lines)
    };
    let prepared = PreparedChange {
        text_formats: Some(PreparedTextFormats {
            before: existing
                .as_ref()
                .map(|snapshot| document_text_format(&snapshot.document))
                .unwrap_or(Value::Null),
            after: document_text_format(&after_document),
        }),
        change_kind: String::from(change_kind),
        source_path: path.to_path_buf(),
        destination_path: None,
        original_version: existing
            .as_ref()
            .map(|snapshot| snapshot.version.clone())
            .unwrap_or_default(),
        before_bytes: existing.as_ref().map(|snapshot| snapshot.bytes.clone()),
        original_permissions: existing
            .as_ref()
            .map(|snapshot| snapshot.permissions.clone()),
        after_bytes: Some(after_bytes),
        file_payload,
        operation_results: vec![json!({
            "operation_id": "file-0",
            "operation_index": 0,
            "resource_id": "resource-1",
            "request_path": request_path,
            "status": "applied",
            "original_range": if before_lines.is_empty() { Value::Null } else { json!({"start_line": 1, "line_count": before_lines.len()}) },
            "final_range": {"start_line": 1, "line_count": after_lines.len()},
        })],
    };
    // Reuse the exact extension accepted before the filesystem commit.
    // 复用文件系统提交前已通过校验的精确扩展。
    let extension =
        commit_validated_changes("applied", std::slice::from_ref(&prepared), "file_write")?;
    Ok(native_file_change_response(
        "write",
        vec![prepared],
        extension,
    ))
}

/// Deletes one regular file transactionally with rollback bytes retained.
/// 以事务方式删除一份普通文件并保留回滚字节。
fn native_delete_file(path: &Path, request_path: &str) -> Result<FileToolResponse, FileToolError> {
    let snapshot = read_raw_snapshot(path)?;
    let file_payload = raw_lifecycle_payload(path, "delete", &snapshot, "resource-1");
    let prepared = PreparedChange {
        text_formats: None,
        change_kind: String::from("delete"),
        source_path: path.to_path_buf(),
        destination_path: None,
        original_version: snapshot.version,
        before_bytes: Some(snapshot.bytes),
        original_permissions: Some(snapshot.permissions),
        after_bytes: None,
        file_payload,
        operation_results: vec![json!({
            "operation_id": "file-0",
            "operation_index": 0,
            "resource_id": "resource-1",
            "request_path": request_path,
            "status": "applied",
            "original_range": Value::Null,
            "final_range": Value::Null,
        })],
    };
    // Reuse the exact extension accepted before the filesystem commit.
    // 复用文件系统提交前已通过校验的精确扩展。
    let extension = commit_validated_changes("applied", std::slice::from_ref(&prepared), "file")?;
    Ok(native_file_change_response(
        "delete",
        vec![prepared],
        extension,
    ))
}

/// Moves one regular file transactionally without decoding or changing its bytes.
/// 以事务方式移动一份普通文件且不解码或改变其字节。
fn native_move_file(
    workspace_root: &Path,
    path: &Path,
    request_path: &str,
    object: &Map<String, Value>,
) -> Result<FileToolResponse, FileToolError> {
    let destination_raw = required_string(object, "destination")?;
    let destination = resolve_path(workspace_root, &destination_raw)?;
    if path_key(path)? == path_key(&destination)? {
        return Err(FileToolError::new(
            "same_path",
            "The source and destination refer to the same path. No move was performed.",
        ));
    }
    if path_exists_including_symlink(&destination) {
        return Err(FileToolError::new(
            "destination_exists",
            format!(
                "The move destination already exists: {}",
                display_path(&destination)
            ),
        ));
    }
    validate_parent_directory(&destination)?;
    let snapshot = read_raw_snapshot(path)?;
    let mut file_payload = raw_lifecycle_payload(path, "rename", &snapshot, "resource-1");
    if let Some(payload) = file_payload.as_object_mut() {
        payload.insert(String::from("path"), json!(display_path(&destination)));
        payload.insert(String::from("old_path"), json!(display_path(path)));
        payload.insert(String::from("new_path"), json!(display_path(&destination)));
    }
    let prepared = PreparedChange {
        text_formats: None,
        change_kind: String::from("rename"),
        source_path: path.to_path_buf(),
        destination_path: Some(destination.clone()),
        original_version: snapshot.version,
        before_bytes: Some(snapshot.bytes.clone()),
        original_permissions: Some(snapshot.permissions),
        after_bytes: Some(snapshot.bytes),
        file_payload,
        operation_results: vec![json!({
            "operation_id": "file-0",
            "operation_index": 0,
            "resource_id": "resource-1",
            "request_path": request_path,
            "destination_request_path": destination_raw,
            "resolved_path": display_path(&destination),
            "status": "applied",
            "original_range": Value::Null,
            "final_range": Value::Null,
        })],
    };
    // Reuse the exact extension accepted before the filesystem commit.
    // 复用文件系统提交前已通过校验的精确扩展。
    let extension = commit_validated_changes("applied", std::slice::from_ref(&prepared), "file")?;
    Ok(native_file_change_response(
        "move",
        vec![prepared],
        extension,
    ))
}

/// Builds one whole-file modify row with canonical before and after line domains.
/// 使用 canonical 的变更前后逻辑行域构建一条整文件修改记录。
///
/// `path` identifies the modified file, `before_lines` contains the original logical
/// lines with terminators, and `after_lines` contains the replacement lines and terminators. The returned row
/// is valid only for the V1 `modify` lifecycle and therefore always carries one hunk.
/// `path` 标识被修改文件，`before_lines` 包含原始行及终止符，`after_lines` 包含替换后的
/// 行及终止符。返回记录仅用于 V1 `modify` 生命周期，因此始终携带一条 hunk。
fn whole_file_modify_payload(
    path: &Path,
    before_lines: &[DocumentLine],
    after_lines: &[DocumentLine],
) -> Value {
    // Trim unchanged edges so a small whole-file overwrite does not report every line as edited.
    // 去除未变的首尾，避免小范围整文件覆盖把所有行都报告为编辑。
    let mut prefix = before_lines
        .iter()
        .zip(after_lines)
        .take_while(|(left, right)| left == right)
        .count();
    // Format-only rewrites still replace the final physical line; formats carry the exact byte distinction.
    // 仅格式变化仍替换最后一条物理行；格式元数据记录精确字节差异。
    let format_only = prefix == before_lines.len() && prefix == after_lines.len();
    if format_only && prefix > 0 {
        prefix -= 1;
    }
    let suffix = if format_only {
        0
    } else {
        before_lines[prefix..]
            .iter()
            .rev()
            .zip(after_lines[prefix..].iter().rev())
            .take_while(|(left, right)| left == right)
            .count()
    };
    json!({
        "resource_id": "resource-1",
        "change": "modify",
        "path": display_path(path),
        "hunks": [{
            "before": "",
            "delete": before_lines[prefix..before_lines.len() - suffix].iter().enumerate().map(|(index, line)| json!({"line": prefix + index + 1, "content": line.content})).collect::<Vec<_>>(),
            "insert": after_lines[prefix..after_lines.len() - suffix].iter().enumerate().map(|(index, line)| json!({"line": prefix + index + 1, "content": line.content})).collect::<Vec<_>>(),
            "after": "",
        }],
    })
}

/// Builds one whole-file create row with explicit bounded lifecycle content.
/// 使用明确且有界的生命周期正文构建一条整文件创建记录。
///
/// `path` identifies the newly created file, `text` is the exact normalized text to
/// publish, and `lines` supplies the authoritative logical-line count. The returned
/// row intentionally contains no modify hunks.
/// `path` 标识新建文件，`text` 是即将发布的精确归一化文本，`lines` 提供权威逻辑
/// 行数。返回记录有意不包含任何 modify hunk。
fn whole_file_create_payload(path: &Path, text: &str, lines: &[String]) -> Value {
    // payload is the lifecycle-only row that must never acquire modify hunks.
    // payload 是绝不能携带修改 hunk 的纯生命周期记录。
    let mut payload = Map::new();
    payload.insert(String::from("resource_id"), json!("resource-1"));
    payload.insert(String::from("change"), json!("create"));
    payload.insert(String::from("path"), json!(display_path(path)));
    insert_lifecycle_content(&mut payload, text, lines);
    Value::Object(payload)
}

/// Builds the model result and common change set for native file mutations.
/// 为原生文件变更构建模型结果与公共 change set。
/// `action` labels the response; `changes` supplies committed rows; `extension` is the prevalidated metadata.
/// `action` 标记响应，`changes` 提供已提交记录，`extension` 是预先校验的元数据。
/// Returns the model result and the same persistable ChangeSet metadata without regenerating it.
/// 返回模型结果与同一份可持久化 ChangeSet 元数据，不重新生成。
fn native_file_change_response(
    action: &str,
    changes: Vec<PreparedChange>,
    extension: Value,
) -> FileToolResponse {
    let files = changes
        .iter()
        .map(|change| change.file_payload.clone())
        .collect::<Vec<_>>();
    let operation_results = changes
        .iter()
        .flat_map(|change| change.operation_results.iter().cloned())
        .collect::<Vec<_>>();
    FileToolResponse {
        model_output: json!({
            "ok": true,
            "action": action,
            "file_count": files.len(),
            "operations": operation_results,
        }),
        read_metadata: None,
        change_set: Some(json!({
            "mode": "applied",
            "summary": format!("File {action} applied."),
            "files": files,
            "extensions": {
                "vulcan.file_tools.v2": extension
            },
        })),
    }
}

/// Builds the shared native change-set extension used by all three public file tools.
/// 构建三个公开原生文件工具共用的 change-set 扩展。
fn native_file_tools_extension(
    source_tool: &str,
    changes: &[PreparedChange],
    operation_results: &[Value],
    commit_scope: &str,
) -> Value {
    let resources = changes
        .iter()
        .enumerate()
        .map(|(resource_index, change)| {
            let operations = change
                .operation_results
                .iter()
                .filter_map(Value::as_object)
                .collect::<Vec<_>>();
            let resource_id = change
                .file_payload
                .get("resource_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("resource-{}", resource_index + 1));
            let request_path = operations
                .iter()
                .find_map(|operation| operation.get("request_path").and_then(Value::as_str));
            let operation_indexes = operations
                .iter()
                .filter_map(|operation| operation.get("operation_index").cloned())
                .collect::<Vec<_>>();
            let hunk_indexes = operations
                .iter()
                .filter_map(|operation| operation.get("hunk_index").cloned())
                .filter(|value| !value.is_null())
                .collect::<Vec<_>>();
            let ranges = operations
                .iter()
                .map(|operation| {
                    json!({
                        "operation_id": operation.get("operation_id").cloned().unwrap_or(Value::Null),
                        "operation_index": operation.get("operation_index").cloned().unwrap_or(Value::Null),
                        "hunk_index": operation.get("hunk_index").cloned().unwrap_or(Value::Null),
                        "original_range": operation.get("original_range").cloned().unwrap_or(Value::Null),
                        "final_range": operation.get("final_range").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "resource_id": resource_id,
                "request_path": request_path,
                "resolved_path": display_path(
                    change.destination_path.as_ref().unwrap_or(&change.source_path)
                ),
                "operation_indexes": operation_indexes,
                "hunk_indexes": hunk_indexes,
                "before_text_format": match &change.text_formats {
                    Some(formats) => formats.before.clone(),
                    None => strict_text_format(change.before_bytes.as_deref()),
                },
                "after_text_format": match &change.text_formats {
                    Some(formats) => formats.after.clone(),
                    None => strict_text_format(change.after_bytes.as_deref()),
                },
                "actual_ranges": ranges,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "source_tool": source_tool,
        "operation_results": operation_results,
        "resources": resources,
        "transaction": {"commit_scope": commit_scope, "rollback_state": "not_needed"},
    })
}

/// Returns strict text-format metadata or null for absent, binary, or ambiguous bytes.
/// 返回严格文本格式元数据；字节缺失、二进制或编码不明确时返回 null。
fn strict_text_format(bytes: Option<&[u8]>) -> Value {
    let Some(bytes) = bytes else {
        return Value::Null;
    };
    let Ok(decoded) = decode_text(bytes, None) else {
        return Value::Null;
    };
    let document = TextDocument::from_decoded(decoded);
    document_text_format(&document)
}

/// Projects the already chosen codec and exact terminators without detecting the same bytes again.
/// 投影已选定编码及精确终止符，不重新检测同一份字节。
/// Parameters: document is the preflight authority; returns content-free format metadata.
/// 参数：document 是预检权威；返回不包含正文的格式元数据。
fn document_text_format(document: &TextDocument) -> Value {
    json!({
        "encoding": document.encoding.label(),
        "encoding_source": document.encoding_source.as_str(),
        "encoding_certain": document.encoding_certain,
        "bom": document.bom.is_present(),
        "line_ending": document.line_ending_label(),
        "preferred_line_ending": document.preferred_line_ending.label(),
        "mixed_line_endings": document.mixed_line_endings,
        "final_newline": document.final_newline(),
    })
}

/// Returns the canonical BOM for one encoding when presence is requested.
/// 在请求存在 BOM 时返回编码对应的 canonical BOM。
fn default_bom_for_encoding(encoding: TextEncoding) -> Result<TextBom, FileToolError> {
    match encoding {
        TextEncoding::Utf8 => Ok(TextBom::Utf8),
        TextEncoding::Utf16Le => Ok(TextBom::Utf16Le),
        TextEncoding::Utf16Be => Ok(TextBom::Utf16Be),
        TextEncoding::Utf32Le => Ok(TextBom::Utf32Le),
        TextEncoding::Utf32Be => Ok(TextBom::Utf32Be),
        TextEncoding::Legacy(_) => Err(FileToolError::new(
            "unsupported_encoding",
            "A BOM cannot be added to the selected legacy encoding.",
        )),
    }
}

/// Converts one strict codec failure into the shared file-tool error type.
/// 将一条严格编解码失败转换为共享文件工具错误类型。
fn codec_error(error: crate::text_encoding::TextCodecError) -> FileToolError {
    FileToolError::new(error.code, error.message)
}

/// Executes one target and returns one bounded result row.
/// 执行一个目标并返回一条有界结果行。
fn execute_file_target(
    workspace_root: &Path,
    request: &Map<String, Value>,
    target: &Value,
    action: &str,
) -> Result<Value, FileToolError> {
    let target_object = target
        .as_object()
        .ok_or_else(|| FileToolError::new("invalid_arguments", "Each target must be an object."))?;
    validate_file_target_fields(target_object, action)?;
    let raw_path = required_string(target_object, "path")?;
    let path = resolve_path(workspace_root, &raw_path)?;
    let path_text = display_path(&path);
    match action {
        "read" => read_target(&path, &path_text, target_object),
        "list" => list_target(&path, &path_text, request),
        "search" => search_target(&path, &path_text, request),
        "stat" => stat_target(&path, &path_text),
        _ => Err(FileToolError::new(
            "invalid_arguments",
            "Unsupported file action.",
        )),
    }
}

/// Rejects request-level fields that do not belong to the selected read action.
/// 拒绝不属于当前只读动作的请求级字段，避免模型参数被静默忽略。
fn validate_file_request_fields(
    object: &Map<String, Value>,
    action: &str,
) -> Result<(), FileToolError> {
    let allowed = match action {
        "read" => ["action", "targets"].as_slice(),
        "list" => [
            "action",
            "targets",
            "recursive",
            "include_ignored",
            "pattern",
            "limit",
        ]
        .as_slice(),
        "search" => [
            "action",
            "targets",
            "query",
            "query_type",
            "recursive",
            "include_ignored",
            "case_sensitive",
            "context_lines",
            "limit",
        ]
        .as_slice(),
        "stat" => ["action", "targets"].as_slice(),
        _ => {
            return Err(FileToolError::new(
                "invalid_arguments",
                "Unsupported file action.",
            ));
        }
    };
    validate_allowed_fields(object, allowed, "file request")
}

/// Rejects target-level fields that do not belong to the selected read action.
/// 拒绝不属于当前只读动作的目标级字段，保证每个字段都有明确语义。
fn validate_file_target_fields(
    object: &Map<String, Value>,
    action: &str,
) -> Result<(), FileToolError> {
    let allowed = match action {
        "read" => ["path", "segments", "numbered"].as_slice(),
        "list" | "search" | "stat" => ["path"].as_slice(),
        _ => {
            return Err(FileToolError::new(
                "invalid_arguments",
                "Unsupported file action.",
            ));
        }
    };
    validate_allowed_fields(object, allowed, "file target")
}

/// Rejects operation fields that are not meaningful for one edit kind.
/// 拒绝对当前编辑类型没有意义的字段，避免形成不透明的隐式兼容。
fn validate_edit_operation_fields(
    object: &Map<String, Value>,
    kind: EditKind,
) -> Result<(), FileToolError> {
    let allowed = match kind {
        EditKind::Create => ["id", "operation", "path", "new_lines", "final_newline"].as_slice(),
        EditKind::Replace => [
            "id",
            "operation",
            "path",
            "anchors",
            "old_lines",
            "new_lines",
            "match_mode",
            "expected_version",
            "final_newline",
        ]
        .as_slice(),
        EditKind::Delete => ["id", "operation", "path", "expected_version"].as_slice(),
        EditKind::Move => ["id", "operation", "path", "destination", "expected_version"].as_slice(),
    };
    validate_allowed_fields(object, allowed, "file_edit operation")
}

/// Validates a closed JSON object against the fields permitted by its contract.
/// 按契约校验封闭 JSON 对象的字段白名单。
fn validate_allowed_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), FileToolError> {
    for field in object.keys() {
        if !allowed.contains(&field.as_str()) {
            return Err(FileToolError::new(
                "invalid_arguments",
                format!("Unknown field {field} in {context}."),
            ));
        }
    }
    Ok(())
}

/// Rejects cross-operation source and destination collisions before any snapshot is planned.
/// 在规划快照前拒绝跨操作的源路径与目标路径冲突。
fn validate_operation_path_conflicts(
    operations: &[ParsedEditOperation],
) -> Result<(), FileToolError> {
    let mut destinations = HashSet::new();
    for (index, operation) in operations.iter().enumerate() {
        let Some(destination) = operation.destination.as_ref() else {
            continue;
        };
        let destination_key = path_key(destination)?;
        if !destinations.insert(destination_key.clone()) {
            return Err(FileToolError::new(
                "overlapping_operations",
                format!(
                    "Multiple operations target the same destination: {}.",
                    display_path(destination)
                ),
            ));
        }
        for (other_index, other) in operations.iter().enumerate() {
            if index != other_index && destination_key == path_key(&other.path)? {
                return Err(FileToolError::new(
                    "overlapping_operations",
                    format!(
                        "A move destination conflicts with another operation path: {}.",
                        display_path(destination)
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Reads one UTF-8 text target and applies bounded logical-line segments.
/// 读取一个 UTF-8 文本目标并应用有界逻辑行分段。
fn read_target(
    path: &Path,
    path_text: &str,
    target: &Map<String, Value>,
) -> Result<Value, FileToolError> {
    let snapshot = read_snapshot(path)?;
    let numbered = optional_bool(target, "numbered", true)?;
    let segments = parse_segments(target.get("segments"))?;
    let mut output = Vec::new();
    let mut truncated = false;
    let mut response_bytes = 0usize;
    if segments.is_empty() {
        for (index, line) in snapshot.lines.iter().enumerate() {
            let value = line_value(index + 1, line, numbered);
            if output.len() >= MAX_READ_LINES
                || response_bytes.saturating_add(serialized_value_bytes(&value))
                    > MAX_READ_RESPONSE_BYTES
            {
                truncated = true;
                break;
            }
            response_bytes = response_bytes.saturating_add(serialized_value_bytes(&value));
            output.push(value);
        }
    } else {
        for (start_line, line_count) in segments {
            let start = start_line.saturating_sub(1);
            let end = start.saturating_add(line_count).min(snapshot.lines.len());
            for index in start..end {
                let value = line_value(index + 1, &snapshot.lines[index], numbered);
                if output.len() >= MAX_READ_LINES
                    || response_bytes.saturating_add(serialized_value_bytes(&value))
                        > MAX_READ_RESPONSE_BYTES
                {
                    truncated = true;
                    break;
                }
                response_bytes = response_bytes.saturating_add(serialized_value_bytes(&value));
                output.push(value);
            }
            if truncated {
                break;
            }
        }
    }
    Ok(json!({
        "ok": true,
        "path": path_text,
        "version": snapshot.version,
        "encoding": snapshot.document.encoding.label(),
        "encoding_source": snapshot.document.encoding_source.as_str(),
        "line_ending": snapshot.document.line_ending_label(),
        "bom": snapshot.bom,
        "final_newline": snapshot.final_newline,
        "line_count": snapshot.lines.len(),
        "returned_line_count": output.len(),
        "truncated": truncated,
        "lines": output,
    }))
}

/// Returns bounded existence, type, size, modification, text, and version metadata.
/// 返回有界的存在性、类型、大小、修改时间、文本和版本元数据。
fn stat_target(path: &Path, path_text: &str) -> Result<Value, FileToolError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("path_not_found", path, error))?;
    if metadata.file_type().is_symlink() {
        return Err(FileToolError::new(
            "unsupported_file_type",
            "Symbolic links are not supported by the native file tool.",
        ));
    }
    let mut result = json!({
        "ok": true,
        "path": path_text,
        "exists": true,
        "kind": if metadata.is_dir() { "directory" } else if metadata.is_file() { "file" } else { "other" },
        "size": metadata.len(),
        "modified": modified_millis(&metadata),
    });
    if metadata.is_file()
        && metadata.len() <= MAX_TEXT_BYTES as u64
        && let Ok(snapshot) = read_snapshot(path)
    {
        result["encoding"] = Value::String(String::from(snapshot.document.encoding.label()));
        result["encoding_source"] =
            Value::String(String::from(snapshot.document.encoding_source.as_str()));
        result["encoding_certain"] = Value::Bool(snapshot.document.encoding_certain);
        result["line_ending"] = Value::String(String::from(snapshot.document.line_ending_label()));
        result["preferred_line_ending"] = Value::String(String::from(
            snapshot.document.preferred_line_ending.label(),
        ));
        result["mixed_line_endings"] = Value::Bool(snapshot.document.mixed_line_endings);
        result["bom"] = Value::Bool(snapshot.bom);
        result["line_count"] = json!(snapshot.lines.len());
        result["final_newline"] = Value::Bool(snapshot.final_newline);
        result["version"] = Value::String(snapshot.version);
    }
    Ok(result)
}

/// Lists one file or recursively walks one directory with bounded glob matching.
/// 列出一个文件或按有界 glob 匹配递归遍历一个目录。
fn list_target(
    path: &Path,
    path_text: &str,
    request: &Map<String, Value>,
) -> Result<Value, FileToolError> {
    let recursive = optional_bool(request, "recursive", true)?;
    let include_ignored = optional_bool(request, "include_ignored", false)?;
    let pattern = optional_string(request, "pattern")?;
    validate_search_text(pattern.as_deref(), "pattern")?;
    let limit = optional_limit(request, "limit", 500, 1, MAX_LIST_ENTRIES)?;
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("path_not_found", path, error))?;
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut response_bytes = 0usize;
    if metadata.is_file() {
        if pattern_matches(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
            pattern.as_deref(),
        ) {
            let entry = file_entry(path)?;
            if serialized_value_bytes(&entry) > MAX_LIST_RESPONSE_BYTES {
                truncated = true;
            } else {
                entries.push(entry);
            }
        }
    } else if metadata.is_dir() {
        let mut pending = vec![path.to_path_buf()];
        while let Some(directory) = pending.pop() {
            let mut directory_entries = fs::read_dir(&directory)
                .map_err(|error| io_error("path_not_found", &directory, error))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| io_error("read_failed", &directory, error))?;
            directory_entries.sort_by_key(|entry| entry.file_name());
            for entry in directory_entries {
                let entry_path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if !include_ignored && is_ignored_name(&name) {
                    continue;
                }
                let entry_metadata = fs::symlink_metadata(&entry_path)
                    .map_err(|error| io_error("read_failed", &entry_path, error))?;
                if entry_metadata.is_dir() {
                    if pattern_matches(&name, pattern.as_deref()) {
                        let entry = file_entry(&entry_path)?;
                        let entry_bytes = serialized_value_bytes(&entry);
                        if response_bytes.saturating_add(entry_bytes) > MAX_LIST_RESPONSE_BYTES {
                            truncated = true;
                            break;
                        }
                        response_bytes = response_bytes.saturating_add(entry_bytes);
                        entries.push(entry);
                        if entries.len() > limit {
                            truncated = true;
                            break;
                        }
                    }
                    if recursive {
                        pending.push(entry_path);
                    }
                    continue;
                }
                if entry_metadata.is_file() && pattern_matches(&name, pattern.as_deref()) {
                    let entry = file_entry(&entry_path)?;
                    let entry_bytes = serialized_value_bytes(&entry);
                    if response_bytes.saturating_add(entry_bytes) > MAX_LIST_RESPONSE_BYTES {
                        truncated = true;
                        break;
                    }
                    response_bytes = response_bytes.saturating_add(entry_bytes);
                    entries.push(entry);
                    if entries.len() > limit {
                        truncated = true;
                        break;
                    }
                }
            }
            if truncated {
                break;
            }
        }
    } else {
        return Err(FileToolError::new(
            "unsupported_file_type",
            "The target is not a regular file or directory.",
        ));
    }
    if entries.len() > limit {
        entries.truncate(limit);
    }
    entries.sort_by(|left, right| {
        left.get("path")
            .and_then(Value::as_str)
            .cmp(&right.get("path").and_then(Value::as_str))
    });
    Ok(json!({
        "ok": true,
        "path": path_text,
        "entries": entries,
        "truncated": truncated,
    }))
}

/// Searches one file or directory using literal or regular-expression matching.
/// 使用字面量或正则匹配搜索一个文件或目录。
fn search_target(
    path: &Path,
    path_text: &str,
    request: &Map<String, Value>,
) -> Result<Value, FileToolError> {
    let query = required_string(request, "query")?;
    validate_search_text(Some(&query), "query")?;
    let query_type =
        optional_string(request, "query_type")?.unwrap_or_else(|| String::from("literal"));
    let case_sensitive = optional_bool(request, "case_sensitive", true)?;
    let context_lines = optional_limit(request, "context_lines", 2, 0, 20)?;
    let limit = optional_limit(request, "limit", MAX_SEARCH_RESULTS, 1, MAX_SEARCH_RESULTS)?;
    let matcher = SearchMatcher::new(&query, &query_type, case_sensitive)?;
    let mut files = Vec::new();
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("path_not_found", path, error))?;
    if metadata.is_file() {
        files.push(path.to_path_buf());
    } else if metadata.is_dir() {
        let recursive = optional_bool(request, "recursive", true)?;
        let include_ignored = optional_bool(request, "include_ignored", false)?;
        collect_files(path, recursive, include_ignored, &mut files)?;
    } else {
        return Err(FileToolError::new(
            "unsupported_file_type",
            "The search target is not a regular file or directory.",
        ));
    }
    files.sort_by_key(|file| display_path(file));
    let mut matches = Vec::new();
    let mut truncated = false;
    let mut response_bytes = 0usize;
    for file in files {
        let snapshot = match read_snapshot(&file) {
            Ok(value) => value,
            Err(_) => continue,
        };
        for (index, line) in snapshot.lines.iter().enumerate() {
            if !matcher.matches(line) {
                continue;
            }
            if matches.len() >= limit {
                truncated = true;
                break;
            }
            let start = index.saturating_sub(context_lines);
            let end = (index + context_lines + 1).min(snapshot.lines.len());
            let value = json!({
                "path": display_path(&file),
                "line": index + 1,
                "text": line,
                "before": snapshot.lines[start..index].iter().enumerate().map(|(offset, value)| json!({"line": start + offset + 1, "text": value})).collect::<Vec<_>>(),
                "after": snapshot.lines[index + 1..end].iter().enumerate().map(|(offset, value)| json!({"line": index + offset + 2, "text": value})).collect::<Vec<_>>(),
            });
            let value_bytes = serialized_value_bytes(&value);
            if response_bytes.saturating_add(value_bytes) > MAX_SEARCH_RESPONSE_BYTES {
                truncated = true;
                break;
            }
            response_bytes = response_bytes.saturating_add(value_bytes);
            matches.push(value);
        }
        if truncated {
            break;
        }
    }
    if matches.len() > limit {
        matches.truncate(limit);
    }
    Ok(json!({
        "ok": true,
        "path": path_text,
        "query": query,
        "query_type": query_type,
        "matches": matches,
        "truncated": truncated,
    }))
}

/// Returns regular files below a directory while honoring the native default ignore names.
/// 在目录下收集普通文件，并遵守原生默认忽略名称。
fn collect_files(
    root: &Path,
    recursive: bool,
    include_ignored: bool,
    files: &mut Vec<PathBuf>,
) -> Result<(), FileToolError> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut directory_entries = fs::read_dir(&directory)
            .map_err(|error| io_error("read_failed", &directory, error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| io_error("read_failed", &directory, error))?;
        directory_entries.sort_by_key(|entry| entry.file_name());
        for entry in directory_entries {
            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !include_ignored && is_ignored_name(&name) {
                continue;
            }
            let metadata = fs::symlink_metadata(&entry_path)
                .map_err(|error| io_error("read_failed", &entry_path, error))?;
            if metadata.is_dir() {
                if recursive {
                    pending.push(entry_path);
                }
            } else if metadata.is_file() {
                files.push(entry_path);
                if files.len() > MAX_SEARCH_FILES {
                    return Err(FileToolError::new(
                        "output_limit_exceeded",
                        format!(
                            "A directory search may scan at most {MAX_SEARCH_FILES} regular files."
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Returns one regular-file listing row with bounded metadata.
/// 返回一条带有界元数据的普通文件列表行。
fn file_entry(path: &Path) -> Result<Value, FileToolError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("read_failed", path, error))?;
    Ok(json!({
        "path": display_path(path),
        "kind": if metadata.is_dir() { "directory" } else { "file" },
        "size": metadata.len(),
        "modified": modified_millis(&metadata),
    }))
}

/// Returns one line object with optional one-based numbering.
/// 返回一条可选带一基行号的行对象。
fn line_value(line: usize, content: &str, numbered: bool) -> Value {
    if numbered {
        json!({"line": line, "content": content})
    } else {
        json!({"content": content})
    }
}

/// Returns a conservative serialized size for one bounded result row.
/// 返回一条有界结果行的保守序列化大小。
fn serialized_value_bytes(value: &impl serde::Serialize) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len().saturating_add(1))
        .unwrap_or(usize::MAX)
}

/// Executes the complete structured edit transaction synchronously.
/// 同步执行完整的结构化编辑事务。
fn execute_file_edit_sync(
    workspace_root: &Path,
    arguments: &Value,
) -> Result<FileToolResponse, FileToolError> {
    if arguments
        .as_object()
        .is_some_and(|object| object.contains_key("operations"))
    {
        return execute_legacy_file_edit_sync(workspace_root, arguments);
    }
    execute_exact_file_edit_sync(workspace_root, arguments)
}

/// Executes the public exact-substring file_edit contract.
/// 执行公开的精确子串 file_edit 契约。
fn execute_exact_file_edit_sync(
    workspace_root: &Path,
    arguments: &Value,
) -> Result<FileToolResponse, FileToolError> {
    let object = arguments.as_object().ok_or_else(|| {
        enrich_request_edit_error(FileToolError::new(
            "invalid_arguments",
            "The file_edit arguments must be an object.",
        ))
    })?;
    validate_allowed_fields(
        object,
        &[
            "path",
            "old_string",
            "new_string",
            "replace_all",
            "encoding",
        ],
        "file_edit request",
    )
    .map_err(enrich_request_edit_error)?;
    let request_bytes = serde_json::to_vec(arguments).map_err(|error| {
        enrich_request_edit_error(FileToolError::new(
            "invalid_arguments",
            format!("The file_edit request could not be serialized: {error}"),
        ))
    })?;
    if request_bytes.len() > MAX_EDIT_REQUEST_BYTES {
        return Err(enrich_request_edit_error(FileToolError::new(
            "output_limit_exceeded",
            format!("The file_edit request body may not exceed {MAX_EDIT_REQUEST_BYTES} bytes."),
        )));
    }
    let raw_path = required_string(object, "path").map_err(enrich_request_edit_error)?;
    let old_string = required_string(object, "old_string").map_err(enrich_request_edit_error)?;
    let new_string = object
        .get("new_string")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            enrich_request_edit_error(FileToolError::new(
                "invalid_arguments",
                "new_string is required and must be a string.",
            ))
        })?;
    let replace_all =
        optional_bool(object, "replace_all", false).map_err(enrich_request_edit_error)?;
    let path = resolve_path(workspace_root, &raw_path).map_err(enrich_request_edit_error)?;
    let resource_locks =
        acquire_resource_locks(std::slice::from_ref(&path)).map_err(enrich_request_edit_error)?;
    let _resource_guards = lock_resource_set(&resource_locks).map_err(enrich_request_edit_error)?;
    let encoding = optional_string(object, "encoding").map_err(enrich_request_edit_error)?;
    let snapshot =
        read_edit_snapshot(&path, encoding.as_deref()).map_err(enrich_request_edit_error)?;
    let replacement = snapshot
        .document
        .replace_exact(&old_string, &new_string, replace_all)
        .map_err(|error| {
            let mut details = Map::new();
            details.insert(String::from("path"), json!(display_path(&path)));
            details.insert(String::from("requires_read"), Value::Bool(true));
            details.insert(String::from("retryable"), Value::Bool(false));
            if let Some(match_count) = error.match_count {
                details.insert(String::from("match_count"), json!(match_count));
            }
            if let Some(ranges) = error.candidate_ranges.as_ref() {
                details.insert(String::from("candidate_count"), json!(ranges.len()));
                details.insert(
                    String::from("candidate_ranges"),
                    candidate_ranges_value(ranges),
                );
                details.insert(
                    String::from("candidates_truncated"),
                    json!(error.match_count.is_some_and(|count| count > ranges.len())),
                );
            }
            enrich_request_edit_error(
                FileToolError::new(error.code, error.message).with_details(Value::Object(details)),
            )
        })?;
    let after_bytes = replacement.document.encode().map_err(|error| {
        let codec = codec_error(error).with_details(json!({
            "path": display_path(&path),
            "requires_read": false,
            "retryable": false,
            "hint": "Use text representable in the current encoding, or explicitly convert the complete file with file_write.",
        }));
        enrich_request_edit_error(codec)
    })?;
    if after_bytes == snapshot.bytes {
        return Err(enrich_request_edit_error(FileToolError::new(
            "no_change",
            "The exact replacement would not change the file.",
        ).with_details(json!({"hint": "Provide new_string that differs from the matched text, or skip the edit."}))));
    }
    let final_lines = replacement.document.logical_lines();
    let (operation_windows, merged_windows) = exact_edit_line_windows(
        &snapshot.document.lines,
        &replacement.document.lines,
        &replacement.ranges,
    );
    let hunks = merged_windows
        .iter()
        .map(|window| exact_edit_hunk(&snapshot.lines, &final_lines, *window))
        .collect::<Vec<_>>();
    let mut operation_results = Vec::with_capacity(replacement.ranges.len());
    for (operation_index, window) in operation_windows.iter().enumerate() {
        operation_results.push(json!({
            "operation_id": format!("file-edit-{operation_index}"),
            "operation_index": operation_index,
            "resource_id": "resource-1",
            "request_path": raw_path,
            "resolved_path": display_path(&path),
            "status": "applied",
            "match_mode": "exact_text",
            "original_range": {
                "start_line": window.original_start + 1,
                "line_count": window.original_end.saturating_sub(window.original_start),
            },
            "final_range": {
                "start_line": window.final_start + 1,
                "line_count": window.final_end.saturating_sub(window.final_start),
            },
        }));
    }
    let file_payload = json!({
        "resource_id": "resource-1",
        "change": "modify",
        "path": display_path(&path),
        "hunks": hunks,
    });
    let prepared = PreparedChange {
        text_formats: Some(PreparedTextFormats {
            before: document_text_format(&snapshot.document),
            after: document_text_format(&replacement.document),
        }),
        change_kind: String::from("modify"),
        source_path: path.clone(),
        destination_path: None,
        original_version: snapshot.version,
        before_bytes: Some(snapshot.bytes),
        original_permissions: Some(snapshot.permissions),
        after_bytes: Some(after_bytes),
        file_payload: file_payload.clone(),
        operation_results: operation_results.clone(),
    };
    // Reuse resource formats validated before mutation; the extra fields below are presentation metadata.
    // 复用变更前已校验的资源格式；下方附加字段仅为展示元数据。
    let mut extension =
        commit_validated_changes("applied", std::slice::from_ref(&prepared), "file_edit")
            .map_err(enrich_request_edit_error)?;
    if let Some(object) = extension.as_object_mut() {
        object.insert(
            String::from("encoding"),
            json!(replacement.document.encoding.label()),
        );
        object.insert(
            String::from("bom"),
            json!(replacement.document.bom.is_present()),
        );
        object.insert(
            String::from("line_ending"),
            json!(replacement.document.line_ending_label()),
        );
    }
    Ok(FileToolResponse {
        model_output: json!({
            "ok": true,
            "path": display_path(&path),
            "match_count": replacement.match_count,
            "encoding": replacement.document.encoding.label(),
            "bom": replacement.document.bom.is_present(),
            "line_ending": replacement.document.line_ending_label(),
            "operation_count": operation_results.len(),
            "operations": operation_results,
        }),
        read_metadata: None,
        change_set: Some(json!({
            "mode": "applied",
            "summary": "Exact text replacement applied.",
            "files": [file_payload],
            "extensions": {"vulcan.file_tools.v2": extension},
        })),
    })
}

/// One minimal line window changed by an exact-text replacement.
/// 一段由精确文本替换改变的最小行窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExactEditLineWindow {
    /// Zero-based original start line.
    /// 零基原始起始行。
    original_start: usize,
    /// Zero-based exclusive original end line.
    /// 零基原始结束行，不包含该行。
    original_end: usize,
    /// Zero-based final start line.
    /// 零基最终起始行。
    final_start: usize,
    /// Zero-based exclusive final end line.
    /// 零基最终结束行，不包含该行。
    final_end: usize,
}

/// Projects exact substring matches into per-operation and non-overlapping file hunks.
/// 将精确子串命中投影为逐操作范围与互不重叠的文件 hunk。
fn exact_edit_line_windows(
    before_lines: &[DocumentLine],
    after_lines: &[DocumentLine],
    ranges: &[TextReplacementRange],
) -> (Vec<ExactEditLineWindow>, Vec<ExactEditLineWindow>) {
    let mut cumulative_delta = 0isize;
    let mut operation_windows = Vec::with_capacity(ranges.len());
    for range in ranges {
        let final_start_line =
            (range.original_start_line as isize + cumulative_delta).max(1) as usize;
        let mut window = ExactEditLineWindow {
            original_start: range
                .original_start_line
                .saturating_sub(1)
                .min(before_lines.len()),
            original_end: range
                .original_start_line
                .saturating_sub(1)
                .saturating_add(range.original_line_count)
                .saturating_add(1)
                .min(before_lines.len()),
            final_start: final_start_line.saturating_sub(1).min(after_lines.len()),
            final_end: final_start_line
                .saturating_sub(1)
                .saturating_add(range.final_line_count)
                .saturating_add(1)
                .min(after_lines.len()),
        };
        trim_exact_edit_line_window(before_lines, after_lines, &mut window);
        operation_windows.push(window);
        cumulative_delta += range.final_line_count as isize - range.original_line_count as isize;
    }

    let mut merged_windows = Vec::<ExactEditLineWindow>::new();
    for window in operation_windows.iter().copied() {
        if let Some(previous) = merged_windows.last_mut()
            && (window.original_start <= previous.original_end
                || window.final_start <= previous.final_end)
        {
            previous.original_start = previous.original_start.min(window.original_start);
            previous.original_end = previous.original_end.max(window.original_end);
            previous.final_start = previous.final_start.min(window.final_start);
            previous.final_end = previous.final_end.max(window.final_end);
            trim_exact_edit_line_window(before_lines, after_lines, previous);
            continue;
        }
        merged_windows.push(window);
    }
    (operation_windows, merged_windows)
}

/// Removes equal complete lines, including terminators, from both ends of an edit window.
/// 从编辑窗口两端移除包含终止符在内完全相等的行。
fn trim_exact_edit_line_window(
    before_lines: &[DocumentLine],
    after_lines: &[DocumentLine],
    window: &mut ExactEditLineWindow,
) {
    while window.original_start < window.original_end
        && window.final_start < window.final_end
        && before_lines[window.original_start] == after_lines[window.final_start]
    {
        window.original_start += 1;
        window.final_start += 1;
    }
    while window.original_start < window.original_end
        && window.final_start < window.final_end
        && before_lines[window.original_end - 1] == after_lines[window.final_end - 1]
    {
        window.original_end -= 1;
        window.final_end -= 1;
    }
}

/// Builds one canonical change-set hunk from actual before and after logical lines.
/// 使用实际变更前后逻辑行构建一个 canonical change-set hunk。
fn exact_edit_hunk(
    before_lines: &[String],
    after_lines: &[String],
    window: ExactEditLineWindow,
) -> Value {
    let before_context_start = window.original_start.saturating_sub(2);
    let after_context_end = window
        .original_end
        .saturating_add(2)
        .min(before_lines.len());
    json!({
        "before": before_lines[before_context_start..window.original_start].join("\n"),
        "delete": before_lines[window.original_start..window.original_end]
            .iter()
            .enumerate()
            .map(|(index, content)| json!({"line": window.original_start + index + 1, "content": content}))
            .collect::<Vec<_>>(),
        "insert": after_lines[window.final_start..window.final_end]
            .iter()
            .enumerate()
            .map(|(index, content)| json!({"line": window.final_start + index + 1, "content": content}))
            .collect::<Vec<_>>(),
        "after": before_lines[window.original_end..after_context_end].join("\n"),
    })
}

/// Parses, preflights, and atomically applies one multi-file Apply Patch document.
/// 解析、预校验并原子应用一份多文件 Apply Patch 文档。
fn execute_apply_patch_sync(
    workspace_root: &Path,
    patch: &str,
) -> Result<FileToolResponse, FileToolError> {
    if patch.len() > MAX_EDIT_REQUEST_BYTES {
        return Err(FileToolError::new(
            "output_limit_exceeded",
            format!("The patch may not exceed {MAX_EDIT_REQUEST_BYTES} UTF-8 bytes."),
        ));
    }
    let operations = parse_apply_patch(patch)?;
    if operations.is_empty() || operations.len() > MAX_EDIT_OPERATIONS {
        return Err(FileToolError::new(
            "patch_syntax_error",
            format!("A patch must contain between 1 and {MAX_EDIT_OPERATIONS} file operations."),
        ));
    }
    let mut mutation_paths = Vec::with_capacity(operations.len().saturating_mul(2));
    for operation in &operations {
        mutation_paths.push(resolve_path(workspace_root, operation.path())?);
        if let ParsedPatchFile::Update {
            move_to: Some(destination),
            ..
        } = operation
        {
            mutation_paths.push(resolve_path(workspace_root, destination)?);
        }
    }
    let resource_locks = acquire_resource_locks(&mutation_paths)?;
    let _resource_guards = lock_resource_set(&resource_locks)?;
    let mut source_paths = HashSet::new();
    let mut final_paths = HashSet::new();
    let mut changes = Vec::with_capacity(operations.len());
    for (operation_index, operation) in operations.iter().enumerate() {
        let resource_id = format!("resource-{}", operation_index + 1);
        let source_path = resolve_path(workspace_root, operation.path())?;
        if !source_paths.insert(path_key(&source_path)?) {
            return Err(FileToolError::new(
                "invalid_arguments",
                format!(
                    "A patch may contain only one file operation for each source path: {}",
                    display_path(&source_path)
                ),
            ));
        }
        let prepared = match operation {
            ParsedPatchFile::Add { path: _, lines } => {
                if path_exists_including_symlink(&source_path) {
                    return Err(FileToolError::new(
                        "destination_exists",
                        format!(
                            "The add target already exists: {}",
                            display_path(&source_path)
                        ),
                    ));
                }
                validate_parent_directory(&source_path)?;
                let edit = ParsedEditOperation {
                    id: format!("patch-{operation_index}"),
                    kind: EditKind::Create,
                    path: source_path,
                    destination: None,
                    old_lines: Vec::new(),
                    new_lines: lines.clone(),
                    anchors: Vec::new(),
                    match_mode: String::from("apply_patch"),
                    final_newline: Some(true),
                    expected_version: None,
                };
                build_create_change(&edit, &resource_id, false)?
            }
            ParsedPatchFile::Delete { path: _ } => {
                let snapshot = read_raw_snapshot(&source_path)?;
                build_patch_delete_change(
                    operation_index,
                    operation.path(),
                    source_path,
                    snapshot,
                    &resource_id,
                )
            }
            ParsedPatchFile::Update {
                path: _,
                move_to,
                hunks,
            } => {
                let destination = move_to
                    .as_deref()
                    .map(|path| resolve_path(workspace_root, path))
                    .transpose()?;
                if let Some(destination) = destination.as_ref() {
                    if path_exists_including_symlink(destination) {
                        return Err(FileToolError::new(
                            "destination_exists",
                            format!(
                                "The move destination already exists: {}",
                                display_path(destination)
                            ),
                        ));
                    }
                    validate_parent_directory(destination)?;
                }
                if hunks.is_empty() {
                    let destination = destination.ok_or_else(|| {
                        FileToolError::new(
                            "patch_syntax_error",
                            "An update without hunks requires a Move to destination.",
                        )
                    })?;
                    let snapshot = read_raw_snapshot(&source_path)?;
                    build_patch_move_change(
                        operation_index,
                        operation.path(),
                        source_path,
                        destination,
                        snapshot,
                        &resource_id,
                    )
                } else {
                    let snapshot = read_edit_snapshot(&source_path, None)?;
                    build_patch_update_change(
                        operation_index,
                        operation.path(),
                        source_path,
                        destination,
                        &snapshot,
                        hunks,
                        &resource_id,
                    )?
                }
            }
        };
        let final_path = prepared
            .destination_path
            .as_ref()
            .unwrap_or(&prepared.source_path);
        if !final_paths.insert(path_key(final_path)?) {
            return Err(FileToolError::new(
                "invalid_arguments",
                format!(
                    "A patch may produce each final path only once: {}",
                    display_path(final_path)
                ),
            ));
        }
        changes.push(prepared);
    }
    // Validate the complete resource table before touching any patch destination.
    // 在修改任何补丁目标之前校验完整资源表。
    let extension = commit_validated_changes("applied", &changes, "apply_patch")?;
    let files = changes
        .iter()
        .map(|change| change.file_payload.clone())
        .collect::<Vec<_>>();
    let operation_results = changes
        .iter()
        .flat_map(|change| change.operation_results.iter().cloned())
        .collect::<Vec<_>>();
    Ok(FileToolResponse {
        model_output: json!({
            "ok": true,
            "file_count": files.len(),
            "operation_count": operation_results.len(),
            "operations": operation_results,
        }),
        read_metadata: None,
        change_set: Some(json!({
            "mode": "applied",
            "summary": "Apply Patch changes applied.",
            "files": files,
            "extensions": {
                "vulcan.file_tools.v2": extension
            },
        })),
    })
}

/// Builds one raw-byte delete change for an Apply Patch file operation.
/// 为一项 Apply Patch 文件操作构建原始字节删除变更。
fn build_patch_delete_change(
    operation_index: usize,
    request_path: &str,
    source_path: PathBuf,
    snapshot: RawFileSnapshot,
    resource_id: &str,
) -> PreparedChange {
    let file_payload = raw_lifecycle_payload(&source_path, "delete", &snapshot, resource_id);
    PreparedChange {
        text_formats: None,
        change_kind: String::from("delete"),
        source_path,
        destination_path: None,
        original_version: snapshot.version,
        before_bytes: Some(snapshot.bytes),
        original_permissions: Some(snapshot.permissions),
        after_bytes: None,
        file_payload,
        operation_results: vec![json!({
            "operation_id": format!("patch-{operation_index}"),
            "operation_index": operation_index,
            "resource_id": resource_id,
            "request_path": request_path,
            "status": "applied",
            "original_range": Value::Null,
            "final_range": Value::Null,
        })],
    }
}

/// Builds one raw-byte move change for an Apply Patch operation without text hunks.
/// 为不含文本 hunk 的 Apply Patch 操作构建原始字节移动变更。
fn build_patch_move_change(
    operation_index: usize,
    request_path: &str,
    source_path: PathBuf,
    destination_path: PathBuf,
    snapshot: RawFileSnapshot,
    resource_id: &str,
) -> PreparedChange {
    let mut file_payload = raw_lifecycle_payload(&source_path, "rename", &snapshot, resource_id);
    if let Some(payload) = file_payload.as_object_mut() {
        payload.insert(String::from("path"), json!(display_path(&destination_path)));
        payload.insert(String::from("old_path"), json!(display_path(&source_path)));
        payload.insert(
            String::from("new_path"),
            json!(display_path(&destination_path)),
        );
    }
    PreparedChange {
        text_formats: None,
        change_kind: String::from("rename"),
        source_path,
        destination_path: Some(destination_path),
        original_version: snapshot.version,
        before_bytes: Some(snapshot.bytes.clone()),
        original_permissions: Some(snapshot.permissions),
        after_bytes: Some(snapshot.bytes),
        file_payload,
        operation_results: vec![json!({
            "operation_id": format!("patch-{operation_index}"),
            "operation_index": operation_index,
            "resource_id": resource_id,
            "request_path": request_path,
            "status": "applied",
            "original_range": Value::Null,
            "final_range": Value::Null,
        })],
    }
}

/// Builds one update or move change from parsed patch hunks.
/// 从已解析 patch hunk 构建一条更新或移动变更。
fn build_patch_update_change(
    operation_index: usize,
    request_path: &str,
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
    snapshot: &TextSnapshot,
    hunks: &[ParsedPatchHunk],
    resource_id: &str,
) -> Result<PreparedChange, FileToolError> {
    let replacements = plan_patch_replacements(operation_index, snapshot, hunks)?;
    if replacements.is_empty() && destination_path.is_none() {
        return Err(FileToolError::new(
            "no_change",
            "The update operation contains no effective hunks or move destination.",
        ));
    }
    let after_bytes = if replacements.is_empty() {
        snapshot.bytes.clone()
    } else {
        render_snapshot_replacements(snapshot, &replacements, snapshot.final_newline)?
    };
    if after_bytes == snapshot.bytes && destination_path.is_none() {
        return Err(FileToolError::new(
            "no_change",
            "The patch would not change the file.",
        ));
    }
    let change_kind = if destination_path.is_some() {
        "rename"
    } else {
        "modify"
    };
    let final_path = destination_path.as_ref().unwrap_or(&source_path);
    let mut file_payload = Map::new();
    file_payload.insert(String::from("resource_id"), json!(resource_id));
    file_payload.insert(String::from("change"), json!(change_kind));
    file_payload.insert(String::from("path"), json!(display_path(final_path)));
    if let Some(destination) = destination_path.as_ref() {
        file_payload.insert(String::from("old_path"), json!(display_path(&source_path)));
        file_payload.insert(String::from("new_path"), json!(display_path(destination)));
    }
    file_payload.insert(
        String::from("hunks"),
        Value::Array(
            replacements
                .iter()
                .map(|replacement| replacement_hunk(snapshot, replacement, &replacements))
                .collect(),
        ),
    );
    let mut operation_results = replacements
        .iter()
        .enumerate()
        .map(|(hunk_index, replacement)| {
            let mut result = operation_result(replacement, &replacements, false, resource_id);
            if let Some(object) = result.as_object_mut() {
                object.insert(String::from("operation_index"), json!(operation_index));
                object.insert(String::from("hunk_index"), json!(hunk_index));
                object.insert(String::from("request_path"), json!(request_path));
            }
            result
        })
        .collect::<Vec<_>>();
    if destination_path.is_some() {
        operation_results.push(json!({
            "operation_id": format!("patch-{operation_index}-move"),
            "operation_index": operation_index,
            "hunk_index": Value::Null,
            "resource_id": resource_id,
            "request_path": request_path,
            "status": "applied",
            "original_range": Value::Null,
            "final_range": Value::Null,
        }));
    }
    Ok(PreparedChange {
        text_formats: None,
        change_kind: String::from(change_kind),
        source_path,
        destination_path,
        original_version: snapshot.version.clone(),
        before_bytes: Some(snapshot.bytes.clone()),
        original_permissions: Some(snapshot.permissions.clone()),
        after_bytes: Some(after_bytes),
        file_payload: Value::Object(file_payload),
        operation_results,
    })
}

/// Converts ordered patch hunks into immutable original-snapshot replacement plans.
/// 将有序 patch hunk 转换为不可变原始快照上的替换计划。
fn plan_patch_replacements(
    operation_index: usize,
    snapshot: &TextSnapshot,
    hunks: &[ParsedPatchHunk],
) -> Result<Vec<ReplacementPlan>, FileToolError> {
    let mut working = snapshot.lines.clone();
    let mut replacements = Vec::with_capacity(hunks.len());
    let mut cursor = 0usize;
    let mut cumulative_delta = 0isize;
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        let anchor_start = match hunk.context.as_deref() {
            Some(context) => working
                .iter()
                .enumerate()
                .skip(cursor)
                .find(|(_, line)| line.as_str() == context)
                .map(|(index, _)| index + 1)
                .ok_or_else(|| {
                    FileToolError::new(
                        "patch_context_not_found",
                        format!(
                            "Update operation {operation_index} hunk {hunk_index} could not find @@ {context}."
                        ),
                    )
                })?,
            None => cursor,
        };
        let old_lines = hunk
            .lines
            .iter()
            .filter(|line| line.kind != PatchLineKind::Add)
            .map(|line| line.content.clone())
            .collect::<Vec<_>>();
        let new_lines = hunk
            .lines
            .iter()
            .filter(|line| line.kind != PatchLineKind::Delete)
            .map(|line| line.content.clone())
            .collect::<Vec<_>>();
        let start = if old_lines.is_empty() {
            if hunk.end_of_file && anchor_start != working.len() {
                return Err(FileToolError::new(
                    "patch_context_not_found",
                    format!(
                        "Update operation {operation_index} hunk {hunk_index} did not reach the end of the file."
                    ),
                ));
            }
            if hunk.end_of_file {
                working.len()
            } else {
                anchor_start
            }
        } else {
            let candidates = (anchor_start..=working.len().saturating_sub(old_lines.len()))
                .filter(|start| {
                    working[*start..*start + old_lines.len()] == old_lines
                        && (!hunk.end_of_file || *start + old_lines.len() == working.len())
                })
                .collect::<Vec<_>>();
            match candidates.as_slice() {
                [start] => *start,
                [] => {
                    return Err(FileToolError::new(
                        "patch_context_not_found",
                        format!(
                            "Update operation {operation_index} hunk {hunk_index} could not match its context and deleted lines."
                        ),
                    ));
                }
                _ if hunk.context.is_some() => candidates[0],
                _ => {
                    return Err(FileToolError::new(
                        "ambiguous_match",
                        format!(
                            "Update operation {operation_index} hunk {hunk_index} matches more than once; add an @@ anchor."
                        ),
                    ));
                }
            }
        };
        let original_start_signed = start as isize - cumulative_delta;
        if original_start_signed < 0 {
            return Err(FileToolError::new(
                "overlapping_operations",
                "A patch hunk depends on text inserted by an earlier hunk.",
            ));
        }
        let original_start = original_start_signed as usize;
        let original_end = original_start.saturating_add(old_lines.len());
        if original_end > snapshot.lines.len()
            || snapshot.lines[original_start..original_end] != old_lines
        {
            return Err(FileToolError::new(
                "overlapping_operations",
                "Patch hunks must target non-overlapping ranges in the original file snapshot.",
            ));
        }
        cursor = start.saturating_add(new_lines.len());
        // A context-only hunk has been verified against the file but changes nothing; it must
        // not produce a replacement, because a ChangeSet hunk needs a deleted or inserted line.
        // 仅含上下文的 hunk 已完成定位校验但不改变内容；ChangeSet hunk 必须含删除或插入行，故不生成替换。
        if old_lines == new_lines {
            continue;
        }
        working.splice(start..start + old_lines.len(), new_lines.clone());
        cumulative_delta += new_lines.len() as isize - old_lines.len() as isize;
        replacements.push(ReplacementPlan {
            operation_id: format!("patch-{operation_index}-{hunk_index}"),
            start: original_start,
            end: original_end,
            old_lines,
            new_lines,
            match_mode: String::from("apply_patch"),
        });
    }
    sorted_non_overlapping_replacements(replacements)
}

/// Converts the vendored parser output into the host's prepared-operation representation.
/// 将 vendored 解析器输出转换为宿主的预备操作表示。
fn parse_apply_patch(patch: &str) -> Result<Vec<ParsedPatchFile>, FileToolError> {
    vulcan_apply_patch::parse_apply_patch(patch)
        .map_err(|error| FileToolError::new("patch_syntax_error", error.to_string()))?
        .into_iter()
        .map(|operation| {
            Ok(match operation {
                vulcan_apply_patch::PatchFile::Add { path, lines } => {
                    ParsedPatchFile::Add { path, lines }
                }
                vulcan_apply_patch::PatchFile::Delete { path } => ParsedPatchFile::Delete { path },
                vulcan_apply_patch::PatchFile::Update {
                    path,
                    move_to,
                    hunks,
                } => ParsedPatchFile::Update {
                    path,
                    move_to,
                    hunks: hunks
                        .into_iter()
                        .map(|hunk| {
                            let mut lines = hunk
                                .old_lines
                                .into_iter()
                                .map(|content| ParsedPatchLine {
                                    kind: PatchLineKind::Delete,
                                    content,
                                })
                                .collect::<Vec<_>>();
                            lines.extend(hunk.new_lines.into_iter().map(|content| {
                                ParsedPatchLine {
                                    kind: PatchLineKind::Add,
                                    content,
                                }
                            }));
                            ParsedPatchHunk {
                                context: hunk.context,
                                end_of_file: hunk.end_of_file,
                                lines,
                            }
                        })
                        .collect(),
                },
            })
        })
        .collect()
}

/// One parsed file operation in the Apply Patch language.
/// Apply Patch 语言中的一项已解析文件操作。
#[derive(Debug, Clone)]
enum ParsedPatchFile {
    /// Adds a file from logical lines.
    /// 从逻辑行新增文件。
    Add { path: String, lines: Vec<String> },
    /// Deletes an existing file.
    /// 删除已有文件。
    Delete { path: String },
    /// Updates and optionally moves an existing file.
    /// 更新并可选移动已有文件。
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<ParsedPatchHunk>,
    },
}

impl ParsedPatchFile {
    /// Returns the source or creation path.
    /// 返回源路径或创建路径。
    fn path(&self) -> &str {
        match self {
            Self::Add { path, .. } | Self::Delete { path } | Self::Update { path, .. } => path,
        }
    }
}

/// One parsed update hunk.
/// 一个已解析更新块。
#[derive(Debug, Clone, Default)]
struct ParsedPatchHunk {
    /// Optional exact logical-line anchor.
    /// 可选的精确逻辑行锚点。
    context: Option<String>,
    /// Whether the hunk must reach the file end.
    /// 更新块是否必须到达文件末尾。
    end_of_file: bool,
    /// Ordered semantic lines used to derive old and new sides.
    /// 用于派生新旧两侧的有序语义行。
    lines: Vec<ParsedPatchLine>,
}

/// One semantic line in an update hunk.
/// 更新块中的一条语义行。
#[derive(Debug, Clone)]
struct ParsedPatchLine {
    /// Semantic line kind.
    /// 语义行类型。
    kind: PatchLineKind,
    /// Logical content without a prefix.
    /// 不含前缀的逻辑内容。
    content: String,
}

/// Semantic line kinds required by host replacement planning.
/// 宿主替换规划需要的语义行类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchLineKind {
    /// Deleted source line.
    /// 删除的源行。
    Delete,
    /// Added destination line.
    /// 新增的目标行。
    Add,
}

/// Executes the hidden historical operation-array contract for persisted calls.
/// 为持久化调用执行隐藏的历史 operation 数组契约。
fn execute_legacy_file_edit_sync(
    workspace_root: &Path,
    arguments: &Value,
) -> Result<FileToolResponse, FileToolError> {
    let object = arguments.as_object().ok_or_else(|| {
        enrich_request_edit_error(FileToolError::new(
            "invalid_arguments",
            "The file_edit arguments must be an object.",
        ))
    })?;
    let request_bytes = serde_json::to_vec(arguments).map_err(|error| {
        enrich_request_edit_error(FileToolError::new(
            "invalid_arguments",
            format!("The file_edit request could not be serialized: {error}"),
        ))
    })?;
    if request_bytes.len() > MAX_EDIT_REQUEST_BYTES {
        return Err(enrich_request_edit_error(FileToolError::new(
            "output_limit_exceeded",
            format!("The file_edit request body may not exceed {MAX_EDIT_REQUEST_BYTES} bytes."),
        )));
    }
    validate_allowed_fields(object, &["dry_run", "operations"], "file_edit request")
        .map_err(enrich_request_edit_error)?;
    let dry_run = optional_bool(object, "dry_run", false).map_err(enrich_request_edit_error)?;
    let operations = object
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            enrich_request_edit_error(FileToolError::new(
                "invalid_arguments",
                "The file_edit request requires an operations array.",
            ))
        })?;
    if operations.is_empty() || operations.len() > MAX_EDIT_OPERATIONS {
        return Err(enrich_request_edit_error(FileToolError::new(
            "invalid_arguments",
            format!("The operations array must contain between 1 and {MAX_EDIT_OPERATIONS} items."),
        )));
    }
    let parsed = operations
        .iter()
        .enumerate()
        .map(|(index, value)| {
            parse_edit_operation(workspace_root, index, value)
                .map_err(|error| enrich_raw_edit_error(error, index, value))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut operation_ids = HashSet::new();
    for (index, operation) in parsed.iter().enumerate() {
        if !operation_ids.insert(operation.id.clone()) {
            return Err(enrich_parsed_edit_error(
                FileToolError::new(
                    "invalid_arguments",
                    format!("Operation id is duplicated: {}.", operation.id),
                ),
                &parsed,
                Some(std::slice::from_ref(&index)),
            ));
        }
    }
    let total_edit_lines = parsed
        .iter()
        .map(|operation| {
            operation.old_lines.len() + operation.new_lines.len() + operation.anchors.len()
        })
        .sum::<usize>();
    if total_edit_lines > MAX_EDIT_TOTAL_LINES {
        return Err(enrich_parsed_edit_error(
            FileToolError::new(
                "output_limit_exceeded",
                format!(
                    "A file_edit request may contain at most {MAX_EDIT_TOTAL_LINES} logical array lines."
                ),
            ),
            &parsed,
            None,
        ));
    }
    validate_operation_path_conflicts(&parsed)
        .map_err(|error| enrich_parsed_edit_error(error, &parsed, None))?;
    let mutation_paths = parsed
        .iter()
        .flat_map(|operation| {
            std::iter::once(operation.path.clone()).chain(operation.destination.iter().cloned())
        })
        .collect::<Vec<_>>();
    let resource_locks = acquire_resource_locks(&mutation_paths)
        .map_err(|error| enrich_parsed_edit_error(error, &parsed, None))?;
    let _resource_guards = lock_resource_set(&resource_locks)
        .map_err(|error| enrich_parsed_edit_error(error, &parsed, None))?;
    let mut snapshots = HashMap::<String, TextSnapshot>::new();
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    for (index, operation) in parsed.iter().enumerate() {
        let operation_path_key = path_key(&operation.path)
            .map_err(|error| enrich_parsed_edit_error(error, &parsed, Some(&[index])))?;
        groups
            .entry(operation_path_key.clone())
            .or_default()
            .push(index);
        if !matches!(operation.kind, EditKind::Create)
            && !snapshots.contains_key(&operation_path_key)
        {
            let snapshot = read_edit_snapshot(&operation.path, None).map_err(|error| {
                enrich_parsed_edit_error(error, &parsed, Some(std::slice::from_ref(&index)))
            })?;
            snapshots.insert(operation_path_key.clone(), snapshot);
        }
        if let Some(expected_version) = operation.expected_version.as_deref()
            && let Some(snapshot) = snapshots.get(&operation_path_key)
            && expected_version != snapshot.version
        {
            return Err(enrich_parsed_edit_error(
                FileToolError::new(
                    "stale_version",
                    format!(
                        "The expected version is stale for {}.",
                        display_path(&operation.path)
                    ),
                ),
                &parsed,
                Some(std::slice::from_ref(&index)),
            ));
        }
        if operation.kind == EditKind::Create && path_exists_including_symlink(&operation.path) {
            return Err(enrich_parsed_edit_error(
                FileToolError::new(
                    "path_already_exists",
                    format!(
                        "The create target already exists: {}",
                        display_path(&operation.path)
                    ),
                ),
                &parsed,
                Some(std::slice::from_ref(&index)),
            ));
        }
        if let Some(destination) = operation.destination.as_ref()
            && path_exists_including_symlink(destination)
        {
            return Err(enrich_parsed_edit_error(
                FileToolError::new(
                    "path_already_exists",
                    format!(
                        "The move destination already exists: {}",
                        display_path(destination)
                    ),
                ),
                &parsed,
                Some(std::slice::from_ref(&index)),
            ));
        }
        if operation.kind == EditKind::Create {
            validate_parent_directory(&operation.path).map_err(|error| {
                enrich_parsed_edit_error(error, &parsed, Some(std::slice::from_ref(&index)))
            })?;
        }
        if let Some(destination) = operation.destination.as_ref() {
            validate_parent_directory(destination).map_err(|error| {
                enrich_parsed_edit_error(error, &parsed, Some(std::slice::from_ref(&index)))
            })?;
        }
    }
    let mut changes = Vec::with_capacity(groups.len());
    // A sorted map keeps the multi-file operation-to-resource projection deterministic.
    // 使用有序映射保持多文件操作到资源的投影具有确定性。
    let mut operation_resource_ids = BTreeMap::<String, String>::new();
    for (resource_index, indexes) in groups.values().enumerate() {
        let resource_id = format!("resource-{}", resource_index + 1);
        let change = build_prepared_change(&parsed, indexes, &snapshots, &resource_id, dry_run)
            .map_err(|error| enrich_parsed_edit_error(error, &parsed, Some(indexes)))?;
        for operation_result in &change.operation_results {
            if let Some(operation_id) = operation_result.get("operation_id").and_then(Value::as_str)
            {
                operation_resource_ids.insert(operation_id.to_string(), resource_id.clone());
            }
        }
        changes.push(change);
    }
    // mode binds validation to the exact preview or applied result returned to the caller.
    // mode 将校验绑定到返回给调用方的精确预览或已应用结果。
    let mode = if dry_run { "preview" } else { "applied" };
    validate_prepared_change_set(mode, &changes, None)
        .map_err(|error| enrich_parsed_edit_error(error, &parsed, None))?;
    if !dry_run {
        verify_preconditions(&changes)
            .map_err(|error| enrich_parsed_edit_error(error, &parsed, None))?;
        commit_changes(&changes).map_err(|error| enrich_parsed_edit_error(error, &parsed, None))?;
    }
    let files = changes
        .iter()
        .map(|change| change.file_payload.clone())
        .collect::<Vec<_>>();
    let operation_results = changes
        .iter()
        .flat_map(|change| change.operation_results.iter().cloned())
        .collect::<Vec<_>>();
    let extension = json!({
        "operation_results": operation_results,
        "transaction": {
            "commit_scope": if dry_run { "none" } else { "all" },
            "rollback_state": "not_needed",
        },
    });
    let change_set = json!({
        "mode": mode,
        "summary": if dry_run { "File edit preview." } else { "File edit applied." },
        "files": files,
        "extensions": { "vulcan.file_edit.v1": extension },
    });
    Ok(FileToolResponse {
        model_output: json!({
            "ok": true,
            "mode": mode,
            "operation_count": parsed.len(),
            "file_count": changes.len(),
            "operations": operation_results,
            "resources": operation_resource_ids,
        }),
        read_metadata: None,
        change_set: Some(change_set),
    })
}

/// Adds operation context and recovery metadata to a parse-time edit error.
/// 为解析阶段的编辑错误补充操作上下文和恢复元数据。
fn enrich_raw_edit_error(
    error: FileToolError,
    operation_index: usize,
    value: &Value,
) -> FileToolError {
    let mut error = enrich_request_edit_error(error);
    let mut details = take_error_details(&mut error);
    details.insert(String::from("operation_index"), json!(operation_index));
    if let Some(object) = value.as_object() {
        match object.get("id").and_then(Value::as_str) {
            Some(operation_id) => {
                details.insert(
                    String::from("operation_id"),
                    Value::String(bounded_error_text(operation_id)),
                );
            }
            None => {
                details.insert(String::from("operation_id"), Value::Null);
            }
        }
        match object.get("path").and_then(Value::as_str) {
            Some(path) => {
                details.insert(
                    String::from("path"),
                    Value::String(bounded_error_text(path)),
                );
            }
            None => {
                details.insert(String::from("path"), Value::Null);
            }
        }
        if let Some(destination) = object.get("destination").and_then(Value::as_str) {
            details.insert(
                String::from("destination"),
                Value::String(bounded_error_text(destination)),
            );
        }
    } else {
        details.insert(String::from("operation_id"), Value::Null);
        details.insert(String::from("path"), Value::Null);
    }
    error.details = Value::Object(details);
    error
}

/// Adds operation context and recovery metadata to an error after parsing succeeded.
/// 为解析完成后的错误补充操作上下文和恢复元数据。
fn enrich_parsed_edit_error(
    error: FileToolError,
    operations: &[ParsedEditOperation],
    indexes: Option<&[usize]>,
) -> FileToolError {
    let mut error = enrich_request_edit_error(error);
    let selected_indexes = indexes
        .map(<[usize]>::to_vec)
        .unwrap_or_else(|| (0..operations.len()).collect::<Vec<_>>());
    let selected_indexes = selected_indexes
        .into_iter()
        .take(MAX_ERROR_OPERATION_CONTEXTS)
        .collect::<Vec<_>>();
    let contexts = selected_indexes
        .iter()
        .filter_map(|index| {
            operations
                .get(*index)
                .map(|operation| edit_operation_context(operation, *index))
        })
        .collect::<Vec<_>>();
    let mut details = take_error_details(&mut error);
    details.insert(String::from("operations"), Value::Array(contexts.clone()));

    if contexts.len() == 1 {
        if let Some(context) = contexts.first().and_then(Value::as_object) {
            for field in ["operation_id", "operation_index", "path"] {
                if let Some(value) = context.get(field) {
                    details.insert(String::from(field), value.clone());
                }
            }
            if let Some(destination) = context.get("destination") {
                details.insert(String::from("destination"), destination.clone());
            }
        }
    } else {
        let operation_ids = contexts
            .iter()
            .filter_map(|context| context.get("operation_id").cloned())
            .collect::<Vec<_>>();
        let paths = contexts
            .iter()
            .filter_map(|context| context.get("path").cloned())
            .collect::<Vec<_>>();
        details.insert(String::from("operation_id"), Value::Null);
        details.insert(String::from("operation_index"), Value::Null);
        details.insert(String::from("path"), Value::Null);
        details.insert(String::from("operation_ids"), Value::Array(operation_ids));
        details.insert(String::from("paths"), Value::Array(paths));
    }
    error.details = Value::Object(details);
    error
}

/// Adds the common diagnostic contract to a request-level edit error.
/// 为请求级编辑错误补充通用诊断契约。
fn enrich_request_edit_error(mut error: FileToolError) -> FileToolError {
    let mut details = take_error_details(&mut error);
    insert_error_contract_defaults(&mut details, &error.code);
    details
        .entry(String::from("operation_id"))
        .or_insert(Value::Null);
    details
        .entry(String::from("operation_index"))
        .or_insert(Value::Null);
    details.entry(String::from("path")).or_insert(Value::Null);
    error.details = Value::Object(details);
    error
}

/// Converts an operation into a bounded, content-free diagnostic context.
/// 将操作转换为有界且不包含文件正文的诊断上下文。
fn edit_operation_context(operation: &ParsedEditOperation, operation_index: usize) -> Value {
    let mut context = Map::new();
    context.insert(
        String::from("operation_id"),
        Value::String(bounded_error_text(&operation.id)),
    );
    context.insert(String::from("operation_index"), json!(operation_index));
    context.insert(
        String::from("operation"),
        Value::String(String::from(edit_kind_name(operation.kind))),
    );
    context.insert(
        String::from("path"),
        Value::String(bounded_error_text(&display_path(&operation.path))),
    );
    if let Some(destination) = operation.destination.as_ref() {
        context.insert(
            String::from("destination"),
            Value::String(bounded_error_text(&display_path(destination))),
        );
    }
    Value::Object(context)
}

/// Returns the stable schema name of one parsed edit kind.
/// 返回解析后编辑类型对应的稳定 Schema 名称。
fn edit_kind_name(kind: EditKind) -> &'static str {
    match kind {
        EditKind::Create => "create",
        EditKind::Replace => "replace",
        EditKind::Delete => "delete_file",
        EditKind::Move => "move_file",
    }
}

/// Takes existing object details without allowing an unstructured value to replace the contract.
/// 取出已有对象详情，避免非结构化值覆盖统一错误契约。
fn take_error_details(error: &mut FileToolError) -> Map<String, Value> {
    match std::mem::replace(&mut error.details, Value::Null) {
        Value::Object(details) => details,
        _ => Map::new(),
    }
}

/// Inserts the common bounded diagnostic fields required by native file_edit errors.
/// 写入原生 file_edit 错误必须具备的通用有界诊断字段。
fn insert_error_contract_defaults(details: &mut Map<String, Value>, code: &str) {
    let requires_read = edit_error_requires_reread(code);
    let recovery_hint = edit_error_recovery_hint(code);
    details
        .entry(String::from("commit_scope"))
        .or_insert_with(|| Value::String(String::from("none")));
    details
        .entry(String::from("rollback_state"))
        .or_insert_with(|| Value::String(String::from("not_started")));
    details
        .entry(String::from("requires_reread"))
        .or_insert_with(|| Value::Bool(requires_read));
    details
        .entry(String::from("requires_read"))
        .or_insert_with(|| Value::Bool(requires_read));
    details
        .entry(String::from("retryable"))
        .or_insert_with(|| Value::Bool(false));
    details
        .entry(String::from("recovery_hint"))
        .or_insert_with(|| Value::String(String::from(recovery_hint)));
    details
        .entry(String::from("hint"))
        .or_insert_with(|| Value::String(String::from(recovery_hint)));
}

/// Returns whether retrying this edit should begin with a fresh file read.
/// 返回重试该编辑前是否必须先重新读取文件。
fn edit_error_requires_reread(code: &str) -> bool {
    matches!(
        code,
        "path_already_exists"
            | "expected_lines_not_found"
            | "target_not_unique"
            | "anchor_chain_not_found"
            | "stale_version"
            | "stale_file"
            | "match_not_found"
            | "ambiguous_match"
            | "encoding_ambiguous"
            | "patch_context_not_found"
            | "write_failed"
            | "transaction_failed"
            | "disk_state_uncertain"
    )
}

/// Returns a direct English recovery hint for a model-visible edit error.
/// 返回面向模型的直接英文编辑错误恢复提示。
fn edit_error_recovery_hint(code: &str) -> &'static str {
    match code {
        "invalid_arguments" => {
            "Correct the current operation according to the schema; do not guess fields."
        }
        "path_not_found" => {
            "Check the path or use file with action=list on an existing parent directory before retrying."
        }
        "same_path" => "Choose a different destination, or skip the move.",
        "path_already_exists" => "Choose another path or explicitly handle the existing target.",
        "unsupported_file_type" => {
            "Check the target type with file stat. Use file list for directories; this action requires a supported regular file."
        }
        "expected_lines_not_found" => {
            "Use file_read and copy the complete current logical lines before retrying."
        }
        "target_not_unique" => {
            "Expand old_lines or add stable ordered anchors until exactly one target remains."
        }
        "match_not_found" => {
            "Read the current file and copy one exact distinctive old_string before retrying."
        }
        "ambiguous_match" => {
            "Read the current file and include more surrounding text, or set replace_all only when every occurrence must change."
        }
        "patch_syntax_error" => {
            "Correct the Apply Patch text using the attached grammar and resend the complete patch."
        }
        "patch_context_not_found" => {
            "Read every affected file again and regenerate the patch against the current exact text."
        }
        "anchor_chain_not_found" => {
            "Use file_read to verify that anchors occur before the target in the declared order."
        }
        "overlapping_operations" => {
            "Merge overlapping replacements into one operation against the original snapshot."
        }
        "stale_version" => "Use file stat or read to obtain a fresh version token before retrying.",
        "stale_file" => "Read all affected files again and regenerate the complete request.",
        "no_change" => {
            "Skip the operation or provide replacement text that differs from the current content."
        }
        "write_failed" => {
            "Fix the reported filesystem error, reread affected paths, and retry with a fresh request."
        }
        "transaction_failed" => {
            "The transaction was rolled back; reread every affected path before retrying."
        }
        "encoding_ambiguous" => {
            "Confirm the source encoding with file_read using encoding, then use file_edit with that encoding. For whole-file writes provide source_encoding; encoding selects the output encoding."
        }
        "binary_file" => {
            "If this is known text without a BOM (for example UTF-16LE/BE), read it with an explicit encoding. Do not retry unchanged arguments or edit a binary file as text."
        }
        "encoding_unrepresentable" => {
            "Use text representable in the current encoding, or explicitly convert the complete file."
        }
        "disk_state_uncertain" => {
            "Reread every affected path listed in the error before taking any further action."
        }
        "execution_failed" => {
            "Execution stopped without a confirmed outcome. Inspect affected paths before retrying; changes may already have been applied."
        }
        "invalid_change_set" => {
            "The host generated an invalid change receipt before committing file changes. Report this internal defect with its diagnostic; do not change tool arguments or repeat the operation to work around it."
        }
        _ => "Inspect the structured error and correct the request before retrying.",
    }
}

/// Bounds a diagnostic string without including file contents in ordinary errors.
/// 对诊断字符串设定上限，避免普通错误泄露或携带完整文件正文。
fn bounded_error_text(value: &str) -> String {
    value.chars().take(1024).collect()
}

/// Converts zero-based line ranges into bounded one-based diagnostic objects.
/// 将零基行范围转换为有界的一基诊断对象。
fn candidate_ranges_value(ranges: &[(usize, usize)]) -> Value {
    Value::Array(
        ranges
            .iter()
            .take(MAX_ERROR_CANDIDATE_RANGES)
            .map(|(start, end)| {
                json!({
                    "start_line": start.saturating_add(1),
                    "line_count": end.saturating_sub(*start),
                })
            })
            .collect(),
    )
}

/// Parses one operation and resolves its paths without touching the filesystem.
/// 解析一项操作并解析路径，但不触碰文件系统。
fn parse_edit_operation(
    workspace_root: &Path,
    index: usize,
    value: &Value,
) -> Result<ParsedEditOperation, FileToolError> {
    let object = value.as_object().ok_or_else(|| {
        FileToolError::new(
            "invalid_arguments",
            format!("Operation {index} must be an object."),
        )
    })?;
    let id = required_string(object, "id")?;
    if id.len() > 128 {
        return Err(FileToolError::new(
            "invalid_arguments",
            "Operation ids must be at most 128 bytes.",
        ));
    }
    let kind = match required_string(object, "operation")?.as_str() {
        "create" => EditKind::Create,
        "replace" => EditKind::Replace,
        "delete_file" => EditKind::Delete,
        "move_file" => EditKind::Move,
        _ => {
            return Err(FileToolError::new(
                "invalid_arguments",
                "Unsupported file_edit operation.",
            ));
        }
    };
    validate_edit_operation_fields(object, kind)?;
    let path = resolve_path(workspace_root, &required_string(object, "path")?)?;
    let destination = if kind == EditKind::Move {
        Some(resolve_path(
            workspace_root,
            &required_string(object, "destination")?,
        )?)
    } else {
        None
    };
    if let Some(destination) = destination.as_ref()
        && path_key(destination)? == path_key(&path)?
    {
        return Err(FileToolError::new(
            "invalid_arguments",
            "A move operation cannot target the same path.",
        ));
    }
    let old_lines = if kind == EditKind::Replace {
        parse_lines(object.get("old_lines"), "old_lines")?
    } else {
        Vec::new()
    };
    let new_lines = if matches!(kind, EditKind::Create | EditKind::Replace) {
        parse_lines(object.get("new_lines"), "new_lines")?
    } else {
        Vec::new()
    };
    let anchors = if kind == EditKind::Replace {
        parse_lines(object.get("anchors"), "anchors")?
    } else {
        Vec::new()
    };
    if kind == EditKind::Replace && old_lines.is_empty() && !anchors.is_empty() {
        return Err(FileToolError::new(
            "invalid_arguments",
            "anchors cannot be used when old_lines is empty; empty-file insertion has one fixed start.",
        ));
    }
    let match_mode = match object.get("match_mode") {
        Some(value) => value
            .as_str()
            .ok_or_else(|| FileToolError::new("invalid_arguments", "match_mode must be a string."))?
            .to_string(),
        None => String::from("exact"),
    };
    if !matches!(match_mode.as_str(), "exact" | "ignore_trailing_whitespace") {
        return Err(FileToolError::new(
            "invalid_arguments",
            "match_mode must be exact or ignore_trailing_whitespace.",
        ));
    }
    let final_newline = parse_final_newline(object.get("final_newline"))?;
    let expected_version = match object.get("expected_version") {
        Some(value) => Some(
            value
                .as_str()
                .ok_or_else(|| {
                    FileToolError::new("invalid_arguments", "expected_version must be a string.")
                })?
                .to_string(),
        ),
        None => None,
    };
    if expected_version.as_deref().is_some_and(str::is_empty) {
        return Err(FileToolError::new(
            "invalid_arguments",
            "expected_version must be a non-empty opaque version token.",
        ));
    }
    if matches!(kind, EditKind::Delete | EditKind::Move) && expected_version.is_none() {
        return Err(FileToolError::new(
            "invalid_arguments",
            "delete_file and move_file require expected_version from a prior file read or stat.",
        ));
    }
    Ok(ParsedEditOperation {
        id,
        kind,
        path,
        destination,
        old_lines,
        new_lines,
        anchors,
        match_mode,
        final_newline,
        expected_version,
    })
}

/// Builds one resource-level change from all operations targeting one source file.
/// 根据指向同一源文件的全部操作构建一条资源级变更。
fn build_prepared_change(
    operations: &[ParsedEditOperation],
    indexes: &[usize],
    snapshots: &HashMap<String, TextSnapshot>,
    resource_id: &str,
    dry_run: bool,
) -> Result<PreparedChange, FileToolError> {
    let first = operations
        .get(
            *indexes
                .first()
                .ok_or_else(|| FileToolError::new("invalid_arguments", "Empty resource group."))?,
        )
        .ok_or_else(|| {
            FileToolError::new("invalid_arguments", "Invalid operation resource group.")
        })?;
    let creates = indexes
        .iter()
        .filter(|index| operations[**index].kind == EditKind::Create)
        .count();
    let deletes = indexes
        .iter()
        .filter(|index| operations[**index].kind == EditKind::Delete)
        .count();
    let moves = indexes
        .iter()
        .filter(|index| operations[**index].kind == EditKind::Move)
        .count();
    if creates > 0 && indexes.len() != 1 || deletes > 0 && indexes.len() != 1 || moves > 1 {
        return Err(FileToolError::new(
            "overlapping_operations",
            format!(
                "Operations for {} cannot be chained as one resource.",
                display_path(&first.path)
            ),
        ));
    }
    if moves == 1 {
        let move_index = indexes
            .iter()
            .find(|index| operations[**index].kind == EditKind::Move)
            .copied()
            .unwrap_or_default();
        if indexes.iter().any(|index| *index > move_index) {
            return Err(FileToolError::new(
                "invalid_arguments",
                "move_file must be the last operation for one resource.",
            ));
        }
    }
    if creates == 1 {
        return build_create_change(
            &operations[*indexes.first().unwrap_or(&0)],
            resource_id,
            dry_run,
        );
    }
    let first_path_key = path_key(&first.path)?;
    let snapshot = snapshots.get(&first_path_key).ok_or_else(|| {
        FileToolError::new(
            "path_not_found",
            "The source file snapshot was not available.",
        )
    })?;
    if deletes == 1 {
        return build_delete_change(
            &operations[*indexes.first().unwrap_or(&0)],
            snapshot,
            resource_id,
            dry_run,
        );
    }
    let replacements = indexes
        .iter()
        .filter(|index| operations[**index].kind == EditKind::Replace)
        .map(|index| {
            let operation = &operations[*index];
            let range = find_unique_range(snapshot, operation)?;
            Ok(ReplacementPlan {
                operation_id: operation.id.clone(),
                start: range.0,
                end: range.1,
                old_lines: operation.old_lines.clone(),
                new_lines: operation.new_lines.clone(),
                match_mode: operation.match_mode.clone(),
            })
        })
        .collect::<Result<Vec<_>, FileToolError>>()?;
    let replacements = sorted_non_overlapping_replacements(replacements)?;
    let final_newline = indexes
        .iter()
        .filter_map(|index| operations[*index].final_newline)
        .next_back()
        .unwrap_or(snapshot.final_newline);
    let after_bytes = render_snapshot_replacements(snapshot, &replacements, final_newline)?;
    let move_operation = indexes
        .iter()
        .map(|index| &operations[*index])
        .find(|operation| operation.kind == EditKind::Move);
    if after_bytes == snapshot.bytes && move_operation.is_none() {
        return Err(FileToolError::new(
            "no_change",
            "The edit would not change the file.",
        ));
    }
    let change_kind = if move_operation.is_some() {
        "rename"
    } else {
        "modify"
    };
    let mut file_payload = Map::new();
    file_payload.insert(
        String::from("resource_id"),
        Value::String(resource_id.to_string()),
    );
    file_payload.insert(
        String::from("change"),
        Value::String(change_kind.to_string()),
    );
    file_payload.insert(
        String::from("path"),
        Value::String(display_path(
            move_operation
                .and_then(|value| value.destination.as_ref())
                .unwrap_or(&first.path),
        )),
    );
    if let Some(move_operation) = move_operation {
        file_payload.insert(
            String::from("old_path"),
            Value::String(display_path(&first.path)),
        );
        file_payload.insert(
            String::from("new_path"),
            Value::String(display_path(
                move_operation.destination.as_ref().ok_or_else(|| {
                    FileToolError::new("invalid_arguments", "Move destination is missing.")
                })?,
            )),
        );
    }
    let hunks = replacements
        .iter()
        .map(|replacement| replacement_hunk(snapshot, replacement, &replacements))
        .collect::<Vec<_>>();
    if change_kind == "rename" && hunks.is_empty() {
        insert_lifecycle_content(&mut file_payload, &snapshot.document.text, &snapshot.lines);
    } else {
        file_payload.insert(String::from("hunks"), Value::Array(hunks));
    }
    let mut operation_results = replacements
        .iter()
        .map(|replacement| operation_result(replacement, &replacements, dry_run, resource_id))
        .collect::<Vec<_>>();
    if let Some(move_operation) = move_operation {
        operation_results.push(json!({
            "operation_id": move_operation.id,
            "resource_id": resource_id,
            "status": if dry_run { "preview" } else { "applied" },
            "match_mode": "version",
            "original_range": Value::Null,
            "final_range": Value::Null,
        }));
    }
    Ok(PreparedChange {
        text_formats: None,
        change_kind: change_kind.to_string(),
        source_path: first.path.clone(),
        destination_path: move_operation.and_then(|value| value.destination.clone()),
        original_version: snapshot.version.clone(),
        before_bytes: Some(snapshot.bytes.clone()),
        original_permissions: Some(snapshot.permissions.clone()),
        after_bytes: Some(after_bytes),
        file_payload: Value::Object(file_payload),
        operation_results,
    })
}

/// Builds one create resource with its full new logical-line content.
/// 使用完整的新逻辑行内容构建一条创建资源。
fn build_create_change(
    operation: &ParsedEditOperation,
    resource_id: &str,
    dry_run: bool,
) -> Result<PreparedChange, FileToolError> {
    let final_newline = operation.final_newline.unwrap_or(true);
    let line_ending = String::from("\n");
    let after_bytes = render_text(&operation.new_lines, false, &line_ending, final_newline);
    let mut file_payload = Map::new();
    file_payload.insert(
        String::from("resource_id"),
        Value::String(resource_id.to_string()),
    );
    file_payload.insert(
        String::from("change"),
        Value::String(String::from("create")),
    );
    file_payload.insert(
        String::from("path"),
        Value::String(display_path(&operation.path)),
    );
    let content = String::from_utf8(after_bytes.clone()).map_err(|error| {
        FileToolError::new(
            "write_failed",
            format!("Generated UTF-8 create content was invalid: {error}"),
        )
    })?;
    insert_lifecycle_content(&mut file_payload, &content, &operation.new_lines);
    Ok(PreparedChange {
        text_formats: None,
        change_kind: String::from("create"),
        source_path: operation.path.clone(),
        destination_path: None,
        original_version: String::new(),
        before_bytes: None,
        original_permissions: None,
        after_bytes: Some(after_bytes),
        file_payload: Value::Object(file_payload),
        operation_results: vec![json!({
            "operation_id": operation.id,
            "resource_id": resource_id,
            "status": if dry_run { "preview" } else { "applied" },
            "match_mode": "create",
            "original_range": Value::Null,
            "final_range": {"start_line": 1, "line_count": operation.new_lines.len()},
        })],
    })
}

/// Builds one delete resource and keeps a bounded deleted-content summary.
/// 构建一条删除资源并保留有界的删除正文摘要。
fn build_delete_change(
    operation: &ParsedEditOperation,
    snapshot: &TextSnapshot,
    resource_id: &str,
    dry_run: bool,
) -> Result<PreparedChange, FileToolError> {
    let mut file_payload = Map::new();
    file_payload.insert(
        String::from("resource_id"),
        Value::String(resource_id.to_string()),
    );
    file_payload.insert(
        String::from("change"),
        Value::String(String::from("delete")),
    );
    file_payload.insert(
        String::from("path"),
        Value::String(display_path(&operation.path)),
    );
    insert_lifecycle_content(&mut file_payload, &snapshot.document.text, &snapshot.lines);
    Ok(PreparedChange {
        text_formats: None,
        change_kind: String::from("delete"),
        source_path: operation.path.clone(),
        destination_path: None,
        original_version: snapshot.version.clone(),
        before_bytes: Some(snapshot.bytes.clone()),
        original_permissions: Some(snapshot.permissions.clone()),
        after_bytes: None,
        file_payload: Value::Object(file_payload),
        operation_results: vec![json!({
            "operation_id": operation.id,
            "resource_id": resource_id,
            "status": if dry_run { "preview" } else { "applied" },
            "match_mode": "version",
            "original_range": {"start_line": 1, "line_count": snapshot.lines.len()},
            "final_range": Value::Null,
        })],
    })
}

/// Verifies every source version and every create/rename destination immediately before commit.
/// 在提交前立即复核所有源版本以及创建或重命名目标。
fn verify_preconditions(changes: &[PreparedChange]) -> Result<(), FileToolError> {
    let mut checked = HashSet::new();
    for change in changes {
        if checked.insert(path_key(&change.source_path)?) {
            if change.before_bytes.is_none() {
                if path_exists_including_symlink(&change.source_path) {
                    return Err(FileToolError::new(
                        "path_already_exists",
                        "The create target appeared before commit.",
                    ));
                }
            } else {
                let current = read_raw_snapshot(&change.source_path)?;
                if current.version != change.original_version {
                    return Err(FileToolError::new(
                        "stale_file",
                        format!(
                            "The file changed before commit: {}",
                            display_path(&change.source_path)
                        ),
                    ));
                }
            }
        }
        if let Some(destination) = change.destination_path.as_ref()
            && path_exists_including_symlink(destination)
        {
            return Err(FileToolError::new(
                "path_already_exists",
                format!(
                    "The destination appeared before commit: {}",
                    display_path(destination)
                ),
            ));
        }
    }
    Ok(())
}

/// Validates one prepared public ChangeSet with the canonical V1 parser.
/// 使用 canonical V1 解析器校验一组准备完成的公共 ChangeSet。
///
/// `mode` is the producer-declared ChangeSet mode and `changes` contains the exact
/// public file rows that will be returned after the operation. This function performs
/// no filesystem mutation and returns a stable internal-contract error when any row
/// violates the V1 lifecycle contract.
/// `mode` 是产出方声明的 ChangeSet 模式，`changes` 包含操作完成后将返回的精确
/// 公共文件记录。此函数不执行文件系统变更；任意记录违反 V1 生命周期契约时返回稳定的
/// 内部契约错误。
/// `extension` is the native v2 resource table; legacy producers omit it because they do not emit v2 metadata.
/// `extension` 是原生 v2 资源表；旧产出方不输出 v2 元数据，因此不传此参数。
fn validate_prepared_change_set(
    mode: &str,
    changes: &[PreparedChange],
    extension: Option<&Value>,
) -> Result<(), FileToolError> {
    // files are the unchanged public rows that the caller will receive after commit.
    // files 是提交后调用方将收到的未改写公共记录。
    let files = changes
        .iter()
        .map(|change| change.file_payload.clone())
        .collect::<Vec<_>>();
    // host_result recreates only the standard envelope required by the canonical parser.
    // host_result 仅重建规范解析器所需的标准外层信封。
    let host_result = json!({
        "kind": "change_set",
        "payload": {
            "mode": mode,
            "files": files,
            "extensions": extension.map(|value| json!({"vulcan.file_tools.v2": value})).unwrap_or_else(|| json!({})),
        },
    });
    match persistent_change_set_from_host_result(&host_result) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(FileToolError::new(
            "invalid_change_set",
            "The native file tool produced a result that was not recognized as a ChangeSet.",
        )),
        Err(error) => Err(FileToolError::new(
            "invalid_change_set",
            format!("The native file tool produced an invalid V1 ChangeSet: {error}"),
        )),
    }
}

/// Validates and commits prepared changes without exposing an invalid result after mutation.
/// 在文件变更落盘前完成校验，避免变更后才暴露无效结果。
///
/// `mode` and `changes` are first checked against the canonical V1 ChangeSet
/// contract, then filesystem preconditions are revalidated immediately before the
/// transactional commit. `source_tool` identifies the native producer; returns the validated v2 extension.
/// `mode` 与 `changes` 会先按 canonical V1 ChangeSet 契约校验，再于事务提交前
/// 重新核验文件系统前置条件。`source_tool` 标识原生产出方；返回已校验的 v2 扩展。
fn commit_validated_changes(
    mode: &str,
    changes: &[PreparedChange],
    source_tool: &str,
) -> Result<Value, FileToolError> {
    // Build the same operation and resource metadata that will cross the durability boundary.
    // 构建将跨越持久化边界的同一份操作与资源元数据。
    let operation_results = changes
        .iter()
        .flat_map(|change| change.operation_results.iter().cloned())
        .collect::<Vec<_>>();
    // Return this exact extension so post-commit assembly cannot reintroduce unvalidated resource identities.
    // 返回此精确扩展，避免提交后重新组装引入未经校验的资源标识。
    let extension = native_file_tools_extension(source_tool, changes, &operation_results, "all");
    validate_prepared_change_set(mode, changes, Some(&extension))?;
    verify_preconditions(changes)?;
    commit_changes(changes)?;
    Ok(extension)
}

/// Commits one validated mutation batch transactionally and rolls back partial writes.
/// 以事务方式提交一组已校验变更，并回滚部分写入。
///
/// `changes` must already satisfy the public ChangeSet contract and filesystem
/// preconditions. The function publishes every mutation or restores the captured
/// pre-operation state and returns one structured error.
/// `changes` 必须已经满足公共 ChangeSet 契约和文件系统前置条件。此函数要么发布
/// 全部变更，要么恢复捕获的操作前状态并返回一条结构化错误。
fn commit_changes(changes: &[PreparedChange]) -> Result<(), FileToolError> {
    let mut backups = BTreeMap::<PathBuf, FileBackup>::new();
    let mut published_new_paths = HashSet::<PathBuf>::new();
    let mut created_directories = BTreeSet::<PathBuf>::new();
    for change in changes {
        backups.insert(
            change.source_path.clone(),
            FileBackup {
                bytes: change.before_bytes.clone(),
                permissions: change.original_permissions.clone(),
            },
        );
        if let Some(destination) = change.destination_path.as_ref() {
            backups.insert(
                destination.clone(),
                FileBackup {
                    bytes: None,
                    permissions: None,
                },
            );
        }
    }
    for change in changes {
        let result = match change.change_kind.as_str() {
            "create" | "modify" => match change.after_bytes.as_deref() {
                Some(bytes) => {
                    let replace_existing = change.change_kind == "modify";
                    ensure_parent_directory(&change.source_path, &mut created_directories)
                        .and_then(|()| {
                            atomic_write(
                                &change.source_path,
                                bytes,
                                change.original_permissions.as_ref(),
                                replace_existing,
                            )
                        })
                        .map(|()| {
                            if !replace_existing {
                                published_new_paths.insert(change.source_path.clone());
                            }
                        })
                }
                None => Err(FileToolError::new(
                    "write_failed",
                    "The prepared file content is missing.",
                )),
            },
            "delete" => remove_file(&change.source_path),
            "rename" => {
                match (
                    change.destination_path.as_ref(),
                    change.after_bytes.as_deref(),
                ) {
                    (Some(destination), Some(bytes)) => {
                        let destination_result =
                            ensure_parent_directory(destination, &mut created_directories)
                                .and_then(|()| {
                                    atomic_write(
                                        destination,
                                        bytes,
                                        change.original_permissions.as_ref(),
                                        false,
                                    )
                                });
                        match destination_result {
                            Ok(()) => {
                                published_new_paths.insert(destination.clone());
                                remove_file(&change.source_path)
                            }
                            Err(error) => Err(error),
                        }
                    }
                    (None, _) => Err(FileToolError::new(
                        "write_failed",
                        "The prepared rename destination is missing.",
                    )),
                    (_, None) => Err(FileToolError::new(
                        "write_failed",
                        "The prepared rename content is missing.",
                    )),
                }
            }
            _ => Err(FileToolError::new(
                "write_failed",
                "Unknown prepared change kind.",
            )),
        };
        if let Err(error) = result {
            if let Err(rollback_error) =
                rollback_paths(&backups, &published_new_paths, &created_directories)
            {
                return Err(FileToolError::new(
                    "disk_state_uncertain",
                    format!("{} Rollback failed: {}", error.message, rollback_error.message),
                )
                .with_details(json!({
                    "commit_scope": "unknown",
                    "rollback_state": "failed",
                    "affected_paths": backups.keys().map(|path| display_path(path)).collect::<Vec<_>>(),
                    "cause": error.as_value(),
                    "rollback_error": rollback_error.as_value(),
                })));
            }
            return Err(FileToolError::new(
                "transaction_failed",
                format!(
                    "The file transaction failed and every captured path was restored: {}",
                    error.message
                ),
            )
            .with_details(json!({
                "commit_scope": "none",
                "rollback_state": "complete",
                "affected_paths": backups.keys().map(|path| display_path(path)).collect::<Vec<_>>(),
                "cause": error.as_value(),
            })));
        }
    }
    Ok(())
}

/// Restores captured paths in reverse lexical order after a failed commit.
/// 在提交失败后按逆序恢复已捕获路径。
fn rollback_paths(
    backups: &BTreeMap<PathBuf, FileBackup>,
    published_new_paths: &HashSet<PathBuf>,
    created_directories: &BTreeSet<PathBuf>,
) -> Result<(), FileToolError> {
    for (path, backup) in backups.iter().rev() {
        match backup.bytes.as_deref() {
            Some(bytes) => atomic_write(path, bytes, backup.permissions.as_ref(), true)?,
            None if published_new_paths.contains(path) => remove_file_if_exists(path)?,
            None => {}
        }
    }
    for path in created_directories.iter().rev() {
        match fs::remove_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("write_failed", path, error)),
        }
    }
    Ok(())
}

/// Captures the bytes and permission mode needed to restore one transaction path.
/// 保存单个事务路径恢复所需的字节和权限模式。
struct FileBackup {
    /// Original file bytes, or `None` when the path was absent before the transaction.
    /// 原始文件字节；如事务前路径不存在则为 `None`。
    bytes: Option<Vec<u8>>,
    /// Original permission mode when the path was a regular file.
    /// 原始普通文件的权限模式。
    permissions: Option<fs::Permissions>,
}

/// Writes one file through a same-directory temporary file and a final rename.
/// 通过同目录临时文件和最终重命名写入一份文件。
fn atomic_write(
    path: &Path,
    bytes: &[u8],
    requested_permissions: Option<&fs::Permissions>,
    replace_existing: bool,
) -> Result<(), FileToolError> {
    if let Some(parent) = path.parent() {
        let metadata =
            fs::metadata(parent).map_err(|error| io_error("path_not_found", parent, error))?;
        if !metadata.is_dir() {
            return Err(FileToolError::new(
                "path_not_found",
                format!(
                    "The parent path is not a directory: {}",
                    display_path(parent)
                ),
            ));
        }
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".vulcan-file-edit-")
        .tempfile_in(parent)
        .map_err(|error| io_error("write_failed", parent, error))?;
    let temporary_path = temporary.path().to_path_buf();
    temporary
        .write_all(bytes)
        .map_err(|error| io_error("write_failed", &temporary_path, error))?;
    let permissions = requested_permissions.cloned().or_else(|| {
        fs::metadata(path)
            .ok()
            .map(|metadata| metadata.permissions())
    });
    if let Some(permissions) = permissions {
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| io_error("write_failed", &temporary_path, error))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| io_error("write_failed", &temporary_path, error))?;
    let published = if replace_existing {
        temporary.persist(path)
    } else {
        temporary.persist_noclobber(path)
    };
    published
        .map(|_| ())
        .map_err(|error| io_error("write_failed", path, error.error))
}

/// Checks whether a path exists, including a dangling symbolic link.
/// 检查路径是否存在，并将断开的符号链接也视为存在。
fn path_exists_including_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Validates that a write target has a real directory ancestor and no file or link blocks its parent chain.
/// 验证写入目标具有真实目录祖先，且父路径链未被文件或链接阻断。
fn validate_parent_directory(path: &Path) -> Result<(), FileToolError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let mut current = parent.to_path_buf();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() => return Ok(()),
            Ok(_) => {
                return Err(FileToolError::new(
                    "path_not_found",
                    format!(
                        "The parent path is not a directory: {}",
                        display_path(&current)
                    ),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(next) = current.parent() else {
                    return Err(io_error("path_not_found", &current, error));
                };
                current = next.to_path_buf();
            }
            Err(error) => return Err(io_error("path_not_found", &current, error)),
        }
    }
}

/// Creates missing parent directories during the commit and records only directories created by this transaction.
/// 在提交阶段创建缺失父目录，并且只记录本事务实际创建的目录。
fn ensure_parent_directory(
    path: &Path,
    created_directories: &mut BTreeSet<PathBuf>,
) -> Result<(), FileToolError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let mut missing = Vec::new();
    let mut current = parent.to_path_buf();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() => break,
            Ok(_) => {
                return Err(FileToolError::new(
                    "path_not_found",
                    format!(
                        "The parent path is not a directory: {}",
                        display_path(&current)
                    ),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.clone());
                let Some(next) = current.parent() else {
                    return Err(io_error("path_not_found", &current, error));
                };
                current = next.to_path_buf();
            }
            Err(error) => return Err(io_error("path_not_found", &current, error)),
        }
    }
    for directory in missing.iter().rev() {
        match fs::create_dir(directory) {
            Ok(()) => {
                created_directories.insert(directory.clone());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(directory)
                    .map_err(|error| io_error("path_not_found", directory, error))?;
                if !metadata.is_dir() {
                    return Err(FileToolError::new(
                        "path_not_found",
                        format!(
                            "The parent path is not a directory: {}",
                            display_path(directory)
                        ),
                    ));
                }
            }
            Err(error) => return Err(io_error("write_failed", directory, error)),
        }
    }
    Ok(())
}

/// Removes one regular file and reports missing targets explicitly.
/// 删除一份普通文件，并显式报告目标不存在。
fn remove_file(path: &Path) -> Result<(), FileToolError> {
    fs::remove_file(path).map_err(|error| io_error("write_failed", path, error))
}

/// Removes a rollback target while treating an already absent path as restored state.
/// 删除回滚目标，并将目标已不存在视为已恢复状态。
fn remove_file_if_exists(path: &Path) -> Result<(), FileToolError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("write_failed", path, error)),
    }
}

/// Finds the unique old-line range selected by exact content and ordered anchors.
/// 根据精确旧内容和有序锚点寻找唯一目标范围。
fn find_unique_range(
    snapshot: &TextSnapshot,
    operation: &ParsedEditOperation,
) -> Result<(usize, usize), FileToolError> {
    if operation.old_lines.is_empty() {
        if snapshot.lines.is_empty() {
            return Ok((0, 0));
        }
        return Err(FileToolError::new(
            "expected_lines_not_found",
            "An empty old_lines array is only valid for an empty file.",
        ));
    }
    if operation.old_lines.len() > snapshot.lines.len() {
        return Err(FileToolError::new(
            "expected_lines_not_found",
            format!(
                "The old_lines block was not found for operation {}.",
                operation.id
            ),
        ));
    }
    let mut content_matches = 0usize;
    let mut content_candidates = Vec::new();
    let mut candidates = Vec::new();
    for start in 0..=snapshot
        .lines
        .len()
        .saturating_sub(operation.old_lines.len())
    {
        let end = start + operation.old_lines.len();
        if snapshot.lines[start..end]
            .iter()
            .zip(operation.old_lines.iter())
            .all(|(actual, expected)| line_matches(actual, expected, &operation.match_mode))
        {
            content_matches = content_matches.saturating_add(1);
            content_candidates.push((start, end));
            if anchors_match(&snapshot.lines, start, &operation.anchors) {
                candidates.push((start, end));
            }
        }
    }
    match candidates.as_slice() {
        [(start, end)] => Ok((*start, *end)),
        [] if content_matches > 0 && !operation.anchors.is_empty() => Err(FileToolError::new(
            "anchor_chain_not_found",
            format!(
                "The ordered anchor chain did not select the old_lines block for operation {}.",
                operation.id
            ),
        )
        .with_details(json!({
            "candidate_count": content_matches,
            "candidate_ranges": candidate_ranges_value(&content_candidates),
        }))),
        [] => Err(FileToolError::new(
            "expected_lines_not_found",
            format!(
                "The old_lines block was not found for operation {}.",
                operation.id
            ),
        )
        .with_details(json!({
            "candidate_count": 0,
            "candidate_ranges": [],
        }))),
        _ => Err(FileToolError::new(
            "target_not_unique",
            format!(
                "The old_lines block matched multiple locations for operation {}.",
                operation.id
            ),
        )
        .with_details(json!({
            "candidate_count": candidates.len(),
            "candidate_ranges": candidate_ranges_value(&candidates),
        }))),
    }
}

/// Sorts replacement ranges and rejects overlap on the immutable original snapshot.
/// 按不可变原始快照排序替换范围并拒绝重叠。
fn sorted_non_overlapping_replacements(
    mut replacements: Vec<ReplacementPlan>,
) -> Result<Vec<ReplacementPlan>, FileToolError> {
    replacements.sort_by_key(|replacement| replacement.start);
    for pair in replacements.windows(2) {
        if pair[0].end > pair[1].start {
            return Err(FileToolError::new(
                "overlapping_operations",
                "Replacement ranges overlap on the same original file snapshot.",
            )
            .with_details(json!({
                "candidate_count": 2,
                "candidate_ranges": candidate_ranges_value(&[
                    (pair[0].start, pair[0].end),
                    (pair[1].start, pair[1].end),
                ]),
                "operation_ids": [pair[0].operation_id.clone(), pair[1].operation_id.clone()],
            })));
        }
    }
    Ok(replacements)
}

/// Builds one host hunk from an original range and its replacement arrays.
/// 根据原始范围和替换数组构建一条宿主 hunk。
fn replacement_hunk(
    snapshot: &TextSnapshot,
    replacement: &ReplacementPlan,
    replacements: &[ReplacementPlan],
) -> Value {
    let actual_old = &snapshot.lines[replacement.start..replacement.end];
    let common_prefix = actual_old
        .iter()
        .zip(replacement.new_lines.iter())
        .take_while(|(old, new)| old == new)
        .count();
    let mut common_suffix = 0usize;
    while common_suffix < actual_old.len().saturating_sub(common_prefix)
        && common_suffix < replacement.new_lines.len().saturating_sub(common_prefix)
        && actual_old[actual_old.len() - common_suffix - 1]
            == replacement.new_lines[replacement.new_lines.len() - common_suffix - 1]
    {
        common_suffix += 1;
    }
    let old_change_start = replacement.start + common_prefix;
    let old_change_end = replacement.end.saturating_sub(common_suffix);
    let new_change_start = common_prefix;
    let new_change_end = replacement.new_lines.len().saturating_sub(common_suffix);
    let before_start = replacement.start.saturating_sub(2);
    let after_start = old_change_end;
    let after_end = replacement.end.saturating_add(2).min(snapshot.lines.len());
    let preceding_delta: isize = replacements
        .iter()
        .filter(|other| other.start < replacement.start)
        .map(|other| other.new_lines.len() as isize - other.old_lines.len() as isize)
        .sum();
    let final_start = (replacement.start as isize + preceding_delta).max(0) as usize;
    json!({
        "before": snapshot.lines[before_start..old_change_start].join("\n"),
        "delete": actual_old[common_prefix..old_change_end - replacement.start].iter().enumerate().map(|(index, content)| json!({"line": old_change_start + index + 1, "content": content})).collect::<Vec<_>>(),
        "insert": replacement.new_lines[new_change_start..new_change_end].iter().enumerate().map(|(index, content)| json!({"line": final_start + new_change_start + index + 1, "content": content})).collect::<Vec<_>>(),
        "after": snapshot.lines[after_start..after_end].join("\n"),
    })
}

/// Builds one operation-level correlation record with original and final ranges.
/// 构建一条带原始范围和最终范围的操作级关联记录。
fn operation_result(
    replacement: &ReplacementPlan,
    replacements: &[ReplacementPlan],
    dry_run: bool,
    resource_id: &str,
) -> Value {
    let preceding_delta: isize = replacements
        .iter()
        .filter(|other| other.start < replacement.start)
        .map(|other| other.new_lines.len() as isize - other.old_lines.len() as isize)
        .sum();
    let final_start = (replacement.start as isize + preceding_delta).max(0) as usize;
    json!({
        "operation_id": replacement.operation_id,
        "resource_id": resource_id,
        "status": if dry_run { "preview" } else { "applied" },
        "match_mode": replacement.match_mode,
        "original_range": {"start_line": replacement.start + 1, "line_count": replacement.old_lines.len()},
        "final_range": {"start_line": final_start + 1, "line_count": replacement.new_lines.len()},
    })
}

/// Inserts a bounded create/delete content summary into a canonical file row.
/// 将有界的创建/删除正文摘要写入 canonical 文件行。
fn insert_lifecycle_content(object: &mut Map<String, Value>, text: &str, lines: &[String]) {
    object.insert(String::from("total_line_count"), json!(lines.len()));
    if text.len() <= MAX_INLINE_LIFECYCLE_BYTES {
        object.insert(String::from("content"), Value::String(text.to_string()));
        object.insert(
            String::from("content_mode"),
            Value::String(String::from("full")),
        );
        return;
    }
    let head = text.chars().take(2000).collect::<String>();
    let tail = text
        .chars()
        .rev()
        .take(2000)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    object.insert(
        String::from("content_mode"),
        Value::String(String::from("truncated")),
    );
    object.insert(String::from("content_head"), Value::String(head));
    object.insert(String::from("content_tail"), Value::String(tail));
}

/// Builds a lifecycle change-set row from raw bytes without lossy text conversion.
/// 从原始字节构建生命周期 change-set 行且不进行有损文本转换。
/// `path`, `change_kind`, and `snapshot` describe the file; `resource_id` is allocated by the owning request.
/// `path`、`change_kind` 与 `snapshot` 描述文件；`resource_id` 由所属请求分配。
/// Returns one lifecycle row with the same identity as its operation and extension resource.
/// 返回与操作记录及扩展资源使用同一标识的生命周期行。
fn raw_lifecycle_payload(
    path: &Path,
    change_kind: &str,
    snapshot: &RawFileSnapshot,
    resource_id: &str,
) -> Value {
    let mut payload = Map::new();
    payload.insert(String::from("resource_id"), json!(resource_id));
    payload.insert(String::from("change"), json!(change_kind));
    payload.insert(String::from("path"), json!(display_path(path)));
    payload.insert(String::from("byte_count"), json!(snapshot.bytes.len()));
    match decode_text(&snapshot.bytes, None) {
        Ok(decoded) => {
            let document = TextDocument::from_decoded(decoded);
            let lines = document.logical_lines();
            insert_lifecycle_content(&mut payload, &document.text, &lines);
            payload.insert(String::from("encoding"), json!(document.encoding.label()));
            payload.insert(String::from("bom"), json!(document.bom.is_present()));
            payload.insert(
                String::from("line_ending"),
                json!(document.line_ending_label()),
            );
        }
        Err(_) => {
            payload.insert(String::from("binary"), Value::Bool(true));
            payload.insert(String::from("content_mode"), json!("omitted"));
        }
    }
    Value::Object(payload)
}

/// Returns whether an actual line satisfies the selected strict matching mode.
/// 判断实际行是否满足选定的严格匹配模式。
fn line_matches(actual: &str, expected: &str, mode: &str) -> bool {
    match mode {
        "ignore_trailing_whitespace" => actual.trim_end() == expected.trim_end(),
        _ => actual == expected,
    }
}

/// Requires every ordered anchor to occur before the selected old block.
/// 要求每个有序锚点都在选定旧块之前出现。
fn anchors_match(lines: &[String], target_start: usize, anchors: &[String]) -> bool {
    let mut cursor = 0;
    for anchor in anchors {
        let Some(relative) = lines[cursor..target_start]
            .iter()
            .position(|line| line == anchor)
        else {
            return false;
        };
        cursor += relative + 1;
    }
    true
}

/// Parses a logical-line string array and rejects embedded physical newlines.
/// 解析逻辑行字符串数组，并拒绝包含物理换行符的元素。
fn parse_lines(value: Option<&Value>, field: &str) -> Result<Vec<String>, FileToolError> {
    let Some(value) = value else {
        return if field == "anchors" {
            Ok(Vec::new())
        } else {
            Err(FileToolError::new(
                "invalid_arguments",
                format!("{field} is required."),
            ))
        };
    };
    let array = value.as_array().ok_or_else(|| {
        FileToolError::new(
            "invalid_arguments",
            format!("{field} must be an array of strings."),
        )
    })?;
    let maximum = if field == "anchors" {
        MAX_EDIT_ANCHORS
    } else {
        MAX_EDIT_LINES_PER_ARRAY
    };
    if array.len() > maximum {
        return Err(FileToolError::new(
            "output_limit_exceeded",
            format!("The {field} array may contain at most {maximum} logical lines."),
        ));
    }
    let mut lines = Vec::with_capacity(array.len());
    for line in array {
        let text = line.as_str().ok_or_else(|| {
            FileToolError::new(
                "invalid_arguments",
                format!("Every {field} item must be a string."),
            )
        })?;
        if text.contains('\n') || text.contains('\r') || text.len() > MAX_LINE_BYTES {
            return Err(FileToolError::new(
                "invalid_arguments",
                format!(
                    "Every {field} item must be one logical line below {MAX_LINE_BYTES} bytes."
                ),
            ));
        }
        lines.push(text.to_string());
    }
    Ok(lines)
}

/// Parses a final-newline policy into an optional concrete boolean.
/// 将末尾换行策略解析为可选的具体布尔值。
fn parse_final_newline(value: Option<&Value>) -> Result<Option<bool>, FileToolError> {
    let value = match value {
        Some(value) => value.as_str().ok_or_else(|| {
            FileToolError::new("invalid_arguments", "final_newline must be a string.")
        })?,
        None => "preserve",
    };
    match value {
        "preserve" => Ok(None),
        "present" => Ok(Some(true)),
        "absent" => Ok(Some(false)),
        _ => Err(FileToolError::new(
            "invalid_arguments",
            "final_newline must be preserve, present, or absent.",
        )),
    }
}

/// Parses read segments as one-based start line and positive line count pairs.
/// 将读取分段解析为一基起始行和正数行数对。
fn parse_segments(value: Option<&Value>) -> Result<Vec<(usize, usize)>, FileToolError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| FileToolError::new("invalid_arguments", "segments must be an array."))?;
    if array.len() > MAX_READ_SEGMENTS {
        return Err(FileToolError::new(
            "output_limit_exceeded",
            format!("A read target may contain at most {MAX_READ_SEGMENTS} segments."),
        ));
    }
    let mut segments = Vec::with_capacity(array.len());
    for item in array {
        let object = item.as_object().ok_or_else(|| {
            FileToolError::new("invalid_arguments", "Each segment must be an object.")
        })?;
        validate_allowed_fields(object, &["start_line", "line_count"], "read segment")?;
        let start = required_positive_usize(object, "start_line")?;
        let count = required_positive_usize(object, "line_count")?;
        if count > MAX_READ_LINES {
            return Err(FileToolError::new(
                "output_limit_exceeded",
                format!("A read segment may request at most {MAX_READ_LINES} lines."),
            ));
        }
        segments.push((start, count));
    }
    Ok(segments)
}

/// Reads an optional boolean without silently accepting a wrong JSON type.
/// 读取可选布尔值，避免错误 JSON 类型被静默当成默认值。
fn optional_bool(
    object: &Map<String, Value>,
    field: &str,
    default: bool,
) -> Result<bool, FileToolError> {
    match object.get(field) {
        None => Ok(default),
        Some(value) => value.as_bool().ok_or_else(|| {
            FileToolError::new("invalid_arguments", format!("{field} must be a boolean."))
        }),
    }
}

/// Reads an optional string while preserving an explicit empty string.
/// 读取可选字符串，并保留调用方显式传入的空字符串。
fn optional_string(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<String>, FileToolError> {
    match object.get(field) {
        None => Ok(None),
        Some(value) => value.as_str().map(String::from).map(Some).ok_or_else(|| {
            FileToolError::new("invalid_arguments", format!("{field} must be a string."))
        }),
    }
}

/// Reads a bounded unsigned integer and rejects values outside the public contract.
/// 读取有界无符号整数，并拒绝超出公开契约范围的值。
fn optional_limit(
    object: &Map<String, Value>,
    field: &str,
    default: usize,
    minimum: usize,
    maximum: usize,
) -> Result<usize, FileToolError> {
    let Some(value) = object.get(field) else {
        return Ok(default);
    };
    let value = value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            FileToolError::new(
                "invalid_arguments",
                format!("{field} must be a non-negative integer."),
            )
        })?;
    if value < minimum || value > maximum {
        return Err(FileToolError::new(
            "invalid_arguments",
            format!("{field} must be between {minimum} and {maximum}."),
        ));
    }
    Ok(value)
}

/// Reads a required positive unsigned integer field.
/// 读取必需的正无符号整数值字段。
fn required_positive_usize(
    object: &Map<String, Value>,
    field: &str,
) -> Result<usize, FileToolError> {
    let value = object
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            FileToolError::new(
                "invalid_arguments",
                format!("{field} must be a positive integer."),
            )
        })?;
    if value == 0 {
        return Err(FileToolError::new(
            "invalid_arguments",
            format!("{field} must be positive."),
        ));
    }
    Ok(value)
}

/// Validates the bounded text size of a search query or filename pattern.
/// 校验搜索查询或文件名模式的有界文本长度。
fn validate_search_text(value: Option<&str>, field: &str) -> Result<(), FileToolError> {
    if let Some(value) = value
        && value.len() > MAX_SEARCH_QUERY_BYTES
    {
        return Err(FileToolError::new(
            "output_limit_exceeded",
            format!("The {field} may contain at most {MAX_SEARCH_QUERY_BYTES} UTF-8 bytes."),
        ));
    }
    Ok(())
}

/// Returns one required non-empty string field.
/// 返回一个必填的非空字符串字段，并保留其字面量空白。
fn required_string(object: &Map<String, Value>, field: &str) -> Result<String, FileToolError> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            FileToolError::new(
                "invalid_arguments",
                format!("{field} must be a non-empty string."),
            )
        })?;
    Ok(value.to_string())
}

/// Returns one required string field while allowing intentionally empty text content.
/// 返回一个必填字符串字段，同时允许有意提供的空文本内容。
fn required_text(object: &Map<String, Value>, field: &str) -> Result<String, FileToolError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            FileToolError::new(
                "invalid_arguments",
                format!("{field} is required and must be a string."),
            )
        })
}

/// Resolves a local path and validates workspace containment only when the path belongs to it.
/// 解析本地路径，并且仅在路径归属于工作区时校验工作区包含关系。
fn resolve_path(workspace_root: &Path, raw_path: &str) -> Result<PathBuf, FileToolError> {
    let (workspace_path, path) = resolve_lexical_path(workspace_root, raw_path)?;
    // Workspace containment protects only paths that actually belong to the workspace. External
    // absolute or parent-relative paths are ordinary local paths; the permission gate decides
    // whether writing them needs approval.
    // 工作区包含校验仅保护实际属于工作区的路径；外部绝对或父级相对路径按普通本地路径处理，
    // 是否需要批准写入由权限闸门决定。
    if path.starts_with(&workspace_path) {
        ensure_path_within_workspace(&workspace_path, &path)?;
    }
    Ok(path)
}

/// Expands placeholders and lexically normalizes one tool path against the workspace root.
/// 展开占位符，并相对工作区根目录对一个工具路径做词法规范化。
/// Returns `(absolute workspace root, absolute candidate path)`; nothing needs to exist.
/// 返回 `(绝对工作区根目录, 绝对候选路径)`；两者都不要求存在。
fn resolve_lexical_path(
    workspace_root: &Path,
    raw_path: &str,
) -> Result<(PathBuf, PathBuf), FileToolError> {
    let expanded_path = expand_environment_path(raw_path)?;
    let candidate = PathBuf::from(expanded_path);
    let workspace_path = if workspace_root.is_absolute() {
        lexical_normalize(workspace_root)
    } else {
        lexical_normalize(
            &std::env::current_dir()
                .map_err(|error| io_error("path_not_found", workspace_root, error))?
                .join(workspace_root),
        )
    };
    let path = if candidate.is_absolute() {
        candidate
    } else {
        workspace_path.join(candidate)
    };
    Ok((workspace_path, lexical_normalize(&path)))
}

/// Reports whether one raw tool path resolves inside the workspace after placeholder expansion,
/// lexical normalization, and canonicalization of its nearest existing ancestor.
/// 报告一个原始工具路径在展开占位符、词法规范化并规范化最近已存在祖先后是否位于工作区内。
/// Symbolic links and junctions are followed, so an in-workspace alias of an outside directory
/// is outside. Errors mean the path cannot be resolved; callers must treat them as outside.
/// 会跟随符号链接与目录联接，因此指向外部目录的工作区内别名视为外部；错误表示路径无法解析，
/// 调用方必须按外部处理。
pub fn path_resolves_inside_workspace(
    workspace_root: &Path,
    raw_path: &str,
) -> Result<bool, FileToolError> {
    let (workspace_path, path) = resolve_lexical_path(workspace_root, raw_path)?;
    let canonical_workspace = fs::canonicalize(&workspace_path)
        .map_err(|error| io_error("path_not_found", &workspace_path, error))?;
    let Some(existing_ancestor) = nearest_existing_ancestor(&path)? else {
        return Ok(false);
    };
    let canonical_ancestor = fs::canonicalize(&existing_ancestor)
        .map_err(|error| io_error("path_not_found", &existing_ancestor, error))?;
    Ok(canonical_ancestor.starts_with(&canonical_workspace))
}

/// Reports whether one raw tool path addresses Git metadata: a `.git` component below the
/// workspace root, at any depth. Git runs commands named in that metadata (`core.fsmonitor`,
/// hooks, diff drivers), so a write there is never a plain workspace edit.
/// 报告一个原始工具路径是否指向 Git 元数据：工作区根目录之下任意深度的 `.git` 组件。Git 会运行
/// 这些元数据中指定的命令（`core.fsmonitor`、钩子、差异驱动），因此写入那里绝不是普通的工作区编辑。
/// The existing part is canonicalized, so case, Windows short names and trailing dots or spaces
/// cannot hide the component. Errors mean the path cannot be resolved; callers must treat them as
/// metadata.
/// 已存在部分会被规范化，因此大小写、Windows 短文件名与结尾的点或空格无法隐藏该组件。错误表示路径
/// 无法解析，调用方必须按元数据处理。
pub fn path_targets_git_metadata(
    workspace_root: &Path,
    raw_path: &str,
) -> Result<bool, FileToolError> {
    let (workspace_path, path) = resolve_lexical_path(workspace_root, raw_path)?;
    let canonical_workspace = fs::canonicalize(&workspace_path)
        .map_err(|error| io_error("path_not_found", &workspace_path, error))?;
    let Some(existing_ancestor) = nearest_existing_ancestor(&path)? else {
        return Ok(false);
    };
    let canonical_ancestor = fs::canonicalize(&existing_ancestor)
        .map_err(|error| io_error("path_not_found", &existing_ancestor, error))?;
    let existing = canonical_ancestor
        .strip_prefix(&canonical_workspace)
        .unwrap_or(&canonical_ancestor);
    let missing = path.strip_prefix(&existing_ancestor).unwrap_or(&path);
    Ok(existing
        .components()
        .chain(missing.components())
        .any(|component| match component {
            Component::Normal(name) => name
                .to_string_lossy()
                .trim_end_matches(['.', ' '])
                .eq_ignore_ascii_case(".git"),
            _ => false,
        }))
}

/// Lists every path an `apply_patch` body would write, including move destinations.
/// 列出 `apply_patch` 正文将写入的全部路径，包括移动目标。
/// Paths are returned raw, as written in the patch; syntax errors are returned unchanged.
/// 路径按补丁原文返回；语法错误原样返回。
pub fn apply_patch_write_paths(patch: &str) -> Result<Vec<String>, FileToolError> {
    let mut paths = Vec::new();
    for operation in parse_apply_patch(patch)? {
        match operation {
            ParsedPatchFile::Add { path, .. } | ParsedPatchFile::Delete { path } => {
                paths.push(path);
            }
            ParsedPatchFile::Update { path, move_to, .. } => {
                paths.push(path);
                paths.extend(move_to);
            }
        }
    }
    Ok(paths)
}

/// Verifies the nearest existing path resolves under the canonical workspace root.
/// 验证最近的已存在路径在规范化后仍位于工作区根目录内。
fn ensure_path_within_workspace(
    workspace_root: &Path,
    candidate: &Path,
) -> Result<(), FileToolError> {
    let canonical_workspace = fs::canonicalize(workspace_root)
        .map_err(|error| io_error("path_not_found", workspace_root, error))?;
    let Some(existing_ancestor) = nearest_existing_ancestor(candidate)? else {
        return Err(FileToolError::new(
            "path_outside_workspace",
            "A path addressed through the current workspace must resolve inside that workspace.",
        ));
    };
    let canonical_ancestor = fs::canonicalize(&existing_ancestor)
        .map_err(|error| io_error("path_not_found", &existing_ancestor, error))?;
    if !canonical_ancestor.starts_with(&canonical_workspace) {
        return Err(FileToolError::new(
            "path_outside_workspace",
            "A path addressed through the current workspace resolves outside that workspace.",
        ));
    }
    Ok(())
}

/// Returns the nearest ancestor of `candidate` (itself included) that exists, without following it.
/// 返回 `candidate`（含自身）最近的已存在祖先，不跟随该路径。
/// `None` means no component exists, which only happens for unrooted or unreachable paths.
/// `None` 表示没有任何组件存在，仅发生于无根或不可达路径。
fn nearest_existing_ancestor(candidate: &Path) -> Result<Option<PathBuf>, FileToolError> {
    let mut existing_ancestor = candidate.to_path_buf();
    loop {
        match fs::symlink_metadata(&existing_ancestor) {
            Ok(_) => return Ok(Some(existing_ancestor)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !existing_ancestor.pop() {
                    return Ok(None);
                }
            }
            Err(error) => {
                return Err(io_error("path_not_found", &existing_ancestor, error));
            }
        }
    }
}

/// Expands the established `${env:NAME}` path placeholders exactly once.
/// 按既有约定对路径中的 `${env:NAME}` 占位符执行一次展开。
fn expand_environment_path(raw_path: &str) -> Result<String, FileToolError> {
    const ENVIRONMENT_MARKER: &str = "${env:";

    let mut expanded = String::with_capacity(raw_path.len());
    let mut cursor = 0;
    while let Some(relative_start) = raw_path[cursor..].find(ENVIRONMENT_MARKER) {
        let start = cursor + relative_start;
        expanded.push_str(&raw_path[cursor..start]);

        let name_start = start + ENVIRONMENT_MARKER.len();
        let Some(relative_end) = raw_path[name_start..].find('}') else {
            return Err(FileToolError::new(
                "invalid_environment_variable_reference",
                "Environment variable path references must end with `}`.",
            ));
        };
        let end = name_start + relative_end;
        let name = raw_path[name_start..end].trim();
        if name.is_empty() {
            return Err(FileToolError::new(
                "invalid_environment_variable_reference",
                "Environment variable path references must include a variable name.",
            ));
        }

        let value = std::env::var(name).map_err(|_| {
            FileToolError::new(
                "environment_variable_not_found",
                format!("Environment variable `{name}` referenced in path is not defined."),
            )
        })?;
        expanded.push_str(&value);
        cursor = end + 1;
    }
    expanded.push_str(&raw_path[cursor..]);

    if expanded.contains(ENVIRONMENT_MARKER) {
        return Err(FileToolError::new(
            "invalid_environment_variable_reference",
            "Environment variable path references must use the `${env:NAME}` form.",
        ));
    }

    let expanded = expanded.trim();
    if expanded.is_empty() {
        return Err(FileToolError::new(
            "invalid_arguments",
            "Path resolves to an empty string.",
        ));
    }
    Ok(expanded.to_string())
}

/// Normalizes dot components without requiring the target to exist.
/// 不要求目标存在即可规范化点号路径组件。
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(output.components().next_back(), Some(Component::Normal(_))) {
                    output.pop();
                }
            }
            other => output.push(other.as_os_str()),
        }
    }
    output
}

/// Returns a canonical resource key through the nearest existing path ancestor.
/// 通过最近的已存在路径祖先返回规范资源键。
///
/// # Parameters
/// 参数
/// `path` is an already workspace-authorized lexical path that may not exist yet.
/// `path` 是已经过工作区授权、但可能尚不存在的词法路径。
///
/// # Returns
/// 返回值
/// A canonical identity that collapses in-workspace symbolic-link aliases.
/// 返回可折叠工作区内符号链接别名的规范身份。
fn path_key(path: &Path) -> Result<String, FileToolError> {
    let mut ancestor = path.to_path_buf();
    let mut suffix = Vec::new();
    let canonical_ancestor = loop {
        match fs::canonicalize(&ancestor) {
            Ok(canonical) => break canonical,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let component = ancestor.file_name().ok_or_else(|| {
                    FileToolError::new(
                        "path_not_found",
                        format!(
                            "No existing ancestor could identify the resource path: {}",
                            display_path(path)
                        ),
                    )
                })?;
                suffix.push(component.to_os_string());
                if !ancestor.pop() {
                    return Err(io_error("path_not_found", path, error));
                }
            }
            Err(error) => return Err(io_error("path_not_found", &ancestor, error)),
        }
    };
    let mut identity = canonical_ancestor;
    for component in suffix.iter().rev() {
        identity.push(component);
    }
    let value = display_path(&identity);
    #[cfg(windows)]
    {
        Ok(value.to_ascii_lowercase())
    }
    #[cfg(not(windows))]
    {
        Ok(value)
    }
}

/// Resolves stable per-path mutation locks in lexical order to prevent deadlocks.
/// 按词法顺序解析稳定的逐路径变更锁以避免死锁。
fn acquire_resource_locks(paths: &[PathBuf]) -> Result<Vec<Arc<Mutex<()>>>, FileToolError> {
    let mut keys = paths
        .iter()
        .map(|path| path_key(path))
        .collect::<Result<Vec<_>, _>>()?;
    keys.sort();
    keys.dedup();
    let registry = FILE_RESOURCE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry.lock().map_err(|_| {
        FileToolError::new(
            "execution_failed",
            "The native file resource-lock registry is poisoned.",
        )
    })?;
    registry.retain(|_, weak| weak.strong_count() > 0);
    Ok(keys
        .into_iter()
        .map(|key| {
            if let Some(lock) = registry.get(&key).and_then(Weak::upgrade) {
                return lock;
            }
            let lock = Arc::new(Mutex::new(()));
            registry.insert(key, Arc::downgrade(&lock));
            lock
        })
        .collect())
}

/// Acquires an already sorted resource-lock set for one complete transaction.
/// 为一个完整事务获取已排序的资源锁集合。
fn lock_resource_set<'a>(
    locks: &'a [Arc<Mutex<()>>],
) -> Result<Vec<MutexGuard<'a, ()>>, FileToolError> {
    locks
        .iter()
        .map(|lock| {
            lock.lock().map_err(|_| {
                FileToolError::new(
                    "execution_failed",
                    "A native file resource lock is poisoned.",
                )
            })
        })
        .collect()
}

/// Resolves a stable editor language hint from one file name without inspecting its contents.
/// 仅根据文件名解析稳定的编辑器语言提示，不检查文件内容。
fn language_for_path(path: &Path) -> &'static str {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(file_name.as_str(), "dockerfile" | "containerfile") {
        return "dockerfile";
    }
    if matches!(file_name.as_str(), "makefile" | "gnumakefile") {
        return "makefile";
    }
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "cs" => "csharp",
        "css" => "css",
        "go" => "go",
        "htm" | "html" => "html",
        "java" => "java",
        "js" | "mjs" | "cjs" => "javascript",
        "json" | "jsonc" => "json",
        "jsx" => "jsx",
        "lua" => "lua",
        "md" | "mdx" => "markdown",
        "php" => "php",
        "ps1" | "psm1" => "powershell",
        "py" => "python",
        "rb" => "ruby",
        "rs" => "rust",
        "sh" | "bash" | "zsh" => "shellscript",
        "sql" => "sql",
        "swift" => "swift",
        "toml" => "toml",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "xml" => "xml",
        "yaml" | "yml" => "yaml",
        _ => "text",
    }
}

/// Converts one path into the stable slash-separated display form.
/// 将路径转换为稳定的斜杠分隔展示形式。
fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Returns a bounded regular-expression or literal search matcher.
/// 返回一个有界的正则或字面量搜索匹配器。
enum SearchMatcher {
    /// Case-sensitive or insensitive literal matcher.
    /// 区分或不区分大小写的字面量匹配器。
    Literal {
        /// Literal query text.
        /// 字面量查询文本。
        query: String,
        /// Whether matching preserves case.
        /// 匹配是否区分大小写。
        case_sensitive: bool,
    },
    /// Compiled regular-expression matcher.
    /// 已编译的正则表达式匹配器。
    Regex(Regex),
}

impl SearchMatcher {
    /// Compiles one matcher from the public search fields.
    /// 根据公开搜索字段编译一个匹配器。
    fn new(query: &str, query_type: &str, case_sensitive: bool) -> Result<Self, FileToolError> {
        match query_type {
            "literal" => Ok(Self::Literal {
                query: query.to_string(),
                case_sensitive,
            }),
            "regex" => {
                let pattern = if case_sensitive {
                    query.to_string()
                } else {
                    format!("(?i:{query})")
                };
                Regex::new(&pattern).map(Self::Regex).map_err(|error| {
                    FileToolError::new(
                        "invalid_arguments",
                        format!("Invalid search regex: {error}"),
                    )
                })
            }
            _ => Err(FileToolError::new(
                "invalid_arguments",
                "query_type must be literal or regex.",
            )),
        }
    }

    /// Tests one line against the compiled matcher.
    /// 使用已编译匹配器测试一条逻辑行。
    fn matches(&self, line: &str) -> bool {
        match self {
            Self::Literal { query, .. } if query.is_empty() => true,
            Self::Literal {
                query,
                case_sensitive: true,
            } => line.contains(query),
            Self::Literal {
                query,
                case_sensitive: false,
            } => line.to_lowercase().contains(&query.to_lowercase()),
            Self::Regex(regex) => regex.is_match(line),
        }
    }
}

/// One parsed edit operation with resolved filesystem paths.
/// 一项已解析且完成文件系统路径解析的编辑操作。
#[derive(Debug, Clone)]
struct ParsedEditOperation {
    /// Call-local operation identifier.
    /// 调用内操作标识。
    id: String,
    /// Parsed operation kind.
    /// 已解析的操作类型。
    kind: EditKind,
    /// Source or target path.
    /// 源文件或目标文件路径。
    path: PathBuf,
    /// Move destination when this is a move operation.
    /// 移动操作的目标路径。
    destination: Option<PathBuf>,
    /// Expected original logical lines for replacement.
    /// 替换要求存在的原始逻辑行。
    old_lines: Vec<String>,
    /// Replacement or creation logical lines.
    /// 替换或创建使用的新逻辑行。
    new_lines: Vec<String>,
    /// Ordered literal anchors before the old block.
    /// 旧块之前的有序字面量锚点。
    anchors: Vec<String>,
    /// Strict line matching mode.
    /// 严格逻辑行匹配模式。
    match_mode: String,
    /// Optional final-newline override.
    /// 可选的末尾换行覆盖策略。
    final_newline: Option<bool>,
    /// Optional expected version token.
    /// 可选的期望版本令牌。
    expected_version: Option<String>,
}

/// Supported structured edit operation kinds.
/// 支持的结构化编辑操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    /// Create one new text file.
    /// 创建一份新文本文件。
    Create,
    /// Replace one or more non-overlapping old-line blocks.
    /// 替换一个或多个不重叠的旧逻辑行块。
    Replace,
    /// Delete one regular file.
    /// 删除一份普通文件。
    Delete,
    /// Move or rename one regular file.
    /// 移动或重命名一份普通文件。
    Move,
}

/// Authoritative text formats frozen by decoded mutation preflight.
/// 由解码变更预检冻结的权威文本格式。
struct PreparedTextFormats {
    /// Original format, or null for a newly created resource.
    /// 原始格式；新建资源为 null。
    before: Value,
    /// Committed format, or null if the resource is removed.
    /// 提交格式；资源被删除时为 null。
    after: Value,
}

/// One prepared resource-level mutation ready for dry-run display or commit.
/// 一条可用于 dry-run 展示或提交的资源级准备变更。
struct PreparedChange {
    /// Known formats for decoded edits; raw lifecycle operations derive optional metadata from their bytes.
    /// 解码编辑的已知格式；原始字节生命周期操作从字节派生可选元数据。
    text_formats: Option<PreparedTextFormats>,
    /// Canonical file-level change kind.
    /// canonical 文件级变更类型。
    change_kind: String,
    /// Source or primary path.
    /// 源路径或主路径。
    source_path: PathBuf,
    /// Rename destination path.
    /// 重命名目标路径。
    destination_path: Option<PathBuf>,
    /// Version token captured before planning.
    /// 规划前捕获的版本令牌。
    original_version: String,
    /// Original bytes used for rollback and concurrency checks.
    /// 用于回滚和竞态检查的原始字节。
    before_bytes: Option<Vec<u8>>,
    /// Original permission bits reused when a deleted or moved file is restored or recreated.
    /// 删除或移动文件恢复或重新创建时保留的原始权限模式。
    original_permissions: Option<fs::Permissions>,
    /// Final bytes to publish, when the resource remains present.
    /// 资源最终保留时需要发布的字节。
    after_bytes: Option<Vec<u8>>,
    /// Canonical public file row.
    /// canonical 公共文件行。
    file_payload: Value,
    /// Native operation correlation rows.
    /// 原生操作关联行集合。
    operation_results: Vec<Value>,
}

/// One text snapshot preserving the data needed for safe line edits.
/// 保存安全行编辑所需数据的一份文本快照。
#[derive(Debug, Clone)]
struct TextSnapshot {
    /// Original bytes.
    /// 原始字节。
    bytes: Vec<u8>,
    /// Original filesystem permission bits.
    /// 原始文件系统权限模式。
    permissions: fs::Permissions,
    /// Strict decoded document with encoding and per-line terminators.
    /// 带编码与逐行终止符的严格解码文档。
    document: TextDocument,
    /// UTF-8 BOM presence.
    /// 是否存在 UTF-8 BOM。
    bom: bool,
    /// Normalized logical lines without terminators.
    /// 去除终止符后的归一化逻辑行。
    lines: Vec<String>,
    /// Whether the original text ended with a line terminator.
    /// 原文是否以换行符结束。
    final_newline: bool,
    /// Host-generated version token.
    /// 宿主生成的版本令牌。
    version: String,
}

/// One regular-file snapshot used by byte-preserving lifecycle operations.
/// 供字节保持型生命周期操作使用的一份普通文件快照。
#[derive(Debug, Clone)]
struct RawFileSnapshot {
    /// Original file bytes.
    /// 原始文件字节。
    bytes: Vec<u8>,
    /// Original filesystem permissions.
    /// 原始文件系统权限。
    permissions: fs::Permissions,
    /// Host-generated byte and metadata version token.
    /// 宿主生成的字节与元数据版本令牌。
    version: String,
}

/// One replacement planned against the immutable original line array.
/// 针对不可变原始逻辑行数组规划的一条替换。
#[derive(Debug, Clone)]
struct ReplacementPlan {
    /// Operation identifier.
    /// 操作标识。
    operation_id: String,
    /// Zero-based original start.
    /// 零基原始起点。
    start: usize,
    /// Zero-based exclusive original end.
    /// 零基原始结束位置（不包含）。
    end: usize,
    /// Original lines.
    /// 原始逻辑行。
    old_lines: Vec<String>,
    /// Replacement lines.
    /// 替换后的逻辑行。
    new_lines: Vec<String>,
    /// Matching mode used to locate this replacement.
    /// 定位该替换时实际使用的匹配模式。
    match_mode: String,
}

/// Reads one auto-detected text snapshot and computes a host version token.
/// 读取一份自动检测编码的文本快照并计算宿主版本令牌。
fn read_snapshot(path: &Path) -> Result<TextSnapshot, FileToolError> {
    read_snapshot_with_encoding(path, None)
}

/// Reads a mutation source using an explicit codec or independently supported automatic evidence.
/// 使用显式编码或独立证据支持的自动检测读取变更源。
/// Parameters select the path and optional source codec; returns a snapshot or a no-write ambiguity error.
/// 参数指定路径及可选源编码；返回快照，或不写入任何字节的歧义错误。
fn read_edit_snapshot(path: &Path, encoding: Option<&str>) -> Result<TextSnapshot, FileToolError> {
    let snapshot = read_snapshot_with_encoding(path, encoding)?;
    if !snapshot.document.encoding_certain {
        return Err(FileToolError::new("encoding_ambiguous", format!(
            "The automatic encoding candidate {} is unverified for {}. No bytes were changed. Confirm the source encoding explicitly before editing.",
            snapshot.document.encoding.label(), display_path(path),
        )).with_details(json!({
            "path": display_path(path),
            "encoding_candidate": snapshot.document.encoding.label(),
            "encoding_certain": false,
        })));
    }
    Ok(snapshot)
}

/// Reads one strict text snapshot with an optional explicit encoding override.
/// 使用可选的显式编码覆盖读取一份严格文本快照。
fn read_snapshot_with_encoding(
    path: &Path,
    requested_encoding: Option<&str>,
) -> Result<TextSnapshot, FileToolError> {
    let raw = read_raw_snapshot(path)?;
    let decoded = decode_text(&raw.bytes, requested_encoding).map_err(codec_error)?;
    let document = TextDocument::from_decoded(decoded);
    let bom = document.bom.is_present();
    let final_newline = document.final_newline();
    let lines = document.logical_lines();
    Ok(TextSnapshot {
        version: raw.version,
        bytes: raw.bytes,
        permissions: raw.permissions,
        document,
        bom,
        lines,
        final_newline,
    })
}

/// Reads one bounded regular file without requiring any text decoding.
/// 在不要求任何文本解码的情况下读取一份有界普通文件。
fn read_raw_snapshot(path: &Path) -> Result<RawFileSnapshot, FileToolError> {
    let link_metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("path_not_found", path, error))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return Err(FileToolError::new(
            "unsupported_file_type",
            format!("The target is not a regular file: {}", display_path(path)),
        ));
    }
    if link_metadata.len() > MAX_TEXT_BYTES as u64 {
        return Err(FileToolError::new(
            "output_limit_exceeded",
            "The text file exceeds the native file size limit.",
        ));
    }
    let file = File::open(path).map_err(|error| io_error("read_failed", path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| io_error("read_failed", path, error))?;
    if !metadata.is_file() || metadata.len() > MAX_TEXT_BYTES as u64 {
        return Err(FileToolError::new(
            "output_limit_exceeded",
            "The text file exceeds the native file size limit.",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(MAX_TEXT_BYTES as u64) as usize);
    file.take(MAX_TEXT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read_failed", path, error))?;
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(FileToolError::new(
            "output_limit_exceeded",
            "The text file exceeds the native file size limit.",
        ));
    }
    Ok(RawFileSnapshot {
        version: version_token(&bytes, &metadata),
        bytes,
        permissions: metadata.permissions(),
    })
}

/// Renders logical lines with a selected BOM, line ending, and final-newline state.
/// 使用指定 BOM、换行符和末尾换行状态渲染逻辑行。
fn render_text(lines: &[String], bom: bool, line_ending: &str, final_newline: bool) -> Vec<u8> {
    let mut output = String::new();
    for (index, line) in lines.iter().enumerate() {
        output.push_str(line);
        if index + 1 < lines.len() || final_newline {
            output.push_str(line_ending);
        }
    }
    let mut bytes = Vec::with_capacity(output.len() + usize::from(bom) * 3);
    if bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(output.as_bytes());
    bytes
}

/// Renders line-array replacements while preserving untouched per-line terminators and encoding.
/// 渲染行数组替换，同时保留未触及的逐行终止符与编码。
fn render_snapshot_replacements(
    snapshot: &TextSnapshot,
    replacements: &[ReplacementPlan],
    final_newline: bool,
) -> Result<Vec<u8>, FileToolError> {
    let mut lines = snapshot.document.lines.clone();
    for replacement in replacements.iter().rev() {
        let local_ending = lines
            .get(replacement.start..replacement.end)
            .and_then(|slice| slice.iter().find_map(|line| line.ending))
            .or_else(|| {
                replacement
                    .start
                    .checked_sub(1)
                    .and_then(|index| lines.get(index))
                    .and_then(|line| line.ending)
            })
            .or_else(|| lines.get(replacement.end).and_then(|line| line.ending))
            .unwrap_or(snapshot.document.preferred_line_ending);
        let has_following_line = replacement.end < lines.len();
        let original_text = lines
            .get(replacement.start..replacement.end)
            .unwrap_or_default()
            .iter()
            .map(|line| {
                let mut text = line.content.clone();
                if let Some(ending) = line.ending {
                    text.push_str(ending.as_str());
                }
                text
            })
            .collect::<String>();
        let mut normalized_replacement = replacement.new_lines.join("\n");
        if has_following_line && !replacement.new_lines.is_empty() {
            normalized_replacement.push('\n');
        }
        let inserted = parse_document_lines(&rewrite_replacement_line_endings(
            &normalized_replacement,
            &original_text,
            local_ending,
        ));
        lines.splice(replacement.start..replacement.end, inserted);
    }
    if let Some(last) = lines.last_mut() {
        last.ending = if final_newline {
            Some(
                last.ending
                    .unwrap_or(snapshot.document.preferred_line_ending),
            )
        } else {
            None
        };
    }
    let mut text = String::new();
    for line in lines {
        text.push_str(&line.content);
        if let Some(ending) = line.ending {
            text.push_str(ending.as_str());
        }
    }
    encode_text(&text, snapshot.document.encoding, snapshot.document.bom).map_err(codec_error)
}

/// Computes a content-and-metadata version token for optimistic concurrency checks.
/// 为乐观竞态检查计算内容与元数据版本令牌。
fn version_token(bytes: &[u8], metadata: &Metadata) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.update(metadata.len().to_le_bytes());
    hasher.update([u8::from(metadata.permissions().readonly())]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        hasher.update(metadata.permissions().mode().to_le_bytes());
        hasher.update(metadata.dev().to_le_bytes());
        hasher.update(metadata.ino().to_le_bytes());
    }
    #[cfg(windows)]
    if let Ok(created) = metadata.created()
        && let Ok(duration) = created.duration_since(UNIX_EPOCH)
    {
        // Stable std metadata exposes creation time without requiring forbidden unsafe Win32 FFI.
        // 稳定标准库元数据可提供创建时间，不需要违反 unsafe_code 禁令的 Win32 FFI。
        hasher.update(duration.as_nanos().to_le_bytes());
    }
    if let Ok(modified) = metadata.modified()
        && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
    {
        hasher.update(duration.as_nanos().to_le_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Returns a best-effort modification timestamp for read-only metadata rows.
/// 为只读元数据行返回尽力而为的修改时间戳。
fn modified_millis(metadata: &Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
}

/// Returns one bounded file-system error with a path context.
/// 返回一条带路径上下文的有界文件系统错误。
fn io_error(code: &str, path: &Path, error: std::io::Error) -> FileToolError {
    FileToolError::new(code, format!("{}: {}", display_path(path), error))
}

/// Returns whether one conventional generated/dependency directory should be skipped.
/// 判断一个常见生成目录或依赖目录是否应被跳过。
fn is_ignored_name(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "target" | "dist" | "build" | ".idea" | ".vscode" | "__pycache__"
    )
}

/// Matches one filename against a small `*` and `?` glob pattern.
/// 使用小型 `*` 和 `?` glob 模式匹配一个文件名。
fn pattern_matches(name: &str, pattern: Option<&str>) -> bool {
    let Some(pattern) = pattern.filter(|value| !value.is_empty()) else {
        return true;
    };
    glob_matches(name, pattern)
}

/// Performs bounded Unicode-scalar wildcard matching without a glob engine.
/// 不依赖 glob 引擎执行有界 Unicode 标量通配符匹配。
fn glob_matches(name: &str, pattern: &str) -> bool {
    let name = name.chars().collect::<Vec<_>>();
    let pattern = pattern.chars().collect::<Vec<_>>();
    let mut name_index = 0;
    let mut pattern_index = 0;
    let mut star = None;
    let mut star_name_index = 0;
    while name_index < name.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == name[name_index])
        {
            name_index += 1;
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some(pattern_index);
            star_name_index = name_index;
            pattern_index += 1;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            star_name_index += 1;
            name_index = star_name_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests;
