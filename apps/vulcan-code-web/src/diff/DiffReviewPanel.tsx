import React from "react";
import hljs from "highlight.js/lib/core";
import type { LanguageFn } from "highlight.js";
import { Check, ChevronDown, ChevronRight, Copy, FileText, GitBranch, RefreshCw, Search } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { DiffBranchSummary, DiffCommitSummary, DiffFile, DiffLine, DiffPanelSnapshot, DiffReviewMode, DiffReviewSelection } from "./types";
import "./diffReview.css";

// HighlightLanguageModule describes one dynamically imported highlight.js language module.
// HighlightLanguageModule 描述一个动态导入的 highlight.js 语言模块。
interface HighlightLanguageModule {
  // Default export may contain the language factory in ESM builds.
  // 默认导出在 ESM 构建中可能包含语言工厂。
  default?: LanguageFn;
  // Some dev transforms expose the language factory under a named export.
  // 部分开发态转换会把语言工厂暴露在命名导出中。
  [key: string]: unknown;
}

// ImportMetaWithGlob exposes Vite's glob import in projects without vite/client ambient types.
// ImportMetaWithGlob 在未引入 vite/client 全局类型的项目中暴露 Vite 的 glob 导入。
type ImportMetaWithGlob = ImportMeta & {
  // glob builds a lazy loader map for matching modules.
  // glob 为匹配模块构建懒加载器映射。
  glob: <T>(pattern: string) => Record<string, () => Promise<T>>;
};

// highlightLanguageModules contains every highlight.js language chunk and loads each only when needed.
// highlightLanguageModules 包含所有 highlight.js 语言分块，并且仅在需要时加载对应语言。
const highlightLanguageModules = (import.meta as ImportMetaWithGlob).glob<HighlightLanguageModule>("/node_modules/highlight.js/es/languages/*.js");

// pendingHighlightLanguageLoads de-duplicates concurrent language imports.
// pendingHighlightLanguageLoads 去重并发的语言导入请求。
const pendingHighlightLanguageLoads = new Map<string, Promise<void>>();

// diffFileRowCountCache stores file row counts for the lifetime of parsed diff objects.
// diffFileRowCountCache 在已解析 diff 对象生命周期内缓存文件行数。
const diffFileRowCountCache = new WeakMap<DiffFile, number>();

// diffVirtualLayoutCache stores virtual layouts for stable parsed diff file objects.
// diffVirtualLayoutCache 为稳定的已解析 diff 文件对象缓存虚拟布局。
const diffVirtualLayoutCache = new WeakMap<DiffFile, DiffVirtualLayout>();

// DIFF_LINE_HEIGHT is the fixed visual height for one diff code row.
// DIFF_LINE_HEIGHT 是单条 diff 代码行的固定视觉高度。
const DIFF_LINE_HEIGHT = 22;

// DIFF_HUNK_HEADER_HEIGHT is the fixed visual height for one hunk header row.
// DIFF_HUNK_HEADER_HEIGHT 是单条 hunk 头部行的固定视觉高度。
const DIFF_HUNK_HEADER_HEIGHT = 29;

// DIFF_VIRTUALIZATION_THRESHOLD enables row virtualization only for large files.
// DIFF_VIRTUALIZATION_THRESHOLD 仅对大文件启用行虚拟化。
const DIFF_VIRTUALIZATION_THRESHOLD = 400;

// DIFF_HIGHLIGHT_ROW_LIMIT disables syntax highlighting for large files before virtualization.
// DIFF_HIGHLIGHT_ROW_LIMIT 在虚拟化前对大文件关闭语法高亮。
const DIFF_HIGHLIGHT_ROW_LIMIT = 2000;

// DIFF_VIRTUAL_OVERSCAN_PX keeps nearby rows mounted to avoid blank edges during fast scroll.
// DIFF_VIRTUAL_OVERSCAN_PX 保留视口附近行，避免快速滚动时出现空白边缘。
const DIFF_VIRTUAL_OVERSCAN_PX = 900;

// DIFF_REVIEW_MODE_LABELS maps review modes to compact UI labels.
// DIFF_REVIEW_MODE_LABELS 将审核模式映射为紧凑界面标签。
const DIFF_REVIEW_MODE_LABELS: Record<DiffReviewMode, string> = {
  unstaged: "未暂存",
  staged: "已暂存",
  commit: "提交",
  branch: "分支",
};

// DIFF_HIGHLIGHT_EXTENSION_MAP maps common file extensions to highlight.js language ids.
// DIFF_HIGHLIGHT_EXTENSION_MAP 将常见文件扩展名映射到 highlight.js 语言标识。
const DIFF_HIGHLIGHT_EXTENSION_MAP: Record<string, string> = {
  bash: "bash",
  bat: "dos",
  c: "c",
  cc: "cpp",
  clj: "clojure",
  cmake: "cmake",
  cmd: "dos",
  cpp: "cpp",
  cs: "csharp",
  css: "css",
  cts: "typescript",
  cxx: "cpp",
  dart: "dart",
  diff: "diff",
  dockerfile: "dockerfile",
  ex: "elixir",
  exs: "elixir",
  fs: "fsharp",
  go: "go",
  graphql: "graphql",
  h: "c",
  hpp: "cpp",
  hs: "haskell",
  html: "xml",
  htm: "xml",
  ini: "ini",
  java: "java",
  js: "javascript",
  json: "json",
  jsonc: "json",
  jsonl: "json",
  jsx: "javascript",
  kt: "kotlin",
  kts: "kotlin",
  less: "less",
  lua: "lua",
  m: "objectivec",
  md: "markdown",
  mdx: "markdown",
  mjs: "javascript",
  mm: "objectivec",
  mts: "typescript",
  php: "php",
  pl: "perl",
  ps1: "powershell",
  psd1: "powershell",
  psm1: "powershell",
  py: "python",
  rb: "ruby",
  rs: "rust",
  sass: "scss",
  scala: "scala",
  scss: "scss",
  sh: "bash",
  sql: "sql",
  svelte: "xml",
  swift: "swift",
  toml: "ini",
  ts: "typescript",
  tsx: "typescript",
  vue: "xml",
  webmanifest: "json",
  xml: "xml",
  yaml: "yaml",
  yml: "yaml",
  zsh: "bash",
};

// DIFF_HIGHLIGHT_FILE_NAME_MAP maps extensionless special files to highlight.js language ids.
// DIFF_HIGHLIGHT_FILE_NAME_MAP 将无扩展特殊文件名映射到 highlight.js 语言标识。
const DIFF_HIGHLIGHT_FILE_NAME_MAP: Record<string, string> = {
  ".bashrc": "bash",
  ".env": "bash",
  ".gitconfig": "ini",
  ".gitignore": "bash",
  ".npmrc": "ini",
  "cargo.lock": "ini",
  "cmakelists.txt": "cmake",
  "dockerfile": "dockerfile",
  "makefile": "makefile",
  "package-lock.json": "json",
  "package.json": "json",
  "pnpm-lock.yaml": "yaml",
  "tsconfig.json": "json",
};

// DiffScrollState stores the scroll viewport used by file-level virtualizers.
// DiffScrollState 保存文件级虚拟列表使用的滚动视口。
interface DiffScrollState {
  // Current scroll offset of the diff file list.
  // diff 文件列表当前滚动偏移。
  scrollTop: number;
  // Visible height of the diff file list.
  // diff 文件列表当前可见高度。
  clientHeight: number;
  // Monotonic revision that updates when the viewport changes.
  // 视口变化时递增的修订号。
  revision: number;
}

// DiffVirtualHunkRange stores row and pixel offsets for one hunk.
// DiffVirtualHunkRange 保存单个 hunk 的行偏移与像素偏移。
interface DiffVirtualHunkRange {
  // Index of the source hunk.
  // 源 hunk 索引。
  hunkIndex: number;
  // First virtual row occupied by this hunk header.
  // 该 hunk 头部占用的首个虚拟行。
  startRow: number;
  // Exclusive virtual row after this hunk.
  // 该 hunk 之后的排他虚拟行。
  endRow: number;
  // Pixel top of this hunk in the file body.
  // 该 hunk 在文件正文中的像素顶部。
  top: number;
  // Pixel height of this hunk.
  // 该 hunk 的像素高度。
  height: number;
}

// DiffVirtualLayout stores compact per-hunk indexes without duplicating every line.
// DiffVirtualLayout 保存紧凑的 hunk 索引，避免复制每一行。
interface DiffVirtualLayout {
  // Compact hunk offset ranges.
  // 紧凑的 hunk 偏移范围。
  hunkRanges: DiffVirtualHunkRange[];
  // Total virtual row count.
  // 虚拟行总数。
  totalRows: number;
  // Total pixel height.
  // 总像素高度。
  totalHeight: number;
}

// DiffVirtualRange stores the mounted row window and spacer heights.
// DiffVirtualRange 保存已挂载行窗口与占位高度。
interface DiffVirtualRange {
  // Top spacer height.
  // 顶部占位高度。
  paddingTop: number;
  // Bottom spacer height.
  // 底部占位高度。
  paddingBottom: number;
}

// DiffMountedVirtualRow stores a visible-only row created for the current viewport.
// DiffMountedVirtualRow 保存仅为当前视口创建的可见行。
type DiffMountedVirtualRow =
  | {
      // Row kind for a hunk header.
      // hunk 头部行类型。
      kind: "hunk";
      // Stable row key.
      // 稳定行键。
      key: string;
      // Header text.
      // 头部文本。
      header: string;
      // Pixel top inside the file body.
      // 文件正文中的像素顶部。
      top: number;
      // Fixed row height.
      // 固定行高。
      height: number;
    }
  | {
      // Row kind for a normal diff line.
      // 普通 diff 行类型。
      kind: "line";
      // Stable row key.
      // 稳定行键。
      key: string;
      // Parsed diff line.
      // 已解析 diff 行。
      line: DiffLine;
      // Pixel top inside the file body.
      // 文件正文中的像素顶部。
      top: number;
      // Fixed row height.
      // 固定行高。
      height: number;
    };

// DiffMountedVirtualWindow stores the rows and spacers for the current viewport.
// DiffMountedVirtualWindow 保存当前视口的行与占位高度。
interface DiffMountedVirtualWindow extends DiffVirtualRange {
  // Visible-only mounted rows.
  // 仅当前可见范围内挂载的行。
  rows: DiffMountedVirtualRow[];
}

// DiffReviewPanel renders one structured Git diff snapshot.
// DiffReviewPanel 渲染单个结构化 Git diff 快照。
export function DiffReviewPanel(props: {
  // Parsed diff snapshot to render.
  // 要渲染的已解析 diff 快照。
  snapshot: DiffPanelSnapshot | null;
  // Whether the panel is currently loading.
  // 面板当前是否正在加载。
  loading?: boolean;
  // Current Git review source selection.
  // 当前 Git 审核来源选择。
  selection?: DiffReviewSelection;
  // Optional empty-state title.
  // 可选空状态标题。
  emptyTitle?: string;
  // Optional empty-state body.
  // 可选空状态正文。
  emptyBody?: string;
  // Optional refresh action.
  // 可选刷新动作。
  onRefresh?: () => void;
  // Optional selection change handler.
  // 可选选择变更处理器。
  onSelectionChange?: (selection: DiffReviewSelection) => void;
  // Optional copy action receiving raw diff text.
  // 接收原始 diff 文本的可选复制动作。
  onCopyRaw?: (rawDiff: string) => void;
}) {
  const fileListRef = React.useRef<HTMLDivElement | null>(null);
  const [scrollState, handleFileListScroll] = useDiffScrollState(fileListRef);
  const snapshot = props.snapshot;
  const selection = props.selection ?? { mode: "unstaged" };

  if (!snapshot) {
    return (
      <section className="diff-review-panel empty">
        <div className="diff-empty-state">
          <FileText size={22} />
          <strong>{props.emptyTitle ?? "Git Diff"}</strong>
          <span>{props.emptyBody ?? "当前没有可展示的 Git Diff。"}</span>
          {props.onRefresh ? (
            <button className="diff-action-button" disabled={props.loading} onClick={() => props.onRefresh?.()} type="button">
              <RefreshCw size={14} />
              <span>{props.loading ? "加载中" : "刷新"}</span>
            </button>
          ) : null}
        </div>
      </section>
    );
  }

  return (
    <section className="diff-review-panel">
      <DiffPanelToolbar
        loading={props.loading}
        selection={selection}
        snapshot={snapshot}
        onCopyRaw={props.onCopyRaw}
        onRefresh={props.onRefresh}
        onSelectionChange={props.onSelectionChange}
      />
      <div className="diff-file-list" onScroll={handleFileListScroll} ref={fileListRef}>
        {snapshot.files.length ? snapshot.files.map((file) => (
          <DiffFileCard
            file={file}
            key={file.key}
            scrollContainerRef={fileListRef}
            scrollState={scrollState}
          />
        )) : (
          <div className="diff-empty-state in-list">
            <FileText size={20} />
            <strong>{props.emptyTitle ?? "Git Diff"}</strong>
            <span>{snapshot.notice ?? props.emptyBody ?? "当前没有可展示的 Git Diff。"}</span>
          </div>
        )}
      </div>
    </section>
  );
}

// DiffPanelToolbar renders the diff source selector and aggregate actions.
// DiffPanelToolbar 渲染 diff 来源选择器与聚合操作。
function DiffPanelToolbar(props: {
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

// DiffFileCardProps stores render inputs for one memoized file card.
// DiffFileCardProps 保存单个记忆化文件卡片的渲染输入。
interface DiffFileCardProps {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
}

// DiffFileCard renders one file-level diff card.
// DiffFileCard 渲染单个文件级 diff 卡片。
const DiffFileCard = React.memo(function DiffFileCard(props: DiffFileCardProps) {
  const [collapsed, setCollapsed] = React.useState(false);
  const ToggleIcon = collapsed ? ChevronRight : ChevronDown;
  const rowCount = React.useMemo(() => fileTotalRowCount(props.file), [props.file]);
  const shouldHighlight = rowCount <= DIFF_HIGHLIGHT_ROW_LIMIT;
  const language = !collapsed && shouldHighlight ? diffLanguageForFile(props.file) : null;
  const highlightReady = useHighlightLanguage(language);
  const handleToggle = React.useCallback(() => {
    setCollapsed((previous) => !previous);
  }, []);
  return (
    <article className={`diff-file-card ${props.file.status}`}>
      <button className="diff-file-header" onClick={handleToggle} type="button">
        <ToggleIcon size={15} />
        <div className="diff-file-paths">
          <strong>
            <span>{displayPath(props.file)}</span>
            {props.file.status === "added" ? <span aria-label="新增文件" className="diff-file-status-dot" /> : null}
          </strong>
          {props.file.status === "renamed" ? <span>{`${props.file.oldPath} -> ${props.file.newPath}`}</span> : null}
        </div>
        <div className="diff-file-meta">
          <span className="diff-stat added">+{props.file.additions}</span>
          <span className="diff-stat removed">-{props.file.deletions}</span>
        </div>
      </button>
      {!collapsed ? (
        <div className="diff-file-body">
          {props.file.isBinary ? (
            <div className="diff-binary-note">二进制文件变更，未渲染文本内容。</div>
          ) : props.file.hunks.length ? (
            <DiffFileLines
              file={props.file}
              highlightReady={highlightReady}
              language={language}
              rowCount={rowCount}
              scrollContainerRef={props.scrollContainerRef}
              scrollState={props.scrollState}
            />
          ) : (
            <div className="diff-binary-note">该文件没有可渲染的文本 hunk。</div>
          )}
        </div>
      ) : null}
    </article>
  );
}, areDiffFileCardPropsEqual);

// DiffFileLines switches between direct and virtualized rendering for one file.
// DiffFileLines 为单个文件在直接渲染和虚拟渲染之间切换。
function DiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the language module is registered and ready.
  // 语言模块是否已经注册并可用。
  highlightReady: boolean;
  // Precomputed visual row count for this file.
  // 该文件预先计算的可视行数。
  rowCount: number;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
}) {
  if (props.rowCount < DIFF_VIRTUALIZATION_THRESHOLD) {
    return (
      <>
        {props.file.hunks.map((hunk, index) => (
          <section className="diff-hunk" key={`${props.file.key}-hunk-${index}`}>
            <div className="diff-hunk-header">{hunk.header}</div>
            <div className="diff-lines">
              {hunk.lines.map((line, lineIndex) => (
                <DiffLineRow
                  highlightReady={props.highlightReady}
                  key={`${props.file.key}-${index}-${lineIndex}`}
                  language={props.language}
                  line={line}
                />
              ))}
            </div>
          </section>
        ))}
      </>
    );
  }
  return (
    <VirtualDiffFileLines
      file={props.file}
      highlightReady={props.highlightReady}
      language={props.language}
      scrollContainerRef={props.scrollContainerRef}
      scrollState={props.scrollState}
    />
  );
}

// areDiffFileCardPropsEqual avoids re-rendering unaffected file cards during scroll or collapse.
// areDiffFileCardPropsEqual 避免滚动或折叠时重渲染未受影响的文件卡片。
function areDiffFileCardPropsEqual(previous: DiffFileCardProps, next: DiffFileCardProps): boolean {
  if (
    previous.file !== next.file
    || previous.scrollContainerRef !== next.scrollContainerRef
  ) {
    return false;
  }
  if (fileTotalRowCount(next.file) < DIFF_VIRTUALIZATION_THRESHOLD) {
    return true;
  }
  return previous.scrollState.scrollTop === next.scrollState.scrollTop
    && previous.scrollState.clientHeight === next.scrollState.clientHeight
    && previous.scrollState.revision === next.scrollState.revision;
}

// VirtualDiffFileLines renders only the visible rows for one large file diff.
// VirtualDiffFileLines 仅渲染大文件 diff 当前可见的行。
function VirtualDiffFileLines(props: {
  // File-level diff data.
  // 文件级 diff 数据。
  file: DiffFile;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the language module is registered and ready.
  // 语言模块是否已经注册并可用。
  highlightReady: boolean;
  // Shared scroll container used by virtualized file bodies.
  // 虚拟化文件正文使用的共享滚动容器。
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  // Current shared scroll viewport state.
  // 当前共享滚动视口状态。
  scrollState: DiffScrollState;
}) {
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  const [hostTop, setHostTop] = React.useState(0);
  const layout = React.useMemo(() => buildDiffVirtualLayout(props.file), [props.file]);
  const window = React.useMemo(
    () => mountedDiffRows(props.file, layout, props.scrollState.scrollTop - hostTop, props.scrollState.clientHeight),
    [hostTop, layout, props.file, props.scrollState.clientHeight, props.scrollState.scrollTop],
  );

  React.useLayoutEffect(() => {
    const host = hostRef.current;
    const container = props.scrollContainerRef.current;
    if (!host || !container) {
      return undefined;
    }
    const nextTop = elementTopWithinScrollContainer(host, container);
    setHostTop((previousTop) => (Math.abs(previousTop - nextTop) > 0.5 ? nextTop : previousTop));
    const frame = globalThis.requestAnimationFrame(() => {
      const currentHost = hostRef.current;
      const currentContainer = props.scrollContainerRef.current;
      if (!currentHost || !currentContainer) {
        return;
      }
      const delayedTop = elementTopWithinScrollContainer(currentHost, currentContainer);
      setHostTop((previousTop) => (Math.abs(previousTop - delayedTop) > 0.5 ? delayedTop : previousTop));
    });
    return () => globalThis.cancelAnimationFrame(frame);
  }, [layout.totalHeight, props.scrollContainerRef, props.scrollState.revision]);

  return (
    <div className="diff-virtual-lines" ref={hostRef}>
      {window.paddingTop > 0 ? <div aria-hidden="true" className="diff-virtual-spacer" style={{ height: window.paddingTop }} /> : null}
      {window.rows.map((row) => (
        row.kind === "hunk" ? (
          <div className="diff-hunk-header virtual" key={row.key}>{row.header}</div>
        ) : (
          <DiffLineRow
            highlightReady={props.highlightReady}
            key={row.key}
            language={props.language}
            line={row.line}
          />
        )
      ))}
      {window.paddingBottom > 0 ? <div aria-hidden="true" className="diff-virtual-spacer" style={{ height: window.paddingBottom }} /> : null}
    </div>
  );
}

// DiffLineRow renders one memoized line-level diff row.
// DiffLineRow 渲染单个带记忆化的行级 diff 行。
const DiffLineRow = React.memo(function DiffLineRow(props: {
  // Parsed line data.
  // 已解析行数据。
  line: DiffLine;
  // Syntax group inferred from the owning file path.
  // 从所属文件路径推断出的语法分组。
  language: string | null;
  // Whether the language module is registered and ready.
  // 语言模块是否已经注册并可用。
  highlightReady: boolean;
}) {
  const visualLineNumber = props.line.kind === "removed" ? props.line.oldLine : props.line.newLine;
  const highlightedHtml = React.useMemo(
    () => (!props.highlightReady || props.line.kind === "metadata" || !props.language
      ? escapeHtml(props.line.content || " ")
      : highlightedDiffLineHtml(props.line.content || " ", props.language)),
    [props.highlightReady, props.language, props.line.content, props.line.kind],
  );
  return (
    <div className={`diff-line-row ${props.line.kind}`}>
      <span className="diff-line-number">{formatLineNumber(visualLineNumber)}</span>
      <code dangerouslySetInnerHTML={{ __html: highlightedHtml }} />
    </div>
  );
});

// useDiffScrollState tracks the scroll viewport for the diff file list.
// useDiffScrollState 跟踪 diff 文件列表的滚动视口。
function useDiffScrollState(containerRef: React.RefObject<HTMLDivElement | null>): [DiffScrollState, React.UIEventHandler<HTMLDivElement>] {
  const frameRef = React.useRef<number | null>(null);
  const [state, setState] = React.useState<DiffScrollState>({ scrollTop: 0, clientHeight: 0, revision: 0 });

  const updateState = React.useCallback(() => {
    const element = containerRef.current;
    if (!element) {
      return;
    }
    setState((previous) => {
      const nextScrollTop = element.scrollTop;
      const nextClientHeight = element.clientHeight;
      if (previous.scrollTop === nextScrollTop && previous.clientHeight === nextClientHeight) {
        return previous;
      }
      return {
        scrollTop: nextScrollTop,
        clientHeight: nextClientHeight,
        revision: previous.revision + 1,
      };
    });
  }, [containerRef]);

  const scheduleUpdate = React.useCallback(() => {
    if (frameRef.current != null) {
      return;
    }
    frameRef.current = window.requestAnimationFrame(() => {
      frameRef.current = null;
      updateState();
    });
  }, [updateState]);

  React.useLayoutEffect(() => {
    updateState();
    const element = containerRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return undefined;
    }
    const observer = new ResizeObserver(scheduleUpdate);
    observer.observe(element);
    scheduleUpdate();
    return () => {
      observer.disconnect();
      if (frameRef.current != null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
    };
  }, [containerRef, scheduleUpdate, updateState]);

  return [state, scheduleUpdate];
}

// fileTotalRowCount returns visual row count including hunk headers.
// fileTotalRowCount 返回包含 hunk 头部的可视行数量。
function fileTotalRowCount(file: DiffFile): number {
  const cached = diffFileRowCountCache.get(file);
  if (cached != null) {
    return cached;
  }
  const rowCount = file.hunks.reduce((total, hunk) => total + 1 + hunk.lines.length, 0);
  diffFileRowCountCache.set(file, rowCount);
  return rowCount;
}

// buildDiffVirtualLayout builds compact per-hunk offsets without duplicating diff lines.
// buildDiffVirtualLayout 构建紧凑的 hunk 偏移，不复制 diff 行。
function buildDiffVirtualLayout(file: DiffFile): DiffVirtualLayout {
  const cached = diffVirtualLayoutCache.get(file);
  if (cached) {
    return cached;
  }
  let nextRow = 0;
  let nextTop = 0;
  const hunkRanges = file.hunks.map<DiffVirtualHunkRange>((hunk, hunkIndex) => {
    const rowCount = 1 + hunk.lines.length;
    const height = DIFF_HUNK_HEADER_HEIGHT + hunk.lines.length * DIFF_LINE_HEIGHT;
    const range = {
      hunkIndex,
      startRow: nextRow,
      endRow: nextRow + rowCount,
      top: nextTop,
      height,
    };
    nextRow += rowCount;
    nextTop += height;
    return range;
  });
  const layout = { hunkRanges, totalRows: nextRow, totalHeight: nextTop };
  diffVirtualLayoutCache.set(file, layout);
  return layout;
}

// mountedDiffRows creates only visible rows for the current viewport.
// mountedDiffRows 仅为当前视口创建可见行。
function mountedDiffRows(file: DiffFile, layout: DiffVirtualLayout, viewportTop: number, viewportHeight: number): DiffMountedVirtualWindow {
  if (layout.totalHeight <= 0) {
    return { rows: [], paddingTop: 0, paddingBottom: 0 };
  }
  const effectiveViewportHeight = viewportHeight > 0 ? viewportHeight : DIFF_VIRTUAL_OVERSCAN_PX;
  const effectiveViewportTop = Number.isFinite(viewportTop) ? viewportTop : 0;
  const startTop = Math.max(0, effectiveViewportTop - DIFF_VIRTUAL_OVERSCAN_PX);
  const endBottom = Math.min(layout.totalHeight, effectiveViewportTop + effectiveViewportHeight + DIFF_VIRTUAL_OVERSCAN_PX);
  if (endBottom <= 0) {
    return mountedDiffRows(file, layout, 0, effectiveViewportHeight);
  }
  if (startTop >= layout.totalHeight) {
    return { rows: [], paddingTop: layout.totalHeight, paddingBottom: 0 };
  }

  const rows: DiffMountedVirtualRow[] = [];
  const firstHunkIndex = firstIntersectingHunkIndex(layout.hunkRanges, startTop);
  for (let rangeIndex = firstHunkIndex; rangeIndex < layout.hunkRanges.length; rangeIndex += 1) {
    const range = layout.hunkRanges[rangeIndex];
    if (range.top >= endBottom) {
      break;
    }
    const hunk = file.hunks[range.hunkIndex];
    if (!hunk) {
      continue;
    }
    appendMountedHunkRows(rows, file, hunk, range, startTop, endBottom);
  }

  if (!rows.length) {
    return mountedDiffRows(file, layout, 0, effectiveViewportHeight);
  }
  const firstTop = rows[0].top;
  const lastRow = rows[rows.length - 1];
  const lastBottom = lastRow.top + lastRow.height;
  return {
    rows,
    paddingTop: firstTop,
    paddingBottom: Math.max(0, layout.totalHeight - lastBottom),
  };
}

// appendMountedHunkRows appends visible rows from one hunk range.
// appendMountedHunkRows 从单个 hunk 范围追加可见行。
function appendMountedHunkRows(
  rows: DiffMountedVirtualRow[],
  file: DiffFile,
  hunk: DiffFile["hunks"][number],
  range: DiffVirtualHunkRange,
  startTop: number,
  endBottom: number,
): void {
  if (range.top + DIFF_HUNK_HEADER_HEIGHT >= startTop && range.top <= endBottom) {
    rows.push({
      kind: "hunk",
      key: `${file.key}-virtual-hunk-${range.hunkIndex}`,
      header: hunk.header,
      top: range.top,
      height: DIFF_HUNK_HEADER_HEIGHT,
    });
  }
  const linesTop = range.top + DIFF_HUNK_HEADER_HEIGHT;
  const firstLine = Math.max(0, Math.floor((startTop - linesTop) / DIFF_LINE_HEIGHT));
  const lastLine = Math.min(hunk.lines.length, Math.ceil((endBottom - linesTop) / DIFF_LINE_HEIGHT));
  for (let lineIndex = firstLine; lineIndex < lastLine; lineIndex += 1) {
    const line = hunk.lines[lineIndex];
    if (!line) {
      continue;
    }
    rows.push({
      kind: "line",
      key: `${file.key}-virtual-line-${range.hunkIndex}-${lineIndex}`,
      line,
      top: linesTop + lineIndex * DIFF_LINE_HEIGHT,
      height: DIFF_LINE_HEIGHT,
    });
  }
}

// firstIntersectingHunkIndex finds the first hunk whose pixel range intersects the viewport.
// firstIntersectingHunkIndex 查找首个像素范围与视口相交的 hunk。
function firstIntersectingHunkIndex(hunkRanges: DiffVirtualHunkRange[], top: number): number {
  let low = 0;
  let high = hunkRanges.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    const range = hunkRanges[middle];
    if (range.top + range.height < top) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

// elementTopWithinScrollContainer measures one child top in scroll-content coordinates.
// elementTopWithinScrollContainer 按滚动内容坐标测量子元素顶部。
function elementTopWithinScrollContainer(element: HTMLElement, container: HTMLElement): number {
  const elementRect = element.getBoundingClientRect();
  const containerRect = container.getBoundingClientRect();
  return elementRect.top - containerRect.top + container.scrollTop;
}

// displayPath returns the primary file path shown in the file header.
// displayPath 返回文件头中展示的主路径。
function displayPath(file: DiffFile): string {
  return file.newPath && file.newPath !== "/dev/null" ? file.newPath : file.oldPath;
}

// formatLineNumber formats optional old and new line numbers for the gutter.
// formatLineNumber 格式化 gutter 中可选的新旧行号。
function formatLineNumber(line: number | null): string {
  return line == null ? "" : String(line);
}

// diffLanguageForFile infers a highlight.js language id from one diff file path.
// diffLanguageForFile 从单个 diff 文件路径推断 highlight.js 语言标识。
function diffLanguageForFile(file: DiffFile): string | null {
  const path = displayPath(file).replace(/\\/g, "/").toLowerCase();
  const fileName = path.split("/").at(-1) ?? path;
  const extension = fileName.includes(".") ? fileName.split(".").at(-1) ?? "" : "";
  const specialName = DIFF_HIGHLIGHT_FILE_NAME_MAP[fileName];
  if (specialName) {
    return specialName;
  }
  const mappedExtension = DIFF_HIGHLIGHT_EXTENSION_MAP[extension];
  if (mappedExtension) {
    return mappedExtension;
  }
  return languageModuleExists(extension) ? extension : null;
}

// useHighlightLanguage registers one highlight.js language module on demand.
// useHighlightLanguage 按需注册单个 highlight.js 语言模块。
function useHighlightLanguage(language: string | null): boolean {
  const [, refresh] = React.useReducer((value: number) => value + 1, 0);
  React.useEffect(() => {
    if (!language || hljs.getLanguage(language)) {
      return;
    }
    let cancelled = false;
    void ensureHighlightLanguage(language).then(() => {
      if (!cancelled) {
        refresh();
      }
    });
    return () => {
      cancelled = true;
    };
  }, [language]);
  return !language || Boolean(hljs.getLanguage(language));
}

// ensureHighlightLanguage imports and registers one language module if highlight.js supports it.
// ensureHighlightLanguage 在 highlight.js 支持时导入并注册单个语言模块。
async function ensureHighlightLanguage(language: string): Promise<void> {
  if (hljs.getLanguage(language)) {
    return;
  }
  const existingLoad = pendingHighlightLanguageLoads.get(language);
  if (existingLoad) {
    return existingLoad;
  }
  const modulePath = highlightLanguageModulePath(language);
  const loader = modulePath ? highlightLanguageModules[modulePath] : undefined;
  if (!loader) {
    return;
  }
  const load = loader()
    .then((module) => {
      const languageFactory = highlightLanguageFactory(module);
      if (languageFactory && !hljs.getLanguage(language)) {
        hljs.registerLanguage(language, languageFactory);
      }
    })
    .finally(() => pendingHighlightLanguageLoads.delete(language));
  pendingHighlightLanguageLoads.set(language, load);
  return load;
}

// highlightLanguageFactory extracts a highlight.js language factory from a lazy module.
// highlightLanguageFactory 从懒加载模块中提取 highlight.js 语言工厂。
function highlightLanguageFactory(module: HighlightLanguageModule): LanguageFn | null {
  if (typeof module.default === "function") {
    return module.default;
  }
  const namedFactory = Object.values(module).find((value) => typeof value === "function");
  return typeof namedFactory === "function" ? (namedFactory as LanguageFn) : null;
}

// highlightedDiffLineHtml returns syntax-highlighted, HTML-escaped code for one diff line.
// highlightedDiffLineHtml 返回单条 diff 行经过语法高亮和 HTML 转义后的代码。
function highlightedDiffLineHtml(line: string, language: string): string {
  try {
    return hljs.highlight(line, { language, ignoreIllegals: true }).value;
  } catch {
    return escapeHtml(line);
  }
}

// languageModuleExists returns whether highlight.js has a bundled module for one language id.
// languageModuleExists 返回 highlight.js 是否内置某个语言标识对应的模块。
function languageModuleExists(language: string): boolean {
  return Boolean(highlightLanguageModulePath(language));
}

// highlightLanguageModulePath resolves the Vite glob key for one highlight.js language id.
// highlightLanguageModulePath 解析单个 highlight.js 语言标识对应的 Vite glob 键。
function highlightLanguageModulePath(language: string): string | null {
  const path = `/node_modules/highlight.js/es/languages/${language}.js`;
  return Object.prototype.hasOwnProperty.call(highlightLanguageModules, path) ? path : null;
}

// escapeHtml safely renders fallback text through dangerouslySetInnerHTML.
// escapeHtml 为 dangerouslySetInnerHTML 安全渲染兜底文本。
function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
