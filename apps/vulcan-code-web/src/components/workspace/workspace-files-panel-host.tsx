import React from "react";
import "@/ui-kit/styles/features/workspace-files.css";
import { ChevronsUpDown, FileText, Folder, FolderX, Info, ListFilter, MoreHorizontal, X } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import {
  activeWorkbenchSurfaceId,
  selectedConversationProjectId,
  selectedConversationWorkspacePath,
  type WorkspaceReviewDraftConversation,
} from "@/diff/workspace-review-model";
import { WorkspaceFileTree, type WorkspaceFileTreeNode } from "@/components/workspace/workspace-file-tree";
import { pathMentionText } from "@/components/composer/composer-panel/use-composer-path-mention";
import { FileTypeIcon, resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { PageNotice } from "@/components/page/page-feedback";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui-kit/primitives/popover";
import { ContextMenuItem } from "@/ui-kit/primitives/context-menu";
import "@/ui-kit/styles/features/file-bar.css";
import {
  WorkspaceFilePreview,
  workspaceFilePreviewErrorMessage,
} from "@/components/workspace/workspace-files-preview";
import { useWorkspaceFilesRequestOwnership } from "@/components/workspace/workspace-files-panel-request-ownership";
import { useWorkspaceFilesRefresh, WorkspaceFilesRefreshButton } from "@/components/workspace/workspace-files-panel-refresh";
import { workspacePathActions, type WorkspacePathAction } from "@/components/workspace/workspace-path-actions";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui-kit/primitives/empty";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { DEFAULT_WORKSPACE_FILE_FILTERS } from "@/service/workbench-snapshot-service";
import type {
  WorkbenchSnapshot,
  WorkspaceFileDirectoryListing,
  WorkspaceFileEntrySummary,
  WorkspaceFileFilterOptions,
  WorkspaceFileReadResponse,
} from "@/types";
import { useI18n, type TranslationKey } from "@/i18n";
import "@/diff/diffReview.css";

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
  // Backend directory-filter fingerprint that produced the cached listings.
  // 生成缓存目录列表的后端过滤器指纹。
  listingFiltersKey: string;
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
  // Cached directory listings keyed by absolute directory path.
  // 按绝对目录路径缓存的目录列表。
  directories: [string, WorkspaceFileDirectoryListing][];
}

// WorkspaceFilesPanelHost owns workspace file tree and preview state for the right context panel.
// WorkspaceFilesPanelHost 持有右侧上下文面板的工作区文件树与预览状态。
// Parameters: props supplies route identity, typed file IO and clipboard callbacks. Returns the tree and preview surface.
// 参数：props 提供路由身份、强类型文件输入输出及剪贴板回调。返回文件树与预览界面。
// Parameters: props supplies workspace identity, file services and context-panel callbacks; returns authoritative tree and preview state with explicit read recovery.
// 参数：props 提供工作区身份、文件服务及上下文面板回调；返回带明确读取恢复的权威目录树与预览状态。
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
  onCopyText: (text: string) => void | Promise<void>;
  // Updates the parent dynamic tab title.
  // 更新父级动态页签标题。
  onTitleChange: (title: string) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
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
  // filters stores the active backend file tree filtering options.
  // filters 保存当前后端文件树过滤选项。
  const [filters, setFilters] = React.useState<WorkspaceFileFilterOptions>(() => (
    readPersistedWorkspaceFileFilters()
  ));
  // listingFilters stores tree filters with media always visible in the directory tree.
  // listingFilters 保存目录树过滤器，并始终让媒体文件在目录树中可见。
  const listingFilters = React.useMemo(
    () => workspaceFileListingFilters(filters),
    [filters.filterNonPreviewFiles, filters.ignoreDotNames, filters.ignoreHidden, filters.useGitignore, filters.useIgnoreFiles],
  );
  // listingFiltersKey is the exact backend-filter fingerprint used to validate cached listings and request scope.
  // listingFiltersKey 是用于校验缓存目录与请求作用域的准确后端过滤器指纹。
  const listingFiltersKey = React.useMemo(() => JSON.stringify(listingFilters), [listingFilters]);
  // initialCache restores only panel state produced by the active backend directory filters.
  // initialCache 仅恢复由当前后端目录过滤器生成的面板状态。
  const initialCache = React.useMemo(() => {
    // cachedEntry is the route-local snapshot retained across right-rail unmounts.
    // cachedEntry 是右侧栏卸载期间保留的路由局部快照。
    const cachedEntry = workspaceFilesPanelCache.get(cacheKey) ?? null;
    return cachedEntry?.listingFiltersKey === listingFiltersKey ? cachedEntry : null;
  }, [cacheKey, listingFiltersKey]);
  // filterText stores the local file tree filter query.
  // filterText 保存本地文件树过滤查询。
  const [filterText, setFilterText] = React.useState(() => initialCache?.filterText ?? "");
  // pickerOpen says whether the file bar's tree menu is open.
  // pickerOpen 表示文件栏的目录树菜单是否打开。
  const [pickerOpen, setPickerOpen] = React.useState(false);
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
  const [panelError, setPanelError] = React.useState<{
    // Exact failed request argument; null explicitly represents the root listing.
    // 失败请求的精确参数；null 明确表示根目录列表。
    directoryPath: string | null;
    // Service error displayed without losing the retry target.
    // 保留重试目标时展示的服务错误。
    message: string;
  } | null>(null);
  // loadedInitialDirectoryRef separates first directory hydration from user filter changes.
  // loadedInitialDirectoryRef 区分首次目录水合与用户过滤规则变化。
  const loadedInitialDirectoryRef = React.useRef(false);
  // skipNextDirectoryResetRef prevents cache hydration from looking like a user filter change.
  // skipNextDirectoryResetRef 防止缓存水合被误判为用户过滤规则变化。
  const skipNextDirectoryResetRef = React.useRef(false);
  // lastHydratedCacheKeyRef prevents filter changes from rerunning route-cache hydration.
  // lastHydratedCacheKeyRef 防止过滤器变化重新执行路由缓存水合。
  const lastHydratedCacheKeyRef = React.useRef<string | null>(null);
  // requestScopeKey binds async writers to the current panel route and backend directory filters.
  // requestScopeKey 将异步写入者绑定到当前面板路由与后端目录过滤器。
  const requestScopeKey = React.useMemo(
    () => JSON.stringify([cacheKey, listingFiltersKey]),
    [cacheKey, listingFiltersKey],
  );
  // requestOwnership enforces per-directory coalescing and latest-file preview ownership.
  // requestOwnership 强制逐目录请求合并与最新文件预览所有权。
  const requestOwnership = useWorkspaceFilesRequestOwnership(requestScopeKey);
  // hydratedRequestScopeKey records which route and filter scope owns the current React state fields.
  // hydratedRequestScopeKey 记录当前 React 状态字段所属的路由与过滤作用域。
  const [hydratedRequestScopeKey, setHydratedRequestScopeKey] = React.useState(requestScopeKey);

  React.useLayoutEffect(() => {
    // Scope changes synchronously clear loading indicators owned by invalidated requests.
    // 作用域变化会同步清理由失效请求持有的加载指示器。
    setLoadingDirectories(new Set());
    setPreviewLoading(false);
  }, [requestScopeKey]);

  // loadDirectory fetches one directory, merges it into the directory cache and resolves with it when it applied.
  // loadDirectory 获取单个目录并合并到目录缓存，生效时解析为该目录列表。
  const loadDirectory = React.useCallback(async (directoryPath: string | null) => {
    if (!workspacePath) {
      return;
    }
    // loadingKey identifies one directory request and loading indicator inside the active scope.
    // loadingKey 标识当前作用域内的一次目录请求及其加载指示器。
    const loadingKey = directoryPath ?? workspacePath;
    // ownedRequest prevents duplicate loads and binds every continuation to the committed scope.
    // ownedRequest 防止重复加载，并将全部延续逻辑绑定到已提交作用域。
    const ownedRequest = requestOwnership.beginDirectoryRequest(loadingKey);
    if (!ownedRequest) {
      return;
    }
    setLoadingDirectories((current) => new Set(current).add(loadingKey));
    setPanelError(null);
    try {
      // listing is the backend directory projection captured for this exact route and filter scope.
      // listing 是为该准确路由与过滤作用域捕获的后端目录投影。
      const listing = await props.loadWorkspaceFileDirectory(
        workspacePath,
        workspaceProjectId,
        props.selectedConversationId,
        directoryPath,
        listingFilters,
      );
      if (!requestOwnership.ownsDirectoryRequest(loadingKey, ownedRequest)) {
        return;
      }
      setDirectories((current) => {
        const next = new Map(current);
        next.set(listing.path, listing);
        return next;
      });
      if (listing.notice) {
        toast.info(listing.notice);
      }
      return listing;
    } catch (error) {
      if (requestOwnership.ownsDirectoryRequest(loadingKey, ownedRequest)) {
        setPanelError({ directoryPath, message: error instanceof Error ? error.message : String(error) });
      }
    } finally {
      if (requestOwnership.finishDirectoryRequest(loadingKey, ownedRequest)) {
        setLoadingDirectories((current) => {
          const next = new Set(current);
          next.delete(loadingKey);
          return next;
        });
      }
    }
  }, [listingFilters, props.loadWorkspaceFileDirectory, props.selectedConversationId, requestOwnership, workspacePath, workspaceProjectId]);

  React.useEffect(() => {
    if (lastHydratedCacheKeyRef.current === cacheKey) {
      return;
    }
    lastHydratedCacheKeyRef.current = cacheKey;
    // cachedEntry is the route-local snapshot retained across right-rail unmounts.
    // cachedEntry 是右侧栏卸载期间保留的路由局部快照。
    const cachedEntry = workspaceFilesPanelCache.get(cacheKey) ?? null;
    // nextCache rejects directory state produced by different backend filters.
    // nextCache 拒绝由不同后端过滤器生成的目录状态。
    const nextCache = cachedEntry?.listingFiltersKey === listingFiltersKey ? cachedEntry : null;
    loadedInitialDirectoryRef.current = false;
    skipNextDirectoryResetRef.current = true;
    setFilters(readPersistedWorkspaceFileFilters());
    setFilterText(nextCache?.filterText ?? "");
    setExpandedPaths(new Set(nextCache?.expandedPaths ?? []));
    setDirectories(new Map(nextCache?.directories ?? []));
    setSelectedPath(nextCache?.selectedPath ?? null);
    setSelectedEntrySnapshot(nextCache?.selectedEntry ?? null);
    setPreview(null);
    setPreviewError(null);
    setMediaPermissionEntry(null);
    setPanelError(null);
    setHydratedRequestScopeKey(requestScopeKey);
  }, [cacheKey, listingFiltersKey, requestScopeKey]);

  React.useEffect(() => {
    if (!loadedInitialDirectoryRef.current || skipNextDirectoryResetRef.current) {
      loadedInitialDirectoryRef.current = true;
      skipNextDirectoryResetRef.current = false;
      setHydratedRequestScopeKey(requestScopeKey);
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
    props.onTitleChange(t("workspaceFilePreview.openFile"));
    setHydratedRequestScopeKey(requestScopeKey);
    void loadDirectory(null);
  }, [listingFiltersKey, loadDirectory, props.onTitleChange, requestScopeKey, t, workspacePath]);

  React.useEffect(() => {
    if (hydratedRequestScopeKey !== requestScopeKey) {
      return;
    }
    const cacheEntry: WorkspaceFilesPanelCacheEntry = {
      directories: Array.from(directories.entries()),
      expandedPaths: Array.from(expandedPaths),
      filterText,
      listingFiltersKey,
      selectedEntry: selectedEntrySnapshot,
      selectedPath,
    };
    workspaceFilesPanelCache.set(cacheKey, cacheEntry);
    trimWorkspaceFilesPanelCache();
  }, [cacheKey, directories, expandedPaths, filterText, hydratedRequestScopeKey, listingFiltersKey, requestScopeKey, selectedEntrySnapshot, selectedPath]);

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
  const handleOpenDirectoryPathsChange = React.useCallback((paths: ReadonlySet<string>) => setExpandedPaths(new Set(paths)), []);
  // treeRefresh reloads the loaded tree and the open file's text on demand and after the project's runs or Git changes.
  // treeRefresh 按需以及在项目运行结束或 Git 变化后重新加载已加载的目录树与已打开文件的文本。
  const treeRefresh = useWorkspaceFilesRefresh({ workspacePath, projectId: workspaceProjectId, enabled: hydratedRequestScopeKey === requestScopeKey, directories, loadDirectory, setDirectories,
    openFile: { entry: previewLoading ? null : selectedEntry, preview, ownership: requestOwnership, setPreview, load: (entry) => props.loadWorkspaceFilePreview(workspacePath ?? "", workspaceProjectId, props.selectedConversationId, entry.path, filters) } });
  // handleDirectoryOpenStateChange lazily loads children when the shared drawer opens a directory.
  // handleDirectoryOpenStateChange 在共享抽屉展开目录时懒加载子项。
  const handleDirectoryOpenStateChange = React.useCallback((path: string, open: boolean) => {
    if (hydratedRequestScopeKey === requestScopeKey && open && !directories.has(path)) {
      void loadDirectory(path);
    }
  }, [directories, hydratedRequestScopeKey, loadDirectory, requestScopeKey]);

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
    if (!workspacePath || hydratedRequestScopeKey !== requestScopeKey) {
      return;
    }
    // ownedRequest makes only the latest selected file eligible to update preview state.
    // ownedRequest 仅允许最近选择的文件更新预览状态。
    const ownedRequest = requestOwnership.beginPreviewRequest();
    if (!ownedRequest) {
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
      if (requestOwnership.ownsPreviewRequest(ownedRequest)) {
        setPreview(response);
      }
    } catch (error) {
      if (requestOwnership.ownsPreviewRequest(ownedRequest)) {
        setPreviewError(workspaceFilePreviewErrorMessage(error, entry, t));
      }
    } finally {
      if (requestOwnership.finishPreviewRequest(ownedRequest)) {
        setPreviewLoading(false);
      }
    }
  }, [hydratedRequestScopeKey, props.loadWorkspaceFilePreview, props.selectedConversationId, requestOwnership, requestScopeKey, t, workspacePath, workspaceProjectId]);

  React.useEffect(() => {
    if (hydratedRequestScopeKey !== requestScopeKey) {
      return;
    }
    // Restored file tabs must reload their previous preview even before the tree is reopened.
    // 恢复的文件页签必须在目录树重新展开前重新加载上次预览。
    if (!selectedEntry || selectedEntry.isDirectory || preview || previewLoading || previewError || mediaPermissionEntry) {
      return;
    }
    if (selectedEntry.kind === "other") {
      return;
    }
    if (workspaceFileEntryNeedsMediaPermission(selectedEntry, filters)) {
      requestOwnership.invalidatePreviewRequest();
      setPreviewLoading(false);
      setMediaPermissionEntry(selectedEntry);
      return;
    }
    void loadFilePreview(selectedEntry, filters);
  }, [filters, hydratedRequestScopeKey, loadFilePreview, mediaPermissionEntry, preview, previewError, previewLoading, requestOwnership, requestScopeKey, selectedEntry]);

  // selectFile loads a preview for one file row.
  // selectFile 为单个文件行加载预览。
  const selectFile = React.useCallback(async (entry: WorkspaceFileEntrySummary) => {
    if (!workspacePath || hydratedRequestScopeKey !== requestScopeKey || entry.isDirectory) {
      return;
    }
    setSelectedPath(entry.path);
    setSelectedEntrySnapshot(entry);
    props.onTitleChange(entry.name || workspaceFileNameFromPath(entry.relativePath || entry.path, t("workspaceFilePreview.openFile")));
    if (entry.kind === "other") {
      requestOwnership.invalidatePreviewRequest();
      setPreview(null);
      setPreviewError(null);
      setPreviewLoading(false);
      setMediaPermissionEntry(null);
      return;
    }
    if (workspaceFileEntryNeedsMediaPermission(entry, filters)) {
      requestOwnership.invalidatePreviewRequest();
      setPreview(null);
      setPreviewError(null);
      setPreviewLoading(false);
      setMediaPermissionEntry(entry);
      return;
    }
    await loadFilePreview(entry, filters);
  }, [filters, hydratedRequestScopeKey, loadFilePreview, props.onTitleChange, requestOwnership, requestScopeKey, t, workspacePath]);

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
  // Parameters: entry is the exact requested file. Returns after preview loading and clipboard completion or visible failure.
  // 参数：entry 是准确请求的文件。预览读取与剪贴板完成或可见失败处理后返回。
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
      await props.onCopyText(response.content ?? "");
      if (response.truncated) toast.info(t("workspaceAudit.copyTruncated"));
      else toast.success(t("workspaceFiles.copyContentSuccess"));
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
  // closePreviewAndShowTree closes the open file and returns the panel to the project tree.
  // closePreviewAndShowTree 关闭已打开文件，使面板回到项目目录树。
  const closePreviewAndShowTree = React.useCallback(() => {
    requestOwnership.invalidatePreviewRequest();
    setSelectedPath(null);
    setSelectedEntrySnapshot(null);
    setPreview(null);
    setPreviewError(null);
    setMediaPermissionEntry(null);
    setPreviewLoading(false);
    props.onTitleChange(t("workspaceFilePreview.openFile"));
  }, [props.onTitleChange, requestOwnership, t]);

  if (!workspacePath) {
    return (
      <section className="workspace-files-panel empty">
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon"><Folder /></EmptyMedia>
            <EmptyTitle>{t("contextPanel.files")}</EmptyTitle>
            <EmptyDescription>{t("workspaceFiles.noProjectDirectory")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      </section>
    );
  }

  // rootUnavailable marks a project root that could not be listed (for example a moved or deleted directory);
  // the tree, its filter and its empty label would only repeat that failure.
  // rootUnavailable 标记无法列出的项目根目录（例如目录被移动或删除）；目录树、过滤框与空提示只会重复这一失败。
  const rootUnavailable = !rootListing && panelError?.directoryPath === null;
  const retryPanelLoad = () => { if (panelError) void loadDirectory(panelError.directoryPath); };

  const renderTree = (inPicker: boolean) => (
    <WorkspaceFileTree
      activeFileKey={selectedPath}
      autoFocusFilter={inPicker}
      emptyLabel={rootListing ? t("workspaceFiles.noMatchedFiles") : t("workspaceFiles.noDisplayableFiles")}
      extraFilterActions={<><WorkspaceFilesRefreshButton onRefresh={treeRefresh.refresh} refreshing={treeRefresh.refreshing} /><WorkspaceFilesFilterMenu filters={filters} onFilterChange={updateFilter} /></>}
      fileKey={(entry) => entry.path}
      fileTitle={(entry) => entry.relativePath || entry.name}
      fileDragText={(entry) => pathMentionText(entry.relativePath, false)}
      fileContextMenu={(entry) => workspaceActionMenuItems(workspaceFileActions(entry, props.onCopyText, copyFileContent, t))} directoryContextMenu={(path) => { const entry = findLoadedWorkspaceEntry(directories, path); return entry ? workspaceActionMenuItems(workspaceFileActions(entry, props.onCopyText, copyFileContent, t)) : null; }}
      filter={filterText}
      footer={<WorkspaceFileTreeFooter filters={filters} />}
      loadingDirectoryLabel={t("workspaceFiles.loading")}
      loadingDirectoryPaths={loadingDirectories}
      openDirectoryPaths={expandedPaths}
      tree={fileTree}
      onDirectoryOpenStateChange={handleDirectoryOpenStateChange}
      onFilterChange={setFilterText}
      onOpenDirectoryPathsChange={handleOpenDirectoryPathsChange}
      onSelectFile={(entry) => { setPickerOpen(false); void selectFile(entry); }}
    />
  );

  return (
    <section className="workspace-files-panel" data-open={selectedPath ? "file" : "tree"}>
      {selectedPath ? (
        <WorkspaceOpenFileBar
          actions={selectedEntry ? <WorkspaceFileActionsMenu entry={selectedEntry} onCopyFileContent={copyFileContent} onCopyText={props.onCopyText} /> : null}
          path={selectedEntry?.relativePath || selectedEntry?.name || selectedPath}
          pickerOpen={pickerOpen}
          tree={renderTree(true)}
          truncated={Boolean(preview?.truncated)}
          onClose={closePreviewAndShowTree}
          onPickerOpenChange={setPickerOpen}
        />
      ) : null}
      {!rootListing && loadingDirectories.has(workspacePath) ? (
        <PageNotice tone="info">{t("workspaceFiles.loadingTree")}</PageNotice>
      ) : null}
      {panelError && !rootUnavailable ? (
        <PageNotice
          actions={<Button size="sm" variant="outline" onClick={retryPanelLoad}>{t("workspaceAudit.retry")}</Button>}
          tone="error"
        >
          {panelError.message}
        </PageNotice>
      ) : null}
      {selectedPath ? (
        <WorkspaceFilePreview
          entry={selectedEntry}
          error={previewError}
          loading={previewLoading}
          missingPath={selectedEntry ? null : selectedPath}
          preview={preview}
          mediaPermissionEntry={mediaPermissionEntry}
          resolveMediaSourceUrl={props.resolveMediaSourceUrl}
          onAllowMediaPreview={allowMediaPreview}
          onRetryPreview={() => { if (selectedEntry) void loadFilePreview(selectedEntry, filters); }}
        />
      ) : rootUnavailable ? (
        <WorkspaceFilesRootUnavailable message={panelError?.message ?? ""} onRetry={retryPanelLoad} />
      ) : renderTree(false)}
    </section>
  );
}

// WorkspaceFilesRootUnavailable replaces the project tree when the project root cannot be listed, keeping the
// service error (which names the path) and a retry.
// WorkspaceFilesRootUnavailable 在项目根目录无法列出时替代目录树，保留服务错误（其中包含路径）与重试入口。
function WorkspaceFilesRootUnavailable(props: { message: string; onRetry: () => void }) {
  const { t } = useI18n();
  return (
    <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon"><FolderX /></EmptyMedia>
        <EmptyTitle>{t("workspaceFiles.rootUnavailable")}</EmptyTitle>
        <EmptyDescription className="workspace-files-root-error">{props.message}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button size="sm" variant="outline" onClick={props.onRetry}>{t("workspaceAudit.retry")}</Button>
      </EmptyContent>
    </Empty>
  );
}

// WorkspaceOpenFileBar names the open file above its preview. Its picker opens the project tree as a menu to
// switch files, next to the open file's actions and the button that closes it back to the tree.
// WorkspaceOpenFileBar 在预览上方标明已打开的文件。其选择器以菜单形式打开项目目录树用于切换文件，旁边是该文件的操作与关闭回目录树的按钮。
function WorkspaceOpenFileBar(props: {
  // Project path of the open file.
  // 已打开文件的项目路径。
  path: string;
  // Whether the preview shows only the start of the file.
  // 预览是否只展示文件开头部分。
  truncated: boolean;
  pickerOpen: boolean;
  onPickerOpenChange: (open: boolean) => void;
  // The project tree shown inside the picker.
  // 选择器中展示的项目目录树。
  tree: React.ReactNode;
  // The open file's actions menu.
  // 已打开文件的操作菜单。
  actions: React.ReactNode;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [dir, name] = splitWorkspaceRelativePath(props.path);
  return (
    <div className="vk-filebar">
      <Popover onOpenChange={props.onPickerOpenChange} open={props.pickerOpen}>
        <PopoverTrigger aria-label={`${t("workspaceFiles.switchFile")} · ${props.path}`} className="vk-filepick" title={props.path} type="button">
          <FileTypeIcon icon={resolveFileTypeIcon(name)} />
          <span className="vk-filepick-name">
            <strong>{name}</strong>
            {dir ? <small>{dir}</small> : null}
          </span>
          {props.truncated ? <span className="vk-badge" data-tone="warn">{t("workspaceFilePreview.truncated")}</span> : null}
          <ChevronsUpDown aria-hidden="true" className="vk-faint" size={13} />
        </PopoverTrigger>
        <PopoverContent align="start" className="vk-filemenu" sideOffset={2}>
          {props.tree}
        </PopoverContent>
      </Popover>
      {props.actions}
      <IconButton aria-label={t("workspaceFiles.closeFile")} onClick={props.onClose} size="sm" title={t("workspaceFiles.closeFile")}>
        <X size={14} />
      </IconButton>
    </div>
  );
}

// WorkspaceFileTreeFooter says what the tree leaves out and that the panel shows one file at a time.
// WorkspaceFileTreeFooter 说明目录树省略了哪些内容，以及面板一次只展示一个文件。
function WorkspaceFileTreeFooter(props: { filters: WorkspaceFileFilterOptions }) {
  const { t } = useI18n();
  const text = [...workspaceFileActiveRuleLabels(props.filters, t), t("workspaceFiles.oneFileAtATime")].join(" · ");
  return (
    <>
      <Info aria-hidden="true" size={12} />
      <span title={text}>{text}</span>
    </>
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
  const { t } = useI18n();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <IconButton aria-label={t("workspaceFiles.filterRules")} size="sm" title={t("workspaceFiles.filterRules")}>
          <ListFilter size={14} />
        </IconButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="workspace-files-filter-menu">
        <DropdownMenuLabel>{t("workspaceFiles.hiddenRules")}</DropdownMenuLabel>
        <DropdownMenuCheckboxItem
          checked={props.filters.ignoreHidden}
          onCheckedChange={(value) => props.onFilterChange("ignoreHidden", Boolean(value))}
        >
          {t("workspaceFiles.ignoreHiddenFiles")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={props.filters.ignoreDotNames}
          onCheckedChange={(value) => props.onFilterChange("ignoreDotNames", Boolean(value))}
        >
          {t("workspaceFiles.ignoreDotNames")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={props.filters.useIgnoreFiles}
          onCheckedChange={(value) => props.onFilterChange("useIgnoreFiles", Boolean(value))}
        >
          {t("workspaceFiles.useIgnoreRules")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={props.filters.useGitignore}
          onCheckedChange={(value) => props.onFilterChange("useGitignore", Boolean(value))}
        >
          {t("workspaceFiles.useGitignoreRules")}
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={props.filters.filterNonPreviewFiles}
          onCheckedChange={(value) => props.onFilterChange("filterNonPreviewFiles", Boolean(value))}
        >
          {t("workspaceFiles.filterNonPreviewFiles")}
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// WorkspaceFileActionsMenu holds what can be done with the open file: mention it in the draft the way the
// composer's @ list does, copy its path, its path inside the project, or its content when it is code.
// WorkspaceFileActionsMenu 收纳对已打开文件的操作：以输入区 @ 列表的方式在草稿中引用它、复制其路径、项目内路径，
// 或在其为代码时复制内容。
function WorkspaceFileActionsMenu(props: {
  // The open file.
  // 已打开的文件。
  entry: WorkspaceFileEntrySummary;
  // Copies text using the host clipboard.
  // 使用宿主剪贴板复制文本。
  onCopyText: (text: string) => void | Promise<void>;
  // Copies one code file's content through the backend preview path.
  // 通过后端预览路径复制单个代码文件内容。
  onCopyFileContent: (entry: WorkspaceFileEntrySummary) => void;
}) {
  const { t } = useI18n();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <IconButton aria-label={t("workspaceFiles.fileActions")} data-action="file-actions" size="sm" title={t("workspaceFiles.fileActions")}>
          <MoreHorizontal size={14} />
        </IconButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {workspaceFileActions(props.entry, props.onCopyText, props.onCopyFileContent, t).map((action) => (
          <DropdownMenuItem disabled={action.disabled} key={action.key} onClick={action.run}>{action.icon}{action.label}</DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// workspaceFileActions lists what can be done with one file, shared by the open file's menu and the tree rows'
// right-click menu.
// workspaceFileActions 列出对单个文件可执行的操作，供已打开文件的菜单与目录树行的右键菜单共用。
function workspaceFileActions(
  entry: WorkspaceFileEntrySummary,
  onCopyText: (text: string) => void | Promise<void>,
  onCopyFileContent: (entry: WorkspaceFileEntrySummary) => void,
  t: (key: TranslationKey) => string,
): WorkspacePathAction[] {
  const actions = workspacePathActions(entry, onCopyText, t);
  if (entry.isDirectory) return actions;
  // A file adds its content copy before the system-folder entry.
  // 文件在系统文件夹项之前加入内容复制。
  actions.splice(actions.length - 1, 0, { key: "content", icon: <FileText size={14} />, label: t("workspaceFiles.copyFileContent"),
    disabled: entry.kind !== "code", run: () => onCopyFileContent(entry) });
  return actions;
}

// workspaceActionMenuItems renders file or folder actions as right-click menu items.
// workspaceActionMenuItems 将文件或目录操作渲染为右键菜单项。
function workspaceActionMenuItems(actions: ReturnType<typeof workspaceFileActions>): React.ReactNode {
  return actions.map((action) => <ContextMenuItem disabled={action.disabled} key={action.key} onSelect={action.run}>{action.icon}{action.label}</ContextMenuItem>);
}

// findLoadedWorkspaceEntry finds the loaded listing entry for one absolute path, or null before its parent loads.
// findLoadedWorkspaceEntry 查找某个绝对路径对应的已加载列表条目；父目录尚未加载时返回 null。
function findLoadedWorkspaceEntry(directories: Map<string, WorkspaceFileDirectoryListing>, path: string): WorkspaceFileEntrySummary | null {
  for (const listing of directories.values()) {
    const entry = listing.entries.find((candidate) => candidate.path === path);
    if (entry) return entry;
  }
  return null;
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

// workspaceFileActiveRuleLabels names the filter rules that are on, in menu order.
// workspaceFileActiveRuleLabels 按菜单顺序列出已开启的过滤规则名称。
function workspaceFileActiveRuleLabels(filters: WorkspaceFileFilterOptions, t: (key: TranslationKey) => string): string[] {
  const rules: Array<[boolean, TranslationKey]> = [
    [filters.useGitignore, "workspaceFiles.useGitignoreRules"],
    [filters.useIgnoreFiles, "workspaceFiles.useIgnoreRules"],
    [filters.ignoreHidden, "workspaceFiles.ignoreHiddenFiles"],
    [filters.ignoreDotNames, "workspaceFiles.ignoreDotNames"],
    [filters.filterNonPreviewFiles, "workspaceFiles.filterNonPreviewFiles"],
  ];
  return rules.filter(([on]) => on).map(([, key]) => t(key));
}

// splitWorkspaceRelativePath splits a project path into its folder and file name.
// splitWorkspaceRelativePath 将项目路径拆分为目录与文件名。
function splitWorkspaceRelativePath(path: string): [string, string] {
  const normalized = path.replace(/\\/g, "/");
  const slash = normalized.lastIndexOf("/");
  return slash < 0 ? ["", normalized] : [normalized.slice(0, slash), normalized.slice(slash + 1)];
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
    filterNonPreviewFiles: typeof filters.filterNonPreviewFiles === "boolean" ? filters.filterNonPreviewFiles : DEFAULT_WORKSPACE_FILE_FILTERS.filterNonPreviewFiles,
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
  // Fallback label used when the path has no visible file name.
  // 路径没有可见文件名时使用的回退标签。
  fallback: string,
): string {
  // normalizedPath stores slash-normalized path text.
  // normalizedPath 保存斜杠规整后的路径文本。
  const normalizedPath = path.replace(/\\/g, "/");
  return normalizedPath.split("/").filter(Boolean).pop() ?? fallback;
}
