import type { ProjectDirectoryListing } from "./projects-files";

// KnowledgeLibrarySummary mirrors one host-managed knowledge library.
// KnowledgeLibrarySummary 对齐一个宿主管理知识库。
export interface KnowledgeLibrarySummary {
  // Immutable library identifier.
  // 不可变知识库标识符。
  id: string;
  // User-visible name.
  // 用户可见名称。
  name: string;
  // User-visible description.
  // 用户可见说明。
  description: string;
  // Durable lifecycle state.
  // 持久化生命周期状态。
  lifecycleState: string;
  // Authoritative document count.
  // 权威文档数量。
  documentCount: number;
  // Authoritative chunk count.
  // 权威切片数量。
  chunkCount: number;
  // Derived Jieba FTS index state.
  // 派生 Jieba FTS 索引状态。
  indexStatus: string;
  // Latest successful import timestamp.
  // 最近成功导入时间戳。
  lastCompletedImportAtMs?: number | null;
  // Creation timestamp.
  // 创建时间戳。
  createdAtMs: number;
  // Latest update timestamp.
  // 最近更新时间戳。
  updatedAtMs: number;
}

// KnowledgeSourceSummary mirrors one durable Markdown source directory.
// KnowledgeSourceSummary 对齐一个持久化 Markdown 来源目录。
export interface KnowledgeSourceSummary {
  // Immutable identity retained when the physical directory is remapped.
  // 物理目录重新映射时保持不变的身份。
  id: string;
  // Canonical native directory path currently assigned to the source.
  // 当前分配给来源的规范原生目录路径。
  canonicalRoot: string;
  // User-visible directory name derived from the canonical root.
  // 从规范根目录派生的用户可见目录名称。
  displayName: string;
  // Current authoritative Markdown document count owned by the source.
  // 当前由来源持有的权威 Markdown 文档数量。
  documentCount: number;
  // Current authoritative searchable chunk count owned by the source.
  // 当前由来源持有的权威可检索切片数量。
  chunkCount: number;
  // Source identity creation timestamp.
  // 来源身份创建时间戳。
  createdAtMs: number;
  // Most recent source scan timestamp.
  // 最近一次来源扫描时间戳。
  lastScannedAtMs: number;
}

// KnowledgeCustomWord mirrors one library-local Jieba dictionary entry.
// KnowledgeCustomWord 对齐一个知识库局部 Jieba 词典条目。
export interface KnowledgeCustomWord {
  // Exact custom word text.
  // 精确的自定义词文本。
  word: string;
  // Jieba word weight.
  // Jieba 词权重。
  weight: number;
}

// KnowledgeProjectBinding mirrors one project authorization row.
// KnowledgeProjectBinding 对齐一条项目授权记录。
export interface KnowledgeProjectBinding {
  // Owning library identifier.
  // 所属知识库标识符。
  libraryId: string;
  // Authorized project identifier.
  // 已授权项目标识符。
  projectId: string;
  // Whether inherited sessions enable the library.
  // 继承型会话是否启用知识库。
  defaultEnabled: boolean;
}

// KnowledgeDocumentSummary mirrors one imported Markdown document.
// KnowledgeDocumentSummary 对齐一份已导入 Markdown 文档。
export interface KnowledgeDocumentSummary {
  // Immutable document identifier.
  // 不可变文档标识符。
  id: string;
  // Source identifier.
  // 来源标识符。
  sourceId: string;
  // Relative source path.
  // 相对来源路径。
  relativePath: string;
  // Logical folder path.
  // 逻辑目录路径。
  folderPath: string;
  // Deterministic title.
  // 确定性标题。
  title: string;
  // Source byte size.
  // 来源字节数。
  byteSize: number;
  // Source line count.
  // 来源行数。
  lineCount: number;
  // Searchable chunk count.
  // 可检索切片数。
  chunkCount: number;
  // Latest update timestamp.
  // 最近更新时间戳。
  updatedAtMs: number;
}

// KnowledgeDocumentContent contains one authoritative Markdown document.
// KnowledgeDocumentContent 包含一份权威 Markdown 文档。
export interface KnowledgeDocumentContent {
  // Document metadata.
  // 文档元数据。
  document: KnowledgeDocumentSummary;
  // Newline-normalized Markdown source.
  // 换行规范化后的 Markdown 原文。
  content: string;
}

// KnowledgeSearchHit is one citation-bearing BM25 result.
// KnowledgeSearchHit 是一条携带引用的 BM25 结果。
export interface KnowledgeSearchHit {
  // Owning library identifier.
  // 所属知识库标识符。
  libraryId: string;
  // Owning library name.
  // 所属知识库名称。
  libraryName: string;
  // Owning document identifier.
  // 所属文档标识符。
  documentId: string;
  // Exact chunk identifier.
  // 精确切片标识符。
  chunkId: string;
  // Document title.
  // 文档标题。
  title: string;
  // Relative source path.
  // 相对来源路径。
  relativePath: string;
  // Markdown heading ancestry.
  // Markdown 标题层级。
  headingPath: string;
  // One-based first source line.
  // 从一开始的来源起始行。
  startLine: number;
  // One-based last source line.
  // 从一开始的来源结束行。
  endLine: number;
  // Authoritative Markdown chunk.
  // 权威 Markdown 切片。
  content: string;
  // Stable lexical match strategy returned by the runtime.
  // 运行时返回的稳定词法匹配策略。
  matchKind: "bm25" | "substring";
  // Per-library BM25 score.
  // 单知识库 BM25 分数。
  bm25Score: number;
  // Optional federated RRF score.
  // 可选联合 RRF 分数。
  rrfScore?: number | null;
}

// KnowledgeSearchResult is one federated Jieba BM25 response.
// KnowledgeSearchResult 是一次联合 Jieba BM25 响应。
export interface KnowledgeSearchResult {
  // Normalized user query.
  // 规范化用户查询。
  query: string;
  // Ranked citation-bearing hits.
  // 已排序且携带引用的命中。
  hits: KnowledgeSearchHit[];
  // Explicit library failures.
  // 显式知识库失败。
  failures: Array<{ libraryId: string; error: string }>;
  // Exact per-library vldb FTS expressions.
  // 精确的逐库 vldb FTS 表达式。
  ftsQueries: Array<{ libraryId: string; expression: string }>;
}

// KnowledgeImportReport is the terminal result of one Markdown import.
// KnowledgeImportReport 是一次 Markdown 导入的终态结果。
export interface KnowledgeImportReport {
  // Durable job identifier.
  // 持久化任务标识符。
  jobId: string;
  // Target library identifier.
  // 目标知识库标识符。
  libraryId: string;
  // Scanned Markdown file count.
  // 已扫描 Markdown 文件数。
  scannedFiles: number;
  // Changed document count.
  // 已变化文档数。
  changedFiles: number;
  // Unchanged document count.
  // 未变化文档数。
  unchangedFiles: number;
  // Deleted document count.
  // 已删除文档数。
  deletedFiles: number;
  // Skipped file count.
  // 已跳过文件数。
  skippedFiles: number;
  // Total source bytes read during the import.
  // 导入期间读取的来源总字节数。
  processedBytes: number;
  // Authoritative chunk count after commit.
  // 提交后的权威切片数。
  chunkCount: number;
  // Bounded import warnings.
  // 有界导入告警。
  warnings: string[];
}

// KnowledgeImportJobSummary is one dedicated import-progress snapshot.
// KnowledgeImportJobSummary 是一份专用导入进度快照。
export interface KnowledgeImportJobSummary {
  // Durable job identifier.
  // 持久化任务标识符。
  id: string;
  // Target library identifier.
  // 目标知识库标识符。
  libraryId: string;
  // Exact operation category retained across page refreshes.
  // 跨页面刷新保留的精确操作类别。
  operationKind: "import" | "repair" | "rebuild";
  // Durable job state.
  // 持久化任务状态。
  state: string;
  // Current import phase.
  // 当前导入阶段。
  phase: string;
  // Discovered Markdown count.
  // 已发现 Markdown 数量。
  scannedFiles: number;
  // Exact Markdown total after discovery completes, or zero while discovery is active.
  // 发现完成后的准确 Markdown 总数，发现期间为零。
  totalFiles: number;
  // Number of Markdown files whose parsing work has completed.
  // 已完成解析工作的 Markdown 文件数。
  processedFiles: number;
  // Total work items in the active indexing or recovery phase.
  // 当前索引或恢复阶段的工作项总数。
  totalIndexItems: number;
  // Completed work items in the active indexing or recovery phase.
  // 当前索引或恢复阶段已完成的工作项数。
  processedIndexItems: number;
  // Current relative directory or file path being visited.
  // 当前正在访问的相对目录或文件路径。
  currentFile?: string | null;
  // Source bytes processed so far.
  // 当前已处理来源字节数。
  processedBytes: number;
  // Bounded warnings.
  // 有界告警。
  warnings: string[];
  // Optional terminal error.
  // 可选终态错误。
  errorMessage?: string | null;
}

// KnowledgeIndexIntegritySummary combines revision gates with exact physical row checks.
// KnowledgeIndexIntegritySummary 将版本门控与精确物理记录检查组合在一起。
export interface KnowledgeIndexIntegritySummary {
  // Inspected library identifier.
  // 被检查的知识库标识符。
  libraryId: string;
  // Whether the physical FTS table exists.
  // 物理 FTS 表是否存在。
  indexExists: boolean;
  // Whether search can safely use this index now.
  // 当前是否可以安全使用该索引进行检索。
  searchable: boolean;
  // Persisted index lifecycle state.
  // 持久化索引生命周期状态。
  indexStatus: string;
  // Persisted tokenizer identity.
  // 持久化分词器身份。
  tokenizer: string;
  // Latest authoritative content revision.
  // 最新权威内容版本。
  contentRevision: number;
  // Content revision represented by the FTS table.
  // FTS 表代表的内容版本。
  indexedContentRevision: number;
  // Latest custom dictionary revision.
  // 最新自定义词典版本。
  dictionaryRevision: number;
  // Dictionary revision represented by the FTS table.
  // FTS 表代表的词典版本。
  indexedDictionaryRevision: number;
  // Exact authoritative chunk count.
  // 精确权威切片数量。
  authoritativeChunkCount: number;
  // Exact physical FTS row count.
  // 精确物理 FTS 记录数量。
  indexedChunkCount: number;
  // Missing or content-mismatched FTS row count.
  // 缺失或内容不匹配的 FTS 记录数量。
  missingOrMismatchedCount: number;
  // Orphaned or metadata-mismatched FTS row count.
  // 孤立或元数据不匹配的 FTS 记录数量。
  orphanOrMismatchedCount: number;
}

// SessionKnowledgeLibrarySelectionSummary describes persisted and effective session access.
// SessionKnowledgeLibrarySelectionSummary 描述持久化与生效的会话访问选择。
export interface SessionKnowledgeLibrarySelectionSummary {
  // Persisted selection mode.
  // 持久化选择模式。
  mode: "inherit" | "explicit";
  // Persisted explicit library identifiers.
  // 持久化显式知识库标识符。
  libraryIds: string[];
  // Effective identifiers after authorization.
  // 授权校验后的生效标识符。
  effectiveLibraryIds: string[];
}

// KnowledgePackageManifest describes the validated package.json inside one .vkb archive.
// KnowledgePackageManifest 描述一个 .vkb 归档中经过校验的 package.json。
export interface KnowledgePackageManifest {
  // Stable package format identifier.
  // 稳定包格式标识。
  format: string;
  // Portable package protocol version.
  // 可移植包协议版本。
  packageVersion: number;
  // Source library identity retained only for provenance.
  // 仅用于来源追踪的原知识库标识。
  sourceLibraryId: string;
  // Original library name.
  // 原始知识库名称。
  name: string;
  // Original library description.
  // 原始知识库说明。
  description: string;
  // Authoritative per-library schema version.
  // 权威单库结构版本。
  schemaVersion: number;
  // Exported document count.
  // 导出的文档数量。
  documentCount: number;
  // Exported chunk count.
  // 导出的切片数量。
  chunkCount: number;
  // Exported FTS index state.
  // 导出的 FTS 索引状态。
  indexStatus: string;
  // Export timestamp in Unix milliseconds.
  // Unix 毫秒导出时间戳。
  exportedAtMs: number;
  // Lowercase SQLite SHA-256 digest.
  // 小写 SQLite SHA-256 摘要。
  contentSha256: string;
  // Exact uncompressed SQLite size.
  // 精确的未压缩 SQLite 大小。
  contentBytes: number;
}

// KnowledgePackageExportResult reports one completed host export.
// KnowledgePackageExportResult 报告一次完成的宿主导出。
export interface KnowledgePackageExportResult {
  // Final native archive path.
  // 最终原生归档路径。
  archivePath: string;
  // Metadata written into the package.
  // 写入包中的元数据。
  package: KnowledgePackageManifest;
}

// KnowledgePackageInspection reports validated metadata and an optional same-name conflict.
// KnowledgePackageInspection 报告已校验元数据和可选的同名冲突。
export interface KnowledgePackageInspection {
  // Canonical native archive path.
  // 规范原生归档路径。
  archivePath: string;
  // Validated package metadata.
  // 已校验的包元数据。
  package: KnowledgePackageManifest;
  // Existing same-name library when present.
  // 存在时的同名知识库。
  conflictingLibrary?: KnowledgeLibrarySummary | null;
}

// KnowledgePackageImportResult reports one created or explicitly replaced library.
// KnowledgePackageImportResult 报告一个新建或显式替换的知识库。
export interface KnowledgePackageImportResult {
  // Imported authoritative library summary.
  // 已导入的权威知识库摘要。
  library: KnowledgeLibrarySummary;
  // Whether one existing library was replaced.
  // 是否替换了一个现有知识库。
  replacedExisting: boolean;
  // Non-fatal warnings produced after a successful commit.
  // 成功提交后产生的非致命告警。
  warnings: string[];
}

// KnowledgePackageFileSummary is one selectable .vkb file from the host browser.
// KnowledgePackageFileSummary 是宿主浏览器返回的一个可选 .vkb 文件。
export interface KnowledgePackageFileSummary {
  // File display name.
  // 文件显示名称。
  name: string;
  // Absolute native file path.
  // 原生绝对文件路径。
  path: string;
  // Compressed archive byte size.
  // 压缩归档字节数。
  byteSize: number;
}

// KnowledgePackageFileListing combines directory navigation with .vkb rows.
// KnowledgePackageFileListing 将目录导航与 .vkb 文件行组合在一起。
export interface KnowledgePackageFileListing {
  // Shared directory navigation payload.
  // 共享目录导航载荷。
  directory: ProjectDirectoryListing;
  // Selectable packages in the current directory.
  // 当前目录中的可选知识库包。
  packages: KnowledgePackageFileSummary[];
}
