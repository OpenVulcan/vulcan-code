import React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { Check, ChevronDown, ChevronUp, Columns2, Copy, FolderGit2, GitBranch, GitCommitHorizontal, MoreHorizontal, RefreshCw, Rows3, Search } from "lucide-react";

import { IconButton } from "@/ui-kit/primitives/button";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import "@/ui-kit/styles/features/diff-toolbar.css";
import { useI18n } from "@/i18n";
import { formatCompactRelativeTime, formatLocalDateTime } from "@/components/project/project-sidebar-time";
import type { TranslationKey } from "@/i18n-messages";

import type {
  DiffBranchSummary,
  DiffCommitSummary,
  DiffPanelSnapshot,
  DiffRenderPreferences,
  DiffReviewMode,
  DiffReviewSelection,
  DiffViewMode,
  WorkspaceGitRepositorySummary,
} from "./types";

// DIFF_REVIEW_MODE_LABELS maps review modes to compact UI labels.
// DIFF_REVIEW_MODE_LABELS 将审核模式映射为紧凑界面标签。
const DIFF_REVIEW_MODE_LABEL_KEYS: Record<DiffReviewMode, TranslationKey> = {
  unstaged: "diff.mode.unstaged",
  staged: "diff.mode.staged",
  commit: "diff.mode.commit",
  branch: "diff.mode.branch",
  last_turn: "diff.mode.lastTurn",
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

// DiffPanelToolbar says what the review compares and how it is shown. The first row picks the repository and
// the source of the changes (unstaged, staged, a commit, a branch or the last turn) with their totals, then
// the view, search, commit, refresh and display options; branch comparison and search each add a row below.
// DiffPanelToolbar 说明审核比较的内容与展示方式。第一行选择仓库与变更来源（未暂存、已暂存、提交、分支或上一轮）及其合计，
// 然后是视图、搜索、提交、刷新与显示选项；分支对比与搜索各自在下方增加一行。
export function DiffPanelToolbar(props: {
  // Parsed diff snapshot to render.
  // 要渲染的已解析 diff 快照。
  snapshot: DiffPanelSnapshot;
  // Current Git review source selection.
  // 当前 Git 审核来源选择。
  selection: DiffReviewSelection;
  // Discovered repositories available for right-panel selection.
  // 右侧面板可选择的已发现仓库。
  repositories?: WorkspaceGitRepositorySummary[];
  // Whether the panel is currently loading.
  // 面板当前是否正在加载。
  loading?: boolean;
  // Optional refresh action.
  // 可选刷新动作。
  onRefresh?: () => void | Promise<void>;
  // Optional copy action receiving raw diff text.
  // 接收原始 diff 文本的可选复制动作。
  onCopyRaw?: (rawDiff: string) => void | Promise<void>;
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
  // Whether the shared Git commit action is currently unavailable.
  // 共享 Git 提交动作当前是否不可用。
  gitCommitActionDisabled?: boolean;
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
  const { t } = useI18n();
  const [searchOpen, setSearchOpen] = React.useState(false);
  const searchTriggerRef = React.useRef<HTMLButtonElement | null>(null);
  const selectedCommit = selectedDiffCommit(props.snapshot.recentCommits ?? [], props.selection.commitHash ?? props.snapshot.selectedCommitHash);
  const selectedRepositoryKey = props.selection.repositoryKey
    ?? props.repositories?.find((repository) => repository.isPrimary)?.repositoryKey
    ?? props.repositories?.[0]?.repositoryKey;
  const baseBranch = props.selection.baseBranch ?? props.snapshot.selectedBaseBranch ?? props.snapshot.currentBranch ?? "";
  const compareBranch = props.selection.compareBranch ?? props.snapshot.selectedCompareBranch ?? props.snapshot.defaultBranch ?? "";
  const titleText = props.selection.mode === "commit" && selectedCommit ? selectedCommit.subject : null;
  const closeSearch = () => {
    setSearchOpen(false);
    searchTriggerRef.current?.focus();
  };
  return (
    <header className="vk-diffbar">
      <div className="vk-diffbar-row">
        {props.repositories && props.repositories.length > 1 ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button aria-label={t("diff.repository")} className="vk-diffbar-pick" data-kind="repo" type="button">
                <FolderGit2 aria-hidden="true" size={13} />
                <span>
                  {props.repositories.find((repository) => repository.repositoryKey === selectedRepositoryKey)?.name
                    ?? props.repositories[0]?.name
                    ?? "Git"}
                </span>
                <ChevronDown aria-hidden="true" size={12} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="vk-diffbar-menu">
              {props.repositories.map((repository) => (
                <DropdownMenuItem
                  key={repository.repositoryKey}
                  onSelect={() => props.onSelectionChange?.({ ...props.selection, repositoryKey: repository.repositoryKey })}
                >
                  <FolderGit2 size={13} />
                  <span className="vk-diffbar-menu-main">{repository.name}</span>
                  {repository.relativePath !== repository.name ? <small>{repository.relativePath}</small> : null}
                  {repository.repositoryKey === selectedRepositoryKey ? <Check className="vk-diffbar-check" size={13} /> : null}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
        <DiffModeMenu
          changedFileCount={props.snapshot.stats.files}
          commits={props.snapshot.recentCommits ?? []}
          selection={props.selection}
          onSelectionChange={props.onSelectionChange}
        />
        {titleText ? <span className="vk-diffbar-title" title={titleText}>{titleText}</span> : null}
        <span className="vk-filestat">
          <span className="vk-add">+{props.snapshot.stats.additions}</span>
          <span className="vk-del">−{props.snapshot.stats.deletions}</span>
        </span>
        <div className="vk-diffbar-acts">
          <SegmentedControl aria-label={t("diff.view.label")} onValueChange={(value) => props.onViewModeChange(value === "split" ? "split" : "unified")} value={props.viewMode}>
            <SegmentedControlItem aria-label={t("diff.view.unified")} title={t("diff.view.unified")} value="unified"><Rows3 size={13} /></SegmentedControlItem>
            <SegmentedControlItem aria-label={t("diff.view.split")} title={t("diff.view.split")} value="split"><Columns2 size={13} /></SegmentedControlItem>
          </SegmentedControl>
          {props.onSearchQueryChange ? (
            <IconButton
              aria-expanded={searchOpen}
              aria-label={t("diff.search.open")}
              data-active={props.searchQuery ? "" : undefined}
              onClick={() => setSearchOpen((open) => !open)}
              ref={searchTriggerRef}
              size="sm"
              title={t("diff.search.label")}
            >
              <Search size={14} />
            </IconButton>
          ) : null}
          {props.onOpenGitCommitAction ? (
            <IconButton
              aria-label={props.gitCommitActionLabel}
              disabled={Boolean(props.gitCommitActionBusy || props.gitCommitActionDisabled)}
              onClick={props.onOpenGitCommitAction}
              size="sm"
              title={props.gitCommitActionLabel}
            >
              <GitCommitHorizontal className={props.gitCommitActionBusy ? "vk-spin" : undefined} size={14} />
            </IconButton>
          ) : null}
          {props.onRefresh ? (
            <IconButton aria-label={t("diff.refresh")} disabled={props.loading} onClick={() => props.onRefresh?.()} size="sm" title={t("diff.refresh")}>
              <RefreshCw className={props.loading ? "vk-spin" : undefined} size={14} />
            </IconButton>
          ) : null}
          <DiffMoreMenu
            preferences={props.renderPreferences}
            rawDiff={props.snapshot.rawDiff}
            onCopyRaw={props.onCopyRaw}
            onPreferencesChange={props.onRenderPreferencesChange}
          />
        </div>
      </div>
      {props.selection.mode === "branch" ? (
        <div className="vk-diffbar-row" data-row="branch">
          <span className="vk-diffbar-branch" data-readonly="" title={baseBranch || t("diff.branch.current")}>
            <GitBranch aria-hidden="true" size={13} />
            <span>{baseBranch ? branchDisplayName(baseBranch, props.snapshot.branches ?? []) : t("diff.branch.current")}</span>
          </span>
          <span aria-hidden="true" className="vk-faint">→</span>
          <BranchSelector
            branches={props.snapshot.branches ?? []}
            fallbackLabel={compareBranch || t("diff.branch.main")}
            value={compareBranch}
            onChange={(branch) => props.onSelectionChange?.({ ...props.selection, compareBranch: branch || null })}
          />
        </div>
      ) : null}
      {searchOpen && props.onSearchQueryChange ? (
        <DiffSearchRow
          canSearchNext={Boolean(props.canSearchNext)}
          canSearchPrevious={Boolean(props.canSearchPrevious)}
          query={props.searchQuery ?? ""}
          resultLabel={props.searchResultLabel ?? ""}
          resultTruncated={Boolean(props.searchResultTruncated)}
          onClose={closeSearch}
          onNext={props.onSearchNext}
          onPrevious={props.onSearchPrevious}
          onQueryChange={props.onSearchQueryChange}
          onSearchSubmit={props.onSearchSubmit}
        />
      ) : null}
    </header>
  );
}

// DiffMoreMenu holds copying the raw diff and the display options, each shown as on or off.
// DiffMoreMenu 收纳复制原始 diff 与各项显示选项，每项以开或关呈现。
function DiffMoreMenu(props: {
  // Current target-compatible rendering preferences.
  // 当前目标兼容渲染偏好。
  preferences: DiffRenderPreferences;
  // Raw unified diff text used by the copy action.
  // 复制操作使用的原始 unified diff 文本。
  rawDiff: string;
  // Optional copy action receiving raw diff text.
  // 接收原始 diff 文本的可选复制动作。
  onCopyRaw?: (rawDiff: string) => void | Promise<void>;
  // Preference change handler.
  // 偏好变更处理器。
  onPreferencesChange: (preferences: DiffRenderPreferences) => void;
}) {
  const { t } = useI18n();
  // Keep clipboard failures inside the initiating menu and prevent overlapping copies.
  // 将剪贴板错误留在发起操作的菜单边界，并防止复制重叠。
  const copyingRef = React.useRef(false);
  const [copying, setCopying] = React.useState(false);
  const copyRawDiff = React.useCallback(async () => {
    if (!props.onCopyRaw || copyingRef.current) return;
    copyingRef.current = true;
    setCopying(true);
    try {
      await props.onCopyRaw(props.rawDiff);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      copyingRef.current = false;
      setCopying(false);
    }
  }, [props]);
  const set = (patch: Partial<DiffRenderPreferences>) => props.onPreferencesChange({ ...props.preferences, ...patch });
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <IconButton aria-label={t("diff.moreActions")} size="sm" title={t("diff.moreActions")}>
          <MoreHorizontal size={14} />
        </IconButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="vk-diffbar-menu" sideOffset={4}>
        {props.onCopyRaw ? (
          <>
            <DropdownMenuItem disabled={copying} onSelect={() => void copyRawDiff()}>
              <Copy size={13} />
              <span>{t("diff.copyRaw")}</span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        ) : null}
        <DropdownMenuLabel>{t("diff.pref.title")}</DropdownMenuLabel>
        <DropdownMenuCheckboxItem checked={props.preferences.inlineDiff} onCheckedChange={(on) => set({ inlineDiff: Boolean(on) })}>
          {t("diff.pref.inline")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem checked={props.preferences.overflowMode === "wrap"} onCheckedChange={(on) => set({ overflowMode: on ? "wrap" : "scroll" })}>
          {t("diff.pref.wrap")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem checked={props.preferences.hunkSeparatorStyle === "line-info"} onCheckedChange={(on) => set({ hunkSeparatorStyle: on ? "line-info" : "simple" })}>
          {t("diff.pref.contextControls")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem checked={props.preferences.indicatorStyle === "bars"} onCheckedChange={(on) => set({ indicatorStyle: on ? "bars" : "classic" })}>
          {t("diff.pref.bars")}
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// DiffSearchRow searches the diff from a row under the toolbar: Enter goes to the next match, Shift+Enter to
// the previous one, and Escape closes the row and returns focus to the search button.
// DiffSearchRow 在工具栏下方一行中搜索 diff：Enter 跳到下一处，Shift+Enter 跳到上一处，Escape 关闭该行并将焦点还给搜索按钮。
function DiffSearchRow(props: {
  query: string;
  // Compact result label.
  // 紧凑结果标签。
  resultLabel: string;
  // Whether result count was capped.
  // 结果数量是否被截断。
  resultTruncated: boolean;
  canSearchPrevious: boolean;
  canSearchNext: boolean;
  onQueryChange: (query: string) => void;
  onPrevious?: () => void;
  onNext?: () => void;
  // Search submit handler used by Enter navigation.
  // Enter 导航使用的搜索提交处理器。
  onSearchSubmit?: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const inputRef = React.useRef<HTMLInputElement | null>(null);
  React.useEffect(() => {
    const frameId = window.requestAnimationFrame(() => inputRef.current?.focus());
    return () => window.cancelAnimationFrame(frameId);
  }, []);
  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      props.onClose();
      return;
    }
    if (event.key === "Enter" && event.target === inputRef.current) {
      event.preventDefault();
      if (event.shiftKey) props.onPrevious?.();
      else props.onSearchSubmit?.();
    }
  };
  return (
    <div className="vk-diffbar-row" data-row="search" onKeyDown={handleKeyDown}>
      <div className="vk-input-wrap">
        <Search aria-hidden="true" size={13} />
        <input
          aria-label={t("diff.search.label")}
          autoComplete="off"
          className="vk-input"
          data-size="sm"
          onChange={(event) => props.onQueryChange(event.currentTarget.value)}
          placeholder={t("diff.search.label")}
          ref={inputRef}
          value={props.query}
        />
      </div>
      {props.query ? (
        <span className="vk-diffbar-count" title={props.resultTruncated ? t("diff.search.truncated") : undefined}>{props.resultLabel}</span>
      ) : null}
      <IconButton aria-label={t("diff.search.previous")} disabled={!props.canSearchPrevious} onClick={() => props.onPrevious?.()} size="sm" title={t("diff.search.previous")}>
        <ChevronUp size={14} />
      </IconButton>
      <IconButton aria-label={t("diff.search.next")} disabled={!props.canSearchNext} onClick={() => props.onNext?.()} size="sm" title={t("diff.search.next")}>
        <ChevronDown size={14} />
      </IconButton>
    </div>
  );
}

// DiffModeMenu picks where the reviewed changes come from and shows how many files that source changes.
// DiffModeMenu 选择所审核变更的来源，并展示该来源变更的文件数。
function DiffModeMenu(props: {
  // Current Git review source selection.
  // 当前 Git 审核来源选择。
  selection: DiffReviewSelection;
  // Number of files changed by the current diff source.
  // 当前 diff 来源中发生变动的文件数量。
  changedFileCount: number;
  // Unpushed commits available for commit review mode.
  // 提交审核模式可用的未推送提交。
  commits: DiffCommitSummary[];
  // Optional selection change handler.
  // 可选选择变更处理器。
  onSelectionChange?: (selection: DiffReviewSelection) => void;
}) {
  const { locale, t } = useI18n();
  const nowMs = Date.now();
  const selectMode = (mode: DiffReviewMode) => props.onSelectionChange?.({ ...props.selection, mode });
  const selectedCommitHash = props.selection.commitHash ?? props.commits[0]?.hash ?? null;
  const mark = (mode: DiffReviewMode) => props.selection.mode === mode ? <Check className="vk-diffbar-check" size={13} /> : null;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button aria-label={t("diff.source")} className="vk-diffbar-pick" data-kind="mode" type="button">
          <span>{t(DIFF_REVIEW_MODE_LABEL_KEYS[props.selection.mode])}</span>
          {props.selection.mode !== "commit" ? <span className="vk-diffbar-count">{props.changedFileCount}</span> : null}
          <ChevronDown aria-hidden="true" size={12} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="vk-diffbar-menu" sideOffset={4}>
        <DropdownMenuItem onSelect={() => selectMode("unstaged")}>
          <span className="vk-diffbar-menu-main">{t("diff.mode.unstaged")}</span>
          {mark("unstaged")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => selectMode("staged")}>
          <span className="vk-diffbar-menu-main">{t("diff.mode.staged")}</span>
          {mark("staged")}
        </DropdownMenuItem>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <span className="vk-diffbar-menu-main">{t("diff.mode.commit")}</span>
            {mark("commit")}
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="vk-diffbar-menu" data-kind="commits" sideOffset={8}>
            {props.commits.length ? props.commits.map((commit) => (
              <DropdownMenuItem
                key={commit.hash}
                onSelect={() => props.onSelectionChange?.({ ...props.selection, commitHash: commit.hash, mode: "commit" })}
              >
                <GitCommitHorizontal size={13} />
                <span className="vk-diffbar-menu-main">{commit.subject}</span>
                {Number.isFinite(commit.committedAtMs) ? (
                  <small title={formatLocalDateTime(commit.committedAtMs, locale)}>{formatCompactRelativeTime(nowMs - commit.committedAtMs, locale)}</small>
                ) : null}
                {selectedCommitHash === commit.hash ? <Check className="vk-diffbar-check" size={13} /> : null}
              </DropdownMenuItem>
            )) : (
              <DropdownMenuItem disabled>{t("diff.commit.none")}</DropdownMenuItem>
            )}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuItem onSelect={() => selectMode("branch")}>
          <span className="vk-diffbar-menu-main">{t("diff.mode.branch")}</span>
          {mark("branch")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => selectMode("last_turn")}>
          <span className="vk-diffbar-menu-main">{t("diff.mode.lastTurn")}</span>
          {mark("last_turn")}
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
  const { t } = useI18n();
  const [query, setQuery] = React.useState("");
  const branchOptions = React.useMemo(() => buildBranchSelectorOptions(props.branches, props.value), [props.branches, props.value]);
  const filteredBranches = React.useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return normalizedQuery ? branchOptions.filter((option) => option.searchText.includes(normalizedQuery)) : branchOptions;
  }, [branchOptions, query]);
  const label = props.value ? branchDisplayName(props.value, props.branches) : props.fallbackLabel;
  return (
    <DropdownMenu onOpenChange={(open) => { if (!open) setQuery(""); }}>
      <DropdownMenuTrigger asChild>
        <button className="vk-diffbar-branch" title={props.value || props.fallbackLabel} type="button">
          <GitBranch aria-hidden="true" size={13} />
          <span>{label}</span>
          <ChevronDown aria-hidden="true" size={12} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="vk-diffbar-menu" data-kind="branches" sideOffset={5}>
        <div className="vk-input-wrap vk-diffbar-menu-search" onKeyDown={(event) => event.stopPropagation()}>
          <Search aria-hidden="true" size={13} />
          <input
            aria-label={t("diff.branch.search")}
            className="vk-input"
            data-size="sm"
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder={t("diff.branch.search")}
            value={query}
          />
        </div>
        <DropdownMenuLabel>{t("diff.branch.label")}</DropdownMenuLabel>
        {filteredBranches.length ? filteredBranches.map((option) => (
          <DropdownMenuItem key={option.branch.name} onSelect={() => props.onChange(option.branch.name)}>
            <GitBranch size={13} />
            <span className="vk-diffbar-menu-main">{option.label}</span>
            {branchNamesEquivalent(props.value, option.branch.name) ? <Check className="vk-diffbar-check" size={13} /> : null}
          </DropdownMenuItem>
        )) : (
          <DropdownMenuItem disabled>{t("diff.branch.noMatches")}</DropdownMenuItem>
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
