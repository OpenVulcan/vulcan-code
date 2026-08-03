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
  // Whether host-managed Codebase Memory is enabled for this project.
  // 此项目是否启用了宿主管理的 Codebase Memory。
  codebaseMemoryEnabled?: boolean;
}

// CodebaseMemoryIndexJobSummary describes one exact managed index job snapshot.
// CodebaseMemoryIndexJobSummary 描述一份精确的托管索引任务快照。
export interface CodebaseMemoryIndexJobSummary {
  // Sidecar runtime identity.
  // Sidecar 运行时身份。
  runtimeId: string;
  // Runtime-local job identity.
  // 运行时内任务身份。
  jobId: string;
  // Monotonic project-local job generation.
  // 项目内单调递增的任务代次。
  jobGeneration: string;
  // Exact update or rebuild mode.
  // 精确的更新或重建模式。
  mode: string;
  // Stable scheduling trigger.
  // 稳定的调度来源。
  trigger: string;
  // Current or terminal job state.
  // 当前或终态任务状态。
  state: string;
  // Current monotonic job phase.
  // 当前单调任务阶段。
  phase: string;
  // Completed work units as lossless decimal text.
  // 以无损十进制文本表示的已完成工作单元。
  completedUnits: string;
  // Stable total work units when known.
  // 已知时的稳定总工作单元。
  totalUnits?: string | null;
  // Bounded work-unit label.
  // 有界工作单元标签。
  unit: string;
  // RFC 3339 job start timestamp.
  // RFC 3339 任务开始时间。
  startedAt?: string | null;
  // RFC 3339 latest progress timestamp.
  // RFC 3339 最近进度时间。
  updatedAt?: string | null;
  // RFC 3339 terminal timestamp.
  // RFC 3339 终态时间。
  completedAt?: string | null;
  // Whether the request joined an existing physical job.
  // 请求是否合并进既有物理任务。
  coalesced: boolean;
  // Optional bounded job error.
  // 可选的有界任务错误。
  error?: string | null;
}

// CodebaseMemoryProjectSettingsSummary describes project-owned enablement and managed runtime diagnostics.
// CodebaseMemoryProjectSettingsSummary 描述项目自有启用状态与托管运行时诊断。
export interface CodebaseMemoryProjectSettingsSummary {
  // Stable project identifier.
  // 稳定项目标识。
  projectId: string;
  // Persisted project-owned enablement state.
  // 项目持有的持久化启用状态。
  enabled: boolean;
  // Current managed sidecar lifecycle status.
  // 当前托管 Sidecar 生命周期状态。
  serviceStatus: string;
  // Availability of the last atomically published index.
  // 最近一次原子发布索引的可用状态。
  availability?: string | null;
  // Whether the project watcher is registered.
  // 项目监视器是否已经注册。
  watcherRegistered?: boolean | null;
  // Last successful index timestamp reported by the sidecar.
  // Sidecar 报告的最近成功索引时间。
  lastSuccessfulIndexAt?: string | null;
  // Current project index revision reported by the sidecar.
  // Sidecar 报告的当前项目索引修订号。
  indexRevision?: string | null;
  // Current indexed node count from the latest live query.
  // 最近一次实时查询得到的索引节点数。
  nodeCount?: string | null;
  // Current indexed edge count from the latest live query.
  // 最近一次实时查询得到的索引边数。
  edgeCount?: string | null;
  // Currently active physical index job.
  // 当前活动的物理索引任务。
  activeJob?: CodebaseMemoryIndexJobSummary | null;
  // Latest immutable terminal index job.
  // 最近一次不可变终态索引任务。
  lastJob?: CodebaseMemoryIndexJobSummary | null;
  // Latest non-sensitive project or sidecar error.
  // 最近一次不含敏感信息的项目或 Sidecar 错误。
  lastError?: string | null;
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
