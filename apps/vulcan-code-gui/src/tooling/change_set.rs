//! Host-side helpers for structured `change_set` tool results.
//! 结构化 `change_set` 工具结果的宿主侧辅助逻辑。

use serde_json::{Map, Value, json};
use std::fs;
use std::path::Path;

/// Stable structured host-result kind used by IDE-style file edit tools.
/// IDE 风格文件编辑工具使用的稳定结构化宿主结果类型。
pub const CHANGE_SET_HOST_RESULT_KIND: &str = "change_set";

/// Shared payload byte ceiling announced to LuaSkills so file-edit tools can opt into structured host results without flooding the host metadata channel.
/// 向 LuaSkills 公布的共享载荷字节上限，使文件编辑工具可以启用结构化宿主结果，同时避免淹没宿主元数据通道。
pub const LUASKILLS_CHANGE_SET_HOST_RESULT_MAX_PAYLOAD_BYTES: u64 = 512 * 1024;

/// Compact modal-ready summary derived from one canonical `change_set` payload.
/// 从一份 canonical `change_set` 载荷派生出的紧凑弹层摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeSetModalSummary {
    /// Whether the source payload represents a preview-only edit plan or one already-applied filesystem mutation.
    /// 源载荷表示的是仅预览的编辑计划，还是已经落盘的文件系统变更。
    pub mode: ChangeSetMode,
    /// Optional tool-provided human-readable summary.
    /// 工具提供的可选人类可读摘要。
    pub summary: Option<String>,
    /// Aggregated per-change counters extracted from the payload.
    /// 从载荷中提取出的按变更类型聚合计数。
    pub stats: ChangeSetStats,
    /// Compact file rows preserved for tool cards, tool modals, and persistence.
    /// 为工具卡、工具弹层与持久化保留的紧凑文件行。
    pub files: Vec<ChangeSetFileSummary>,
    /// Optional diagnostics emitted by the producing tool.
    /// 产出工具发出的可选诊断信息。
    pub diagnostics: Vec<ChangeSetDiagnosticSummary>,
    /// Host-side verification outcome derived from the current filesystem state.
    /// 根据当前文件系统状态推导出的宿主侧校验结果。
    pub verification: ChangeSetVerificationSummary,
}

/// Aggregated change counters shown by the GUI and used for compact inline summaries.
/// 供图形界面展示并用于紧凑内联摘要的聚合变更计数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeSetStats {
    /// Total file record count carried by the payload.
    /// 载荷携带的文件记录总数。
    pub file_count: usize,
    /// Number of `create` file records.
    /// `create` 文件记录数量。
    pub create_count: usize,
    /// Number of `modify` file records.
    /// `modify` 文件记录数量。
    pub modify_count: usize,
    /// Number of `delete` file records.
    /// `delete` 文件记录数量。
    pub delete_count: usize,
    /// Number of `rename` file records.
    /// `rename` 文件记录数量。
    pub rename_count: usize,
    /// Total inserted line count aggregated across modify hunks.
    /// 在全部 modify hunk 中聚合得到的插入行总数。
    pub inserted_line_count: usize,
    /// Total deleted line count aggregated across modify hunks.
    /// 在全部 modify hunk 中聚合得到的删除行总数。
    pub deleted_line_count: usize,
}

/// Compact per-file summary row shown inside the tool modal.
/// 展示在工具弹层中的紧凑单文件摘要行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeSetFileSummary {
    /// Stable file change kind.
    /// 稳定的文件变更类型。
    pub change: ChangeSetFileChangeKind,
    /// Optional primary path of the file record.
    /// 文件记录的可选主路径。
    pub path: Option<String>,
    /// Optional rename source path.
    /// 重命名源路径的可选值。
    pub old_path: Option<String>,
    /// Optional rename destination path.
    /// 重命名目标路径的可选值。
    pub new_path: Option<String>,
    /// Number of modify hunks preserved for the file record.
    /// 为当前文件记录保留的 modify hunk 数量。
    pub hunk_count: usize,
    /// Inserted line count aggregated for the file record.
    /// 为当前文件记录聚合得到的插入行数。
    pub inserted_line_count: usize,
    /// Deleted line count aggregated for the file record.
    /// 为当前文件记录聚合得到的删除行数。
    pub deleted_line_count: usize,
    /// Optional text byte count derived from create/delete content.
    /// 从 create/delete 内容推导出的可选文本字节数。
    pub content_bytes: Option<usize>,
    /// Optional logical line count derived from create/delete content.
    /// 从 create/delete 内容推导出的可选逻辑行数。
    pub content_lines: Option<usize>,
    /// Optional delete content mode preserved for deleted-file lifecycle records.
    /// 为删除文件生命周期记录保留的可选删除内容模式。
    pub delete_content_mode: Option<ChangeSetDeleteContentMode>,
    /// Whether the original file record exposed one optional patch field.
    /// 原始文件记录是否暴露了可选 patch 字段。
    pub patch_present: bool,
}

/// Compact diagnostic row derived from the original payload.
/// 从原始载荷派生出的紧凑诊断行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeSetDiagnosticSummary {
    /// Diagnostic level reported by the producer.
    /// 产出方上报的诊断级别。
    pub level: String,
    /// Human-readable diagnostic message.
    /// 人类可读的诊断消息。
    pub message: String,
}

/// Host-side verification snapshot attached to compact `change_set` payloads.
/// 附着在紧凑 `change_set` 载荷上的宿主侧校验快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeSetVerificationSummary {
    /// High-level verification status reported by the host.
    /// 宿主上报的高层校验状态。
    pub status: ChangeSetVerificationStatus,
    /// Number of file records the host attempted to verify against the filesystem.
    /// 宿主尝试对照文件系统校验的文件记录数量。
    pub checked_file_count: usize,
    /// Human-readable warning strings describing suspicious mismatches.
    /// 描述可疑不一致之处的人类可读警告字符串集合。
    pub warnings: Vec<String>,
}

/// High-level verification status used by compact `change_set` summaries.
/// 紧凑 `change_set` 摘要使用的高层校验状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeSetVerificationStatus {
    /// The payload is preview-only, so no filesystem verification is attempted.
    /// 载荷仅代表预览，因此不会尝试文件系统校验。
    Skipped,
    /// The host verified every checked file record without finding suspicious mismatches.
    /// 宿主完成了全部已检查文件记录的校验，且未发现可疑不一致。
    Verified,
    /// The host found one or more suspicious mismatches while checking the payload.
    /// 宿主在检查载荷时发现了一条或多条可疑不一致。
    Warning,
}

impl ChangeSetVerificationStatus {
    /// Returns the stable lowercase string used inside compact payload metadata.
    /// 返回紧凑载荷元数据内部使用的稳定小写字符串。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Verified => "verified",
            Self::Warning => "warning",
        }
    }

    /// Parses one compact verification-status string.
    /// 解析一条紧凑校验状态字符串。
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "skipped" => Some(Self::Skipped),
            "verified" => Some(Self::Verified),
            "warning" => Some(Self::Warning),
            _ => None,
        }
    }
}

/// Canonical `change_set` mode declared by the producing tool.
/// 产出工具声明的 canonical `change_set` 模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeSetMode {
    /// The payload only previews edits that have not been applied yet.
    /// 当前载荷仅预览尚未落盘的编辑操作。
    Preview,
    /// The payload claims the edits have already been applied to disk.
    /// 当前载荷声明这些编辑已经落盘生效。
    Applied,
}

impl ChangeSetMode {
    /// Returns the stable lowercase serialization value used by LuaSkills.
    /// 返回 LuaSkills 使用的稳定小写序列化值。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Applied => "applied",
        }
    }

    /// Parses one canonical `change_set.mode` string.
    /// 解析一条 canonical `change_set.mode` 字符串。
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "preview" => Some(Self::Preview),
            "applied" => Some(Self::Applied),
            _ => None,
        }
    }
}

/// Supported file-level change kinds inside one compact `change_set` summary.
/// 单个紧凑 `change_set` 摘要中支持的文件级变更类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeSetFileChangeKind {
    /// One newly created file.
    /// 一份新创建的文件。
    Create,
    /// One modified existing file.
    /// 一份被修改的既有文件。
    Modify,
    /// One deleted file.
    /// 一份被删除的文件。
    Delete,
    /// One renamed file.
    /// 一份被重命名的文件。
    Rename,
}

impl ChangeSetFileChangeKind {
    /// Returns the stable lowercase serialization value.
    /// 返回稳定的小写序列化值。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
            Self::Rename => "rename",
        }
    }

    /// Parses one file-level change kind string.
    /// 解析一条文件级变更类型字符串。
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "create" => Some(Self::Create),
            "modify" => Some(Self::Modify),
            "delete" => Some(Self::Delete),
            "rename" => Some(Self::Rename),
            _ => None,
        }
    }
}

/// Stable delete content mode used by canonical `change_set` delete lifecycle records.
/// canonical `change_set` 删除生命周期记录使用的稳定删除内容模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeSetDeleteContentMode {
    /// The deleted file body stays inline as full text.
    /// 被删除文件正文以内联全文形式保留。
    Full,
    /// The deleted file body is intentionally summarized into head and tail snippets.
    /// 被删除文件正文会被有意摘要为前后片段。
    Truncated,
}

impl ChangeSetDeleteContentMode {
    /// Returns the stable lowercase serialization value.
    /// 返回稳定的小写序列化值。
    fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Truncated => "truncated",
        }
    }

    /// Parses one delete content-mode field while treating absent values as the legacy full mode.
    /// 解析删除内容模式字段，并把缺失值视为旧版全文模式。
    fn parse(value: Option<&Value>) -> Option<Self> {
        match value.and_then(Value::as_str).map(str::trim) {
            None | Some("") | Some("full") => Some(Self::Full),
            Some("truncated") => Some(Self::Truncated),
            _ => None,
        }
    }
}

/// One raw line entry preserved inside a canonical modify hunk.
/// 保存在 canonical modify hunk 中的一条原始行记录。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawChangeSetLineEntry {
    /// One-based line number reported by the producer.
    /// 由产出方上报的 1-based 行号。
    line: usize,
    /// Exact text content carried by the line entry.
    /// 行记录携带的精确文本内容。
    content: String,
}

/// One raw modify hunk preserved by a canonical file record.
/// canonical 文件记录保留的一条原始 modify hunk。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawChangeSetHunk {
    /// Stable leading context text before the edited block.
    /// 编辑区块之前的稳定前置上下文文本。
    before: String,
    /// Stable trailing context text after the edited block.
    /// 编辑区块之后的稳定后置上下文文本。
    after: String,
    /// Deleted source lines carried by the hunk.
    /// hunk 携带的删除源行集合。
    deleted: Vec<RawChangeSetLineEntry>,
    /// Inserted destination lines carried by the hunk.
    /// hunk 携带的插入目标行集合。
    inserted: Vec<RawChangeSetLineEntry>,
}

/// One raw canonical file record parsed from the producer payload.
/// 从产出载荷解析得到的一条原始 canonical 文件记录。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawChangeSetFileRecord {
    /// Stable file-level change kind.
    /// 稳定的文件级变更类型。
    change: ChangeSetFileChangeKind,
    /// Optional primary file path.
    /// 可选主文件路径。
    path: Option<String>,
    /// Optional rename source path.
    /// 可选重命名源路径。
    old_path: Option<String>,
    /// Optional rename destination path.
    /// 可选重命名目标路径。
    new_path: Option<String>,
    /// Optional file content used by create/delete lifecycle records.
    /// 供 create/delete 生命周期记录使用的可选文件内容。
    content: Option<String>,
    /// Optional delete content mode reported by delete lifecycle records.
    /// 删除生命周期记录上报的可选删除内容模式。
    delete_content_mode: Option<ChangeSetDeleteContentMode>,
    /// Optional total logical line count reported by lifecycle records.
    /// 生命周期记录上报的可选总逻辑行数。
    total_line_count: Option<usize>,
    /// Parsed modify hunks of the current file record.
    /// 当前文件记录的已解析 modify hunk 集合。
    hunks: Vec<RawChangeSetHunk>,
    /// Whether the source record exposed one optional patch field.
    /// 源记录是否暴露了可选 patch 字段。
    patch_present: bool,
}

/// Builds the host-result capability payload injected into LuaSkills request contexts.
/// 构建注入到 LuaSkills 请求上下文中的宿主结果能力载荷。
pub(crate) fn luaskills_change_set_host_result_capabilities() -> Value {
    json!({
        "host_result": {
            "enabled": true,
            "allowed_kinds": [CHANGE_SET_HOST_RESULT_KIND],
            "max_payload_bytes": LUASKILLS_CHANGE_SET_HOST_RESULT_MAX_PAYLOAD_BYTES,
        }
    })
}

/// Compacts one raw `change_set` host-result object into the minimal shape needed by transcript projection, persistence, and tool-modal summaries.
/// 将一份原始 `change_set` 宿主结果对象压缩成转录投影、持久化与工具弹层摘要所需的最小形态。
pub(crate) fn compact_change_set_host_result(host_result: &Value) -> Option<Value> {
    let object = host_result.as_object()?;
    if object.get("kind").and_then(Value::as_str) != Some(CHANGE_SET_HOST_RESULT_KIND) {
        return None;
    }
    let payload = object.get("payload")?;
    let compact_payload = compact_change_set_payload(payload)?;
    Some(json!({
        "kind": CHANGE_SET_HOST_RESULT_KIND,
        "payload": compact_payload,
    }))
}

/// Derives one modal-ready `change_set` summary from either a raw producer payload or an already-compacted host payload.
/// 从原始产出载荷或已压缩宿主载荷中派生一份可供弹层使用的 `change_set` 摘要。
pub(crate) fn change_set_modal_summary_from_payload(
    payload: &Value,
) -> Option<ChangeSetModalSummary> {
    let compact_payload = compact_change_set_payload(payload)?;
    let object = compact_payload.as_object()?;
    let mode = ChangeSetMode::parse(object.get("mode")?.as_str()?)?;
    let summary = object
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let stats = parse_compact_stats(object.get("stats")?)?;
    let files = object
        .get("files")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_compact_file_summary)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let diagnostics = object
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_compact_diagnostic_summary)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let verification = parse_compact_verification(object.get("verification")?)?;
    Some(ChangeSetModalSummary {
        mode,
        summary,
        stats,
        files,
        diagnostics,
        verification,
    })
}

/// Builds one English host-side warning notice from the current raw `change_set` payload when the just-applied filesystem state looks suspicious.
/// 当当前原始 `change_set` 载荷对应的刚应用文件系统状态看起来可疑时，构建一条英文宿主侧警告提示。
pub(crate) fn change_set_host_verification_notice(payload: &Value) -> Option<String> {
    let object = payload.as_object()?;
    let mode = ChangeSetMode::parse(object.get("mode")?.as_str()?)?;
    if mode != ChangeSetMode::Applied {
        return None;
    }
    let raw_files = object.get("files").and_then(Value::as_array)?;
    let files = raw_files
        .iter()
        .filter_map(parse_raw_file_record)
        .collect::<Vec<_>>();
    if files.is_empty() && !raw_files.is_empty() {
        return None;
    }
    let verification = verify_change_set_records(mode, &files);
    if verification.status != ChangeSetVerificationStatus::Warning {
        return None;
    }
    let mut lines = vec![String::from(
        "Host verification warning: the reported applied change_set could not be fully verified against the current filesystem state.",
    )];
    for warning in verification.warnings.iter().take(6) {
        lines.push(format!("- {warning}"));
    }
    Some(lines.join("\n"))
}

/// Compacts one raw or already-compacted payload into the stable compact payload shape reused by transcript projection and persistence without retaining mutable filesystem verification state.
/// 将原始载荷或已压缩载荷规范化为稳定的紧凑载荷形态，供转录投影与持久化复用，且不保留会随文件变化而漂移的文件系统校验状态。
fn compact_change_set_payload(payload: &Value) -> Option<Value> {
    let object = payload.as_object()?;
    if object.get("stats").is_some()
        && object.get("verification").is_some()
        && object.get("files").and_then(Value::as_array).is_some()
        && object
            .get("mode")
            .and_then(Value::as_str)
            .and_then(ChangeSetMode::parse)
            .is_some()
    {
        let mode = ChangeSetMode::parse(object.get("mode")?.as_str()?)?;
        let mut compact = object.clone();
        compact.insert(
            String::from("verification"),
            compact_verification_value(&build_persisted_verification_summary(mode)),
        );
        return Some(Value::Object(compact));
    }

    let mode = ChangeSetMode::parse(object.get("mode")?.as_str()?)?;
    let raw_files = object.get("files").and_then(Value::as_array)?;
    let files = raw_files
        .iter()
        .filter_map(parse_raw_file_record)
        .collect::<Vec<_>>();
    if files.is_empty() && !raw_files.is_empty() {
        return None;
    }

    let stats = build_change_set_stats(&files);
    let verification = build_persisted_verification_summary(mode);
    let compact_files = files.iter().map(compact_file_record).collect::<Vec<_>>();
    let diagnostics = compact_diagnostic_values(object.get("diagnostics"));

    let mut compact = Map::new();
    compact.insert(
        String::from("mode"),
        Value::String(mode.as_str().to_string()),
    );
    if let Some(summary) = object
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact.insert(String::from("summary"), Value::String(summary.to_string()));
    }
    compact.insert(String::from("files"), Value::Array(compact_files));
    compact.insert(String::from("stats"), compact_stats_value(&stats));
    if !diagnostics.is_empty() {
        compact.insert(String::from("diagnostics"), Value::Array(diagnostics));
    }
    compact.insert(
        String::from("verification"),
        compact_verification_value(&verification),
    );
    Some(Value::Object(compact))
}

/// Builds the stable verification snapshot persisted with compact `change_set` payloads so later transcript rendering never reinterprets mutable filesystem drift as a historical tool failure.
/// 构建与紧凑 `change_set` 载荷一同持久化的稳定校验快照，避免后续转录渲染把可变文件系统漂移误解成历史工具失败。
fn build_persisted_verification_summary(_mode: ChangeSetMode) -> ChangeSetVerificationSummary {
    ChangeSetVerificationSummary {
        status: ChangeSetVerificationStatus::Skipped,
        checked_file_count: 0,
        warnings: Vec::new(),
    }
}

/// Parses one raw canonical file record emitted by the producing tool.
/// 解析一条由产出工具发出的原始 canonical 文件记录。
fn parse_raw_file_record(file: &Value) -> Option<RawChangeSetFileRecord> {
    let object = file.as_object()?;
    let change = ChangeSetFileChangeKind::parse(object.get("change")?.as_str()?)?;
    let path = compact_optional_string(object.get("path"));
    let old_path = compact_optional_string(object.get("old_path"));
    let new_path = compact_optional_string(object.get("new_path"));
    let content = object
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_string);
    let delete_content_mode = if change == ChangeSetFileChangeKind::Delete {
        ChangeSetDeleteContentMode::parse(object.get("content_mode"))
    } else {
        None
    };
    let total_line_count = compact_optional_usize(object.get("total_line_count"));
    let hunks = object
        .get("hunks")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_raw_change_set_hunk)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(RawChangeSetFileRecord {
        change,
        path,
        old_path,
        new_path,
        content,
        delete_content_mode,
        total_line_count,
        hunks,
        patch_present: object
            .get("patch")
            .is_some_and(|value| !value.is_null() && value.is_string()),
    })
}

/// Parses one raw canonical modify hunk emitted by the producing tool.
/// 解析一条由产出工具发出的原始 canonical modify hunk。
fn parse_raw_change_set_hunk(hunk: &Value) -> Option<RawChangeSetHunk> {
    let object = hunk.as_object()?;
    Some(RawChangeSetHunk {
        before: object
            .get("before")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        after: object
            .get("after")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        deleted: parse_raw_line_entries(object.get("delete")),
        inserted: parse_raw_line_entries(object.get("insert")),
    })
}

/// Parses one ordered delete/insert line-entry array.
/// 解析一组有序的 delete/insert 行记录数组。
fn parse_raw_line_entries(value: Option<&Value>) -> Vec<RawChangeSetLineEntry> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| {
                    let object = entry.as_object()?;
                    Some(RawChangeSetLineEntry {
                        line: object
                            .get("line")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok())?,
                        content: object.get("content").and_then(Value::as_str)?.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Aggregates raw file records into one compact counter snapshot.
/// 将原始文件记录聚合为一份紧凑计数快照。
fn build_change_set_stats(files: &[RawChangeSetFileRecord]) -> ChangeSetStats {
    let mut stats = ChangeSetStats {
        file_count: files.len(),
        create_count: 0,
        modify_count: 0,
        delete_count: 0,
        rename_count: 0,
        inserted_line_count: 0,
        deleted_line_count: 0,
    };
    for file in files {
        match file.change {
            ChangeSetFileChangeKind::Create => stats.create_count += 1,
            ChangeSetFileChangeKind::Modify => stats.modify_count += 1,
            ChangeSetFileChangeKind::Delete => stats.delete_count += 1,
            ChangeSetFileChangeKind::Rename => stats.rename_count += 1,
        }
        for hunk in &file.hunks {
            stats.inserted_line_count += hunk.inserted.len();
            stats.deleted_line_count += hunk.deleted.len();
        }
    }
    stats
}

/// Verifies one raw file-record set against the current filesystem and returns one host-side warning snapshot.
/// 对照当前文件系统校验一组原始文件记录，并返回一份宿主侧警告快照。
fn verify_change_set_records(
    mode: ChangeSetMode,
    files: &[RawChangeSetFileRecord],
) -> ChangeSetVerificationSummary {
    if mode == ChangeSetMode::Preview {
        return ChangeSetVerificationSummary {
            status: ChangeSetVerificationStatus::Skipped,
            checked_file_count: 0,
            warnings: Vec::new(),
        };
    }

    let mut checked_file_count = 0_usize;
    let mut warnings = Vec::new();
    for file in files {
        match file.change {
            ChangeSetFileChangeKind::Create => {
                if let Some(path) = file.path.as_deref() {
                    checked_file_count += 1;
                    verify_created_file(path, file.content.as_deref(), &mut warnings);
                }
            }
            ChangeSetFileChangeKind::Modify => {
                if let Some(path) = file.path.as_deref() {
                    checked_file_count += 1;
                    verify_modified_file(path, &file.hunks, &mut warnings);
                }
            }
            ChangeSetFileChangeKind::Delete => {
                if let Some(path) = file.path.as_deref() {
                    checked_file_count += 1;
                    verify_deleted_file(path, &mut warnings);
                }
            }
            ChangeSetFileChangeKind::Rename => {
                if let (Some(old_path), Some(new_path)) =
                    (file.old_path.as_deref(), file.new_path.as_deref())
                {
                    checked_file_count += 1;
                    verify_renamed_file(old_path, new_path, &mut warnings);
                }
            }
        }
    }

    let status = if warnings.is_empty() {
        ChangeSetVerificationStatus::Verified
    } else {
        ChangeSetVerificationStatus::Warning
    };
    ChangeSetVerificationSummary {
        status,
        checked_file_count,
        warnings,
    }
}

/// Verifies one created file against the expected on-disk state.
/// 对照预期落盘状态校验一份创建文件。
fn verify_created_file(path: &str, expected_content: Option<&str>, warnings: &mut Vec<String>) {
    let file_path = Path::new(path);
    if !file_path.exists() {
        warnings.push(format!("Created file is missing: {}", file_path.display()));
        return;
    }
    let Some(expected_content) = expected_content else {
        return;
    };
    match fs::read(file_path) {
        Ok(actual_bytes) => {
            if !file_bytes_match_expected_text(&actual_bytes, expected_content) {
                warnings.push(format!(
                    "Created file content does not match the reported change_set payload: {}",
                    file_path.display()
                ));
            }
        }
        Err(error) => warnings.push(format!(
            "Created file exists but could not be read for verification ({}): {}",
            file_path.display(),
            error
        )),
    }
}

/// Verifies one deleted file against the expected absence on disk.
/// 对照预期的磁盘缺失状态校验一份删除文件。
fn verify_deleted_file(path: &str, warnings: &mut Vec<String>) {
    let file_path = Path::new(path);
    if file_path.exists() {
        warnings.push(format!(
            "Deleted file still exists on disk: {}",
            file_path.display()
        ));
    }
}

/// Verifies one rename record against the expected old/new path state on disk.
/// 对照预期旧路径和新路径状态校验一条重命名记录。
fn verify_renamed_file(old_path: &str, new_path: &str, warnings: &mut Vec<String>) {
    let old_file = Path::new(old_path);
    let new_file = Path::new(new_path);
    if old_file.exists() {
        warnings.push(format!(
            "Renamed source path still exists on disk: {}",
            old_file.display()
        ));
    }
    if !new_file.exists() {
        warnings.push(format!(
            "Renamed destination path is missing on disk: {}",
            new_file.display()
        ));
    }
}

/// Verifies one modified file using exact inserted-line checks plus best-effort surrounding-context checks.
/// 通过精确插入行校验和尽力而为的上下文校验来验证一份修改文件。
fn verify_modified_file(path: &str, hunks: &[RawChangeSetHunk], warnings: &mut Vec<String>) {
    let file_path = Path::new(path);
    if !file_path.exists() {
        warnings.push(format!("Modified file is missing: {}", file_path.display()));
        return;
    }
    let actual_bytes = match fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            warnings.push(format!(
                "Modified file could not be read for verification ({}): {}",
                file_path.display(),
                error
            ));
            return;
        }
    };
    let actual_text = match std::str::from_utf8(&actual_bytes) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!(
                "Modified file could not be decoded as UTF-8 for hunk verification ({}): {}",
                file_path.display(),
                error
            ));
            return;
        }
    };
    let normalized_text = normalize_text(actual_text);
    let normalized_lines = split_normalized_lines(&normalized_text);

    for (hunk_index, hunk) in hunks.iter().enumerate() {
        if !hunk.before.trim().is_empty()
            && !normalized_text.contains(normalize_text(&hunk.before).as_str())
        {
            warnings.push(format!(
                "Modify hunk {} is missing its reported leading context in {}",
                hunk_index + 1,
                file_path.display()
            ));
        }
        if !hunk.after.trim().is_empty()
            && !normalized_text.contains(normalize_text(&hunk.after).as_str())
        {
            warnings.push(format!(
                "Modify hunk {} is missing its reported trailing context in {}",
                hunk_index + 1,
                file_path.display()
            ));
        }
        for inserted in &hunk.inserted {
            let actual_line = normalized_lines
                .get(inserted.line.saturating_sub(1))
                .map(String::as_str)
                .unwrap_or_default();
            if actual_line != normalize_text(&inserted.content) {
                warnings.push(format!(
                    "Modify hunk {} inserted line {} does not match the current file content in {}",
                    hunk_index + 1,
                    inserted.line,
                    file_path.display()
                ));
            }
        }
    }
}

/// Converts one raw file record into the compact JSON row persisted by the host.
/// 将一条原始文件记录转换成由宿主持久化的紧凑 JSON 行。
fn compact_file_record(file: &RawChangeSetFileRecord) -> Value {
    let mut object = Map::new();
    object.insert(
        String::from("change"),
        Value::String(file.change.as_str().to_string()),
    );
    if let Some(path) = file.path.as_ref() {
        object.insert(String::from("path"), Value::String(path.clone()));
    }
    if let Some(old_path) = file.old_path.as_ref() {
        object.insert(String::from("old_path"), Value::String(old_path.clone()));
    }
    if let Some(new_path) = file.new_path.as_ref() {
        object.insert(String::from("new_path"), Value::String(new_path.clone()));
    }
    let inserted_line_count = file
        .hunks
        .iter()
        .map(|hunk| hunk.inserted.len())
        .sum::<usize>();
    let deleted_line_count = file
        .hunks
        .iter()
        .map(|hunk| hunk.deleted.len())
        .sum::<usize>();
    if !file.hunks.is_empty() {
        object.insert(String::from("hunk_count"), json!(file.hunks.len()));
    }
    if inserted_line_count > 0 {
        object.insert(
            String::from("inserted_line_count"),
            json!(inserted_line_count),
        );
    }
    if deleted_line_count > 0 {
        object.insert(
            String::from("deleted_line_count"),
            json!(deleted_line_count),
        );
    }
    match file.change {
        ChangeSetFileChangeKind::Create => {
            if let Some(content) = file.content.as_deref() {
                let normalized_content = normalize_text(content);
                object.insert(
                    String::from("content_bytes"),
                    json!(normalized_content.len()),
                );
                object.insert(
                    String::from("content_lines"),
                    json!(count_change_set_content_lines(&normalized_content)),
                );
            }
        }
        ChangeSetFileChangeKind::Delete => {
            let delete_content_mode = file
                .delete_content_mode
                .unwrap_or(ChangeSetDeleteContentMode::Full);
            object.insert(
                String::from("content_mode"),
                Value::String(delete_content_mode.as_str().to_string()),
            );
            if let Some(total_line_count) = file.total_line_count {
                object.insert(String::from("content_lines"), json!(total_line_count));
            }
            if delete_content_mode == ChangeSetDeleteContentMode::Full
                && let Some(content) = file.content.as_deref()
            {
                let normalized_content = normalize_text(content);
                object.insert(
                    String::from("content_bytes"),
                    json!(normalized_content.len()),
                );
                if file.total_line_count.is_none() {
                    object.insert(
                        String::from("content_lines"),
                        json!(count_change_set_content_lines(&normalized_content)),
                    );
                }
            }
        }
        ChangeSetFileChangeKind::Modify | ChangeSetFileChangeKind::Rename => {}
    }
    if file.patch_present {
        object.insert(String::from("patch_present"), Value::Bool(true));
    }
    Value::Object(object)
}

/// Parses one compact file row back into the typed summary used by the tool modal.
/// 将一条紧凑文件行解析回工具弹层使用的带类型摘要。
fn parse_compact_file_summary(file: &Value) -> Option<ChangeSetFileSummary> {
    let object = file.as_object()?;
    let change = ChangeSetFileChangeKind::parse(object.get("change")?.as_str()?)?;
    Some(ChangeSetFileSummary {
        change,
        path: compact_optional_string(object.get("path")),
        old_path: compact_optional_string(object.get("old_path")),
        new_path: compact_optional_string(object.get("new_path")),
        hunk_count: compact_usize(object.get("hunk_count")),
        inserted_line_count: compact_usize(object.get("inserted_line_count")),
        deleted_line_count: compact_usize(object.get("deleted_line_count")),
        content_bytes: compact_optional_usize(object.get("content_bytes")),
        content_lines: compact_optional_usize(object.get("content_lines")),
        delete_content_mode: if change == ChangeSetFileChangeKind::Delete {
            ChangeSetDeleteContentMode::parse(object.get("content_mode"))
        } else {
            None
        },
        patch_present: object
            .get("patch_present")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Compacts one optional diagnostics array into the host-owned minimal JSON shape.
/// 将可选 diagnostics 数组压缩为宿主自有的最小 JSON 形态。
fn compact_diagnostic_values(value: Option<&Value>) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|diagnostic| {
                    let object = diagnostic.as_object()?;
                    let level = compact_optional_string(object.get("level"))?;
                    let message = compact_optional_string(object.get("message"))?;
                    Some(json!({
                        "level": level,
                        "message": message,
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Parses one compact diagnostic row into the typed modal summary shape.
/// 将一条紧凑诊断行解析为带类型的弹层摘要形态。
fn parse_compact_diagnostic_summary(value: &Value) -> Option<ChangeSetDiagnosticSummary> {
    let object = value.as_object()?;
    Some(ChangeSetDiagnosticSummary {
        level: compact_optional_string(object.get("level"))?,
        message: compact_optional_string(object.get("message"))?,
    })
}

/// Converts one counter snapshot into the compact JSON shape persisted by the host.
/// 将一份计数快照转换成由宿主持久化的紧凑 JSON 形态。
fn compact_stats_value(stats: &ChangeSetStats) -> Value {
    json!({
        "file_count": stats.file_count,
        "create_count": stats.create_count,
        "modify_count": stats.modify_count,
        "delete_count": stats.delete_count,
        "rename_count": stats.rename_count,
        "inserted_line_count": stats.inserted_line_count,
        "deleted_line_count": stats.deleted_line_count,
    })
}

/// Parses one compact stats object back into the typed summary shape.
/// 将一份紧凑 stats 对象解析回带类型的摘要形态。
fn parse_compact_stats(value: &Value) -> Option<ChangeSetStats> {
    let object = value.as_object()?;
    Some(ChangeSetStats {
        file_count: compact_usize(object.get("file_count")),
        create_count: compact_usize(object.get("create_count")),
        modify_count: compact_usize(object.get("modify_count")),
        delete_count: compact_usize(object.get("delete_count")),
        rename_count: compact_usize(object.get("rename_count")),
        inserted_line_count: compact_usize(object.get("inserted_line_count")),
        deleted_line_count: compact_usize(object.get("deleted_line_count")),
    })
}

/// Converts one host-side verification snapshot into the compact JSON shape persisted by the host.
/// 将一份宿主侧校验快照转换成由宿主持久化的紧凑 JSON 形态。
fn compact_verification_value(verification: &ChangeSetVerificationSummary) -> Value {
    json!({
        "status": verification.status.as_str(),
        "checked_file_count": verification.checked_file_count,
        "warning_count": verification.warnings.len(),
        "warnings": verification.warnings,
    })
}

/// Parses one compact verification object back into the typed summary shape.
/// 将一份紧凑校验对象解析回带类型的摘要形态。
fn parse_compact_verification(value: &Value) -> Option<ChangeSetVerificationSummary> {
    let object = value.as_object()?;
    let warnings = object
        .get("warnings")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(ChangeSetVerificationSummary {
        status: ChangeSetVerificationStatus::parse(object.get("status")?.as_str()?)?,
        checked_file_count: compact_usize(object.get("checked_file_count")),
        warnings,
    })
}

/// Returns one trimmed non-empty string clone when present.
/// 当存在时返回一份去首尾空白后的非空字符串克隆。
fn compact_optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Returns one required-or-zero unsigned counter value from compact JSON.
/// 从紧凑 JSON 中返回一条必填或默认零的无符号计数值。
fn compact_usize(value: Option<&Value>) -> usize {
    compact_optional_usize(value).unwrap_or(0)
}

/// Returns one optional unsigned counter value from compact JSON.
/// 从紧凑 JSON 中返回一条可选无符号计数值。
fn compact_optional_usize(value: Option<&Value>) -> Option<usize> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

/// Returns whether raw file bytes match the expected text exactly or after newline normalization.
/// 返回原始文件字节是否与期望文本完全一致，或在换行规范化后保持一致。
fn file_bytes_match_expected_text(actual_bytes: &[u8], expected_text: &str) -> bool {
    if actual_bytes == expected_text.as_bytes() {
        return true;
    }
    std::str::from_utf8(actual_bytes)
        .map(|text| normalize_text(text) == normalize_text(expected_text))
        .unwrap_or(false)
}

/// Normalizes mixed newline sequences into `\n` so textual comparisons stay deterministic across platforms.
/// 将混合换行序列规范化为 `\n`，使跨平台文本比较保持确定性。
fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Splits normalized text into logical lines while preserving empty trailing results when they matter for line counts.
/// 将规范化后的文本拆分为逻辑行，并在影响行数时保留尾部空结果。
fn split_normalized_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').map(str::to_string).collect()
    }
}

/// Counts logical lifecycle-content lines while ignoring one purely trailing newline marker.
/// 统计生命周期内容的逻辑行数，并忽略一个纯粹表示文件结尾的尾随换行。
fn count_change_set_content_lines(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut line_count = text.split('\n').count();
    if text.ends_with('\n') {
        line_count = line_count.saturating_sub(1);
    }
    line_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Verifies LuaSkills host-result capabilities explicitly enable only the canonical `change_set` bridge surface.
    /// 验证 LuaSkills 宿主结果能力会显式启用且仅暴露 canonical `change_set` 桥接面。
    #[test]
    fn luaskills_capabilities_enable_change_set_only() {
        let capabilities = luaskills_change_set_host_result_capabilities();
        assert_eq!(capabilities["host_result"]["enabled"], Value::Bool(true));
        assert_eq!(
            capabilities["host_result"]["allowed_kinds"],
            json!(["change_set"])
        );
        assert_eq!(
            capabilities["host_result"]["max_payload_bytes"],
            json!(LUASKILLS_CHANGE_SET_HOST_RESULT_MAX_PAYLOAD_BYTES)
        );
    }

    /// Verifies raw canonical `change_set` payloads are compacted into summary-ready metadata with aggregated counters while keeping filesystem verification transient to the current run.
    /// 验证原始 canonical `change_set` 载荷会被压缩成带聚合计数的摘要元数据，同时让文件系统校验仅保留为当前运行期的瞬时提示。
    #[test]
    fn compact_change_set_host_result_builds_summary_metadata() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current timestamp")
            .as_millis();
        let root =
            std::env::temp_dir().join(format!("vulcan-change-set-helper-test-{unique_suffix}"));
        fs::create_dir_all(&root).expect("create test root");
        let modified_path = root.join("modified.lua");
        let created_path = root.join("created.lua");
        let deleted_path = root.join("deleted.lua");
        let renamed_old_path = root.join("old.lua");
        let renamed_new_path = root.join("new.lua");
        fs::write(&modified_path, "local value = 2\nreturn value\n").expect("write modified file");
        fs::write(&created_path, "return created\n").expect("write created file");
        fs::write(&deleted_path, "return deleted\n").expect("write deleted file");
        fs::write(&renamed_old_path, "return old\n").expect("write rename source file");
        fs::write(&renamed_new_path, "return renamed\n").expect("write renamed file");

        let host_result = json!({
            "kind": "change_set",
            "payload": {
                "mode": "applied",
                "summary": "Applied file edits.",
                "files": [
                    {
                        "change": "modify",
                        "path": modified_path.display().to_string(),
                        "hunks": [{
                            "before": "local value = 1",
                            "delete": [{ "line": 1, "content": "local value = 1" }],
                            "insert": [{ "line": 1, "content": "local value = 2" }],
                            "after": "return value"
                        }]
                    },
                    {
                        "change": "create",
                        "path": created_path.display().to_string(),
                        "content": "return created\n"
                    },
                    {
                        "change": "delete",
                        "path": deleted_path.display().to_string(),
                        "content": "return deleted\n"
                    },
                    {
                        "change": "rename",
                        "old_path": renamed_old_path.display().to_string(),
                        "new_path": renamed_new_path.display().to_string()
                    }
                ],
                "diagnostics": [{
                    "level": "warning",
                    "message": "One warning"
                }]
            }
        });

        let compacted =
            compact_change_set_host_result(&host_result).expect("change_set host result");
        let payload = compacted["payload"].clone();
        let summary =
            change_set_modal_summary_from_payload(&payload).expect("compact summary should parse");
        let runtime_notice = change_set_host_verification_notice(&host_result["payload"])
            .expect("runtime notice should still reflect the current filesystem state");
        assert_eq!(summary.mode, ChangeSetMode::Applied);
        assert_eq!(summary.stats.file_count, 4);
        assert_eq!(summary.stats.modify_count, 1);
        assert_eq!(summary.stats.create_count, 1);
        assert_eq!(summary.stats.delete_count, 1);
        assert_eq!(summary.stats.rename_count, 1);
        assert_eq!(summary.stats.inserted_line_count, 1);
        assert_eq!(summary.stats.deleted_line_count, 1);
        assert_eq!(summary.files.len(), 4);
        assert_eq!(
            summary.files[2].delete_content_mode,
            Some(ChangeSetDeleteContentMode::Full)
        );
        assert_eq!(summary.files[2].content_lines, Some(1));
        assert_eq!(summary.diagnostics.len(), 1);
        assert_eq!(
            summary.verification.status,
            ChangeSetVerificationStatus::Skipped
        );
        assert_eq!(summary.verification.checked_file_count, 0);
        assert!(summary.verification.warnings.is_empty());
        assert!(runtime_notice.contains("Deleted file still exists"));
        assert!(runtime_notice.contains("Renamed source path still exists"));

        let _ = fs::remove_dir_all(&root);
    }

    /// Verifies truncated delete lifecycle records preserve the large-delete summary without requiring one inline full body.
    /// 验证截断 delete 生命周期记录会保留大删除摘要，而不再要求内联完整正文。
    #[test]
    fn compact_change_set_host_result_preserves_truncated_delete_summary_metadata() {
        let host_result = json!({
            "kind": "change_set",
            "payload": {
                "mode": "applied",
                "summary": "Applied one large file deletion.",
                "files": [{
                    "change": "delete",
                    "path": "D:/demo/large-delete.lua",
                    "content_mode": "truncated",
                    "total_line_count": 12345,
                    "content_head": "deleted line 1\ndeleted line 2",
                    "content_tail": "deleted line 12344\ndeleted line 12345"
                }]
            }
        });

        let compacted =
            compact_change_set_host_result(&host_result).expect("change_set host result");
        let payload = compacted["payload"].clone();
        let summary =
            change_set_modal_summary_from_payload(&payload).expect("compact summary should parse");
        assert_eq!(summary.stats.file_count, 1);
        assert_eq!(summary.stats.delete_count, 1);
        assert_eq!(summary.files.len(), 1);
        assert_eq!(
            summary.files[0].delete_content_mode,
            Some(ChangeSetDeleteContentMode::Truncated)
        );
        assert_eq!(summary.files[0].content_lines, Some(12345));
        assert_eq!(summary.files[0].content_bytes, None);
    }

    /// Verifies preview payloads skip filesystem verification and therefore do not emit warning notices.
    /// 验证预览载荷会跳过文件系统校验，因此不会产出警告提示。
    #[test]
    fn preview_change_set_skips_verification_notice() {
        let payload = json!({
            "mode": "preview",
            "files": [{
                "change": "create",
                "path": "D:/demo.txt",
                "content": "demo"
            }]
        });
        let summary =
            change_set_modal_summary_from_payload(&payload).expect("preview summary should parse");
        assert_eq!(
            summary.verification.status,
            ChangeSetVerificationStatus::Skipped
        );
        assert!(change_set_host_verification_notice(&payload).is_none());
    }
}
