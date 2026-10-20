//! Memory mode settings and Markdown memory page requests and views.
//! 记忆模式设置与 Markdown 记忆页面的请求和视图。

use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use vulcan_markdown_memory::{
    ConflictChoice, ConflictResolution, ExportedPackage, ImportPreview, ImportReport,
    InjectionBudget, MemoryMode, NewerSide, NodeConflict, NodeSide, NodeSummary, PackageProject,
    PackageProjectPreview, ProjectCandidate, ProjectMapping, ProjectTarget, ScopeMergePreview,
};

/// Saved memory settings plus the limits the editor needs.
/// 已保存的记忆设置及编辑器所需的上限。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettingsView {
    /// Global memory mode.
    /// 全局记忆模式。
    pub mode: MemoryMode,
    /// Injection budget.
    /// 注入预算。
    pub budget: InjectionBudget,
    /// Default budget restored by the editor's reset action.
    /// 编辑器重置操作恢复的默认预算。
    pub default_budget: InjectionBudget,
    /// Largest accepted value of one budget part.
    /// 单个预算分项可接受的最大值。
    pub max_part_tokens: usize,
    /// Usage percentage of a budget part from which pages and write receipts warn.
    /// 预算分项达到该用量百分比时，页面与写入回执发出提醒。
    pub warning_percent: usize,
}

/// Saves the global memory settings.
/// 保存全局记忆设置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySettingsSaveRequest {
    /// Global memory mode.
    /// 全局记忆模式。
    pub mode: MemoryMode,
    /// Injection budget.
    /// 注入预算。
    pub budget: InjectionBudget,
}

/// Selects the project whose memory the page shows.
/// 选择页面显示其记忆的项目。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownMemoryScopeRequest {
    /// Host project of the page; absent outside a project.
    /// 页面所属宿主项目；不在项目中时缺省。
    #[serde(default)]
    pub project_id: Option<String>,
    /// Explicit memory project, for example an orphan chosen in the project tab.
    /// 显式记忆项目，例如在项目页签中选择的孤立项目。
    #[serde(default)]
    pub memory_project_id: Option<String>,
}

/// One memory node.
/// 一个记忆节点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownMemoryNodeView {
    /// Stable or path-derived id.
    /// 稳定标识或路径派生标识。
    pub id: String,
    /// Heading level, 1 to 4.
    /// 标题级别，1 至 4。
    pub level: u8,
    /// Title path from the section down to the node.
    /// 从分区到本节点的标题路径。
    pub path: Vec<String>,
    /// Section key of the top-level ancestor.
    /// 顶层祖先的分区键。
    pub section: Option<String>,
    /// Revision required to change the node.
    /// 修改节点所需的修订。
    pub revision: String,
    /// Own Markdown body.
    /// 自身 Markdown 正文。
    pub body: String,
    /// Creation time, `YYYY-MM-DDTHH:MMZ`.
    /// 创建时间，`YYYY-MM-DDTHH:MMZ`。
    pub created: Option<String>,
    /// Last update time, `YYYY-MM-DDTHH:MMZ`.
    /// 最近更新时间，`YYYY-MM-DDTHH:MMZ`。
    pub updated: Option<String>,
    /// Conversation that last wrote the node.
    /// 最近写入该节点的会话。
    pub session: Option<String>,
    /// Last writer: `ai` or `user`; absent for nodes never written through Vulcan.
    /// 最近写入方：`ai` 或 `user`；从未经 Vulcan 写入的节点缺省。
    pub source: Option<String>,
    /// Number of nodes below.
    /// 其下节点数。
    pub descendants: usize,
}

/// Usage of one injected part.
/// 单个注入部分的占用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownMemoryUsageView {
    /// Part key: `user`, `spaceBase`, `projectBase`, `spaceCatalog` or `projectCatalog`.
    /// 分项键：`user`、`spaceBase`、`projectBase`、`spaceCatalog` 或 `projectCatalog`。
    pub part: String,
    /// Estimated tokens of the full part.
    /// 完整分项的估算 Token。
    pub tokens: usize,
    /// Budget of the part.
    /// 分项预算。
    pub budget: usize,
    /// Titles left out of the injection.
    /// 被排除在注入之外的标题。
    pub omitted: Vec<String>,
}

/// One memory project.
/// 一个记忆项目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownMemoryProjectView {
    /// Memory project id.
    /// 记忆项目标识。
    pub id: String,
    /// Display name.
    /// 显示名。
    pub name: String,
    /// Bound host project ids.
    /// 已绑定的宿主项目标识。
    pub host_project_ids: Vec<String>,
    /// Root paths seen for the project.
    /// 该项目出现过的根路径。
    pub paths: Vec<String>,
    /// Whether memory is on for the project.
    /// 该项目是否开启记忆。
    pub memory_enabled: bool,
    /// Whether no recorded path exists any more.
    /// 是否已不存在任何记录路径。
    pub orphaned: bool,
    /// Last update time.
    /// 最近更新时间。
    pub updated: String,
}

/// A preset section created with a new scope file.
/// 新建层文件时创建的预置分区。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownMemorySectionView {
    /// Scope key: `user`, `space` or `project`.
    /// 层键：`user`、`space` 或 `project`。
    pub scope: String,
    /// Stable section key.
    /// 稳定分区键。
    pub key: String,
    /// Default title written into the file.
    /// 写入文件的默认标题。
    pub title: String,
}

/// An orphaned project suggested for the current host project.
/// 为当前宿主项目建议的孤立项目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownMemoryOrphanView {
    /// Memory project id.
    /// 记忆项目标识。
    pub memory_project_id: String,
    /// Stable reason keys: `same_directory_name`, `same_first_commit`.
    /// 稳定原因键：`same_directory_name`、`same_first_commit`。
    pub reasons: Vec<String>,
}

/// Everything the Markdown memory page shows.
/// Markdown 记忆页面显示的全部内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownMemoryOverview {
    /// Saved settings.
    /// 已保存的设置。
    pub settings: MemorySettingsView,
    /// User scope nodes.
    /// 用户层节点。
    pub user: Vec<MarkdownMemoryNodeView>,
    /// Space scope nodes.
    /// 空间层节点。
    pub space: Vec<MarkdownMemoryNodeView>,
    /// Project scope nodes of the shown memory project.
    /// 所显示记忆项目的项目层节点。
    pub project: Vec<MarkdownMemoryNodeView>,
    /// Memory project shown in the project tab.
    /// 项目页签显示的记忆项目。
    pub memory_project_id: Option<String>,
    /// Memory project bound to the host project.
    /// 绑定到宿主项目的记忆项目。
    pub bound_memory_project_id: Option<String>,
    /// Host project of the page.
    /// 页面所属的宿主项目。
    pub host_project_id: Option<String>,
    /// Orphans suggested for linking; empty once bound or dismissed.
    /// 建议关联的孤立项目；已绑定或已忽略时为空。
    pub orphan_candidates: Vec<MarkdownMemoryOrphanView>,
    /// Every memory project.
    /// 全部记忆项目。
    pub projects: Vec<MarkdownMemoryProjectView>,
    /// Injection usage for the shown scopes.
    /// 所显示各层的注入占用。
    pub usage: Vec<MarkdownMemoryUsageView>,
    /// Preset sections of every scope, offered when a node is added.
    /// 各层的预置分区，供新增节点时选择。
    pub preset_sections: Vec<MarkdownMemorySectionView>,
}

/// Applies node operations written by the user in the memory page.
/// 应用用户在记忆页中写入的节点操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownMemoryWriteRequest {
    /// Host project of the page; absent outside a project.
    /// 页面所属宿主项目；不在项目中时缺省。
    #[serde(default)]
    pub project_id: Option<String>,
    /// Explicit memory project chosen in the project tab.
    /// 在项目页签中显式选择的记忆项目。
    #[serde(default)]
    pub memory_project_id: Option<String>,
    /// Operations in the same JSON shape as the `memory_write` tool.
    /// 与 `memory_write` 工具相同 JSON 形状的操作。
    pub operations: Vec<Value>,
}

/// Selects one memory file.
/// 选择一个记忆文件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownMemoryFileRequest {
    /// Scope key: `user`, `space` or `project`.
    /// 层键：`user`、`space` 或 `project`。
    pub scope: String,
    /// Memory project of a project file.
    /// 项目文件所属的记忆项目。
    #[serde(default)]
    pub memory_project_id: Option<String>,
}

/// Exact text of one memory file.
/// 单个记忆文件的精确文本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownMemoryFileView {
    /// File path.
    /// 文件路径。
    pub path: String,
    /// Exact text; empty when the file does not exist yet.
    /// 精确文本；文件尚不存在时为空。
    pub text: String,
}

/// Replaces one whole memory file if it still holds the text the editor started from.
/// 若记忆文件仍是编辑器起始文本则整体替换。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownMemoryFileReplaceRequest {
    /// Scope key: `user`, `space` or `project`.
    /// 层键：`user`、`space` 或 `project`。
    pub scope: String,
    /// Memory project of a project file.
    /// 项目文件所属的记忆项目。
    #[serde(default)]
    pub memory_project_id: Option<String>,
    /// Text the editor loaded.
    /// 编辑器加载时的文本。
    pub expected: String,
    /// New text.
    /// 新文本。
    pub text: String,
}

/// One change to the memory project registry.
/// 对记忆项目注册表的一项变更。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MarkdownMemoryProjectAction {
    /// Binds the host project to an existing memory project.
    /// 将宿主项目绑定到既有记忆项目。
    Link {
        /// Host project.
        /// 宿主项目。
        project_id: String,
        /// Memory project to bind.
        /// 要绑定的记忆项目。
        memory_project_id: String,
    },
    /// Creates and binds a new memory project for the host project.
    /// 为宿主项目新建并绑定记忆项目。
    Create {
        /// Host project.
        /// 宿主项目。
        project_id: String,
    },
    /// Dismisses the orphan suggestion for the host project.
    /// 忽略宿主项目的孤立项目建议。
    Ignore {
        /// Host project.
        /// 宿主项目。
        project_id: String,
    },
    /// Turns memory on or off for a memory project.
    /// 为记忆项目开启或关闭记忆。
    SetEnabled {
        /// Memory project.
        /// 记忆项目。
        memory_project_id: String,
        /// New state.
        /// 新状态。
        enabled: bool,
    },
    /// Renames a memory project.
    /// 重命名记忆项目。
    Rename {
        /// Memory project.
        /// 记忆项目。
        memory_project_id: String,
        /// New display name.
        /// 新显示名。
        name: String,
    },
}

/// Writes an export package of the chosen scopes into an existing directory.
/// 将所选层的导出包写入既有目录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownMemoryExportRequest {
    /// Existing native destination directory.
    /// 已有原生目标目录。
    pub destination_directory: String,
    /// Whether the user scope is exported.
    /// 是否导出用户层。
    pub user: bool,
    /// Whether the space scope is exported.
    /// 是否导出空间层。
    pub space: bool,
    /// Memory projects whose project scope is exported.
    /// 导出其项目层的记忆项目。
    #[serde(default)]
    pub memory_project_ids: Vec<String>,
}

/// Previews importing a package; projects without a mapping are only listed.
/// 预览导入导出包；未指定去向的项目只列出。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownMemoryImportPreviewRequest {
    /// Exact native package path.
    /// 精确原生导出包路径。
    pub archive_path: String,
    /// Chosen targets of packaged projects so far.
    /// 目前已为包内项目选择的去向。
    #[serde(default)]
    pub projects: Vec<ProjectMapping>,
}

/// Applies a previewed import with a target for every packaged project and a resolution for
/// every conflict.
/// 应用已预览的导入：每个包内项目都有去向，每个冲突都有解决方式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownMemoryImportRequest {
    /// Exact native package path.
    /// 精确原生导出包路径。
    pub archive_path: String,
    /// SHA-256 returned by the preview.
    /// 预览返回的 SHA-256。
    pub expected_sha256: String,
    /// Target of every packaged project.
    /// 每个包内项目的去向。
    pub projects: Vec<ProjectMapping>,
    /// Resolution of every conflict.
    /// 每个冲突的解决方式。
    #[serde(default)]
    pub resolutions: Vec<ConflictResolution>,
}
