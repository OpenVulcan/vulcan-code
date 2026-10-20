import React from "react";
import { ChevronDown, ChevronRight, Folder, FolderOpen, ListCollapse, ListTree, Search } from "lucide-react";
import { FileTypeIcon, resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { IconButton } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { useI18n } from "@/i18n";

// WorkspaceFileTreeNode stores one directory or file row of the tree.
// WorkspaceFileTreeNode 保存目录树中的一个目录或文件行。
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
}

// WorkspaceFileTree is the project's folders and files with a filter on top: folders open in place and a file
// row opens that file. It fills whatever holds it, so the files tab shows it both as the landing view and inside
// the file bar's picker.
// WorkspaceFileTree 是顶部带筛选的项目目录与文件：目录原地展开，文件行打开该文件。它铺满容器，因此文件标签页既将其作为
// 落地视图，也放在文件栏的选择器中。
export function WorkspaceFileTree<RowFile>(props: {
  // Tree root whose children are listed.
  // 列出其子项的目录树根节点。
  tree: WorkspaceFileTreeNode<RowFile>;
  // Key of the open file.
  // 已打开文件的 key。
  activeFileKey: string | null;
  // Filter text.
  // 筛选文本。
  filter: string;
  // Placeholder displayed inside the filter input.
  // 筛选输入框内展示的占位文本。
  filterPlaceholder?: string;
  // Empty state shown when the tree has no visible nodes.
  // 目录树没有可见节点时展示的空状态。
  emptyLabel?: string;
  // Controlled set of open directories; the tree keeps its own when left out.
  // 受控的已展开目录集合；省略时目录树自行保存。
  openDirectoryPaths?: ReadonlySet<string>;
  // Directories open at first when the tree keeps its own set; every directory when left out.
  // 目录树自行保存集合时初始展开的目录；省略时展开全部目录。
  defaultOpenDirectoryPaths?: ReadonlySet<string>;
  // Directories that are currently loading children.
  // 当前正在加载子项的目录集合。
  loadingDirectoryPaths?: ReadonlySet<string>;
  // Label shown inside an expanded loading directory.
  // 展开且正在加载的目录内展示的文本。
  loadingDirectoryLabel?: string;
  // Controls added after expand-all and collapse-all.
  // 追加在全部展开与全部折叠之后的控件。
  extraFilterActions?: React.ReactNode;
  // Line under the list that explains what the tree leaves out.
  // 列表下方说明目录树省略了哪些内容的一行。
  footer?: React.ReactNode;
  // Moves focus into the filter when the tree appears.
  // 目录树出现时将焦点移入筛选框。
  autoFocusFilter?: boolean;
  // Called when the filter text changes.
  // 筛选文本变化时调用。
  onFilterChange: (filter: string) => void;
  // Called when the controlled directory set should change.
  // 受控目录集合需要变化时调用。
  onOpenDirectoryPathsChange?: (paths: ReadonlySet<string>) => void;
  // Called when one directory opens or closes.
  // 单个目录展开或折叠时调用。
  onDirectoryOpenStateChange?: (path: string, open: boolean) => void;
  // Extracts a stable key from one file payload.
  // 从单个文件载荷中提取稳定 key。
  fileKey: (file: RowFile) => string;
  // Resolves the tooltip title for a file row.
  // 解析文件行的提示标题。
  fileTitle?: (file: RowFile, node: WorkspaceFileTreeNode<RowFile>) => string;
  // Called when a file row is chosen.
  // 选择文件行时调用。
  onSelectFile: (file: RowFile) => void;
}) {
  const { t } = useI18n();
  // everyDirectory is what expand-all opens and the uncontrolled tree starts with.
  // everyDirectory 是全部展开所打开的目录，也是非受控目录树的初始状态。
  const everyDirectory = React.useMemo(() => defaultWorkspaceFileTreeOpenDirectoryPaths(props.tree), [props.tree]);
  const initialOpen = props.defaultOpenDirectoryPaths ?? everyDirectory;
  const isControlled = props.openDirectoryPaths !== undefined;
  const [ownOpen, setOwnOpen] = React.useState<ReadonlySet<string>>(initialOpen);
  const openDirectoryPaths = props.openDirectoryPaths ?? ownOpen;
  const listRef = React.useRef<HTMLDivElement | null>(null);
  React.useEffect(() => {
    if (!isControlled) setOwnOpen(initialOpen);
  }, [initialOpen, isControlled]);
  const setOpen = (next: ReadonlySet<string>) => {
    props.onOpenDirectoryPathsChange?.(next);
    if (!isControlled) setOwnOpen(next);
  };
  const toggleDirectory = (path: string) => {
    const next = new Set(openDirectoryPaths);
    const opening = !next.delete(path);
    if (opening) next.add(path);
    setOpen(next);
    props.onDirectoryOpenStateChange?.(path, opening);
  };
  const expandAll = () => {
    setOpen(everyDirectory);
    for (const path of everyDirectory) props.onDirectoryOpenStateChange?.(path, true);
  };
  const collapseAll = () => {
    for (const path of openDirectoryPaths) props.onDirectoryOpenStateChange?.(path, false);
    setOpen(new Set<string>());
  };
  // moveFocus walks the visible rows with the arrow keys, starting from the filter field.
  // moveFocus 用方向键在可见行间移动，从筛选框开始。
  const moveFocus = (event: React.KeyboardEvent, from: Element | null) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const rows = Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>(".vk-ftree-row") ?? []);
    if (!rows.length) return;
    event.preventDefault();
    const at = from ? rows.indexOf(from as HTMLButtonElement) : -1;
    const next = event.key === "ArrowDown" ? Math.min(rows.length - 1, at + 1) : Math.max(0, at - 1);
    rows[at < 0 && event.key === "ArrowUp" ? rows.length - 1 : next]?.focus();
  };
  return (
    <div className="vk-ftree">
      <div className="vk-filemenu-head">
        <div className="vk-input-wrap">
          <Search aria-hidden="true" size={13} />
          <input
            aria-label={t("workspaceFiles.filterFiles")}
            autoComplete="off"
            autoFocus={props.autoFocusFilter}
            className="vk-input"
            data-size="sm"
            onChange={(event) => props.onFilterChange(event.currentTarget.value)}
            onKeyDown={(event) => moveFocus(event, null)}
            placeholder={props.filterPlaceholder ?? t("workspaceFiles.filterPlaceholder")}
            value={props.filter}
          />
        </div>
        <IconButton aria-label={t("workspaceFileTree.expandAll")} disabled={everyDirectory.size === 0} onClick={expandAll} size="sm" title={t("workspaceFileTree.expandAll")}>
          <ListTree size={14} />
        </IconButton>
        <IconButton aria-label={t("workspaceFileTree.collapseAll")} disabled={everyDirectory.size === 0} onClick={collapseAll} size="sm" title={t("workspaceFileTree.collapseAll")}>
          <ListCollapse size={14} />
        </IconButton>
        {props.extraFilterActions}
      </div>
      <div className="vk-ftree-list" onKeyDown={(event) => moveFocus(event, document.activeElement)} ref={listRef}>
        {props.tree.children.length ? props.tree.children.map((node) => (
          <WorkspaceFileTreeRow
            activeFileKey={props.activeFileKey}
            depth={0}
            fileKey={props.fileKey}
            fileTitle={props.fileTitle}
            key={node.path}
            loadingDirectoryLabel={props.loadingDirectoryLabel}
            loadingDirectoryPaths={props.loadingDirectoryPaths}
            node={node}
            onSelectFile={props.onSelectFile}
            onToggleDirectory={toggleDirectory}
            openDirectoryPaths={openDirectoryPaths}
          />
        )) : <p className="vk-filemenu-none">{props.emptyLabel ?? t("workspaceFiles.noMatchedFiles")}</p>}
      </div>
      {props.footer ? <div className="vk-filemenu-foot">{props.footer}</div> : null}
    </div>
  );
}

// WorkspaceFileTreeRow renders one folder with its open children, or one file.
// WorkspaceFileTreeRow 渲染一个目录及其展开的子项，或一个文件。
function WorkspaceFileTreeRow<RowFile>(props: {
  node: WorkspaceFileTreeNode<RowFile>;
  depth: number;
  activeFileKey: string | null;
  openDirectoryPaths: ReadonlySet<string>;
  loadingDirectoryPaths?: ReadonlySet<string>;
  loadingDirectoryLabel?: string;
  onToggleDirectory: (path: string) => void;
  onSelectFile: (file: RowFile) => void;
  fileKey: (file: RowFile) => string;
  fileTitle?: (file: RowFile, node: WorkspaceFileTreeNode<RowFile>) => string;
}) {
  const { t } = useI18n();
  const { node } = props;
  const depthStyle = { "--vk-depth": props.depth } as React.CSSProperties;
  if (!node.file) {
    const isOpen = props.openDirectoryPaths.has(node.path);
    const isLoading = Boolean(props.loadingDirectoryPaths?.has(node.path));
    return (
      <>
        <button
          aria-busy={isLoading}
          aria-expanded={isOpen}
          className="vk-ftree-row"
          data-kind="dir"
          onClick={() => props.onToggleDirectory(node.path)}
          style={depthStyle}
          title={node.name}
          type="button"
        >
          {isOpen ? <ChevronDown aria-hidden="true" size={13} /> : <ChevronRight aria-hidden="true" size={13} />}
          {isOpen ? <FolderOpen aria-hidden="true" size={14} /> : <Folder aria-hidden="true" size={14} />}
          <span>{node.name}</span>
        </button>
        {isOpen ? node.children.map((child) => <WorkspaceFileTreeRow {...props} depth={props.depth + 1} key={child.path} node={child} />) : null}
        {isOpen && isLoading ? (
          <StreamPlaceholder className="vk-ftree-loading" count={2} label={props.loadingDirectoryLabel ?? t("workspaceFiles.loading")} />
        ) : null}
      </>
    );
  }
  const file = node.file;
  const isActive = props.fileKey(file) === props.activeFileKey;
  return (
    <button
      aria-current={isActive ? "true" : undefined}
      className="vk-ftree-row"
      data-kind="file"
      onClick={() => props.onSelectFile(file)}
      style={depthStyle}
      title={props.fileTitle?.(file, node) ?? node.name}
      type="button"
    >
      <FileTypeIcon icon={resolveFileTypeIcon(node.name)} />
      <span>{node.name}</span>
    </button>
  );
}

// defaultWorkspaceFileTreeOpenDirectoryPaths returns every directory of the tree.
// defaultWorkspaceFileTreeOpenDirectoryPaths 返回目录树中的全部目录。
export function defaultWorkspaceFileTreeOpenDirectoryPaths<RowFile>(root: WorkspaceFileTreeNode<RowFile>): ReadonlySet<string> {
  const openPaths = new Set<string>();
  const collect = (node: WorkspaceFileTreeNode<RowFile>) => {
    for (const child of node.children) {
      if (child.file) continue;
      openPaths.add(child.path);
      collect(child);
    }
  };
  collect(root);
  return openPaths;
}
