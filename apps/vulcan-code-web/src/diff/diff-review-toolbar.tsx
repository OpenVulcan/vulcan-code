import React from "react";
import { Check, ChevronDown, ChevronUp, Columns2, Copy, GitBranch, ListFilter, RefreshCw, Rows3, Search } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import type {
  DiffBranchSummary,
  DiffCommitSummary,
  DiffHunkSeparatorStyle,
  DiffIndicatorStyle,
  DiffOverflowMode,
  DiffPanelSnapshot,
  DiffRenderPreferences,
  DiffReviewMode,
  DiffReviewSelection,
  DiffViewMode,
} from "./types";

// DIFF_REVIEW_MODE_LABELS maps review modes to compact UI labels.
// DIFF_REVIEW_MODE_LABELS 将审核模式映射为紧凑界面标签。
const DIFF_REVIEW_MODE_LABELS: Record<DiffReviewMode, string> = {
  unstaged: "未暂存",
  staged: "已暂存",
  commit: "提交",
  branch: "分支",
};

// DIFF_OVERFLOW_MODE_LABELS maps target overflow modes to menu labels.
// DIFF_OVERFLOW_MODE_LABELS 将目标溢出模式映射为菜单标签。
const DIFF_OVERFLOW_MODE_LABELS: Record<DiffOverflowMode, string> = {
  scroll: "横向滚动",
  wrap: "自动换行",
};

// DIFF_HUNK_SEPARATOR_LABELS maps target hunk separator styles to menu labels.
// DIFF_HUNK_SEPARATOR_LABELS 将目标 hunk 分隔样式映射为菜单标签。
const DIFF_HUNK_SEPARATOR_LABELS: Record<DiffHunkSeparatorStyle, string> = {
  "line-info": "默认",
  simple: "简洁",
};

// DIFF_INDICATOR_STYLE_LABELS maps target indicator styles to menu labels.
// DIFF_INDICATOR_STYLE_LABELS 将目标航标样式映射为菜单标签。
const DIFF_INDICATOR_STYLE_LABELS: Record<DiffIndicatorStyle, string> = {
  bars: "左侧线条",
  classic: "+/- 符号",
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
  onRefresh?: () => void | Promise<void>;
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
  // Target-compatible rendering preferences.
  // 目标兼容的渲染偏好。
  renderPreferences: DiffRenderPreferences;
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
  // Target-compatible rendering preference change handler.
  // 目标兼容渲染偏好变更处理器。
  onRenderPreferencesChange: (preferences: DiffRenderPreferences) => void;
  // Optional selection change handler.
  // 可选选择变更处理器。
  onSelectionChange?: (selection: DiffReviewSelection) => void;
}) {
  const selectedCommit = selectedDiffCommit(props.snapshot.recentCommits ?? [], props.selection.commitHash ?? props.snapshot.selectedCommitHash);
  const baseBranch = props.selection.baseBranch ?? props.snapshot.selectedBaseBranch ?? props.snapshot.currentBranch ?? "";
  const compareBranch = props.selection.compareBranch ?? props.snapshot.selectedCompareBranch ?? props.snapshot.defaultBranch ?? "";
  const titleText = props.selection.mode === "commit" && selectedCommit
    ? selectedCommit.subject
    : null;
  return (
    <header className="diff-panel-header">
      <div className="diff-panel-main">
        <div className="diff-panel-title-row">
          <DiffModeMenu
            changedFileCount={props.snapshot.stats.files}
            commits={props.snapshot.recentCommits ?? []}
            selection={props.selection}
            onSelectionChange={props.onSelectionChange}
          />
          {titleText ? <span className="diff-panel-title-text">{titleText}</span> : null}
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
          {props.viewMode === "split" ? <Rows3 size={12} /> : <Columns2 size={12} />}
        </button>
        <DiffRenderStyleMenu
          preferences={props.renderPreferences}
          onPreferencesChange={props.onRenderPreferencesChange}
        />
        {props.onCopyRaw ? (
          <button className="diff-icon-button" onClick={() => props.onCopyRaw?.(props.snapshot.rawDiff)} title="复制 raw diff" type="button">
            <Copy size={12} />
          </button>
        ) : null}
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
        {props.onRefresh ? (
          <button className="diff-icon-button" disabled={props.loading} onClick={() => props.onRefresh?.()} title="刷新 diff" type="button">
            <RefreshCw className={props.loading ? "diff-refresh-icon loading" : "diff-refresh-icon"} size={12} />
          </button>
        ) : null}
      </div>
    </header>
  );
}

// DiffRenderStyleMenu renders the target diff style switches.
// DiffRenderStyleMenu 渲染目标 diff 风格切换项。
function DiffRenderStyleMenu(props: {
  // Current target-compatible rendering preferences.
  // 当前目标兼容渲染偏好。
  preferences: DiffRenderPreferences;
  // Preference change handler.
  // 偏好变更处理器。
  onPreferencesChange: (preferences: DiffRenderPreferences) => void;
}) {
  const selectOverflowMode = React.useCallback((overflowMode: DiffOverflowMode) => {
    props.onPreferencesChange({ ...props.preferences, overflowMode });
  }, [props]);
  const selectHunkSeparatorStyle = React.useCallback((hunkSeparatorStyle: DiffHunkSeparatorStyle) => {
    props.onPreferencesChange({ ...props.preferences, hunkSeparatorStyle });
  }, [props]);
  const selectIndicatorStyle = React.useCallback((indicatorStyle: DiffIndicatorStyle) => {
    props.onPreferencesChange({ ...props.preferences, indicatorStyle });
  }, [props]);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button className="diff-icon-button" title="Diff 显示风格" type="button">
          <ListFilter size={12} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="diff-style-menu" sideOffset={4}>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="diff-mode-sub-trigger">
            溢出：{DIFF_OVERFLOW_MODE_LABELS[props.preferences.overflowMode]}
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="diff-style-sub-menu" sideOffset={8}>
            {(["scroll", "wrap"] as const).map((mode) => (
              <DropdownMenuItem key={mode} onSelect={() => selectOverflowMode(mode)}>
                {props.preferences.overflowMode === mode ? <Check size={13} /> : <span className="diff-menu-check-spacer" />}
                {DIFF_OVERFLOW_MODE_LABELS[mode]}
              </DropdownMenuItem>
            ))}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="diff-mode-sub-trigger">
            行信息：{DIFF_HUNK_SEPARATOR_LABELS[props.preferences.hunkSeparatorStyle]}
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="diff-style-sub-menu" sideOffset={8}>
            {(["line-info", "simple"] as const).map((style) => (
              <DropdownMenuItem key={style} onSelect={() => selectHunkSeparatorStyle(style)}>
                {props.preferences.hunkSeparatorStyle === style ? <Check size={13} /> : <span className="diff-menu-check-spacer" />}
                {DIFF_HUNK_SEPARATOR_LABELS[style]}
              </DropdownMenuItem>
            ))}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="diff-mode-sub-trigger">
            航标：{DIFF_INDICATOR_STYLE_LABELS[props.preferences.indicatorStyle]}
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="diff-style-sub-menu" sideOffset={8}>
            {(["bars", "classic"] as const).map((style) => (
              <DropdownMenuItem key={style} onSelect={() => selectIndicatorStyle(style)}>
                {props.preferences.indicatorStyle === style ? <Check size={13} /> : <span className="diff-menu-check-spacer" />}
                {DIFF_INDICATOR_STYLE_LABELS[style]}
              </DropdownMenuItem>
            ))}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
      </DropdownMenuContent>
    </DropdownMenu>
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
  const [open, setOpen] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement | null>(null);
  React.useEffect(() => {
    if (!open || typeof window === "undefined") {
      return undefined;
    }
    const frameId = window.requestAnimationFrame(() => inputRef.current?.focus());
    return () => window.cancelAnimationFrame(frameId);
  }, [open]);
  const handleBlur = React.useCallback((event: React.FocusEvent<HTMLDivElement>) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
      setOpen(false);
    }
  }, []);
  const handleKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      setOpen(false);
    }
  }, []);
  return (
    <div className="diff-panel-search-anchor" onBlur={handleBlur} onKeyDown={handleKeyDown}>
      <button
        aria-expanded={open}
        aria-label="打开 diff 搜索"
        className={`diff-icon-button diff-search-toggle${props.query ? " active" : ""}`}
        onClick={() => setOpen((current) => !current)}
        title="搜索 diff"
        type="button"
      >
        <Search size={12} />
      </button>
      {open ? (
        <div className="diff-panel-search-popover">
          <div className="diff-panel-search">
            <Search size={13} />
            <input
              aria-label="搜索 diff"
              onChange={(event) => props.onQueryChange(event.currentTarget.value)}
              placeholder="搜索 diff"
              ref={inputRef}
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
        </div>
      ) : null}
    </div>
  );
}

// DiffModeMenu renders the primary diff source dropdown.
// DiffModeMenu 渲染主要 diff 来源下拉菜单。
function DiffModeMenu(props: {
  // Current Git review source selection.
  // 当前 Git 审核来源选择。
  selection: DiffReviewSelection;
  // Number of files changed by the current diff source.
  // 当前 diff 来源中发生变动的文件数量。
  changedFileCount: number;
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
  const shouldShowFileCount = props.selection.mode !== "commit";
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button className="diff-mode-trigger" type="button">
          <span>{DIFF_REVIEW_MODE_LABELS[props.selection.mode]}</span>
          {shouldShowFileCount ? <span className="diff-mode-count">{props.changedFileCount}</span> : null}
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
