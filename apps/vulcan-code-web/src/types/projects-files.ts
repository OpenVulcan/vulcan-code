export interface ProjectSummary {
  // Stable project identifier.
  // 稳定项目标识。
  id: string;
  // Project display name.
  // 项目显示名称。
  name: string;
  // Project path.
  // 项目路径。
  path: string;
  // Whether the project is currently pinned.
  // 项目当前是否处于置顶状态。
  pinned?: boolean;
  // Optional icon pack selected for the project.
  // 项目选择的可选图标包。
  iconPack?: string | null;
  // Optional icon key selected for the project.
  // 项目选择的可选图标键。
  iconKey?: string | null;
  // How the sidebar orders the project's unpinned sessions; absent means last-request order.
  // 侧栏如何排列该项目的非置顶会话；缺省表示按最近请求排序。
  sessionSortMode?: ProjectSessionSortMode;
}

// ProjectSessionSortMode mirrors the service enum: recency groups versus a user-arranged, draggable order.
// ProjectSessionSortMode 镜像服务枚举：按时间分组或用户排列且可拖动的顺序。
export type ProjectSessionSortMode = "last_request" | "manual";

// ProjectGroupSummary describes one user-named project group in display order.
// ProjectGroupSummary 描述一个按显示顺序排列的用户命名项目分组。
export interface ProjectGroupSummary {
  // Stable group identifier.
  // 稳定分组标识。
  id: string;
  // Group display name.
  // 分组显示名称。
  name: string;
  // Visible member project identifiers in display order.
  // 按显示顺序排列的可见成员项目标识。
  projectIds: string[];
}

// ProjectTreeItemKind is the kind of an item in the project tree's pinned section.
// ProjectTreeItemKind 是项目树置顶分区中条目的种类。
export type ProjectTreeItemKind = "session" | "project" | "group";

// ProjectTreePin is one item of the pinned section; the default group pins as "default".
// ProjectTreePin 是置顶分区中的一个条目；默认分组以 "default" 置顶。
export interface ProjectTreePin {
  // Kind of the pinned item.
  // 置顶条目的种类。
  kind: ProjectTreeItemKind;
  // Item identifier within its kind.
  // 条目在其种类内的标识。
  id: string;
}

// ProjectTreeSummary is the code flow's project tree: user groups, the default group position and the pins.
// ProjectTreeSummary 是代码流的项目树：用户分组、默认分组位置与置顶。
export interface ProjectTreeSummary {
  // User groups in display order, without the default group.
  // 按显示顺序排列的用户分组，不含默认分组。
  groups: ProjectGroupSummary[];
  // Position of the default group among the user groups, from 0 (first) to the group count (last).
  // 默认分组在用户分组之间的位置，从 0（最前）到分组数量（最后）。
  defaultGroupIndex: number;
  // Visible pinned items in display order.
  // 按显示顺序排列的可见置顶条目。
  pins: ProjectTreePin[];
}

// ProjectGroupLayoutEntry places one group's members in a requested project-group layout.
// ProjectGroupLayoutEntry 在请求的项目分组布局中放置一个分组的成员。
export interface ProjectGroupLayoutEntry {
  // Existing group identifier.
  // 已存在的分组标识。
  groupId: string;
  // Visible member project identifiers in display order.
  // 按显示顺序排列的可见成员项目标识。
  projectIds: string[];
}

// ProjectDirectoryEntrySummary describes one child directory returned by the service.
// ProjectDirectoryEntrySummary 描述服务返回的单个子目录。
export interface ProjectDirectoryEntrySummary {
  // Directory display name.
  // 目录显示名称。
  name: string;
  // Absolute path on the service host.
  // 服务宿主上的绝对路径。
  path: string;
  // Whether the directory is hidden by name convention.
  // 目录是否符合隐藏命名约定。
  hidden: boolean;
}

// ProjectDirectoryRootSummary describes one filesystem root returned by the service.
// ProjectDirectoryRootSummary 描述服务返回的单个文件系统根。
export interface ProjectDirectoryRootSummary {
  // Root display label.
  // 根目录显示标签。
  label: string;
  // Absolute root path on the service host.
  // 服务宿主上的绝对根路径。
  path: string;
}

// ProjectDirectoryListing describes one browsed service-host directory.
// ProjectDirectoryListing 描述一个已浏览的服务宿主目录。
export interface ProjectDirectoryListing {
  // Selectable filesystem roots.
  // 可选择的文件系统根。
  roots?: ProjectDirectoryRootSummary[];
  // Current absolute directory path.
  // 当前绝对目录路径。
  path: string;
  // Optional parent directory path.
  // 可选父目录路径。
  parentPath?: string | null;
  // Whether the current directory can be selected.
  // 当前目录是否可以选择。
  canSelect: boolean;
  // Child directory rows.
  // 子目录行。
  entries: ProjectDirectoryEntrySummary[];
  // Optional listing notice.
  // 可选列表提示。
  notice?: string | null;
}

// PackageFileSummary is one selectable package file of a browsed directory.
// PackageFileSummary 是已浏览目录中单个可选的套件文件。
export interface PackageFileSummary {
  // Visible file name.
  // 可见文件名。
  name: string;
  // Absolute native path.
  // 原生绝对路径。
  path: string;
  // File size in bytes.
  // 文件字节大小。
  byteSize: number;
}

// PackageFileListing is directory navigation with the selectable package files of one kind.
// PackageFileListing 是包含某一类可选套件文件的目录导航。
export interface PackageFileListing {
  // Shared native directory navigation.
  // 共享原生目录导航。
  directory: ProjectDirectoryListing;
  // Ordinary files of the package kind in the directory.
  // 目录中该套件类型的普通文件。
  packages: PackageFileSummary[];
}

// WorkspaceFileEntryKind describes the file families rendered by the workspace file panel.
// WorkspaceFileEntryKind 描述工作区文件面板渲染的文件类别。
export type WorkspaceFileEntryKind = "directory" | "code" | "image" | "video" | "other";

// WorkspaceFileFilterOptions stores the active workspace file tree filters.
// WorkspaceFileFilterOptions 保存当前工作区文件树过滤器。
export interface WorkspaceFileFilterOptions {
  // Whether platform-hidden entries are filtered out.
  // 是否过滤平台隐藏条目。
  ignoreHidden: boolean;
  // Whether dot-prefixed entries are filtered out.
  // 是否过滤点开头条目。
  ignoreDotNames: boolean;
  // Whether `.ignore` files are applied.
  // 是否应用 `.ignore` 文件。
  useIgnoreFiles: boolean;
  // Whether `.gitignore` and Git ignore sources are applied.
  // 是否应用 `.gitignore` 与 Git 忽略来源。
  useGitignore: boolean;
  // Whether files outside code, image, and video preview groups are filtered out.
  // 是否过滤代码、图片和视频预览组之外的文件。
  filterNonPreviewFiles: boolean;
  // Whether image files may be previewed after selection.
  // 选中后是否允许预览图片文件。
  allowImages: boolean;
  // Whether video files may be previewed after selection.
  // 选中后是否允许预览视频文件。
  allowVideos: boolean;
}

// WorkspaceFileEntrySummary describes one file tree row returned by the service.
// WorkspaceFileEntrySummary 描述服务返回的单个文件树行。
export interface WorkspaceFileEntrySummary {
  // Display file or directory name.
  // 文件或目录显示名称。
  name: string;
  // Absolute path on the service host.
  // 服务宿主上的绝对路径。
  path: string;
  // Workspace-root-relative path.
  // 工作区根相对路径。
  relativePath: string;
  // Entry kind used by the preview renderer.
  // 预览渲染器使用的条目类别。
  kind: WorkspaceFileEntryKind;
  // Whether the row is a directory.
  // 当前行是否为目录。
  isDirectory: boolean;
  // Optional lowercase extension.
  // 可选小写扩展名。
  extension?: string | null;
  // Optional MIME type for preview.
  // 用于预览的可选 MIME 类型。
  mimeType?: string | null;
  // Optional file size in bytes.
  // 可选文件字节大小。
  sizeBytes?: number | null;
  // Optional modified timestamp in Unix milliseconds text.
  // 可选 Unix 毫秒文本修改时间。
  modifiedAt?: string | null;
  // Whether the row is hidden by platform or naming rules.
  // 当前行是否被平台或命名规则视为隐藏。
  hidden: boolean;
  // Whether a directory may have children.
  // 目录是否可能存在子项。
  hasChildren: boolean;
}

// WorkspaceFileDirectoryListing describes one lazy directory listing response.
// WorkspaceFileDirectoryListing 描述一次懒加载目录列表响应。
export interface WorkspaceFileDirectoryListing {
  // Absolute workspace root path.
  // 工作区根绝对路径。
  workspacePath: string;
  // Current absolute directory path.
  // 当前目录绝对路径。
  path: string;
  // Current relative directory path.
  // 当前目录相对路径。
  relativePath: string;
  // Optional parent path inside the workspace.
  // 工作区内的可选父路径。
  parentPath?: string | null;
  // Child entries returned by the service.
  // 服务返回的子条目。
  entries: WorkspaceFileEntrySummary[];
  // Backend-applied filters.
  // 后端实际应用的过滤器。
  filters: WorkspaceFileFilterOptions;
  // Whether the response was truncated.
  // 响应是否被截断。
  truncated: boolean;
  // Number of returned entries.
  // 返回条目数量。
  entryCount: number;
  // Optional human-readable notice.
  // 可选人类可读提示。
  notice?: string | null;
}

// WorkspaceFileReadResponse describes one workspace file preview response.
// WorkspaceFileReadResponse 描述单个工作区文件预览响应。
export interface WorkspaceFileReadResponse {
  // Absolute workspace root path.
  // 工作区根绝对路径。
  workspacePath: string;
  // Absolute file path.
  // 文件绝对路径。
  path: string;
  // Workspace-root-relative file path.
  // 工作区根相对文件路径。
  relativePath: string;
  // Preview kind.
  // 预览类别。
  kind: WorkspaceFileEntryKind;
  // Optional MIME type.
  // 可选 MIME 类型。
  mimeType?: string | null;
  // File size in bytes.
  // 文件字节大小。
  sizeBytes: number;
  // UTF-8 content for code files.
  // 代码文件的 UTF-8 内容。
  content?: string | null;
  // Encoding label for textual content.
  // 文本内容编码标签。
  encoding?: string | null;
  // Optional returned line count.
  // 可选返回行数。
  lineCount?: number | null;
  // Whether returned text is truncated.
  // 返回文本是否已截断。
  truncated: boolean;
}
