use serde_json::Value;

/// Compact modal-ready summary derived from one canonical `change_set` payload.
/// 从一份 canonical `change_set` 载荷派生出的紧凑弹层摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSetModalSummary {
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

/// Aggregated change counters shown by the WebUI and used for compact inline summaries.
/// 供WebUI展示并用于紧凑内联摘要的聚合变更计数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSetStats {
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
pub struct ChangeSetFileSummary {
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
pub struct ChangeSetDiagnosticSummary {
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
pub struct ChangeSetVerificationSummary {
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
pub enum ChangeSetVerificationStatus {
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Verified => "verified",
            Self::Warning => "warning",
        }
    }

    /// Parses one compact verification-status string.
    /// 解析一条紧凑校验状态字符串。
    pub(super) fn parse(value: &str) -> Option<Self> {
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
pub enum ChangeSetMode {
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Applied => "applied",
        }
    }

    /// Parses one canonical `change_set.mode` string.
    /// 解析一条 canonical `change_set.mode` 字符串。
    pub(super) fn parse(value: &str) -> Option<Self> {
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
pub enum ChangeSetFileChangeKind {
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
            Self::Rename => "rename",
        }
    }

    /// Parses one file-level change kind string.
    /// 解析一条文件级变更类型字符串。
    pub(super) fn parse(value: &str) -> Option<Self> {
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
pub enum ChangeSetDeleteContentMode {
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
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Truncated => "truncated",
        }
    }

    /// Parses one delete content-mode field while treating absent values as the legacy full mode.
    /// 解析删除内容模式字段，并把缺失值视为旧版全文模式。
    pub(super) fn parse(value: Option<&Value>) -> Option<Self> {
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
pub(super) struct RawChangeSetLineEntry {
    /// One-based line number reported by the producer.
    /// 由产出方上报的 1-based 行号。
    pub(super) line: usize,
    /// Exact text content carried by the line entry.
    /// 行记录携带的精确文本内容。
    pub(super) content: String,
}

/// One raw modify hunk preserved by a canonical file record.
/// canonical 文件记录保留的一条原始 modify hunk。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RawChangeSetHunk {
    /// Stable leading context text before the edited block.
    /// 编辑区块之前的稳定前置上下文文本。
    pub(super) before: String,
    /// Stable trailing context text after the edited block.
    /// 编辑区块之后的稳定后置上下文文本。
    pub(super) after: String,
    /// Deleted source lines carried by the hunk.
    /// hunk 携带的删除源行集合。
    pub(super) deleted: Vec<RawChangeSetLineEntry>,
    /// Inserted destination lines carried by the hunk.
    /// hunk 携带的插入目标行集合。
    pub(super) inserted: Vec<RawChangeSetLineEntry>,
}

/// One raw canonical file record parsed from the producer payload.
/// 从产出载荷解析得到的一条原始 canonical 文件记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RawChangeSetFileRecord {
    /// Stable file-level change kind.
    /// 稳定的文件级变更类型。
    pub(super) change: ChangeSetFileChangeKind,
    /// Optional primary file path.
    /// 可选主文件路径。
    pub(super) path: Option<String>,
    /// Optional rename source path.
    /// 可选重命名源路径。
    pub(super) old_path: Option<String>,
    /// Optional rename destination path.
    /// 可选重命名目标路径。
    pub(super) new_path: Option<String>,
    /// Optional file content used by create/delete lifecycle records.
    /// 供 create/delete 生命周期记录使用的可选文件内容。
    pub(super) content: Option<String>,
    /// Optional delete content mode reported by delete lifecycle records.
    /// 删除生命周期记录上报的可选删除内容模式。
    pub(super) delete_content_mode: Option<ChangeSetDeleteContentMode>,
    /// Optional total logical line count reported by lifecycle records.
    /// 生命周期记录上报的可选总逻辑行数。
    pub(super) total_line_count: Option<usize>,
    /// Parsed modify hunks of the current file record.
    /// 当前文件记录的已解析 modify hunk 集合。
    pub(super) hunks: Vec<RawChangeSetHunk>,
    /// Whether the source record exposed one optional patch field.
    /// 源记录是否暴露了可选 patch 字段。
    pub(super) patch_present: bool,
}
