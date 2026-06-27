import React from "react";
import { Check, ChevronDown, ChevronUp, Columns2, Copy, Diff, GitBranch, GitCommitHorizontal, GitCompareArrows, ListCollapse, LoaderCircle, MoreHorizontal, RefreshCw, Rows3, Search, SquarePlus, WrapText } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import type {
  DiffBranchSummary,
  DiffCommitSummary,
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

// BranchSelectorOption stores one deduplicated branch menu option.
// BranchSelectorOption 保存单个去重后的分支菜单选项。
interface BranchSelectorOption {
  // Original branch summary used by Git requests.
  // Git 请求使用的原始分支摘要。
  branch: DiffBranchSummary;
  // Compact branch label displayed to the user.
  // 展示给用户的紧凑分支标签。
  label: string;
  // Searchable text containing both compact and original labels.
  // 同时包含紧凑标签与原始标签的可搜索文本。
  searchText: string;
}

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
  // Search submit handler used by Enter navigation.
  // Enter 导航使用的搜索提交处理器。
  onSearchSubmit?: () => void;
  // Optional shared Git commit action label shown before search controls.
  // 搜索控件前展示的可选共享 Git 提交动作标签。
  gitCommitActionLabel?: string;
  // Whether the shared Git commit action is currently running.
  // 共享 Git 提交动作当前是否正在运行。
  gitCommitActionBusy?: boolean;
  // Opens the shared Git commit or push dialog.
  // 打开共享 Git 提交或推送弹窗。
  onOpenGitCommitAction?: () => void;
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
            <BranchReadOnlyDisplay
              branches={props.snapshot.branches ?? []}
              fallbackLabel={baseBranch || "当前分支"}
              value={baseBranch}
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
        <DiffMoreMenu
          preferences={props.renderPreferences}
          rawDiff={props.snapshot.rawDiff}
          onCopyRaw={props.onCopyRaw}
          onPreferencesChange={props.onRenderPreferencesChange}
        />
        <button
          className="diff-icon-button"
          onClick={() => props.onViewModeChange(props.viewMode === "split" ? "unified" : "split")}
          title={props.viewMode === "split" ? "切换到统一视图" : "切换到分栏视图"}
          type="button"
        >
          {props.viewMode === "split" ? <Rows3 size={12} /> : <Columns2 size={12} />}
        </button>
        {props.onOpenGitCommitAction ? (
          <button
            className="diff-icon-button"
            disabled={Boolean(props.gitCommitActionBusy)}
            onClick={props.onOpenGitCommitAction}
            title={props.gitCommitActionLabel}
            type="button"
          >
            {props.gitCommitActionBusy ? (
              <LoaderCircle className="diff-refresh-icon loading" size={12} />
            ) : (
              <GitCommitHorizontal size={12} />
            )}
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
            onSearchSubmit={props.onSearchSubmit}
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

// DiffMoreMenu renders secondary diff actions and target display switches.
// DiffMoreMenu 渲染 diff 次级操作与目标显示开关。
function DiffMoreMenu(props: {
  // Current target-compatible rendering preferences.
  // 当前目标兼容渲染偏好。
  preferences: DiffRenderPreferences;
  // Raw unified diff text used by the copy action.
  // 复制操作使用的原始 unified diff 文本。
  rawDiff: string;
  // Optional copy action receiving raw diff text.
  // 接收原始 diff 文本的可选复制动作。
  onCopyRaw?: (rawDiff: string) => void;
  // Preference change handler.
  // 偏好变更处理器。
  onPreferencesChange: (preferences: DiffRenderPreferences) => void;
}) {
  const copyRawDiff = React.useCallback(() => {
    props.onCopyRaw?.(props.rawDiff);
  }, [props]);
  const toggleInlineDiff = React.useCallback(() => {
    props.onPreferencesChange({ ...props.preferences, inlineDiff: !props.preferences.inlineDiff });
  }, [props]);
  const toggleOverflowMode = React.useCallback(() => {
    props.onPreferencesChange({
      ...props.preferences,
      overflowMode: props.preferences.overflowMode === "wrap" ? "scroll" : "wrap",
    });
  }, [props]);
  const toggleHunkSeparatorStyle = React.useCallback(() => {
    props.onPreferencesChange({
      ...props.preferences,
      hunkSeparatorStyle: props.preferences.hunkSeparatorStyle === "line-info" ? "simple" : "line-info",
    });
  }, [props]);
  const toggleIndicatorStyle = React.useCallback(() => {
    props.onPreferencesChange({
      ...props.preferences,
      indicatorStyle: props.preferences.indicatorStyle === "bars" ? "classic" : "bars",
    });
  }, [props]);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button className="diff-icon-button" title="更多 diff 操作" type="button">
          <MoreHorizontal size={12} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="diff-style-menu" sideOffset={4}>
        {props.onCopyRaw ? (
          <>
            <DropdownMenuItem className="diff-style-toggle-item" onSelect={copyRawDiff}>
              <Copy size={13} />
              <span>复制 raw diff</span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        ) : null}
        <DropdownMenuItem className="diff-style-toggle-item" onSelect={toggleInlineDiff}>
          <Diff size={13} />
          <span>{props.preferences.inlineDiff ? "禁用文字差异" : "启用文字差异"}</span>
        </DropdownMenuItem>
        <DropdownMenuItem className="diff-style-toggle-item" onSelect={toggleOverflowMode}>
          <WrapText size={13} />
          <span>{props.preferences.overflowMode === "wrap" ? "禁用自动换行" : "启用自动换行"}</span>
        </DropdownMenuItem>
        <DropdownMenuItem className="diff-style-toggle-item" onSelect={toggleHunkSeparatorStyle}>
          {props.preferences.hunkSeparatorStyle === "line-info" ? <Rows3 size={13} /> : <ListCollapse size={13} />}
          <span>{props.preferences.hunkSeparatorStyle === "line-info" ? "隐藏上下文展开控件" : "显示上下文展开控件"}</span>
        </DropdownMenuItem>
        <DropdownMenuItem className="diff-style-toggle-item" onSelect={toggleIndicatorStyle}>
          {props.preferences.indicatorStyle === "bars" ? <SquarePlus size={13} /> : <GitCompareArrows size={13} />}
          <span>{props.preferences.indicatorStyle === "bars" ? "使用 +/- 行标" : "使用颜色线条行标"}</span>
        </DropdownMenuItem>
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
  // Search submit handler used by Enter navigation.
  // Enter 导航使用的搜索提交处理器。
  onSearchSubmit?: () => void;
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
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.shiftKey) {
        props.onPrevious?.();
      } else {
        props.onSearchSubmit?.();
      }
    }
  }, [props]);
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

// BranchReadOnlyDisplay renders the immutable left side of branch comparisons.
// BranchReadOnlyDisplay 渲染分支对比中不可修改的左侧分支。
function BranchReadOnlyDisplay(props: {
  // Branches available for display disambiguation.
  // 用于展示消歧的可用分支列表。
  branches: DiffBranchSummary[];
  // Currently selected branch name.
  // 当前选中的分支名称。
  value: string;
  // Fallback label used before metadata is available.
  // 元数据可用前使用的回退标签。
  fallbackLabel: string;
}) {
  const label = props.value
    ? branchDisplayName(props.value, props.branches)
    : props.fallbackLabel;
  return (
    <span className="diff-branch-trigger diff-branch-trigger-readonly" title={props.value || props.fallbackLabel}>
      <span>{label}</span>
    </span>
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
  const branchOptions = React.useMemo(
    () => buildBranchSelectorOptions(props.branches, props.value),
    [props.branches, props.value],
  );
  const filteredBranches = React.useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) {
      return branchOptions;
    }
    return branchOptions.filter((option) => option.searchText.includes(normalizedQuery));
  }, [branchOptions, query]);
  const label = props.value ? branchDisplayName(props.value, props.branches) : props.fallbackLabel;
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
        {filteredBranches.length ? filteredBranches.map((option) => (
          <DropdownMenuItem
            className="diff-branch-menu-item"
            key={option.branch.name}
            onSelect={() => props.onChange(option.branch.name)}
          >
            <GitBranch size={13} />
            <span>{option.label}</span>
            {branchNamesEquivalent(props.value, option.branch.name) ? <Check size={14} /> : null}
          </DropdownMenuItem>
        )) : (
          <DropdownMenuItem disabled>没有匹配分支</DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// buildBranchSelectorOptions hides duplicate origin-tracking branches behind one display label.
// buildBranchSelectorOptions 将重复的 origin 跟踪分支折叠为一个展示标签。
function buildBranchSelectorOptions(branches: DiffBranchSummary[], selectedValue: string): BranchSelectorOption[] {
  const byLabel = new Map<string, BranchSelectorOption>();
  for (const branch of branches) {
    const label = branchDisplayName(branch.name, branches);
    const key = label.toLowerCase();
    const option = {
      branch,
      label,
      searchText: `${label} ${branch.name}`.toLowerCase(),
    };
    const existing = byLabel.get(key);
    if (!existing || branchOptionRank(branch, selectedValue) < branchOptionRank(existing.branch, selectedValue)) {
      byLabel.set(key, option);
    }
  }
  if (selectedValue && !Array.from(byLabel.values()).some((option) => branchNamesEquivalent(option.branch.name, selectedValue))) {
    const branch = { current: false, name: selectedValue, remote: false };
    const label = branchDisplayName(selectedValue, branches);
    byLabel.set(label.toLowerCase(), {
      branch,
      label,
      searchText: `${label} ${selectedValue}`.toLowerCase(),
    });
  }
  return Array.from(byLabel.values());
}

// branchOptionRank picks the most useful branch when compact labels collide.
// branchOptionRank 在紧凑标签冲突时选择更适合展示的分支。
function branchOptionRank(branch: DiffBranchSummary, selectedValue: string): number {
  if (branchNamesEquivalent(branch.name, selectedValue)) {
    return 0;
  }
  if (branch.current) {
    return 1;
  }
  if (!branch.remote) {
    return 2;
  }
  if (branchShortName(branch.name).startsWith("origin/")) {
    return 3;
  }
  return 4;
}

// branchDisplayName returns a compact label while preserving non-origin remote ambiguity.
// branchDisplayName 返回紧凑标签，同时保留非 origin 远端歧义。
function branchDisplayName(name: string, branches: DiffBranchSummary[]): string {
  const shortName = branchShortName(name);
  if (!shortName.startsWith("origin/")) {
    return shortName;
  }
  const localName = shortName.slice("origin/".length);
  const hasLocalTwin = branches.some((branch) => !branch.remote && branchShortName(branch.name) === localName);
  const hasOtherRemoteTwin = branches.some((branch) => {
    const candidate = branchShortName(branch.name);
    return branch.remote && !candidate.startsWith("origin/") && candidate.endsWith(`/${localName}`);
  });
  return hasOtherRemoteTwin && !hasLocalTwin ? shortName : localName;
}

// branchShortName removes Git ref prefixes while keeping remote names when they matter.
// branchShortName 移除 Git ref 前缀，同时在必要时保留远端名称。
function branchShortName(name: string): string {
  return name
    .replace(/^refs\/heads\//, "")
    .replace(/^refs\/remotes\//, "");
}

// branchNamesEquivalent treats local and origin tracking names as the same display branch.
// branchNamesEquivalent 将本地分支与 origin 跟踪分支视为相同展示分支。
function branchNamesEquivalent(left: string, right: string): boolean {
  return branchCanonicalName(left) === branchCanonicalName(right);
}

// branchCanonicalName normalizes local and origin-prefixed branch names for comparison.
// branchCanonicalName 规整本地与 origin 前缀分支名称以便比较。
function branchCanonicalName(name: string): string {
  const shortName = branchShortName(name);
  return shortName.startsWith("origin/") ? shortName.slice("origin/".length) : shortName;
}

// selectedDiffCommit resolves the currently selected commit summary.
// selectedDiffCommit 解析当前选中的提交摘要。
function selectedDiffCommit(commits: DiffCommitSummary[], hash: string | null | undefined): DiffCommitSummary | null {
  if (!hash) {
    return commits[0] ?? null;
  }
  return commits.find((commit) => commit.hash === hash) ?? commits[0] ?? null;
}
