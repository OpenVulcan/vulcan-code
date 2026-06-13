import React from "react";
import { Check, ChevronDown, ChevronUp, Columns2, Copy, GitBranch, RefreshCw, Rows3, Search } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import type { DiffBranchSummary, DiffCommitSummary, DiffPanelSnapshot, DiffReviewMode, DiffReviewSelection, DiffViewMode } from "./types";

// DIFF_REVIEW_MODE_LABELS maps review modes to compact UI labels.
// DIFF_REVIEW_MODE_LABELS 将审核模式映射为紧凑界面标签。
const DIFF_REVIEW_MODE_LABELS: Record<DiffReviewMode, string> = {
  unstaged: "未暂存",
  staged: "已暂存",
  commit: "提交",
  branch: "分支",
};

// DiffPanelToolbar renders the diff source selector and aggregate actions.
// DiffPanelToolbar 渲染 diff 来源选择器与聚合操作。
export function DiffPanelToolbar(props: {
  // Parsed diff snapshot to render.
  // 要渲染的已解析 diff 快照。
  snapshot: DiffPanelSnapshot;
  // Current Git review source selection.
  // 当前 Git 审核来源选择。
  selection: DiffReviewSelection;
  // Whether the panel is currently loading.
  // 面板当前是否正在加载。
  loading?: boolean;
  // Optional refresh action.
  // 可选刷新动作。
  onRefresh?: () => void;
  // Optional copy action receiving raw diff text.
  // 接收原始 diff 文本的可选复制动作。
  onCopyRaw?: (rawDiff: string) => void;
  // Current diff search query.
  // 当前 diff 搜索查询。
  searchQuery?: string;
  // Compact search result label.
  // 紧凑搜索结果标签。
  searchResultLabel?: string;
  // Whether search results were truncated by the result cap.
  // 搜索结果是否因结果上限被截断。
  searchResultTruncated?: boolean;
  // Whether previous search navigation is available.
  // 是否可以导航到上一条搜索结果。
  canSearchPrevious?: boolean;
  // Whether next search navigation is available.
  // 是否可以导航到下一条搜索结果。
  canSearchNext?: boolean;
  // Current visual diff layout mode.
  // 当前 diff 视觉布局模式。
  viewMode: DiffViewMode;
  // Search query change handler.
  // 搜索查询变更处理器。
  onSearchQueryChange?: (query: string) => void;
  // Previous search result handler.
  // 上一条搜索结果处理器。
  onSearchPrevious?: () => void;
  // Next search result handler.
  // 下一条搜索结果处理器。
  onSearchNext?: () => void;
  // Visual diff layout mode change handler.
  // diff 视觉布局模式变更处理器。
  onViewModeChange: (viewMode: DiffViewMode) => void;
  // Optional selection change handler.
  // 可选选择变更处理器。
  onSelectionChange?: (selection: DiffReviewSelection) => void;
}) {
  const selectedCommit = selectedDiffCommit(props.snapshot.recentCommits ?? [], props.selection.commitHash ?? props.snapshot.selectedCommitHash);
  const baseBranch = props.selection.baseBranch ?? props.snapshot.selectedBaseBranch ?? props.snapshot.currentBranch ?? "";
  const compareBranch = props.selection.compareBranch ?? props.snapshot.selectedCompareBranch ?? props.snapshot.defaultBranch ?? "";
  const titleText = props.selection.mode === "commit" && selectedCommit
    ? selectedCommit.subject
    : `${props.snapshot.stats.files} 个文件`;
  return (
    <header className="diff-panel-header">
      <div className="diff-panel-main">
        <div className="diff-panel-title-row">
          <DiffModeMenu
            commits={props.snapshot.recentCommits ?? []}
            selection={props.selection}
            onSelectionChange={props.onSelectionChange}
          />
          <span className="diff-panel-title-text">{titleText}</span>
          <span className="diff-stat added">+{props.snapshot.stats.additions}</span>
          <span className="diff-stat removed">-{props.snapshot.stats.deletions}</span>
        </div>
        {props.selection.mode === "branch" ? (
          <div className="diff-branch-compare-row">
            <BranchSelector
              branches={props.snapshot.branches ?? []}
              fallbackLabel={baseBranch || "当前分支"}
              value={baseBranch}
              onChange={(branch) => props.onSelectionChange?.({ ...props.selection, baseBranch: branch || null })}
            />
            <span className="diff-branch-arrow">→</span>
            <BranchSelector
              branches={props.snapshot.branches ?? []}
              fallbackLabel={compareBranch || "主分支"}
              value={compareBranch}
              onChange={(branch) => props.onSelectionChange?.({ ...props.selection, compareBranch: branch || null })}
            />
          </div>
        ) : null}
      </div>
      <div className="diff-panel-actions">
        <button
          className="diff-icon-button"
          onClick={() => props.onViewModeChange(props.viewMode === "split" ? "unified" : "split")}
          title={props.viewMode === "split" ? "切换到统一视图" : "切换到分栏视图"}
          type="button"
        >
          {props.viewMode === "split" ? <Rows3 size={14} /> : <Columns2 size={14} />}
        </button>
        {props.onSearchQueryChange ? (
          <DiffSearchControl
            canSearchNext={Boolean(props.canSearchNext)}
            canSearchPrevious={Boolean(props.canSearchPrevious)}
            query={props.searchQuery ?? ""}
            resultLabel={props.searchResultLabel ?? ""}
            resultTruncated={Boolean(props.searchResultTruncated)}
            onNext={props.onSearchNext}
            onPrevious={props.onSearchPrevious}
            onQueryChange={props.onSearchQueryChange}
          />
        ) : null}
        {props.onCopyRaw ? (
          <button className="diff-icon-button" onClick={() => props.onCopyRaw?.(props.snapshot.rawDiff)} title="复制 raw diff" type="button">
            <Copy size={14} />
          </button>
        ) : null}
        {props.onRefresh ? (
          <button className="diff-icon-button" disabled={props.loading} onClick={() => props.onRefresh?.()} title="刷新 diff" type="button">
            <RefreshCw size={14} />
          </button>
        ) : null}
      </div>
    </header>
  );
}

// DiffSearchControl renders compact in-panel diff search controls.
// DiffSearchControl 渲染紧凑的面板内 diff 搜索控件。
function DiffSearchControl(props: {
  // Current search query.
  // 当前搜索查询。
  query: string;
  // Compact result label.
  // 紧凑结果标签。
  resultLabel: string;
  // Whether result count was capped.
  // 结果数量是否被截断。
  resultTruncated: boolean;
  // Whether previous navigation is enabled.
  // 是否启用上一条导航。
  canSearchPrevious: boolean;
  // Whether next navigation is enabled.
  // 是否启用下一条导航。
  canSearchNext: boolean;
  // Query change handler.
  // 查询变更处理器。
  onQueryChange: (query: string) => void;
  // Previous result handler.
  // 上一条结果处理器。
  onPrevious?: () => void;
  // Next result handler.
  // 下一条结果处理器。
  onNext?: () => void;
}) {
  return (
    <div className="diff-panel-search">
      <Search size={13} />
      <input
        aria-label="搜索 diff"
        onChange={(event) => props.onQueryChange(event.currentTarget.value)}
        placeholder="搜索 diff"
        value={props.query}
      />
      {props.query ? (
        <span className="diff-search-count" title={props.resultTruncated ? "结果已截断" : undefined}>
          {props.resultLabel}
        </span>
      ) : null}
      <button
        className="diff-search-step"
        disabled={!props.canSearchPrevious}
        onClick={() => props.onPrevious?.()}
        title="上一条"
        type="button"
      >
        <ChevronUp size={13} />
      </button>
      <button
        className="diff-search-step"
        disabled={!props.canSearchNext}
        onClick={() => props.onNext?.()}
        title="下一条"
        type="button"
      >
        <ChevronDown size={13} />
      </button>
    </div>
  );
}

// DiffModeMenu renders the primary diff source dropdown.
// DiffModeMenu 渲染主要 diff 来源下拉菜单。
function DiffModeMenu(props: {
  // Current Git review source selection.
  // 当前 Git 审核来源选择。
  selection: DiffReviewSelection;
  // Recent commits available for commit review mode.
  // 提交审核模式可用的最近提交。
  commits: DiffCommitSummary[];
  // Optional selection change handler.
  // 可选选择变更处理器。
  onSelectionChange?: (selection: DiffReviewSelection) => void;
}) {
  const selectMode = React.useCallback((mode: DiffReviewMode) => {
    props.onSelectionChange?.({ ...props.selection, mode });
  }, [props]);
  const selectedCommitHash = props.selection.commitHash ?? props.commits[0]?.hash ?? null;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button className="diff-mode-trigger" type="button">
          <span>{DIFF_REVIEW_MODE_LABELS[props.selection.mode]}</span>
          <ChevronDown size={13} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="diff-mode-menu" sideOffset={4}>
        <DropdownMenuItem onSelect={() => selectMode("unstaged")}>
          <span>未暂存</span>
          {props.selection.mode === "unstaged" ? <Check size={14} /> : null}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => selectMode("staged")}>
          <span>已暂存</span>
          {props.selection.mode === "staged" ? <Check size={14} /> : null}
        </DropdownMenuItem>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="diff-mode-sub-trigger">
            <span>提交</span>
            {props.selection.mode === "commit" ? <Check size={14} /> : null}
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="diff-commit-menu" sideOffset={8}>
            {props.commits.length ? props.commits.map((commit) => (
              <DropdownMenuItem
                className="diff-commit-menu-item"
                key={commit.hash}
                onSelect={() => props.onSelectionChange?.({ ...props.selection, commitHash: commit.hash, mode: "commit" })}
              >
                <span>{commit.subject}</span>
                <small>{commit.relativeTime}</small>
                {selectedCommitHash === commit.hash ? <Check size={14} /> : null}
              </DropdownMenuItem>
            )) : (
              <DropdownMenuItem disabled>暂无提交记录</DropdownMenuItem>
            )}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuItem onSelect={() => selectMode("branch")}>
          <span>分支</span>
          {props.selection.mode === "branch" ? <Check size={14} /> : null}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// BranchSelector renders one searchable branch dropdown.
// BranchSelector 渲染单个可搜索分支下拉框。
function BranchSelector(props: {
  // Branches available for selection.
  // 可用于选择的分支列表。
  branches: DiffBranchSummary[];
  // Currently selected branch name.
  // 当前选中的分支名称。
  value: string;
  // Fallback label used before metadata is available.
  // 元数据可用前使用的回退标签。
  fallbackLabel: string;
  // Selection change handler.
  // 选择变更处理器。
  onChange: (branch: string) => void;
}) {
  const [query, setQuery] = React.useState("");
  const filteredBranches = React.useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) {
      return props.branches;
    }
    return props.branches.filter((branch) => branch.name.toLowerCase().includes(normalizedQuery));
  }, [props.branches, query]);
  const label = props.value || props.fallbackLabel;
  return (
    <DropdownMenu onOpenChange={(open) => {
      if (!open) {
        setQuery("");
      }
    }}>
      <DropdownMenuTrigger asChild>
        <button className="diff-branch-trigger" type="button">
          <span>{label}</span>
          <ChevronDown size={12} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="diff-branch-menu" sideOffset={5}>
        <div className="diff-branch-search" onKeyDown={(event) => event.stopPropagation()}>
          <Search size={13} />
          <input
            aria-label="搜索分支"
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="搜索分支"
            value={query}
          />
        </div>
        <div className="diff-branch-menu-label">分支</div>
        {filteredBranches.length ? filteredBranches.map((branch) => (
          <DropdownMenuItem
            className="diff-branch-menu-item"
            key={branch.name}
            onSelect={() => props.onChange(branch.name)}
          >
            <GitBranch size={13} />
            <span>{branch.name}</span>
            {props.value === branch.name ? <Check size={14} /> : null}
          </DropdownMenuItem>
        )) : (
          <DropdownMenuItem disabled>没有匹配分支</DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// selectedDiffCommit resolves the currently selected commit summary.
// selectedDiffCommit 解析当前选中的提交摘要。
function selectedDiffCommit(commits: DiffCommitSummary[], hash: string | null | undefined): DiffCommitSummary | null {
  if (!hash) {
    return commits[0] ?? null;
  }
  return commits.find((commit) => commit.hash === hash) ?? commits[0] ?? null;
}
