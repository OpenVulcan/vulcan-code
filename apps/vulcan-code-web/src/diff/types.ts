// DiffPanelSource identifies where one rendered diff snapshot came from.
// DiffPanelSource 标识单个已渲染 diff 快照的来源。
export type DiffPanelSource = "workspace" | "turn" | "change_set" | "message";

// DiffReviewMode identifies the Git comparison source selected by the review panel.
// DiffReviewMode 标识审核面板当前选择的 Git 对比来源。
export type DiffReviewMode = "unstaged" | "staged" | "commit" | "branch";

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

// DiffCommitSummary describes one selectable recent commit.
// DiffCommitSummary 描述一个可选择的最近提交。
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
  // Parsed hunks for textual changes.
  // 文本变更对应的已解析 hunk。
  hunks: DiffHunk[];
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
  // Optional notice displayed when parsing is partial or empty.
  // 解析结果不完整或为空时显示的可选提示。
  notice?: string;
  // Raw unified diff kept for copy and fallback.
  // 为复制和回退保留的原始 unified diff。
  rawDiff: string;
  // Current Git branch name when available.
  // 可用时的当前 Git 分支名称。
  currentBranch?: string | null;
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
  // Recent commits available for commit review mode.
  // 可用于提交审核模式的最近提交列表。
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
  // Backend-counted added lines.
  // 后端统计的新增行数。
  additions: number;
  // Backend-counted deleted lines.
  // 后端统计的删除行数。
  deletions: number;
  // Optional backend notice for empty or partial snapshots.
  // 空快照或局部快照对应的后端提示。
  notice?: string | null;
  // Current Git branch name when available.
  // 可用时的当前 Git 分支名称。
  currentBranch?: string | null;
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
  // Recent commits available for commit review mode.
  // 可用于提交审核模式的最近提交列表。
  recentCommits?: DiffCommitSummary[];
}
