use super::*;

/// Summarizes the dynamic and static context injected into the selected session.
/// 汇总当前选中会话中的动态与静态上下文注入信息。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInjectionSnapshot {
    /// Dynamic per-turn injection cards.
    /// 按轮次生效的动态注入卡片。
    pub dynamic_cards: Vec<ContextInjectionCardSummary>,
    /// Structured active VMM profile snapshot for static memory display.
    /// 用于静态记忆展示的结构化 active VMM 画像快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_profile: Option<VmmStaticProfileSummary>,
    /// Static persistent injection cards.
    /// 持续生效的静态注入卡片。
    pub static_cards: Vec<ContextInjectionCardSummary>,
}

/// Summarizes the current active VMM profile nodes.
/// 汇总当前 active VMM 画像节点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmStaticProfileSummary {
    /// Unix-millis text captured when the profile snapshot was updated.
    /// 画像快照更新时捕获的 Unix 毫秒文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Profile sections grouped by VMM target scope.
    /// 按 VMM 目标作用域分组的画像段落。
    pub sections: Vec<VmmStaticProfileSectionSummary>,
}

/// Summarizes one VMM profile target section.
/// 汇总一个 VMM 画像目标段落。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmStaticProfileSectionSummary {
    /// Stable target key such as team, space, project, or user.
    /// 稳定目标键，例如 team、space、project 或 user。
    pub target: String,
    /// Uppercase target label displayed by the panel.
    /// 面板展示的大写目标标签。
    pub label: String,
    /// Date groups under this target.
    /// 当前目标下的日期分组。
    pub dates: Vec<VmmStaticProfileDateGroupSummary>,
}

/// Summarizes one date group inside a VMM profile section.
/// 汇总 VMM 画像段落内的单个日期分组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmStaticProfileDateGroupSummary {
    /// Human-readable profile date.
    /// 人类可读画像日期。
    pub date: String,
    /// Active profile nodes for this date.
    /// 当前日期下的 active 画像节点。
    pub nodes: Vec<VmmStaticProfileNodeSummary>,
}

/// Summarizes one active VMM profile node for UI display.
/// 汇总单个用于 UI 展示的 active VMM 画像节点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VmmStaticProfileNodeSummary {
    /// Durable VMM profile node id.
    /// 长期 VMM 画像节点标识。
    pub id: u64,
    /// Priority marker such as P1.
    /// 优先级标记，例如 P1。
    pub priority: String,
    /// Lifetime marker such as L2.
    /// 生命周期标记，例如 L2。
    pub level: String,
    /// Refresh weight marker such as W3.
    /// 刷新权重标记，例如 W3。
    pub refresh_weight: String,
    /// Atomic profile content.
    /// 原子化画像内容。
    pub content: String,
    /// Source kind key returned by VMM.
    /// VMM 返回的来源类型键。
    pub source_kind: String,
    /// Source row id such as the originating turn id.
    /// 来源行标识，例如来源回合标识。
    pub source_id: u64,
    /// Bound user/project/team/space id.
    /// 绑定的 user/project/team/space 标识。
    pub bind_id: u64,
}

/// Summarizes one context injection source group.
/// 汇总单个上下文注入来源分组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInjectionCardSummary {
    /// Stable context injection tag.
    /// 稳定的上下文注入标记。
    pub tag: String,
    /// Human-readable card description.
    /// 面向用户的卡片说明。
    pub description: String,
    /// Items emitted by this injection source.
    /// 当前注入来源产生的条目。
    pub items: Vec<ContextInjectionItemSummary>,
}

/// Summarizes one visible context injection item.
/// 汇总单条可见上下文注入内容。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextInjectionItemSummary {
    /// Compact content preview displayed in the side panel.
    /// 侧面板展示的紧凑内容预览。
    pub content: String,
    /// Full detail text shown on hover or expansion.
    /// 悬停或展开时展示的完整详情文本。
    pub detail: String,
    /// Optional turn label that owns this injection.
    /// 拥有该注入的可选轮次标签。
    pub turn_label: Option<String>,
    /// TTL or persistence label.
    /// TTL 或持久化状态标签。
    pub ttl_label: String,
    /// Optional source display label.
    /// 可选来源显示标签。
    pub source_display: Option<String>,
    /// Optional stable injection dedupe key for current-session suppression.
    /// 用于当前会话抑制的可选稳定注入去重键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    /// Optional durable VMM memory id when this item comes from a memory row.
    /// 当条目来自长期 VMM 记忆行时对应的可选记忆标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<u64>,
    /// Optional source turn id when the backing dialogue can be queried.
    /// 当背后对话可查询时对应的可选来源回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<u64>,
    /// Optional relevance score label.
    /// 可选相关性评分标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_label: Option<String>,
    /// Whether the item can query full turn detail.
    /// 条目是否可以查询完整回合详情。
    #[serde(default)]
    pub has_dialogue: bool,
    /// Optional VMM creation datetime.
    /// 可选 VMM 创建时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_datetime: Option<String>,
    /// Optional injection update timestamp.
    /// 可选注入更新时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Describes one calibrated category inside the context-usage card.
/// 描述上下文占用卡片中的单个校准分类。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageBreakdownItem {
    /// Stable category key consumed by WebUI styling.
    /// WebUI 样式消费的稳定分类键。
    pub kind: String,
    /// User-facing category label.
    /// 面向用户的分类标签。
    pub label: String,
    /// Provider-calibrated token count assigned to this category.
    /// 分配给该分类的 Provider 校准 Token 数。
    pub tokens: usize,
    /// Share of used context in basis points from zero through ten thousand.
    /// 占已使用上下文的比例，单位为万分之一，范围为零到一万。
    pub basis_points: u16,
}

/// Summarizes context token usage for the selected session composer.
/// 汇总当前选中会话输入区的上下文 Token 占用。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageSnapshot {
    /// Rounded usage percent shown inside the orb.
    /// 圆盘中显示的四舍五入占用百分比。
    pub percent: u8,
    /// Precise window usage in basis points from zero through ten thousand.
    /// 精确窗口占用比例，单位为万分之一，范围为零到一万。
    #[serde(default)]
    pub usage_basis_points: u16,
    /// Provider-calibrated active context tokens.
    /// Provider 校准后的活动上下文 Token 数。
    #[serde(default)]
    pub used_tokens: usize,
    /// Effective context window when the active model declares one.
    /// 当前模型声明时的有效上下文窗口。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    /// Cumulative active-branch cache hit rate in basis points.
    /// 活动分支累计缓存命中率，单位为万分之一。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hit_basis_points: Option<u16>,
    /// Calibrated category rows; empty means compact legacy display.
    /// 校准后的分类行；为空时表示使用旧会话紧凑展示。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakdown: Vec<ContextUsageBreakdownItem>,
    /// Severity key used by WebUI styling.
    /// WebUI 样式使用的严重级别键。
    pub severity: String,
    /// Whether the active runtime still matches the accepted baseline.
    /// 当前运行时是否仍匹配已接受的基线。
    pub baseline_matches_active_runtime: bool,
    /// Tooltip diagnostic rows.
    /// 提示浮层诊断行。
    pub tooltip_rows: Vec<String>,
}

/// Summarizes a project in the WebUI sidebar.
/// 汇总 WebUI 侧栏中的项目。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// Stable project identifier.
    /// 稳定项目标识。
    pub id: String,
    /// Project display name.
    /// 项目显示名称。
    pub name: String,
    /// Project path shown to users.
    /// 展示给用户的项目路径。
    pub path: String,
    /// Whether the project is currently pinned.
    /// 项目当前是否处于置顶状态。
    #[serde(default)]
    pub pinned: bool,
    /// Optional icon pack used to render the project.
    /// 用于渲染项目的可选图标包。
    #[serde(default)]
    pub icon_pack: Option<String>,
    /// Optional icon key used to render the project.
    /// 用于渲染项目的可选图标键。
    #[serde(default)]
    pub icon_key: Option<String>,
}

/// Summarizes one user-named project group shown in the WebUI sidebar.
/// 汇总 WebUI 侧栏中显示的一个用户命名项目分组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGroupSummary {
    /// Stable group identifier.
    /// 稳定分组标识。
    pub id: String,
    /// Group display name.
    /// 分组显示名称。
    pub name: String,
    /// Visible member project identifiers in display order.
    /// 按显示顺序排列的可见成员项目标识。
    pub project_ids: Vec<String>,
}

/// Places one group's members in a requested project-group layout.
/// 在请求的项目分组布局中放置一个分组的成员。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGroupLayoutEntry {
    /// Existing group identifier.
    /// 已存在的分组标识。
    pub group_id: String,
    /// Visible member project identifiers in display order.
    /// 按显示顺序排列的可见成员项目标识。
    pub project_ids: Vec<String>,
}

/// Summarizes one filesystem directory row returned to WebUI folder pickers.
/// 汇总一条返回给 WebUI 文件夹选择器的文件系统目录行。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectoryEntrySummary {
    /// Directory display name.
    /// 目录显示名称。
    pub name: String,
    /// Absolute directory path on the service host.
    /// 服务宿主上的绝对目录路径。
    pub path: String,
    /// Whether the directory name looks hidden to the current platform.
    /// 当前目录名称在平台语义下是否看起来为隐藏目录。
    pub hidden: bool,
}

/// Summarizes one selectable filesystem root returned to WebUI folder pickers.
/// 汇总一条返回给 WebUI 文件夹选择器的可选文件系统根。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectoryRootSummary {
    /// User-facing root label such as C: or /.
    /// 面向用户显示的根标签，例如 C: 或 /。
    pub label: String,
    /// Absolute root path on the service host.
    /// 服务宿主上的绝对根路径。
    pub path: String,
}

/// Describes one browsed filesystem directory for WebUI project selection.
/// 描述一次 WebUI 项目选择浏览到的文件系统目录。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDirectoryListing {
    /// Selectable filesystem roots available on the service host.
    /// 服务宿主上可选择的文件系统根列表。
    #[serde(default)]
    pub roots: Vec<ProjectDirectoryRootSummary>,
    /// Absolute current directory path on the service host.
    /// 服务宿主上的当前绝对目录路径。
    pub path: String,
    /// Optional parent directory path.
    /// 可选父目录路径。
    pub parent_path: Option<String>,
    /// Whether the current directory may be selected as a project.
    /// 当前目录是否可以作为项目选择。
    pub can_select: bool,
    /// Child directory rows sorted for display.
    /// 已按显示顺序排序的子目录行。
    pub entries: Vec<ProjectDirectoryEntrySummary>,
    /// Optional human-readable notice for partial listing failures.
    /// 局部读取失败时使用的可选人类可读提示。
    pub notice: Option<String>,
}

/// Describes one workspace file entry category returned to the WebUI file panel.
/// 描述返回给 WebUI 文件面板的单个工作区文件条目类别。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceFileEntryKind {
    /// Directory entry that can be expanded lazily.
    /// 可懒加载展开的目录条目。
    Directory,
    /// Text or source-code file that can be previewed as UTF-8 content.
    /// 可作为 UTF-8 内容预览的文本或源代码文件。
    Code,
    /// Image file that can be displayed through the local media route.
    /// 可通过本地媒体路由显示的图片文件。
    Image,
    /// Video file that can be displayed through the local media route.
    /// 可通过本地媒体路由显示的视频文件。
    Video,
    /// Other file type that is not previewed by the first workspace file panel.
    /// 首版工作区文件面板不预览的其他文件类型。
    Other,
}

/// Stores the active workspace file tree filtering options.
/// 保存当前工作区文件树过滤选项。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileFilterOptions {
    /// Whether platform-hidden files and folders are filtered out.
    /// 是否过滤平台隐藏文件与文件夹。
    pub ignore_hidden: bool,
    /// Whether dot-prefixed file and folder names are filtered out.
    /// 是否过滤以点开头的文件与文件夹名称。
    pub ignore_dot_names: bool,
    /// Whether `.ignore` files are applied while listing the tree.
    /// 列出文件树时是否应用 `.ignore` 文件。
    pub use_ignore_files: bool,
    /// Whether Git ignore sources are applied while listing the tree.
    /// 列出文件树时是否应用 Git ignore 来源。
    pub use_gitignore: bool,
    /// Whether files outside code, image, and video preview groups are filtered out.
    /// 是否过滤代码、图片和视频预览组之外的文件。
    #[serde(default)]
    pub filter_non_preview_files: bool,
    /// Whether image files may be previewed after selection.
    /// 选中后是否允许预览图片文件。
    pub allow_images: bool,
    /// Whether video files may be previewed after selection.
    /// 选中后是否允许预览视频文件。
    pub allow_videos: bool,
}

impl Default for WorkspaceFileFilterOptions {
    /// Builds the default safe workspace file tree filtering options.
    /// 构建默认安全的工作区文件树过滤选项。
    fn default() -> Self {
        Self {
            ignore_hidden: true,
            ignore_dot_names: true,
            use_ignore_files: true,
            use_gitignore: true,
            filter_non_preview_files: false,
            allow_images: false,
            allow_videos: false,
        }
    }
}

/// Summarizes one file or directory row returned to the WebUI workspace file panel.
/// 汇总一条返回给 WebUI 工作区文件面板的文件或目录行。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileEntrySummary {
    /// Display name of the file or directory.
    /// 文件或目录显示名称。
    pub name: String,
    /// Absolute path on the service host.
    /// 服务宿主上的绝对路径。
    pub path: String,
    /// Workspace-root-relative path using forward slashes.
    /// 使用正斜杠的工作区根相对路径。
    pub relative_path: String,
    /// Entry category used by the WebUI to pick icons and preview behavior.
    /// WebUI 用于选择图标与预览行为的条目类别。
    pub kind: WorkspaceFileEntryKind,
    /// Whether the row is a directory.
    /// 当前行是否为目录。
    pub is_directory: bool,
    /// Lowercase file extension when present.
    /// 存在时的小写文件扩展名。
    pub extension: Option<String>,
    /// Best-effort MIME type used by media previews.
    /// 媒体预览使用的尽力推断 MIME 类型。
    pub mime_type: Option<String>,
    /// File size in bytes when available.
    /// 可用时的文件字节大小。
    pub size_bytes: Option<u64>,
    /// Last modified Unix timestamp in milliseconds when available.
    /// 可用时的 Unix 毫秒级最后修改时间戳。
    pub modified_at: Option<String>,
    /// Whether the entry matched a platform-hidden or dot-name rule.
    /// 条目是否命中平台隐藏或点名称规则。
    pub hidden: bool,
    /// Whether a directory may contain children after filtering.
    /// 目录在过滤后是否可能包含子项。
    pub has_children: bool,
}

/// Describes one lazily loaded workspace file directory listing.
/// 描述一次懒加载的工作区文件目录列表。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileDirectoryListing {
    /// Absolute workspace root path used for the listing.
    /// 本次列表使用的工作区根绝对路径。
    pub workspace_path: String,
    /// Absolute current directory path.
    /// 当前目录绝对路径。
    pub path: String,
    /// Workspace-root-relative current directory path.
    /// 当前目录相对工作区根的路径。
    pub relative_path: String,
    /// Optional absolute parent directory path inside the same workspace.
    /// 同一工作区内的可选父目录绝对路径。
    pub parent_path: Option<String>,
    /// Child rows returned for the current directory.
    /// 当前目录返回的子行。
    pub entries: Vec<WorkspaceFileEntrySummary>,
    /// Filtering options applied by the backend.
    /// 后端实际应用的过滤选项。
    pub filters: WorkspaceFileFilterOptions,
    /// Whether the result was truncated by the backend listing limit.
    /// 结果是否因后端列表上限被截断。
    pub truncated: bool,
    /// Number of entries returned after filtering and truncation.
    /// 过滤与截断后返回的条目数量。
    pub entry_count: usize,
    /// Optional human-readable notice for partial or bounded results.
    /// 局部或受限结果的人类可读提示。
    pub notice: Option<String>,
}

/// Describes one workspace file preview response.
/// 描述单个工作区文件预览响应。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileReadResponse {
    /// Absolute workspace root path used for the read.
    /// 本次读取使用的工作区根绝对路径。
    pub workspace_path: String,
    /// Absolute file path on the service host.
    /// 服务宿主上的文件绝对路径。
    pub path: String,
    /// Workspace-root-relative file path using forward slashes.
    /// 使用正斜杠的工作区根相对文件路径。
    pub relative_path: String,
    /// File category used by the WebUI preview renderer.
    /// WebUI 预览渲染器使用的文件类别。
    pub kind: WorkspaceFileEntryKind,
    /// Best-effort MIME type for the file.
    /// 文件的尽力推断 MIME 类型。
    pub mime_type: Option<String>,
    /// File size in bytes.
    /// 文件字节大小。
    pub size_bytes: u64,
    /// UTF-8 code content for code files.
    /// 代码文件的 UTF-8 内容。
    pub content: Option<String>,
    /// Encoding label for textual content.
    /// 文本内容的编码标签。
    pub encoding: Option<String>,
    /// Number of lines in the returned content when textual content is present.
    /// 返回文本内容存在时的行数。
    pub line_count: Option<usize>,
    /// Whether textual content was truncated by the backend read limit.
    /// 文本内容是否因后端读取上限被截断。
    pub truncated: bool,
}
