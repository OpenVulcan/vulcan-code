import React from "react";
import { ChevronDown, ChevronRight, FileWarning, Folder, ListCollapse, ListTree, Search } from "lucide-react";

// WORKSPACE_FILE_TREE_HOVER_DELAY_MS delays drawer hover changes to avoid accidental drag-triggered opens.
// WORKSPACE_FILE_TREE_HOVER_DELAY_MS 延迟目录抽屉 hover 状态变化，避免拖拽时误触展开。
const WORKSPACE_FILE_TREE_HOVER_DELAY_MS = 500;

// WorkspaceFileTreeNode stores one reusable directory or file-tree row.
// WorkspaceFileTreeNode 保存一个可复用目录或文件树行节点。
export interface WorkspaceFileTreeNode<RowFile> {
  // Stable tree path used for React keys and directory toggles.
  // 用于 React key 与目录展开状态的稳定树路径。
  path: string;
  // Display name for the current path segment.
  // 当前路径片段的展示名称。
  name: string;
  // Child directories and files sorted for display.
  // 排序后用于展示的子目录与文件。
  children: WorkspaceFileTreeNode<RowFile>[];
  // File payload when this node is a leaf.
  // 当前节点为叶子文件时对应的文件数据。
  file?: RowFile;
  // Optional added-line count used by the review tree.
  // 审核目录树使用的可选新增行数。
  additions?: number;
  // Optional deleted-line count used by the review tree.
  // 审核目录树使用的可选删除行数。
  deletions?: number;
}

// WorkspaceFileTreeDrawer renders the shared collapsible file-directory navigation drawer.
// WorkspaceFileTreeDrawer 渲染共享的可收起文件目录导航抽屉。
export function WorkspaceFileTreeDrawer<RowFile>(props: {
  // Tree root to render in the drawer.
  // 需要在抽屉中渲染的目录树根节点。
  tree: WorkspaceFileTreeNode<RowFile>;
  // Current selected file key.
  // 当前选中的文件 key。
  activeFileKey: string | null;
  // File tree filter text.
  // 文件目录树筛选文本。
  filter: string;
  // Whether the drawer must stay open because no file is selected.
  // 是否因未选择文件而必须保持目录抽屉展开。
  lockedOpen: boolean;
  // Whether the drawer is explicitly pinned open by the user.
  // 目录抽屉是否被用户显式固定展开。
  pinnedOpen: boolean;
  // Whether a direct file-title click should close the drawer after selecting.
  // 直接点击文件标题选择后是否自动收起抽屉。
  collapseOnSelect?: boolean;
  // Placeholder displayed inside the filter input.
  // 筛选输入框内展示的占位文本。
  filterPlaceholder?: string;
  // Empty state shown when the tree has no visible nodes.
  // 目录树没有可见节点时展示的空状态。
  emptyLabel?: string;
  // Optional controlled directory open path set.
  // 可选的受控目录展开路径集合。
  openDirectoryPaths?: ReadonlySet<string>;
  // Optional default directory open path set for uncontrolled usage.
  // 非受控用法下可选的默认目录展开路径集合。
  defaultOpenDirectoryPaths?: ReadonlySet<string>;
  // Directories that are currently loading children.
  // 当前正在加载子项的目录集合。
  loadingDirectoryPaths?: ReadonlySet<string>;
  // Label shown inside an expanded loading directory.
  // 展开且正在加载的目录内展示的文本。
  loadingDirectoryLabel?: string;
  // Label shown inside an expanded empty directory.
  // 展开且为空的目录内展示的文本。
  emptyDirectoryLabel?: string;
  // Additional controls appended to the filter action group.
  // 追加到筛选操作区域的额外控件。
  extraFilterActions?: React.ReactNode;
  // Called when the filter text changes.
  // 筛选文本变化时调用。
  onFilterChange: (filter: string) => void;
  // Called when the pinned drawer open state changes.
  // 目录抽屉固定展开状态变化时调用。
  onPinnedOpenChange: (open: boolean) => void;
  // Called when the controlled directory path set should change.
  // 受控目录展开路径集合需要变化时调用。
  onOpenDirectoryPathsChange?: (paths: ReadonlySet<string>) => void;
  // Called when one directory changes open state.
  // 单个目录展开状态变化时调用。
  onDirectoryOpenStateChange?: (path: string, open: boolean) => void;
  // Extracts a stable key from one file payload.
  // 从单个文件载荷中提取稳定 key。
  fileKey: (file: RowFile) => string;
  // Resolves the tooltip title for a file node.
  // 解析文件节点的提示标题。
  fileTitle?: (file: RowFile, node: WorkspaceFileTreeNode<RowFile>) => string;
  // Renders the compact file icon content inside the shared icon slot.
  // 在共享图标位中渲染紧凑文件图标内容。
  renderFileIcon?: (node: WorkspaceFileTreeNode<RowFile>) => React.ReactNode;
  // Resolves the class appended to the shared file icon slot.
  // 解析追加到共享文件图标位的类名。
  fileIconClassName?: (node: WorkspaceFileTreeNode<RowFile>) => string;
  // Renders trailing content at the end of a file row.
  // 渲染文件行末尾的附加内容。
  renderFileTrailing?: (node: WorkspaceFileTreeNode<RowFile>) => React.ReactNode;
  // Called when a file node is selected.
  // 选择文件节点时调用。
  onSelectFile: (file: RowFile) => void;
}) {
  // fallbackOpenDirectoryPaths expands all visible directories for scan-friendly review usage.
  // fallbackOpenDirectoryPaths 展开全部可见目录，保持审核场景的快速浏览体验。
  const fallbackOpenDirectoryPaths = React.useMemo(
    () => defaultWorkspaceFileTreeOpenDirectoryPaths(props.tree),
    [props.tree],
  );
  // defaultOpenDirectoryPaths stores the effective uncontrolled initial open set.
  // defaultOpenDirectoryPaths 保存非受控模式下生效的初始展开集合。
  const defaultOpenDirectoryPaths = props.defaultOpenDirectoryPaths ?? fallbackOpenDirectoryPaths;
  // isControlled stores whether parent state owns directory expansion.
  // isControlled 保存目录展开状态是否由父组件控制。
  const isControlled = props.openDirectoryPaths !== undefined;
  // uncontrolledOpenDirectoryPaths stores directory expansion for review-style local usage.
  // uncontrolledOpenDirectoryPaths 保存审核式本地用法的目录展开状态。
  const [uncontrolledOpenDirectoryPaths, setUncontrolledOpenDirectoryPaths] = React.useState<ReadonlySet<string>>(
    defaultOpenDirectoryPaths,
  );
  // openDirectoryPaths resolves the current controlled or uncontrolled open set.
  // openDirectoryPaths 解析当前受控或非受控的展开集合。
  const openDirectoryPaths = props.openDirectoryPaths ?? uncontrolledOpenDirectoryPaths;
  // hoverOpen stores transient hover expansion for the collapsed drawer handle.
  // hoverOpen 保存折叠抽屉把手的临时 hover 展开状态。
  const [hoverOpen, setHoverOpen] = React.useState(false);
  // suppressHoverOpen prevents immediate hover reopen after manual collapse or dragging.
  // suppressHoverOpen 避免手动收起或拖拽后立即被 hover 重新展开。
  const [suppressHoverOpen, setSuppressHoverOpen] = React.useState(false);
  // hoverOpenTimeoutRef owns the delayed hover-open timer.
  // hoverOpenTimeoutRef 持有延迟 hover 展开的计时器。
  const hoverOpenTimeoutRef = React.useRef<number | null>(null);
  // hoverCloseTimeoutRef owns the delayed hover-close timer.
  // hoverCloseTimeoutRef 持有延迟 hover 收起的计时器。
  const hoverCloseTimeoutRef = React.useRef<number | null>(null);
  // drawerOpen stores whether the drawer should currently be visible.
  // drawerOpen 保存抽屉当前是否应该可见。
  const drawerOpen = props.lockedOpen || props.pinnedOpen || hoverOpen;
  const clearHoverOpenTimeout = React.useCallback(() => {
    if (hoverOpenTimeoutRef.current != null) {
      window.clearTimeout(hoverOpenTimeoutRef.current);
      hoverOpenTimeoutRef.current = null;
    }
  }, []);
  const clearHoverCloseTimeout = React.useCallback(() => {
    if (hoverCloseTimeoutRef.current != null) {
      window.clearTimeout(hoverCloseTimeoutRef.current);
      hoverCloseTimeoutRef.current = null;
    }
  }, []);
  React.useEffect(() => {
    if (!isControlled) {
      setUncontrolledOpenDirectoryPaths(defaultOpenDirectoryPaths);
    }
  }, [defaultOpenDirectoryPaths, isControlled]);
  React.useEffect(() => {
    if (props.lockedOpen) {
      clearHoverOpenTimeout();
      clearHoverCloseTimeout();
      setHoverOpen(false);
      setSuppressHoverOpen(false);
    }
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout, props.lockedOpen]);
  React.useEffect(() => {
    return () => {
      clearHoverOpenTimeout();
      clearHoverCloseTimeout();
    };
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout]);
  const updateOpenDirectoryPaths = React.useCallback((
    updater: (current: ReadonlySet<string>) => ReadonlySet<string>,
  ) => {
    const next = updater(openDirectoryPaths);
    props.onOpenDirectoryPathsChange?.(next);
    if (!isControlled) {
      setUncontrolledOpenDirectoryPaths(next);
    }
  }, [isControlled, openDirectoryPaths, props]);
  const toggleDirectory = React.useCallback((path: string) => {
    let nextOpen = false;
    updateOpenDirectoryPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
        nextOpen = true;
      }
      return next;
    });
    props.onDirectoryOpenStateChange?.(path, nextOpen);
  }, [props, updateOpenDirectoryPaths]);
  // expandAllDirectories opens every visible directory in the filtered tree.
  // expandAllDirectories 展开当前筛选目录树里的全部可见目录。
  const expandAllDirectories = React.useCallback(() => {
    updateOpenDirectoryPaths(() => defaultOpenDirectoryPaths);
    for (const path of defaultOpenDirectoryPaths) {
      props.onDirectoryOpenStateChange?.(path, true);
    }
  }, [defaultOpenDirectoryPaths, props, updateOpenDirectoryPaths]);
  // collapseAllDirectories closes every directory in the filtered tree.
  // collapseAllDirectories 折叠当前筛选目录树里的全部目录。
  const collapseAllDirectories = React.useCallback(() => {
    for (const path of openDirectoryPaths) {
      props.onDirectoryOpenStateChange?.(path, false);
    }
    updateOpenDirectoryPaths(() => new Set<string>());
  }, [openDirectoryPaths, props, updateOpenDirectoryPaths]);
  const handleDrawerMouseEnter = React.useCallback((event: React.MouseEvent<HTMLElement>) => {
    clearHoverCloseTimeout();
    if (props.lockedOpen || suppressHoverOpen) {
      return;
    }
    if ((event.buttons & 1) === 1) {
      clearHoverOpenTimeout();
      setSuppressHoverOpen(true);
      return;
    }
    clearHoverOpenTimeout();
    hoverOpenTimeoutRef.current = window.setTimeout(() => {
      hoverOpenTimeoutRef.current = null;
      setHoverOpen(true);
    }, WORKSPACE_FILE_TREE_HOVER_DELAY_MS);
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout, props.lockedOpen, suppressHoverOpen]);
  const handleDrawerMouseLeave = React.useCallback(() => {
    clearHoverOpenTimeout();
    setSuppressHoverOpen(false);
    hoverCloseTimeoutRef.current = window.setTimeout(() => {
      hoverCloseTimeoutRef.current = null;
      setHoverOpen(false);
    }, WORKSPACE_FILE_TREE_HOVER_DELAY_MS);
  }, [clearHoverOpenTimeout]);
  const handleDrawerToggle = React.useCallback(() => {
    if (props.lockedOpen) {
      return;
    }
    clearHoverOpenTimeout();
    clearHoverCloseTimeout();
    if (drawerOpen) {
      props.onPinnedOpenChange(false);
      setHoverOpen(false);
      setSuppressHoverOpen(true);
      return;
    }
    props.onPinnedOpenChange(true);
    setSuppressHoverOpen(false);
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout, drawerOpen, props]);
  // handleSelectFile collapses only after a direct file-title selection click.
  // handleSelectFile 仅在直接点击文件标题选择后收起抽屉。
  const handleSelectFile = React.useCallback((file: RowFile) => {
    props.onSelectFile(file);
    if (!props.collapseOnSelect) {
      return;
    }
    clearHoverOpenTimeout();
    clearHoverCloseTimeout();
    props.onPinnedOpenChange(false);
    setHoverOpen(false);
    setSuppressHoverOpen(true);
  }, [clearHoverCloseTimeout, clearHoverOpenTimeout, props]);
  return (
    <aside
      className={`diff-file-tree-drawer${drawerOpen ? " open" : " collapsed"}${props.lockedOpen ? " locked-open" : ""}`}
      onMouseEnter={handleDrawerMouseEnter}
      onMouseLeave={handleDrawerMouseLeave}
    >
      <button
        aria-label={drawerOpen ? "收起文件目录" : "展开文件目录"}
        className="diff-file-tree-handle"
        onClick={handleDrawerToggle}
        type="button"
      >
        {drawerOpen ? <ChevronRight size={14} /> : <ChevronLeftIcon />}
      </button>
      <div className="diff-file-tree-panel">
        <div className="diff-file-tree-filter">
          <Search size={14} />
          <input
            aria-label="筛选文件"
            onChange={(event) => props.onFilterChange(event.currentTarget.value)}
            placeholder={props.filterPlaceholder ?? "筛选文件..."}
            value={props.filter}
          />
          <span className="diff-file-tree-filter-actions">
            <button
              aria-label="全部展开"
              disabled={fallbackOpenDirectoryPaths.size === 0}
              onClick={expandAllDirectories}
              title="全部展开"
              type="button"
            >
              <ListTree size={13} />
            </button>
            <button
              aria-label="全部折叠"
              disabled={fallbackOpenDirectoryPaths.size === 0}
              onClick={collapseAllDirectories}
              title="全部折叠"
              type="button"
            >
              <ListCollapse size={13} />
            </button>
            {props.extraFilterActions}
          </span>
        </div>
        <div className="diff-file-tree-list">
          {props.tree.children.length ? props.tree.children.map((node) => (
            <WorkspaceFileTreeNodeView
              activeFileKey={props.activeFileKey}
              depth={0}
              emptyDirectoryLabel={props.emptyDirectoryLabel}
              fileIconClassName={props.fileIconClassName}
              fileKey={props.fileKey}
              fileTitle={props.fileTitle}
              key={node.path}
              loadingDirectoryLabel={props.loadingDirectoryLabel}
              loadingDirectoryPaths={props.loadingDirectoryPaths}
              node={node}
              onSelectFile={handleSelectFile}
              onToggleDirectory={toggleDirectory}
              openDirectoryPaths={openDirectoryPaths}
              renderFileIcon={props.renderFileIcon}
              renderFileTrailing={props.renderFileTrailing}
            />
          )) : (
            <div className="diff-file-tree-empty">{props.emptyLabel ?? "没有匹配的文件"}</div>
          )}
        </div>
      </div>
    </aside>
  );
}

// WorkspaceFileTreeNodeView renders one recursive directory-tree row.
// WorkspaceFileTreeNodeView 渲染目录树中的一个递归节点行。
function WorkspaceFileTreeNodeView<RowFile>(props: {
  // Tree node to render.
  // 需要渲染的树节点。
  node: WorkspaceFileTreeNode<RowFile>;
  // Current tree nesting depth.
  // 当前目录树嵌套深度。
  depth: number;
  // Current selected file key.
  // 当前选中的文件 key。
  activeFileKey: string | null;
  // Open directory path set.
  // 已展开目录路径集合。
  openDirectoryPaths: ReadonlySet<string>;
  // Directories that are currently loading children.
  // 当前正在加载子项的目录集合。
  loadingDirectoryPaths?: ReadonlySet<string>;
  // Label shown inside an expanded loading directory.
  // 展开且正在加载的目录内展示的文本。
  loadingDirectoryLabel?: string;
  // Label shown inside an expanded empty directory.
  // 展开且为空的目录内展示的文本。
  emptyDirectoryLabel?: string;
  // Directory toggle handler.
  // 目录展开切换处理器。
  onToggleDirectory: (path: string) => void;
  // File selection handler.
  // 文件选择处理器。
  onSelectFile: (file: RowFile) => void;
  // Extracts a stable key from one file payload.
  // 从单个文件载荷中提取稳定 key。
  fileKey: (file: RowFile) => string;
  // Resolves the tooltip title for a file node.
  // 解析文件节点的提示标题。
  fileTitle?: (file: RowFile, node: WorkspaceFileTreeNode<RowFile>) => string;
  // Renders the compact file icon content inside the shared icon slot.
  // 在共享图标位中渲染紧凑文件图标内容。
  renderFileIcon?: (node: WorkspaceFileTreeNode<RowFile>) => React.ReactNode;
  // Resolves the class appended to the shared file icon slot.
  // 解析追加到共享文件图标位的类名。
  fileIconClassName?: (node: WorkspaceFileTreeNode<RowFile>) => string;
  // Renders trailing content at the end of a file row.
  // 渲染文件行末尾的附加内容。
  renderFileTrailing?: (node: WorkspaceFileTreeNode<RowFile>) => React.ReactNode;
}) {
  // isDirectory stores whether the current node can contain child nodes.
  // isDirectory 保存当前节点是否可以包含子节点。
  const isDirectory = !props.node.file;
  // isOpen stores whether the current directory is expanded.
  // isOpen 保存当前目录是否展开。
  const isOpen = isDirectory && props.openDirectoryPaths.has(props.node.path);
  // isActive stores whether the current file payload is selected.
  // isActive 保存当前文件载荷是否被选中。
  const isActive = props.node.file ? props.fileKey(props.node.file) === props.activeFileKey : false;
  // rowStyle stores the CSS variable consumed by the shared diff tree styles.
  // rowStyle 保存共享 diff 目录树样式使用的 CSS 变量。
  const rowStyle = { "--diff-tree-depth": props.depth } as React.CSSProperties;
  if (isDirectory) {
    // isLoading stores whether this expanded directory is waiting on a lazy child request.
    // isLoading 保存当前展开目录是否正在等待懒加载子项请求。
    const isLoading = Boolean(props.loadingDirectoryPaths?.has(props.node.path));
    return (
      <div className="diff-file-tree-node directory">
        <button
          className="diff-file-tree-row directory"
          onClick={() => props.onToggleDirectory(props.node.path)}
          style={rowStyle}
          type="button"
        >
          {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          <Folder size={14} />
          <span className="diff-file-tree-name">{props.node.name}</span>
        </button>
        {isOpen ? props.node.children.map((child) => (
          <WorkspaceFileTreeNodeView
            activeFileKey={props.activeFileKey}
            depth={props.depth + 1}
            emptyDirectoryLabel={props.emptyDirectoryLabel}
            fileIconClassName={props.fileIconClassName}
            fileKey={props.fileKey}
            fileTitle={props.fileTitle}
            key={child.path}
            loadingDirectoryLabel={props.loadingDirectoryLabel}
            loadingDirectoryPaths={props.loadingDirectoryPaths}
            node={child}
            onSelectFile={props.onSelectFile}
            onToggleDirectory={props.onToggleDirectory}
            openDirectoryPaths={props.openDirectoryPaths}
            renderFileIcon={props.renderFileIcon}
            renderFileTrailing={props.renderFileTrailing}
          />
        )) : null}
        {isOpen && isLoading ? (
          <div className="diff-file-tree-empty">{props.loadingDirectoryLabel ?? "正在加载。"}</div>
        ) : null}
        {isOpen && !isLoading && props.node.children.length === 0 && props.emptyDirectoryLabel ? (
          <div className="diff-file-tree-empty">{props.emptyDirectoryLabel}</div>
        ) : null}
      </div>
    );
  }
  // fileTitle stores the native tooltip text for the current file row.
  // fileTitle 保存当前文件行的原生提示文本。
  const fileTitle = props.node.file ? props.fileTitle?.(props.node.file, props.node) ?? props.node.name : props.node.name;
  return (
    <div
      className={`diff-file-tree-row file${isActive ? " active" : ""}`}
      style={rowStyle}
      title={fileTitle}
    >
      <button
        className="diff-file-tree-file-main"
        onClick={() => props.node.file ? props.onSelectFile(props.node.file) : undefined}
        type="button"
      >
        <span className={`diff-file-tree-file-icon ${props.fileIconClassName?.(props.node) ?? ""}`}>
          {props.renderFileIcon?.(props.node) ?? workspaceFileTreeExtensionLabel(props.node.name)}
        </span>
        <span className="diff-file-tree-name">{props.node.name}</span>
      </button>
      {props.renderFileTrailing?.(props.node)}
    </div>
  );
}

// WorkspaceFileTreeStats renders compact added/deleted counters.
// WorkspaceFileTreeStats 渲染紧凑的新增/删除计数。
export function WorkspaceFileTreeStats(props: {
  // Aggregated added-line count.
  // 聚合新增行数。
  additions: number;
  // Aggregated deleted-line count.
  // 聚合删除行数。
  deletions: number;
}) {
  return (
    <span className="diff-file-tree-stats">
      {props.additions ? <span className="added">{`+${props.additions}`}</span> : null}
      {props.deletions ? <span className="removed">{`-${props.deletions}`}</span> : null}
    </span>
  );
}

// defaultWorkspaceFileTreeOpenDirectoryPaths returns the initial expanded directory set.
// defaultWorkspaceFileTreeOpenDirectoryPaths 返回目录树初始展开目录集合。
export function defaultWorkspaceFileTreeOpenDirectoryPaths<RowFile>(
  // Tree root to inspect.
  // 需要检查的目录树根节点。
  root: WorkspaceFileTreeNode<RowFile>,
): ReadonlySet<string> {
  const openPaths = new Set<string>();
  collectWorkspaceFileTreeOpenDirectoryPaths(root, openPaths);
  return openPaths;
}

// workspaceFileTreeExtensionLabel returns a compact file-type label for the tree icon slot.
// workspaceFileTreeExtensionLabel 返回目录树图标位使用的紧凑文件类型标签。
export function workspaceFileTreeExtensionLabel(
  // File name displayed by the tree row.
  // 目录树行展示的文件名。
  name: string,
): string {
  const extension = name.includes(".") ? name.split(".").pop()?.trim() : "";
  return extension ? extension.slice(0, 3).toUpperCase() : "•";
}

// collectWorkspaceFileTreeOpenDirectoryPaths expands all visible directories for fast scanning.
// collectWorkspaceFileTreeOpenDirectoryPaths 展开所有可见目录，方便快速浏览。
function collectWorkspaceFileTreeOpenDirectoryPaths<RowFile>(
  // Current tree node.
  // 当前目录树节点。
  node: WorkspaceFileTreeNode<RowFile>,
  // Mutable set receiving directory paths.
  // 接收目录路径的可变集合。
  openPaths: Set<string>,
): void {
  for (const child of node.children) {
    if (child.file) {
      continue;
    }
    openPaths.add(child.path);
    collectWorkspaceFileTreeOpenDirectoryPaths(child, openPaths);
  }
}

// ChevronLeftIcon provides a small local icon for the collapsed drawer handle.
// ChevronLeftIcon 为收起目录把手提供一个小型本地图标。
function ChevronLeftIcon() {
  return (
    <svg aria-hidden="true" fill="none" height="14" viewBox="0 0 14 14" width="14">
      <path d="M8.75 3.5 5.25 7l3.5 3.5" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.5" />
    </svg>
  );
}

// WorkspaceDefaultFileIcon renders a warning icon when callers mark the node as too large.
// WorkspaceDefaultFileIcon 在调用方标记节点过大时渲染警告图标。
export function WorkspaceDefaultFileIcon(props: {
  // Whether the file should show a warning icon.
  // 文件是否应该展示警告图标。
  warning: boolean;
  // File name used for the fallback extension label.
  // 用于回退扩展名标签的文件名。
  name: string;
}) {
  return props.warning ? <FileWarning size={14} /> : workspaceFileTreeExtensionLabel(props.name);
}
