//! Public knowledge-base domain models shared by storage, service, and AI tooling.
//! 由存储、服务与 AI 工具共享的资料库领域模型。

use serde::{Deserialize, Serialize};

/// Stable format identifier stored in every portable knowledge package.
/// 每个可移植资料库包中存储的稳定格式标识。
pub const KNOWLEDGE_PACKAGE_FORMAT: &str = "vulcan-code-knowledge-base";

/// Current portable knowledge package version.
/// 当前可移植资料库包版本。
pub const KNOWLEDGE_PACKAGE_VERSION: u32 = 1;

/// Maximum accepted portable package archive size.
/// 可接受的可移植资料库归档最大字节数。
pub const MAX_KNOWLEDGE_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Portable metadata stored as package.json inside one .vkb archive.
/// 单个 .vkb 归档内以 package.json 保存的可移植元数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageManifest {
    /// Stable package format identifier.
    /// 稳定包格式标识。
    pub format: String,
    /// Portable package protocol version.
    /// 可移植包协议版本。
    pub package_version: u32,
    /// Source library identity retained only for provenance and validation.
    /// 仅用于来源追踪和校验的原资料库标识。
    pub source_library_id: String,
    /// Original user-visible library name.
    /// 原始用户可见资料库名称。
    pub name: String,
    /// Original user-visible library description.
    /// 原始用户可见资料库说明。
    pub description: String,
    /// Authoritative per-library SQLite schema version.
    /// 权威单库 SQLite 结构版本。
    pub schema_version: i64,
    /// Authoritative document count captured at export.
    /// 导出时捕获的权威文档数量。
    pub document_count: u64,
    /// Authoritative chunk count captured at export.
    /// 导出时捕获的权威切片数量。
    pub chunk_count: u64,
    /// Exported derived-index state.
    /// 导出的派生索引状态。
    pub index_status: String,
    /// Unix timestamp when the package was created.
    /// 创建资料库包时的 Unix 毫秒时间戳。
    pub exported_at_ms: i64,
    /// Lowercase SHA-256 digest of library/content.sqlite3.
    /// library/content.sqlite3 的小写 SHA-256 摘要。
    pub content_sha256: String,
    /// Exact uncompressed SQLite byte size.
    /// SQLite 文件的精确未压缩字节数。
    pub content_bytes: u64,
}

/// Result returned after one package is exported.
/// 一个资料库包导出后返回的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageExportResult {
    /// Final native archive path selected by the host.
    /// 宿主选择的最终原生归档路径。
    pub archive_path: String,
    /// Portable package metadata written into the archive.
    /// 写入归档的可移植包元数据。
    pub package: KnowledgePackageManifest,
}

/// Read-only inspection result for one candidate package.
/// 一个候选资料库包的只读检查结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageInspection {
    /// Canonical native archive path accepted by the host.
    /// 宿主接受的规范原生归档路径。
    pub archive_path: String,
    /// Validated portable package metadata.
    /// 已校验的可移植包元数据。
    pub package: KnowledgePackageManifest,
    /// Existing active library with the same normalized name, when present.
    /// 存在时返回同规范名称的现有活动资料库。
    pub conflicting_library: Option<KnowledgeLibrarySummary>,
}

/// Result returned after one package import or replacement.
/// 一个资料库包导入或替换后返回的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageImportResult {
    /// Imported or replaced authoritative library summary.
    /// 已导入或替换的权威资料库摘要。
    pub library: KnowledgeLibrarySummary,
    /// Whether an existing physical library was explicitly replaced.
    /// 是否显式替换了现有物理资料库。
    pub replaced_existing: bool,
    /// Non-fatal cleanup or post-commit warnings retained after a successful import.
    /// 成功导入后保留的非致命清理或提交后告警。
    pub warnings: Vec<String>,
}

/// One custom Jieba word stored inside a physical library database.
/// 存储在物理资料库数据库中的一个 Jieba 自定义词。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCustomWord {
    /// Exact custom word text.
    /// 精确的自定义词文本。
    pub word: String,
    /// Jieba dictionary weight.
    /// Jieba 词典权重。
    pub weight: u32,
}

/// One project-scoped knowledge library shown to users and runtime policy code.
/// 展示给用户和运行时策略代码的一个项目级资料库。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibrarySummary {
    /// Immutable UUID string identifying the library.
    /// 标识资料库的不可变 UUID 字符串。
    pub id: String,
    /// User-visible library name.
    /// 用户可见的资料库名称。
    pub name: String,
    /// Optional user-visible library description.
    /// 可选的用户可见资料库说明。
    pub description: String,
    /// Durable lifecycle state such as active, deleting, or error.
    /// 持久化生命周期状态，例如 active、deleting 或 error。
    pub lifecycle_state: String,
    /// Number of authoritative Markdown documents.
    /// 权威 Markdown 文档数量。
    pub document_count: u64,
    /// Number of authoritative searchable chunks.
    /// 权威可检索切片数量。
    pub chunk_count: u64,
    /// Derived FTS index state.
    /// 派生 FTS 索引状态。
    pub index_status: String,
    /// Unix timestamp of the most recent completed import.
    /// 最近一次完成导入的 Unix 毫秒时间戳。
    pub last_completed_import_at_ms: Option<i64>,
    /// Unix timestamp when the library was created.
    /// 资料库创建时的 Unix 毫秒时间戳。
    pub created_at_ms: i64,
    /// Unix timestamp when the library metadata last changed.
    /// 资料库元数据最近变更时的 Unix 毫秒时间戳。
    pub updated_at_ms: i64,
}

/// One durable Markdown source directory attached to a knowledge library.
/// 关联到一个资料库的持久化 Markdown 来源目录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceSummary {
    /// Immutable source identifier retained when the physical directory is remapped.
    /// 物理目录重新映射时保持不变的来源标识符。
    pub id: String,
    /// Canonical native directory path currently assigned to the source.
    /// 当前分配给该来源的规范原生目录路径。
    pub canonical_root: String,
    /// User-visible directory name derived from the canonical root.
    /// 从规范根目录派生的用户可见目录名称。
    pub display_name: String,
    /// Number of authoritative Markdown documents currently owned by the source.
    /// 当前由该来源持有的权威 Markdown 文档数量。
    pub document_count: u64,
    /// Number of authoritative searchable chunks currently owned by the source.
    /// 当前由该来源持有的权威可检索切片数量。
    pub chunk_count: u64,
    /// Unix timestamp when the durable source identity was created.
    /// 持久化来源身份创建时的 Unix 毫秒时间戳。
    pub created_at_ms: i64,
    /// Unix timestamp when the source directory was last scanned.
    /// 来源目录最近一次扫描时的 Unix 毫秒时间戳。
    pub last_scanned_at_ms: i64,
}

/// Stable kind for one durable knowledge import or index-maintenance operation.
/// 一次持久化资料导入或索引维护操作的稳定类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeOperationKind {
    /// Recursive Markdown directory import and incremental index synchronization.
    /// 递归 Markdown 目录导入与增量索引同步。
    Import,
    /// Non-destructive completion of missing, mismatched, or orphaned derived FTS rows.
    /// 非破坏性补全缺失、错配或孤立的派生 FTS 记录。
    Repair,
    /// Destructive FTS-only rebuild from all authoritative chunks.
    /// 从全部权威切片执行仅针对 FTS 的破坏性重建。
    Rebuild,
}

impl KnowledgeOperationKind {
    /// Returns the stable database and wire representation for this operation kind.
    /// 返回该操作类型稳定的数据库与线协议表示。
    ///
    /// Returns: one of `import`, `repair`, or `rebuild`.
    /// 返回值：`import`、`repair` 或 `rebuild` 之一。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Repair => "repair",
            Self::Rebuild => "rebuild",
        }
    }

    /// Parses one exact persisted operation kind without accepting aliases.
    /// 解析一个精确的持久化操作类型，不接受别名。
    ///
    /// Parameters: `value` is the database text to validate.
    /// Returns: the typed operation kind or an explicit integrity error.
    /// 参数：`value` 是待校验的数据库文本。返回值：强类型操作类型或明确的完整性错误。
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "import" => Ok(Self::Import),
            "repair" => Ok(Self::Repair),
            "rebuild" => Ok(Self::Rebuild),
            _ => Err(format!("未知的资料库操作类型：{value}")),
        }
    }
}

/// Exact metadata and physical-row integrity snapshot for one derived Jieba FTS index.
/// 一个派生 Jieba FTS 索引的精确元数据与物理记录完整性快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeIndexIntegritySummary {
    /// Immutable library identifier inspected by the runtime.
    /// 运行时检查的不可变资料库标识符。
    pub library_id: String,
    /// Whether the physical FTS virtual table currently exists.
    /// 物理 FTS 虚拟表当前是否存在。
    pub index_exists: bool,
    /// Whether every fail-closed metadata and physical integrity condition currently passes.
    /// 当前是否通过全部失败关闭元数据与物理完整性条件。
    pub searchable: bool,
    /// Persisted index lifecycle state.
    /// 持久化索引生命周期状态。
    pub index_status: String,
    /// Configured tokenizer identifier.
    /// 配置的分词器标识符。
    pub tokenizer: String,
    /// Current authoritative content revision.
    /// 当前权威内容版本。
    pub content_revision: u64,
    /// Content revision represented by the FTS index.
    /// FTS 索引表示的内容版本。
    pub indexed_content_revision: u64,
    /// Current Jieba dictionary revision.
    /// 当前 Jieba 词典版本。
    pub dictionary_revision: u64,
    /// Dictionary revision represented by the FTS index.
    /// FTS 索引表示的词典版本。
    pub indexed_dictionary_revision: u64,
    /// Number of authoritative Markdown chunks.
    /// 权威 Markdown 切片数量。
    pub authoritative_chunk_count: u64,
    /// Number of physical FTS rows.
    /// 物理 FTS 记录数量。
    pub indexed_chunk_count: u64,
    /// Authoritative chunks without one exact matching FTS source row.
    /// 没有精确匹配 FTS 源记录的权威切片数量。
    pub missing_or_mismatched_count: u64,
    /// Excess duplicate, orphaned, or source-mismatched physical FTS rows.
    /// 冗余重复、孤立或源字段错配的物理 FTS 记录数量。
    pub orphan_or_mismatched_count: u64,
}

/// One explicit authorization binding between a library and a project.
/// 资料库与项目之间的一条显式授权绑定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectBinding {
    /// Immutable library identifier.
    /// 不可变资料库标识符。
    pub library_id: String,
    /// Immutable project identifier.
    /// 不可变项目标识符。
    pub project_id: String,
    /// Whether inherited session selection enables this library by default.
    /// 继承型会话选择是否默认启用此资料库。
    pub default_enabled: bool,
}

/// Persisted summary for one imported Markdown document.
/// 一份已导入 Markdown 文档的持久化摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentSummary {
    /// Immutable document identifier.
    /// 不可变文档标识符。
    pub id: String,
    /// Owning source identifier.
    /// 所属来源标识符。
    pub source_id: String,
    /// Slash-normalized path relative to the imported source root.
    /// 相对于导入来源根目录、以斜杠规范化的路径。
    pub relative_path: String,
    /// Slash-normalized folder classification derived from the source tree.
    /// 从来源目录树派生的斜杠规范化文件夹分类。
    pub folder_path: String,
    /// Deterministic document title.
    /// 确定性的文档标题。
    pub title: String,
    /// Source byte size.
    /// 来源字节大小。
    pub byte_size: u64,
    /// Source line count after newline normalization.
    /// 换行规范化后的来源行数。
    pub line_count: u64,
    /// Number of searchable chunks owned by the document.
    /// 文档拥有的可检索切片数量。
    pub chunk_count: u64,
    /// Unix timestamp when the authoritative row last changed.
    /// 权威记录最近变更时的 Unix 毫秒时间戳。
    pub updated_at_ms: i64,
}

/// Full authoritative content returned for one knowledge document.
/// 返回给调用方的一份资料文档完整权威内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentContent {
    /// Document metadata.
    /// 文档元数据。
    pub document: KnowledgeDocumentSummary,
    /// Newline-normalized UTF-8 Markdown content.
    /// 换行规范化后的 UTF-8 Markdown 内容。
    pub content: String,
}

/// One authoritative Markdown chunk and its source citation.
/// 一个权威 Markdown 切片及其来源引用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunk {
    /// Immutable chunk identifier used by FTS.
    /// FTS 使用的不可变切片标识符。
    pub id: String,
    /// Owning document identifier.
    /// 所属文档标识符。
    pub document_id: String,
    /// Zero-based deterministic ordinal inside the document.
    /// 文档内从零开始的确定性序号。
    pub ordinal: u64,
    /// Slash-separated Markdown heading ancestry.
    /// 以斜杠分隔的 Markdown 标题层级。
    pub heading_path: String,
    /// One-based inclusive first source line.
    /// 从一开始且包含边界的来源起始行。
    pub start_line: u64,
    /// One-based inclusive last source line.
    /// 从一开始且包含边界的来源结束行。
    pub end_line: u64,
    /// Original Markdown slice.
    /// 原始 Markdown 切片。
    pub content: String,
    /// Mixed-language estimated token count.
    /// 中英文混合估算 token 数量。
    pub estimated_tokens: u64,
}

/// One fully materialized BM25 search hit.
/// 一条完成权威数据物化的 BM25 检索命中。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchHit {
    /// Library containing the hit.
    /// 命中所属资料库标识符。
    pub library_id: String,
    /// User-visible library name.
    /// 用户可见资料库名称。
    pub library_name: String,
    /// Document containing the hit.
    /// 命中所属文档标识符。
    pub document_id: String,
    /// Chunk identifier used by FTS.
    /// FTS 使用的切片标识符。
    pub chunk_id: String,
    /// Document title.
    /// 文档标题。
    pub title: String,
    /// Relative source path.
    /// 相对来源路径。
    pub relative_path: String,
    /// Markdown heading ancestry.
    /// Markdown 标题层级。
    pub heading_path: String,
    /// One-based inclusive first source line.
    /// 从一开始且包含边界的来源起始行。
    pub start_line: u64,
    /// One-based inclusive last source line.
    /// 从一开始且包含边界的来源结束行。
    pub end_line: u64,
    /// Authoritative Markdown chunk content.
    /// 权威 Markdown 切片内容。
    pub content: String,
    /// Stable lexical match strategy, either `bm25` or `substring`.
    /// 稳定的词法匹配策略，只能是 `bm25` 或 `substring`。
    pub match_kind: String,
    /// Per-library BM25 score where larger is better.
    /// 单资料库内越大越优的 BM25 分数；子串补充命中固定为零。
    pub bm25_score: f64,
    /// Optional reciprocal-rank-fusion score used only across libraries.
    /// 仅跨资料库融合时使用的可选倒数排名融合分数。
    pub rrf_score: Option<f64>,
}

/// One library-level search failure retained in a federated response.
/// 联合检索响应中保留的一条资料库级失败。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchFailure {
    /// Library that failed.
    /// 失败的资料库标识符。
    pub library_id: String,
    /// Explicit failure detail.
    /// 显式失败详情。
    pub error: String,
}

/// One exact FTS MATCH expression generated by vldb for a physical library.
/// vldb 为一个物理资料库生成的一条精确 FTS MATCH 表达式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFtsQuery {
    /// Library whose dictionary produced the expression.
    /// 其词典生成该表达式的资料库。
    pub library_id: String,
    /// Safe Jieba-tokenized expression sent to SQLite FTS5.
    /// 发送给 SQLite FTS5 的安全 Jieba 分词表达式。
    pub expression: String,
}

/// Federated BM25 response with explicit partial failures.
/// 带显式部分失败信息的联合 BM25 响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchResult {
    /// Original non-empty query.
    /// 原始非空查询。
    pub query: String,
    /// Ranked, authoritative hits.
    /// 完成排序的权威命中。
    pub hits: Vec<KnowledgeSearchHit>,
    /// Per-library FTS expressions generated by vldb.
    /// vldb 生成的逐资料库 FTS 表达式。
    pub fts_queries: Vec<KnowledgeFtsQuery>,
    /// Library-level failures not hidden from the caller.
    /// 不向调用方隐藏的资料库级失败。
    pub failures: Vec<KnowledgeSearchFailure>,
}

/// Final report for one synchronous Markdown directory import.
/// 一次同步 Markdown 目录导入的最终报告。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeImportReport {
    /// Durable import job identifier.
    /// 持久化导入任务标识符。
    pub job_id: String,
    /// Target library identifier.
    /// 目标资料库标识符。
    pub library_id: String,
    /// Number of Markdown files discovered.
    /// 扫描到的 Markdown 文件数量。
    pub scanned_files: u64,
    /// Number of new or changed documents committed.
    /// 已提交的新增或变更文档数量。
    pub changed_files: u64,
    /// Number of unchanged documents retained.
    /// 保留的未变化文档数量。
    pub unchanged_files: u64,
    /// Number of documents deleted after a confirmed rescan.
    /// 在确认重新扫描后删除的文档数量。
    pub deleted_files: u64,
    /// Number of files skipped because they could not be read safely.
    /// 因无法安全读取而跳过的文件数量。
    pub skipped_files: u64,
    /// Total source bytes read during the import.
    /// 导入期间读取的来源总字节数。
    pub processed_bytes: u64,
    /// Number of authoritative chunks after the import.
    /// 导入后的权威切片数量。
    pub chunk_count: u64,
    /// Bounded user-visible warnings.
    /// 有界的用户可见告警。
    pub warnings: Vec<String>,
}

/// Durable import or index-maintenance job progress and terminal state.
/// 持久化导入或索引维护任务的进度与终态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeImportJobSummary {
    /// Durable job identifier.
    /// 持久化任务标识符。
    pub id: String,
    /// Target library identifier.
    /// 目标资料库标识符。
    pub library_id: String,
    /// Stable import, repair, or rebuild operation kind.
    /// 稳定的导入、补全或重建操作类型。
    pub operation_kind: KnowledgeOperationKind,
    /// Durable lifecycle state.
    /// 持久化生命周期状态。
    pub state: String,
    /// Current processing phase.
    /// 当前处理阶段。
    pub phase: String,
    /// Discovered Markdown count.
    /// 已发现 Markdown 数量。
    pub scanned_files: u64,
    /// Total Markdown count after discovery completes, or zero while still scanning.
    /// 发现完成后的 Markdown 总数，扫描中为零。
    pub total_files: u64,
    /// Number of Markdown files whose parsing work has completed.
    /// 已完成解析工作的 Markdown 文件数。
    pub processed_files: u64,
    /// Total index items in the active indexing or recovery phase.
    /// 当前索引或恢复阶段的索引项总数。
    pub total_index_items: u64,
    /// Index items completed in the active indexing or recovery phase.
    /// 当前索引或恢复阶段已完成的索引项数。
    pub processed_index_items: u64,
    /// Current relative path being scanned or processed.
    /// 当前正在扫描或处理的相对路径。
    pub current_file: Option<String>,
    /// Source bytes read so far.
    /// 当前已读取来源字节数。
    pub processed_bytes: u64,
    /// Bounded warnings.
    /// 有界告警。
    pub warnings: Vec<String>,
    /// Optional terminal error.
    /// 可选终态错误。
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{KnowledgeImportJobSummary, KnowledgeOperationKind, KnowledgeSourceSummary};

    /// Verifies the browser progress contract uses explicit camelCase field names.
    /// 验证浏览器进度契约使用显式 camelCase 字段名。
    #[test]
    fn import_job_serializes_with_browser_field_names() {
        // Job is a complete representative snapshot for every compound progress field.
        // Job 是覆盖全部复合进度字段的完整代表性快照。
        let job = KnowledgeImportJobSummary {
            id: String::from("job-1"),
            library_id: String::from("library-1"),
            operation_kind: KnowledgeOperationKind::Import,
            state: String::from("running"),
            phase: String::from("parsing"),
            scanned_files: 8,
            total_files: 8,
            processed_files: 3,
            total_index_items: 21,
            processed_index_items: 13,
            current_file: Some(String::from("guide/intro.md")),
            processed_bytes: 1024,
            warnings: Vec::new(),
            error_message: None,
        };
        // Value exposes the exact JSON shape consumed by KnowledgeBaseArea.
        // Value 暴露 KnowledgeBaseArea 使用的准确 JSON 结构。
        let value = serde_json::to_value(job).expect("serialize import progress");
        assert_eq!(value["libraryId"], "library-1");
        assert_eq!(value["operationKind"], "import");
        assert_eq!(value["scannedFiles"], 8);
        assert_eq!(value["totalFiles"], 8);
        assert_eq!(value["processedFiles"], 3);
        assert_eq!(value["totalIndexItems"], 21);
        assert_eq!(value["processedIndexItems"], 13);
        assert_eq!(value["currentFile"], "guide/intro.md");
        assert_eq!(value["processedBytes"], 1024);
        assert!(value.get("total_files").is_none());
    }

    /// Verifies durable source management uses the browser's camelCase contract.
    /// 验证持久化来源管理使用浏览器侧 camelCase 契约。
    #[test]
    fn source_summary_serializes_with_browser_field_names() {
        // source is a complete representative durable Markdown root.
        // source 是一条完整且具有代表性的持久化 Markdown 根目录。
        let source = KnowledgeSourceSummary {
            id: String::from("source-1"),
            canonical_root: String::from("D:\\docs"),
            display_name: String::from("docs"),
            document_count: 42,
            chunk_count: 128,
            created_at_ms: 100,
            last_scanned_at_ms: 200,
        };
        // value exposes the exact JSON shape consumed by the source management list.
        // value 暴露来源管理列表使用的准确 JSON 结构。
        let value = serde_json::to_value(source).expect("serialize knowledge source summary");
        assert_eq!(value["canonicalRoot"], "D:\\docs");
        assert_eq!(value["displayName"], "docs");
        assert_eq!(value["documentCount"], 42);
        assert_eq!(value["chunkCount"], 128);
        assert_eq!(value["lastScannedAtMs"], 200);
        assert!(value.get("canonical_root").is_none());
    }
}
