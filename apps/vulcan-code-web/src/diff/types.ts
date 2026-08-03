// DiffPanelSource identifies where one rendered diff snapshot came from.
// DiffPanelSource 标识单个已渲染 diff 快照的来源。
export type DiffPanelSource = "workspace" | "turn" | "change_set" | "message";

// DiffReviewMode identifies the Git comparison source selected by the review panel.
// DiffReviewMode 标识审核面板当前选择的 Git 对比来源。
export type DiffReviewMode = "unstaged" | "staged" | "commit" | "branch";

// WorkspaceGitFileActionKind identifies file-level Git mutations exposed by the review panel.
// WorkspaceGitFileActionKind 标识审核面板暴露的文件级 Git 变更操作。
export type WorkspaceGitFileActionKind =
  | "stageAll"
  | "restoreAll"
  | "stageFile"
  | "restoreFile"
  | "unstageAll"
  | "unstageFile";

// DiffViewMode identifies the visual layout used for rendered file hunks.
// DiffViewMode 标识文件 hunk 渲染使用的视觉布局。
export type DiffViewMode = "unified" | "split";

// DiffOverflowMode identifies how code columns handle long lines.
// DiffOverflowMode 标识代码列如何处理长行。
export type DiffOverflowMode = "scroll" | "wrap";

// DiffHunkSeparatorStyle identifies the supported hunk separator style.
// DiffHunkSeparatorStyle 标识支持的 hunk 分隔样式。
export type DiffHunkSeparatorStyle = "line-info" | "simple";

// DiffIndicatorStyle identifies the target-supported change indicator style.
// DiffIndicatorStyle 标识目标支持的变更航标样式。
export type DiffIndicatorStyle = "bars" | "classic";

// DiffRenderPreferences stores target-compatible rendering switches.
// DiffRenderPreferences 保存目标兼容的渲染开关。
export interface DiffRenderPreferences {
  // Horizontal overflow behavior for rendered code columns.
  // 渲染代码列的横向溢出行为。
  overflowMode: DiffOverflowMode;
  // Whether paired changed lines render word-level inline diff marks.
  // 是否为成对变更行渲染词级行内差异标记。
  inlineDiff: boolean;
  // Hunk separator style used by the target diff renderer.
  // 目标 diff 渲染器使用的 hunk 分隔样式。
  hunkSeparatorStyle: DiffHunkSeparatorStyle;
  // Change indicator style used by the target diff renderer.
  // 目标 diff 渲染器使用的变更航标样式。
  indicatorStyle: DiffIndicatorStyle;
}

// DiffBranchSummary describes one selectable Git branch reference.
// DiffBranchSummary 描述一个可选择的 Git 分支引用。
export interface DiffBranchSummary {
  // Full branch reference label displayed to the user.
  // 展示给用户的完整分支引用标签。
  name: string;
  // Whether this branch is the currently checked out branch.
  // 该分支是否为当前检出的分支。
  current: boolean;
  // Whether this branch is remote-tracking.
  // 该分支是否为远端跟踪分支。
  remote: boolean;
}

// DiffCommitSummary describes one selectable unpushed commit.
// DiffCommitSummary 描述一个可选择的未推送提交。
export interface DiffCommitSummary {
  // Full commit hash used for Git commands.
  // Git 命令使用的完整提交哈希。
  hash: string;
  // Short commit hash for compact display.
  // 用于紧凑展示的短提交哈希。
  shortHash: string;
  // Commit subject line.
  // 提交标题行。
  subject: string;
  // Human-readable relative commit time.
  // 人类可读的相对提交时间。
  relativeTime: string;
}

// DiffReviewSelection stores the current Git diff source selection.
// DiffReviewSelection 保存当前 Git diff 来源选择。
export interface DiffReviewSelection {
  // Stable project-relative repository key; `.` selects the project root repository.
  // 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
  repositoryKey?: string | null;
  // Selected review mode.
  // 当前选择的审核模式。
  mode: DiffReviewMode;
  // Optional base branch used by branch comparisons.
  // 分支对比使用的可选基准分支。
  baseBranch?: string | null;
  // Optional compare branch used by branch comparisons.
  // 分支对比使用的可选目标分支。
  compareBranch?: string | null;
  // Optional commit hash used by commit review mode.
  // 提交审核模式使用的可选提交哈希。
  commitHash?: string | null;
}

// DiffFileStatus describes the file-level change category inferred from a git patch.
// DiffFileStatus 描述从 git patch 推断出的文件级变更类别。
export type DiffFileStatus = "added" | "deleted" | "modified" | "renamed" | "binary";

// DiffLineKind describes one visual row inside a parsed hunk.
// DiffLineKind 描述已解析 hunk 中的单个可视行。
export type DiffLineKind = "added" | "removed" | "context" | "metadata";

// DiffInlineChangeRange marks the changed character range inside one paired line.
// DiffInlineChangeRange 标记配对行内部发生变化的字符范围。
export interface DiffInlineChangeRange {
  // Inclusive start offset in UTF-16 code units.
  // UTF-16 code unit 中的包含式起始偏移。
  start: number;
  // Exclusive end offset in UTF-16 code units.
  // UTF-16 code unit 中的排他式结束偏移。
  end: number;
}

// DiffHunkExpansionDirection describes which side of a hunk context request should grow.
// DiffHunkExpansionDirection 描述 hunk 上下文请求需要扩展的方向。
export type DiffHunkExpansionDirection = "up" | "down" | "both";

// DiffHunkExpansionRequest stores a targeted hunk context expansion request.
// DiffHunkExpansionRequest 保存一次定向 hunk 上下文展开请求。
export interface DiffHunkExpansionRequest {
  // Zero-based hunk index in the currently rendered file.
  // 当前渲染文件中的零基 hunk 索引。
  hunkIndex: number;
  // Side of the hunk whose surrounding context should expand.
  // 需要扩展周边上下文的 hunk 方向。
  direction: DiffHunkExpansionDirection;
}

// DiffCompletenessState describes whether the current diff payload is complete.
// DiffCompletenessState 描述当前 diff 载荷是否完整。
export type DiffCompletenessState = "complete" | "truncated" | "partial";

// DiffFileLoadStatus describes lazy file-detail hydration state.
// DiffFileLoadStatus 描述文件详情懒加载水合状态。
export type DiffFileLoadStatus = "idle" | "loading" | "loaded" | "failed" | "timed-out";

// DiffFileLoadState stores the current lazy detail status for one file.
// DiffFileLoadState 保存单个文件当前的懒加载详情状态。
export interface DiffFileLoadState {
  // Lazy detail status for this file.
  // 该文件的懒加载详情状态。
  status: DiffFileLoadStatus;
  // Optional user-facing status message.
  // 可选的面向用户状态消息。
  message?: string | null;
}

// DiffStats stores aggregate counters for one diff panel.
// DiffStats 保存单个 diff 面板的聚合计数。
export interface DiffStats {
  // Number of changed files.
  // 变更文件数量。
  files: number;
  // Number of added lines.
  // 新增行数量。
  additions: number;
  // Number of removed lines.
  // 删除行数量。
  deletions: number;
  // Number of changed lines counted as additions plus deletions.
  // 以新增行加删除行计算的变更行数量。
  totalChangedLines: number;
  // Estimated UTF-8 byte size of changed lines.
  // 变更行的估算 UTF-8 字节体积。
  totalChangedBytes: number;
  // Whether aggregate stats cross target-compatible large-diff limits.
  // 聚合统计是否超过目标兼容的大 diff 限制。
  isLarge: boolean;
  // Internal large-diff reason ids used for rendering policy and diagnostics.
  // 用于渲染策略与诊断的内部大 diff 原因标识。
  largeReasons: string[];
}

// DiffLine stores one parsed line with old and new file line numbers.
// DiffLine 保存带旧文件与新文件行号的单条解析行。
export interface DiffLine {
  // Semantic row kind used by the renderer.
  // 渲染器使用的语义行类型。
  kind: DiffLineKind;
  // Raw content without the leading unified-diff marker.
  // 去除 unified diff 前导标记后的原始内容。
  content: string;
  // Optional old-file line number.
  // 可选旧文件行号。
  oldLine: number | null;
  // Optional new-file line number.
  // 可选新文件行号。
  newLine: number | null;
  // Optional inline changed range used by the high-performance HTML renderer.
  // 高性能 HTML 渲染器使用的可选行内变更范围。
  inlineChangeRange?: DiffInlineChangeRange;
  // Optional inline changed ranges used by token-level LCS diffing.
  // token 级 LCS diff 使用的可选多段行内变更范围。
  inlineChangeRanges?: DiffInlineChangeRange[];
}

// DiffHunk stores one parsed unified-diff hunk.
// DiffHunk 保存单个已解析 unified diff hunk。
export interface DiffHunk {
  // Original hunk header.
  // 原始 hunk 头部。
  header: string;
  // First old-file line covered by the hunk.
  // hunk 覆盖的旧文件起始行。
  oldStart: number;
  // First new-file line covered by the hunk.
  // hunk 覆盖的新文件起始行。
  newStart: number;
  // Parsed hunk body lines.
  // 已解析的 hunk 正文行。
  lines: DiffLine[];
}

// DiffFile stores one file-level patch section.
// DiffFile 保存单个文件级 patch 片段。
export interface DiffFile {
  // Stable key used by React and collapse state.
  // React 与折叠状态使用的稳定键。
  key: string;
  // Previous file path.
  // 旧文件路径。
  oldPath: string;
  // Current file path.
  // 当前文件路径。
  newPath: string;
  // File-level change status.
  // 文件级变更状态。
  status: DiffFileStatus;
  // Number of added lines in this file.
  // 该文件新增行数量。
  additions: number;
  // Number of removed lines in this file.
  // 该文件删除行数量。
  deletions: number;
  // Whether this file is binary-only in the diff.
  // 该文件是否在 diff 中仅表现为二进制变更。
  isBinary: boolean;
  // Whether this file is a gitlink or submodule entry.
  // 该文件是否为 gitlink 或子模块条目。
  isGitlink: boolean;
  // File mode parsed from diff metadata when available.
  // 从 diff 元数据解析出的可选文件模式。
  fileMode?: string | null;
  // Number of changed lines counted as additions plus deletions.
  // 以新增行加删除行计算的变更行数量。
  changedLines: number;
  // Estimated UTF-8 byte size of changed lines in this file.
  // 该文件变更行的估算 UTF-8 字节体积。
  changedBytes: number;
  // Largest estimated UTF-8 byte size among changed lines.
  // 变更行中的最大估算 UTF-8 字节体积。
  maxChangedLineBytes: number;
  // Whether this file crosses target-compatible large-file limits.
  // 该文件是否超过目标兼容的大文件限制。
  isLarge: boolean;
  // Whether the backend intentionally omitted this file body from WebUI rendering.
  // 后端是否有意从 WebUI 渲染载荷中省略该文件正文。
  tooLargeToDisplay?: boolean;
  // Internal large-file reason ids used for rendering policy and diagnostics.
  // 用于渲染策略与诊断的内部大文件原因标识。
  largeReasons: string[];
  // Visual row count including hunk headers, computed during parsing when available.
  // 可用时在解析阶段计算出的可视行数，包含 hunk 头部。
  rowCount?: number;
  // Estimated full visual row count from backend summaries before lazy detail loading.
  // 懒加载详情前由后端摘要估算出的完整可视行数。
  estimatedRowCount?: number;
  // Estimated hunk header count from backend summaries before lazy detail loading.
  // 懒加载详情前由后端摘要估算出的 hunk 头部数量。
  estimatedHunkCount?: number;
  // Complete old-side source file line count returned by the backend when available.
  // 后端可用时返回的旧侧源文件完整行数。
  oldFileLineCount?: number | null;
  // Complete new-side source file line count returned by the backend when available.
  // 后端可用时返回的新侧源文件完整行数。
  newFileLineCount?: number | null;
  // Whether only part of this file diff is available.
  // 该文件 diff 是否只有局部内容可用。
  isPartial?: boolean;
  // Lazy file-detail hydration state.
  // 文件详情懒加载水合状态。
  fileDiffLoadState?: DiffFileLoadState;
  // Unified-diff context lines used by the loaded file detail.
  // 已加载文件详情使用的 unified-diff 上下文行数。
  contextLineCount?: number;
  // Raw unified diff section for this file when it has been loaded.
  // 文件已加载时对应的原始 unified diff 片段。
  rawDiff?: string;
  // Parsed hunks for textual changes.
  // 文本变更对应的已解析 hunk。
  hunks: DiffHunk[];
}

// WorkspaceDiffFileSummary mirrors one backend file summary computed before diff truncation.
// WorkspaceDiffFileSummary 对应后端在 diff 截断前计算出的单文件摘要。
export interface WorkspaceDiffFileSummary {
  // Stable summary key used by the backend.
  // 后端使用的稳定摘要键。
  key: string;
  // Previous file path.
  // 旧文件路径。
  oldPath: string;
  // Current file path.
  // 当前文件路径。
  newPath: string;
  // File-level change status.
  // 文件级变更状态。
  status: DiffFileStatus;
  // Number of added lines in this file.
  // 该文件新增行数量。
  additions: number;
  // Number of removed lines in this file.
  // 该文件删除行数量。
  deletions: number;
  // Whether this file is binary-only in the diff.
  // 该文件是否在 diff 中仅表现为二进制变更。
  isBinary: boolean;
  // Whether this file is a gitlink or submodule entry.
  // 该文件是否为 gitlink 或子模块条目。
  isGitlink: boolean;
  // File mode parsed from diff metadata when available.
  // 从 diff 元数据解析出的可选文件模式。
  fileMode?: string | null;
  // Number of changed lines counted as additions plus deletions.
  // 以新增行加删除行计算的变更行数量。
  changedLines: number;
  // Whether the backend intentionally omitted this file body from WebUI rendering.
  // 后端是否有意从 WebUI 渲染载荷中省略该文件正文。
  tooLargeToDisplay?: boolean;
  // Estimated UTF-8 byte size of changed lines in this file.
  // 该文件变更行的估算 UTF-8 字节体积。
  changedBytes: number;
  // Largest estimated UTF-8 byte size among changed lines.
  // 变更行中的最大估算 UTF-8 字节体积。
  maxChangedLineBytes: number;
  // Estimated visual row count including hunk headers.
  // 包含 hunk 头部的估算可视行数。
  rowCount?: number;
  // Complete old-side source file line count when available.
  // 可用时旧侧源文件完整行数。
  oldFileLineCount?: number | null;
  // Complete new-side source file line count when available.
  // 可用时新侧源文件完整行数。
  newFileLineCount?: number | null;
  // Estimated hunk header count in this file diff.
  // 该文件 diff 中估算出的 hunk 头部数量。
  hunkCount?: number;
}

// WorkspaceDiffFileDetailRequest identifies one file whose full diff should be loaded.
// WorkspaceDiffFileDetailRequest 标识需要加载完整 diff 的单个文件。
export interface WorkspaceDiffFileDetailRequest {
  // Stable file key from the current summary snapshot.
  // 当前摘要快照中的稳定文件键。
  key: string;
  // Previous file path from the summary snapshot.
  // 摘要快照中的旧文件路径。
  oldPath: string;
  // Current file path from the summary snapshot.
  // 摘要快照中的当前文件路径。
  newPath: string;
  // Optional unified-diff context lines requested for hunk expansion.
  // hunk 展开请求使用的可选 unified-diff 上下文行数。
  contextLines?: number;
  // Optional target hunk index for directional expansion.
  // 定向展开使用的可选目标 hunk 索引。
  hunkIndex?: number;
  // Optional target direction for hunk context expansion.
  // hunk 上下文展开使用的可选目标方向。
  hunkExpansionDirection?: DiffHunkExpansionDirection;
}

// WorkspaceGitFileActionFile mirrors one selected diff file sent to the backend.
// WorkspaceGitFileActionFile 对应发送给后端的单个已选 diff 文件。
export interface WorkspaceGitFileActionFile {
  // Previous file path from the active diff snapshot.
  // 当前 diff 快照中的旧文件路径。
  oldPath: string;
  // Current file path from the active diff snapshot.
  // 当前 diff 快照中的当前文件路径。
  newPath: string;
  // File-level change status from the active diff snapshot.
  // 当前 diff 快照中的文件级变更状态。
  status: DiffFileStatus;
}

// WorkspaceGitFileActionRequest describes one file-level Git mutation request.
// WorkspaceGitFileActionRequest 描述一次文件级 Git 变更请求。
export interface WorkspaceGitFileActionRequest {
  // Stable project-relative repository key; `.` selects the project root repository.
  // 稳定的项目相对仓库键；`.` 表示选择项目根仓库。
  repositoryKey?: string | null;
  // Action to execute against the worktree or index.
  // 要对工作区或索引执行的操作。
  action: WorkspaceGitFileActionKind;
  // Optional files used by file-scoped actions.
  // 文件级操作使用的可选文件列表。
  files?: WorkspaceGitFileActionFile[];
}

// WorkspaceReviewFocusRequest identifies an external review-panel focus action.
// WorkspaceReviewFocusRequest 标识一次外部审核面板聚焦动作。
export interface WorkspaceReviewFocusRequest {
  // Monotonic focus request sequence.
  // 单调递增的聚焦请求序号。
  sequence: number;
  // Repository key that should be selected when the panel opens.
  // 面板打开时需要选择的仓库键。
  repositoryKey: string | null;
}

// WorkspaceGitRepositorySummary describes one discovered repository node.
// WorkspaceGitRepositorySummary 描述一个已发现的仓库节点。
export interface WorkspaceGitRepositorySummary {
  // Stable project-relative repository key.
  // 稳定的项目相对仓库键。
  repositoryKey: string;
  // Canonical backend repository root.
  // 后端使用的规范化仓库根目录。
  repositoryRoot: string;
  // Compact repository name.
  // 紧凑展示的仓库名称。
  name: string;
  // Stable project-relative path shown as secondary context.
  // 展示为次级上下文的稳定项目相对路径。
  relativePath: string;
  // Whether this repository is the deterministic primary node.
  // 该仓库是否为确定性的主要节点。
  isPrimary: boolean;
  // Whether live review monitoring is enabled for this repository.
  // 该仓库是否启用实时审查监听。
  monitoringEnabled: boolean;
}

// WorkspaceGitRepositoryCatalog describes all repositories discovered for one project.
// WorkspaceGitRepositoryCatalog 描述单个项目发现的全部仓库。
export interface WorkspaceGitRepositoryCatalog {
  // Canonical project root used by discovery.
  // 发现流程使用的规范化项目根目录。
  projectRoot: string;
  // Deterministically ordered repository nodes.
  // 按确定性顺序排列的仓库节点。
  repositories: WorkspaceGitRepositorySummary[];
}

// WorkspaceGitFileActionResult mirrors the backend Git mutation result.
// WorkspaceGitFileActionResult 对应后端 Git 变更操作结果。
export interface WorkspaceGitFileActionResult {
  // Repository root used for the Git command.
  // Git 命令使用的仓库根目录。
  repositoryRoot: string;
  // Executed action name.
  // 已执行的操作名称。
  action: WorkspaceGitFileActionKind;
  // Number of pathspecs affected by the operation.
  // 本次操作影响的路径规格数量。
  affectedPathCount: number;
  // User-facing operation summary.
  // 面向用户的操作摘要。
  summary: string;
}

// WorkspaceDiffFileDetailResponse mirrors the backend payload for one file diff.
// WorkspaceDiffFileDetailResponse 对应后端返回的单文件 diff 载荷。
export interface WorkspaceDiffFileDetailResponse {
  // Repository root used by the backend Git command.
  // 后端 Git 命令使用的仓库根目录。
  repositoryRoot: string;
  // Backend summary for the requested file when available.
  // 可用时请求文件对应的后端摘要。
  file?: WorkspaceDiffFileSummary | null;
  // Raw unified diff for only the requested file.
  // 仅包含请求文件的原始 unified diff。
  unifiedDiff: string;
  // Compatibility flag from legacy backend responses; normal file diff payloads are complete.
  // 旧版后端响应的兼容标记；正常单文件 diff 载荷为完整内容。
  truncated?: boolean | null;
  // Optional backend notice for empty, partial, or skipped details.
  // 空详情、局部详情或跳过详情对应的后端提示。
  notice?: string | null;
  // Unified-diff context lines used by the backend response.
  // 后端响应实际使用的 unified-diff 上下文行数。
  contextLines?: number | null;
  // Optional target hunk index echoed by the backend response.
  // 后端响应回传的可选目标 hunk 索引。
  hunkIndex?: number | null;
  // Optional target hunk expansion direction echoed by the backend response.
  // 后端响应回传的可选 hunk 展开方向。
  hunkExpansionDirection?: DiffHunkExpansionDirection | null;
}

// WorkspaceDiffSearchRequest stores backend summary-search options.
// WorkspaceDiffSearchRequest 保存后端摘要搜索选项。
export interface WorkspaceDiffSearchRequest {
  // Search query matched against the complete backend diff.
  // 与后端完整 diff 匹配的搜索词。
  query: string;
  // Optional maximum number of matching files returned by the backend.
  // 后端返回的可选最大命中文件数量。
  limit?: number;
}

// WorkspaceDiffSearchFileMatch describes one backend file-level search hit.
// WorkspaceDiffSearchFileMatch 描述一个后端文件级搜索命中。
export interface WorkspaceDiffSearchFileMatch {
  // Stable file key from the backend summary scanner.
  // 后端摘要扫描器生成的稳定文件键。
  key: string;
  // Previous file path.
  // 旧文件路径。
  oldPath: string;
  // Current file path.
  // 当前文件路径。
  newPath: string;
  // File-level change status.
  // 文件级变更状态。
  status: DiffFileStatus;
  // Number of matched diff lines in this file.
  // 该文件中命中的 diff 行数量。
  matchCount: number;
  // First compact matching line preview.
  // 首条紧凑命中行预览。
  firstPreview: string;
  // Bounded compact matching line previews for summary-only display.
  // 用于摘要态展示的受限紧凑命中行预览。
  previews?: string[];
  // Hunk index for the first matching diff row when available.
  // 可用时首条命中 diff 行所在的 hunk 索引。
  firstHunkIndex?: number | null;
  // Old-file line number for the first matching diff row when available.
  // 可用时首条命中 diff 行对应的旧文件行号。
  firstOldLine?: number | null;
  // New-file line number for the first matching diff row when available.
  // 可用时首条命中 diff 行对应的新文件行号。
  firstNewLine?: number | null;
  // Diff row kind for the first matching diff row when available.
  // 可用时首条命中 diff 行对应的行类型。
  firstLineKind?: DiffLineKind | "hunk" | null;
}

// WorkspaceDiffSearchResponse mirrors the backend compact diff search payload.
// WorkspaceDiffSearchResponse 对应后端紧凑 diff 搜索载荷。
export interface WorkspaceDiffSearchResponse {
  // Repository root used by the backend Git command.
  // 后端 Git 命令使用的仓库根目录。
  repositoryRoot: string;
  // Backend-trimmed query.
  // 后端裁剪空白后的搜索词。
  query: string;
  // File-level matches ordered by diff appearance.
  // 按 diff 出现顺序排列的文件级命中。
  matches: WorkspaceDiffSearchFileMatch[];
  // Whether additional matching files were omitted by the backend limit.
  // 是否因后端上限省略了更多命中文件。
  truncated: boolean;
  // Number of files scanned in the complete backend diff.
  // 后端完整 diff 中已扫描的文件数量。
  scannedFiles: number;
  // Number of diff lines scanned in the complete backend diff.
  // 后端完整 diff 中已扫描的 diff 行数量。
  scannedLines: number;
  // Optional backend notice for empty, partial, or skipped searches.
  // 空搜索、局部搜索或跳过搜索对应的后端提示。
  notice?: string | null;
}

// DiffPanelSnapshot is the complete renderer-ready diff payload.
// DiffPanelSnapshot 是可直接交给渲染器使用的完整 diff 载荷。
export interface DiffPanelSnapshot {
  // Human-readable source category.
  // 人类可读的来源类别。
  source: DiffPanelSource;
  // Optional repository root used for display.
  // 用于展示的可选仓库根目录。
  repositoryRoot?: string;
  // Parsed file-level diff records.
  // 已解析的文件级 diff 记录。
  files: DiffFile[];
  // Aggregate diff counters.
  // 聚合 diff 计数。
  stats: DiffStats;
  // Completeness state for the diff payload.
  // diff 载荷的完整性状态。
  completeness: DiffCompletenessState;
  // Whether the payload was cut before the renderer received it.
  // 载荷在渲染器收到前是否已被截断。
  isTruncated: boolean;
  // Optional notice displayed when parsing is partial or empty.
  // 解析结果不完整或为空时显示的可选提示。
  notice?: string;
  // Raw unified diff kept for copy and loaded-detail reconstruction.
  // 为复制和已加载详情重组保留的原始 unified diff。
  rawDiff: string;
  // Current Git branch name when available.
  // 可用时的当前 Git 分支名称。
  currentBranch?: string | null;
  // Number of current-branch commits that have not reached the remote view.
  // 当前分支尚未进入远端视图的提交数量。
  currentBranchUnpushedCommits?: number | null;
  // Default mainline branch resolved by the backend.
  // 后端解析出的默认主线分支。
  defaultBranch?: string | null;
  // Branch selected as the comparison base.
  // 当前作为对比基准的分支。
  selectedBaseBranch?: string | null;
  // Branch selected as the comparison target.
  // 当前作为对比目标的分支。
  selectedCompareBranch?: string | null;
  // Commit selected for commit review mode.
  // 提交审核模式当前选择的提交。
  selectedCommitHash?: string | null;
  // Branches available for branch comparison.
  // 可用于分支对比的分支列表。
  branches?: DiffBranchSummary[];
  // Unpushed commits available for commit review mode.
  // 可用于提交审核模式的未推送提交列表。
  recentCommits?: DiffCommitSummary[];
}

// WorkspaceDiffSnapshotResponse mirrors the Tauri service Git diff snapshot.
// WorkspaceDiffSnapshotResponse 对应 Tauri 服务返回的 Git diff 快照。
export interface WorkspaceDiffSnapshotResponse {
  // Repository root used by the backend Git command.
  // 后端 Git 命令使用的仓库根目录。
  repositoryRoot: string;
  // Raw combined unified diff.
  // 原始组合 unified diff。
  unifiedDiff: string;
  // Backend-counted changed file count.
  // 后端统计的变更文件数量。
  changedFiles: number;
  // Backend file summaries computed from the returned complete diff.
  // 后端基于返回的完整 diff 计算出的文件摘要。
  files?: WorkspaceDiffFileSummary[];
  // Backend-counted added lines.
  // 后端统计的新增行数。
  additions: number;
  // Backend-counted deleted lines.
  // 后端统计的删除行数。
  deletions: number;
  // Optional backend-counted changed line total.
  // 后端统计的可选变更行总数。
  totalChangedLines?: number | null;
  // Optional backend-counted changed byte total.
  // 后端统计的可选变更字节总数。
  totalChangedBytes?: number | null;
  // Compatibility flag from legacy backend responses; normal unified diff payloads are complete.
  // 旧版后端响应的兼容标记；正常 unified diff 载荷为完整内容。
  truncated?: boolean | null;
  // Optional backend notice for empty or partial snapshots.
  // 空快照或局部快照对应的后端提示。
  notice?: string | null;
  // Current Git branch name when available.
  // 可用时的当前 Git 分支名称。
  currentBranch?: string | null;
  // Number of current-branch commits that have not reached the remote view.
  // 当前分支尚未进入远端视图的提交数量。
  currentBranchUnpushedCommits?: number | null;
  // Default mainline branch resolved by the backend.
  // 后端解析出的默认主线分支。
  defaultBranch?: string | null;
  // Branch selected as the comparison base.
  // 当前作为对比基准的分支。
  selectedBaseBranch?: string | null;
  // Branch selected as the comparison target.
  // 当前作为对比目标的分支。
  selectedCompareBranch?: string | null;
  // Commit selected for commit review mode.
  // 提交审核模式当前选择的提交。
  selectedCommitHash?: string | null;
  // Branches available for branch comparison.
  // 可用于分支对比的分支列表。
  branches?: DiffBranchSummary[];
  // Unpushed commits available for commit review mode.
  // 可用于提交审核模式的未推送提交列表。
  recentCommits?: DiffCommitSummary[];
}
