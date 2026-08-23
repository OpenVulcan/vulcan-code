//! Shared Rust file-reading and file-editing runtime used by native AI tools.
//! 供原生 AI 工具使用的共享 Rust 文件读写运行时。

use regex::Regex;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;
use tokio::task;
use uuid::Uuid;

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
/// 鍗曚釜 replace 鎿嶄綔鍏佽鎺ュ彈鐨勬湁搴忛敋鐐规渶澶ф暟閲忋€?
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
pub const MAX_LIST_ENTRIES: usize = 5_000;

/// Maximum UTF-8 bytes accepted in one search query or pattern.
/// 单个搜索查询或文件名模式允许的最大 UTF-8 字节数。
pub const MAX_SEARCH_QUERY_BYTES: usize = 4_096;

/// Maximum serialized line payload returned by one read target.
/// 鍗曚釜 read 鐩爣杩斿洖鐨勮姝ｆ枃搴忓垪鍖栧瓧鑺傛渶澶уぇ灏忋€?
const MAX_READ_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Maximum serialized entry payload returned by one list target.
/// 鍗曚釜 list 鐩爣杩斿洖鐨勬枃浠舵潯鐩簭鍒楀寲瀛楄妭鏈€澶уぇ灏忋€?
const MAX_LIST_RESPONSE_BYTES: usize = 1 * 1024 * 1024;

/// Maximum serialized match payload returned by one search target.
/// 鍗曚釜 search 鐩爣杩斿洖鐨勬悳绱㈠懡涓簭鍒楀寲瀛楄妭鏈€澶уぇ灏忋€?
const MAX_SEARCH_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Maximum inline lifecycle content retained in a change-set row.
/// 变更集文件行允许内联保留的生命周期正文最大字节数。
const MAX_INLINE_LIFECYCLE_BYTES: usize = 64 * 1024;

/// Maximum candidate ranges retained in one structured edit error.
/// 单条结构化编辑错误最多保留的候选范围数量。
const MAX_ERROR_CANDIDATE_RANGES: usize = 32;

/// Maximum operation contexts retained in one structured edit error.
/// 单条结构化编辑错误最多保留的操作上下文数量。
const MAX_ERROR_OPERATION_CONTEXTS: usize = MAX_EDIT_OPERATIONS;

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
        if !self.details.is_null() {
            object.insert(String::from("details"), self.details.clone());
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
#[derive(Debug, Clone)]
pub struct FileToolResponse {
    /// Compact model-visible result body.
    /// 紧凑的模型可见结果正文。
    pub model_output: Value,
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

/// Executes the read-only action synchronously after arguments have crossed the async boundary.
/// 参数跨过异步边界后，同步执行只读动作。
fn execute_file_sync(
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
        change_set: None,
    })
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
        if !destinations.insert(path_key(destination)) {
            return Err(FileToolError::new(
                "overlapping_operations",
                format!(
                    "Multiple operations target the same destination: {}.",
                    display_path(destination)
                ),
            ));
        }
        for (other_index, other) in operations.iter().enumerate() {
            if index != other_index && path_key(destination) == path_key(&other.path) {
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
        "encoding": "utf-8",
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
    if metadata.is_file() && metadata.len() <= MAX_TEXT_BYTES as u64 {
        if let Ok(snapshot) = read_snapshot(path) {
            result["encoding"] = Value::String(String::from("utf-8"));
            result["bom"] = Value::Bool(snapshot.bom);
            result["line_count"] = json!(snapshot.lines.len());
            result["final_newline"] = Value::Bool(snapshot.final_newline);
            result["version"] = Value::String(snapshot.version);
        }
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
fn serialized_value_bytes(value: &Value) -> usize {
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
    let mut snapshots = HashMap::<String, TextSnapshot>::new();
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    for (index, operation) in parsed.iter().enumerate() {
        groups
            .entry(path_key(&operation.path))
            .or_default()
            .push(index);
        if !matches!(operation.kind, EditKind::Create)
            && !snapshots.contains_key(&path_key(&operation.path))
        {
            let snapshot = read_snapshot(&operation.path).map_err(|error| {
                enrich_parsed_edit_error(error, &parsed, Some(std::slice::from_ref(&index)))
            })?;
            snapshots.insert(path_key(&operation.path), snapshot);
        }
        if let Some(expected_version) = operation.expected_version.as_deref()
            && let Some(snapshot) = snapshots.get(&path_key(&operation.path))
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
    if !dry_run {
        verify_preconditions(&changes)
            .map_err(|error| enrich_parsed_edit_error(error, &parsed, None))?;
        commit_changes(&changes).map_err(|error| enrich_parsed_edit_error(error, &parsed, None))?;
    }
    let mode = if dry_run { "preview" } else { "applied" };
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
        .map(|values| values.iter().copied().collect::<Vec<_>>())
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
    details.insert(String::from("operation_id"), Value::Null);
    details.insert(String::from("operation_index"), Value::Null);
    details.insert(String::from("path"), Value::Null);
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
    details
        .entry(String::from("commit_scope"))
        .or_insert_with(|| Value::String(String::from("none")));
    details
        .entry(String::from("rollback_state"))
        .or_insert_with(|| Value::String(String::from("not_started")));
    details
        .entry(String::from("candidate_ranges"))
        .or_insert_with(|| Value::Array(Vec::new()));
    details
        .entry(String::from("candidate_count"))
        .or_insert_with(|| json!(0));
    details
        .entry(String::from("requires_reread"))
        .or_insert_with(|| Value::Bool(edit_error_requires_reread(code)));
    details
        .entry(String::from("recovery_hint"))
        .or_insert_with(|| Value::String(String::from(edit_error_recovery_hint(code))));
}

/// Returns whether retrying this edit should begin with a fresh file read.
/// 返回重试该编辑前是否必须先重新读取文件。
fn edit_error_requires_reread(code: &str) -> bool {
    matches!(
        code,
        "path_not_found"
            | "path_already_exists"
            | "expected_lines_not_found"
            | "target_not_unique"
            | "anchor_chain_not_found"
            | "stale_version"
            | "concurrent_change"
            | "write_failed"
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
        "path_not_found" => "Use file to rediscover the path before retrying.",
        "path_already_exists" => "Choose another path or explicitly handle the existing target.",
        "unsupported_file_type" => {
            "Use a tool that supports this directory, symbolic link, binary, or non-UTF-8 target."
        }
        "expected_lines_not_found" => {
            "Use file read and copy the complete current logical lines before retrying."
        }
        "target_not_unique" => {
            "Expand old_lines or add stable ordered anchors until exactly one target remains."
        }
        "anchor_chain_not_found" => {
            "Use file read to verify that anchors occur before the target in the declared order."
        }
        "overlapping_operations" => {
            "Merge overlapping replacements into one operation against the original snapshot."
        }
        "stale_version" => "Use file stat or read to obtain a fresh version token before retrying.",
        "concurrent_change" => "Read all affected files again and regenerate the complete request.",
        "no_change" => {
            "Remove the operation or provide new_lines that differ from the current content."
        }
        "write_failed" => {
            "Fix the reported filesystem error, reread affected paths, and retry with a fresh request."
        }
        "disk_state_uncertain" => {
            "Reread every affected path listed in the error before taking any further action."
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
    if destination
        .as_ref()
        .is_some_and(|value| path_key(value) == path_key(&path))
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
    let snapshot = snapshots.get(&path_key(&first.path)).ok_or_else(|| {
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
    let final_lines = apply_replacements(&snapshot.lines, &replacements);
    let after_bytes = render_text(
        &final_lines,
        snapshot.bom,
        &snapshot.line_ending,
        final_newline,
    );
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
    file_payload.insert(String::from("hunks"), Value::Array(hunks));
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
    insert_lifecycle_content(&mut file_payload, &after_bytes, &operation.new_lines);
    Ok(PreparedChange {
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
    insert_lifecycle_content(&mut file_payload, &snapshot.bytes, &snapshot.lines);
    Ok(PreparedChange {
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
        if checked.insert(path_key(&change.source_path)) {
            if change.before_bytes.is_none() {
                if path_exists_including_symlink(&change.source_path) {
                    return Err(FileToolError::new(
                        "path_already_exists",
                        "The create target appeared before commit.",
                    ));
                }
            } else {
                let current = read_snapshot(&change.source_path)?;
                if current.version != change.original_version {
                    return Err(FileToolError::new(
                        "concurrent_change",
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

/// Commits all prepared changes and restores every captured path when one publish fails.
/// 提交所有已准备变更，并在任一发布失败时恢复全部已捕获路径。
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
                    "commit_scope": "none",
                    "rollback_state": "failed",
                    "affected_paths": backups.keys().map(|path| display_path(path)).collect::<Vec<_>>(),
                    "cause": error.as_value(),
                    "rollback_error": rollback_error.as_value(),
                })));
            }
            return Err(error.with_details(json!({
                "commit_scope": "none",
                "rollback_state": "complete",
                "affected_paths": backups.keys().map(|path| display_path(path)).collect::<Vec<_>>(),
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
/// 淇濆瓨鍗曚釜浜嬪姟璺緞鎭㈠鎵€闇€鐨勫瓧鑺傚拰鏉冮檺妯″紡銆?
struct FileBackup {
    /// Original file bytes, or `None` when the path was absent before the transaction.
    /// 鍘熷鏂囦欢瀛楄妭锛涘浜嬪姟鍓嶈矾寰勪笉瀛樺湪鍒欎负 `None`銆?
    bytes: Option<Vec<u8>>,
    /// Original permission mode when the path was a regular file.
    /// 鍘熷鏅€氭枃浠剁殑鏉冮檺妯″紡銆?
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
    let temporary = path.with_extension(format!("vulcan-file-edit-{}", Uuid::new_v4()));
    let result = (|| -> Result<(), FileToolError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error("write_failed", &temporary, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error("write_failed", &temporary, error))?;
        let permissions = requested_permissions.cloned().or_else(|| {
            fs::metadata(path)
                .ok()
                .map(|metadata| metadata.permissions())
        });
        if let Some(permissions) = permissions {
            fs::set_permissions(&temporary, permissions)
                .map_err(|error| io_error("write_failed", &temporary, error))?;
        }
        file.sync_all()
            .map_err(|error| io_error("write_failed", &temporary, error))?;
        drop(file);
        let publish_result = if replace_existing {
            publish_temporary_file(&temporary, path)
        } else {
            publish_temporary_file_without_replace(&temporary, path)
        };
        publish_result.map_err(|error| io_error("write_failed", path, error))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Publishes a same-directory temporary file with platform-appropriate replacement semantics.
/// 使用平台适配的替换语义发布同目录临时文件。
fn publish_temporary_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        // The host crate forbids FFI; keep the same-directory publish path recoverable on Windows.
        // 宿主 crate 禁止 FFI；Windows 下保留同目录发布路径并由上层回滚保护。
        match fs::rename(temporary, path) {
            Ok(()) => Ok(()),
            Err(_rename_error) if path_exists_including_symlink(path) => {
                fs::remove_file(path)?;
                fs::rename(temporary, path)
            }
            Err(error) => Err(error),
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(temporary, path)
    }
}

/// Publishes a new temporary file without replacing an existing destination.
/// 在不替换已有目标的前提下发布新的临时文件。
fn publish_temporary_file_without_replace(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::hard_link(temporary, path)?;
    if let Err(error) = fs::remove_file(temporary) {
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

/// Checks whether a path exists, including a dangling symbolic link.
/// 检查路径是否存在，并将断开的符号链接也视为存在。
fn path_exists_including_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Validates that a write target has a real directory ancestor and no file or symlink blocks its parent chain.
/// 验证写入目标的父目录已经存在，避免工具隐式创建目录。
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

/// Applies sorted replacements from the end so earlier ranges keep original coordinates.
/// 从文件尾部应用排序后的替换，使前面的范围保持原始坐标。
fn apply_replacements(lines: &[String], replacements: &[ReplacementPlan]) -> Vec<String> {
    let mut output = lines.to_vec();
    for replacement in replacements.iter().rev() {
        output.splice(
            replacement.start..replacement.end,
            replacement.new_lines.clone(),
        );
    }
    output
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
fn insert_lifecycle_content(object: &mut Map<String, Value>, bytes: &[u8], lines: &[String]) {
    object.insert(String::from("total_line_count"), json!(lines.len()));
    if bytes.len() <= MAX_INLINE_LIFECYCLE_BYTES {
        object.insert(
            String::from("content"),
            Value::String(String::from_utf8_lossy(bytes).to_string()),
        );
        object.insert(
            String::from("content_mode"),
            Value::String(String::from("full")),
        );
        return;
    }
    let content = String::from_utf8_lossy(bytes);
    let head = content.chars().take(2000).collect::<String>();
    let tail = content
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

/// Resolves a relative path against the workspace while rejecting lexical escapes.
/// 根据工作区解析相对路径，并拒绝词法路径逃逸。
fn resolve_path(workspace_root: &Path, raw_path: &str) -> Result<PathBuf, FileToolError> {
    let expanded_path = expand_environment_path(raw_path)?;
    let candidate = PathBuf::from(expanded_path);
    if !candidate.is_absolute()
        && candidate
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(FileToolError::new(
            "invalid_arguments",
            "Relative paths may not escape the workspace with parent-directory components.",
        ));
    }
    let path = if candidate.is_absolute() {
        candidate
    } else {
        workspace_root.join(candidate)
    };
    Ok(lexical_normalize(&path))
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

/// Returns a stable path key suitable for grouping operations in one request.
/// 返回适合在单次请求内归并操作的稳定路径键。
fn path_key(path: &Path) -> String {
    let value = display_path(path);
    #[cfg(windows)]
    {
        value.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        value
    }
}

/// Returns a slash-normalized path string for model and host payloads.
/// 返回供模型和宿主载荷使用的斜杠归一化路径字符串。
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

/// One prepared resource-level mutation ready for dry-run display or commit.
/// 一条可用于 dry-run 展示或提交的资源级准备变更。
struct PreparedChange {
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
    /// 鍒犻櫎鎴栫Щ鍔ㄦ枃浠舵仮澶嶆垨閲嶆柊鍒涘缓鏃朵繚鐣欏師濮嬫潈闄愭ā寮忋€?
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
    /// 鍘熷鏂囦欢绯荤粺鏉冮檺妯″紡銆?
    permissions: fs::Permissions,
    /// UTF-8 BOM presence.
    /// 是否存在 UTF-8 BOM。
    bom: bool,
    /// Normalized logical lines without terminators.
    /// 去除终止符后的归一化逻辑行。
    lines: Vec<String>,
    /// Dominant original line ending.
    /// 原文占主导的换行符。
    line_ending: String,
    /// Whether the original text ended with a line terminator.
    /// 原文是否以换行符结束。
    final_newline: bool,
    /// Host-generated version token.
    /// 宿主生成的版本令牌。
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

/// Reads one UTF-8 snapshot and computes a host version token.
/// 读取一份 UTF-8 快照并计算宿主版本令牌。
fn read_snapshot(path: &Path) -> Result<TextSnapshot, FileToolError> {
    let link_metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("path_not_found", path, error))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return Err(FileToolError::new(
            "unsupported_file_type",
            format!(
                "The target is not a regular UTF-8 file: {}",
                display_path(path)
            ),
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
    let (bom, body) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (true, &bytes[3..])
    } else {
        (false, bytes.as_slice())
    };
    let text = String::from_utf8(body.to_vec()).map_err(|_| {
        FileToolError::new(
            "unsupported_file_type",
            "Only UTF-8 text files are supported by native file tools.",
        )
    })?;
    let final_newline = text.ends_with('\n') || text.ends_with('\r');
    let line_ending = dominant_line_ending(&text);
    let lines = logical_lines(&text);
    Ok(TextSnapshot {
        version: version_token(&bytes, &metadata),
        bytes,
        permissions: metadata.permissions(),
        bom,
        lines,
        line_ending,
        final_newline,
    })
}

/// Splits text into logical lines while normalizing CRLF and CR separators.
/// 将文本按逻辑行拆分，同时归一化 CRLF 和 CR 分隔符。
fn logical_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut parts = normalized
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if normalized.ends_with('\n') {
        let _ = parts.pop();
    }
    parts
}

/// Returns the dominant original line-ending style.
/// 返回原文占主导的换行符样式。
fn dominant_line_ending(text: &str) -> String {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count().saturating_sub(crlf);
    let cr = text.matches('\r').count().saturating_sub(crlf);
    if crlf == 0 && lf == 0 && cr == 0 {
        String::from("\n")
    } else if crlf >= lf && crlf >= cr {
        String::from("\r\n")
    } else if cr >= lf {
        String::from("\r")
    } else {
        String::from("\n")
    }
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
    glob_matches(name.as_bytes(), pattern.as_bytes())
}

/// Performs bounded wildcard matching without allocating a glob engine.
/// 不分配 glob 引擎即可执行有界通配符匹配。
fn glob_matches(name: &[u8], pattern: &[u8]) -> bool {
    let mut name_index = 0;
    let mut pattern_index = 0;
    let mut star = None;
    let mut star_name_index = 0;
    while name_index < name.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == name[name_index])
        {
            name_index += 1;
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
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
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_EDIT_LINES_PER_ARRAY, MAX_READ_LINES, execute_file, execute_file_edit, resolve_path,
    };
    use serde_json::{Value, json};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    /// Verifies native path resolution expands the established environment placeholder syntax.
    /// 验证原生路径解析会展开既有的环境变量占位符语法。
    #[test]
    fn environment_path_placeholders_are_expanded_once() {
        let workspace = Path::new("D:/workspace");
        let expected_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let resolved = resolve_path(workspace, "${env:CARGO_MANIFEST_DIR}/src")
            .expect("environment placeholder should resolve");

        assert_eq!(resolved, expected_root.join("src"));
    }

    /// Verifies malformed environment references fail with an explicit diagnostic.
    /// 验证格式错误的环境变量引用会返回明确诊断。
    #[test]
    fn malformed_environment_path_placeholder_is_rejected() {
        let error = resolve_path(Path::new("D:/workspace"), "${env:CARGO_MANIFEST_DIR/src")
            .expect_err("malformed environment placeholder should fail");

        assert_eq!(error.code, "invalid_environment_variable_reference");
    }

    /// Verifies read, search, and stat expose bounded text metadata without writing.
    /// 验证读取、搜索和统计会暴露有界文本元数据且不会写入文件。
    #[tokio::test]
    async fn read_search_and_stat_are_read_only() {
        let directory = tempdir().expect("create temporary directory");
        let path = directory.path().join("sample.txt");
        fs::write(&path, "alpha\nbeta\n").expect("write fixture");
        let read = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action": "read",
                "targets": [{"path": "sample.txt"}]
            }),
        )
        .await
        .expect("read should succeed");
        assert_eq!(read.model_output["results"][0]["line_count"], json!(2));
        let search = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action": "search",
                "targets": [{"path": "."}],
                "query": "beta"
            }),
        )
        .await
        .expect("search should succeed");
        assert_eq!(
            search.model_output["results"][0]["matches"][0]["line"],
            json!(2)
        );
        let bounded_path = directory.path().join("bounded.txt");
        fs::write(&bounded_path, "beta\n").expect("write bounded fixture");
        let exact_limit = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action": "search",
                "targets": [{"path": "bounded.txt"}],
                "query": "beta",
                "limit": 1
            }),
        )
        .await
        .expect("bounded search should succeed");
        assert_eq!(
            exact_limit.model_output["results"][0]["truncated"],
            json!(false)
        );
        fs::write(&bounded_path, "beta\nbeta\n").expect("extend bounded fixture");
        let over_limit = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action": "search",
                "targets": [{"path": "bounded.txt"}],
                "query": "beta",
                "limit": 1
            }),
        )
        .await
        .expect("over-limit search should succeed");
        assert_eq!(
            over_limit.model_output["results"][0]["truncated"],
            json!(true)
        );
        assert_eq!(
            over_limit.model_output["results"][0]["matches"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        let stat = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action": "stat",
                "targets": [{"path": "sample.txt"}]
            }),
        )
        .await
        .expect("stat should succeed");
        assert!(stat.model_output["results"][0]["version"].is_string());
        assert_eq!(
            fs::read_to_string(path).expect("read fixture after read-only calls"),
            "alpha\nbeta\n"
        );
    }

    /// Verifies one dry-run and one applied request can contain multiple non-overlapping replacements.
    /// 验证一次 dry-run 和一次应用请求都可以包含多个不重叠替换。
    #[tokio::test]
    async fn edit_supports_arrays_anchors_and_atomic_change_set() {
        let directory = tempdir().expect("create temporary directory");
        let path = directory.path().join("sample.txt");
        fs::write(
            &path,
            "fn a() {\n  return 1;\n}\nfn c() {\n  return 3;\n}\n",
        )
        .expect("write fixture");
        let request = json!({
            "operations": [
                {
                    "id": "change-a",
                    "operation": "replace",
                    "path": "sample.txt",
                    "anchors": ["fn a() {"],
                    "old_lines": ["  return 1;"],
                    "new_lines": ["  return 2;", "  return 20;"]
                },
                {
                    "id": "change-c",
                    "operation": "replace",
                    "path": "sample.txt",
                    "anchors": ["fn c() {"],
                    "old_lines": ["  return 3;"],
                    "new_lines": ["  return 4;"]
                }
            ]
        });
        let preview = execute_file_edit(
            directory.path().to_path_buf(),
            json!({"dry_run": true, "operations": request["operations"]}),
        )
        .await
        .expect("dry-run should succeed");
        assert_eq!(
            preview.change_set.as_ref().expect("change set")["mode"],
            json!("preview")
        );
        assert_eq!(
            fs::read_to_string(&path).expect("read after dry-run"),
            "fn a() {\n  return 1;\n}\nfn c() {\n  return 3;\n}\n"
        );
        let applied = execute_file_edit(directory.path().to_path_buf(), request)
            .await
            .expect("apply should succeed");
        assert_eq!(
            applied.change_set.as_ref().expect("change set")["mode"],
            json!("applied")
        );
        assert_eq!(
            applied.change_set.as_ref().expect("change set")["files"][0]["resource_id"],
            json!("resource-1")
        );
        assert_eq!(
            fs::read_to_string(path).expect("read after apply"),
            "fn a() {\n  return 2;\n  return 20;\n}\nfn c() {\n  return 4;\n}\n"
        );
        assert_eq!(
            applied.change_set.as_ref().expect("change set")["files"][0]["hunks"][1]["insert"][0]["line"],
            json!(6)
        );
    }

    /// Verifies create preflight rejects existing targets, creates missing parents only on apply, and hunks show real changed lines.
    /// 验证创建预检会拒绝已有目标或缺失父目录，并且 hunk 只展示真实变更行。
    #[tokio::test]
    async fn edit_preflight_and_hunk_diff_are_explicit() {
        let directory = tempdir().expect("create temporary directory");
        let path = directory.path().join("sample.txt");
        fs::write(&path, "line one\nline two\nline three\n").expect("write fixture");

        let existing_create = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "dry_run": true,
                "operations": [{
                    "id": "existing",
                    "operation": "create",
                    "path": "sample.txt",
                    "new_lines": ["replacement"]
                }]
            }),
        )
        .await
        .expect_err("creating an existing target must fail during preview");
        assert_eq!(existing_create.code, "path_already_exists");

        let missing_parent_request = json!({
            "operations": [{
                "id": "missing-parent",
                "operation": "create",
                "path": "missing/child.txt",
                "new_lines": ["content"]
            }]
        });
        let missing_parent_preview = execute_file_edit(
            directory.path().to_path_buf(),
            json!({"dry_run": true, "operations": missing_parent_request["operations"]}),
        )
        .await
        .expect("missing parents should be previewable without writes");
        assert_eq!(
            missing_parent_preview
                .change_set
                .expect("preview change set")["mode"],
            json!("preview")
        );
        assert!(!directory.path().join("missing").exists());

        execute_file_edit(directory.path().to_path_buf(), missing_parent_request)
            .await
            .expect("apply should create missing parents");
        assert_eq!(
            fs::read_to_string(directory.path().join("missing/child.txt"))
                .expect("read created file"),
            "content\n"
        );

        let result = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [{
                    "id": "middle-line",
                    "operation": "replace",
                    "path": "sample.txt",
                    "old_lines": ["line one", "line two", "line three"],
                    "new_lines": ["line one", "changed", "line three"]
                }]
            }),
        )
        .await
        .expect("replace should succeed");
        let hunk = &result.change_set.expect("change set")["files"][0]["hunks"][0];
        assert_eq!(hunk["before"], json!("line one"));
        assert_eq!(hunk["delete"].as_array().map(Vec::len), Some(1));
        assert_eq!(hunk["delete"][0]["content"], json!("line two"));
        assert_eq!(hunk["insert"].as_array().map(Vec::len), Some(1));
        assert_eq!(hunk["insert"][0]["content"], json!("changed"));
        assert_eq!(hunk["after"], json!("line three"));
    }

    /// Verifies adding logical lines to a file without an existing newline defaults to LF.
    /// 验证向原本没有换行符的文件追加逻辑行时默认使用 LF。
    #[tokio::test]
    async fn edit_without_original_line_ending_defaults_to_lf() {
        let directory = tempdir().expect("create temporary directory");
        let path = directory.path().join("sample.txt");
        fs::write(&path, "one").expect("write fixture");

        execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [{
                    "id": "append-line",
                    "operation": "replace",
                    "path": "sample.txt",
                    "old_lines": ["one"],
                    "new_lines": ["one", "two"]
                }]
            }),
        )
        .await
        .expect("replace should succeed");

        assert_eq!(fs::read(&path).expect("read result"), b"one\ntwo");
    }

    /// Verifies a pure move is accepted even when file bytes remain unchanged.
    /// 验证纯移动即使文件字节不变也会被正确接受。
    #[tokio::test]
    async fn move_file_produces_canonical_rename() {
        let directory = tempdir().expect("create temporary directory");
        let source = directory.path().join("source.txt");
        let destination = directory.path().join("destination.txt");
        fs::write(&source, "unchanged\n").expect("write fixture");
        let stat = execute_file(
            directory.path().to_path_buf(),
            json!({"action": "stat", "targets": [{"path": "source.txt"}]}),
        )
        .await
        .expect("stat should succeed");
        let version = stat.model_output["results"][0]["version"]
            .as_str()
            .expect("version");

        let result = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [{
                    "id": "move-source",
                    "operation": "move_file",
                    "path": "source.txt",
                    "destination": "destination.txt",
                    "expected_version": version
                }]
            }),
        )
        .await
        .expect("move should succeed");
        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(&destination).expect("read destination"),
            "unchanged\n"
        );
        let file = &result.change_set.expect("change set")["files"][0];
        assert_eq!(file["change"], json!("rename"));
        assert!(
            file["old_path"]
                .as_str()
                .unwrap_or_default()
                .ends_with("/source.txt")
        );
        assert!(
            file["new_path"]
                .as_str()
                .unwrap_or_default()
                .ends_with("/destination.txt")
        );
    }

    /// Verifies a move cannot silently reorder later replacements for the same resource.
    /// 验证移动操作不能静默重排同一资源后续的替换操作。
    #[tokio::test]
    async fn move_must_be_last_for_one_source_resource() {
        let directory = tempdir().expect("create temporary directory");
        let source = directory.path().join("source.txt");
        fs::write(&source, "before\n").expect("write fixture");
        let stat = execute_file(
            directory.path().to_path_buf(),
            json!({"action": "stat", "targets": [{"path": "source.txt"}]}),
        )
        .await
        .expect("stat should succeed");
        let version = stat.model_output["results"][0]["version"]
            .as_str()
            .expect("version");

        let error = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [
                    {
                        "id": "move-first",
                        "operation": "move_file",
                        "path": "source.txt",
                        "destination": "destination.txt",
                        "expected_version": version
                    },
                    {
                        "id": "replace-after-move",
                        "operation": "replace",
                        "path": "source.txt",
                        "old_lines": ["before"],
                        "new_lines": ["after"]
                    }
                ]
            }),
        )
        .await
        .expect_err("move-before-replace must be rejected");

        assert_eq!(error.code, "invalid_arguments");
        assert!(source.exists());
        assert!(!directory.path().join("destination.txt").exists());
    }

    /// Verifies ambiguous old blocks and stale version tokens fail before any write.
    /// 验证重复旧块和陈旧版本令牌会在写入前失败。
    #[tokio::test]
    async fn edit_rejects_ambiguous_and_stale_requests() {
        let directory = tempdir().expect("create temporary directory");
        let path = directory.path().join("sample.txt");
        fs::write(&path, "same\nsame\n").expect("write fixture");
        let ambiguous = execute_file_edit(
            directory.path().to_path_buf(),
            json!({"operations": [{"id": "ambiguous", "operation": "replace", "path": "sample.txt", "old_lines": ["same"], "new_lines": ["new"]}]}),
        )
        .await;
        let ambiguous_error = ambiguous.expect_err("ambiguous target should fail");
        assert_eq!(ambiguous_error.code, "target_not_unique");
        assert_eq!(ambiguous_error.details["operation_id"], json!("ambiguous"));
        assert_eq!(ambiguous_error.details["operation_index"], json!(0));
        assert!(
            ambiguous_error.details["path"]
                .as_str()
                .unwrap_or_default()
                .ends_with("sample.txt")
        );
        assert_eq!(
            ambiguous_error.details["candidate_ranges"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(ambiguous_error.details["candidate_count"], json!(2));
        assert_eq!(ambiguous_error.details["commit_scope"], json!("none"));
        assert_eq!(ambiguous_error.details["requires_reread"], json!(true));
        assert!(ambiguous_error.details["recovery_hint"].is_string());
        let oversized_old_block = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [{
                    "id": "too-long",
                    "operation": "replace",
                    "path": "sample.txt",
                    "old_lines": ["same", "same", "missing"],
                    "new_lines": ["new"]
                }]
            }),
        )
        .await
        .expect_err("an old block longer than the file should fail safely");
        assert_eq!(oversized_old_block.code, "expected_lines_not_found");
        let anchor_mismatch = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [{
                    "id": "anchor-mismatch",
                    "operation": "replace",
                    "path": "sample.txt",
                    "anchors": ["missing anchor"],
                    "old_lines": ["same"],
                    "new_lines": ["new"]
                }]
            }),
        )
        .await
        .expect_err("missing anchor chain should fail");
        assert_eq!(anchor_mismatch.code, "anchor_chain_not_found");
        let read = execute_file(
            directory.path().to_path_buf(),
            json!({"action": "stat", "targets": [{"path": "sample.txt"}]}),
        )
        .await
        .expect("stat should succeed");
        let version = read.model_output["results"][0]["version"]
            .as_str()
            .expect("version");
        fs::write(&path, "changed\n").expect("mutate fixture");
        let stale = execute_file_edit(
            directory.path().to_path_buf(),
            json!({"operations": [{"id": "delete", "operation": "delete_file", "path": "sample.txt", "expected_version": version}]}),
        )
        .await;
        assert!(stale.is_err());
        assert!(path.exists());
    }

    /// Verifies empty-file insertion rejects anchors that cannot provide a position.
    /// 验证空文件插入会拒绝无法提供定位的 anchors。
    #[tokio::test]
    async fn empty_file_insertion_rejects_anchors() {
        let directory = tempdir().expect("create temporary directory");
        let path = directory.path().join("empty.txt");
        fs::write(&path, "").expect("write empty fixture");

        let error = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [{
                    "id": "invalid-anchor",
                    "operation": "replace",
                    "path": "empty.txt",
                    "anchors": ["unused"],
                    "old_lines": [],
                    "new_lines": ["content"]
                }]
            }),
        )
        .await
        .expect_err("empty-file insertion should not silently ignore anchors");

        assert_eq!(error.code, "invalid_arguments");
    }

    /// Verifies strict action fields, duplicate ids, and relaxed match reporting.
    /// 验证动作字段严格校验、重复 ID 拒绝以及宽松匹配结果的准确报告。
    #[tokio::test]
    async fn edit_and_read_reject_silent_contract_drift() {
        let directory = tempdir().expect("create temporary directory");
        let path = directory.path().join("sample.txt");
        fs::write(&path, "value  \n").expect("write fixture");

        let invalid_read = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action": "read",
                "targets": [{"path": "sample.txt"}],
                "query": "value"
            }),
        )
        .await;
        assert!(invalid_read.is_err());

        let duplicate_ids = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [
                    {"id": "same", "operation": "replace", "path": "sample.txt", "old_lines": ["value  "], "new_lines": ["one"]},
                    {"id": "same", "operation": "replace", "path": "sample.txt", "old_lines": ["value  "], "new_lines": ["two"]}
                ]
            }),
        )
        .await;
        assert!(duplicate_ids.is_err());
        assert_eq!(
            fs::read_to_string(&path).expect("read fixture"),
            "value  \n"
        );

        let relaxed = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [{
                    "id": "relaxed",
                    "operation": "replace",
                    "path": "sample.txt",
                    "old_lines": ["value"],
                    "new_lines": ["changed"],
                    "match_mode": "ignore_trailing_whitespace"
                }]
            }),
        )
        .await
        .expect("relaxed edit should succeed");
        assert_eq!(
            relaxed.change_set.as_ref().expect("change set")["extensions"]["vulcan.file_edit.v1"]["operation_results"]
                [0]["match_mode"],
            json!("ignore_trailing_whitespace")
        );
    }

    /// Verifies malformed optional types and hard output limits become explicit diagnostics.
    /// 验证错误的可选字段类型和硬输出上限都会变成明确的诊断结果。
    #[tokio::test]
    async fn read_and_edit_limits_are_explicit() {
        let directory = tempdir().expect("create temporary directory");
        let path = directory.path().join("sample.txt");
        let large_text = (0..=MAX_READ_LINES)
            .map(|index| format!("line-{index}\n"))
            .collect::<String>();
        fs::write(&path, large_text).expect("write large fixture");

        let invalid_type = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action": "read",
                "targets": [{"path": "sample.txt", "numbered": "yes"}]
            }),
        )
        .await
        .expect("per-target validation should be returned as a result row");
        assert_eq!(
            invalid_type.model_output["results"][0]["error"]["code"],
            json!("invalid_arguments")
        );

        let oversized_read = execute_file(
            directory.path().to_path_buf(),
            json!({"action": "read", "targets": [{"path": "sample.txt"}]}),
        )
        .await
        .expect("per-target output should be returned as a bounded result row");
        assert_eq!(
            oversized_read.model_output["results"][0]["truncated"],
            json!(true)
        );
        assert_eq!(
            oversized_read.model_output["results"][0]["lines"]
                .as_array()
                .map(Vec::len),
            Some(MAX_READ_LINES)
        );

        let too_many_lines = (0..=MAX_EDIT_LINES_PER_ARRAY)
            .map(|index| Value::String(format!("line-{index}")))
            .collect::<Vec<_>>();
        let oversized_edit = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [{
                    "id": "too-many-lines",
                    "operation": "create",
                    "path": "new.txt",
                    "new_lines": too_many_lines
                }]
            }),
        )
        .await
        .expect_err("line-array limit should be rejected");
        assert_eq!(oversized_edit.code, "output_limit_exceeded");
    }
}
