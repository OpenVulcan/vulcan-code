//! VMM management workbench state shared by the dedicated memory panel.
//! 专用记忆面板共享的 VMM 管理工作台状态。

use std::fmt;
use vulcan_vmm::{
    GetProfileBundleResponse, GetTurnDetailsResponse, ProfileNodeEntry, ProfileTarget,
    ProjectEntry, SearchMemoryEventsResponse, UserEntry, WriteMemoriesResponse,
};

/// Main OpenCode-compatible management section shown in the VMM workbench.
/// VMM 工作台中展示的 OpenCode 兼容主管理分区。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum VmmManagementSection {
    /// User manager section.
    /// 用户管理分区。
    #[default]
    UserManager,
    /// Project manager section.
    /// 项目管理分区。
    ProjectManager,
    /// Profile center section.
    /// 画像中心分区。
    ProfileCenter,
    /// Profile bundle test section.
    /// 画像 Bundle 测试分区。
    ProfileBundleTest,
    /// AI tool debug section.
    /// AI 工具调试分区。
    ToolsDebug,
    /// Memory settings section.
    /// 记忆设置分区。
    MemorySettings,
    /// gRPC transport section.
    /// gRPC 传输分区。
    GrpcTransport,
}

impl VmmManagementSection {
    /// Returns every management section in the same order as the OpenCode TUI home menu.
    /// 按 OpenCode TUI 首页菜单顺序返回全部管理分区。
    pub(super) const fn all() -> &'static [Self] {
        &[
            Self::UserManager,
            Self::ProjectManager,
            Self::ProfileCenter,
            Self::ProfileBundleTest,
            Self::ToolsDebug,
            Self::MemorySettings,
            Self::GrpcTransport,
        ]
    }

    /// Returns the visible section title.
    /// 返回可见分区标题。
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::UserManager => "用户管理",
            Self::ProjectManager => "项目管理",
            Self::ProfileCenter => "画像中心",
            Self::ProfileBundleTest => "画像 Bundle 测试",
            Self::ToolsDebug => "工具调试",
            Self::MemorySettings => "记忆设置",
            Self::GrpcTransport => "gRPC 传输",
        }
    }

    /// Returns the stable OpenCode-style English section key.
    /// 返回稳定的 OpenCode 风格英文分区键。
    pub(super) const fn key(self) -> &'static str {
        match self {
            Self::UserManager => "USER MANAGER",
            Self::ProjectManager => "PROJECT MANAGER",
            Self::ProfileCenter => "PROFILE CENTER",
            Self::ProfileBundleTest => "PROFILE BUNDLE TEST",
            Self::ToolsDebug => "TOOLS DEBUG",
            Self::MemorySettings => "MEMORY SETTINGS",
            Self::GrpcTransport => "GRPC TRANSPORT",
        }
    }

    /// Returns the compact section description.
    /// 返回紧凑分区说明。
    pub(super) const fn description(self) -> &'static str {
        match self {
            Self::UserManager => "解析、创建预检和删除长期用户。",
            Self::ProjectManager => "解析、创建预检、删除和迁移项目路径。",
            Self::ProfileCenter => "查看 active profile nodes 并应用手工画像指令。",
            Self::ProfileBundleTest => "读取当前 user/project 的完整画像 Bundle。",
            Self::ToolsDebug => "模拟 AI 专属工具检索、详情展开和主动写入。",
            Self::MemorySettings => "配置记忆开关、TTL、画像刷新和注入策略。",
            Self::GrpcTransport => "配置全局 VMM 连接和传输参数。",
        }
    }
}

/// Editable target scope selected in the VMM profile workspace.
/// VMM 画像工作区中选中的可编辑目标作用域。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum VmmProfileTargetChoice {
    /// User profile target.
    /// 用户画像目标。
    #[default]
    User,
    /// Project profile target.
    /// 项目画像目标。
    Project,
    /// Team profile target.
    /// 团队画像目标。
    Team,
    /// Space profile target.
    /// 空间画像目标。
    Space,
}

impl VmmProfileTargetChoice {
    /// Returns every profile target choice in stable UI order.
    /// 按稳定界面顺序返回全部画像目标选项。
    pub(super) const fn all() -> &'static [Self] {
        &[Self::User, Self::Project, Self::Team, Self::Space]
    }

    /// Returns the user-facing label for this target.
    /// 返回该目标的用户可见标签。
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::User => "用户画像",
            Self::Project => "项目画像",
            Self::Team => "团队画像",
            Self::Space => "空间画像",
        }
    }

    /// Converts the UI choice into a VMM protocol target.
    /// 将界面选项转换为 VMM 协议目标。
    pub(super) const fn into_protocol(self) -> ProfileTarget {
        match self {
            Self::User => ProfileTarget::ProfileTargetUser,
            Self::Project => ProfileTarget::ProfileTargetProject,
            Self::Team => ProfileTarget::ProfileTargetTeam,
            Self::Space => ProfileTarget::ProfileTargetSpace,
        }
    }
}

/// Scope selected by the dedicated VMM management workbench.
/// 专用 VMM 管理工作台选中的配置作用域。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum VmmManagementScopeChoice {
    /// Use the global VMM configuration draft.
    /// 使用全局 VMM 配置草稿。
    #[default]
    Global,
    /// Use the currently selected project VMM configuration draft.
    /// 使用当前选中项目的 VMM 配置草稿。
    Project,
}

impl VmmManagementScopeChoice {
    /// Returns the user-facing label for this management scope.
    /// 返回该管理作用域的用户可见标签。
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Global => "全局配置",
            Self::Project => "项目配置",
        }
    }
}

/// Pick-list choice for applying VMM settings to one workspace project.
/// 将 VMM 设置应用到某个工作区项目时使用的下拉选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VmmProjectScopeChoice {
    /// Stable task-project identifier.
    /// 稳定任务项目标识。
    pub(super) project_id: String,
    /// Absolute workspace path for the project.
    /// 项目的绝对工作区路径。
    pub(super) workspace_path: String,
    /// User-facing label shown in the project picker.
    /// 项目选择器中展示的用户可见标签。
    pub(super) label: String,
}

impl fmt::Display for VmmProjectScopeChoice {
    /// Formats one project scope choice for pick-list display.
    /// 将一个项目作用域选项格式化为下拉展示文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Structured VMM management data shown by the main memory panel.
/// 主记忆面板展示的结构化 VMM 管理数据。
#[derive(Debug, Clone, Default)]
pub(super) struct VmmManagementWorkbenchState {
    /// Currently visible management section.
    /// 当前可见的管理分区。
    pub(super) section: VmmManagementSection,
    /// Configuration scope used by management actions.
    /// 管理动作使用的配置作用域。
    pub(super) scope: VmmManagementScopeChoice,
    /// Selected task-project identifier used when the scope is project.
    /// 作用域为项目时选中的任务项目标识。
    pub(super) selected_project_id: Option<String>,
    /// Filter text used by the OpenCode-style management launcher.
    /// OpenCode 风格管理启动器使用的筛选文本。
    pub(super) section_filter: String,
    /// Latest durable users returned by VMM.
    /// VMM 返回的最新长期用户列表。
    pub(super) users: Vec<UserEntry>,
    /// Filter text used by the user manager list.
    /// 用户管理列表使用的筛选文本。
    pub(super) user_filter: String,
    /// Latest durable projects returned by VMM.
    /// VMM 返回的最新长期项目列表。
    pub(super) projects: Vec<ProjectEntry>,
    /// Filter text used by the project manager list.
    /// 项目管理列表使用的筛选文本。
    pub(super) project_filter: String,
    /// Currently selected profile target.
    /// 当前选中的画像目标。
    pub(super) profile_target: VmmProfileTargetChoice,
    /// Filter text used by the profile node workspace.
    /// 画像节点工作区使用的筛选文本。
    pub(super) profile_filter: String,
    /// Latest active profile nodes returned by VMM.
    /// VMM 返回的最新 active 画像节点。
    pub(super) profile_nodes: Vec<ProfileNodeEntry>,
    /// Latest full profile bundle returned by VMM.
    /// VMM 返回的最新完整画像 bundle。
    pub(super) profile_bundle: Option<GetProfileBundleResponse>,
    /// Editable manual profile instruction.
    /// 可编辑手工画像指令。
    pub(super) profile_instruction: String,
    /// Editable user reference used by user-management actions.
    /// 用户管理动作使用的可编辑用户引用。
    pub(super) user_ref: String,
    /// Editable user deletion confirmation code.
    /// 删除用户时使用的可编辑确认码。
    pub(super) user_delete_confirmation_code: String,
    /// Editable project path used by project-management actions.
    /// 项目管理动作使用的可编辑项目路径。
    pub(super) project_path: String,
    /// Whether project deletion is explicitly confirmed.
    /// 是否已显式确认删除项目。
    pub(super) project_delete_confirmed: bool,
    /// Editable source project path used by migration.
    /// 迁移使用的可编辑源项目路径。
    pub(super) migrate_source_project_path: String,
    /// Editable target project path used by migration.
    /// 迁移使用的可编辑目标项目路径。
    pub(super) migrate_target_project_path: String,
    /// Whether project migration is explicitly confirmed.
    /// 是否已显式确认迁移项目。
    pub(super) migrate_confirmed: bool,
    /// Comma-separated memory-search query draft.
    /// 逗号分隔的记忆检索查询草稿。
    pub(super) tool_search_queries: String,
    /// Editable memory-search top-k value.
    /// 可编辑记忆检索 top-k 值。
    pub(super) tool_search_top_k: String,
    /// Comma-separated turn identifiers for detail inspection.
    /// 用于详情查看的逗号分隔 turn 标识。
    pub(super) tool_turn_ids: String,
    /// Editable direct-write memory abstract.
    /// 可编辑主动写入记忆摘要。
    pub(super) tool_write_abstract: String,
    /// Editable direct-write memory details.
    /// 可编辑主动写入记忆详情。
    pub(super) tool_write_details: String,
    /// Editable direct-write scope level.
    /// 可编辑主动写入作用域级别。
    pub(super) tool_write_scope_level: String,
    /// Editable direct-write category.
    /// 可编辑主动写入分类。
    pub(super) tool_write_category: String,
    /// Editable direct-write priority.
    /// 可编辑主动写入优先级。
    pub(super) tool_write_priority: String,
    /// Editable direct-write memory level.
    /// 可编辑主动写入记忆层级。
    pub(super) tool_write_memory_level: String,
    /// Latest memory-search response shown by the tools-debug panel.
    /// 工具调试面板展示的最新记忆检索响应。
    pub(super) memory_search: Option<SearchMemoryEventsResponse>,
    /// Latest turn-detail response shown by the tools-debug panel.
    /// 工具调试面板展示的最新 turn 详情响应。
    pub(super) turn_details: Option<GetTurnDetailsResponse>,
    /// Latest direct-write response shown by the tools-debug panel.
    /// 工具调试面板展示的最新主动写入响应。
    pub(super) write_memories: Option<WriteMemoriesResponse>,
    /// Latest tools-debug request payload preview.
    /// 工具调试面板展示的最新请求载荷预览。
    pub(super) tool_request_preview: String,
    /// Latest tools-debug response payload preview.
    /// 工具调试面板展示的最新响应载荷预览。
    pub(super) tool_response_preview: String,
    /// Whether a VMM management action is currently running.
    /// 当前是否有 VMM 管理动作正在执行。
    pub(super) in_flight: bool,
    /// Local management notice.
    /// 局部管理提示。
    pub(super) notice: String,
}

impl VmmManagementWorkbenchState {
    /// Builds the default VMM management workbench state.
    /// 构建默认 VMM 管理工作台状态。
    pub(super) fn new() -> Self {
        Self {
            notice: String::from("选择一个管理动作以读取 VMM 状态。"),
            tool_search_top_k: String::from("5"),
            tool_write_scope_level: String::from("2"),
            tool_write_category: String::from("1"),
            tool_write_priority: String::from("2"),
            tool_write_memory_level: String::from("2"),
            ..Self::default()
        }
    }
}
