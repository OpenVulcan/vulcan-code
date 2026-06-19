import React from "react";
import {
  Copy,
  FileText,
  Folder,
  MoreHorizontal,
} from "lucide-react";
import { toast } from "sonner";
import {
  activeWorkbenchSurfaceId,
  selectedConversationProjectId,
  selectedConversationWorkspacePath,
  type WorkspaceReviewDraftConversation,
} from "@/diff/workspace-review-model";
import {
  diffLanguageForFile,
  highlightedDiffLineSpansForDiffLine,
  useHighlightedDiffFileRevision,
  useHighlightLanguage,
} from "@/diff/diff-highlight";
import { escapeHtml, tokenSpansToHtml } from "@/diff/diff-shiki-shared";
import type { DiffFile, DiffLine } from "@/diff/types";
import { Button } from "@/components/ui/button";
import {
  WorkspaceFileTreeDrawer,
  workspaceFileTreeExtensionLabel,
  type WorkspaceFileTreeNode,
} from "@/components/workspace/WorkspaceFileTreeDrawer";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { DEFAULT_WORKSPACE_FILE_FILTERS } from "@/service/workbench-snapshot-service";
import type {
  WorkbenchSnapshot,
  WorkspaceFileDirectoryListing,
  WorkspaceFileEntrySummary,
  WorkspaceFileFilterOptions,
  WorkspaceFileReadResponse,
} from "@/types";
import "@/diff/diffReview.css";

// workspaceFilePreviewMaxHighlightRows caps Shiki preview work inside the right rail.
// workspaceFilePreviewMaxHighlightRows 限制右侧栏内 Shiki 预览工作量。
const workspaceFilePreviewMaxHighlightRows = 3_000;

// workspaceFilesPanelCacheLimit caps retained file panel states across rail collapse cycles.
// workspaceFilesPanelCacheLimit 限制右侧栏折叠周期中保留的文件面板状态数量。
const workspaceFilesPanelCacheLimit = 12;

// workspaceFilesPanelCache preserves file panel state while the right rail is unmounted.
// workspaceFilesPanelCache 在右侧栏卸载期间保留文件面板状态。
const workspaceFilesPanelCache = new Map<string, WorkspaceFilesPanelCacheEntry>();

// workspaceFileFilterStorageKey stores the global persisted file filter preference key.
// workspaceFileFilterStorageKey 保存全局持久化文件过滤偏好的键名。
const workspaceFileFilterStorageKey = "vulcan.workspace.files.filters.v1";

// WorkspaceFilesPanelCacheEntry stores one right file panel state snapshot.
// WorkspaceFilesPanelCacheEntry 保存单个右侧文件面板状态快照。
interface WorkspaceFilesPanelCacheEntry {
  // Expanded directory absolute paths.
  // 已展开目录的绝对路径。
  expandedPaths: string[];
  // Selected file absolute path.
  // 已选中文件的绝对路径。
  selectedPath: string | null;
  // Selected file entry snapshot used when its parent directory is not loaded yet.
  // 已选中文件条目快照，用于其父目录尚未加载时恢复预览。
  selectedEntry: WorkspaceFileEntrySummary | null;
  // Active filter text.
  // 当前过滤文本。
  filterText: string;
  // Whether the directory drawer was pinned open.
  // 目录抽屉是否被固定展开。
  drawerPinnedOpen: boolean;
  // Cached directory listings keyed by absolute directory path.
  // 按绝对目录路径缓存的目录列表。
  directories: [string, WorkspaceFileDirectoryListing][];
}

// WorkspaceFilesPanelHost owns workspace file tree and preview state for the right context panel.
// WorkspaceFilesPanelHost 持有右侧上下文面板的工作区文件树与预览状态。
export function WorkspaceFilesPanelHost(props: {
  // Unique dynamic context tab instance id.
  // 唯一动态上下文页签实例标识。
  instanceId: string;
  // Current draft conversation selection when the selected route is not persisted yet.
  // 当前路由尚未持久化时对应的草稿会话选择。
  selectedDraftConversation: WorkspaceReviewDraftConversation | null;
  // Current selected conversation id from the local workbench route.
  // 来自本地工作台路由的当前选中会话标识。
  selectedConversationId: string | null;
  // Workbench snapshot carrying project and conversation ownership.
  // 携带项目与会话归属关系的工作台快照。
  workbench: WorkbenchSnapshot;
  // Loads one lazy workspace file directory.
  // 加载单个懒加载工作区文件目录。
  loadWorkspaceFileDirectory: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    directoryPath: string | null,
    filters: WorkspaceFileFilterOptions,
  ) => Promise<WorkspaceFileDirectoryListing>;
  // Loads one workspace file preview response.
  // 加载单个工作区文件预览响应。
  loadWorkspaceFilePreview: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    filePath: string,
    filters: WorkspaceFileFilterOptions,
  ) => Promise<WorkspaceFileReadResponse>;
  // Resolves one local media path into a WebUI-renderable URL.
  // 将单个本地媒体路径解析为 WebUI 可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Copies text using the host-aware clipboard implementation.
  // 使用宿主感知的剪贴板实现复制文本。
  onCopyText: (text: string) => void;
  // Updates the parent dynamic tab title.
  // 更新父级动态页签标题。
  onTitleChange: (title: string) => void;
}) {
  // activeSurfaceId stores the conversation or subagent surface represented by this panel.
  // activeSurfaceId 保存当前面板代表的会话或子代理界面标识。
  const activeSurfaceId = activeWorkbenchSurfaceId(props.workbench, props.selectedConversationId);
  // workspaceProjectId stores the backend project id used for file RPC symmetry.
  // workspaceProjectId 保存用于文件 RPC 协议对称的后端项目标识。
  const workspaceProjectId = React.useMemo(
    () => selectedConversationProjectId(props.workbench, props.selectedConversationId, props.selectedDraftConversation),
    [props.selectedConversationId, props.selectedDraftConversation, props.workbench],
  );
  // workspacePath stores the selected conversation owner project path.
  // workspacePath 保存当前选中会话所属项目路径。
  const workspacePath = React.useMemo(
    () => selectedConversationWorkspacePath(props.workbench, props.selectedConversationId, props.selectedDraftConversation),
    [props.selectedConversationId, props.selectedDraftConversation, props.workbench],
  );
  // cacheKey stores the cache bucket for this visible conversation surface.
  // cacheKey 保存当前可见会话界面的缓存桶。
  const cacheKey = React.useMemo(
    () => workspaceFilesPanelCacheKey(activeSurfaceId, workspacePath, props.instanceId),
    [activeSurfaceId, props.instanceId, workspacePath],
  );
  // initialCache restores state from a previously collapsed panel.
  // initialCache 从先前折叠的面板恢复状态。
  const initialCache = React.useMemo(() => workspaceFilesPanelCache.get(cacheKey) ?? null, [cacheKey]);
  // filters stores the active backend file tree filtering options.
  // filters 保存当前后端文件树过滤选项。
  const [filters, setFilters] = React.useState<WorkspaceFileFilterOptions>(() => (
    readPersistedWorkspaceFileFilters()
  ));
  // listingFilters stores tree filters with media always visible in the directory tree.
  // listingFilters 保存目录树过滤器，并始终让媒体文件在目录树中可见。
  const listingFilters = React.useMemo(
    () => workspaceFileListingFilters(filters),
    [filters.ignoreDotNames, filters.ignoreHidden, filters.useGitignore, filters.useIgnoreFiles],
  );
  // filterText stores the local file tree filter query.
  // filterText 保存本地文件树过滤查询。
  const [filterText, setFilterText] = React.useState(() => initialCache?.filterText ?? "");
  // fileTreeDrawerPinnedOpen stores the shared drawer pin state for the file tab.
  // fileTreeDrawerPinnedOpen 保存文件标签页共享抽屉的固定展开状态。
  const [fileTreeDrawerPinnedOpen, setFileTreeDrawerPinnedOpen] = React.useState(() => (
    initialCache?.drawerPinnedOpen ?? !initialCache?.selectedPath
  ));
  // expandedPaths stores absolute directory paths expanded in the tree.
  // expandedPaths 保存文件树中已展开的绝对目录路径。
  const [expandedPaths, setExpandedPaths] = React.useState<Set<string>>(
    () => new Set(initialCache?.expandedPaths ?? []),
  );
  // directories stores loaded directory listings keyed by absolute path.
  // directories 按绝对路径保存已加载目录列表。
  const [directories, setDirectories] = React.useState<Map<string, WorkspaceFileDirectoryListing>>(
    () => new Map(initialCache?.directories ?? []),
  );
  // loadingDirectories stores directories with an in-flight listing request.
  // loadingDirectories 保存正在请求列表的目录。
  const [loadingDirectories, setLoadingDirectories] = React.useState<Set<string>>(() => new Set());
  // selectedPath stores the current selected file path.
  // selectedPath 保存当前选中文件路径。
  const [selectedPath, setSelectedPath] = React.useState<string | null>(() => initialCache?.selectedPath ?? null);
  // selectedEntrySnapshot stores the latest selected file row outside the loaded tree.
  // selectedEntrySnapshot 在已加载目录树之外保存最近选中的文件行。
  const [selectedEntrySnapshot, setSelectedEntrySnapshot] = React.useState<WorkspaceFileEntrySummary | null>(
    () => initialCache?.selectedEntry ?? null,
  );
  // preview stores the latest loaded file preview response.
  // preview 保存最近加载的文件预览响应。
  const [preview, setPreview] = React.useState<WorkspaceFileReadResponse | null>(null);
  // previewLoading stores whether a file preview request is running.
  // previewLoading 保存文件预览请求是否正在运行。
  const [previewLoading, setPreviewLoading] = React.useState(false);
  // previewError stores the latest file preview error.
  // previewError 保存最近一次文件预览错误。
  const [previewError, setPreviewError] = React.useState<string | null>(null);
  // mediaPermissionEntry stores the selected media file waiting for display consent.
  // mediaPermissionEntry 保存正在等待展示许可的已选中媒体文件。
  const [mediaPermissionEntry, setMediaPermissionEntry] = React.useState<WorkspaceFileEntrySummary | null>(null);
  // panelError stores the latest directory loading error.
  // panelError 保存最近一次目录加载错误。
  const [panelError, setPanelError] = React.useState<string | null>(null);
  // listingFiltersKey stores a stable string used to refresh the root after tree filter changes.
  // listingFiltersKey 保存目录树过滤变化后刷新根目录使用的稳定字符串。
  const listingFiltersKey = React.useMemo(() => JSON.stringify(listingFilters), [listingFilters]);
  // loadedInitialDirectoryRef separates first directory hydration from user filter changes.
  // loadedInitialDirectoryRef 区分首次目录水合与用户过滤规则变化。
  const loadedInitialDirectoryRef = React.useRef(false);
  // skipNextDirectoryResetRef prevents cache hydration from looking like a user filter change.
  // skipNextDirectoryResetRef 防止缓存水合被误判为用户过滤规则变化。
  const skipNextDirectoryResetRef = React.useRef(false);

  // loadDirectory fetches one directory and merges it into the directory cache.
  // loadDirectory 获取单个目录并合并到目录缓存。
  const loadDirectory = React.useCallback(async (directoryPath: string | null) => {
    if (!workspacePath) {
      return;
    }
    const loadingKey = directoryPath ?? workspacePath;
    setLoadingDirectories((current) => new Set(current).add(loadingKey));
    setPanelError(null);
    try {
      const listing = await props.loadWorkspaceFileDirectory(
        workspacePath,
        workspaceProjectId,
        props.selectedConversationId,
        directoryPath,
        listingFilters,
      );
      setDirectories((current) => {
        const next = new Map(current);
        next.set(listing.path, listing);
        return next;
      });
      if (listing.notice) {
        toast.info(listing.notice);
      }
    } catch (error) {
      setPanelError(error instanceof Error ? error.message : String(error));
    } finally {
      setLoadingDirectories((current) => {
        const next = new Set(current);
        next.delete(loadingKey);
        return next;
      });
    }
  }, [listingFilters, props.loadWorkspaceFileDirectory, props.selectedConversationId, workspacePath, workspaceProjectId]);

  React.useEffect(() => {
    const nextCache = workspaceFilesPanelCache.get(cacheKey) ?? null;
    loadedInitialDirectoryRef.current = false;
    skipNextDirectoryResetRef.current = true;
    setFilters(readPersistedWorkspaceFileFilters());
    setFilterText(nextCache?.filterText ?? "");
    setExpandedPaths(new Set(nextCache?.expandedPaths ?? []));
    setDirectories(new Map(nextCache?.directories ?? []));
    setSelectedPath(nextCache?.selectedPath ?? null);
    setSelectedEntrySnapshot(nextCache?.selectedEntry ?? null);
    setFileTreeDrawerPinnedOpen(nextCache?.drawerPinnedOpen ?? !nextCache?.selectedPath);
    setPreview(null);
    setPreviewError(null);
    setMediaPermissionEntry(null);
    setPanelError(null);
  }, [cacheKey]);

  React.useEffect(() => {
    if (!loadedInitialDirectoryRef.current || skipNextDirectoryResetRef.current) {
      loadedInitialDirectoryRef.current = true;
      skipNextDirectoryResetRef.current = false;
      void loadDirectory(null);
      return;
    }
    setDirectories(new Map());
    setPreview(null);
    setPreviewError(null);
    setMediaPermissionEntry(null);
    setPanelError(null);
    setSelectedPath(null);
    setSelectedEntrySnapshot(null);
    props.onTitleChange("打开文件");
    void loadDirectory(null);
  }, [listingFiltersKey, loadDirectory, props.onTitleChange, workspacePath]);

  React.useEffect(() => {
    const cacheEntry: WorkspaceFilesPanelCacheEntry = {
      directories: Array.from(directories.entries()),
      drawerPinnedOpen: fileTreeDrawerPinnedOpen,
      expandedPaths: Array.from(expandedPaths),
      filterText,
      selectedEntry: selectedEntrySnapshot,
      selectedPath,
    };
    workspaceFilesPanelCache.set(cacheKey, cacheEntry);
    trimWorkspaceFilesPanelCache();
  }, [cacheKey, directories, expandedPaths, fileTreeDrawerPinnedOpen, filterText, selectedEntrySnapshot, selectedPath]);

  // rootListing stores the currently loaded root directory listing.
  // rootListing 保存当前已加载的根目录列表。
  const rootListing = workspacePath ? rootWorkspaceFileListing(directories, workspacePath) : null;
  // selectedEntry stores the selected file row from the loaded tree.
  // selectedEntry 从已加载树中保存当前选中文件行。
  const selectedEntry = React.useMemo(
    () => selectedPath ? findWorkspaceFileEntry(directories, selectedPath) ?? selectedEntrySnapshot : null,
    [directories, selectedEntrySnapshot, selectedPath],
  );
  // fileTree stores the project listing in the shared review-tree node format.
  // fileTree 以共享审核目录树节点格式保存项目文件列表。
  const fileTree = React.useMemo(
    () => rootListing ? buildWorkspaceFileTree(rootListing.entries, directories, filterText) : emptyWorkspaceFileTree(),
    [directories, filterText, rootListing],
  );
  // handleOpenDirectoryPathsChange mirrors the shared drawer expansion state into panel cache state.
  // handleOpenDirectoryPathsChange 将共享抽屉展开状态同步到面板缓存状态。
  const handleOpenDirectoryPathsChange = React.useCallback((paths: ReadonlySet<string>) => {
    setExpandedPaths(new Set(paths));
  }, []);
  // handleDirectoryOpenStateChange lazily loads children when the shared drawer opens a directory.
  // handleDirectoryOpenStateChange 在共享抽屉展开目录时懒加载子项。
  const handleDirectoryOpenStateChange = React.useCallback((path: string, open: boolean) => {
    if (open && !directories.has(path)) {
      void loadDirectory(path);
    }
  }, [directories, loadDirectory]);

  // loadFilePreview loads the preview payload for one already selected file.
  // loadFilePreview 为一个已选中文件加载预览载荷。
  const loadFilePreview = React.useCallback(async (
    // File entry to preview.
    // 需要预览的文件条目。
    entry: WorkspaceFileEntrySummary,
    // Filters used by the backend preview permission check.
    // 后端预览权限检查使用的过滤器。
    previewFilters: WorkspaceFileFilterOptions,
  ) => {
    if (!workspacePath) {
      return;
    }
    setPreview(null);
    setPreviewError(null);
    setMediaPermissionEntry(null);
    setPreviewLoading(true);
    try {
      const response = await props.loadWorkspaceFilePreview(
        workspacePath,
        workspaceProjectId,
        props.selectedConversationId,
        entry.path,
        previewFilters,
      );
      setPreview(response);
    } catch (error) {
      setPreviewError(workspaceFilePreviewErrorMessage(error, entry));
    } finally {
      setPreviewLoading(false);
    }
  }, [props.loadWorkspaceFilePreview, props.selectedConversationId, workspacePath, workspaceProjectId]);

  React.useEffect(() => {
    // Restored file tabs must reload their previous preview even before the tree is reopened.
    // 恢复的文件页签必须在目录树重新展开前重新加载上次预览。
    if (!selectedEntry || selectedEntry.isDirectory || preview || previewLoading || previewError || mediaPermissionEntry) {
      return;
    }
    if (workspaceFileEntryNeedsMediaPermission(selectedEntry, filters)) {
      setMediaPermissionEntry(selectedEntry);
      return;
    }
    void loadFilePreview(selectedEntry, filters);
  }, [filters, loadFilePreview, mediaPermissionEntry, preview, previewError, previewLoading, selectedEntry]);

  // selectFile loads a preview for one file row.
  // selectFile 为单个文件行加载预览。
  const selectFile = React.useCallback(async (entry: WorkspaceFileEntrySummary) => {
    if (!workspacePath || entry.isDirectory) {
      return;
    }
    setSelectedPath(entry.path);
    setSelectedEntrySnapshot(entry);
    props.onTitleChange(entry.name || workspaceFileNameFromPath(entry.relativePath || entry.path));
    if (workspaceFileEntryNeedsMediaPermission(entry, filters)) {
      setPreview(null);
      setPreviewError(null);
      setPreviewLoading(false);
      setMediaPermissionEntry(entry);
      return;
    }
    await loadFilePreview(entry, filters);
  }, [filters, loadFilePreview, props.onTitleChange, workspacePath]);

  // allowMediaPreview grants one media kind and immediately opens the selected media file.
  // allowMediaPreview 授权单个媒体类型并立即打开已选中的媒体文件。
  const allowMediaPreview = React.useCallback((entry: WorkspaceFileEntrySummary) => {
    // nextFilters stores the persisted media display preference after consent.
    // nextFilters 保存授权后的持久化媒体展示偏好。
    const nextFilters = workspaceFileFiltersWithMediaPermission(filters, entry.kind);
    persistWorkspaceFileFilters(nextFilters);
    setFilters(nextFilters);
    void loadFilePreview(entry, nextFilters);
  }, [filters, loadFilePreview]);

  // copyFileContent loads and copies one code file's textual preview content.
  // copyFileContent 加载并复制单个代码文件的文本预览内容。
  const copyFileContent = React.useCallback(async (entry: WorkspaceFileEntrySummary) => {
    if (!workspacePath || entry.kind !== "code") {
      return;
    }
    try {
      const response = await props.loadWorkspaceFilePreview(
        workspacePath,
        workspaceProjectId,
        props.selectedConversationId,
        entry.path,
        filters,
      );
      props.onCopyText(response.content ?? "");
      toast.success("已复制文件内容。");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [filters, props.loadWorkspaceFilePreview, props.onCopyText, props.selectedConversationId, workspacePath, workspaceProjectId]);

  // updateFilter toggles one backend file tree filter field.
  // updateFilter 切换单个后端文件树过滤字段。
  const updateFilter = React.useCallback((key: keyof WorkspaceFileFilterOptions, value: boolean) => {
    setFilters((current) => {
      // next stores the updated global filter preference.
      // next 保存更新后的全局过滤偏好。
      const next = { ...current, [key]: value };
      persistWorkspaceFileFilters(next);
      return next;
    });
  }, []);
  // closePreviewAndShowTree clears the file preview and reopens the shared directory drawer.
  // closePreviewAndShowTree 清空文件预览并重新打开共享目录抽屉。
  const closePreviewAndShowTree = React.useCallback(() => {
    setSelectedPath(null);
    setSelectedEntrySnapshot(null);
    setPreview(null);
    setPreviewError(null);
    setMediaPermissionEntry(null);
    setPreviewLoading(false);
    setFileTreeDrawerPinnedOpen(true);
    props.onTitleChange("打开文件");
  }, [props.onTitleChange]);

  if (!workspacePath) {
    return (
      <section className="workspace-files-panel empty">
        <div className="context-empty-state">
          <Folder size={24} />
          <strong>文件</strong>
          <span>当前会话没有关联项目目录。</span>
        </div>
      </section>
    );
  }

  return (
    <section className="workspace-files-panel">
      <div className={`workspace-files-body${selectedEntry ? " has-selection" : " directory-only"}`}>
        <WorkspaceFileTreeDrawer
          activeFileKey={selectedPath}
          collapseOnSelect
          emptyLabel={panelError ?? (rootListing ? "没有匹配的文件。" : "暂无可显示文件。")}
          extraFilterActions={<WorkspaceFilesFilterMenu filters={filters} onFilterChange={updateFilter} />}
          fileIconClassName={(node) => workspaceFileTreeIconClassName(node.file)}
          fileKey={(entry) => entry.path}
          fileTitle={(entry) => entry.relativePath || entry.name}
          filter={filterText}
          filterPlaceholder="筛选文件..."
          loadingDirectoryLabel="正在加载。"
          loadingDirectoryPaths={loadingDirectories}
          lockedOpen={!selectedEntry}
          openDirectoryPaths={expandedPaths}
          pinnedOpen={fileTreeDrawerPinnedOpen}
          renderFileIcon={(node) => workspaceFileTreeIconLabel(node)}
          renderFileTrailing={(node) => node.file ? (
            <WorkspaceFileRowMenu
              entry={node.file}
              onCopyFileContent={copyFileContent}
              onCopyText={props.onCopyText}
            />
          ) : null}
          tree={fileTree}
          onDirectoryOpenStateChange={handleDirectoryOpenStateChange}
          onFilterChange={setFilterText}
          onOpenDirectoryPathsChange={handleOpenDirectoryPathsChange}
          onPinnedOpenChange={setFileTreeDrawerPinnedOpen}
          onSelectFile={selectFile}
        />
        {!rootListing && loadingDirectories.has(workspacePath) ? (
          <div className="workspace-files-panel-notice">正在加载文件树。</div>
        ) : null}
        {panelError ? <div className="workspace-files-panel-notice error">{panelError}</div> : null}
        <WorkspaceFilePreview
          entry={selectedEntry}
          error={previewError}
          loading={previewLoading}
          missingPath={selectedPath && !selectedEntry ? selectedPath : null}
          preview={preview}
          mediaPermissionEntry={mediaPermissionEntry}
          resolveMediaSourceUrl={props.resolveMediaSourceUrl}
          onAllowMediaPreview={allowMediaPreview}
          onClosePreview={closePreviewAndShowTree}
        />
      </div>
    </section>
  );
}

// WorkspaceFilesFilterMenu renders the global file tree filter dropdown.
// WorkspaceFilesFilterMenu 渲染全局文件树过滤下拉菜单。
function WorkspaceFilesFilterMenu(props: {
  // Active file tree filters.
  // 当前文件树过滤器。
  filters: WorkspaceFileFilterOptions;
  // Updates one filter value.
  // 更新单个过滤值。
  onFilterChange: (key: keyof WorkspaceFileFilterOptions, value: boolean) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button aria-label="文件过滤规则" size="icon-sm" type="button" variant="ghost">
          <MoreHorizontal size={16} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="workspace-files-filter-menu">
        <DropdownMenuLabel>隐藏规则</DropdownMenuLabel>
        <DropdownMenuCheckboxItem
          checked={props.filters.ignoreHidden}
          onCheckedChange={(value) => props.onFilterChange("ignoreHidden", Boolean(value))}
        >
          忽略隐藏文件
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={props.filters.ignoreDotNames}
          onCheckedChange={(value) => props.onFilterChange("ignoreDotNames", Boolean(value))}
        >
          忽略点开头名称
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={props.filters.useIgnoreFiles}
          onCheckedChange={(value) => props.onFilterChange("useIgnoreFiles", Boolean(value))}
        >
          使用 .ignore 规则
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={props.filters.useGitignore}
          onCheckedChange={(value) => props.onFilterChange("useGitignore", Boolean(value))}
        >
          使用 .gitignore 规则
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// WorkspaceFileRowMenu renders lightweight copy actions for one file tree row.
// WorkspaceFileRowMenu 为单个文件树行渲染轻量复制动作。
function WorkspaceFileRowMenu(props: {
  // File tree entry that owns the row menu.
  // 拥有行菜单的文件树条目。
  entry: WorkspaceFileEntrySummary;
  // Copies text using the host clipboard.
  // 使用宿主剪贴板复制文本。
  onCopyText: (text: string) => void;
  // Copies one code file's content through the backend preview path.
  // 通过后端预览路径复制单个代码文件内容。
  onCopyFileContent: (entry: WorkspaceFileEntrySummary) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button aria-label="文件操作" className="workspace-file-row-menu-trigger" size="icon-xs" type="button" variant="ghost">
          <MoreHorizontal size={13} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="workspace-file-row-menu">
        <DropdownMenuItem onClick={() => props.onCopyText(props.entry.path)}>
          <Copy size={14} />
          复制路径
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => props.onCopyText(props.entry.relativePath)}>
          <Copy size={14} />
          复制相对路径
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={props.entry.kind !== "code"}
          onClick={() => props.onCopyFileContent(props.entry)}
        >
          <FileText size={14} />
          复制文件内容
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// WorkspaceFilePreview renders the selected file preview pane.
// WorkspaceFilePreview 渲染当前选中文件预览区域。
function WorkspaceFilePreview(props: {
  // Selected file entry.
  // 当前选中文件条目。
  entry: WorkspaceFileEntrySummary | null;
  // Selected file path when its entry can no longer be resolved.
  // 无法再解析已选中文件条目时的选中文件路径。
  missingPath: string | null;
  // Loaded preview response.
  // 已加载预览响应。
  preview: WorkspaceFileReadResponse | null;
  // Selected media file waiting for display permission.
  // 等待展示许可的已选中媒体文件。
  mediaPermissionEntry: WorkspaceFileEntrySummary | null;
  // Whether a preview request is running.
  // 预览请求是否正在运行。
  loading: boolean;
  // Latest preview error.
  // 最近一次预览错误。
  error: string | null;
  // Resolves media file paths into renderable URLs.
  // 将媒体文件路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Grants media display permission and opens the selected media file.
  // 授权媒体展示并打开已选中的媒体文件。
  onAllowMediaPreview: (entry: WorkspaceFileEntrySummary) => void;
  // Closes the current preview and opens the directory drawer.
  // 关闭当前预览并打开目录抽屉。
  onClosePreview: () => void;
}) {
  if (!props.entry) {
    if (props.missingPath) {
      return (
        <WorkspaceMissingFilePreview
          missingPath={props.missingPath}
        />
      );
    }
    return (
      <div className="workspace-file-preview empty">
        <FileText size={24} />
        <strong>打开文件</strong>
        <span>从文件树中选择代码或媒体文件。</span>
      </div>
    );
  }
  if (props.loading) {
    return <div className="workspace-file-preview empty">正在加载预览。</div>;
  }
  if (props.error) {
    return <div className="workspace-file-preview empty error">{props.error}</div>;
  }
  if (props.mediaPermissionEntry && props.entry.path === props.mediaPermissionEntry.path) {
    return (
      <WorkspaceMediaPermissionPrompt
        entry={props.mediaPermissionEntry}
        onAllowMediaPreview={props.onAllowMediaPreview}
      />
    );
  }
  if (!props.preview) {
    return <div className="workspace-file-preview empty">暂无预览内容。</div>;
  }
  if (props.preview.kind === "image") {
    return (
      <div className="workspace-file-preview media">
        <WorkspaceFilePreviewHeader preview={props.preview} onClosePreview={props.onClosePreview} />
        <img alt={props.preview.relativePath} decoding="async" src={props.resolveMediaSourceUrl(props.preview.path)} />
      </div>
    );
  }
  if (props.preview.kind === "video") {
    return (
      <div className="workspace-file-preview media">
        <WorkspaceFilePreviewHeader preview={props.preview} onClosePreview={props.onClosePreview} />
        <video controls preload="metadata" src={props.resolveMediaSourceUrl(props.preview.path)} />
      </div>
    );
  }
  return (
    <div className="workspace-file-preview code">
      <WorkspaceFilePreviewHeader preview={props.preview} onClosePreview={props.onClosePreview} />
      <WorkspaceHighlightedCodePreview preview={props.preview} />
    </div>
  );
}

// WorkspaceMissingFilePreview renders a moved-or-deleted selected file notice.
// WorkspaceMissingFilePreview 渲染已选中文件被移动或删除的提示。
function WorkspaceMissingFilePreview(props: {
  // Previously selected file path that no longer resolves in the tree.
  // 先前选中但已无法在目录树中解析的文件路径。
  missingPath: string;
}) {
  return (
    <div className="workspace-file-preview empty error">
      <FileText size={24} />
      <strong>文件已被删除或移动</strong>
      <span>{props.missingPath}</span>
    </div>
  );
}

// WorkspaceMediaPermissionPrompt asks before enabling image or video previews.
// WorkspaceMediaPermissionPrompt 在启用图片或视频预览前请求确认。
function WorkspaceMediaPermissionPrompt(props: {
  // Selected media entry waiting for consent.
  // 等待授权的已选中媒体条目。
  entry: WorkspaceFileEntrySummary;
  // Grants media display permission and opens the selected media file.
  // 授权媒体展示并打开已选中的媒体文件。
  onAllowMediaPreview: (entry: WorkspaceFileEntrySummary) => void;
}) {
  // mediaLabel stores the localized media kind name.
  // mediaLabel 保存本地化媒体类型名称。
  const mediaLabel = props.entry.kind === "video" ? "视频" : "图片";
  return (
    <div className="workspace-file-preview empty media-permission">
      <FileText size={24} />
      <strong>{mediaLabel}文件不允许展示</strong>
      <span>是否允许展示当前{mediaLabel}文件？允许后同类文件会直接预览。</span>
      <Button size="sm" type="button" onClick={() => props.onAllowMediaPreview(props.entry)}>
        允许显示{mediaLabel}
      </Button>
    </div>
  );
}

// WorkspaceFilePreviewHeader renders the glass overlay title used to reopen the directory drawer.
// WorkspaceFilePreviewHeader 渲染用于重新打开目录抽屉的玻璃覆盖标题。
function WorkspaceFilePreviewHeader(props: {
  // Loaded preview response shown by the header.
  // 标题展示的已加载预览响应。
  preview: WorkspaceFileReadResponse;
  // Closes the current preview and opens the directory drawer.
  // 关闭当前预览并打开目录抽屉。
  onClosePreview: () => void;
}) {
  // iconLabel stores the same compact file-type label used by the shared tree row.
  // iconLabel 保存与共享目录树行一致的紧凑文件类型标签。
  const iconLabel = workspaceFilePreviewIconLabel(props.preview);
  // iconClassName stores the same file-kind palette used by the shared tree row.
  // iconClassName 保存与共享目录树行一致的文件类型色板。
  const iconClassName = workspaceFilePreviewIconClassName(props.preview);
  return (
    <button className="workspace-file-preview-header" onClick={props.onClosePreview} type="button">
      <span className={`diff-file-tree-file-icon workspace-file-preview-format-icon ${iconClassName}`}>
        {iconLabel}
      </span>
      <span className="workspace-file-preview-title-text">{props.preview.relativePath}</span>
      {props.preview.truncated ? <small>已截断</small> : null}
    </button>
  );
}

// WorkspaceHighlightedCodePreview renders a read-only code preview through the review Shiki pipeline.
// WorkspaceHighlightedCodePreview 通过审核 Shiki 管线渲染只读代码预览。
function WorkspaceHighlightedCodePreview(props: {
  // Loaded code preview response.
  // 已加载的代码预览响应。
  preview: WorkspaceFileReadResponse;
}) {
  // diffFile stores a synthetic context-only diff file used to reuse review language and highlight hooks.
  // diffFile 保存合成的纯上下文 diff 文件，用于复用审核语言推断与高亮 hooks。
  const diffFile = React.useMemo(() => workspacePreviewDiffFile(props.preview), [props.preview]);
  // language stores the Shiki language inferred by the review highlighter.
  // language 保存审核高亮器推断出的 Shiki 语言。
  const language = React.useMemo(() => diffLanguageForFile(diffFile), [diffFile]);
  // highlightSupported stores whether the inferred language is handled by the review worker.
  // highlightSupported 保存审核 Worker 是否支持推断语言。
  const highlightSupported = useHighlightLanguage(language);
  // highlightEnabled stores whether this preview is small enough for token highlighting.
  // highlightEnabled 保存当前预览是否足够小以启用 token 高亮。
  const highlightEnabled = Boolean(language && highlightSupported && diffFile.hunks[0].lines.length <= workspaceFilePreviewMaxHighlightRows);
  // highlightRevision requests and tracks Shiki token readiness for the synthetic file.
  // highlightRevision 请求并跟踪合成文件的 Shiki token 就绪状态。
  const highlightRevision = useHighlightedDiffFileRevision(diffFile, language, highlightEnabled);
  // rows stores rendered preview line nodes.
  // rows 保存已渲染的预览行节点。
  const rows = React.useMemo(
    () => workspaceHighlightedCodeRows(diffFile.hunks[0].lines, language, highlightRevision, highlightEnabled),
    [diffFile, highlightEnabled, highlightRevision, language],
  );
  return <div className="workspace-file-preview-code-lines">{rows}</div>;
}

// emptyWorkspaceFileTree creates the shared drawer root used before data has loaded.
// emptyWorkspaceFileTree 创建数据加载前供共享抽屉使用的根节点。
function emptyWorkspaceFileTree(): WorkspaceFileTreeNode<WorkspaceFileEntrySummary> {
  return {
    path: "",
    name: "",
    children: [],
  };
}

// buildWorkspaceFileTree converts loaded project entries into the shared review-tree format.
// buildWorkspaceFileTree 将已加载项目条目转换为共享审核目录树格式。
function buildWorkspaceFileTree(
  // Root entries returned by the backend listing.
  // 后端目录列表返回的根条目。
  entries: WorkspaceFileEntrySummary[],
  // Cached child listings keyed by absolute directory path.
  // 按绝对目录路径缓存的子目录列表。
  directories: Map<string, WorkspaceFileDirectoryListing>,
  // Local filter text applied to already loaded nodes.
  // 应用于已加载节点的本地筛选文本。
  filterText: string,
): WorkspaceFileTreeNode<WorkspaceFileEntrySummary> {
  const root = emptyWorkspaceFileTree();
  const query = filterText.trim().toLowerCase();
  root.children = entries
    .map((entry) => workspaceFileEntryToTreeNode(entry, directories, query))
    .filter((node): node is WorkspaceFileTreeNode<WorkspaceFileEntrySummary> => Boolean(node));
  compressWorkspaceFileTreeDirectoryChains(root);
  return root;
}

// workspaceFileEntryToTreeNode converts one loaded entry and its loaded descendants.
// workspaceFileEntryToTreeNode 转换单个已加载条目及其已加载后代。
function workspaceFileEntryToTreeNode(
  // File or directory entry being converted.
  // 正在转换的文件或目录条目。
  entry: WorkspaceFileEntrySummary,
  // Cached child listings keyed by absolute directory path.
  // 按绝对目录路径缓存的子目录列表。
  directories: Map<string, WorkspaceFileDirectoryListing>,
  // Lowercase filter query.
  // 小写化后的筛选查询。
  query: string,
): WorkspaceFileTreeNode<WorkspaceFileEntrySummary> | null {
  const childEntries = entry.isDirectory ? directories.get(entry.path)?.entries ?? [] : [];
  const children = childEntries
    .map((child) => workspaceFileEntryToTreeNode(child, directories, query))
    .filter((node): node is WorkspaceFileTreeNode<WorkspaceFileEntrySummary> => Boolean(node));
  if (entry.isDirectory && directories.has(entry.path) && children.length === 0) {
    return null;
  }
  const matches = !query || workspaceFileTreeEntryMatchesQuery(entry, query) || children.length > 0;
  if (!matches) {
    return null;
  }
  return {
    path: entry.path,
    name: entry.name,
    children,
    file: entry.isDirectory ? undefined : entry,
  };
}

// compressWorkspaceFileTreeDirectoryChains merges single-child directory chains into review-style compact paths.
// compressWorkspaceFileTreeDirectoryChains 将单子目录链合并为审核式紧凑路径。
function compressWorkspaceFileTreeDirectoryChains(
  // Mutable root node whose directory descendants should be compacted.
  // 需要压缩其目录后代的可变根节点。
  root: WorkspaceFileTreeNode<WorkspaceFileEntrySummary>,
): void {
  root.children = root.children.map(compressWorkspaceFileTreeDirectoryNode);
}

// compressWorkspaceFileTreeDirectoryNode returns one node with folder-only chains merged.
// compressWorkspaceFileTreeDirectoryNode 返回已合并纯目录链的单个节点。
function compressWorkspaceFileTreeDirectoryNode(
  // Tree node to compact.
  // 需要压缩的目录树节点。
  node: WorkspaceFileTreeNode<WorkspaceFileEntrySummary>,
): WorkspaceFileTreeNode<WorkspaceFileEntrySummary> {
  if (node.file) {
    return node;
  }
  let compacted: WorkspaceFileTreeNode<WorkspaceFileEntrySummary> = {
    ...node,
    children: node.children.map(compressWorkspaceFileTreeDirectoryNode),
  };
  while (compacted.children.length === 1 && !compacted.children[0].file) {
    const child = compacted.children[0];
    compacted = {
      ...child,
      name: `${compacted.name}/${child.name}`,
    };
  }
  return compacted;
}

// workspaceFileTreeEntryMatchesQuery returns whether one entry matches the local filter query.
// workspaceFileTreeEntryMatchesQuery 返回单个条目是否匹配本地过滤查询。
function workspaceFileTreeEntryMatchesQuery(entry: WorkspaceFileEntrySummary, query: string): boolean {
  return entry.name.toLowerCase().includes(query) || entry.relativePath.toLowerCase().includes(query);
}

// workspaceFileTreeIconLabel returns the compact icon text used by the shared tree row.
// workspaceFileTreeIconLabel 返回共享目录树行使用的紧凑图标文本。
function workspaceFileTreeIconLabel(
  // Shared tree node being rendered.
  // 正在渲染的共享目录树节点。
  node: WorkspaceFileTreeNode<WorkspaceFileEntrySummary>,
): string {
  if (node.file?.kind === "image") {
    return "IMG";
  }
  if (node.file?.kind === "video") {
    return "VID";
  }
  return workspaceFileTreeExtensionLabel(node.name);
}

// workspaceFileTreeIconClassName maps project file kinds into the shared review icon palette.
// workspaceFileTreeIconClassName 将项目文件类型映射到共享审核图标色板。
function workspaceFileTreeIconClassName(
  // File entry for the rendered row.
  // 当前渲染行对应的文件条目。
  entry: WorkspaceFileEntrySummary | undefined,
): string {
  if (!entry) {
    return "";
  }
  return entry.kind === "code" ? "modified" : "";
}

// workspaceFilePreviewIconLabel returns the compact file-type label for the preview title.
// workspaceFilePreviewIconLabel 返回预览标题使用的紧凑文件类型标签。
function workspaceFilePreviewIconLabel(
  // Loaded preview response shown by the title.
  // 标题展示的已加载预览响应。
  preview: WorkspaceFileReadResponse,
): string {
  if (preview.kind === "image") {
    return "IMG";
  }
  if (preview.kind === "video") {
    return "VID";
  }
  return workspaceFileTreeExtensionLabel(preview.relativePath || preview.path);
}

// workspaceFilePreviewIconClassName maps preview kinds into the shared review icon palette.
// workspaceFilePreviewIconClassName 将预览类型映射到共享审核图标色板。
function workspaceFilePreviewIconClassName(
  // Loaded preview response shown by the title.
  // 标题展示的已加载预览响应。
  preview: WorkspaceFileReadResponse,
): string {
  return preview.kind === "code" ? "modified" : "";
}

// workspacePreviewDiffFile builds a synthetic context-only diff file for code preview highlighting.
// workspacePreviewDiffFile 为代码预览高亮构建合成的纯上下文 diff 文件。
function workspacePreviewDiffFile(
  // Loaded code preview response.
  // 已加载的代码预览响应。
  preview: WorkspaceFileReadResponse,
): DiffFile {
  const lines = workspacePreviewDiffLines(preview.content ?? "");
  const hunk = {
    header: `@@ -1,${lines.length} +1,${lines.length} @@`,
    oldStart: 1,
    newStart: 1,
    lines,
  };
  return {
    key: preview.path,
    oldPath: preview.relativePath || preview.path,
    newPath: preview.relativePath || preview.path,
    status: "modified",
    additions: 0,
    deletions: 0,
    isBinary: false,
    isGitlink: false,
    changedLines: 0,
    changedBytes: preview.sizeBytes,
    maxChangedLineBytes: 0,
    isLarge: false,
    largeReasons: [],
    hunks: [hunk],
  };
}

// workspacePreviewDiffLines converts plain source content into context rows with line numbers.
// workspacePreviewDiffLines 将纯源码内容转换为带行号的上下文行。
function workspacePreviewDiffLines(
  // Source text returned by the workspace file preview RPC.
  // 工作区文件预览 RPC 返回的源码文本。
  content: string,
): DiffLine[] {
  const sourceLines = content.split(/\r?\n/);
  if (sourceLines.length > 1 && sourceLines[sourceLines.length - 1] === "") {
    sourceLines.pop();
  }
  return sourceLines.map((line, index) => ({
    kind: "context",
    content: line,
    oldLine: index + 1,
    newLine: index + 1,
  }));
}

// workspaceHighlightedCodeRows renders numbered source rows with cached Shiki token HTML when available.
// workspaceHighlightedCodeRows 使用可用的缓存 Shiki token HTML 渲染带行号源码行。
function workspaceHighlightedCodeRows(
  // Synthetic diff lines representing the preview source.
  // 表示预览源码的合成 diff 行。
  lines: DiffLine[],
  // Inferred Shiki language.
  // 推断出的 Shiki 语言。
  language: string | null,
  // Highlight revision used to refresh memoized rows when worker results arrive.
  // Worker 结果到达时刷新记忆化行使用的高亮修订号。
  _highlightRevision: number,
  // Whether highlighted rendering should be attempted.
  // 是否应尝试高亮渲染。
  highlightEnabled: boolean,
): React.ReactNode[] {
  const rows: React.ReactNode[] = new Array(lines.length);
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const highlightedSpans = highlightEnabled && language
      ? highlightedDiffLineSpansForDiffLine(line, language)
      : null;
    const html = highlightedSpans ? tokenSpansToHtml(highlightedSpans) : escapeHtml(line.content);
    rows[index] = (
      <div className="workspace-file-preview-code-row" key={index}>
        <span className="workspace-file-preview-code-gutter">{index + 1}</span>
        <span
          className="workspace-file-preview-code-content"
          dangerouslySetInnerHTML={{ __html: html || " " }}
        />
      </div>
    );
  }
  return rows;
}

// findWorkspaceFileEntry finds one loaded tree row by absolute path.
// findWorkspaceFileEntry 通过绝对路径查找单个已加载树行。
function findWorkspaceFileEntry(
  directories: Map<string, WorkspaceFileDirectoryListing>,
  path: string,
): WorkspaceFileEntrySummary | null {
  for (const listing of directories.values()) {
    const match = listing.entries.find((entry) => entry.path === path);
    if (match) {
      return match;
    }
  }
  return null;
}

// rootWorkspaceFileListing resolves the loaded root even when the backend canonicalizes the path key.
// rootWorkspaceFileListing 在后端规范化路径键时也能解析已加载根目录。
function rootWorkspaceFileListing(
  directories: Map<string, WorkspaceFileDirectoryListing>,
  workspacePath: string,
): WorkspaceFileDirectoryListing | null {
  return directories.get(workspacePath)
    ?? Array.from(directories.values()).find((listing) => listing.relativePath === "")
    ?? null;
}

// workspaceFilesPanelCacheKey builds a stable cache key for one visible file panel surface.
// workspaceFilesPanelCacheKey 为单个可见文件面板界面构建稳定缓存键。
function workspaceFilesPanelCacheKey(
  // Conversation or subagent surface id.
  // 会话或子代理界面标识。
  surfaceId: string | null,
  // Workspace root path.
  // 工作区根路径。
  workspacePath: string | null,
  // Dynamic file tab instance id.
  // 动态文件页签实例标识。
  instanceId: string,
): string {
  return `${surfaceId ?? "none"}:${workspacePath ?? "none"}:${instanceId}`;
}

// workspaceFileListingFilters keeps media files visible while preserving real tree filters.
// workspaceFileListingFilters 保持媒体文件在目录树中可见，同时保留真实目录过滤条件。
function workspaceFileListingFilters(
  // Current file filter preferences.
  // 当前文件过滤偏好。
  filters: WorkspaceFileFilterOptions,
): WorkspaceFileFilterOptions {
  return {
    ...filters,
    allowImages: true,
    allowVideos: true,
  };
}

// workspaceFileEntryNeedsMediaPermission returns whether a selected media file needs consent.
// workspaceFileEntryNeedsMediaPermission 返回已选中媒体文件是否需要展示授权。
function workspaceFileEntryNeedsMediaPermission(
  // Selected file entry.
  // 已选中文件条目。
  entry: WorkspaceFileEntrySummary,
  // Current file filter preferences.
  // 当前文件过滤偏好。
  filters: WorkspaceFileFilterOptions,
): boolean {
  return (entry.kind === "image" && !filters.allowImages)
    || (entry.kind === "video" && !filters.allowVideos);
}

// workspaceFileFiltersWithMediaPermission enables the selected media kind.
// workspaceFileFiltersWithMediaPermission 启用已选中的媒体类型。
function workspaceFileFiltersWithMediaPermission(
  // Current file filter preferences.
  // 当前文件过滤偏好。
  filters: WorkspaceFileFilterOptions,
  // Selected entry kind.
  // 已选中条目的类型。
  kind: WorkspaceFileEntrySummary["kind"],
): WorkspaceFileFilterOptions {
  if (kind === "image") {
    return { ...filters, allowImages: true };
  }
  if (kind === "video") {
    return { ...filters, allowVideos: true };
  }
  return filters;
}

// workspaceFilePreviewErrorMessage converts backend preview failures into user-facing text.
// workspaceFilePreviewErrorMessage 将后端预览失败转换为面向用户的提示文本。
function workspaceFilePreviewErrorMessage(
  // Preview load error from the service edge.
  // 来自服务边界的预览加载错误。
  error: unknown,
  // File entry that was being previewed.
  // 正在预览的文件条目。
  entry: WorkspaceFileEntrySummary,
): string {
  // message stores the raw backend or runtime error text.
  // message 保存后端或运行时返回的原始错误文本。
  const message = error instanceof Error ? error.message : String(error);
  // normalizedMessage stores a lowercase copy used for portable missing-file detection.
  // normalizedMessage 保存小写副本，用于跨平台判断文件缺失。
  const normalizedMessage = message.toLowerCase();
  if (
    message.includes("读取文件元数据失败")
    || message.includes("系统找不到指定的文件")
    || normalizedMessage.includes("no such file")
    || normalizedMessage.includes("not found")
    || normalizedMessage.includes("cannot find")
  ) {
    return `文件已被删除或移动，无法预览：${entry.relativePath || entry.path}`;
  }
  return message;
}

// readPersistedWorkspaceFileFilters reads global file filter preferences from localStorage.
// readPersistedWorkspaceFileFilters 从 localStorage 读取全局文件过滤偏好。
function readPersistedWorkspaceFileFilters(): WorkspaceFileFilterOptions {
  if (typeof window === "undefined") {
    return DEFAULT_WORKSPACE_FILE_FILTERS;
  }
  try {
    // rawValue stores the serialized filter preference payload.
    // rawValue 保存序列化后的过滤偏好载荷。
    const rawValue = window.localStorage.getItem(workspaceFileFilterStorageKey);
    if (!rawValue) {
      return DEFAULT_WORKSPACE_FILE_FILTERS;
    }
    // parsedValue stores the untrusted localStorage payload.
    // parsedValue 保存来自 localStorage 的未可信载荷。
    const parsedValue = JSON.parse(rawValue) as Partial<WorkspaceFileFilterOptions>;
    return normalizeWorkspaceFileFilters(parsedValue);
  } catch {
    return DEFAULT_WORKSPACE_FILE_FILTERS;
  }
}

// persistWorkspaceFileFilters writes global file filter preferences to localStorage.
// persistWorkspaceFileFilters 将全局文件过滤偏好写入 localStorage。
function persistWorkspaceFileFilters(
  // Filter preferences to persist.
  // 需要持久化的过滤偏好。
  filters: WorkspaceFileFilterOptions,
): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(workspaceFileFilterStorageKey, JSON.stringify(normalizeWorkspaceFileFilters(filters)));
  } catch {
    // Ignore storage failures so file browsing still works in restricted WebViews.
    // 忽略存储失败，确保受限 WebView 中仍可继续浏览文件。
  }
}

// normalizeWorkspaceFileFilters merges untrusted filter payloads with current defaults.
// normalizeWorkspaceFileFilters 将未可信过滤载荷与当前默认值合并。
function normalizeWorkspaceFileFilters(
  // Partial persisted filter preferences.
  // 部分持久化过滤偏好。
  filters: Partial<WorkspaceFileFilterOptions>,
): WorkspaceFileFilterOptions {
  return {
    allowImages: typeof filters.allowImages === "boolean" ? filters.allowImages : DEFAULT_WORKSPACE_FILE_FILTERS.allowImages,
    allowVideos: typeof filters.allowVideos === "boolean" ? filters.allowVideos : DEFAULT_WORKSPACE_FILE_FILTERS.allowVideos,
    ignoreDotNames: typeof filters.ignoreDotNames === "boolean" ? filters.ignoreDotNames : DEFAULT_WORKSPACE_FILE_FILTERS.ignoreDotNames,
    ignoreHidden: typeof filters.ignoreHidden === "boolean" ? filters.ignoreHidden : DEFAULT_WORKSPACE_FILE_FILTERS.ignoreHidden,
    useGitignore: typeof filters.useGitignore === "boolean" ? filters.useGitignore : DEFAULT_WORKSPACE_FILE_FILTERS.useGitignore,
    useIgnoreFiles: typeof filters.useIgnoreFiles === "boolean" ? filters.useIgnoreFiles : DEFAULT_WORKSPACE_FILE_FILTERS.useIgnoreFiles,
  };
}

// trimWorkspaceFilesPanelCache trims old panel cache entries after new writes.
// trimWorkspaceFilesPanelCache 在写入新缓存后裁剪旧面板缓存条目。
function trimWorkspaceFilesPanelCache() {
  while (workspaceFilesPanelCache.size > workspaceFilesPanelCacheLimit) {
    const firstKey = workspaceFilesPanelCache.keys().next().value;
    if (!firstKey) {
      return;
    }
    workspaceFilesPanelCache.delete(firstKey);
  }
}

// forgetWorkspaceFilesPanelCacheInstance removes runtime cache for a closed file tab.
// forgetWorkspaceFilesPanelCacheInstance 移除已关闭文件页签的运行期缓存。
export function forgetWorkspaceFilesPanelCacheInstance(
  // Dynamic file tab instance id.
  // 动态文件页签实例标识。
  instanceId: string,
): void {
  for (const key of Array.from(workspaceFilesPanelCache.keys())) {
    if (key.endsWith(`:${instanceId}`)) {
      workspaceFilesPanelCache.delete(key);
    }
  }
}

// workspaceFileNameFromPath extracts a display file name from a path string.
// workspaceFileNameFromPath 从路径字符串中提取展示文件名。
function workspaceFileNameFromPath(
  // Relative or absolute file path.
  // 相对或绝对文件路径。
  path: string,
): string {
  // normalizedPath stores slash-normalized path text.
  // normalizedPath 保存斜杠规整后的路径文本。
  const normalizedPath = path.replace(/\\/g, "/");
  return normalizedPath.split("/").filter(Boolean).pop() ?? "打开文件";
}
