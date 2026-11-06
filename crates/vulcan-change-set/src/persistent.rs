//! Canonical serializable change-set payload preserved inside V1 session events.
//! 保存在 V1 会话事件内的 canonical 可序列化 change-set 载荷。
//!
//! The V1 session model stores the complete structured change-set inside the
//! owning tool event so tool cards and diff statistics never re-parse another
//! text representation or re-read the current file. This module defines the
//! serializable canonical shape plus the hunk-derived add/delete statistics.
//! V1 会话模型把完整结构化 change-set 保存在所属工具事件内，使工具卡与 diff
//! 统计永远不再解析另一份文本表示或重新读取当前文件。本模块定义可序列化的
//! canonical 结构以及由 hunk 推导出的新增/删除统计。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

use crate::{
    CHANGE_SET_HOST_RESULT_KIND, ChangeSetFileChangeKind, ChangeSetMode, ChangeSetStats,
    count_change_set_content_lines, normalize_text,
};

/// Exact before/after text formats keyed by one producer resource identifier.
/// 按产出方资源标识索引的精确变更前后文本格式。
type ResourceFormatMap =
    BTreeMap<String, (Option<PersistentTextFormat>, Option<PersistentTextFormat>)>;

/// Canonical, self-contained change-set preserved in one V1 tool event.
/// 保存在单条 V1 工具事件内的 canonical 自包含 change-set。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentChangeSet {
    /// Producer-declared canonical mode of this payload.
    /// 产出方声明的 canonical 载荷模式。
    pub mode: ChangeSetMode,
    /// Optional producer-authored summary.
    /// 产出方提供的可选摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Complete per-file records with their raw hunks.
    /// 携带原始 hunk 的完整逐文件记录。
    pub files: Vec<PersistentChangeSetFile>,
    /// Structured diagnostics emitted by the producer.
    /// 产出方发出的结构化诊断集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<PersistentChangeSetDiagnostic>,
}

/// One structured diagnostic preserved with the canonical change-set.
/// 与 canonical change-set 一同保留的一条结构化诊断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentChangeSetDiagnostic {
    /// Producer-declared diagnostic level.
    /// 产出方声明的诊断级别。
    pub level: String,
    /// Human-readable diagnostic message.
    /// 人类可读的诊断消息。
    pub message: String,
}

/// Exact text-format snapshot preserved before or after one file mutation.
/// 单次文件变更前后保留的精确文本格式快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentTextFormat {
    /// Stable encoding label.
    /// 稳定编码标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    /// Source used to resolve the encoding.
    /// 用于解析编码的来源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding_source: Option<String>,
    /// Whether the producer considered the encoding detection certain.
    /// 产出方是否认为编码检测结果可靠。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding_certain: Option<bool>,
    /// Whether a byte-order mark is present.
    /// 是否存在字节顺序标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bom: Option<bool>,
    /// Observed line-ending label.
    /// 观测到的换行符标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<String>,
    /// Preferred line-ending label for future writes.
    /// 后续写入优先使用的换行符标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_line_ending: Option<String>,
    /// Whether the document contains mixed line endings.
    /// 文档是否包含混合换行符。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mixed_line_endings: Option<bool>,
    /// Whether the document ends with a final newline.
    /// 文档是否以最终换行符结尾。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_newline: Option<bool>,
}

/// Exact lifecycle content representation emitted for create, delete, or rename records.
/// 为创建、删除或重命名记录产出的精确生命周期正文表示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PersistentLifecycleContent {
    /// Complete text remains available inline.
    /// 完整文本以内联形式保留。
    Full {
        /// Complete lifecycle text.
        /// 完整生命周期文本。
        content: String,
        /// Producer-reported logical line count.
        /// 产出方上报的逻辑行数。
        total_line_count: usize,
    },
    /// The producer intentionally retained only explicit head and tail snippets.
    /// 产出方有意只保留明确的头部与尾部片段。
    Truncated {
        /// Retained leading text.
        /// 保留的头部文本。
        head: String,
        /// Retained trailing text.
        /// 保留的尾部文本。
        tail: String,
        /// Producer-reported complete logical line count.
        /// 产出方上报的完整逻辑行数。
        total_line_count: usize,
    },
    /// Text is intentionally omitted, normally because the resource is binary.
    /// 文本被有意省略，通常因为资源是二进制文件。
    Omitted {
        /// Optional producer-reported logical line count when one exists.
        /// 产出方可选上报的逻辑行数。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_line_count: Option<usize>,
    },
}

impl PersistentLifecycleContent {
    /// Returns the complete logical line count declared by the producer when available.
    /// 在可用时返回产出方声明的完整逻辑行数。
    pub fn total_line_count(&self) -> Option<usize> {
        match self {
            Self::Full {
                total_line_count, ..
            }
            | Self::Truncated {
                total_line_count, ..
            } => Some(*total_line_count),
            Self::Omitted { total_line_count } => *total_line_count,
        }
    }
}

/// One canonical file record inside a persistent change-set.
/// 持久化 change-set 内的一条 canonical 文件记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentChangeSetFile {
    /// Optional opaque resource identifier used to correlate one file.
    /// 用于关联单个文件的可选不透明资源标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    /// Stable file-level change kind.
    /// 稳定的文件级变更类型。
    pub change: ChangeSetFileChangeKind,
    /// Optional primary path of the file record.
    /// 文件记录的可选主路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional rename source path.
    /// 可选重命名源路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// Optional rename destination path.
    /// 可选重命名目标路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_path: Option<String>,
    /// Complete modify hunks preserved for the file record.
    /// 为文件记录保留的完整 modify hunk 集合。
    pub hunks: Vec<PersistentChangeSetHunk>,
    /// Optional exact lifecycle content for create, delete, or rename records.
    /// 创建、删除或重命名记录的可选精确生命周期正文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_content: Option<PersistentLifecycleContent>,
    /// Optional text-format snapshot before the mutation.
    /// 变更前的可选文本格式快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_text_format: Option<PersistentTextFormat>,
    /// Optional text-format snapshot after the mutation.
    /// 变更后的可选文本格式快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_text_format: Option<PersistentTextFormat>,
}

/// One canonical modify hunk with encoding and line-ending metadata.
/// 携带编码与换行元数据的一条 canonical modify hunk。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentChangeSetHunk {
    /// Stable leading context text before the edited block.
    /// 编辑区块之前的稳定前置上下文文本。
    pub before: String,
    /// Stable trailing context text after the edited block.
    /// 编辑区块之后的稳定后置上下文文本。
    pub after: String,
    /// Deleted source lines carried by the hunk.
    /// hunk 携带的删除源行集合。
    pub deleted: Vec<PersistentChangeSetLine>,
    /// Inserted destination lines carried by the hunk.
    /// hunk 携带的插入目标行集合。
    pub inserted: Vec<PersistentChangeSetLine>,
}

/// One line entry preserved inside a canonical modify hunk.
/// 保存在 canonical modify hunk 内的一条行记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentChangeSetLine {
    /// One-based line number reported by the producer.
    /// 产出方上报的 1-based 行号。
    pub line: usize,
    /// Exact text content carried by the line entry.
    /// 行记录携带的精确文本内容。
    pub content: String,
}

/// Structured conversion failure for one producer-owned change-set payload.
/// 产出方 change-set 载荷的一条结构化转换失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentChangeSetError {
    /// Exact logical location of the invalid field.
    /// 无效字段的精确逻辑位置。
    pub location: String,
    /// Concrete contract violation.
    /// 具体契约违例。
    pub detail: String,
}

impl PersistentChangeSetError {
    /// Builds one exact conversion error without guessing another source field.
    /// 构建一条精确转换错误，不猜测其他来源字段。
    fn new(location: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for PersistentChangeSetError {
    /// Formats one stable producer-contract failure.
    /// 格式化一条稳定的产出方契约失败。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Invalid change-set field at `{}`: {}",
            self.location, self.detail
        )
    }
}

impl std::error::Error for PersistentChangeSetError {}

/// Parses one canonical host result into the complete V1-persisted change-set contract.
/// 把一份 canonical 宿主结果解析为完整的 V1 持久化 change-set 契约。
///
/// A non-change-set host result returns `Ok(None)`. Once the producer declares
/// `kind = change_set`, every file is parsed strictly and no invalid row is
/// silently filtered out.
/// 非 change-set 宿主结果返回 `Ok(None)`。一旦产出方声明 `kind = change_set`，
/// 每条文件记录都会被严格解析，任何无效记录都不会被静默过滤。
pub fn persistent_change_set_from_host_result(
    host_result: &Value,
) -> Result<Option<PersistentChangeSet>, PersistentChangeSetError> {
    let Some(host_object) = host_result.as_object() else {
        return Ok(None);
    };
    if host_object.get("kind").and_then(Value::as_str) != Some(CHANGE_SET_HOST_RESULT_KIND) {
        return Ok(None);
    }
    let payload = host_object
        .get("payload")
        .and_then(Value::as_object)
        .ok_or_else(|| PersistentChangeSetError::new("payload", "expected one object payload"))?;
    let mode = parse_change_set_mode(payload.get("mode"), "payload.mode")?;
    let summary = parse_optional_string(payload.get("summary"), "payload.summary")?;
    let resource_formats = parse_resource_formats(payload.get("extensions"))?;
    let file_values = payload
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| PersistentChangeSetError::new("payload.files", "expected one array"))?;
    let files = file_values
        .iter()
        .enumerate()
        .map(|(index, file)| parse_persistent_file(file, index, &resource_formats))
        .collect::<Result<Vec<_>, _>>()?;
    let diagnostics = parse_persistent_diagnostics(payload.get("diagnostics"))?;
    let changeset = PersistentChangeSet {
        mode,
        summary,
        files,
        diagnostics,
    };
    validate_persistent_change_set(&changeset)?;
    Ok(Some(changeset))
}

/// Validates the complete persistent change-set as one authoritative producer contract.
/// 将完整持久化 change-set 作为一份权威产出方契约进行校验。
pub fn validate_persistent_change_set(
    changeset: &PersistentChangeSet,
) -> Result<(), PersistentChangeSetError> {
    for (file_index, file) in changeset.files.iter().enumerate() {
        validate_persistent_file(file, file_index)?;
    }
    Ok(())
}

/// Validates one file record without inferring missing paths or content from another field.
/// 校验单条文件记录，不从其他字段推断缺失路径或正文。
fn validate_persistent_file(
    file: &PersistentChangeSetFile,
    file_index: usize,
) -> Result<(), PersistentChangeSetError> {
    let location = format!("payload.files[{file_index}]");
    let path = require_non_empty_path(file.path.as_deref(), &format!("{location}.path"))?;
    match file.change {
        ChangeSetFileChangeKind::Create | ChangeSetFileChangeKind::Delete => {
            reject_present_path(file.old_path.as_deref(), &format!("{location}.old_path"))?;
            reject_present_path(file.new_path.as_deref(), &format!("{location}.new_path"))?;
            if !file.hunks.is_empty() {
                return Err(PersistentChangeSetError::new(
                    format!("{location}.hunks"),
                    "lifecycle changes must not contain modify hunks",
                ));
            }
            if file.lifecycle_content.is_none() {
                return Err(PersistentChangeSetError::new(
                    format!("{location}.lifecycle_content"),
                    "lifecycle changes require one explicit full, truncated, or omitted body",
                ));
            }
        }
        ChangeSetFileChangeKind::Modify => {
            reject_present_path(file.old_path.as_deref(), &format!("{location}.old_path"))?;
            reject_present_path(file.new_path.as_deref(), &format!("{location}.new_path"))?;
            // Only a format-only change of a file without lines may carry no hunk.
            // 只有无行文件的纯格式变化可以不带 hunk。
            let format_only = matches!(
                (&file.before_text_format, &file.after_text_format),
                (Some(before), Some(after)) if before != after
            );
            if file.hunks.is_empty() && !format_only {
                return Err(PersistentChangeSetError::new(
                    format!("{location}.hunks"),
                    "modify changes require at least one non-empty hunk",
                ));
            }
            if file.lifecycle_content.is_some() {
                return Err(PersistentChangeSetError::new(
                    format!("{location}.lifecycle_content"),
                    "modify changes must not contain lifecycle content",
                ));
            }
        }
        ChangeSetFileChangeKind::Rename => {
            let old_path =
                require_non_empty_path(file.old_path.as_deref(), &format!("{location}.old_path"))?;
            let new_path =
                require_non_empty_path(file.new_path.as_deref(), &format!("{location}.new_path"))?;
            if path != new_path {
                return Err(PersistentChangeSetError::new(
                    format!("{location}.path"),
                    "rename path must equal new_path",
                ));
            }
            if old_path == new_path {
                return Err(PersistentChangeSetError::new(
                    format!("{location}.new_path"),
                    "rename old_path and new_path must differ",
                ));
            }
            match (file.hunks.is_empty(), file.lifecycle_content.is_some()) {
                (true, true) | (false, false) => {}
                (true, false) => {
                    return Err(PersistentChangeSetError::new(
                        format!("{location}.lifecycle_content"),
                        "a pure rename requires one explicit lifecycle body",
                    ));
                }
                (false, true) => {
                    return Err(PersistentChangeSetError::new(
                        format!("{location}.lifecycle_content"),
                        "a rename with modify hunks must not contain lifecycle content",
                    ));
                }
            }
        }
    }

    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        validate_persistent_hunk(hunk, &format!("{location}.hunks[{hunk_index}]"))?;
    }
    if let Some(lifecycle_content) = file.lifecycle_content.as_ref() {
        validate_lifecycle_content(lifecycle_content, &format!("{location}.lifecycle_content"))?;
    }
    Ok(())
}

/// Requires one producer-owned path to be present and non-blank.
/// 要求一条产出方路径存在且不为空白。
fn require_non_empty_path<'a>(
    path: Option<&'a str>,
    location: &str,
) -> Result<&'a str, PersistentChangeSetError> {
    path.filter(|value| !value.trim().is_empty())
        .ok_or_else(|| PersistentChangeSetError::new(location, "expected one non-empty path"))
}

/// Rejects a path field that is forbidden for the selected change kind.
/// 拒绝当前变更类型不允许出现的路径字段。
fn reject_present_path(path: Option<&str>, location: &str) -> Result<(), PersistentChangeSetError> {
    if path.is_some() {
        return Err(PersistentChangeSetError::new(
            location,
            "field is not allowed for this change kind",
        ));
    }
    Ok(())
}

/// Validates one hunk and its independently ordered deleted and inserted line domains.
/// 校验一条 hunk 及其彼此独立有序的删除行和插入行域。
fn validate_persistent_hunk(
    hunk: &PersistentChangeSetHunk,
    location: &str,
) -> Result<(), PersistentChangeSetError> {
    if hunk.deleted.is_empty() && hunk.inserted.is_empty() {
        return Err(PersistentChangeSetError::new(
            location,
            "hunk must contain at least one deleted or inserted line",
        ));
    }
    validate_line_sequence(&hunk.deleted, &format!("{location}.deleted"))?;
    validate_line_sequence(&hunk.inserted, &format!("{location}.inserted"))?;
    Ok(())
}

/// Validates one line domain as positive, strictly increasing, and duplicate-free.
/// 将一个行号域校验为正数、严格递增且无重复。
fn validate_line_sequence(
    lines: &[PersistentChangeSetLine],
    location: &str,
) -> Result<(), PersistentChangeSetError> {
    let mut previous = None;
    for (line_index, line) in lines.iter().enumerate() {
        let line_location = format!("{location}[{line_index}].line");
        if line.line == 0 {
            return Err(PersistentChangeSetError::new(
                line_location,
                "line numbers are one-based and must be positive",
            ));
        }
        if previous.is_some_and(|previous_line| line.line <= previous_line) {
            return Err(PersistentChangeSetError::new(
                line_location,
                "line numbers must be strictly increasing without duplicates",
            ));
        }
        previous = Some(line.line);
    }
    Ok(())
}

/// Validates lifecycle body metadata whose complete text can be checked exactly.
/// 校验可以依据完整正文精确核对的生命周期正文元数据。
fn validate_lifecycle_content(
    lifecycle_content: &PersistentLifecycleContent,
    location: &str,
) -> Result<(), PersistentChangeSetError> {
    if let PersistentLifecycleContent::Full {
        content,
        total_line_count,
    } = lifecycle_content
    {
        let normalized_content = normalize_text(content);
        let actual_line_count = count_change_set_content_lines(&normalized_content);
        if *total_line_count != actual_line_count {
            return Err(PersistentChangeSetError::new(
                format!("{location}.total_line_count"),
                format!(
                    "declared {total_line_count} lines but complete content contains {actual_line_count}"
                ),
            ));
        }
    }
    Ok(())
}

/// Parses one required change-set mode.
/// 解析一条必填的 change-set 模式。
fn parse_change_set_mode(
    value: Option<&Value>,
    location: &str,
) -> Result<ChangeSetMode, PersistentChangeSetError> {
    match value.and_then(Value::as_str) {
        Some("preview") => Ok(ChangeSetMode::Preview),
        Some("applied") => Ok(ChangeSetMode::Applied),
        _ => Err(PersistentChangeSetError::new(
            location,
            "expected `preview` or `applied`",
        )),
    }
}

/// Parses one optional string while rejecting a present non-string value.
/// 解析一条可选字符串，并拒绝已出现的非字符串值。
fn parse_optional_string(
    value: Option<&Value>,
    location: &str,
) -> Result<Option<String>, PersistentChangeSetError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(PersistentChangeSetError::new(
            location,
            "expected a string or null",
        )),
    }
}

/// Parses one optional boolean while rejecting a present non-boolean value.
/// 解析一条可选布尔值，并拒绝已出现的非布尔值。
fn parse_optional_bool(
    value: Option<&Value>,
    location: &str,
) -> Result<Option<bool>, PersistentChangeSetError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(flag)) => Ok(Some(*flag)),
        Some(_) => Err(PersistentChangeSetError::new(
            location,
            "expected a boolean or null",
        )),
    }
}

/// Parses one optional platform-sized unsigned integer without truncation.
/// 无截断地解析一条可选平台宽度无符号整数。
fn parse_optional_usize(
    value: Option<&Value>,
    location: &str,
) -> Result<Option<usize>, PersistentChangeSetError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| usize::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| {
                PersistentChangeSetError::new(location, "expected a non-negative exact integer")
            }),
    }
}

/// Parses one required platform-sized unsigned integer without truncation.
/// 无截断地解析一条必填的平台宽度无符号整数。
fn parse_required_usize(
    value: Option<&Value>,
    location: &str,
) -> Result<usize, PersistentChangeSetError> {
    parse_optional_usize(value, location)?.ok_or_else(|| {
        PersistentChangeSetError::new(location, "expected a non-negative exact integer")
    })
}

/// Parses one exact file-level change kind.
/// 解析一条精确的文件级变更类型。
fn parse_change_kind(
    value: Option<&Value>,
    location: &str,
) -> Result<ChangeSetFileChangeKind, PersistentChangeSetError> {
    match value.and_then(Value::as_str) {
        Some("create") => Ok(ChangeSetFileChangeKind::Create),
        Some("modify") => Ok(ChangeSetFileChangeKind::Modify),
        Some("delete") => Ok(ChangeSetFileChangeKind::Delete),
        Some("rename") => Ok(ChangeSetFileChangeKind::Rename),
        _ => Err(PersistentChangeSetError::new(
            location,
            "expected `create`, `modify`, `delete`, or `rename`",
        )),
    }
}

/// Parses all exact text-format rows from the registered native file-tools extension.
/// 从已注册的原生文件工具扩展中解析全部精确文本格式记录。
fn parse_resource_formats(
    extensions: Option<&Value>,
) -> Result<ResourceFormatMap, PersistentChangeSetError> {
    let Some(extensions) = extensions else {
        return Ok(BTreeMap::new());
    };
    let extensions = extensions.as_object().ok_or_else(|| {
        PersistentChangeSetError::new("payload.extensions", "expected one object")
    })?;
    let Some(extension) = extensions.get("vulcan.file_tools.v2") else {
        return Ok(BTreeMap::new());
    };
    let resources = extension
        .as_object()
        .and_then(|object| object.get("resources"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PersistentChangeSetError::new(
                "payload.extensions.vulcan.file_tools.v2.resources",
                "expected one array",
            )
        })?;
    let mut formats = BTreeMap::new();
    for (index, resource) in resources.iter().enumerate() {
        let location = format!("payload.extensions.vulcan.file_tools.v2.resources[{index}]");
        let object = resource
            .as_object()
            .ok_or_else(|| PersistentChangeSetError::new(&location, "expected one object"))?;
        let resource_id = object
            .get("resource_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                PersistentChangeSetError::new(
                    format!("{location}.resource_id"),
                    "expected one non-empty string",
                )
            })?;
        let before = parse_text_format(
            object.get("before_text_format"),
            &format!("{location}.before_text_format"),
        )?;
        let after = parse_text_format(
            object.get("after_text_format"),
            &format!("{location}.after_text_format"),
        )?;
        if formats
            .insert(resource_id.to_string(), (before, after))
            .is_some()
        {
            return Err(PersistentChangeSetError::new(
                format!("{location}.resource_id"),
                "duplicate resource identifier",
            ));
        }
    }
    Ok(formats)
}

/// Parses one optional exact text-format snapshot.
/// 解析一份可选的精确文本格式快照。
fn parse_text_format(
    value: Option<&Value>,
    location: &str,
) -> Result<Option<PersistentTextFormat>, PersistentChangeSetError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| PersistentChangeSetError::new(location, error.to_string()))
}

/// Parses one complete persistent file record without filtering malformed hunks.
/// 完整解析一条持久化文件记录，不过滤格式错误的 hunk。
fn parse_persistent_file(
    value: &Value,
    index: usize,
    resource_formats: &ResourceFormatMap,
) -> Result<PersistentChangeSetFile, PersistentChangeSetError> {
    let location = format!("payload.files[{index}]");
    let object = value
        .as_object()
        .ok_or_else(|| PersistentChangeSetError::new(&location, "expected one object"))?;
    let change = parse_change_kind(object.get("change"), &format!("{location}.change"))?;
    let resource_id = parse_optional_string(
        object.get("resource_id"),
        &format!("{location}.resource_id"),
    )?;
    let path = parse_optional_string(object.get("path"), &format!("{location}.path"))?;
    let old_path = parse_optional_string(object.get("old_path"), &format!("{location}.old_path"))?;
    let new_path = parse_optional_string(object.get("new_path"), &format!("{location}.new_path"))?;
    let hunks = parse_persistent_hunks(object.get("hunks"), &format!("{location}.hunks"))?;
    let lifecycle_content = parse_lifecycle_content(object, &location)?;
    let (extension_before, extension_after) = resource_id
        .as_ref()
        .and_then(|resource_id| resource_formats.get(resource_id))
        .cloned()
        .unwrap_or((None, None));
    let direct_format = parse_direct_file_text_format(object, &location)?;
    let (before_text_format, after_text_format) = match change {
        ChangeSetFileChangeKind::Create => (extension_before, extension_after.or(direct_format)),
        ChangeSetFileChangeKind::Delete => (extension_before.or(direct_format), extension_after),
        ChangeSetFileChangeKind::Rename => (extension_before.or(direct_format), extension_after),
        ChangeSetFileChangeKind::Modify => (extension_before, extension_after),
    };
    Ok(PersistentChangeSetFile {
        resource_id,
        change,
        path,
        old_path,
        new_path,
        hunks,
        lifecycle_content,
        before_text_format,
        after_text_format,
    })
}

/// Parses lifecycle content exactly as declared by the producer.
/// 按产出方声明精确解析生命周期正文。
fn parse_lifecycle_content(
    object: &serde_json::Map<String, Value>,
    location: &str,
) -> Result<Option<PersistentLifecycleContent>, PersistentChangeSetError> {
    let Some(mode) = object.get("content_mode") else {
        return Ok(None);
    };
    match mode.as_str() {
        Some("full") => Ok(Some(PersistentLifecycleContent::Full {
            content: object
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistentChangeSetError::new(
                        format!("{location}.content"),
                        "full lifecycle content requires one string",
                    )
                })?
                .to_string(),
            total_line_count: parse_required_usize(
                object.get("total_line_count"),
                &format!("{location}.total_line_count"),
            )?,
        })),
        Some("truncated") => Ok(Some(PersistentLifecycleContent::Truncated {
            head: object
                .get("content_head")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistentChangeSetError::new(
                        format!("{location}.content_head"),
                        "truncated lifecycle content requires one head string",
                    )
                })?
                .to_string(),
            tail: object
                .get("content_tail")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistentChangeSetError::new(
                        format!("{location}.content_tail"),
                        "truncated lifecycle content requires one tail string",
                    )
                })?
                .to_string(),
            total_line_count: parse_required_usize(
                object.get("total_line_count"),
                &format!("{location}.total_line_count"),
            )?,
        })),
        Some("omitted") => Ok(Some(PersistentLifecycleContent::Omitted {
            total_line_count: parse_optional_usize(
                object.get("total_line_count"),
                &format!("{location}.total_line_count"),
            )?,
        })),
        _ => Err(PersistentChangeSetError::new(
            format!("{location}.content_mode"),
            "expected `full`, `truncated`, or `omitted`",
        )),
    }
}

/// Parses file-level text metadata emitted directly by lifecycle producers.
/// 解析生命周期产出方直接发出的文件级文本元数据。
fn parse_direct_file_text_format(
    object: &serde_json::Map<String, Value>,
    location: &str,
) -> Result<Option<PersistentTextFormat>, PersistentChangeSetError> {
    let encoding = parse_optional_string(object.get("encoding"), &format!("{location}.encoding"))?;
    let bom = parse_optional_bool(object.get("bom"), &format!("{location}.bom"))?;
    let line_ending = parse_optional_string(
        object.get("line_ending"),
        &format!("{location}.line_ending"),
    )?;
    if encoding.is_none() && bom.is_none() && line_ending.is_none() {
        return Ok(None);
    }
    Ok(Some(PersistentTextFormat {
        encoding,
        encoding_source: None,
        encoding_certain: None,
        bom,
        line_ending,
        preferred_line_ending: None,
        mixed_line_endings: None,
        final_newline: None,
    }))
}

/// Parses all modify hunks from one file record.
/// 从一条文件记录中解析全部 modify hunk。
fn parse_persistent_hunks(
    value: Option<&Value>,
    location: &str,
) -> Result<Vec<PersistentChangeSetHunk>, PersistentChangeSetError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| PersistentChangeSetError::new(location, "expected one array"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| parse_persistent_hunk(value, &format!("{location}[{index}]")))
        .collect()
}

/// Parses one modify hunk with exact inserted and deleted rows.
/// 解析一条带精确新增与删除行的 modify hunk。
fn parse_persistent_hunk(
    value: &Value,
    location: &str,
) -> Result<PersistentChangeSetHunk, PersistentChangeSetError> {
    let object = value
        .as_object()
        .ok_or_else(|| PersistentChangeSetError::new(location, "expected one object"))?;
    Ok(PersistentChangeSetHunk {
        before: parse_optional_string(object.get("before"), &format!("{location}.before"))?
            .unwrap_or_default(),
        after: parse_optional_string(object.get("after"), &format!("{location}.after"))?
            .unwrap_or_default(),
        deleted: parse_persistent_lines(object.get("delete"), &format!("{location}.delete"))?,
        inserted: parse_persistent_lines(object.get("insert"), &format!("{location}.insert"))?,
    })
}

/// Parses one ordered inserted or deleted line array.
/// 解析一组有序新增行或删除行数组。
fn parse_persistent_lines(
    value: Option<&Value>,
    location: &str,
) -> Result<Vec<PersistentChangeSetLine>, PersistentChangeSetError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| PersistentChangeSetError::new(location, "expected one array"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let row_location = format!("{location}[{index}]");
            let object = value.as_object().ok_or_else(|| {
                PersistentChangeSetError::new(&row_location, "expected one object")
            })?;
            let line = parse_required_usize(object.get("line"), &format!("{row_location}.line"))?;
            if line == 0 {
                return Err(PersistentChangeSetError::new(
                    format!("{row_location}.line"),
                    "line numbers are one-based",
                ));
            }
            let content = object
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistentChangeSetError::new(
                        format!("{row_location}.content"),
                        "expected one string",
                    )
                })?
                .to_string();
            Ok(PersistentChangeSetLine { line, content })
        })
        .collect()
}

/// Parses all producer diagnostics without dropping malformed rows.
/// 解析全部产出方诊断，不丢弃格式错误的记录。
fn parse_persistent_diagnostics(
    value: Option<&Value>,
) -> Result<Vec<PersistentChangeSetDiagnostic>, PersistentChangeSetError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        PersistentChangeSetError::new("payload.diagnostics", "expected one array")
    })?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let location = format!("payload.diagnostics[{index}]");
            let object = value
                .as_object()
                .ok_or_else(|| PersistentChangeSetError::new(&location, "expected one object"))?;
            let level = object
                .get("level")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistentChangeSetError::new(
                        format!("{location}.level"),
                        "expected one string",
                    )
                })?
                .to_string();
            let message = object
                .get("message")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistentChangeSetError::new(
                        format!("{location}.message"),
                        "expected one string",
                    )
                })?
                .to_string();
            Ok(PersistentChangeSetDiagnostic { level, message })
        })
        .collect()
}

/// Derives aggregate change statistics directly from the persisted hunks.
/// 直接从持久化 hunk 推导聚合变更统计。
///
/// The `+/-` counters of a tool card must come from this same persisted hunk
/// structure; expanding or collapsing a card never re-parses another text.
/// 工具卡的 `+/-` 计数必须来自同一份持久化 hunk 结构；卡片展开或折叠
/// 永远不再解析另一份文本。
pub fn persistent_change_set_stats(changeset: &PersistentChangeSet) -> ChangeSetStats {
    let mut stats = ChangeSetStats {
        file_count: changeset.files.len(),
        create_count: 0,
        modify_count: 0,
        delete_count: 0,
        rename_count: 0,
        inserted_line_count: 0,
        deleted_line_count: 0,
    };
    for file in &changeset.files {
        match file.change {
            ChangeSetFileChangeKind::Create => {
                stats.create_count += 1;
                stats.inserted_line_count = stats.inserted_line_count.saturating_add(
                    file.lifecycle_content
                        .as_ref()
                        .and_then(PersistentLifecycleContent::total_line_count)
                        .unwrap_or(0),
                );
            }
            ChangeSetFileChangeKind::Modify => stats.modify_count += 1,
            ChangeSetFileChangeKind::Delete => {
                stats.delete_count += 1;
                stats.deleted_line_count = stats.deleted_line_count.saturating_add(
                    file.lifecycle_content
                        .as_ref()
                        .and_then(PersistentLifecycleContent::total_line_count)
                        .unwrap_or(0),
                );
            }
            ChangeSetFileChangeKind::Rename => stats.rename_count += 1,
        }
        if matches!(
            file.change,
            ChangeSetFileChangeKind::Modify | ChangeSetFileChangeKind::Rename
        ) {
            for hunk in &file.hunks {
                stats.inserted_line_count = stats
                    .inserted_line_count
                    .saturating_add(hunk.inserted.len());
                stats.deleted_line_count =
                    stats.deleted_line_count.saturating_add(hunk.deleted.len());
            }
        }
    }
    stats
}
